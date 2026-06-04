use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MARKER_FILE: &str = ".skills-manage-link.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetToggleResult {
    pub target_id: String,
    pub target_name: String,
    pub skill_name: String,
    pub enabled: bool,
    pub changed: bool,
    pub target_path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedMarker {
    source_path: String,
    target_id: String,
    target_name: String,
    managed_by: String,
    created_at_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TargetProfile {
    id: &'static str,
    name: &'static str,
    root: PathBuf,
}

pub fn set_skill_target_enabled(
    source_path: String,
    target_id: String,
    enabled: bool,
) -> Result<TargetToggleResult, String> {
    let data_root = default_data_root()?;
    let target = target_profile(&target_id)?;
    set_skill_target_enabled_with_root(
        Path::new(&source_path),
        target.id,
        enabled,
        &data_root,
        &target.root,
    )
}

pub fn set_skill_target_enabled_with_root(
    source_path: &Path,
    target_id: &str,
    enabled: bool,
    data_root: &Path,
    target_root_override: &Path,
) -> Result<TargetToggleResult, String> {
    let target = target_profile_with_root(target_id, target_root_override)?;
    let source_canonical = validate_library_source(source_path, data_root)?;
    let skill_name = skill_folder_name(source_path);
    let destination = target.root.join(&skill_name);

    if enabled {
        enable_target_copy(
            &source_canonical,
            source_path,
            &target,
            &destination,
            &skill_name,
        )
    } else {
        disable_target_copy(
            &source_canonical,
            data_root,
            &target,
            &destination,
            &skill_name,
        )
    }
}

pub fn is_managed_target_copy(target_dir: &Path, source_path: &Path, target_id: &str) -> bool {
    let Ok(source_canonical) = source_path.canonicalize() else {
        return false;
    };

    marker_matches(target_dir, &source_canonical, target_id)
}

fn enable_target_copy(
    source_canonical: &Path,
    source_path: &Path,
    target: &TargetProfile,
    destination: &Path,
    skill_name: &str,
) -> Result<TargetToggleResult, String> {
    fs::create_dir_all(&target.root)
        .map_err(|error| format!("Could not create target root: {error}"))?;

    if destination.exists() {
        if marker_matches(destination, source_canonical, target.id) {
            return Ok(TargetToggleResult {
                target_id: target.id.to_string(),
                target_name: target.name.to_string(),
                skill_name: skill_name.to_string(),
                enabled: true,
                changed: false,
                target_path: destination.display().to_string(),
                message: format!("{skill_name} is already enabled for {}.", target.name),
            });
        }

        return Err(format!(
            "Target folder already exists and is not managed by Skills Manage: {}",
            destination.display()
        ));
    }

    copy_skill_dir(source_path, destination)?;
    write_marker(destination, source_canonical, target)?;

    Ok(TargetToggleResult {
        target_id: target.id.to_string(),
        target_name: target.name.to_string(),
        skill_name: skill_name.to_string(),
        enabled: true,
        changed: true,
        target_path: destination.display().to_string(),
        message: format!("Enabled {skill_name} for {}.", target.name),
    })
}

fn disable_target_copy(
    source_canonical: &Path,
    data_root: &Path,
    target: &TargetProfile,
    destination: &Path,
    skill_name: &str,
) -> Result<TargetToggleResult, String> {
    if !destination.exists() {
        return Ok(TargetToggleResult {
            target_id: target.id.to_string(),
            target_name: target.name.to_string(),
            skill_name: skill_name.to_string(),
            enabled: false,
            changed: false,
            target_path: destination.display().to_string(),
            message: format!("{skill_name} is already disabled for {}.", target.name),
        });
    }

    if !marker_matches(destination, source_canonical, target.id) {
        return Err(format!(
            "Target folder exists and is not managed by Skills Manage: {}",
            destination.display()
        ));
    }

    let trash_path = unique_trash_path(data_root, target.id, skill_name)?;
    fs::rename(destination, &trash_path)
        .map_err(|error| format!("Could not move managed target folder to trash: {error}"))?;

    Ok(TargetToggleResult {
        target_id: target.id.to_string(),
        target_name: target.name.to_string(),
        skill_name: skill_name.to_string(),
        enabled: false,
        changed: true,
        target_path: destination.display().to_string(),
        message: format!("Disabled {skill_name} for {}.", target.name),
    })
}

fn validate_library_source(source_path: &Path, data_root: &Path) -> Result<PathBuf, String> {
    if !source_path.is_dir() {
        return Err(format!(
            "Source path is not a directory: {}",
            source_path.display()
        ));
    }

    if !source_path.join("SKILL.md").is_file() {
        return Err(format!(
            "Source folder must contain SKILL.md: {}",
            source_path.display()
        ));
    }

    let library_root = data_root.join("library");
    let library_canonical = library_root
        .canonicalize()
        .map_err(|error| format!("Could not read library root: {error}"))?;
    let source_canonical = source_path
        .canonicalize()
        .map_err(|error| format!("Could not read source path: {error}"))?;

    if !source_canonical.starts_with(&library_canonical) {
        return Err(format!(
            "Only skills in the shared library can be enabled: {}",
            source_path.display()
        ));
    }

    Ok(source_canonical)
}

fn target_profile(target_id: &str) -> Result<TargetProfile, String> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| "Could not determine home directory.".to_string())?;
    let home = PathBuf::from(home);

    match target_id {
        "codex" => Ok(TargetProfile {
            id: "codex",
            name: "Codex",
            root: home.join(".codex").join("skills"),
        }),
        "claude-code" => Ok(TargetProfile {
            id: "claude-code",
            name: "Claude Code",
            root: home.join(".claude").join("skills"),
        }),
        other => Err(format!("Unsupported target id: {other}")),
    }
}

