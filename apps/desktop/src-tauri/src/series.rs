//! Links between local skill folders and the GitHub repository they were
//! published in (`<data root>/series.json`). Many skills are installed by
//! copying folders, so nothing on disk says where they came from; linking a
//! repository once records, for every skill folder in it, the GitHub folder it
//! lives in. The UI groups skills by repository and offers updates from it.
//!
//! Repositories can be linked by hand, or found automatically: "find sources"
//! compares each installed SKILL.md with the ones in well-known skill
//! repositories (by git blob id) and links the skills it recognises.
//!
//! Only this file is written; skill folders themselves are never touched.

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const SERIES_FILE: &str = "series.json";

/// Public repositories that publish many skills. "Find sources" looks for the
/// user's skills in these, plus every repository the user linked by hand.
pub const KNOWN_REPOSITORIES: &[&str] = &[
    "anthropics/skills",
    "openai/skills",
    "mattpocock/skills",
    "nextlevelbuilder/ui-ux-pro-max-skill",
];

#[derive(Debug, Default, Serialize, Deserialize)]
struct SeriesFile {
    #[serde(default)]
    links: BTreeMap<String, String>,
    /// Repositories (`owner/repo`) the user linked by hand.
    #[serde(default)]
    repos: Vec<String>,
}

/// Lower-cased skill folder name -> GitHub URL of that folder.
pub type SeriesLinks = BTreeMap<String, String>;

fn series_path(data_root: &Path) -> PathBuf {
    data_root.join(SERIES_FILE)
}

fn load_file(data_root: &Path) -> SeriesFile {
    fs::read_to_string(series_path(data_root))
        .ok()
        .and_then(|contents| serde_json::from_str::<SeriesFile>(&contents).ok())
        .unwrap_or_default()
}

pub fn load_links(data_root: &Path) -> SeriesLinks {
    load_file(data_root).links
}

pub fn load_default_links() -> SeriesLinks {
    crate::fs_ops::default_data_root()
        .map(|root| load_links(&root))
        .unwrap_or_default()
}

/// The built-in repositories followed by the ones the user linked, no repeats.
pub fn candidate_repositories(data_root: &Path) -> Vec<String> {
    let mut repositories: Vec<String> = KNOWN_REPOSITORIES.iter().map(|repo| repo.to_string()).collect();
    for repo in load_file(data_root).repos {
        if !repositories.iter().any(|known| known.eq_ignore_ascii_case(&repo)) {
            repositories.push(repo);
        }
    }
    repositories
}

/// The linked GitHub folder for a skill folder, if its repository was linked.
pub fn linked_url(links: &SeriesLinks, folder_name: &str) -> Option<String> {
    links.get(&folder_name.to_lowercase()).cloned()
}

