//! Editing a shared-library skill's `name` and `description` in SKILL.md.
//! Only the two frontmatter keys are rewritten; every other line is kept, and
//! tools that have the skill enabled get a refreshed copy afterwards.

use crate::fs_ops::default_data_root;
use crate::targets;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataResult {
    pub name: String,
    pub refreshed_targets: Vec<String>,
    pub failed_targets: Vec<String>,
}

pub fn update_skill_metadata(
    library_path: String,
    name: String,
    description: String,
) -> Result<MetadataResult, String> {
    let data_root = default_data_root()?;
    let roots = targets::toggleable_target_ids()
        .iter()
        .map(|id| targets::target_root_for(id).map(|root| (*id, root)))
        .collect::<Result<Vec<_>, _>>()?;
    update_skill_metadata_with_roots(Path::new(&library_path), &name, &description, &data_root, &roots)
}

pub fn update_skill_metadata_with_roots(
    library_path: &Path,
    name: &str,
    description: &str,
    data_root: &Path,
    target_roots: &[(&str, PathBuf)],
) -> Result<MetadataResult, String> {
    let name = single_line(name);
    let description = single_line(description);
    if name.is_empty() {
        return Err("The skill name can't be empty.".to_string());
    }
    targets::validate_library_skill(library_path, data_root)?;

    let skill_file = library_path.join("SKILL.md");
    let contents =
        fs::read_to_string(&skill_file).map_err(|error| format!("Could not read SKILL.md: {error}"))?;
    let updated = rewrite_frontmatter(&contents, &name, &description);
    fs::write(&skill_file, updated).map_err(|error| format!("Could not write SKILL.md: {error}"))?;
    // The tools' own versions carry the same name and description.
    for version in crate::variants::variant_dirs(library_path) {
        let version_file = version.join("SKILL.md");
        if let Ok(contents) = fs::read_to_string(&version_file) {
            fs::write(&version_file, rewrite_frontmatter(&contents, &name, &description))
                .map_err(|error| format!("Could not write SKILL.md: {error}"))?;
        }
    }

    let (refreshed_targets, failed_targets) =
        targets::refresh_managed_copies(library_path, data_root, target_roots);
    Ok(MetadataResult {
        name,
        refreshed_targets,
        failed_targets,
    })
}

