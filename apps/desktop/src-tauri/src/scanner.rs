use crate::fs_ops::{home_dir, same_skill_content, short_hash, LINK_MARKER_FILE, SOURCE_MARKER_FILE};
use crate::series::{self, SeriesLinks};
use crate::targets::{self, is_managed_target_copy};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub source_path: String,
    /// What kind of thing the skill belongs to (a repository collection, a
    /// Codex plugin, Codex's own system skills, or nothing: standalone).
    pub origin: SkillOrigin,
    pub health: SkillHealth,
    pub targets: Vec<TargetState>,
    pub support_files: Vec<String>,
    /// True for a copy that Skills Manage placed in a tool folder (it carries a
    /// link marker). Such copies mirror a library skill and must not be
    /// re-imported into the library.
    pub managed: bool,
    /// Where a shared-library skill came from, when known (its GitHub URL).
    pub source_url: Option<String>,
    /// How a library skill got there: "github", "local" or "skillpack".
    pub origin_kind: Option<String>,
    /// The original folder (local) or package file name (skillpack).
    pub origin_detail: Option<String>,
    /// For skills not added from GitHub: a GitHub repository named in SKILL.md,
    /// or the folder of a repository the user linked (see `series`). Lets the
    /// UI group a series and offer "update to the latest version".
    pub detected_source_url: Option<String>,
    /// `detected_source_url` comes from a linked repository (can be unlinked).
    pub linked_source: bool,
    /// Specific problems with SKILL.md: "unreadable", "missing-name",
    /// "missing-description".
    pub issues: Vec<String>,
    /// Library skills: the tools it may not suit, so enabling there can warn.
    pub compat: Vec<crate::compat::CompatSummary>,
    /// Library skills: tools that get their own version of it (see `variants`).
    pub variants: Vec<crate::variants::VariantInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetState {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// The tool has a same-named folder whose content differs from the
    /// library skill (not managed by Skills Manage; never touched silently).
    pub conflict: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillHealth {
    Healthy,
    Warning,
    Broken,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillOrigin {
    pub kind: SkillOriginKind,
    pub plugin: Option<PluginIdentity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<RepositoryIdentity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillOriginKind {
    Standalone,
    /// One of several skills from the same repository.
    Collection,
    /// Ships inside a Codex plugin; Codex manages it.
    Plugin,
    /// Codex's own skills (bundled plugins, `~/.codex/skills/.system`).
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryIdentity {
    pub id: String,
    pub name: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginIdentity {
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub repository: Option<String>,
}

impl SkillOrigin {
    fn standalone() -> Self {
        SkillOrigin { kind: SkillOriginKind::Standalone, plugin: None, repository: None }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct PluginManifest {
    name: String,
    #[serde(default)]
    version: String,
    author: Option<PluginAuthor>,
    interface: Option<PluginInterface>,
    repository: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct PluginAuthor {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginInterface {
    display_name: Option<String>,
}

#[derive(Debug, Clone)]
struct PluginPackage {
    channel: String,
    package_root: PathBuf,
    manifest: PluginManifest,
}

#[derive(Debug, Clone, Deserialize)]
struct MarketplaceManifest {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubSourceMetadata {
    kind: String,
    owner: String,
    repo: String,
}

/// `~/.agents/.skill-lock.json`, written by `npx skills add`.
#[derive(Debug, Clone, Deserialize)]
struct SkillLock {
    #[serde(default)]
    skills: BTreeMap<String, LockedSkillSource>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LockedSkillSource {
    source: String,
    source_type: String,
    source_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SkillMetadata {
    name: Option<String>,
    description: Option<String>,
}

pub fn scan_default_skills() -> Vec<ScannedSkill> {
    let roots = candidate_skill_roots();
    let target_roots = default_target_roots();
    let mut skills = scan_skill_roots_with_links(&roots, &target_roots, &series::load_default_links());

    if let Some(home) = home_dir() {
        apply_skill_lock_sources(&mut skills, &home.join(".agents").join(".skill-lock.json"));
        let codex_root = home.join(".codex");
        skills.extend(scan_repository_collections(&codex_root, &target_roots));
        skills.extend(scan_plugin_cache(&codex_root.join("plugins").join("cache"), &target_roots));
    }

    sort_skills(&mut skills);
    skills
}

pub fn candidate_skill_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(home) = home_dir() {
        roots.push(home.join(".agents").join("skills"));
        for (_, _, folder) in targets::TOOLS {
            roots.push(home.join(folder).join("skills"));
        }
        // Codex's own skills; a hidden folder, so not found by the root above.
        roots.push(home.join(".codex").join("skills").join(".system"));
    }
    if let Ok(data_root) = crate::fs_ops::default_data_root() {
        roots.push(data_root.join("library"));
    }

    roots
}

#[cfg(test)]
pub fn scan_skill_roots(roots: &[PathBuf]) -> Vec<ScannedSkill> {
    let target_roots = default_target_roots();
    scan_skill_roots_with_target_roots(roots, &target_roots)
}

pub fn scan_skill_roots_with_target_roots(
    roots: &[PathBuf],
    target_roots: &[(&str, PathBuf)],
) -> Vec<ScannedSkill> {
    scan_skill_roots_with_links(roots, target_roots, &SeriesLinks::new())
}

fn scan_skill_roots_with_links(
    roots: &[PathBuf],
    target_roots: &[(&str, PathBuf)],
    links: &SeriesLinks,
) -> Vec<ScannedSkill> {
    let mut skills = Vec::new();

    for root in roots {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };

        for entry in entries.flatten() {
            // Hidden folders include our own staging dirs, which must never be
            // listed as skills even if a crash left one behind.
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }

            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let skill_file = path.join("SKILL.md");
            if !skill_file.is_file() {
                // Hermes groups skills in category folders: look one level in.
                if is_categorized_root(root) {
                    for child in fs::read_dir(&path).into_iter().flatten().flatten() {
                        let child_path = child.path();
                        let child_file = child_path.join("SKILL.md");
                        let hidden = child.file_name().to_string_lossy().starts_with('.');
                        if !hidden && child_file.is_file() {
                            if let Some(skill) = scan_skill_dir(root, &child_path, &child_file, target_roots, links) {
                                skills.push(skill);
                            }
                        }
                    }
                }
                continue;
            }

            if let Some(skill) = scan_skill_dir(root, &path, &skill_file, target_roots, links) {
                skills.push(skill);
            }
        }
    }

    sort_skills(&mut skills);
    skills
}

fn scan_skill_dir(
    root: &Path,
    skill_dir: &Path,
    skill_file: &Path,
    target_roots: &[(&str, PathBuf)],
    links: &SeriesLinks,
) -> Option<ScannedSkill> {
    let source = source_label(root);
    let skill_origin = skill_origin_for_path(root, skill_dir);
    scan_skill_dir_as(skill_dir, skill_file, &source, skill_origin, target_roots, links)
}

/// Scan one skill folder shown under `source` (a tool's or the library's name).
fn scan_skill_dir_as(
    skill_dir: &Path,
    skill_file: &Path,
    source: &str,
    skill_origin: SkillOrigin,
    target_roots: &[(&str, PathBuf)],
    links: &SeriesLinks,
) -> Option<ScannedSkill> {
    let directory_name = skill_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown-skill")
        .to_string();
    let origin = read_origin(skill_dir);
    let Ok(contents) = fs::read_to_string(skill_file) else {
        return Some(ScannedSkill {
            id: skill_id(&directory_name, skill_dir),
            name: directory_name,
            description: "SKILL.md could not be read.".to_string(),
            source: source.to_string(),
            source_path: skill_dir.display().to_string(),
            origin: skill_origin,
            health: SkillHealth::Broken,
            targets: target_states(source, skill_dir, target_roots),
            support_files: support_files(skill_dir),
            managed: is_managed_copy(skill_dir),
            source_url: origin.url,
            origin_kind: origin.kind,
            origin_detail: origin.detail,
            detected_source_url: None,
            linked_source: false,
            issues: vec!["unreadable".to_string()],
            compat: Vec::new(),
            variants: Vec::new(),
        });
    };
    let metadata = parse_skill_metadata(&contents);
    let health = if metadata.name.is_some() && metadata.description.is_some() {
        SkillHealth::Healthy
    } else {
        SkillHealth::Warning
    };
    let mut issues = Vec::new();
    if metadata.name.is_none() {
        issues.push("missing-name".to_string());
    }
    if metadata.description.is_none() {
        issues.push("missing-description".to_string());
    }
    let name = metadata.name.unwrap_or_else(|| directory_name.clone());
    // Skills not added from GitHub may still name their repository in
    // SKILL.md, or belong to a repository the user linked.
    let (detected_source_url, linked_source) = if origin.kind.as_deref() == Some("github") {
        (None, false)
    } else if let Some(url) = crate::github::detect_github_url(skill_dir) {
        (Some(url), false)
    } else {
        let linked = series::linked_url(links, &directory_name);
        let is_linked = linked.is_some();
        (linked, is_linked)
    };

    let (compat, variants) = if source == "Shared Library" {
        (crate::compat::library_skill_summaries(skill_dir), crate::variants::list(skill_dir))
    } else {
        (Vec::new(), Vec::new())
    };

    Some(ScannedSkill {
        id: skill_id(&name, skill_dir),
        name,
        description: metadata
            .description
            .unwrap_or_else(|| "No description provided.".to_string()),
        source: source.to_string(),
        source_path: skill_dir.display().to_string(),
        origin: skill_origin,
        health,
        targets: target_states(source, skill_dir, target_roots),
        support_files: support_files(skill_dir),
        managed: is_managed_copy(skill_dir),
        source_url: origin.url,
        origin_kind: origin.kind,
        origin_detail: origin.detail,
        detected_source_url,
        linked_source,
        issues,
        compat,
        variants,
    })
}

/// Skills installed with `npx skills add` from GitHub belong to that
/// repository. Tool-folder skills with no known source also get it as their
/// source, so they group with the rest of the repository.
fn apply_skill_lock_sources(skills: &mut [ScannedSkill], lock_path: &Path) {
    let Ok(contents) = fs::read_to_string(lock_path) else {
        return;
    };
    let Ok(lock) = serde_json::from_str::<SkillLock>(&contents) else {
        return;
    };

    for skill in skills {
        if skill.origin.kind != SkillOriginKind::Standalone {
            continue;
        }
        let folder_name = Path::new(&skill.source_path).file_name().and_then(|value| value.to_str());
        let locked_source = folder_name
            .and_then(|name| lock.skills.get(name))
            .or_else(|| lock.skills.get(&skill.name));
        let Some(repository) = locked_source.and_then(locked_repository_identity) else {
            continue;
        };

        if skill.source != "Shared Library" && skill.source_url.is_none() && skill.detected_source_url.is_none() {
            skill.detected_source_url = repository.url.clone();
        }
        skill.origin = SkillOrigin { kind: SkillOriginKind::Collection, plugin: None, repository: Some(repository) };
    }
}

fn locked_repository_identity(source: &LockedSkillSource) -> Option<RepositoryIdentity> {
    if !source.source_type.eq_ignore_ascii_case("github") {
        return None;
    }

    let source_name = source.source.trim().trim_matches('/');
    let mut parts = source_name.rsplit('/');
    let repo = parts.next()?.trim_end_matches(".git");
    let owner = parts.next()?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }

    let source_url = source.source_url.trim().trim_end_matches('/').trim_end_matches(".git");

    Some(RepositoryIdentity {
        id: format!("github/{}/{}", slug_id(owner), slug_id(repo)),
        name: format!("{owner}/{repo}"),
        url: (!source_url.is_empty()).then(|| source_url.to_string()),
    })
}

/// Skill packages Codex keeps in `~/.codex/<package>` with a Claude plugin
/// marketplace manifest: each is a local repository collection.
fn scan_repository_collections(codex_root: &Path, target_roots: &[(&str, PathBuf)]) -> Vec<ScannedSkill> {
    let mut skills = Vec::new();
    let Ok(entries) = fs::read_dir(codex_root) else {
        return skills;
    };

    for entry in entries.flatten() {
        let package_root = entry.path();
        let manifest_path = package_root.join(".claude-plugin").join("marketplace.json");
        let skills_root = package_root.join("skills");
        if !manifest_path.is_file() || !skills_root.is_dir() {
            continue;
        }
        let Ok(contents) = fs::read_to_string(manifest_path) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<MarketplaceManifest>(&contents) else {
            continue;
        };
        if manifest.name.trim().is_empty() {
            continue;
        }

        let origin = SkillOrigin {
            kind: SkillOriginKind::Collection,
            plugin: None,
            repository: Some(RepositoryIdentity {
                id: format!("local/{}", slug_id(&manifest.name)),
                name: manifest.name,
                url: None,
            }),
        };
        skills.extend(scan_skills_folder(&skills_root, "Codex", &origin, target_roots));
    }

    sort_skills(&mut skills);
    skills
}

/// Skills inside the Codex plugins in `~/.codex/plugins/cache/<channel>/<plugin>/<version>`,
/// the latest version of each plugin only.
fn scan_plugin_cache(cache_root: &Path, target_roots: &[(&str, PathBuf)]) -> Vec<ScannedSkill> {
    let mut skills = Vec::new();

    for package in latest_plugin_packages(cache_root) {
        let skills_root = package.package_root.join("skills");
        if !skills_root.is_dir() {
            continue;
        }

        let publisher = package
            .manifest
            .author
            .as_ref()
            .map(|author| author.name.trim())
            .filter(|name| !name.is_empty())
            .unwrap_or("Unknown publisher")
            .to_string();
        let plugin_name = package
            .manifest
            .interface
            .as_ref()
            .and_then(|interface| interface.display_name.as_deref())
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(package.manifest.name.as_str())
            .to_string();
        let version = if package.manifest.version.trim().is_empty() {
            package
                .package_root
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("unknown")
                .to_string()
        } else {
            package.manifest.version.clone()
        };
        let origin = SkillOrigin {
            kind: plugin_origin_kind(&package.channel),
            plugin: Some(PluginIdentity {
                id: format!("{}/{}", slug_id(&publisher), slug_id(&package.manifest.name)),
                name: plugin_name,
                version,
                publisher,
                repository: package
                    .manifest
                    .repository
                    .as_deref()
                    .map(str::trim)
                    .filter(|repository| !repository.is_empty())
                    .map(str::to_string),
            }),
            repository: None,
        };
        skills.extend(scan_skills_folder(&skills_root, "Codex", &origin, target_roots));
    }

    sort_skills(&mut skills);
    skills
}

/// Every skill folder directly inside `skills_root`, all with the same origin.
fn scan_skills_folder(
    skills_root: &Path,
    source: &str,
    origin: &SkillOrigin,
    target_roots: &[(&str, PathBuf)],
) -> Vec<ScannedSkill> {
    let Ok(entries) = fs::read_dir(skills_root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|entry| {
            let skill_dir = entry.path();
            let skill_file = skill_dir.join("SKILL.md");
            if !skill_dir.is_dir() || !skill_file.is_file() {
                return None;
            }
            scan_skill_dir_as(&skill_dir, &skill_file, source, origin.clone(), target_roots, &SeriesLinks::new())
        })
        .collect()
}

fn latest_plugin_packages(cache_root: &Path) -> Vec<PluginPackage> {
    let mut packages = BTreeMap::<String, PluginPackage>::new();
    let Ok(channels) = fs::read_dir(cache_root) else {
        return Vec::new();
    };

    for channel_entry in channels.flatten() {
        let channel_root = channel_entry.path();
        if !channel_root.is_dir() {
            continue;
        }
        let channel = channel_entry.file_name().to_string_lossy().to_string();
        let Ok(plugin_dirs) = fs::read_dir(&channel_root) else {
            continue;
        };

        for plugin_entry in plugin_dirs.flatten() {
            let Ok(version_dirs) = fs::read_dir(plugin_entry.path()) else {
                continue;
            };

            for version_entry in version_dirs.flatten() {
                let package_root = version_entry.path();
                if !package_root.is_dir() || version_entry.file_name().to_string_lossy().eq_ignore_ascii_case("latest") {
                    continue;
                }
                let Ok(contents) = fs::read_to_string(package_root.join(".codex-plugin").join("plugin.json")) else {
                    continue;
                };
                let Ok(manifest) = serde_json::from_str::<PluginManifest>(&contents) else {
                    continue;
                };
                if manifest.name.trim().is_empty() {
                    continue;
                }

                let publisher = manifest
                    .author
                    .as_ref()
                    .map(|author| author.name.trim().to_lowercase())
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| channel.to_lowercase());
                let key = format!("{publisher}/{}", manifest.name.trim().to_lowercase());
                let candidate = PluginPackage { channel: channel.clone(), package_root, manifest };
                let newer = packages.get(&key).is_none_or(|existing| {
                    match natural_version_cmp(&candidate.manifest.version, &existing.manifest.version) {
                        Ordering::Greater => true,
                        Ordering::Equal => candidate.package_root > existing.package_root,
                        Ordering::Less => false,
                    }
                });
                if newer {
                    packages.insert(key, candidate);
                }
            }
        }
    }

    packages.into_values().collect()
}

fn natural_version_cmp(left: &str, right: &str) -> Ordering {
    numeric_version_parts(left)
        .cmp(&numeric_version_parts(right))
        .then_with(|| left.cmp(right))
}

fn numeric_version_parts(value: &str) -> Vec<u64> {
    value
        .split(|character: char| !character.is_ascii_digit())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

/// Plugins Codex ships itself are part of Codex, not something the user added.
fn plugin_origin_kind(channel: &str) -> SkillOriginKind {
    match channel.to_lowercase().as_str() {
        "openai-bundled" | "openai-primary-runtime" => SkillOriginKind::System,
        _ => SkillOriginKind::Plugin,
    }
}

fn skill_origin_for_path(root: &Path, skill_dir: &Path) -> SkillOrigin {
    if let Some(repository) = repository_from_source_marker(skill_dir) {
        return SkillOrigin { kind: SkillOriginKind::Collection, plugin: None, repository: Some(repository) };
    }
    let normalized = root.to_string_lossy().replace('\\', "/").to_lowercase();
    if normalized.ends_with("/.codex/skills/.system") {
        return SkillOrigin { kind: SkillOriginKind::System, plugin: None, repository: None };
    }
    SkillOrigin::standalone()
}

fn repository_from_source_marker(skill_dir: &Path) -> Option<RepositoryIdentity> {
    let contents = fs::read_to_string(skill_dir.join(SOURCE_MARKER_FILE)).ok()?;
    let source = serde_json::from_str::<GithubSourceMetadata>(&contents).ok()?;
    if !source.kind.eq_ignore_ascii_case("github") || source.owner.trim().is_empty() || source.repo.trim().is_empty() {
        return None;
    }
    Some(RepositoryIdentity {
        id: format!("github/{}/{}", slug_id(&source.owner), slug_id(&source.repo)),
        name: format!("{}/{}", source.owner, source.repo),
        url: Some(format!("https://github.com/{}/{}", source.owner, source.repo)),
    })
}

/// Lower-case ASCII words joined by `-` (for repository and plugin ids).
fn slug_id(value: &str) -> String {
    value
        .chars()
        .map(|character| if character.is_ascii_alphanumeric() { character.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn sort_skills(skills: &mut [ScannedSkill]) {
    skills.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn is_managed_copy(skill_dir: &Path) -> bool {
    skill_dir.join(LINK_MARKER_FILE).is_file()
}

#[derive(Default)]
struct Origin {
    kind: Option<String>,
    url: Option<String>,
    detail: Option<String>,
}

fn read_origin(skill_dir: &Path) -> Origin {
    let Ok(contents) = fs::read_to_string(skill_dir.join(SOURCE_MARKER_FILE)) else {
        return Origin::default();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return Origin::default();
    };
    let field = |key: &str| {
        value
            .get(key)
            .and_then(|item| item.as_str())
            .filter(|item| !item.is_empty())
            .map(str::to_string)
    };
    let kind = field("kind");
    let detail = match kind.as_deref() {
        Some("local") => field("path"),
        Some("skillpack") => field("package"),
        _ => None,
    };
    Origin {
        url: if kind.as_deref() == Some("github") { field("url") } else { None },
        kind,
        detail,
    }
}

/// The SKILL.md of a skill the app lists, for the details page. Only a skill
/// folder directly inside a known skills folder (a tool's or the library) can
/// be read, so the UI can't be used to read arbitrary files.
pub fn read_skill_markdown(skill_path: &str) -> Result<String, String> {
    read_skill_markdown_in(skill_path, &candidate_skill_roots())
}

const MAX_SKILL_MD_BYTES: u64 = 1024 * 1024;

fn read_skill_markdown_in(skill_path: &str, roots: &[PathBuf]) -> Result<String, String> {
    let dir = Path::new(skill_path)
        .canonicalize()
        .map_err(|_| "The skill folder was not found.".to_string())?;
    let known = roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| dir.parent() == Some(root.as_path()));
    if !known {
        return Err("Only skills in a known skills folder can be shown.".to_string());
    }
    let file = dir.join("SKILL.md");
    let size = fs::metadata(&file)
        .map_err(|_| "This skill has no SKILL.md.".to_string())?
        .len();
    if size > MAX_SKILL_MD_BYTES {
        return Err("SKILL.md is too large to show.".to_string());
    }
    fs::read_to_string(&file).map_err(|error| format!("Could not read SKILL.md: {error}"))
}

/// Scan one folder the user picked: either a skill folder itself (it holds a
/// SKILL.md) or a folder whose child folders are skills.
pub fn scan_folder(folder: &Path) -> Vec<ScannedSkill> {
    let target_roots = default_target_roots();
    let skill_file = folder.join("SKILL.md");
    if skill_file.is_file() {
        let root = folder.parent().unwrap_or(folder);
        return scan_skill_dir(root, folder, &skill_file, &target_roots, &SeriesLinks::new())
            .into_iter()
            .collect();
    }
    scan_skill_roots_with_target_roots(&[folder.to_path_buf()], &target_roots)
}

fn parse_skill_metadata(contents: &str) -> SkillMetadata {
    let mut metadata = SkillMetadata {
        name: None,
        description: None,
    };

    // Files saved by Windows Notepad and similar editors start with a BOM.
    let contents = contents.strip_prefix('\u{feff}').unwrap_or(contents);
    let mut lines = contents.lines().peekable();
    if lines.next().map(str::trim_end) != Some("---") {
        return metadata;
    }

    while let Some(line) = lines.next() {
        if line.trim_end() == "---" {
            break;
        }

        // Only top-level keys; indented lines belong to a previous value.
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key.starts_with(char::is_whitespace) {
            continue;
        }

        let value = value.trim();
        let value = if is_block_scalar_indicator(value) {
            // `description: >` / `|` — the value is the following indented lines.
            let folded = value.starts_with('>');
            let mut parts = Vec::new();
            while let Some(next) = lines.peek() {
                if next.trim().is_empty() {
                    lines.next();
                    continue;
                }
                if !next.starts_with(char::is_whitespace) {
                    break;
                }
                parts.push(next.trim().to_string());
                lines.next();
            }
            parts.join(if folded { " " } else { "\n" })
        } else {
            clean_metadata_value(value)
        };
        let value = Some(value).filter(|value| !value.is_empty());

        match key.trim() {
            "name" => metadata.name = value,
            "description" => metadata.description = value,
            _ => {}
        }
    }

    metadata
}

/// YAML block scalar headers: `|` or `>`, optionally with chomping/indent hints.
fn is_block_scalar_indicator(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some('|') | Some('>'))
        && characters.all(|character| matches!(character, '-' | '+') || character.is_ascii_digit())
}

fn clean_metadata_value(value: &str) -> String {
    let value = value.trim();
    let quoted = value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')));
    let unquoted = if quoted {
        &value[1..value.len() - 1]
    } else {
        value
    };
    unquoted.trim().to_string()
}

fn support_files(skill_dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(skill_dir) else {
        return vec!["SKILL.md".to_string()];
    };

    let mut files: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                return None;
            }
            Some(name)
        })
        .collect();

    files.sort();
    files
}

/// A tool's skills folder whose skills sit in category folders (Hermes).
fn is_categorized_root(root: &Path) -> bool {
    let normalized = root.to_string_lossy().replace('\\', "/").to_lowercase();
    targets::TOOLS.iter().any(|(id, _, folder)| {
        targets::CATEGORIZED_TOOLS.contains(id) && normalized.ends_with(&format!("/{folder}/skills"))
    })
}

fn source_label(root: &Path) -> String {
    let normalized = root.to_string_lossy().replace('\\', "/").to_lowercase();

    if let Some((_, name, _)) = targets::TOOLS
        .iter()
        .find(|(_, _, folder)| normalized.contains(&format!("/{folder}/skills")))
    {
        name.to_string()
    } else if normalized.contains("/.agents/skills") {
        "Agent Skills".to_string()
    } else if normalized.ends_with("/library") || normalized.contains("/.skills-manage/library") {
        "Shared Library".to_string()
    } else {
        "Manual Path".to_string()
    }
}

fn target_states(
    source: &str,
    skill_dir: &Path,
    target_roots: &[(&str, PathBuf)],
) -> Vec<TargetState> {
    let skill_folder = skill_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown-skill");

    targets::TOOLS
        .iter()
        .map(|(target_id, target_name, _)| {
            let (enabled, conflict) =
                target_state(source, skill_dir, skill_folder, target_id, target_name, target_roots);
            TargetState {
                id: target_id.to_string(),
                name: target_name.to_string(),
                enabled,
                conflict,
            }
        })
        .collect()
}

/// (enabled, conflict) for one tool. A library skill counts as enabled when
/// the tool holds its managed copy or an identical copy of the same name.
fn target_state(
    source: &str,
    skill_dir: &Path,
    skill_folder: &str,
    target_id: &str,
    target_name: &str,
    target_roots: &[(&str, PathBuf)],
) -> (bool, bool) {
    if source == target_name {
        return (true, false);
    }

    if source != "Shared Library" {
        return (false, false);
    }

    let Some((_, target_root)) = target_roots.iter().find(|(candidate_id, _)| *candidate_id == target_id) else {
        return (false, false);
    };
    let copy = target_root.join(skill_folder);
    // No SKILL.md (e.g. only SKILL.md.disabled): the tool has it switched off.
    if !copy.is_dir() || !copy.join("SKILL.md").is_file() {
        return (false, false);
    }
    // A tool with its own version of the skill holds that version, not the library's.
    let expected = crate::variants::effective_source(skill_dir, target_id);
    if is_managed_target_copy(&copy, skill_dir, target_id) || same_skill_content(&expected, &copy) {
        return (true, false);
    }
    (false, true)
}

fn default_target_roots() -> Vec<(&'static str, PathBuf)> {
    targets::toggleable_target_ids()
        .iter()
        .filter_map(|id| targets::target_root_for(id).ok().map(|root| (*id, root)))
        .collect()
}

/// Readable slug (Unicode letters kept) plus a hash of the folder path, so two
/// skills that share a name — or have names with no ASCII at all — still get
/// distinct ids.
fn skill_id(name: &str, skill_dir: &Path) -> String {
    let slug = name
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character.to_lowercase().next().unwrap_or(character)
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let hash = short_hash(&skill_dir.display().to_string().to_lowercase());

    if slug.is_empty() {
        format!("skill-{hash}")
    } else {
        format!("{slug}-{hash}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs_ops::test_temp_dir as unique_temp_dir;
    use std::fs;

    fn create_plain_skill(root: &Path, skill_name: &str) {
        let skill_dir = root.join(skill_name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {skill_name}\ndescription: Collection skill\n---\n"),
        )
        .unwrap();
    }

    fn create_plugin_skill(
        cache_root: &Path,
        channel: &str,
        plugin_id: &str,
        version: &str,
        display_name: &str,
        skill_name: &str,
    ) {
        let package_root = cache_root.join(channel).join(plugin_id).join(version);
        let manifest_dir = package_root.join(".codex-plugin");
        let skill_dir = package_root.join("skills").join(skill_name);
        fs::create_dir_all(&manifest_dir).unwrap();
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            manifest_dir.join("plugin.json"),
            serde_json::json!({
                "name": plugin_id,
                "version": version,
                "author": { "name": "OpenAI" },
                "skills": "./skills/",
                "repository": "https://github.com/openai/plugins",
                "interface": { "displayName": display_name }
            })
            .to_string(),
        )
        .unwrap();
        fs::write(skill_dir.join("SKILL.md"), format!("---\nname: {skill_name}\ndescription: Plugin skill\n---\n")).unwrap();
    }

    #[test]
    fn plain_skills_are_standalone() {
        let root = unique_temp_dir("scan-standalone");
        create_plain_skill(&root, "agent-tool-safety");
        let skills = scan_skill_roots(&[root.clone()]);
        fs::remove_dir_all(root).unwrap();
        assert_eq!(skills[0].origin, SkillOrigin::standalone());
    }

    #[test]
    fn scan_skill_roots_marks_codex_system_skills() {
        let temp_root = unique_temp_dir("scan-system-skill");
        let root = temp_root.join(".codex").join("skills").join(".system");
        create_plain_skill(&root, "skill-installer");

        let skills = scan_skill_roots(&[root]);
        fs::remove_dir_all(temp_root).unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].source, "Codex");
        assert_eq!(skills[0].origin.kind, SkillOriginKind::System);
        assert_eq!(skills[0].origin.plugin, None);
    }

    #[test]
    fn scan_repository_collections_groups_marketplace_skills() {
        let codex_root = unique_temp_dir("scan-repository-collection");
        let package_root = codex_root.join("skills-main");
        let marketplace_dir = package_root.join(".claude-plugin");
        fs::create_dir_all(&marketplace_dir).unwrap();
        fs::write(
            marketplace_dir.join("marketplace.json"),
            serde_json::json!({ "name": "anthropic-agent-skills" }).to_string(),
        )
        .unwrap();
        create_plain_skill(&package_root.join("skills"), "alpha-skill");
        create_plain_skill(&package_root.join("skills"), "beta-skill");

        let skills = scan_repository_collections(&codex_root, &[]);
        fs::remove_dir_all(codex_root).unwrap();

        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].origin.kind, SkillOriginKind::Collection);
        assert_eq!(skills[0].source, "Codex");
        let codex = skills[0].targets.iter().find(|target| target.id == "codex").unwrap();
        assert!(codex.enabled);
        let repository = skills[0].origin.repository.as_ref().unwrap();
        assert_eq!(repository.id, "local/anthropic-agent-skills");
        assert_eq!(repository.name, "anthropic-agent-skills");
        assert_eq!(repository.url, None);
    }

    #[test]
    fn scan_skill_roots_reads_github_repository_marker() {
        let temp_root = unique_temp_dir("scan-github-source");
        let root = temp_root.join(".skills-manage").join("library");
        create_plain_skill(&root, "pdf");
        fs::write(
            root.join("pdf").join(SOURCE_MARKER_FILE),
            serde_json::json!({ "kind": "github", "owner": "anthropics", "repo": "skills" }).to_string(),
        )
        .unwrap();

        let skills = scan_skill_roots(&[root]);
        fs::remove_dir_all(temp_root).unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].origin.kind, SkillOriginKind::Collection);
        let repository = skills[0].origin.repository.as_ref().unwrap();
        assert_eq!(repository.id, "github/anthropics/skills");
        assert_eq!(repository.name, "anthropics/skills");
        assert_eq!(repository.url.as_deref(), Some("https://github.com/anthropics/skills"));
    }

    #[test]
    fn apply_skill_lock_sources_groups_github_skills_across_roots() {
        let temp_root = unique_temp_dir("scan-skill-lock");
        let agents_root = temp_root.join(".agents").join("skills");
        let codex_root = temp_root.join(".codex").join("skills");
        create_plain_skill(&agents_root, "hyperframes");
        create_plain_skill(&codex_root, "motion-graphics");
        let lock_path = temp_root.join(".agents").join(".skill-lock.json");
        let locked = serde_json::json!({
            "source": "heygen-com/hyperframes",
            "sourceType": "github",
            "sourceUrl": "https://github.com/heygen-com/hyperframes.git"
        });
        fs::write(
            &lock_path,
            serde_json::json!({ "skills": { "hyperframes": locked, "motion-graphics": locked } }).to_string(),
        )
        .unwrap();

        let mut skills = scan_skill_roots(&[agents_root, codex_root]);
        apply_skill_lock_sources(&mut skills, &lock_path);
        fs::remove_dir_all(temp_root).unwrap();

        assert_eq!(skills.len(), 2);
        for skill in &skills {
            assert_eq!(skill.origin.kind, SkillOriginKind::Collection);
            let repository = skill.origin.repository.as_ref().unwrap();
            assert_eq!(repository.id, "github/heygen-com/hyperframes");
            assert_eq!(repository.name, "heygen-com/hyperframes");
            assert_eq!(repository.url.as_deref(), Some("https://github.com/heygen-com/hyperframes"));
            // Tool-folder skills get it as their source, so the list groups them.
            assert_eq!(skill.detected_source_url.as_deref(), Some("https://github.com/heygen-com/hyperframes"));
        }
    }

    #[test]
    fn scan_plugin_cache_reads_manifest_and_keeps_latest_version() {
        let cache_root = unique_temp_dir("scan-plugin-cache");
        create_plugin_skill(&cache_root, "openai-curated-remote", "github", "0.1.8", "GitHub", "gh-fix-ci");
        create_plugin_skill(&cache_root, "openai-curated", "github", "0.1.10", "GitHub", "gh-fix-ci");

        let skills = scan_plugin_cache(&cache_root, &[]);
        fs::remove_dir_all(cache_root).unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].source, "Codex");
        assert_eq!(skills[0].origin.kind, SkillOriginKind::Plugin);
        let codex = skills[0].targets.iter().find(|target| target.id == "codex").unwrap();
        assert!(codex.enabled);
        let plugin = skills[0].origin.plugin.as_ref().unwrap();
        assert_eq!(plugin.id, "openai/github");
        assert_eq!(plugin.name, "GitHub");
        assert_eq!(plugin.version, "0.1.10");
        assert_eq!(plugin.publisher, "OpenAI");
        assert_eq!(plugin.repository.as_deref(), Some("https://github.com/openai/plugins"));
    }

    #[test]
    fn scan_plugin_cache_marks_bundled_packages_as_system() {
        let cache_root = unique_temp_dir("scan-bundled-plugin");
        create_plugin_skill(&cache_root, "openai-bundled", "browser", "26.707.31428", "Browser", "browser");

        let skills = scan_plugin_cache(&cache_root, &[]);
        fs::remove_dir_all(cache_root).unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].origin.kind, SkillOriginKind::System);
        assert_eq!(skills[0].origin.plugin.as_ref().unwrap().name, "Browser");
    }

    #[test]
    fn a_copy_of_the_tools_own_version_counts_as_enabled() {
        let temp_root = unique_temp_dir("scan-variant");
        let data_root = temp_root.join(".skills-manage");
        let library_root = data_root.join("library");
        let codex_root = temp_root.join("codex-skills");
        fs::create_dir_all(library_root.join("dev")).unwrap();
        fs::write(library_root.join("dev").join("SKILL.md"), "---\nname: dev\ndescription: Main\n---\nMain\n").unwrap();
        let version = "---\nname: dev\ndescription: Codex\n---\nCodex\n";
        let codex_version = data_root.join("variants").join("dev").join("codex");
        fs::create_dir_all(&codex_version).unwrap();
        fs::write(codex_version.join("SKILL.md"), version).unwrap();
        fs::create_dir_all(codex_root.join("dev")).unwrap();
        fs::write(codex_root.join("dev").join("SKILL.md"), version).unwrap();

        let skills = scan_skill_roots_with_target_roots(&[library_root.clone()], &[("codex", codex_root.clone())]);
        let codex = skills[0].targets.iter().find(|target| target.id == "codex").unwrap();
        assert!(codex.enabled && !codex.conflict);
        assert_eq!(skills[0].variants.len(), 1);
        assert_eq!(skills[0].variants[0].target_id, "codex");
        assert_eq!(skills[0].variants[0].choice, crate::variants::VariantChoice::Variant);

        // Codex was switched to the main version: that copy no longer matches.
        fs::write(data_root.join("variant-prefs.json"), r#"{"dev":{"codex":"default"}}"#).unwrap();
        let skills = scan_skill_roots_with_target_roots(&[library_root.clone()], &[("codex", codex_root.clone())]);
        let codex = skills[0].targets.iter().find(|target| target.id == "codex").unwrap();
        assert!(!codex.enabled && codex.conflict);
        assert_eq!(skills[0].variants[0].choice, crate::variants::VariantChoice::Default);

        fs::remove_dir_all(temp_root).unwrap();
    }

    #[test]
    fn library_skills_report_tools_they_may_not_suit() {
        let temp_root = unique_temp_dir("scan-compat");
        let library_root = temp_root.join(".skills-manage").join("library");
        let claude_root = temp_root.join(".claude").join("skills");
        let body = "---\nname: todo\ndescription: Todo\n---\n\nTrack steps with TodoWrite.\n";
        for root in [&library_root, &claude_root] {
            fs::create_dir_all(root.join("todo")).unwrap();
            fs::write(root.join("todo").join("SKILL.md"), body).unwrap();
        }

        let skills = scan_skill_roots_with_target_roots(&[library_root.clone(), claude_root.clone()], &[]);
        let library = skills.iter().find(|skill| skill.source == "Shared Library").unwrap();
        let in_claude = skills.iter().find(|skill| skill.source == "Claude Code").unwrap();

        let codex = library.compat.iter().find(|entry| entry.target_id == "codex").unwrap();
        assert_eq!(codex.level, crate::compat::CompatLevel::Degraded);
        assert_eq!(codex.first.token, "TodoWrite");
        assert!(library.compat.iter().all(|entry| entry.target_id != "claude-code"));
        assert!(in_claude.compat.is_empty());

        fs::remove_dir_all(temp_root).unwrap();
    }

    #[test]
    fn scan_skill_roots_finds_directory_with_skill_markdown() {
        let root = unique_temp_dir("scan-skill-roots");
        let skill_dir = root.join("agent-tool-safety");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: agent-tool-safety\ndescription: Designs safe tools\n---\n# Body\n",
        )
        .unwrap();
        fs::write(skill_dir.join("notes.md"), "support").unwrap();

        let skills = scan_skill_roots(&[root.clone()]);

        fs::remove_dir_all(root).unwrap();

        assert_eq!(skills.len(), 1);
        assert!(skills[0].id.starts_with("agent-tool-safety-"));
        assert_eq!(skills[0].name, "agent-tool-safety");
        assert_eq!(skills[0].description, "Designs safe tools");
        assert_eq!(skills[0].health, SkillHealth::Healthy);
        assert!(!skills[0].managed);
        assert!(skills[0].support_files.contains(&"SKILL.md".to_string()));
        assert!(skills[0].support_files.contains(&"notes.md".to_string()));
    }

    #[test]
    fn scan_skill_roots_uses_directory_name_when_metadata_is_missing() {
        let root = unique_temp_dir("scan-missing-metadata");
        let skill_dir = root.join("plain-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# Plain skill\n").unwrap();

        let skills = scan_skill_roots(&[root.clone()]);

        fs::remove_dir_all(root).unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "plain-skill");
        assert_eq!(skills[0].description, "No description provided.");
        assert_eq!(skills[0].health, SkillHealth::Warning);
    }

    #[test]
    fn scan_skill_roots_ignores_missing_roots() {
        let root = unique_temp_dir("scan-missing-root");
        let skills = scan_skill_roots(&[root]);

        assert!(skills.is_empty());
    }

    #[test]
    fn scan_skips_hidden_staging_folders() {
        let root = unique_temp_dir("scan-hidden");
        let staging = root.join(".skills-manage-staging-foo-1");
        fs::create_dir_all(&staging).unwrap();
        fs::write(staging.join("SKILL.md"), "---\nname: foo\n---\n").unwrap();

        let skills = scan_skill_roots(&[root.clone()]);

        fs::remove_dir_all(root).unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn non_ascii_and_duplicate_names_get_distinct_ids() {
        let root = unique_temp_dir("scan-unicode-ids");
        for folder in ["first", "second"] {
            let skill_dir = root.join(folder);
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(
                skill_dir.join("SKILL.md"),
                "---\nname: 代码审查\ndescription: 审查代码\n---\n",
            )
            .unwrap();
        }

        let skills = scan_skill_roots(&[root.clone()]);

        fs::remove_dir_all(root).unwrap();
        assert_eq!(skills.len(), 2);
        assert!(skills[0].id.starts_with("代码审查-"));
        assert_ne!(skills[0].id, skills[1].id);
    }

    #[test]
    fn metadata_parser_handles_bom_crlf_quotes_and_block_scalars() {
        let parsed = parse_skill_metadata(
            "\u{feff}---\r\nname: \"bom-skill\"\r\ndescription: >-\r\n  Folded across\r\n  two lines\r\nlicense: MIT\r\n---\r\n# Body\r\n",
        );
        assert_eq!(parsed.name.as_deref(), Some("bom-skill"));
        assert_eq!(parsed.description.as_deref(), Some("Folded across two lines"));

        let literal = parse_skill_metadata("---\nname: lit\ndescription: |\n  line one\n  line two\n---\n");
        assert_eq!(literal.description.as_deref(), Some("line one\nline two"));

        let nested = parse_skill_metadata(
            "---\nmetadata:\n  name: nested\nname: top\ndescription: 'quoted: with colon'\n---\n",
        );
        assert_eq!(nested.name.as_deref(), Some("top"));
        assert_eq!(nested.description.as_deref(), Some("quoted: with colon"));
    }

    #[test]
    fn reports_github_source_url_and_scans_a_single_skill_folder() {
        let root = unique_temp_dir("scan-source-url");
        let skill_dir = root.join("from-github");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "---\nname: from-github\n---\n").unwrap();
        fs::write(
            skill_dir.join(SOURCE_MARKER_FILE),
            r#"{"kind":"github","url":"https://github.com/o/r/tree/main/x"}"#,
        )
        .unwrap();
        let plain = root.join("plain");
        fs::create_dir_all(&plain).unwrap();
        fs::write(plain.join("SKILL.md"), "# plain
").unwrap();

        let all = scan_folder(&root);
        let single = scan_folder(&skill_dir);

        fs::remove_dir_all(root).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(single.len(), 1);
        assert_eq!(
            single[0].source_url.as_deref(),
            Some("https://github.com/o/r/tree/main/x")
        );
        assert!(all.iter().any(|skill| skill.name == "plain" && skill.source_url.is_none()));
    }

    #[test]
    fn identical_tool_copies_count_as_enabled_and_different_ones_as_conflicts() {
        let temp_root = unique_temp_dir("scan-identical");
        let library_root = temp_root.join("library");
        let codex_root = temp_root.join("codex");
        let claude_root = temp_root.join("claude");
        for root in [&library_root, &codex_root, &claude_root] {
            fs::create_dir_all(root.join("same-skill")).unwrap();
        }
        let body = "---\nname: same-skill\ndescription: d\n---\n";
        fs::write(library_root.join("same-skill").join("SKILL.md"), body).unwrap();
        fs::write(codex_root.join("same-skill").join("SKILL.md"), body).unwrap();
        fs::write(claude_root.join("same-skill").join("SKILL.md"), "# edited by hand\n").unwrap();

        let skills = scan_skill_roots_with_target_roots(
            &[library_root.clone()],
            &[("codex", codex_root.clone()), ("claude-code", claude_root.clone())],
        );

        fs::remove_dir_all(temp_root).unwrap();
        let target = |id: &str| skills[0].targets.iter().find(|target| target.id == id).unwrap().clone();
        assert!(target("codex").enabled && !target("codex").conflict);
        assert!(!target("claude-code").enabled && target("claude-code").conflict);
    }

    #[test]
    fn managed_copies_are_flagged() {
        let root = unique_temp_dir("scan-managed-flag");
        let skill_dir = root.join("copy");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "---\nname: copy\n---\n").unwrap();
        fs::write(skill_dir.join(LINK_MARKER_FILE), "{}").unwrap();

        let skills = scan_skill_roots(&[root.clone()]);

        fs::remove_dir_all(root).unwrap();
        assert!(skills[0].managed);
    }

    #[test]
    fn shared_library_skill_reports_managed_target_enabled() {
        let temp_root = unique_temp_dir("scan-target-state-data");
        let data_root = temp_root.join(".skills-manage");
        let library_root = data_root.join("library");
        let codex_root = unique_temp_dir("scan-target-state-codex");
        let skill_dir = library_root.join("managed-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: managed-skill\ndescription: Managed\n---\n",
        )
        .unwrap();
        fs::create_dir_all(codex_root.join("managed-skill")).unwrap();
        fs::write(
            codex_root.join("managed-skill").join("SKILL.md"),
            "# Managed copy\n",
        )
        .unwrap();
        fs::write(
            codex_root
                .join("managed-skill")
                .join(".skills-manage-link.json"),
            serde_json::json!({
                "sourcePath": skill_dir.display().to_string(),
                "targetId": "codex",
                "targetName": "Codex",
                "managedBy": "Skills Manage",
                "createdAtUnixMs": 1
            })
            .to_string(),
        )
        .unwrap();

        let skills = scan_skill_roots_with_target_roots(
            &[library_root.clone()],
            &[("codex", codex_root.clone())],
        );

        let codex = skills[0]
            .targets
            .iter()
            .find(|target| target.id == "codex")
            .unwrap();
        assert!(codex.enabled);

        fs::remove_dir_all(temp_root).unwrap();
        fs::remove_dir_all(codex_root).unwrap();
    }

    #[test]
    fn linked_repositories_give_skills_a_source_but_frontmatter_wins() {
        let root = unique_temp_dir("scan-linked");
        for (folder, body) in [
            ("pdf", "---
name: pdf
description: d
---
"),
            ("reach", "---
name: reach
description: d
homepage: https://github.com/a/reach
---
"),
        ] {
            fs::create_dir_all(root.join(folder)).unwrap();
            fs::write(root.join(folder).join("SKILL.md"), body).unwrap();
        }
        let mut links = SeriesLinks::new();
        links.insert("pdf".to_string(), "https://github.com/o/skills/tree/main/skills/pdf".to_string());
        links.insert("reach".to_string(), "https://github.com/o/skills/tree/main/reach".to_string());

        let skills = scan_skill_roots_with_links(&[root.clone()], &[], &links);
        fs::remove_dir_all(root).unwrap();

        let pdf = skills.iter().find(|skill| skill.name == "pdf").unwrap();
        assert_eq!(pdf.detected_source_url.as_deref(), Some("https://github.com/o/skills/tree/main/skills/pdf"));
        assert!(pdf.linked_source);
        let reach = skills.iter().find(|skill| skill.name == "reach").unwrap();
        assert_eq!(reach.detected_source_url.as_deref(), Some("https://github.com/a/reach"));
        assert!(!reach.linked_source);
    }

    #[test]
    fn reads_skill_markdown_only_inside_known_roots() {
        let root = unique_temp_dir("read-skill-md");
        let skill = root.join("pdf");
        fs::create_dir_all(skill.join("nested")).unwrap();
        fs::write(skill.join("SKILL.md"), "---
name: pdf
---
# PDF
").unwrap();
        fs::write(skill.join("nested").join("SKILL.md"), "x").unwrap();
        let roots = vec![root.clone()];

        let text = read_skill_markdown_in(&skill.display().to_string(), &roots).unwrap();
        assert!(text.contains("# PDF"));
        // Not directly in a known root, or outside it entirely.
        assert!(read_skill_markdown_in(&skill.join("nested").display().to_string(), &roots).is_err());
        assert!(read_skill_markdown_in(&root.display().to_string(), &roots).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn finds_hermes_skills_inside_category_folders() {
        let home = unique_temp_dir("hermes-home");
        let root = home.join(".hermes").join("skills");
        fs::create_dir_all(root.join("research").join("arxiv")).unwrap();
        fs::write(root.join("research").join("arxiv").join("SKILL.md"), "---\nname: arxiv\ndescription: d\n---\n").unwrap();
        fs::create_dir_all(root.join("flat")).unwrap();
        fs::write(root.join("flat").join("SKILL.md"), "---\nname: flat\ndescription: d\n---\n").unwrap();

        let skills = scan_skill_roots(&[root.clone()]);
        let mut names: Vec<_> = skills.iter().map(|skill| skill.name.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["arxiv", "flat"]);
        assert!(skills.iter().all(|skill| skill.source == "Hermes"));

        // Other tools are not searched below their top level.
        let codex = home.join(".codex").join("skills");
        fs::create_dir_all(codex.join("group").join("inner")).unwrap();
        fs::write(codex.join("group").join("inner").join("SKILL.md"), "---\nname: inner\n---\n").unwrap();
        assert!(scan_skill_roots(&[codex]).is_empty());
        let _ = fs::remove_dir_all(home);
    }
}
