//! Per-tool versions of a GitHub skill. Some repositories ship one skill in
//! several tool folders (`skills/x`, `.agents/skills/x`, `.claude/skills/x` …)
//! with different content, each written for its tool. When a skill is added
//! from such a repository, the version the user picked goes to the library as
//! usual and every other tool's own version is kept beside it, in
//! `<data root>/variants/<library folder>/<tool id>/`. Enabling the skill in a
//! tool then copies that tool's version, if there is one.
//!
//! Left out on purpose: copies with the same content (just duplicates), wrapper
//! SKILL.md files that only point at another file in the repository (useless
//! once copied on their own), and folders outside a tool's skills folder
//! (`examples/`, `template/` …), which belong to no tool.

use crate::fs_ops::{unique_timestamped_path, SOURCE_MARKER_FILE};
use crate::github::extract_subdir_from_zip;
use crate::trash::unique_trash_path;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

pub const VARIANTS_DIR: &str = "variants";
const STAGING_PREFIX: &str = ".staging";
const MAX_ENTRIES: usize = 10_000;
/// Only candidate folders are read; this caps what they may add up to.
const MAX_CANDIDATE_BYTES: u64 = 50 * 1024 * 1024;

/// Where a tool keeps skills inside a repository: (prefix, tool id, rank).
/// For the same tool a lower rank wins: Codex reads `.agents/skills` too, but
/// a `.codex*` folder is written for Codex alone.
const TOOL_PREFIXES: [(&str, &str, u8); 9] = [
    (".claude/skills/", "claude-code", 0),
    (".codex/skills/", "codex", 0),
    (".codex-plugin/skills/", "codex", 0),
    (".agents/skills/", "codex", 1),
    (".gemini/skills/", "gemini-cli", 0),
    (".opencode/skills/", "opencode", 0),
    (".qwen/skills/", "qwen-code", 0),
    (".qoder/skills/", "qoder", 0),
    (".codebuddy/skills/", "codebuddy", 0),
];

/// A tool that has its own version of a library skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantInfo {
    pub target_id: String,
    /// The folder in the repository it came from, e.g. `.agents/skills/dev`.
    pub path: String,
}

/// What an update found: the tools' versions now in the repository (tool id ->
/// folder) and, when there are any, the staging folder holding them.
pub struct VariantUpdate {
    pub found: BTreeMap<String, String>,
    pub staged: Option<PathBuf>,
}

fn tool_for_dir(dir: &str) -> Option<(&'static str, u8)> {
    let with_slash = format!("{dir}/");
    TOOL_PREFIXES
        .iter()
        .find(|(prefix, _, _)| with_slash.starts_with(prefix))
        .map(|(_, tool, rank)| (*tool, *rank))
}

