use crate::fs_ops::{
    copy_skill_dir, default_data_root, display_path, home_dir, safe_folder_name,
    same_skill_content, unique_timestamped_path, unix_ms, LINK_MARKER_FILE,
};
use crate::trash::unique_trash_path;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Every tool the UI knows about, in display order. VS Code is listed but not
/// yet toggleable (see `toggleable_target_ids`).
pub const KNOWN_TARGETS: [(&str, &str); 3] = [
    ("codex", "Codex"),
    ("claude-code", "Claude Code"),
    ("vs-code", "VS Code"),
];

const STAGING_PREFIX: &str = ".skills-manage-staging";
/// Staging folders older than this are leftovers from a crashed enable.
const STALE_STAGING_AGE: Duration = Duration::from_secs(60 * 60);

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkToggleResult {
    pub succeeded: usize,
    pub failed: usize,
    pub target_id: String,
    pub target_name: String,
    pub enabled: bool,
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
    replace_existing: bool,
) -> Result<TargetToggleResult, String> {
    let data_root = default_data_root()?;
    let target = target_profile(&target_id)?;
    set_skill_target_enabled_with_options(
        Path::new(&source_path),
        target.id,
        enabled,
        &data_root,
        &target.root,
        replace_existing,
    )
}

pub fn set_skill_targets_bulk(
    source_paths: Vec<String>,
    target_id: String,
    enabled: bool,
) -> Result<BulkToggleResult, String> {
    let data_root = default_data_root()?;
    let target = target_profile(&target_id)?;
    set_skill_targets_bulk_with_root(source_paths, target.id, enabled, &data_root, &target.root)
}

fn set_skill_targets_bulk_with_root(
    source_paths: Vec<String>,
    target_id: &str,
    enabled: bool,
    data_root: &Path,
    target_root: &Path,
) -> Result<BulkToggleResult, String> {
    let target = target_profile_with_root(target_id, target_root)?;

    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut last_error: Option<String> = None;

    for source_path in &source_paths {
        match set_skill_target_enabled_with_root(
            Path::new(source_path),
            target.id,
            enabled,
            data_root,
            &target.root,
        ) {
            Ok(_) => succeeded += 1,
            Err(error) => {
                failed += 1;
                last_error = Some(error);
            }
        }
    }

    if succeeded == 0 {
        return Err(last_error.unwrap_or_else(|| "No skills were updated.".to_string()));
    }

    Ok(BulkToggleResult {
        succeeded,
        failed,
        target_id: target.id.to_string(),
        target_name: target.name.to_string(),
        enabled,
    })
}

pub fn set_skill_target_enabled_with_root(
    source_path: &Path,
    target_id: &str,
    enabled: bool,
    data_root: &Path,
    target_root_override: &Path,
) -> Result<TargetToggleResult, String> {
    set_skill_target_enabled_with_options(source_path, target_id, enabled, data_root, target_root_override, false)
}

/// `replace_existing`: when enabling and the tool already has a same-named
/// folder with different content, move that folder to the trash and install
/// the library version. Only ever set after the user explicitly confirmed.
pub fn set_skill_target_enabled_with_options(
    source_path: &Path,
    target_id: &str,
    enabled: bool,
    data_root: &Path,
    target_root_override: &Path,
    replace_existing: bool,
) -> Result<TargetToggleResult, String> {
    let target = target_profile_with_root(target_id, target_root_override)?;
    let source_canonical = validate_library_source(source_path, data_root)?;
    let skill_name = skill_folder_name(source_path);
    let destination = target.root.join(&skill_name);

    if enabled {
        // A same-named folder the user put there themselves: an identical copy
        // simply becomes the managed copy; a different one is only replaced on
        // explicit request (it goes to the trash, never deleted).
        if destination.exists() && !marker_matches(&destination, &source_canonical, target.id) {
            // A copy the tool has switched off (no SKILL.md, e.g. only
            // SKILL.md.disabled): set it aside in the trash and install fresh.
            if !destination.join("SKILL.md").is_file() {
                let trash_path = unique_trash_path(data_root, &format!("{}-{skill_name}-disabled", target.id))?;
                fs::rename(&destination, &trash_path)
                    .map_err(|error| format!("Could not move the disabled copy to trash: {error}"))?;
                return enable_target_copy(&source_canonical, source_path, &target, &destination, &skill_name);
            }
            if same_skill_content(source_path, &destination) {
                write_marker(&destination, &source_canonical, &target)?;
                return Ok(TargetToggleResult {
                    target_id: target.id.to_string(),
                    target_name: target.name.to_string(),
                    skill_name: skill_name.clone(),
                    enabled: true,
                    changed: true,
                    target_path: display_path(&destination),
                    message: format!("{skill_name} was already in {}; it is now managed.", target.name),
                });
            }
            if replace_existing {
                let trash_path = unique_trash_path(data_root, &format!("{}-{skill_name}-replaced", target.id))?;
                fs::rename(&destination, &trash_path)
                    .map_err(|error| format!("Could not move the existing folder to trash: {error}"))?;
            }
        }
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

/// Target ids that support real enable/disable (VS Code is still pending).
pub fn toggleable_target_ids() -> &'static [&'static str] {
    &["codex", "claude-code"]
}

/// The real, home-based skills-folder root for a toggleable target.
pub fn target_root_for(target_id: &str) -> Result<PathBuf, String> {
    Ok(target_profile(target_id)?.root)
}

/// The display name for a target id, when it is a supported target.
pub fn target_name_for(target_id: &str) -> Option<&'static str> {
    target_profile(target_id).ok().map(|target| target.name)
}

