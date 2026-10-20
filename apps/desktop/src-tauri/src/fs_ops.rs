//! Shared filesystem helpers used by the library, targets, scanner, and pack
//! modules. Consolidated here so the data-root resolution, name sanitization,
//! and the exclusion list have a single source of truth.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Entries never copied into the library, a target folder, or a `.skillpack`.
const EXCLUDED_ENTRIES: [&str; 7] = [
    ".git",
    ".env",
    "node_modules",
    "dist",
    "target",
    "cache",
    ".cache",
];

/// Marker file written inside a managed target copy (records its library source).
pub(crate) const LINK_MARKER_FILE: &str = ".skills-manage-link.json";
/// Marker file written inside a GitHub-sourced library skill (records its origin).
pub(crate) const SOURCE_MARKER_FILE: &str = ".skills-manage-source.json";

/// Device names Windows reserves regardless of extension.
const WINDOWS_RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];
const MAX_FOLDER_NAME_CHARS: usize = 120;

pub(crate) fn home_dir() -> Option<PathBuf> {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
        .map(PathBuf::from)
}

pub(crate) fn default_data_root() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("SKILLS_MANAGE_DATA_ROOT") {
        return Ok(PathBuf::from(path));
    }

    home_dir()
        .map(|home| home.join(".skills-manage"))
        .ok_or_else(|| "Could not determine home directory.".to_string())
}

pub(crate) fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub(crate) fn is_excluded_entry(name: &str) -> bool {
    EXCLUDED_ENTRIES.contains(&name)
}

pub(crate) fn excluded_entries() -> &'static [&'static str] {
    &EXCLUDED_ENTRIES
}

/// Internal bookkeeping files that must never be copied into the library, a
/// target folder, or a `.skillpack` (they are per-machine and would otherwise
/// leak provenance/links across copies and exports).
pub(crate) fn is_internal_marker(name: &str) -> bool {
    name == LINK_MARKER_FILE || name == SOURCE_MARKER_FILE
}

/// True when `name` is a relative path with no `..`/root/prefix components, i.e.
/// safe to join onto a destination root when extracting an archive.
pub(crate) fn is_safe_relative(name: &str) -> bool {
    let path = Path::new(name);
    path.is_relative()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

/// Turn an arbitrary skill name into a folder name that is valid on Windows.
/// Unicode letters and digits (e.g. Chinese names) are kept along with `-`,
/// `_` and `.`; everything else (whitespace, Windows-invalid characters such as
/// `<>:"/\|?*`, control characters, other symbols) collapses to a single `-`.
pub(crate) fn safe_folder_name(value: &str, fallback: &str) -> String {
    let mut collapsed = String::with_capacity(value.len());
    for character in value.chars() {
        let keep = character.is_alphanumeric() || matches!(character, '-' | '_' | '.');
        let mapped = if keep { character } else { '-' };
        if mapped == '-' && collapsed.ends_with('-') {
            continue;
        }
        collapsed.push(mapped);
    }

    let truncated: String = collapsed.chars().take(MAX_FOLDER_NAME_CHARS).collect();
    // Windows strips trailing dots; a leading dot would hide the folder.
    let trimmed = truncated.trim_matches(|character| character == '-' || character == '.');
    if trimmed.is_empty() {
        return fallback.to_string();
    }

    let stem = trimmed.split('.').next().unwrap_or("").to_ascii_uppercase();
    if WINDOWS_RESERVED_NAMES.contains(&stem.as_str()) {
        format!("{trimmed}-skill")
    } else {
        trimmed.to_string()
    }
}

/// Short, stable (FNV-1a) hex hash used to make ids unique without new deps.
pub(crate) fn short_hash(value: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{:08x}", hash & 0xffff_ffff)
}

/// Return `<parent>/<base>-<unix_ms>[-n]` that does not exist yet. Used for
/// staging, trash, and other app-owned scratch folders.
pub(crate) fn unique_timestamped_path(parent: &Path, base: &str) -> PathBuf {
    let stamp = unix_ms();
    let mut index = 0usize;
    loop {
        let suffix = if index == 0 {
            String::new()
        } else {
            format!("-{index}")
        };
        let candidate = parent.join(format!("{base}-{stamp}{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
        index += 1;
    }
}

/// True for symlinks and (on Windows) junctions / other name-surrogate reparse
/// points. `FileType` from `DirEntry` does not follow links.
pub(crate) fn is_link(entry: &fs::DirEntry) -> bool {
    entry
        .file_type()
        .map(|file_type| file_type.is_symlink())
        .unwrap_or(true)
}

/// Recursively copy `source` into `destination`, skipping any entry for which
/// `excluded` returns true. The predicate is applied to each entry's file name.
/// Symlinks and junctions are never followed: they could loop forever or pull
/// in files from outside the skill folder.
pub(crate) fn copy_dir_excluding<F>(
    source: &Path,
    destination: &Path,
    excluded: F,
) -> Result<(), String>
where
    F: Fn(&str) -> bool + Copy,
{
    fs::create_dir_all(destination)
        .map_err(|error| format!("Could not create destination: {error}"))?;

    for entry in fs::read_dir(source).map_err(|error| format!("Could not read source: {error}"))? {
        let entry = entry.map_err(|error| format!("Could not read source entry: {error}"))?;
        let name = entry.file_name();
        let name_text = name.to_string_lossy();

        if excluded(&name_text) || is_link(&entry) {
            continue;
        }

        let source_path = entry.path();
        let destination_path = destination.join(&name);
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Could not read {}: {error}", source_path.display()))?;

        if file_type.is_dir() {
            copy_dir_excluding(&source_path, &destination_path, excluded)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path).map_err(|error| {
                format!("Could not copy file {}: {error}", source_path.display())
            })?;
        }
    }

    Ok(())
}

/// True when two skill folders hold the same files with the same bytes
/// (ignoring Skills Manage's own markers, excluded entries and links). Used to
/// recognize a tool's existing folder as an identical copy of a library skill.
pub(crate) fn same_skill_content(left: &Path, right: &Path) -> bool {
    fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) -> bool {
        let Ok(entries) = fs::read_dir(dir) else {
            return false;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if is_excluded_entry(&name) || is_internal_marker(&name) || is_link(&entry) {
                continue;
            }
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => {
                    if !collect(root, &path, out) {
                        return false;
                    }
                }
                Ok(kind) if kind.is_file() => {
                    let Ok(bytes) = fs::read(&path) else {
                        return false;
                    };
                    let relative = path
                        .strip_prefix(root)
                        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
                        .unwrap_or(name);
                    out.push((relative, bytes));
                }
                _ => {}
            }
        }
        true
    }

    let (mut a, mut b) = (Vec::new(), Vec::new());
    if !collect(left, left, &mut a) || !collect(right, right, &mut b) {
        return false;
    }
    a.sort();
    b.sort();
    a == b
}