/// Other tools' versions of the skill at `subdir` in a repository archive, as
/// (tool id, folder relative to the repository root), sorted by tool.
pub fn detect_in_zip(zip_bytes: &[u8], subdir: &str) -> Result<Vec<(String, String)>, String> {
    let subdir = subdir.trim_matches('/');
    let Some(name) = subdir.rsplit('/').next().filter(|name| !name.is_empty()) else {
        return Ok(Vec::new());
    };
    let mut archive =
        ZipArchive::new(Cursor::new(zip_bytes)).map_err(|error| format!("Could not read the archive: {error}"))?;
    if archive.is_empty() || archive.len() > MAX_ENTRIES {
        return Ok(Vec::new());
    }
    let names: Vec<String> = archive.file_names().map(str::to_string).collect();
    let Some(top) = names.first().and_then(|first| first.split('/').next()).map(str::to_string) else {
        return Ok(Vec::new());
    };
    let own_tool = tool_for_dir(subdir).map(|(tool, _)| tool);

    // Folders with a SKILL.md, the same name, inside some tool's skills folder.
    let mut candidates: Vec<(String, &'static str, u8)> = names
        .iter()
        .filter_map(|entry| entry.strip_prefix(&format!("{top}/"))?.strip_suffix("/SKILL.md").map(str::to_string))
        .filter(|dir| dir != subdir && dir.rsplit('/').next() == Some(name))
        .filter_map(|dir| {
            let (tool, rank) = tool_for_dir(&dir)?;
            (Some(tool) != own_tool).then_some((dir, tool, rank))
        })
        .collect();
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    candidates.sort();

    let mut dirs: Vec<String> = candidates.iter().map(|(dir, _, _)| dir.clone()).collect();
    dirs.push(subdir.to_string());
    let contents = read_dirs(&mut archive, &top, &dirs)?;
    let empty = BTreeMap::new();
    let picked = contents.get(subdir).unwrap_or(&empty);

    let mut best: BTreeMap<&'static str, (u8, String)> = BTreeMap::new();
    for (dir, tool, rank) in candidates {
        let Some(files) = contents.get(&dir) else {
            continue;
        };
        if files == picked {
            continue;
        }
        let skill_md = files.get("SKILL.md").map(|bytes| String::from_utf8_lossy(bytes).to_string()).unwrap_or_default();
        if crate::compat::wrapper_reference(&skill_md, |reference| files.contains_key(reference)).is_some() {
            continue;
        }
        let better = best.get(tool).is_none_or(|(known_rank, _)| rank < *known_rank);
        if better {
            best.insert(tool, (rank, dir));
        }
    }
    Ok(best.into_iter().map(|(tool, (_, dir))| (tool.to_string(), dir)).collect())
}

type DirFiles = BTreeMap<String, Vec<u8>>;

/// The files (path relative to the folder -> bytes) of each folder in `dirs`.
fn read_dirs(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    top: &str,
    dirs: &[String],
) -> Result<BTreeMap<String, DirFiles>, String> {
    let prefixes: Vec<(String, &String)> = dirs.iter().map(|dir| (format!("{top}/{dir}/"), dir)).collect();
    let mut contents: BTreeMap<String, DirFiles> = BTreeMap::new();
    let mut total: u64 = 0;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| format!("Could not read the archive: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        let entry_name = entry.name().to_string();
        // A folder can sit inside another candidate; the longest prefix owns the file.
        let Some((prefix, dir)) = prefixes
            .iter()
            .filter(|(prefix, _)| entry_name.starts_with(prefix.as_str()))
            .max_by_key(|(prefix, _)| prefix.len())
        else {
            continue;
        };
        total = total.saturating_add(entry.size());
        if total > MAX_CANDIDATE_BYTES {
            return Err("The tool versions in this repository are too large to compare.".to_string());
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|error| format!("Could not read {entry_name}: {error}"))?;
        contents
            .entry((*dir).clone())
            .or_default()
            .insert(entry_name[prefix.len()..].to_string(), bytes);
    }
    Ok(contents)
}

/// Extract the found versions into one staging folder (`<staging>/<tool id>`).
/// `None` when nothing was found.
pub fn stage(zip_bytes: &[u8], found: &[(String, String)], data_root: &Path) -> Result<Option<PathBuf>, String> {
    if found.is_empty() {
        return Ok(None);
    }
    let root = data_root.join(VARIANTS_DIR);
    fs::create_dir_all(&root).map_err(|error| format!("Could not create the versions folder: {error}"))?;
    let staging = unique_timestamped_path(&root, STAGING_PREFIX);
    for (tool, dir) in found {
        if let Err(error) = extract_subdir_from_zip(zip_bytes, dir, &staging.join(tool)) {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    }
    Ok(Some(staging))
}

/// The folder holding a library skill's tool versions.
pub fn variants_dir(data_root: &Path, library_folder: &str) -> PathBuf {
    data_root.join(VARIANTS_DIR).join(library_folder)
}

/// Make `staged` the skill's tool versions: the current ones (if any) go to
/// the trash first. `None` just moves the current ones to the trash.
pub fn put_in_place(data_root: &Path, library_folder: &str, staged: Option<&Path>) -> Result<(), String> {
    trash_variants(data_root, library_folder)?;
    if let Some(staged) = staged {
        fs::rename(staged, variants_dir(data_root, library_folder))
            .map_err(|error| format!("Could not put the tool versions in place: {error}"))?;
    }
    Ok(())
}

/// Move a skill's tool versions to the trash (when it leaves the library).
pub fn trash_variants(data_root: &Path, library_folder: &str) -> Result<(), String> {
    let current = variants_dir(data_root, library_folder);
    if !current.exists() {
        return Ok(());
    }
    let trash = unique_trash_path(data_root, &format!("variants-{library_folder}"))?;
    fs::rename(&current, trash).map_err(|error| format!("Could not move the old tool versions to trash: {error}"))
}

/// A library skill's version folder for one tool, when it has one. The data
/// root is the library's parent, so tests with their own roots stay isolated.
pub fn variant_for(library_skill_dir: &Path, target_id: &str) -> Option<PathBuf> {
    let folder = library_skill_dir.file_name()?;
    let data_root = library_skill_dir.parent()?.parent()?;
    let dir = data_root.join(VARIANTS_DIR).join(folder).join(target_id);
    dir.join("SKILL.md").is_file().then_some(dir)
}

/// What a tool gets when the skill is enabled there: its own version, or the
/// library skill itself.
pub fn effective_source(library_skill_dir: &Path, target_id: &str) -> PathBuf {
    variant_for(library_skill_dir, target_id).unwrap_or_else(|| library_skill_dir.to_path_buf())
}

/// Every version folder of a library skill (to keep edits in step).
pub fn variant_dirs(library_skill_dir: &Path) -> Vec<PathBuf> {
    crate::targets::toggleable_target_ids()
        .iter()
        .filter_map(|id| variant_for(library_skill_dir, id))
        .collect()
}

/// The tools with their own version of a library skill, with the repository
/// folder each came from (from the skill's source file).
pub fn list(library_skill_dir: &Path) -> Vec<VariantInfo> {
    let labels = recorded_variants(library_skill_dir);
    crate::targets::toggleable_target_ids()
        .iter()
        .filter(|id| variant_for(library_skill_dir, id).is_some())
        .map(|id| VariantInfo {
            target_id: id.to_string(),
            path: labels.get(*id).cloned().unwrap_or_default(),
        })
        .collect()
}

/// The repository folder a tool's version came from, when it has one.
pub fn variant_label(library_skill_dir: &Path, target_id: &str) -> Option<String> {
    variant_for(library_skill_dir, target_id)?;
    recorded_variants(library_skill_dir).remove(target_id)
}

fn recorded_variants(library_skill_dir: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(library_skill_dir.join(SOURCE_MARKER_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| serde_json::from_value(value.get("variants")?.clone()).ok())
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::fs_ops::test_temp_dir;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    pub(crate) fn build_zip(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut buffer = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buffer));
            for (name, content) in entries {
                zip.start_file(format!("repo-abc123/{name}"), SimpleFileOptions::default()).unwrap();
                zip.write_all(content.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        buffer
    }

    const FULL: &str = "---\nname: dev\ndescription: Dev cycle.\n---\n\n# Dev\n\nRun the tests, then lint, then ship. This is the full guide with every step spelled out.\n";

    #[test]
    fn a_generated_codex_version_beats_the_generic_agents_folder() {
        let zip = build_zip(&[
            ("skills/autopilot/SKILL.md", "---\nname: autopilot\n---\n\nDefault version body.\n"),
            ("skills/autopilot/scripts/run.sh", "echo run\n"),
            (".agents/skills/autopilot/SKILL.md", "---\nname: autopilot\n---\n\nAgents version body.\n"),
            (".codex-plugin/skills/autopilot/SKILL.md", "---\nname: autopilot\n---\n\nGenerated for Codex.\n"),
            (".cursor/skills/autopilot/SKILL.md", "---\nname: autopilot\n---\n\nRead `skills/autopilot/SKILL.md`.\n"),
            (".claude/skills/autopilot/SKILL.md", "---\nname: autopilot\n---\n\nCanonical skill: `skills/autopilot/SKILL.md`\n"),
        ]);
        let found = detect_in_zip(&zip, "skills/autopilot").unwrap();
        assert_eq!(found, vec![("codex".to_string(), ".codex-plugin/skills/autopilot".to_string())]);
    }

    #[test]
    fn wrappers_identical_copies_and_non_tool_folders_are_not_versions() {
        let zip = build_zip(&[
            (".agents/skills/dev/SKILL.md", FULL),
            (".claude/skills/dev/SKILL.md", "---\nname: dev\n---\n\nRead `.agents/skills/dev/SKILL.md` for full instructions.\n"),
            ("examples/skills/dev/SKILL.md", "---\nname: dev\n---\n\nAn older example.\n"),
            ("cli/assets/skills/brand/SKILL.md", "brand"),
            (".claude/skills/brand/SKILL.md", "brand"),
        ]);
        assert!(detect_in_zip(&zip, ".agents/skills/dev").unwrap().is_empty());
        assert!(detect_in_zip(&zip, "cli/assets/skills/brand").unwrap().is_empty());
        assert!(detect_in_zip(&zip, "").unwrap().is_empty());
    }

    #[test]
    fn a_claude_version_is_found_for_a_generic_skill() {
        let zip = build_zip(&[
            (".agents/skills/dev/SKILL.md", FULL),
            (".claude/skills/dev/SKILL.md", "---\nname: dev\n---\n\nUse TodoWrite to track each step of the cycle, then ship.\n"),
        ]);
        let found = detect_in_zip(&zip, ".agents/skills/dev").unwrap();
        assert_eq!(found, vec![("claude-code".to_string(), ".claude/skills/dev".to_string())]);
    }

    #[test]
    fn staged_versions_become_what_each_tool_gets() {
        let data_root = test_temp_dir("variants-place");
        let library_skill = data_root.join("library").join("dev");
        fs::create_dir_all(&library_skill).unwrap();
        fs::write(library_skill.join("SKILL.md"), FULL).unwrap();
        fs::write(
            library_skill.join(SOURCE_MARKER_FILE),
            r#"{"variants":{"claude-code":".claude/skills/dev"}}"#,
        )
        .unwrap();
        let zip = build_zip(&[(".claude/skills/dev/SKILL.md", "claude version"), (".agents/skills/dev/SKILL.md", FULL)]);

        let found = vec![("claude-code".to_string(), ".claude/skills/dev".to_string())];
        let staged = stage(&zip, &found, &data_root).unwrap().unwrap();
        put_in_place(&data_root, "dev", Some(&staged)).unwrap();

        let claude = effective_source(&library_skill, "claude-code");
        assert_eq!(fs::read_to_string(claude.join("SKILL.md")).unwrap(), "claude version");
        assert_eq!(effective_source(&library_skill, "codex"), library_skill);
        assert_eq!(
            list(&library_skill),
            vec![VariantInfo { target_id: "claude-code".to_string(), path: ".claude/skills/dev".to_string() }]
        );
        assert_eq!(variant_label(&library_skill, "claude-code").as_deref(), Some(".claude/skills/dev"));
        assert_eq!(variant_label(&library_skill, "codex"), None);

        // The repository dropped its Claude version: back to the library skill.
        put_in_place(&data_root, "dev", None).unwrap();
        assert_eq!(effective_source(&library_skill, "claude-code"), library_skill);
        assert!(list(&library_skill).is_empty());
        let _ = fs::remove_dir_all(data_root);
    }
}
