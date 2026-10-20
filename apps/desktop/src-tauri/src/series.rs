//! Links between local skill folders and the GitHub repository they were
//! published in (`<data root>/series.json`). Many skills are installed by
//! copying folders, so nothing on disk says where they came from; linking a
//! repository once records, for every skill folder in it, the GitHub folder it
//! lives in. The UI groups skills by repository and offers updates from it.
//!
//! Only this file is written; skill folders themselves are never touched.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const SERIES_FILE: &str = "series.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct SeriesFile {
    #[serde(default)]
    links: BTreeMap<String, String>,
}

/// Lower-cased skill folder name -> GitHub URL of that folder.
pub type SeriesLinks = BTreeMap<String, String>;

fn series_path(data_root: &Path) -> PathBuf {
    data_root.join(SERIES_FILE)
}

pub fn load_links(data_root: &Path) -> SeriesLinks {
    fs::read_to_string(series_path(data_root))
        .ok()
        .and_then(|contents| serde_json::from_str::<SeriesFile>(&contents).ok())
        .map(|file| file.links)
        .unwrap_or_default()
}

pub fn load_default_links() -> SeriesLinks {
    crate::fs_ops::default_data_root()
        .map(|root| load_links(&root))
        .unwrap_or_default()
}

/// The linked GitHub folder for a skill folder, if its repository was linked.
pub fn linked_url(links: &SeriesLinks, folder_name: &str) -> Option<String> {
    links.get(&folder_name.to_lowercase()).cloned()
}

fn save_links(data_root: &Path, links: SeriesLinks) -> Result<(), String> {
    fs::create_dir_all(data_root).map_err(|error| format!("Could not create the data root: {error}"))?;
    let json = serde_json::to_string_pretty(&SeriesFile { links })
        .map_err(|error| format!("Could not serialize repository links: {error}"))?;
    // Write a sibling file first so a crash never leaves a half-written store.
    let partial = data_root.join(format!("{SERIES_FILE}.partial"));
    fs::write(&partial, json).map_err(|error| format!("Could not save repository links: {error}"))?;
    fs::rename(&partial, series_path(data_root))
        .map_err(|error| format!("Could not save repository links: {error}"))
}

fn repo_prefix(owner: &str, repo: &str) -> String {
    format!("https://github.com/{owner}/{repo}/").to_lowercase()
}

fn belongs_to(url: &str, prefix: &str) -> bool {
    format!("{}/", url.trim_end_matches('/')).to_lowercase().starts_with(prefix)
}

/// Replace every link into `owner/repo` with `entries` (folder name, URL).
pub fn replace_repo_links(
    data_root: &Path,
    owner: &str,
    repo: &str,
    entries: &[(String, String)],
) -> Result<(), String> {
    let prefix = repo_prefix(owner, repo);
    let mut links = load_links(data_root);
    links.retain(|_, url| !belongs_to(url, &prefix));
    for (folder, url) in entries {
        links.insert(folder.to_lowercase(), url.clone());
    }
    save_links(data_root, links)
}

/// Forget every link into `owner/repo`. Returns how many were removed.
pub fn remove_repo_links(data_root: &Path, owner: &str, repo: &str) -> Result<usize, String> {
    let prefix = repo_prefix(owner, repo);
    let mut links = load_links(data_root);
    let before = links.len();
    links.retain(|_, url| !belongs_to(url, &prefix));
    let removed = before - links.len();
    if removed > 0 {
        save_links(data_root, links)?;
    }
    Ok(removed)
}

pub fn unlink_github_series(repository: String) -> Result<usize, String> {
    let (owner, repo) = repository
        .trim()
        .trim_start_matches("https://github.com/")
        .split_once('/')
        .map(|(owner, rest)| (owner.to_string(), rest.split('/').next().unwrap_or("").to_string()))
        .filter(|(owner, repo)| !owner.is_empty() && !repo.is_empty())
        .ok_or_else(|| "Expected a repository like owner/repo.".to_string())?;
    remove_repo_links(&crate::fs_ops::default_data_root()?, &owner, &repo)
}

/// From a recursive git tree listing, every folder (under `subdir`) that holds
/// a SKILL.md: (folder name, folder path relative to the repository root).
pub fn skill_folders_in_tree(paths: &[String], subdir: &str, repo: &str) -> Vec<(String, String)> {
    let scope = subdir.trim_matches('/');
    let mut folders: Vec<(String, String)> = paths
        .iter()
        .filter_map(|path| {
            let dir = if path == "SKILL.md" {
                ""
            } else {
                path.strip_suffix("/SKILL.md")?
            };
            let inside = scope.is_empty() || dir == scope || dir.starts_with(&format!("{scope}/"));
            if !inside {
                return None;
            }
            let name = if dir.is_empty() { repo } else { dir.rsplit('/').next().unwrap_or(dir) };
            // Hidden folders (.github, .curated …) hold tooling, not skills.
            if dir.split('/').any(|segment| segment.starts_with('.')) {
                return None;
            }
            Some((name.to_string(), dir.to_string()))
        })
        .collect();
    folders.sort();
    // If a repository has two folders with the same name, keep the shallower.
    folders.sort_by_key(|(name, dir)| (name.to_lowercase(), dir.matches('/').count()));
    folders.dedup_by(|later, earlier| later.0.eq_ignore_ascii_case(&earlier.0));
    folders
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs_ops::test_temp_dir;

    #[test]
    fn finds_skill_folders_inside_the_requested_subdir() {
        let paths = [
            "README.md",
            "skills/pdf/SKILL.md",
            "skills/pdf/scripts/run.py",
            "skills/docx/SKILL.md",
            "other/tool/SKILL.md",
            ".github/skill/SKILL.md",
        ]
        .map(String::from);
        let all = skill_folders_in_tree(&paths, "", "skills");
        assert_eq!(
            all,
            vec![
                ("docx".to_string(), "skills/docx".to_string()),
                ("pdf".to_string(), "skills/pdf".to_string()),
                ("tool".to_string(), "other/tool".to_string()),
            ]
        );
        let scoped = skill_folders_in_tree(&paths, "skills/", "skills");
        assert_eq!(scoped.len(), 2);
        let root = skill_folders_in_tree(&["SKILL.md".to_string()], "", "agent-reach");
        assert_eq!(root, vec![("agent-reach".to_string(), String::new())]);
    }

    #[test]
    fn linking_a_repository_replaces_only_its_own_links() {
        let root = test_temp_dir("series-links");
        replace_repo_links(
            &root,
            "anthropics",
            "skills",
            &[("PDF".to_string(), "https://github.com/anthropics/skills/tree/main/skills/pdf".to_string())],
        )
        .unwrap();
        replace_repo_links(
            &root,
            "someone",
            "kit",
            &[("brand".to_string(), "https://github.com/someone/kit/tree/main/brand".to_string())],
        )
        .unwrap();
        // Re-linking a repository drops folders that are no longer in it.
        replace_repo_links(
            &root,
            "Anthropics",
            "Skills",
            &[("docx".to_string(), "https://github.com/anthropics/skills/tree/main/skills/docx".to_string())],
        )
        .unwrap();

        let links = load_links(&root);
        assert_eq!(linked_url(&links, "pdf"), None);
        assert!(linked_url(&links, "DOCX").is_some());
        assert!(linked_url(&links, "brand").is_some());

        assert_eq!(remove_repo_links(&root, "someone", "kit").unwrap(), 1);
        assert_eq!(linked_url(&load_links(&root), "brand"), None);
        let _ = fs::remove_dir_all(root);
    }
}
