//! Static check of whether a library skill suits a tool. It reads SKILL.md,
//! the skill's other Markdown files and its scripts, and looks for things that
//! only one tool understands (Claude Code's tool names, hooks, `$ARGUMENTS`,
//! Codex's `apply_patch` …). The result only drives a warning: enabling is
//! never blocked, so rules lean towards missing a case over a false alarm.
//!
//! Each rule names the tools a feature belongs to (`owners`); it counts against
//! every other tool. Rules with no owners (a wrapper SKILL.md that points at a
//! file outside the skill) count against every tool.

use crate::fs_ops::{default_data_root, is_excluded_entry, is_link};
use crate::series::git_blob_id;
use crate::targets;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Per-tool "I checked, it works" marks: `{ <library folder>: { <tool id>: <SKILL.md blob id> } }`.
/// A mark only holds while SKILL.md is unchanged, so an update warns again.
const ACKS_FILE: &str = "compat-acks.json";
const MAX_FILES: usize = 200;
const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_DEPTH: usize = 6;
/// A SKILL.md body shorter than this that points at another SKILL.md is a
/// wrapper: on its own in a tool folder, the file it points at is missing.
const WRAPPER_MAX_BODY_CHARS: usize = 600;
const EXCERPT_CHARS: usize = 120;
const SCRIPT_EXTENSIONS: [&str; 9] = ["sh", "bash", "py", "js", "mjs", "cjs", "ts", "ps1", "rb"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CompatLevel {
    Compatible,
    Degraded,
    Incompatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatFinding {
    pub rule_id: String,
    pub level: CompatLevel,
    /// What matched, e.g. `TodoWrite` or `allowed-tools`.
    pub token: String,
    /// Relative to the skill folder, with `/` separators.
    pub file: String,
    pub line: Option<usize>,
    pub excerpt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatReport {
    pub target_id: String,
    pub target_name: String,
    pub level: CompatLevel,
    /// The frontmatter `compatibility` text, when the author wrote one.
    pub declared: Option<String>,
    pub findings: Vec<CompatFinding>,
    pub acknowledged: bool,
}

/// What the skill list needs to warn before enabling: one entry per tool the
/// skill may not suit (tools it suits are left out).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatSummary {
    pub target_id: String,
    pub level: CompatLevel,
    pub count: usize,
    pub first: CompatFinding,
    pub acknowledged: bool,
}

const CLAUDE: &[&str] = &["claude-code"];
const CODEX: &[&str] = &["codex"];

/// A token in Markdown text (and, with `in_scripts`, in scripts). Matching is
/// case-sensitive and on word boundaries, so prose like "task" never matches.
struct TextRule {
    id: &'static str,
    owners: &'static [&'static str],
    level: CompatLevel,
    tokens: &'static [&'static str],
    in_scripts: bool,
}

/// Checked in order; once a line matched a rule, later rules with the same
/// owners skip that line (so `~/.claude/settings.json` is one finding, not two).
const TEXT_RULES: [TextRule; 6] = [
    TextRule {
        id: "cc-hooks",
        owners: CLAUDE,
        level: CompatLevel::Incompatible,
        tokens: &[".claude/settings.json", "CLAUDE_PLUGIN_ROOT"],
        in_scripts: true,
    },
    TextRule {
        id: "cc-tools",
        owners: CLAUDE,
        level: CompatLevel::Degraded,
        tokens: &[
            "TodoWrite",
            "AskUserQuestion",
            "WebFetch",
            "WebSearch",
            "SlashCommand",
            "NotebookEdit",
            "ExitPlanMode",
            "Task tool",
        ],
        in_scripts: false,
    },
    TextRule {
        id: "cc-args",
        owners: CLAUDE,
        level: CompatLevel::Degraded,
        tokens: &["$ARGUMENTS"],
        in_scripts: false,
    },
    TextRule {
        id: "cc-paths",
        owners: CLAUDE,
        level: CompatLevel::Degraded,
        tokens: &["CLAUDE.md", "~/.claude/", ".claude/skills", ".claude/commands", ".claude/agents"],
        in_scripts: true,
    },
    TextRule {
        id: "cx-tools",
        owners: CODEX,
        level: CompatLevel::Degraded,
        tokens: &["apply_patch", "update_plan", "spawn_agent"],
        in_scripts: false,
    },
    TextRule {
        id: "cx-paths",
        owners: CODEX,
        level: CompatLevel::Degraded,
        tokens: &["~/.codex/", ".codex/skills", "CODEX_HOME"],
        in_scripts: true,
    },
];

