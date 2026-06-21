//! Shared filesystem helpers used by the library, targets, scanner, and pack
//! modules. Consolidated here so the data-root resolution, name sanitization,
//! and the exclusion list have a single source of truth.

use std::fs;
use std::path::{Path, PathBuf};
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

pub(crate) fn safe_folder_name(value: &str, fallback: &str) -> String {
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
        fallback.to_string()
    } else {
        sanitized
    }
}

/// Recursively copy `source` into `destination`, skipping any entry for which
/// `excluded` returns true. The predicate is applied to each entry's file name.
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

        if excluded(&name_text) {
            continue;
        }

        let source_path = entry.path();
        let destination_path = destination.join(&name);

        if source_path.is_dir() {
            copy_dir_excluding(&source_path, &destination_path, excluded)?;
        } else if source_path.is_file() {
            fs::copy(&source_path, &destination_path).map_err(|error| {
                format!("Could not copy file {}: {error}", source_path.display())
            })?;
        }
    }

    Ok(())
}

/// Copy a skill directory, skipping the standard excluded entries.
pub(crate) fn copy_skill_dir(source: &Path, destination: &Path) -> Result<(), String> {
    copy_dir_excluding(source, destination, is_excluded_entry)
}