/// A path for display: without Windows' `\\?\` verbatim prefix.
pub(crate) fn display_path(path: &Path) -> String {
    let text = path.display().to_string();
    text.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(text)
}

/// Copy a skill directory, skipping the standard excluded entries and the
/// internal marker files (which are per-machine and must not be carried along).
pub(crate) fn copy_skill_dir(source: &Path, destination: &Path) -> Result<(), String> {
    copy_dir_excluding(source, destination, |name| {
        is_excluded_entry(name) || is_internal_marker(name)
    })
}

/// Unique scratch directory for tests (shared by every module's test suite).
#[cfg(test)]
pub(crate) fn test_temp_dir(prefix: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("skills-manage-{prefix}-{stamp}-{count}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_folder_name_keeps_unicode_letters() {
        assert_eq!(safe_folder_name("代码审查", "fallback"), "代码审查");
        assert_eq!(safe_folder_name("my skill: v2", "fallback"), "my-skill-v2");
        assert_eq!(safe_folder_name("a<>b|c", "fallback"), "a-b-c");
    }

    #[test]
    fn safe_folder_name_avoids_empty_hidden_and_reserved_names() {
        assert_eq!(safe_folder_name("???", "fallback"), "fallback");
        assert_eq!(safe_folder_name(".hidden.", "fallback"), "hidden");
        assert_eq!(safe_folder_name("con", "fallback"), "con-skill");
        assert_eq!(safe_folder_name("NUL.txt", "fallback"), "NUL.txt-skill");
        assert_eq!(safe_folder_name("console", "fallback"), "console");
    }

    #[test]
    fn short_hash_is_stable_and_distinguishes_inputs() {
        assert_eq!(short_hash("a"), short_hash("a"));
        assert_ne!(short_hash("a"), short_hash("b"));
        assert_eq!(short_hash("a").len(), 8);
    }

    #[test]
    fn unique_timestamped_path_skips_existing() {
        let root = test_temp_dir("unique-path");
        fs::create_dir_all(&root).unwrap();
        let first = unique_timestamped_path(&root, "item");
        fs::create_dir_all(&first).unwrap();
        let second = unique_timestamped_path(&root, "item");

        assert_ne!(first, second);
        assert!(!second.exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn copy_skips_junctions() {
        let root = test_temp_dir("copy-junction");
        let source = root.join("source");
        let outside = root.join("outside");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(source.join("SKILL.md"), "# Skill\n").unwrap();
        fs::write(outside.join("secret.txt"), "secret").unwrap();
        let junction = source.join("linked");
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(status.status.success());
        // A junction pointing back at its own parent would loop forever if followed.
        let looped = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(source.join("loop"))
            .arg(&source)
            .output()
            .unwrap();
        assert!(looped.status.success());

        let destination = root.join("destination");
        copy_skill_dir(&source, &destination).unwrap();

        assert!(destination.join("SKILL.md").is_file());
        assert!(!destination.join("linked").exists());
        assert!(!destination.join("loop").exists());

        fs::remove_dir(&junction).unwrap();
        fs::remove_dir(source.join("loop")).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
