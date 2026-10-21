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
//!
//! The user can still pick the library skill for a tool that has its own
//! version; that choice is kept in `<data root>/variant-prefs.json`.

use crate::fs_ops::{unique_timestamped_path, SOURCE_MARKER_FILE};
use crate::github::extract_subdir_from_zip;
use crate::trash::unique_trash_path;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

pub const VARIANTS_DIR: &str = "variants";
const PREFS_FILE: &str = "variant-prefs.json";
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

/// Which version a tool with its own version gets: that one (the default)
/// or the library skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VariantChoice {
    Variant,
    Default,
}

/// A tool that has its own version of a library skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantInfo {
    pub target_id: String,
    /// The folder in the repository it came from, e.g. `.agents/skills/dev`.
    pub path: String,
    pub choice: VariantChoice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantChoiceResult {
    pub target_id: String,
    pub choice: VariantChoice,
    /// The skill was enabled in that tool, so its copy was replaced.
    pub refreshed: bool,
    pub message: String,
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

/// A library skill's version folder for one tool, when it has one (whether
/// the tool uses it or not). The data root is the library's parent, so tests
/// with their own roots stay isolated.
pub fn variant_for(library_skill_dir: &Path, target_id: &str) -> Option<PathBuf> {
    let folder = library_skill_dir.file_name()?;
    let data_root = data_root_of(library_skill_dir)?;
    let dir = data_root.join(VARIANTS_DIR).join(folder).join(target_id);
    dir.join("SKILL.md").is_file().then_some(dir)
}

/// The tool's own version, unless the user picked the library skill there.
pub fn chosen_variant(library_skill_dir: &Path, target_id: &str) -> Option<PathBuf> {
    let dir = variant_for(library_skill_dir, target_id)?;
    (choice_for(library_skill_dir, target_id) == VariantChoice::Variant).then_some(dir)
}

/// What a tool gets when the skill is enabled there: its own version, or the
/// library skill itself.
pub fn effective_source(library_skill_dir: &Path, target_id: &str) -> PathBuf {
    chosen_variant(library_skill_dir, target_id).unwrap_or_else(|| library_skill_dir.to_path_buf())
}

/// Every version folder of a library skill (to keep edits in step).
pub fn variant_dirs(library_skill_dir: &Path) -> Vec<PathBuf> {
    crate::targets::toggleable_target_ids()
        .iter()
        .filter_map(|id| variant_for(library_skill_dir, id))
        .collect()
}

/// The tools with their own version of a library skill, with the repository
/// folder each came from (from the skill's source file) and which version
/// each tool gets.
pub fn list(library_skill_dir: &Path) -> Vec<VariantInfo> {
    let ids: Vec<&str> = crate::targets::toggleable_target_ids()
        .iter()
        .copied()
        .filter(|id| variant_for(library_skill_dir, id).is_some())
        .collect();
    if ids.is_empty() {
        return Vec::new();
    }
    let labels = recorded_variants(library_skill_dir);
    let prefs = data_root_of(library_skill_dir).map(load_prefs).unwrap_or_default();
    let folder = folder_name(library_skill_dir);
    ids.into_iter()
        .map(|id| VariantInfo {
            target_id: id.to_string(),
            path: labels.get(id).cloned().unwrap_or_default(),
            choice: choice_in(&prefs, &folder, id),
        })
        .collect()
}

/// The repository folder of the version a tool gets, when it gets its own.
pub fn variant_label(library_skill_dir: &Path, target_id: &str) -> Option<String> {
    chosen_variant(library_skill_dir, target_id)?;
    recorded_variants(library_skill_dir).remove(target_id)
}

fn data_root_of(library_skill_dir: &Path) -> Option<&Path> {
    library_skill_dir.parent()?.parent()
}

fn folder_name(library_skill_dir: &Path) -> String {
    library_skill_dir
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// library folder -> tool id -> choice. Only tools the user switched appear.
type Prefs = BTreeMap<String, BTreeMap<String, VariantChoice>>;

fn load_prefs(data_root: &Path) -> Prefs {
    fs::read_to_string(data_root.join(PREFS_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_prefs(data_root: &Path, prefs: &Prefs) -> Result<(), String> {
    fs::create_dir_all(data_root).map_err(|error| format!("Could not create the data folder: {error}"))?;
    let text = serde_json::to_string_pretty(prefs).map_err(|error| error.to_string())?;
    // Write beside the file, then swap, so a crash never leaves half a file.
    let staging = data_root.join(format!("{PREFS_FILE}.tmp"));
    fs::write(&staging, text).map_err(|error| format!("Could not save the version choice: {error}"))?;
    fs::rename(&staging, data_root.join(PREFS_FILE))
        .map_err(|error| format!("Could not save the version choice: {error}"))
}

fn choice_in(prefs: &Prefs, folder: &str, target_id: &str) -> VariantChoice {
    prefs
        .get(folder)
        .and_then(|tools| tools.get(target_id))
        .copied()
        .unwrap_or(VariantChoice::Variant)
}

fn choice_for(library_skill_dir: &Path, target_id: &str) -> VariantChoice {
    data_root_of(library_skill_dir)
        .map(|data_root| choice_in(&load_prefs(data_root), &folder_name(library_skill_dir), target_id))
        .unwrap_or(VariantChoice::Variant)
}

fn save_choice(data_root: &Path, folder: &str, target_id: &str, choice: VariantChoice) -> Result<(), String> {
    let mut prefs = load_prefs(data_root);
    prefs.entry(folder.to_string()).or_default().insert(target_id.to_string(), choice);
    save_prefs(data_root, &prefs)
}

/// Drop a skill's choices when it leaves the library, so a later skill with
/// the same folder name starts fresh.
pub fn forget_choices(data_root: &Path, library_folder: &str) -> Result<(), String> {
    let mut prefs = load_prefs(data_root);
    if prefs.remove(library_folder).is_none() {
        return Ok(());
    }
    save_prefs(data_root, &prefs)
}

pub fn set_variant_choice(
    library_path: String,
    target_id: String,
    choice: VariantChoice,
) -> Result<VariantChoiceResult, String> {
    let data_root = crate::fs_ops::default_data_root()?;
    let target_root = crate::targets::target_root_for(&target_id)?;
    set_variant_choice_with_roots(Path::new(&library_path), &target_id, choice, &data_root, &target_root)
}

/// Pick which version a tool gets, and re-copy the skill there when it is
/// enabled. If that copy cannot be replaced, the old choice is put back.
pub fn set_variant_choice_with_roots(
    library_path: &Path,
    target_id: &str,
    choice: VariantChoice,
    data_root: &Path,
    target_root: &Path,
) -> Result<VariantChoiceResult, String> {
    let skill_dir = crate::targets::validate_library_skill(library_path, data_root)?;
    let target_name =
        crate::targets::target_name_for(target_id).ok_or_else(|| format!("Unknown tool: {target_id}"))?;
    if variant_for(&skill_dir, target_id).is_none() {
        return Err(format!("The repository has no {target_name} version of this skill."));
    }
    let folder = folder_name(&skill_dir);
    let previous = choice_in(&load_prefs(data_root), &folder, target_id);
    save_choice(data_root, &folder, target_id, choice)?;

    let roots = [(target_id, target_root.to_path_buf())];
    let (refreshed, failed) = crate::targets::refresh_managed_copies(&skill_dir, data_root, &roots);
    if !failed.is_empty() {
        let _ = save_choice(data_root, &folder, target_id, previous);
        return Err(format!("Could not replace the copy in {target_name}, so the version was not changed."));
    }
    let which = match choice {
        VariantChoice::Variant => format!("the repository's {target_name} version"),
        VariantChoice::Default => "the main version".to_string(),
    };
    Ok(VariantChoiceResult {
        target_id: target_id.to_string(),
        choice,
        refreshed: !refreshed.is_empty(),
        message: format!("{target_name} now uses {which}."),
    })
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
            vec![VariantInfo {
                target_id: "claude-code".to_string(),
                path: ".claude/skills/dev".to_string(),
                choice: VariantChoice::Variant,
            }]
        );
        assert_eq!(variant_label(&library_skill, "claude-code").as_deref(), Some(".claude/skills/dev"));
        assert_eq!(variant_label(&library_skill, "codex"), None);

        // Picking the main version for Claude Code, then the repository's again.
        let claude_root = data_root.join("claude-root");
        crate::targets::set_skill_target_enabled_with_root(&library_skill, "claude-code", true, &data_root, &claude_root)
            .unwrap();
        let claude_copy = claude_root.join("dev").join("SKILL.md");
        assert_eq!(fs::read_to_string(&claude_copy).unwrap(), "claude version");

        let chose_main =
            set_variant_choice_with_roots(&library_skill, "claude-code", VariantChoice::Default, &data_root, &claude_root)
                .unwrap();
        assert!(chose_main.refreshed);
        assert_eq!(fs::read_to_string(&claude_copy).unwrap(), FULL);
        assert_eq!(effective_source(&library_skill, "claude-code"), library_skill);
        assert_eq!(variant_label(&library_skill, "claude-code"), None);
        assert_eq!(list(&library_skill)[0].choice, VariantChoice::Default);
        assert!(crate::targets::is_managed_target_copy(&claude_root.join("dev"), &library_skill, "claude-code"));
        let prefs = fs::read_to_string(data_root.join(PREFS_FILE)).unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&prefs).unwrap()["dev"]["claude-code"], "default");

        set_variant_choice_with_roots(&library_skill, "claude-code", VariantChoice::Variant, &data_root, &claude_root)
            .unwrap();
        assert_eq!(fs::read_to_string(&claude_copy).unwrap(), "claude version");
        assert_eq!(list(&library_skill)[0].choice, VariantChoice::Variant);

        // A tool with no version of its own has nothing to choose, and an
        // unenabled tool just remembers the choice.
        let codex_root = data_root.join("codex-root");
        assert!(set_variant_choice_with_roots(&library_skill, "codex", VariantChoice::Default, &data_root, &codex_root)
            .is_err());
        let unrefreshed =
            set_variant_choice_with_roots(&library_skill, "claude-code", VariantChoice::Default, &data_root, &codex_root)
                .unwrap();
        assert!(!unrefreshed.refreshed);
        forget_choices(&data_root, "dev").unwrap();
        assert_eq!(list(&library_skill)[0].choice, VariantChoice::Variant);

        // The repository dropped its Claude version: back to the library skill.
        put_in_place(&data_root, "dev", None).unwrap();
        assert_eq!(effective_source(&library_skill, "claude-code"), library_skill);
        assert!(list(&library_skill).is_empty());
        let _ = fs::remove_dir_all(data_root);
    }
}