/// Top-level frontmatter keys only one tool acts on: (key, rule id, owners, level).
/// Keys that only steer how Claude Code offers a skill (`disable-model-invocation`,
/// `user-invocable`, `argument-hint`) are left out: losing them changes no
/// behavior, and on real libraries they made most warnings noise.
const FRONTMATTER_RULES: [(&str, &str, &[&str], CompatLevel); 4] = [
    ("hooks", "cc-hooks", CLAUDE, CompatLevel::Incompatible),
    ("allowed-tools", "cc-frontmatter", CLAUDE, CompatLevel::Degraded),
    ("model", "cc-frontmatter", CLAUDE, CompatLevel::Degraded),
    ("agent", "cc-frontmatter", CLAUDE, CompatLevel::Degraded),
];

/// Words that name a tool in a `compatibility` declaration.
const TOOL_ALIASES: [(&str, &[&str]); 9] = [
    ("claude-code", &["claude"]),
    ("codex", &["codex"]),
    ("gemini-cli", &["gemini"]),
    ("grok-build", &["grok"]),
    ("opencode", &["opencode"]),
    ("openclaw", &["openclaw"]),
    ("hermes", &["hermes"]),
    ("qwen-code", &["qwen"]),
    ("qoder", &["qoder"]),
];

struct Feature {
    finding: CompatFinding,
    /// Tools that understand it; empty means no tool does.
    owners: Vec<&'static str>,
}

struct Inspection {
    features: Vec<Feature>,
    declared: Option<String>,
    skill_md_blob: Option<String>,
}

impl Inspection {
    fn report(&self, target_id: &str, target_name: &str, acknowledged: bool) -> CompatReport {
        let mut findings: Vec<CompatFinding> = self
            .features
            .iter()
            .filter(|feature| feature.owners.is_empty() || !feature.owners.contains(&target_id))
            .map(|feature| feature.finding.clone())
            .collect();
        findings.sort_by(|left, right| {
            right.level.cmp(&left.level).then_with(|| (&left.file, left.line).cmp(&(&right.file, right.line)))
        });
        let level = findings.iter().map(|finding| finding.level).max().unwrap_or(CompatLevel::Compatible);
        CompatReport {
            target_id: target_id.to_string(),
            target_name: target_name.to_string(),
            level,
            declared: self.declared.clone(),
            findings,
            acknowledged,
        }
    }
}

pub fn check_skill_compat(source_path: String, target_id: String) -> Result<CompatReport, String> {
    check_skill_compat_with_root(Path::new(&source_path), &target_id, &default_data_root()?)
}

pub fn acknowledge_skill_compat(source_path: String, target_id: String) -> Result<CompatReport, String> {
    acknowledge_skill_compat_with_root(Path::new(&source_path), &target_id, &default_data_root()?)
}

fn check_skill_compat_with_root(skill_dir: &Path, target_id: &str, data_root: &Path) -> Result<CompatReport, String> {
    let target_name = targets::target_name_for(target_id).ok_or_else(|| format!("Unknown tool: {target_id}"))?;
    let skill_dir = library_skill_dir(skill_dir, data_root)?;
    let inspection = inspect(&crate::variants::effective_source(&skill_dir, target_id));
    let acknowledged = is_acknowledged(&load_acks(data_root), &skill_dir, target_id, &inspection);
    Ok(inspection.report(target_id, target_name, acknowledged))
}

