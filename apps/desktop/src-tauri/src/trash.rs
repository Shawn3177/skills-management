//! The app's soft-delete area (`<data root>/trash`). Disabling a target copy or
//! updating a library skill moves the old folder here instead of deleting it.
//! Entries are only ever removed permanently through an explicit, confirmed
//! purge from the Settings screen.

use crate::fs_ops::{default_data_root, unique_timestamped_path, unix_ms};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

const DAY_MS: u128 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashSummary {
    pub path: String,
    pub entries: usize,
    pub total_bytes: u64,
    /// Entries older than the requested age (what a purge would remove).
    pub purgeable_entries: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurgeResult {
    pub removed: usize,
    pub failed: usize,
}

pub(crate) fn trash_root(data_root: &Path) -> PathBuf {
    data_root.join("trash")
}

/// A fresh, not-yet-existing path inside the trash for `label`.
pub(crate) fn unique_trash_path(data_root: &Path, label: &str) -> Result<PathBuf, String> {
    let root = trash_root(data_root);
    fs::create_dir_all(&root).map_err(|error| format!("Could not create trash root: {error}"))?;
    Ok(unique_timestamped_path(&root, label))
}

pub fn trash_summary(older_than_days: u32) -> Result<TrashSummary, String> {
    trash_summary_with_root(&default_data_root()?, older_than_days, unix_ms())
}

pub fn purge_trash(older_than_days: u32) -> Result<PurgeResult, String> {
    purge_trash_with_root(&default_data_root()?, older_than_days, unix_ms())
}

fn trash_summary_with_root(
    data_root: &Path,
    older_than_days: u32,
    now_ms: u128,
) -> Result<TrashSummary, String> {
    let root = trash_root(data_root);
    let mut summary = TrashSummary {
        path: root.display().to_string(),
        entries: 0,
        total_bytes: 0,
        purgeable_entries: 0,
    };
    let Ok(entries) = fs::read_dir(&root) else {
        return Ok(summary);
    };

    let cutoff = cutoff_ms(older_than_days, now_ms);
    for entry in entries.flatten() {
        summary.entries += 1;
        summary.total_bytes += size_on_disk(&entry.path());
        if is_older_than(&entry.file_name().to_string_lossy(), cutoff) {
            summary.purgeable_entries += 1;
        }
    }

    Ok(summary)
}

fn purge_trash_with_root(
    data_root: &Path,
    older_than_days: u32,
    now_ms: u128,
) -> Result<PurgeResult, String> {
    let root = trash_root(data_root);
    let mut result = PurgeResult {
        removed: 0,
        failed: 0,
    };
    let Ok(entries) = fs::read_dir(&root) else {
        return Ok(result);
    };

    let cutoff = cutoff_ms(older_than_days, now_ms);
    for entry in entries.flatten() {
        if !is_older_than(&entry.file_name().to_string_lossy(), cutoff) {
            continue;
        }
        let path = entry.path();
        // `remove_dir_all` does not follow symlinks/junctions, so nothing
        // outside the trash folder can be removed through a link.
        let removed = match entry.file_type() {
            Ok(file_type) if file_type.is_dir() => fs::remove_dir_all(&path),
            Ok(file_type) if file_type.is_symlink() => {
                fs::remove_dir(&path).or_else(|_| fs::remove_file(&path))
            }
            _ => fs::remove_file(&path),
        };
        match removed {
            Ok(()) => result.removed += 1,
            Err(_) => result.failed += 1,
        }
    }

    Ok(result)
}

fn cutoff_ms(older_than_days: u32, now_ms: u128) -> u128 {
    now_ms.saturating_sub(u128::from(older_than_days) * DAY_MS)
}

/// Trash entries are named `<label>-<unix ms>[-n]`; the timestamp is the last
/// 13-digit segment. Entries without one are never purged automatically.
fn is_older_than(name: &str, cutoff_ms: u128) -> bool {
    trashed_at_ms(name).is_some_and(|stamp| stamp < cutoff_ms)
}

fn trashed_at_ms(name: &str) -> Option<u128> {
    name.rsplit('-')
        .find(|segment| segment.len() == 13 && segment.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|segment| segment.parse().ok())
}

fn size_on_disk(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    if !metadata.is_dir() {
        return 0;
    }
    fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| size_on_disk(&entry.path()))
                .sum()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs_ops::test_temp_dir;

    const NOW: u128 = 1_800_000_000_000;

    #[test]
    fn parses_timestamp_from_old_and_new_names() {
        assert_eq!(trashed_at_ms("codex-my-skill-1718000000000"), Some(1_718_000_000_000));
        assert_eq!(trashed_at_ms("library-x-1718000000000-2"), Some(1_718_000_000_000));
        assert_eq!(trashed_at_ms("no-stamp-here"), None);
    }

    #[test]
    fn purges_only_entries_older_than_cutoff() {
        let data_root = test_temp_dir("trash-purge");
        let trash = trash_root(&data_root);
        let old = trash.join(format!("codex-old-{}", NOW - 40 * DAY_MS));
        let fresh = trash.join(format!("codex-fresh-{}", NOW - DAY_MS));
        let unknown = trash.join("hand-made-folder");
        for dir in [&old, &fresh, &unknown] {
            fs::create_dir_all(dir).unwrap();
            fs::write(dir.join("SKILL.md"), "# x\n").unwrap();
        }

        let summary = trash_summary_with_root(&data_root, 30, NOW).unwrap();
        assert_eq!(summary.entries, 3);
        assert_eq!(summary.purgeable_entries, 1);
        assert!(summary.total_bytes > 0);

        let result = purge_trash_with_root(&data_root, 30, NOW).unwrap();
        assert_eq!(result, PurgeResult { removed: 1, failed: 0 });
        assert!(!old.exists());
        assert!(fresh.exists());
        assert!(unknown.exists());

        fs::remove_dir_all(data_root).unwrap();
    }

    #[test]
    fn missing_trash_is_empty() {
        let data_root = test_temp_dir("trash-missing");
        let summary = trash_summary_with_root(&data_root, 30, NOW).unwrap();
        assert_eq!(summary.entries, 0);
        assert_eq!(
            purge_trash_with_root(&data_root, 30, NOW).unwrap(),
            PurgeResult { removed: 0, failed: 0 }
        );
    }
}
