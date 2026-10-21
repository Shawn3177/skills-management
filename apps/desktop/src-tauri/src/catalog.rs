//! "My GitHub": the connected account's own repositories and the ones it
//! starred, each with the skills (folders with a SKILL.md) it holds.
//! The result is cached in `<data root>/github-catalog.json` so the app can
//! show it, and use it to find skill sources, without asking GitHub again.

use crate::fs_ops::{default_data_root, unix_ms};
use crate::github::{self, GithubError};
use crate::series::{self, RemoteSkill};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

const CATALOG_FILE: &str = "github-catalog.json";
/// At most this many pages of 100 for each list (own repos, stars).
const MAX_PAGES: usize = 10;
const WORKERS: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSkill {
    pub folder: String,
    /// Folder relative to the repository root ("" for a repository that is one skill).
    pub path: String,
    pub url: String,
    /// Blob id of its SKILL.md.
    pub blob: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRepo {
    pub full_name: String,
    pub description: Option<String>,
    pub html_url: String,
    pub default_branch: String,
    pub owned: bool,
    pub starred: bool,
    pub fork: bool,
    pub stars: u64,
    pub skills: Vec<CatalogSkill>,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub language: Option<String>,
    /// Last push, as GitHub reports it (ISO 8601).
    #[serde(default)]
    pub pushed_at: Option<String>,
    #[serde(skip)]
    pub empty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubCatalog {
    pub login: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    pub fetched_at: u128,
    /// Every own and starred repository, own ones first.
    pub repos: Vec<CatalogRepo>,
    /// Every starred repository (`owner/repo`), with or without skills.
    pub starred: Vec<String>,
    /// How many repositories were looked into.
    pub scanned: usize,
    /// GitHub's limit stopped the scan early; some repositories are missing.
    pub rate_limited: bool,
}

fn catalog_path(data_root: &Path) -> PathBuf {
    data_root.join(CATALOG_FILE)
}

pub fn load_cache(data_root: &Path) -> Option<GithubCatalog> {
    serde_json::from_str(&fs::read_to_string(catalog_path(data_root)).ok()?).ok()
}

pub fn forget_cache() {
    if let Ok(root) = default_data_root() {
        let _ = fs::remove_file(catalog_path(&root));
    }
}

fn save_cache(data_root: &Path, catalog: &GithubCatalog) -> Result<(), String> {
    fs::create_dir_all(data_root).map_err(|error| format!("Could not create the data root: {error}"))?;
    let json = serde_json::to_string_pretty(catalog)
        .map_err(|error| format!("Could not serialize the GitHub list: {error}"))?;
    let partial = data_root.join(format!("{CATALOG_FILE}.partial"));
    fs::write(&partial, json).map_err(|error| format!("Could not save the GitHub list: {error}"))?;
    fs::rename(&partial, catalog_path(data_root)).map_err(|error| format!("Could not save the GitHub list: {error}"))
}

/// The cached list, or (with `refresh`) a fresh one from GitHub.
pub fn github_catalog(refresh: bool) -> Result<Option<GithubCatalog>, String> {
    let data_root = default_data_root()?;
    if !refresh {
        return Ok(load_cache(&data_root));
    }
    if github::github_token().is_none() {
        return Err("Connect your GitHub account first (Settings › GitHub account).".to_string());
    }
    let catalog = fetch_catalog()?;
    save_cache(&data_root, &catalog)?;
    Ok(Some(catalog))
}

/// Skills in the cached repositories, for "find sources".
pub fn cached_remote_skills(data_root: &Path) -> Vec<RemoteSkill> {
    load_cache(data_root)
        .map(|catalog| {
            catalog
                .repos
                .into_iter()
                .flat_map(|repo| {
                    let repository = repo.full_name.clone();
                    repo.skills.into_iter().map(move |skill| RemoteSkill {
                        repository: repository.clone(),
                        folder: skill.folder,
                        url: skill.url,
                        blob: skill.blob,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn fetch_catalog() -> Result<GithubCatalog, String> {
    let agent = github::build_agent()?;
    let user = github::api_get_json(&agent, "https://api.github.com/user").map_err(|error| error.message())?;
    let login = user.get("login").and_then(|value| value.as_str()).unwrap_or_default().to_string();
    let name = text(&user, "name");
    let avatar_url = text(&user, "avatar_url");

    let owned = list_repos(&agent, "https://api.github.com/user/repos?affiliation=owner&sort=updated")?;
    let starred = list_repos(&agent, "https://api.github.com/user/starred?sort=updated")?;
    let starred_names: Vec<String> = starred.iter().filter_map(|repo| text(repo, "full_name")).collect();

    let mut candidates: Vec<CatalogRepo> = Vec::new();
    for (repo, is_owned) in owned.iter().map(|repo| (repo, true)).chain(starred.iter().map(|repo| (repo, false))) {
        let Some(full_name) = text(repo, "full_name") else { continue };
        if let Some(existing) = candidates.iter_mut().find(|known| known.full_name.eq_ignore_ascii_case(&full_name)) {
            existing.owned |= is_owned;
            continue;
        }
        candidates.push(CatalogRepo {
            starred: starred_names.iter().any(|name| name.eq_ignore_ascii_case(&full_name)),
            html_url: text(repo, "html_url").unwrap_or_else(|| format!("https://github.com/{full_name}")),
            default_branch: text(repo, "default_branch").unwrap_or_else(|| "main".to_string()),
            description: text(repo, "description"),
            fork: repo.get("fork").and_then(|value| value.as_bool()).unwrap_or(false),
            stars: repo.get("stargazers_count").and_then(|value| value.as_u64()).unwrap_or(0),
            owned: is_owned,
            private: repo.get("private").and_then(|value| value.as_bool()).unwrap_or(false),
            language: text(repo, "language"),
            pushed_at: text(repo, "pushed_at"),
            empty: repo.get("size").and_then(|value| value.as_u64()) == Some(0),
            full_name,
            skills: Vec::new(),
        });
    }

    // Read each repository's file list (one request each), a few at a time.
    let next = AtomicUsize::new(0);
    let limited = AtomicBool::new(false);
    let found: Mutex<Vec<(usize, Vec<CatalogSkill>)>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..WORKERS {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::SeqCst);
                if index >= candidates.len() || limited.load(Ordering::SeqCst) {
                    break;
                }
                // An empty repository has no tree to read.
                if candidates[index].empty {
                    continue;
                }
                match repo_skills(&agent, &candidates[index]) {
                    Ok(skills) if !skills.is_empty() => found.lock().unwrap().push((index, skills)),
                    Ok(_) => {}
                    Err(GithubError::RateLimited) => limited.store(true, Ordering::SeqCst),
                    // Moved, emptied or unreadable: just leave it out.
                    Err(_) => {}
                }
            });
        }
    });
    for (index, skills) in found.into_inner().unwrap_or_default() {
        candidates[index].skills = skills;
    }

    let scanned = next.load(Ordering::SeqCst).min(candidates.len());
    let mut repos: Vec<CatalogRepo> = candidates;
    // Own repositories first, then the most recently pushed.
    repos.sort_by(|left, right| {
        right.owned.cmp(&left.owned).then_with(|| right.pushed_at.cmp(&left.pushed_at))
    });
    Ok(GithubCatalog {
        login,
        name,
        avatar_url,
        fetched_at: unix_ms(),
        repos,
        starred: starred_names,
        scanned,
        rate_limited: limited.load(Ordering::SeqCst),
    })
}

fn text(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|item| item.as_str()).filter(|item| !item.is_empty()).map(str::to_string)
}

/// Every page (100 a page, up to `MAX_PAGES`) of a repository list.
fn list_repos(agent: &ureq::Agent, url: &str) -> Result<Vec<serde_json::Value>, String> {
    let mut repos = Vec::new();
    for page in 1..=MAX_PAGES {
        let json = github::api_get_json(agent, &format!("{url}&per_page=100&page={page}"))
            .map_err(|error| error.message())?;
        let items = json.as_array().cloned().unwrap_or_default();
        let last = items.len() < 100;
        repos.extend(items);
        if last {
            break;
        }
    }
    Ok(repos)
}

fn repo_skills(agent: &ureq::Agent, repo: &CatalogRepo) -> Result<Vec<CatalogSkill>, GithubError> {
    let (owner, name) = repo.full_name.split_once('/').unwrap_or((&repo.full_name, ""));
    let (files, _truncated) = github::repo_files(agent, owner, name, &repo.default_branch)?;
    Ok(skills_in_files(&files, owner, name, &repo.default_branch))
}

/// The skill folders among a repository's files, with their SKILL.md blob ids.
pub fn skills_in_files(files: &[(String, String)], owner: &str, repo: &str, branch: &str) -> Vec<CatalogSkill> {
    let paths: Vec<String> = files.iter().map(|(path, _)| path.clone()).collect();
    series::skill_folders_in_tree(&paths, "", repo)
        .into_iter()
        .filter_map(|(folder, dir)| {
            let skill_md = if dir.is_empty() { "SKILL.md".to_string() } else { format!("{dir}/SKILL.md") };
            let blob = files.iter().find(|(path, _)| *path == skill_md)?.1.clone();
            Some(CatalogSkill { url: github::canonical_url(owner, repo, branch, &dir), folder, path: dir, blob })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_skills_with_their_blob_ids() {
        let files = vec![
            ("README.md".to_string(), "r".to_string()),
            ("skills/pdf/SKILL.md".to_string(), "p1".to_string()),
            ("skills/pdf/run.py".to_string(), "p2".to_string()),
            ("tdd/SKILL.md".to_string(), "t1".to_string()),
        ];
        let skills = skills_in_files(&files, "me", "kit", "main");
        let summary: Vec<(&str, &str, &str)> =
            skills.iter().map(|skill| (skill.folder.as_str(), skill.blob.as_str(), skill.url.as_str())).collect();
        assert_eq!(
            summary,
            vec![
                ("pdf", "p1", "https://github.com/me/kit/tree/main/skills/pdf"),
                ("tdd", "t1", "https://github.com/me/kit/tree/main/tdd"),
            ]
        );
    }

    #[test]
    #[ignore = "hits the live GitHub API"]
    fn live_lists_starred_repositories_and_their_skills() {
        let agent = github::build_agent().unwrap();
        // A public list reads the same way as the signed-in /user/starred.
        let starred = list_repos(&agent, "https://api.github.com/users/octocat/starred?sort=updated").unwrap();
        assert!(starred.iter().all(|repo| text(repo, "full_name").is_some()));

        let repo = CatalogRepo {
            full_name: "anthropics/skills".to_string(),
            description: None,
            html_url: String::new(),
            default_branch: "main".to_string(),
            owned: false,
            starred: true,
            fork: false,
            stars: 0,
            skills: Vec::new(),
            private: false,
            language: None,
            pushed_at: None,
            empty: false,
        };
        let skills = repo_skills(&agent, &repo).unwrap();
        assert!(skills.iter().any(|skill| skill.folder == "pdf" && skill.blob.len() == 40));
    }
}
