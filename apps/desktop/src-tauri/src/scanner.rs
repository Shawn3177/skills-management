use crate::fs_ops::{home_dir, SOURCE_MARKER_FILE};
use crate::targets::is_managed_target_copy;
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
    pub origin: SkillOrigin,
    pub health: SkillHealth,
    pub targets: Vec<TargetState>,
    pub support_files: Vec<String>,
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
    Collection,
    Plugin,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetState {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillHealth {
    Healthy,
    Warning,
    Broken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SkillMetadata {
    name: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct PluginManifest {
    name: String,
    #[serde(default)]
    version: String,
    author: Option<PluginAuthor>,
    interface: Option<PluginInterface>,
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
#[serde(rename_all = "camelCase")]
struct GithubSourceMetadata {
    kind: String,
    owner: String,
    repo: String,
}

pub fn scan_default_skills() -> Vec<ScannedSkill> {
    let mut skills = scan_skill_roots(&candidate_skill_roots());

    if let Some(home) = home_dir() {
        let codex_root = home.join(".codex");
        skills.extend(scan_repository_collections(&codex_root));
        skills.extend(scan_plugin_cache(&codex_root.join("plugins").join("cache")));
    }

    sort_skills(&mut skills);
    skills
}

pub fn candidate_skill_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(home) = home_dir() {
        let codex_skills = home.join(".codex").join("skills");
        roots.push(home.join(".agents").join("skills"));
        roots.push(codex_skills.clone());
        roots.push(codex_skills.join(".system"));
        roots.push(home.join(".claude").join("skills"));
        roots.push(home.join(".skills-manage").join("library"));
    }

    roots
}

pub fn scan_skill_roots(roots: &[PathBuf]) -> Vec<ScannedSkill> {
    let target_roots = default_target_roots();
    scan_skill_roots_with_target_roots(roots, &target_roots)
}

pub fn scan_skill_roots_with_target_roots(
    roots: &[PathBuf],
    target_roots: &[(&str, PathBuf)],
) -> Vec<ScannedSkill> {
    let mut skills = Vec::new();

    for root in roots {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let skill_file = path.join("SKILL.md");
            if !skill_file.is_file() {
                continue;
            }

            if let Some(skill) = scan_skill_dir(root, &path, &skill_file, target_roots) {
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
) -> Option<ScannedSkill> {
    let source = source_label(root);
    scan_skill_dir_with_context(
        skill_dir,
        skill_file,
        &source,
        skill_origin_for_path(root, skill_dir),
        None,
        target_roots,
    )
}

fn scan_skill_dir_with_context(
    skill_dir: &Path,
    skill_file: &Path,
    source: &str,
    origin: SkillOrigin,
    id_prefix: Option<&str>,
    target_roots: &[(&str, PathBuf)],
) -> Option<ScannedSkill> {
    let directory_name = skill_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown-skill")
        .to_string();
    let Ok(contents) = fs::read_to_string(skill_file) else {
        return Some(ScannedSkill {
            id: skill_id(id_prefix, &directory_name),
            name: directory_name,
            description: "SKILL.md could not be read.".to_string(),
            source: source.to_string(),
            source_path: skill_dir.display().to_string(),
            origin,
            health: SkillHealth::Broken,
            targets: target_states(source, skill_dir, target_roots),
            support_files: support_files(skill_dir),
        });
    };
    let metadata = parse_skill_metadata(&contents);
    let health = if metadata.name.is_some() && metadata.description.is_some() {
        SkillHealth::Healthy
    } else {
        SkillHealth::Warning
    };
    let name = metadata.name.unwrap_or_else(|| directory_name.clone());

    Some(ScannedSkill {
        id: skill_id(id_prefix, &name),
        name,
        description: metadata
            .description
            .unwrap_or_else(|| "No description provided.".to_string()),
        source: source.to_string(),
        source_path: skill_dir.display().to_string(),
        origin,
        health,
        targets: target_states(source, skill_dir, target_roots),
        support_files: support_files(skill_dir),
    })
}

fn scan_repository_collections(codex_root: &Path) -> Vec<ScannedSkill> {
    let target_roots = default_target_roots();
    let mut skills = Vec::new();
    let Ok(entries) = fs::read_dir(codex_root) else {
        return skills;
    };

    for entry in entries.flatten() {
        let package_root = entry.path();
        if !package_root.is_dir() {
            continue;
        }

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

        let repository = RepositoryIdentity {
            id: format!("local/{}", slug_id(&manifest.name)),
            name: manifest.name,
            url: None,
        };
        let origin = SkillOrigin {
            kind: SkillOriginKind::Collection,
            plugin: None,
            repository: Some(repository.clone()),
        };
        let id_prefix = format!("collection:{}", repository.id);
        let Ok(skill_entries) = fs::read_dir(&skills_root) else {
            continue;
        };

        for skill_entry in skill_entries.flatten() {
            let skill_dir = skill_entry.path();
            let skill_file = skill_dir.join("SKILL.md");
            if !skill_dir.is_dir() || !skill_file.is_file() {
                continue;
            }

            if let Some(skill) = scan_skill_dir_with_context(
                &skill_dir,
                &skill_file,
                "Codex",
                origin.clone(),
                Some(&id_prefix),
                &target_roots,
            ) {
                skills.push(skill);
            }
        }
    }

    sort_skills(&mut skills);
    skills
}

fn scan_plugin_cache(cache_root: &Path) -> Vec<ScannedSkill> {
    let target_roots = default_target_roots();
    let mut skills = Vec::new();

    for package in latest_plugin_packages(cache_root) {
        let skills_root = package.package_root.join("skills");
        let Ok(entries) = fs::read_dir(&skills_root) else {
            continue;
        };

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
        let plugin = PluginIdentity {
            id: format!(
                "{}/{}",
                slug_id(&publisher),
                slug_id(&package.manifest.name)
            ),
            name: plugin_name,
            version,
            publisher,
        };
        let kind = plugin_origin_kind(&package.channel);
        let origin = SkillOrigin {
            kind,
            plugin: Some(plugin.clone()),
            repository: None,
        };
        let id_prefix = format!(
            "{}:{}",
            match kind {
                SkillOriginKind::System => "system",
                SkillOriginKind::Plugin => "plugin",
                SkillOriginKind::Collection => "collection",
                SkillOriginKind::Standalone => "skill",
            },
            plugin.id
        );

        for entry in entries.flatten() {
            let skill_dir = entry.path();
            if !skill_dir.is_dir() {
                continue;
            }

            let skill_file = skill_dir.join("SKILL.md");
            if !skill_file.is_file() {
                continue;
            }

            if let Some(skill) = scan_skill_dir_with_context(
                &skill_dir,
                &skill_file,
                "Codex",
                origin.clone(),
                Some(&id_prefix),
                &target_roots,
            ) {
                skills.push(skill);
            }
        }
    }

    sort_skills(&mut skills);
    skills
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
            let plugin_root = plugin_entry.path();
            if !plugin_root.is_dir() {
                continue;
            }
            let Ok(version_dirs) = fs::read_dir(&plugin_root) else {
                continue;
            };

            for version_entry in version_dirs.flatten() {
                let package_root = version_entry.path();
                if !package_root.is_dir()
                    || version_entry
                        .file_name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case("latest")
                {
                    continue;
                }

                let manifest_path = package_root.join(".codex-plugin").join("plugin.json");
                let Ok(contents) = fs::read_to_string(manifest_path) else {
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
                let candidate = PluginPackage {
                    channel: channel.clone(),
                    package_root,
                    manifest,
                };
                let should_replace = packages.get(&key).is_none_or(|existing| {
                    match natural_version_cmp(
                        &candidate.manifest.version,
                        &existing.manifest.version,
                    ) {
                        Ordering::Greater => true,
                        Ordering::Equal => candidate.package_root > existing.package_root,
                        Ordering::Less => false,
                    }
                });

                if should_replace {
                    packages.insert(key, candidate);
                }
            }
        }
    }

    packages.into_values().collect()
}

fn natural_version_cmp(left: &str, right: &str) -> Ordering {
    let left_numbers = numeric_version_parts(left);
    let right_numbers = numeric_version_parts(right);

    left_numbers
        .cmp(&right_numbers)
        .then_with(|| left.cmp(right))
}

fn numeric_version_parts(value: &str) -> Vec<u64> {
    value
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

fn plugin_origin_kind(channel: &str) -> SkillOriginKind {
    match channel.to_lowercase().as_str() {
        "openai-bundled" | "openai-primary-runtime" => SkillOriginKind::System,
        _ => SkillOriginKind::Plugin,
    }
}

fn skill_origin_for_path(root: &Path, skill_dir: &Path) -> SkillOrigin {
    if let Some(repository) = repository_from_source_marker(skill_dir) {
        return SkillOrigin {
            kind: SkillOriginKind::Collection,
            plugin: None,
            repository: Some(repository),
        };
    }

    let normalized = normalized_path(root);
    let kind = if normalized.contains("/.codex/skills/.system") {
        SkillOriginKind::System
    } else {
        SkillOriginKind::Standalone
    };

    SkillOrigin {
        kind,
        plugin: None,
        repository: None,
    }
}

fn repository_from_source_marker(skill_dir: &Path) -> Option<RepositoryIdentity> {
    let contents = fs::read_to_string(skill_dir.join(SOURCE_MARKER_FILE)).ok()?;
    let source = serde_json::from_str::<GithubSourceMetadata>(&contents).ok()?;
    if !source.kind.eq_ignore_ascii_case("github")
        || source.owner.trim().is_empty()
        || source.repo.trim().is_empty()
    {
        return None;
    }

    Some(RepositoryIdentity {
        id: format!(
            "github/{}/{}",
            slug_id(&source.owner),
            slug_id(&source.repo)
        ),
        name: format!("{}/{}", source.owner, source.repo),
        url: Some(format!(
            "https://github.com/{}/{}",
            source.owner, source.repo
        )),
    })
}

fn parse_skill_metadata(contents: &str) -> SkillMetadata {
    let mut metadata = SkillMetadata {
        name: None,
        description: None,
    };

    let mut lines = contents.lines();
    if lines.next() != Some("---") {
        return metadata;
    }

    for line in lines {
        if line.trim() == "---" {
            break;
        }

        if let Some(value) = line.strip_prefix("name:") {
            metadata.name = Some(clean_metadata_value(value));
        }

        if let Some(value) = line.strip_prefix("description:") {
            metadata.description = Some(clean_metadata_value(value));
        }
    }

    metadata
}

fn clean_metadata_value(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
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

fn source_label(root: &Path) -> String {
    let normalized = normalized_path(root);

    if normalized.contains("/.codex/skills") {
        "Codex".to_string()
    } else if normalized.contains("/.claude/skills") {
        "Claude Code".to_string()
    } else if normalized.contains("/.agents/skills") {
        "Agent Skills".to_string()
    } else if normalized.contains("/.skills-manage/library") {
        "Shared Library".to_string()
    } else {
        "Manual Path".to_string()
    }
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
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

    [
        ("codex", "Codex"),
        ("claude-code", "Claude Code"),
        ("vs-code", "VS Code"),
    ]
    .iter()
    .map(|(target_id, target_name)| TargetState {
        id: target_id.to_string(),
        name: target_name.to_string(),
        enabled: target_enabled(
            source,
            skill_dir,
            skill_folder,
            target_id,
            target_name,
            target_roots,
        ),
    })
    .collect()
}

fn target_enabled(
    source: &str,
    skill_dir: &Path,
    skill_folder: &str,
    target_id: &str,
    target_name: &str,
    target_roots: &[(&str, PathBuf)],
) -> bool {
    if source == target_name {
        return true;
    }

    if source != "Shared Library" {
        return false;
    }

    target_roots
        .iter()
        .find(|(candidate_id, _)| *candidate_id == target_id)
        .map(|(_, target_root)| {
            is_managed_target_copy(&target_root.join(skill_folder), skill_dir, target_id)
        })
        .unwrap_or(false)
}

fn default_target_roots() -> Vec<(&'static str, PathBuf)> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };

    vec![
        ("codex", home.join(".codex").join("skills")),
        ("claude-code", home.join(".claude").join("skills")),
    ]
}

fn skill_id(prefix: Option<&str>, value: &str) -> String {
    let slug = slug_id(value);

    prefix
        .map(|prefix| format!("{prefix}/{slug}"))
        .unwrap_or(slug)
}

fn slug_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

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
        assert_eq!(skills[0].id, "agent-tool-safety");
        assert_eq!(skills[0].name, "agent-tool-safety");
        assert_eq!(skills[0].description, "Designs safe tools");
        assert_eq!(skills[0].health, SkillHealth::Healthy);
        assert_eq!(skills[0].origin.kind, SkillOriginKind::Standalone);
        assert_eq!(skills[0].origin.plugin, None);
        assert!(skills[0].support_files.contains(&"SKILL.md".to_string()));
        assert!(skills[0].support_files.contains(&"notes.md".to_string()));
    }

    #[test]
    fn scan_skill_roots_marks_codex_system_skills() {
        let temp_root = unique_temp_dir("scan-system-skill");
        let root = temp_root.join(".codex").join("skills").join(".system");
        let skill_dir = root.join("skill-installer");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: skill-installer\ndescription: Installs Codex skills\n---\n",
        )
        .unwrap();

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

        let skills = scan_repository_collections(&codex_root);

        fs::remove_dir_all(codex_root).unwrap();

        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].origin.kind, SkillOriginKind::Collection);
        assert_eq!(skills[0].source, "Codex");
        assert!(skills[0].targets[0].enabled);
        let repository = skills[0].origin.repository.as_ref().unwrap();
        assert_eq!(repository.id, "local/anthropic-agent-skills");
        assert_eq!(repository.name, "anthropic-agent-skills");
        assert_eq!(repository.url, None);
        assert!(skills[0]
            .id
            .starts_with("collection:local/anthropic-agent-skills/"));
    }

    #[test]
    fn scan_skill_roots_reads_github_repository_marker() {
        let temp_root = unique_temp_dir("scan-github-source");
        let root = temp_root.join(".skills-manage").join("library");
        let skill_dir = root.join("pdf");
        create_plain_skill(&root, "pdf");
        fs::write(
            skill_dir.join(SOURCE_MARKER_FILE),
            serde_json::json!({
                "kind": "github",
                "owner": "anthropics",
                "repo": "skills"
            })
            .to_string(),
        )
        .unwrap();

        let skills = scan_skill_roots(&[root]);

        fs::remove_dir_all(temp_root).unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].origin.kind, SkillOriginKind::Collection);
        let repository = skills[0].origin.repository.as_ref().unwrap();
        assert_eq!(repository.id, "github/anthropics/skills");
        assert_eq!(repository.name, "anthropics/skills");
        assert_eq!(
            repository.url.as_deref(),
            Some("https://github.com/anthropics/skills")
        );
    }
    #[test]
    fn scan_plugin_cache_reads_manifest_and_keeps_latest_version() {
        let cache_root = unique_temp_dir("scan-plugin-cache");
        create_plugin_skill(
            &cache_root,
            "openai-curated-remote",
            "github",
            "0.1.8",
            "GitHub",
            "gh-fix-ci",
        );
        create_plugin_skill(
            &cache_root,
            "openai-curated",
            "github",
            "0.1.10",
            "GitHub",
            "gh-fix-ci",
        );

        let skills = scan_plugin_cache(&cache_root);

        fs::remove_dir_all(cache_root).unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].source, "Codex");
        assert_eq!(skills[0].origin.kind, SkillOriginKind::Plugin);
        assert_eq!(skills[0].targets[0].id, "codex");
        assert!(skills[0].targets[0].enabled);
        let plugin = skills[0].origin.plugin.as_ref().unwrap();
        assert_eq!(plugin.id, "openai/github");
        assert_eq!(plugin.name, "GitHub");
        assert_eq!(plugin.version, "0.1.10");
        assert_eq!(plugin.publisher, "OpenAI");
    }

    #[test]
    fn scan_plugin_cache_marks_bundled_packages_as_system() {
        let cache_root = unique_temp_dir("scan-bundled-plugin");
        create_plugin_skill(
            &cache_root,
            "openai-bundled",
            "browser",
            "26.707.31428",
            "Browser",
            "browser",
        );

        let skills = scan_plugin_cache(&cache_root);

        fs::remove_dir_all(cache_root).unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].origin.kind, SkillOriginKind::System);
        assert_eq!(skills[0].origin.plugin.as_ref().unwrap().name, "Browser");
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

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("skills-manage-{prefix}-{stamp}"))
    }

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
                "interface": { "displayName": display_name }
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {skill_name}\ndescription: Plugin skill\n---\n"),
        )
        .unwrap();
    }
}