/// Where a given library skill's managed copy lives under `target_root`.
pub fn managed_skill_dir(target_root: &Path, library_path: &Path) -> PathBuf {
    target_root.join(skill_folder_name(library_path))
}

fn enable_target_copy(
    source_canonical: &Path,
    source_path: &Path,
    target: &TargetProfile,
    destination: &Path,
    skill_name: &str,
) -> Result<TargetToggleResult, String> {
    enable_target_copy_with_marker_writer(
        source_canonical,
        source_path,
        target,
        destination,
        skill_name,
        write_marker,
    )
}

fn enable_target_copy_with_marker_writer<F>(
    source_canonical: &Path,
    source_path: &Path,
    target: &TargetProfile,
    destination: &Path,
    skill_name: &str,
    marker_writer: F,
) -> Result<TargetToggleResult, String>
where
    F: Fn(&Path, &Path, &TargetProfile) -> Result<(), String>,
{
    fs::create_dir_all(&target.root)
        .map_err(|error| format!("Could not create target root: {error}"))?;
    remove_stale_staging(&target.root);

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
            "Target folder already exists with different content and is not managed by Skills Manage: {}",
            display_path(destination)
        ));
    }

    let staging_destination =
        unique_timestamped_path(&target.root, &format!("{STAGING_PREFIX}-{skill_name}"));

    if let Err(error) = copy_skill_dir(source_path, &staging_destination) {
        return Err(cleanup_staging_after_failure(&staging_destination, error));
    }

    if let Err(error) = marker_writer(&staging_destination, source_canonical, target) {
        return Err(cleanup_staging_after_failure(&staging_destination, error));
    }

    if destination.exists() {
        let cleanup_result = cleanup_staging(&staging_destination);

        if marker_matches(destination, source_canonical, target.id) {
            if cleanup_result.is_ok() {
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

            return Err(cleanup_result.unwrap_err());
        }

        let error = format!(
            "Target folder already exists and is not managed by Skills Manage: {}",
            destination.display()
        );

        if let Err(cleanup_error) = cleanup_result {
            return Err(format!("{error} {cleanup_error}"));
        }

        return Err(error);
    }

    if let Err(error) = fs::rename(&staging_destination, destination) {
        return Err(cleanup_staging_after_failure(
            &staging_destination,
            format!("Could not move staged target folder into place: {error}"),
        ));
    }

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

    // A copy without SKILL.md is already switched off in that tool.
    if !marker_matches(destination, source_canonical, target.id) && !destination.join("SKILL.md").is_file() {
        return Ok(TargetToggleResult {
            target_id: target.id.to_string(),
            target_name: target.name.to_string(),
            skill_name: skill_name.to_string(),
            enabled: false,
            changed: false,
            target_path: display_path(destination),
            message: format!("{skill_name} is already disabled for {}.", target.name),
        });
    }

    // An identical copy the user placed there is safe to move to the trash:
    // the library still holds the same files.
    if !marker_matches(destination, source_canonical, target.id)
        && !same_skill_content(source_canonical, destination)
    {
        return Err(format!(
            "Target folder exists with different content and is not managed by Skills Manage: {}",
            display_path(destination)
        ));
    }

    let trash_path = unique_trash_path(data_root, &format!("{}-{skill_name}", target.id))?;
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
    let home = home_dir().ok_or_else(|| "Could not determine home directory.".to_string())?;

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

    fs::write(destination.join(LINK_MARKER_FILE), contents)
        .map_err(|error| format!("Could not write managed marker: {error}"))
}