fn acknowledge_skill_compat_with_root(
    skill_dir: &Path,
    target_id: &str,
    data_root: &Path,
) -> Result<CompatReport, String> {
    let target_name = targets::target_name_for(target_id).ok_or_else(|| format!("Unknown tool: {target_id}"))?;
    let skill_dir = library_skill_dir(skill_dir, data_root)?;
    let inspection = inspect(&crate::variants::effective_source(&skill_dir, target_id));
    let blob = inspection
        .skill_md_blob
        .clone()
        .ok_or_else(|| "SKILL.md could not be read.".to_string())?;
    let mut acks = load_acks(data_root);
    acks.entry(folder_name(&skill_dir)).or_default().insert(target_id.to_string(), blob);
    save_acks(data_root, &acks)?;
    Ok(inspection.report(target_id, target_name, true))
}

/// Summaries for a skill in `<data_root>/library`, for every tool it may not
/// suit. A tool with its own version of the skill is judged by that version.
/// The marks are read from that same data root, so tests stay isolated.
pub fn library_skill_summaries(skill_dir: &Path) -> Vec<CompatSummary> {
    let library_version = inspect(skill_dir);
    let variants: BTreeMap<&str, Inspection> = targets::TOOLS
        .iter()
        .filter_map(|(target_id, _, _)| Some((*target_id, inspect(&crate::variants::chosen_variant(skill_dir, target_id)?))))
        .collect();
    if library_version.features.is_empty() && variants.values().all(|inspection| inspection.features.is_empty()) {
        return Vec::new();
    }
    let acks = skill_dir
        .parent()
        .and_then(Path::parent)
        .map(load_acks)
        .unwrap_or_default();
    targets::TOOLS
        .iter()
        .filter_map(|(target_id, target_name, _)| {
            let inspection = variants.get(target_id).unwrap_or(&library_version);
            let report = inspection.report(target_id, target_name, false);
            let first = report.findings.first()?.clone();
            Some(CompatSummary {
                target_id: target_id.to_string(),
                level: report.level,
                count: report.findings.len(),
                first,
                acknowledged: is_acknowledged(&acks, skill_dir, target_id, inspection),
            })
        })
        .collect()
}

fn library_skill_dir(skill_dir: &Path, data_root: &Path) -> Result<PathBuf, String> {
    let library = data_root
        .join("library")
        .canonicalize()
        .map_err(|error| format!("Could not read the shared library: {error}"))?;
    let canonical = skill_dir
        .canonicalize()
        .map_err(|_| "Skill folder was not found in the shared library.".to_string())?;
    if !canonical.starts_with(&library) || canonical == library {
        return Err("Only shared-library skills can be checked.".to_string());
    }
    Ok(canonical)
}