fn target_profile_with_root(target_id: &str, root: &Path) -> Result<TargetProfile, String> {
    let target = target_profile(target_id)?;

    Ok(TargetProfile {
        root: root.to_path_buf(),
        ..target
    })
}

fn default_data_root() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("SKILLS_MANAGE_DATA_ROOT") {
        return Ok(PathBuf::from(path));
    }

    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| "Could not determine home directory.".to_string())?;

    Ok(PathBuf::from(home).join(".skills-manage"))
}

fn write_marker(
    destination: &Path,
    source_canonical: &Path,
    target: &TargetProfile,
) -> Result<(), String> {
    let marker = ManagedMarker {
        source_path: source_canonical.display().to_string(),
        target_id: target.id.to_string(),
        target_name: target.name.to_string(),
        managed_by: "Skills Manage".to_string(),
        created_at_unix_ms: unix_ms(),
    };
    let contents = serde_json::to_string_pretty(&marker)
        .map_err(|error| format!("Could not serialize managed marker: {error}"))?;

    fs::write(destination.join(MARKER_FILE), contents)
        .map_err(|error| format!("Could not write managed marker: {error}"))
}

fn marker_matches(target_dir: &Path, source_canonical: &Path, target_id: &str) -> bool {
    let marker_path = target_dir.join(MARKER_FILE);
    let Ok(contents) = fs::read_to_string(marker_path) else {
        return false;
    };
    let Ok(marker) = serde_json::from_str::<ManagedMarker>(&contents) else {
        return false;
    };

    if marker.target_id != target_id {
        return false;
    }

    let marker_source = PathBuf::from(&marker.source_path);
    marker_source
        .canonicalize()
        .map(|path| path == source_canonical)
        .unwrap_or_else(|_| marker.source_path == source_canonical.display().to_string())
}

fn unique_trash_path(
    data_root: &Path,
    target_id: &str,
    skill_name: &str,
) -> Result<PathBuf, String> {
    let trash_root = data_root.join("trash");
    fs::create_dir_all(&trash_root)
        .map_err(|error| format!("Could not create trash root: {error}"))?;

    for copy_index in 0.. {
        let suffix = if copy_index == 0 {
            String::new()
        } else {
            format!("-{copy_index}")
        };
        let candidate = trash_root.join(format!("{target_id}-{skill_name}-{}{suffix}", unix_ms()));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    unreachable!("unbounded trash index should always find an available folder")
}

fn skill_folder_name(source_path: &Path) -> String {
    source_path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .map(safe_folder_name)
        .unwrap_or_else(|| "managed-skill".to_string())
}

fn safe_folder_name(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();

    if sanitized.is_empty() {
        "managed-skill".to_string()
    } else {
        sanitized
    }
}

fn copy_skill_dir(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|error| format!("Could not create target destination: {error}"))?;

    for entry in fs::read_dir(source).map_err(|error| format!("Could not read source: {error}"))? {
        let entry = entry.map_err(|error| format!("Could not read source entry: {error}"))?;
        let name = entry.file_name();
        let name_text = name.to_string_lossy();

        if is_excluded_entry(&name_text) {
            continue;
        }

        let source_path = entry.path();
        let destination_path = destination.join(&name);

        if source_path.is_dir() {
            copy_skill_dir(&source_path, &destination_path)?;
        } else if source_path.is_file() {
            fs::copy(&source_path, &destination_path).map_err(|error| {
                format!("Could not copy file {}: {error}", source_path.display())
            })?;
        }
    }

    Ok(())
}

