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

pub fn scan_default_skills() -> Vec<ScannedSkill> {
    let roots = candidate_skill_roots();
    scan_skill_roots(&roots)
}

pub fn candidate_skill_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(home) = home_dir() {
        roots.push(home.join(".agents").join("skills"));
        roots.push(home.join(".codex").join("skills"));
        roots.push(home.join(".claude").join("skills"));
        roots.push(home.join(".skills-manage").join("library"));
    }

    roots
}

pub fn scan_skill_roots(roots: &[PathBuf]) -> Vec<ScannedSkill> {
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

            if let Some(skill) = scan_skill_dir(root, &path, &skill_file) {
                skills.push(skill);
            }
        }
    }

    skills.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    skills
}

fn scan_skill_dir(root: &Path, skill_dir: &Path, skill_file: &Path) -> Option<ScannedSkill> {
    let directory_name = skill_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown-skill")
        .to_string();
    let Ok(contents) = fs::read_to_string(skill_file) else {
        return Some(ScannedSkill {
            id: slug_id(&directory_name),
            name: directory_name,
            description: "SKILL.md could not be read.".to_string(),
            source: source_label(root),
            source_path: skill_dir.display().to_string(),
            health: SkillHealth::Broken,
            targets: default_targets(root),
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
        id: slug_id(&name),
        name,
        description: metadata
            .description
            .unwrap_or_else(|| "No description provided.".to_string()),
        source: source_label(root),
        source_path: skill_dir.display().to_string(),
        health,
        targets: default_targets(root),
        support_files: support_files(skill_dir),
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
    let normalized = root.to_string_lossy().replace('\\', "/").to_lowercase();

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

fn default_targets(root: &Path) -> Vec<TargetState> {
    let source = source_label(root);

    ["Codex", "Claude Code", "VS Code"]
        .iter()
        .map(|target| TargetState {
            id: target.to_lowercase().replace(' ', "-"),
            name: target.to_string(),
            enabled: source == *target,
        })
        .collect()
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

fn home_dir() -> Option<PathBuf> {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
        .map(PathBuf::from)
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

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("skills-manage-{prefix}-{stamp}"))
    }
}