fn folder_name(skill_dir: &Path) -> String {
    skill_dir
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

type Acks = BTreeMap<String, BTreeMap<String, String>>;

fn load_acks(data_root: &Path) -> Acks {
    fs::read_to_string(data_root.join(ACKS_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_acks(data_root: &Path, acks: &Acks) -> Result<(), String> {
    fs::create_dir_all(data_root).map_err(|error| format!("Could not create the data folder: {error}"))?;
    let text = serde_json::to_string_pretty(acks).map_err(|error| error.to_string())?;
    // Write beside the file, then swap, so a crash never leaves half a file.
    let staging = data_root.join(format!("{ACKS_FILE}.tmp"));
    fs::write(&staging, text).map_err(|error| format!("Could not save the confirmation: {error}"))?;
    fs::rename(&staging, data_root.join(ACKS_FILE)).map_err(|error| format!("Could not save the confirmation: {error}"))
}

fn is_acknowledged(acks: &Acks, skill_dir: &Path, target_id: &str, inspection: &Inspection) -> bool {
    let Some(blob) = inspection.skill_md_blob.as_deref() else {
        return false;
    };
    acks.get(&folder_name(skill_dir))
        .and_then(|tools| tools.get(target_id))
        .is_some_and(|saved| saved == blob)
}

fn inspect(skill_dir: &Path) -> Inspection {
    let mut inspection = Inspection { features: Vec::new(), declared: None, skill_md_blob: None };
    let mut seen: HashSet<(String, String)> = HashSet::new();

    let skill_md = skill_dir.join("SKILL.md");
    if let Some(bytes) = read_capped(&skill_md) {
        inspection.skill_md_blob = Some(git_blob_id(&bytes));
        let text = String::from_utf8_lossy(&bytes);
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        let lines: Vec<&str> = text.lines().collect();
        let body_start = inspect_frontmatter(skill_dir, &lines, &mut inspection, &mut seen);
        inspect_wrapper(skill_dir, text, &lines, &mut inspection);
        inspect_text("SKILL.md", &lines, body_start, false, &mut inspection, &mut seen);
    }

    for (relative, path) in other_files(skill_dir) {
        let Some(bytes) = read_capped(&path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        let is_script = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| SCRIPT_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()));
        inspect_text(&relative, &lines, 0, is_script, &mut inspection, &mut seen);
    }

    inspection
}

/// Records frontmatter findings and returns the index of the first body line.
fn inspect_frontmatter(
    skill_dir: &Path,
    lines: &[&str],
    inspection: &mut Inspection,
    seen: &mut HashSet<(String, String)>,
) -> usize {
    let mut name: Option<(String, usize)> = None;
    let mut body_start = 0;

    if lines.first().map(|line| line.trim_end()) == Some("---") {
        if let Some(end) = lines.iter().skip(1).position(|line| line.trim_end() == "---") {
            let end = end + 1;
            body_start = end + 1;
            for (index, line) in lines.iter().enumerate().take(end).skip(1) {
                // Only top-level keys: nested ones (e.g. under `metadata:`) are
                // ignored by every tool, which is how converted copies park them.
                if line.starts_with(char::is_whitespace) {
                    continue;
                }
                let Some((key, value)) = line.split_once(':') else {
                    continue;
                };
                let key = key.trim();
                let value = unquote(value.trim());
                let line_number = index + 1;

                for (rule_key, rule_id, owners, level) in FRONTMATTER_RULES {
                    if key == rule_key {
                        push(inspection, seen, rule_id, owners, level, key, "SKILL.md", Some(line_number), line);
                    }
                }
                match key {
                    "context" if value == "fork" => push(
                        inspection,
                        seen,
                        "cc-fork",
                        CLAUDE,
                        CompatLevel::Incompatible,
                        "context: fork",
                        "SKILL.md",
                        Some(line_number),
                        line,
                    ),
                    "name" => name = Some((value.to_string(), line_number)),
                    "compatibility" if !value.is_empty() && !is_block_scalar(value) => {
                        inspection.declared = Some(value.to_string());
                        inspect_declared(value, line_number, line, inspection, seen);
                    }
                    _ => {}
                }
            }
        }
    }

    let folder = folder_name(skill_dir).to_lowercase();
    let (named, line) = match &name {
        Some((value, line)) => (value.to_lowercase(), Some(*line)),
        None => (String::new(), None),
    };
    if named.contains("claude-code") || folder.contains("claude-code") {
        let excerpt = line.and_then(|line| lines.get(line - 1)).copied().unwrap_or("");
        push(inspection, seen, "cc-name", CLAUDE, CompatLevel::Degraded, "claude-code", "SKILL.md", line, excerpt);
    }

    body_start
}

/// A `compatibility` text that names tools counts against the tools it leaves
/// out — strongly when it says "only" or "requires".
fn inspect_declared(
    value: &str,
    line_number: usize,
    line: &str,
    inspection: &mut Inspection,
    seen: &mut HashSet<(String, String)>,
) {
    let lowered = value.to_lowercase();
    let mut owners: Vec<&'static str> = Vec::new();
    for (tool_id, aliases) in TOOL_ALIASES {
        if aliases.iter().any(|alias| contains_word(&lowered, alias)) && !owners.contains(&tool_id) {
            owners.push(tool_id);
        }
    }
    if owners.is_empty() {
        return;
    }
    let strict = ["only", "require", "仅", "只"].iter().any(|word| lowered.contains(word));
    let level = if strict { CompatLevel::Incompatible } else { CompatLevel::Degraded };
    if seen.insert(("declared".to_string(), value.to_string())) {
        inspection.features.push(Feature {
            finding: finding("declared", level, value, "SKILL.md", Some(line_number), line),
            owners,
        });
    }
}

fn inspect_wrapper(skill_dir: &Path, text: &str, lines: &[&str], inspection: &mut Inspection) {
    let Some((reference, line)) = wrapper_reference(text, |reference| skill_dir.join(reference).is_file()) else {
        return;
    };
    let excerpt = lines.get(line - 1).copied().unwrap_or("");
    inspection.features.push(Feature {
        finding: finding("wrapper", CompatLevel::Incompatible, &reference, "SKILL.md", Some(line), excerpt),
        owners: Vec::new(),
    });
}

/// For a wrapper SKILL.md (a short body pointing at a SKILL.md that
/// `exists_inside` says is not part of the skill): that path and its line.
pub(crate) fn wrapper_reference(text: &str, exists_inside: impl Fn(&str) -> bool) -> Option<(String, usize)> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let lines: Vec<&str> = text.lines().collect();
    let body_start = frontmatter_end(&lines);
    let body = lines.get(body_start..).unwrap_or(&[]);
    let length: usize = body.iter().map(|line| line.trim().chars().count()).sum();
    if length >= WRAPPER_MAX_BODY_CHARS {
        return None;
    }
    body.iter().enumerate().find_map(|(offset, line)| {
        skill_md_references(line)
            .into_iter()
            .find(|reference| !exists_inside(reference))
            .map(|reference| (reference, body_start + offset + 1))
    })
}

/// Index of the first line after the frontmatter (0 when there is none).
fn frontmatter_end(lines: &[&str]) -> usize {
    if lines.first().map(|line| line.trim_end()) != Some("---") {
        return 0;
    }
    lines
        .iter()
        .skip(1)
        .position(|line| line.trim_end() == "---")
        .map_or(0, |end| end + 2)
}

/// Paths to another SKILL.md in a line, e.g. `.agents/skills/dev/SKILL.md`.
fn skill_md_references(line: &str) -> Vec<String> {
    let mut references = Vec::new();
    let mut search_from = 0;
    while let Some(found) = line[search_from..].find("SKILL.md") {
        let end = search_from + found;
        let start = line[..end]
            .char_indices()
            .rev()
            .find(|(_, character)| character.is_whitespace() || "`\"'()[]<>".contains(*character))
            .map(|(index, character)| index + character.len_utf8())
            .unwrap_or(0);
        let path = &line[start..end];
        let path = path.strip_prefix("./").unwrap_or(path);
        if path.contains('/') {
            references.push(format!("{path}SKILL.md"));
        }
        search_from = end + "SKILL.md".len();
    }
    references
}

fn inspect_text(
    file: &str,
    lines: &[&str],
    start: usize,
    is_script: bool,
    inspection: &mut Inspection,
    seen: &mut HashSet<(String, String)>,
) {
    for (index, line) in lines.iter().enumerate().skip(start) {
        let mut matched_owners: Vec<&[&str]> = Vec::new();
        for rule in &TEXT_RULES {
            if is_script && !rule.in_scripts {
                continue;
            }
            if matched_owners.contains(&rule.owners) {
                continue;
            }
            if let Some(token) = rule.tokens.iter().find(|token| contains_word(line, token)) {
                matched_owners.push(rule.owners);
                push(inspection, seen, rule.id, rule.owners, rule.level, token, file, Some(index + 1), line);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push(
    inspection: &mut Inspection,
    seen: &mut HashSet<(String, String)>,
    rule_id: &str,
    owners: &[&'static str],
    level: CompatLevel,
    token: &str,
    file: &str,
    line: Option<usize>,
    excerpt: &str,
) {
    // One finding per rule and token: a skill that says TodoWrite twenty
    // times has one problem, shown where it first appears.
    if !seen.insert((rule_id.to_string(), token.to_string())) {
        return;
    }
    inspection.features.push(Feature {
        finding: finding(rule_id, level, token, file, line, excerpt),
        owners: owners.to_vec(),
    });
}

fn finding(rule_id: &str, level: CompatLevel, token: &str, file: &str, line: Option<usize>, excerpt: &str) -> CompatFinding {
    let excerpt = excerpt.trim();
    let excerpt = if excerpt.chars().count() > EXCERPT_CHARS {
        format!("{}…", excerpt.chars().take(EXCERPT_CHARS).collect::<String>())
    } else {
        excerpt.to_string()
    };
    CompatFinding {
        rule_id: rule_id.to_string(),
        level,
        token: token.to_string(),
        file: file.to_string(),
        line,
        excerpt,
    }
}

/// `token` in `text` with no letter, digit or `_` right before or after it.
fn contains_word(text: &str, token: &str) -> bool {
    let is_word = |character: char| character.is_alphanumeric() || character == '_';
    let mut search_from = 0;
    while let Some(found) = text[search_from..].find(token) {
        let start = search_from + found;
        let end = start + token.len();
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();
        let token_starts_word = token.chars().next().is_some_and(is_word);
        let token_ends_word = token.chars().next_back().is_some_and(is_word);
        let clear_before = !token_starts_word || !before.is_some_and(is_word);
        let clear_after = !token_ends_word || !after.is_some_and(is_word);
        if clear_before && clear_after {
            return true;
        }
        search_from = start + token.chars().next().map_or(1, char::len_utf8);
    }
    false
}

fn unquote(value: &str) -> &str {
    let quoted = value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"')) || (value.starts_with('\'') && value.ends_with('\'')));
    if quoted {
        value[1..value.len() - 1].trim()
    } else {
        value
    }
}

fn is_block_scalar(value: &str) -> bool {
    matches!(value.chars().next(), Some('|') | Some('>'))
}

fn read_capped(path: &Path) -> Option<Vec<u8>> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return None;
    }
    fs::read(path).ok()
}

/// Markdown files and scripts in the skill besides its SKILL.md, sorted, with
/// hidden entries, generated folders and links skipped.
fn other_files(skill_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut files = Vec::new();
    collect_files(skill_dir, "", 0, &mut files);
    files.sort();
    files
}

fn collect_files(dir: &Path, prefix: &str, depth: usize, files: &mut Vec<(String, PathBuf)>) {
    if depth > MAX_DEPTH || files.len() >= MAX_FILES {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if files.len() >= MAX_FILES {
            return;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || is_excluded_entry(&name) || is_link(&entry) {
            continue;
        }
        let relative = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, &relative, depth + 1, files);
            continue;
        }
        if relative == "SKILL.md" {
            continue;
        }
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if extension == "md" || SCRIPT_EXTENSIONS.contains(&extension.as_str()) {
            files.push((relative, path));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs_ops::test_temp_dir;

    fn skill(root: &Path, folder: &str, skill_md: &str) -> PathBuf {
        let dir = root.join("library").join(folder);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), skill_md).unwrap();
        dir
    }

    fn report(dir: &Path, target_id: &str) -> CompatReport {
        inspect(dir).report(target_id, target_id, false)
    }

    fn rules(report: &CompatReport) -> Vec<(&str, &str)> {
        report.findings.iter().map(|finding| (finding.rule_id.as_str(), finding.token.as_str())).collect()
    }

    #[test]
    fn plain_skill_suits_every_tool() {
        let root = test_temp_dir("compat-plain");
        let dir = skill(&root, "pdf", "---\nname: pdf\ndescription: Work with PDFs.\n---\n\nKeep track of each task.\nUse the tools you have.\n");
        assert!(inspect(&dir).features.is_empty());
        assert!(library_skill_summaries(&dir).is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn claude_features_count_against_codex_but_not_claude() {
        let root = test_temp_dir("compat-claude");
        let dir = skill(
            &root,
            "review",
            "---\nname: review\ndescription: Review.\nallowed-tools: Read, Grep\n---\n\nTrack steps with TodoWrite.\nPass $ARGUMENTS on.\n",
        );
        let codex = report(&dir, "codex");
        assert_eq!(codex.level, CompatLevel::Degraded);
        assert_eq!(
            rules(&codex),
            vec![("cc-frontmatter", "allowed-tools"), ("cc-tools", "TodoWrite"), ("cc-args", "$ARGUMENTS")]
        );
        assert_eq!(codex.findings[1].line, Some(7));
        assert_eq!(report(&dir, "claude-code").level, CompatLevel::Compatible);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn finds_skill_md_references_after_any_separator() {
        assert_eq!(skill_md_references("Read `.agents/skills/dev/SKILL.md` first."), vec![".agents/skills/dev/SKILL.md"]);
        assert_eq!(skill_md_references("先读　skills/a/SKILL.md"), vec!["skills/a/SKILL.md"]);
        assert!(skill_md_references("Edit ./SKILL.md or SKILL.md").is_empty());
    }

    #[test]
    fn words_must_stand_alone_and_match_case() {
        assert!(contains_word("Use the Task tool here", "Task tool"));
        assert!(!contains_word("Use a task tool here", "Task tool"));
        assert!(!contains_word("MyTodoWriteHelper", "TodoWrite"));
        assert!(contains_word("`TodoWrite`.", "TodoWrite"));
        assert!(contains_word("cat ~/.claude/settings.json", ".claude/settings.json"));
        assert!(contains_word("echo $ARGUMENTS", "$ARGUMENTS"));
        assert!(!contains_word("echo $ARGUMENTS_LIST", "$ARGUMENTS"));
    }

    #[test]
    fn hooks_and_fork_are_incompatible_and_one_line_counts_once() {
        let root = test_temp_dir("compat-hooks");
        let dir = skill(
            &root,
            "git-guardrails-claude-code",
            "---\nname: git-guardrails-claude-code\ndescription: Block git.\ncontext: fork\n---\n\nAdd the hook to ~/.claude/settings.json.\n",
        );
        let codex = report(&dir, "codex");
        assert_eq!(codex.level, CompatLevel::Incompatible);
        assert_eq!(
            rules(&codex),
            vec![("cc-fork", "context: fork"), ("cc-hooks", ".claude/settings.json"), ("cc-name", "claude-code")]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn keys_that_only_steer_invocation_are_not_warnings() {
        let root = test_temp_dir("compat-invocation");
        let dir = skill(
            &root,
            "grill-me",
            "---\nname: grill-me\ndescription: Grill.\ndisable-model-invocation: true\nargument-hint: <topic>\nuser-invocable: true\n---\n\nAsk hard questions.\n",
        );
        assert_eq!(report(&dir, "codex").level, CompatLevel::Compatible);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn nested_frontmatter_keys_are_ignored() {
        let root = test_temp_dir("compat-nested");
        let dir = skill(
            &root,
            "autopilot",
            "---\nname: autopilot\ndescription: Loop.\nmetadata:\n  model: sonnet\n  user-invocable: 'true'\n---\n\nRun the loop.\n",
        );
        assert_eq!(report(&dir, "codex").level, CompatLevel::Compatible);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn codex_features_count_against_claude() {
        let root = test_temp_dir("compat-codex");
        let dir = skill(&root, "edit", "---\nname: edit\ndescription: Edit.\n---\n\nEdit files with apply_patch.\n");
        assert_eq!(report(&dir, "claude-code").level, CompatLevel::Degraded);
        assert_eq!(report(&dir, "codex").level, CompatLevel::Compatible);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_wrapper_pointing_outside_the_skill_suits_no_tool() {
        let root = test_temp_dir("compat-wrapper");
        let dir = skill(
            &root,
            "dev",
            "---\nname: dev\ndescription: Dev cycle.\n---\n\nRead `.agents/skills/dev/SKILL.md` for full instructions.\n",
        );
        for tool in ["claude-code", "codex"] {
            let report = report(&dir, tool);
            assert_eq!(report.level, CompatLevel::Incompatible);
            assert_eq!(rules(&report), vec![("wrapper", ".agents/skills/dev/SKILL.md")]);
        }

        // A link to a file inside the skill is fine.
        let inside = skill(&root, "kit", "---\nname: kit\ndescription: Kit.\n---\n\nSee `parts/a/SKILL.md`.\n");
        fs::create_dir_all(inside.join("parts/a")).unwrap();
        fs::write(inside.join("parts/a/SKILL.md"), "x").unwrap();
        assert!(inspect(&inside).features.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn declared_compatibility_counts_against_unnamed_tools() {
        let root = test_temp_dir("compat-declared");
        let dir = skill(&root, "only", "---\nname: only\ndescription: x\ncompatibility: Claude Code only\n---\n\nBody.\n");
        let codex = report(&dir, "codex");
        assert_eq!(codex.level, CompatLevel::Incompatible);
        assert_eq!(codex.declared.as_deref(), Some("Claude Code only"));
        assert_eq!(report(&dir, "claude-code").level, CompatLevel::Compatible);

        let both = skill(&root, "both", "---\nname: both\ndescription: x\ncompatibility: Works with Claude Code and Codex\n---\n\nBody.\n");
        assert_eq!(report(&both, "codex").level, CompatLevel::Compatible);
        assert_eq!(report(&both, "gemini-cli").level, CompatLevel::Degraded);

        let general = skill(&root, "general", "---\nname: general\ndescription: x\ncompatibility: Needs Python 3.10+\n---\n\nBody.\n");
        assert_eq!(report(&general, "codex").level, CompatLevel::Compatible);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scripts_only_trigger_path_rules() {
        let root = test_temp_dir("compat-scripts");
        let dir = skill(&root, "tool", "---\nname: tool\ndescription: x\n---\n\nRun the script.\n");
        fs::create_dir_all(dir.join("scripts")).unwrap();
        fs::write(dir.join("scripts/run.py"), "# TodoWrite is mentioned here\npath = '~/.claude/skills'\n").unwrap();
        fs::write(dir.join("references.md"), "Then call WebFetch.\n").unwrap();
        let codex = report(&dir, "codex");
        assert_eq!(rules(&codex), vec![("cc-tools", "WebFetch"), ("cc-paths", "~/.claude/")]);
        assert_eq!(codex.findings[1].file, "scripts/run.py");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn acknowledgement_holds_until_skill_md_changes() {
        let root = test_temp_dir("compat-ack");
        let dir = skill(&root, "review", "---\nname: review\ndescription: x\n---\n\nUse TodoWrite.\n");

        let before = check_skill_compat_with_root(&dir, "codex", &root).unwrap();
        assert!(!before.acknowledged);
        assert!(acknowledge_skill_compat_with_root(&dir, "codex", &root).unwrap().acknowledged);
        assert!(check_skill_compat_with_root(&dir, "codex", &root).unwrap().acknowledged);
        let summary = library_skill_summaries(&dir);
        assert!(summary.iter().any(|entry| entry.target_id == "codex" && entry.acknowledged));
        assert!(summary.iter().any(|entry| entry.target_id == "gemini-cli" && !entry.acknowledged));

        fs::write(dir.join("SKILL.md"), "---\nname: review\ndescription: x\n---\n\nUse TodoWrite a lot.\n").unwrap();
        assert!(!check_skill_compat_with_root(&dir, "codex", &root).unwrap().acknowledged);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_tool_with_its_own_version_is_judged_by_it() {
        let root = test_temp_dir("compat-variant");
        let dir = skill(&root, "review", "---\nname: review\ndescription: x\n---\n\nUse TodoWrite.\n");
        let codex_version = root.join("variants").join("review").join("codex");
        fs::create_dir_all(&codex_version).unwrap();
        fs::write(codex_version.join("SKILL.md"), "---\nname: review\ndescription: x\n---\n\nKeep a plan.\n").unwrap();

        let summaries = library_skill_summaries(&dir);
        assert!(summaries.iter().all(|entry| entry.target_id != "codex"));
        assert!(summaries.iter().any(|entry| entry.target_id == "gemini-cli"));
        assert_eq!(check_skill_compat_with_root(&dir, "codex", &root).unwrap().level, CompatLevel::Compatible);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn only_library_skills_and_known_tools_are_checked() {
        let root = test_temp_dir("compat-guard");
        let dir = skill(&root, "review", "---\nname: review\ndescription: x\n---\n\nBody.\n");
        let outside = root.join("elsewhere");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("SKILL.md"), "x").unwrap();

        assert!(check_skill_compat_with_root(&outside, "codex", &root).is_err());
        assert!(check_skill_compat_with_root(&dir, "vs-code", &root).is_err());
        assert!(check_skill_compat_with_root(&root.join("library"), "codex", &root).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