fn is_excluded_entry(name: &str) -> bool {
    matches!(
        name,
        ".git" | ".env" | "node_modules" | "dist" | "target" | "cache" | ".cache" | MARKER_FILE
    )
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn enables_shared_library_skill_as_managed_copy() {
        let data_root = unique_temp_dir("data-root-target-enable");
        let library_skill = create_library_skill(&data_root, "copy-me");
        let target_root = unique_temp_dir("codex-target-root");

        let result = set_skill_target_enabled_with_root(
            &library_skill,
            "codex",
            true,
            &data_root,
            &target_root,
        )
        .unwrap();

        assert!(result.enabled);
        assert!(result.changed);
        assert!(target_root.join("copy-me").join("SKILL.md").is_file());
        assert!(target_root
            .join("copy-me")
            .join(".skills-manage-link.json")
            .is_file());

        fs::remove_dir_all(data_root).unwrap();
        fs::remove_dir_all(target_root).unwrap();
    }

    #[test]
    fn enable_refuses_to_overwrite_unmanaged_target_folder() {
        let data_root = unique_temp_dir("data-root-target-conflict");
        let library_skill = create_library_skill(&data_root, "conflict-skill");
        let target_root = unique_temp_dir("codex-target-conflict");
        fs::create_dir_all(target_root.join("conflict-skill")).unwrap();
        fs::write(
            target_root.join("conflict-skill").join("SKILL.md"),
            "# User folder\n",
        )
        .unwrap();

        let error = set_skill_target_enabled_with_root(
            &library_skill,
            "codex",
            true,
            &data_root,
            &target_root,
        )
        .unwrap_err();

        assert!(error.contains("not managed by Skills Manage"));
        assert_eq!(
            fs::read_to_string(target_root.join("conflict-skill").join("SKILL.md")).unwrap(),
            "# User folder\n"
        );

        fs::remove_dir_all(data_root).unwrap();
        fs::remove_dir_all(target_root).unwrap();
    }

    #[test]
    fn disables_managed_copy_by_moving_it_to_trash() {
        let data_root = unique_temp_dir("data-root-target-disable");
        let library_skill = create_library_skill(&data_root, "disable-me");
        let target_root = unique_temp_dir("claude-target-root");
        set_skill_target_enabled_with_root(
            &library_skill,
            "claude-code",
            true,
            &data_root,
            &target_root,
        )
        .unwrap();

        let result = set_skill_target_enabled_with_root(
            &library_skill,
            "claude-code",
            false,
            &data_root,
            &target_root,
        )
        .unwrap();

        assert!(!result.enabled);
        assert!(result.changed);
        assert!(!target_root.join("disable-me").exists());
        assert!(fs::read_dir(data_root.join("trash")).unwrap().count() >= 1);

        fs::remove_dir_all(data_root).unwrap();
        fs::remove_dir_all(target_root).unwrap();
    }

    #[test]
    fn disable_refuses_unmanaged_target_folder() {
        let data_root = unique_temp_dir("data-root-target-disable-conflict");
        let library_skill = create_library_skill(&data_root, "user-owned");
        let target_root = unique_temp_dir("claude-target-conflict");
        fs::create_dir_all(target_root.join("user-owned")).unwrap();
        fs::write(
            target_root.join("user-owned").join("SKILL.md"),
            "# User folder\n",
        )
        .unwrap();

        let error = set_skill_target_enabled_with_root(
            &library_skill,
            "claude-code",
            false,
            &data_root,
            &target_root,
        )
        .unwrap_err();

        assert!(error.contains("not managed by Skills Manage"));
        assert!(target_root.join("user-owned").join("SKILL.md").is_file());

        fs::remove_dir_all(data_root).unwrap();
        fs::remove_dir_all(target_root).unwrap();
    }

    fn create_library_skill(data_root: &PathBuf, name: &str) -> PathBuf {
        let skill_dir = data_root.join("library").join(name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Test skill\n---\n# Skill\n"),
        )
        .unwrap();
        skill_dir
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("skills-manage-{prefix}-{stamp}"))
    }
}