fn save_file(data_root: &Path, file: &SeriesFile) -> Result<(), String> {
    fs::create_dir_all(data_root).map_err(|error| format!("Could not create the data root: {error}"))?;
    let json = serde_json::to_string_pretty(file)
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

/// Replace every link into `owner/repo` with `entries` (folder name, URL), and
/// remember the repository so "find sources" checks it too.
pub fn replace_repo_links(
    data_root: &Path,
    owner: &str,
    repo: &str,
    entries: &[(String, String)],
) -> Result<(), String> {
    let prefix = repo_prefix(owner, repo);
    let repository = format!("{owner}/{repo}");
    let mut file = load_file(data_root);
    file.links.retain(|_, url| !belongs_to(url, &prefix));
    for (folder, url) in entries {
        file.links.insert(folder.to_lowercase(), url.clone());
    }
    if !file.repos.iter().any(|known| known.eq_ignore_ascii_case(&repository)) {
        file.repos.push(repository);
    }
    save_file(data_root, &file)
}

/// Point each of `entries` (folder name, URL) at its GitHub folder, keeping
/// every other link as it is.
pub fn set_links(data_root: &Path, entries: &[(String, String)]) -> Result<(), String> {
    let mut file = load_file(data_root);
    for (folder, url) in entries {
        file.links.insert(folder.to_lowercase(), url.clone());
    }
    save_file(data_root, &file)
}

/// Forget every link into `owner/repo`. Returns how many were removed.
pub fn remove_repo_links(data_root: &Path, owner: &str, repo: &str) -> Result<usize, String> {
    let prefix = repo_prefix(owner, repo);
    let repository = format!("{owner}/{repo}");
    let mut file = load_file(data_root);
    let before = file.links.len();
    file.links.retain(|_, url| !belongs_to(url, &prefix));
    file.repos.retain(|known| !known.eq_ignore_ascii_case(&repository));
    let removed = before - file.links.len();
    save_file(data_root, &file)?;
    Ok(removed)
}

/// Git's id for a file's content (`sha1("blob <len>\0" + bytes)`), as the
/// GitHub trees API reports it.
pub fn git_blob_id(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", bytes.len()).as_bytes());
    hasher.update(bytes);
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Blob ids a local SKILL.md could have upstream: as is, and with Windows line
/// endings turned back into `\n` (git may have converted them on checkout).
pub fn local_blob_ids(bytes: &[u8]) -> Vec<String> {
    let mut ids = vec![git_blob_id(bytes)];
    if bytes.windows(2).any(|pair| pair == b"\r\n") {
        let text = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
        ids.push(git_blob_id(text.as_bytes()));
    }
    ids
}

/// A skill folder found in a repository.
#[derive(Debug, Clone)]
pub struct RemoteSkill {
    pub repository: String,
    pub folder: String,
    pub url: String,
    /// Blob id of its SKILL.md.
    pub blob: String,
}

/// An installed skill folder (every copy of it, across tool folders).
#[derive(Debug, Clone)]
pub struct LocalSkill {
    pub folder: String,
    pub blobs: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SourceMatch {
    /// Its SKILL.md is byte-for-byte a SKILL.md in this repository.
    Exact(usize),
    /// Only one repository has a skill folder with this name.
    ByName(usize),
    /// Several repositories have a folder with this name; none has this content.
    Ambiguous,
    None,
}

/// Where an installed skill most likely came from. Identical content wins (a
/// folder with the same name is preferred, then the earlier repository); a
/// matching name is only trusted when one repository has it.
pub fn match_source(local: &LocalSkill, remotes: &[RemoteSkill]) -> SourceMatch {
    let same_name = |remote: &RemoteSkill| remote.folder.eq_ignore_ascii_case(&local.folder);
    let identical: Vec<usize> = (0..remotes.len())
        .filter(|&index| local.blobs.iter().any(|blob| blob == &remotes[index].blob))
        .collect();
    // The same content is often mirrored by large collections: prefer the
    // same folder name, then the most focused repository (fewest skills).
    let repo_size = |repository: &str| remotes.iter().filter(|remote| remote.repository == repository).count();
    if let Some(index) = identical
        .iter()
        .copied()
        .min_by_key(|&index| (!same_name(&remotes[index]), repo_size(&remotes[index].repository)))
    {
        return SourceMatch::Exact(index);
    }

    let named: Vec<usize> = (0..remotes.len()).filter(|&index| same_name(&remotes[index])).collect();
    let mut repositories: Vec<String> = named.iter().map(|&index| remotes[index].repository.to_lowercase()).collect();
    repositories.sort();
    repositories.dedup();
    match (named.first(), repositories.len()) {
        (Some(&index), 1) => SourceMatch::ByName(index),
        (Some(_), _) => SourceMatch::Ambiguous,
        (None, _) => SourceMatch::None,
    }
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
            // .github/.git hold tooling, not skills. Other dot folders can be
            // real collections (openai/skills keeps skills in .curated/).
            if dir.split('/').any(|segment| segment.starts_with(".git")) {
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

    fn remote(repository: &str, folder: &str, blob: &str) -> RemoteSkill {
        RemoteSkill {
            repository: repository.to_string(),
            folder: folder.to_string(),
            url: format!("https://github.com/{repository}/tree/main/{folder}"),
            blob: blob.to_string(),
        }
    }

    #[test]
    fn blob_ids_match_git_and_ignore_windows_line_endings() {
        // `printf 'hello\n' | git hash-object --stdin`
        assert_eq!(git_blob_id(b"hello\n"), "ce013625030ba8dba906f756967f9e9ca394464a");
        assert!(local_blob_ids(b"hello\r\n").contains(&"ce013625030ba8dba906f756967f9e9ca394464a".to_string()));
        assert_eq!(local_blob_ids(b"hello\n").len(), 1);
    }

    #[test]
    fn identical_content_wins_and_names_are_trusted_only_when_unique() {
        let remotes = vec![
            remote("anthropics/skills", "pdf", "aaa"),
            remote("openai/skills", "pdf", "bbb"),
            remote("mattpocock/skills", "tdd", "ccc"),
            remote("mattpocock/skills", "renamed-upstream", "ddd"),
        ];
        let local = |folder: &str, blob: &str| LocalSkill { folder: folder.to_string(), blobs: vec![blob.to_string()] };

        assert_eq!(match_source(&local("pdf", "bbb"), &remotes), SourceMatch::Exact(1));
        assert_eq!(match_source(&local("my-copy", "ddd"), &remotes), SourceMatch::Exact(3));
        assert_eq!(match_source(&local("PDF", "old"), &remotes), SourceMatch::Ambiguous);
        assert_eq!(match_source(&local("tdd", "old"), &remotes), SourceMatch::ByName(2));
        assert_eq!(match_source(&local("mine", "zzz"), &remotes), SourceMatch::None);
    }

    #[test]
    fn identical_copies_prefer_the_most_focused_repository() {
        let mut remotes = vec![remote("collector/awesome", "remotion-render", "same")];
        for index in 0..30 {
            remotes.push(remote("collector/awesome", &format!("other-{index}"), &format!("o{index}")));
        }
        remotes.push(remote("remotion-dev/skills", "remotion-render", "same"));
        remotes.push(remote("remotion-dev/skills", "remotion-docs", "d"));
        let local = LocalSkill { folder: "remotion-render".to_string(), blobs: vec!["same".to_string()] };
        assert_eq!(match_source(&local, &remotes), SourceMatch::Exact(31));
    }

    #[test]
    fn hand_linked_repositories_join_the_candidates() {
        let root = test_temp_dir("series-candidates");
        replace_repo_links(&root, "me", "kit", &[]).unwrap();
        replace_repo_links(&root, "Anthropics", "Skills", &[]).unwrap();
        let candidates = candidate_repositories(&root);
        assert_eq!(candidates.len(), KNOWN_REPOSITORIES.len() + 1);
        assert_eq!(candidates.last().map(String::as_str), Some("me/kit"));
        remove_repo_links(&root, "me", "kit").unwrap();
        assert_eq!(candidate_repositories(&root).len(), KNOWN_REPOSITORIES.len());
        let _ = fs::remove_dir_all(root);
    }
}