fn single_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Double-quoted YAML scalar, safe for any single-line value.
fn yaml_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Replace (or add) the top-level `name:` and `description:` frontmatter keys,
/// keeping everything else. A file without frontmatter gets one prepended.
pub(crate) fn rewrite_frontmatter(contents: &str, name: &str, description: &str) -> String {
    let bom = if contents.starts_with('\u{feff}') { "\u{feff}" } else { "" };
    let body = &contents[bom.len()..];
    let newline = if body.contains("\r\n") { "\r\n" } else { "\n" };
    let name_line = format!("name: {}", yaml_quote(name));
    let description_line = format!("description: {}", yaml_quote(description));

    let lines: Vec<&str> = body.split(newline).collect();
    let closing = if lines.first().map(|line| line.trim_end()) == Some("---") {
        lines.iter().skip(1).position(|line| line.trim_end() == "---").map(|index| index + 1)
    } else {
        None
    };

    let Some(closing) = closing else {
        return format!("{bom}---{newline}{name_line}{newline}{description_line}{newline}---{newline}{body}");
    };

    let mut output: Vec<String> = vec![lines[0].to_string()];
    let mut wrote_name = false;
    let mut wrote_description = false;
    let mut skipping_block = false;
    for line in &lines[1..closing] {
        let indented = line.starts_with(char::is_whitespace);
        if skipping_block {
            // Continuation lines of a replaced block value (`description: >`).
            if indented || line.trim().is_empty() {
                continue;
            }
            skipping_block = false;
        }
        let key = line.split_once(':').map(|(key, _)| key);
        match key {
            Some("name") if !indented => {
                output.push(name_line.clone());
                wrote_name = true;
                skipping_block = true;
            }
            Some("description") if !indented => {
                output.push(description_line.clone());
                wrote_description = true;
                skipping_block = true;
            }
            _ => output.push((*line).to_string()),
        }
    }
    if !wrote_name {
        output.insert(1, name_line);
    }
    if !wrote_description {
        let position = output.iter().position(|line| line.starts_with("name:")).map_or(1, |index| index + 1);
        output.insert(position, description_line);
    }
    output.extend(lines[closing..].iter().map(|line| (*line).to_string()));
    format!("{bom}{}", output.join(newline))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs_ops::test_temp_dir;

    #[test]
    fn replaces_existing_keys_and_keeps_other_lines() {
        let input = "---\nname: old\ndescription: >\n  folded\n  text\nlicense: MIT\n---\n# Body\n";
        let output = rewrite_frontmatter(input, "new: name", "Says \"hi\"");
        assert_eq!(
            output,
            "---\nname: \"new: name\"\ndescription: \"Says \\\"hi\\\"\"\nlicense: MIT\n---\n# Body\n"
        );
    }

    #[test]
    fn adds_missing_keys_and_keeps_crlf_and_bom() {
        let input = "\u{feff}---\r\nlicense: MIT\r\n---\r\nBody\r\n";
        let output = rewrite_frontmatter(input, "n", "d");
        assert_eq!(output, "\u{feff}---\r\nname: \"n\"\r\ndescription: \"d\"\r\nlicense: MIT\r\n---\r\nBody\r\n");
    }

    #[test]
    fn prepends_frontmatter_when_missing() {
        assert_eq!(
            rewrite_frontmatter("# Plain\n", "n", "d"),
            "---\nname: \"n\"\ndescription: \"d\"\n---\n# Plain\n"
        );
    }

    #[test]
    fn saving_renames_the_tools_own_versions_too() {
        let data_root = test_temp_dir("metadata-variants");
        let library_skill = data_root.join("library").join("dev");
        fs::create_dir_all(&library_skill).unwrap();
        fs::write(library_skill.join("SKILL.md"), "---\nname: dev\n---\n# Main\n").unwrap();
        let codex_version = data_root.join("variants").join("dev").join("codex");
        fs::create_dir_all(&codex_version).unwrap();
        fs::write(codex_version.join("SKILL.md"), "---\nname: dev\n---\n# Codex\n").unwrap();

        update_skill_metadata_with_roots(&library_skill, "Dev cycle", "Ship it", &data_root, &[]).unwrap();

        let version = fs::read_to_string(codex_version.join("SKILL.md")).unwrap();
        assert!(version.contains("Dev cycle") && version.contains("Ship it") && version.contains("# Codex"));
        fs::remove_dir_all(data_root).unwrap();
    }

    #[test]
    fn saving_refreshes_enabled_tool_copies() {
        let data_root = test_temp_dir("metadata-data");
        let library_skill = data_root.join("library").join("edit-me");
        fs::create_dir_all(&library_skill).unwrap();
        fs::write(library_skill.join("SKILL.md"), "---\nname: edit-me\n---\n# Body\n").unwrap();
        let codex_root = test_temp_dir("metadata-codex");
        targets::set_skill_target_enabled_with_root(&library_skill, "codex", true, &data_root, &codex_root)
            .unwrap();

        let result = update_skill_metadata_with_roots(
            &library_skill,
            "  Edited   name ",
            "Now with a description",
            &data_root,
            &[("codex", codex_root.clone())],
        )
        .unwrap();

        assert_eq!(result.name, "Edited name");
        assert_eq!(result.refreshed_targets, vec!["Codex".to_string()]);
        let copy = fs::read_to_string(codex_root.join("edit-me").join("SKILL.md")).unwrap();
        assert!(copy.contains("description: \"Now with a description\""));

        fs::remove_dir_all(data_root).unwrap();
        fs::remove_dir_all(codex_root).unwrap();
    }
}
