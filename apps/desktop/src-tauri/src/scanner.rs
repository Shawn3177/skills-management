use crate::fs_ops::{home_dir, same_skill_content, short_hash, LINK_MARKER_FILE, SOURCE_MARKER_FILE};
use crate::series::{self, SeriesLinks};
use crate::targets::{self, is_managed_target_copy};
use serde::Serialize;
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct SkillMetadata {
    name: Option<String>,
    description: Option<String>,
}

pub fn scan_default_skills() -> Vec<ScannedSkill> {
    let roots = candidate_skill_roots();
    scan_skill_roots_with_links(&roots, &default_target_roots(), &series::load_default_links())
}

pub fn candidate_skill_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(home) = home_dir() {
        roots.push(home.join(".agents").join("skills"));
        for (_, _, folder) in targets::TOOLS {
            roots.push(home.join(folder).join("skills"));
        }
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

    skills.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    skills
}

fn scan_skill_dir(
    root: &Path,
    skill_dir: &Path,
    skill_file: &Path,
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
            source: source_label(root),
            source_path: skill_dir.display().to_string(),
            health: SkillHealth::Broken,
            targets: target_states(root, skill_dir, target_roots),
            support_files: support_files(skill_dir),
            managed: is_managed_copy(skill_dir),
            source_url: origin.url,
            origin_kind: origin.kind,
            origin_detail: origin.detail,
            detected_source_url: None,
            linked_source: false,
            issues: vec!["unreadable".to_string()],
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

    Some(ScannedSkill {
        id: skill_id(&name, skill_dir),
        name,
        description: metadata
            .description
            .unwrap_or_else(|| "No description provided.".to_string()),
        source: source_label(root),
        source_path: skill_dir.display().to_string(),
        health,
        targets: target_states(root, skill_dir, target_roots),
        support_files: support_files(skill_dir),
        managed: is_managed_copy(skill_dir),
        source_url: origin.url,
        origin_kind: origin.kind,
        origin_detail: origin.detail,
        detected_source_url,
        linked_source,
        issues,
    })
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
    root: &Path,
    skill_dir: &Path,
    target_roots: &[(&str, PathBuf)],
) -> Vec<TargetState> {
    let source = source_label(root);
    let skill_folder = skill_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown-skill");

    targets::TOOLS
        .iter()
        .map(|(target_id, target_name, _)| {
            let (enabled, conflict) =
                target_state(&source, skill_dir, skill_folder, target_id, target_name, target_roots);
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
    if is_managed_target_copy(&copy, skill_dir, target_id) || same_skill_content(skill_dir, &copy) {
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