fn marker_matches(target_dir: &Path, source_canonical: &Path, target_id: &str) -> bool {
    let marker_path = target_dir.join(LINK_MARKER_FILE);
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

/// Best-effort removal of staging folders a crashed enable left behind in a
/// target root. Only our own prefixed folders older than an hour are touched.
fn remove_stale_staging(target_root: &Path) {
    let Ok(entries) = fs::read_dir(target_root) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(STAGING_PREFIX) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age > STALE_STAGING_AGE);
        if stale {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

fn cleanup_staging_after_failure(staging_destination: &Path, error: String) -> String {
    match cleanup_staging(staging_destination) {
        Ok(()) => error,
        Err(cleanup_error) => format!("{error} {cleanup_error}"),
    }
}

fn cleanup_staging(staging_destination: &Path) -> Result<(), String> {
    if !staging_destination.exists() {
        return Ok(());
    }

    fs::remove_dir_all(staging_destination).map_err(|cleanup_error| {
        format!(
            "Cleanup failed. Also could not remove staged target folder {}: {cleanup_error}",
            staging_destination.display()
        )
    })
}

fn skill_folder_name(source_path: &Path) -> String {
    source_path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .map(|value| safe_folder_name(value, "managed-skill"))
        .unwrap_or_else(|| "managed-skill".to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveResult {
    pub skill_name: String,
    /// Tools whose managed copy was moved to the trash first.
    pub removed_from: Vec<String>,
    pub message: String,
}

/// After a tool's own skill folder was imported into the library, mark that
/// original folder as the tool's managed copy of the new library skill (it is
/// an identical copy). Without this, the tool keeps an unmanaged folder of the
/// same name and enabling the library skill for that tool would conflict.
///
/// Only adopts when the folder sits directly in a known tool root and already
/// has the name the managed copy would get; returns the adopting tool's name.
pub fn adopt_imported_original(source_dir: &Path, library_path: &Path) -> Option<&'static str> {
    toggleable_target_ids().iter().find_map(|id| {
        let root = target_root_for(id).ok()?;
        adopt_with_root(source_dir, library_path, id, &root)
            .ok()
            .flatten()
    })
}

fn adopt_with_root(
    source_dir: &Path,
    library_path: &Path,
    target_id: &str,
    target_root: &Path,
) -> Result<Option<&'static str>, String> {
    let target = target_profile_with_root(target_id, target_root)?;
    let (Ok(source), Ok(root), Ok(library)) = (
        source_dir.canonicalize(),
        target.root.canonicalize(),
        library_path.canonicalize(),
    ) else {
        return Ok(None);
    };

    let in_root = source.parent() == Some(root.as_path());
    let expected = root.join(skill_folder_name(&library));
    if !in_root || source != expected || source.join(LINK_MARKER_FILE).exists() {
        return Ok(None);
    }

    write_marker(&source, &library, &target)?;
    Ok(Some(target.name))
}

/// Validate that `library_path` is a skill folder inside the shared library.
pub fn validate_library_skill(library_path: &Path, data_root: &Path) -> Result<PathBuf, String> {
    validate_library_source(library_path, data_root)
}

/// Re-copy a library skill into every tool that has it enabled (after its
/// content changed). Returns the refreshed and the failed tool names.
pub fn refresh_managed_copies(
    library_path: &Path,
    data_root: &Path,
    target_roots: &[(&str, PathBuf)],
) -> (Vec<String>, Vec<String>) {
    let mut refreshed = Vec::new();
    let mut failed = Vec::new();
    for (id, root) in target_roots {
        let copy = managed_skill_dir(root, library_path);
        if !(copy.exists() && is_managed_target_copy(&copy, library_path, id)) {
            continue;
        }
        let name = target_name_for(id).unwrap_or(id).to_string();
        let result = set_skill_target_enabled_with_root(library_path, id, false, data_root, root)
            .and_then(|_| set_skill_target_enabled_with_root(library_path, id, true, data_root, root));
        match result {
            Ok(_) => refreshed.push(name),
            Err(_) => failed.push(name),
        }
    }
    (refreshed, failed)
}

pub fn remove_skill_from_library(library_path: String) -> Result<RemoveResult, String> {
    let data_root = default_data_root()?;
    let roots = toggleable_target_ids()
        .iter()
        .map(|id| target_root_for(id).map(|root| (*id, root)))
        .collect::<Result<Vec<_>, _>>()?;
    remove_skill_from_library_with_roots(Path::new(&library_path), &data_root, &roots)
}

/// Soft-delete a shared-library skill: every tool's managed copy goes to the
/// trash first (so no tool keeps a copy pointing at nothing), then the
/// library folder itself. Unmanaged folders are never touched.
pub fn remove_skill_from_library_with_roots(
    library_path: &Path,
    data_root: &Path,
    target_roots: &[(&str, PathBuf)],
) -> Result<RemoveResult, String> {
    let source_canonical = validate_library_source(library_path, data_root)?;
    let library_root = data_root
        .join("library")
        .canonicalize()
        .map_err(|error| format!("Could not read library root: {error}"))?;
    if source_canonical == library_root {
        return Err("Refusing to remove the library root itself.".to_string());
    }

    let skill_name = skill_folder_name(library_path);
    let mut removed_from = Vec::new();
    for (id, root) in target_roots {
        let copy = root.join(&skill_name);
        if copy.exists() && marker_matches(&copy, &source_canonical, id) {
            let result = set_skill_target_enabled_with_root(library_path, id, false, data_root, root)?;
            removed_from.push(result.target_name);
        }
    }

    let trash_path = unique_trash_path(data_root, &format!("library-{skill_name}"))?;
    fs::rename(&source_canonical, &trash_path)
        .map_err(|error| format!("Could not move the skill to trash: {error}"))?;

    Ok(RemoveResult {
        message: format!("Moved {skill_name} to trash."),
        skill_name,
        removed_from,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs_ops::test_temp_dir as unique_temp_dir;
    use std::fs;
    use std::path::PathBuf;

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
    fn enable_cleans_staged_copy_when_marker_write_fails() {
        let data_root = unique_temp_dir("data-root-target-marker-failure");
        let library_skill = create_library_skill(&data_root, "copy-me");
        let target_root = unique_temp_dir("codex-target-marker-failure");
        let target = target_profile_with_root("codex", &target_root).unwrap();
        let source_canonical = library_skill.canonicalize().unwrap();
        let destination = target_root.join("copy-me");

        let error = enable_target_copy_with_marker_writer(
            &source_canonical,
            &library_skill,
            &target,
            &destination,
            "copy-me",
            |_, _, _| Err("marker write failed".to_string()),
        )
        .unwrap_err();

        assert!(error.contains("marker write failed"));
        assert!(!destination.exists());
        assert_eq!(fs::read_dir(&target_root).unwrap().count(), 0);

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

    #[test]
    fn bulk_enables_multiple_library_skills() {
        let data_root = unique_temp_dir("data-root-bulk-enable");
        let first = create_library_skill(&data_root, "bulk-one");
        let second = create_library_skill(&data_root, "bulk-two");
        let target_root = unique_temp_dir("codex-bulk-enable");

        let result = set_skill_targets_bulk_with_root(
            vec![first.display().to_string(), second.display().to_string()],
            "codex",
            true,
            &data_root,
            &target_root,
        )
        .unwrap();

        assert_eq!(result.succeeded, 2);
        assert_eq!(result.failed, 0);
        assert!(target_root
            .join("bulk-one")
            .join(".skills-manage-link.json")
            .is_file());
        assert!(target_root
            .join("bulk-two")
            .join(".skills-manage-link.json")
            .is_file());

        fs::remove_dir_all(data_root).unwrap();
        fs::remove_dir_all(target_root).unwrap();
    }

    #[test]
    fn adopts_the_imported_original_as_the_tools_managed_copy() {
        let data_root = unique_temp_dir("data-root-adopt");
        let library_skill = create_library_skill(&data_root, "adopt-me");
        let target_root = unique_temp_dir("claude-adopt");
        fs::create_dir_all(target_root.join("adopt-me")).unwrap();
        fs::write(target_root.join("adopt-me").join("SKILL.md"), "# original
").unwrap();
        fs::create_dir_all(target_root.join("other-name")).unwrap();

        let adopted =
            adopt_with_root(&target_root.join("adopt-me"), &library_skill, "claude-code", &target_root)
                .unwrap();
        let mismatched =
            adopt_with_root(&target_root.join("other-name"), &library_skill, "claude-code", &target_root)
                .unwrap();

        assert_eq!(adopted, Some("Claude Code"));
        assert_eq!(mismatched, None);
        assert!(is_managed_target_copy(&target_root.join("adopt-me"), &library_skill, "claude-code"));
        // Disabling now works instead of reporting an unmanaged conflict.
        let disabled = set_skill_target_enabled_with_root(
            &library_skill,
            "claude-code",
            false,
            &data_root,
            &target_root,
        )
        .unwrap();
        assert!(disabled.changed);

        fs::remove_dir_all(data_root).unwrap();
        fs::remove_dir_all(target_root).unwrap();
    }

    #[test]
    fn identical_existing_folder_is_adopted_and_different_one_needs_replace() {
        let data_root = unique_temp_dir("data-root-existing");
        let library_skill = create_library_skill(&data_root, "existing");
        let codex_root = unique_temp_dir("codex-existing");
        let claude_root = unique_temp_dir("claude-existing");
        fs::create_dir_all(codex_root.join("existing")).unwrap();
        fs::copy(library_skill.join("SKILL.md"), codex_root.join("existing").join("SKILL.md")).unwrap();
        fs::create_dir_all(claude_root.join("existing")).unwrap();
        fs::write(claude_root.join("existing").join("SKILL.md"), "# mine\n").unwrap();

        let adopted =
            set_skill_target_enabled_with_root(&library_skill, "codex", true, &data_root, &codex_root).unwrap();
        assert!(adopted.changed);
        assert!(is_managed_target_copy(&codex_root.join("existing"), &library_skill, "codex"));

        let refused =
            set_skill_target_enabled_with_root(&library_skill, "claude-code", true, &data_root, &claude_root)
                .unwrap_err();
        assert!(refused.contains("different content"));
        assert_eq!(fs::read_to_string(claude_root.join("existing").join("SKILL.md")).unwrap(), "# mine\n");

        set_skill_target_enabled_with_options(&library_skill, "claude-code", true, &data_root, &claude_root, true)
            .unwrap();
        assert!(is_managed_target_copy(&claude_root.join("existing"), &library_skill, "claude-code"));
        let trashed = fs::read_dir(data_root.join("trash"))
            .unwrap()
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().contains("replaced"));
        assert!(trashed, "the user's folder should be in the trash, not deleted");

        fs::remove_dir_all(data_root).unwrap();
        fs::remove_dir_all(codex_root).unwrap();
        fs::remove_dir_all(claude_root).unwrap();
    }

    #[test]
    fn a_copy_switched_off_in_the_tool_is_set_aside_when_enabling() {
        let data_root = unique_temp_dir("data-root-disabled-copy");
        let library_skill = create_library_skill(&data_root, "paused");
        let codex_root = unique_temp_dir("codex-disabled-copy");
        fs::create_dir_all(codex_root.join("paused")).unwrap();
        fs::write(codex_root.join("paused").join("SKILL.md.disabled"), "# old").unwrap();

        // Disabling a switched-off copy is a no-op; enabling replaces it.
        let disabled =
            set_skill_target_enabled_with_root(&library_skill, "codex", false, &data_root, &codex_root).unwrap();
        assert!(!disabled.changed);
        set_skill_target_enabled_with_root(&library_skill, "codex", true, &data_root, &codex_root).unwrap();

        assert!(codex_root.join("paused").join("SKILL.md").is_file());
        assert!(is_managed_target_copy(&codex_root.join("paused"), &library_skill, "codex"));
        let set_aside = fs::read_dir(data_root.join("trash"))
            .unwrap()
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().contains("disabled"));
        assert!(set_aside);

        fs::remove_dir_all(data_root).unwrap();
        fs::remove_dir_all(codex_root).unwrap();
    }

    #[test]
    fn removing_from_library_trashes_managed_copies_but_not_user_folders() {
        let data_root = unique_temp_dir("data-root-remove");
        let library_skill = create_library_skill(&data_root, "remove-me");
        let codex_root = unique_temp_dir("codex-remove");
        let claude_root = unique_temp_dir("claude-remove");
        set_skill_target_enabled_with_root(&library_skill, "codex", true, &data_root, &codex_root)
            .unwrap();
        fs::create_dir_all(claude_root.join("remove-me")).unwrap();
        fs::write(claude_root.join("remove-me").join("SKILL.md"), "# user
").unwrap();

        let result = remove_skill_from_library_with_roots(
            &library_skill,
            &data_root,
            &[("codex", codex_root.clone()), ("claude-code", claude_root.clone())],
        )
        .unwrap();

        assert_eq!(result.removed_from, vec!["Codex".to_string()]);
        assert!(!library_skill.exists());
        assert!(!codex_root.join("remove-me").exists());
        assert!(claude_root.join("remove-me").join("SKILL.md").is_file());
        assert_eq!(fs::read_dir(data_root.join("trash")).unwrap().count(), 2);

        fs::remove_dir_all(data_root).unwrap();
        fs::remove_dir_all(codex_root).unwrap();
        fs::remove_dir_all(claude_root).unwrap();
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
}
