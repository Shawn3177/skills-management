//! Public discovery is read-only until a server-held, commit-pinned preview is imported.
use crate::fs_ops::{self, default_data_root, display_path};
use crate::github::{self, GithubSource};
use crate::snapshot::{self, TreeFile};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const MAX_RESPONSE: u64 = 2 * 1024 * 1024;
const MAX_MARKDOWN: u64 = 256 * 1024;

struct Cache<T> {
    entries: VecDeque<(String, Instant, T)>,
    capacity: usize,
    ttl: Duration,
}

impl<T: Clone> Cache<T> {
    fn new(capacity: usize, seconds: u64) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity,
            ttl: Duration::from_secs(seconds),
        }
    }
    fn get(&mut self, key: &str) -> Option<T> {
        self.entries.retain(|(_, at, _)| at.elapsed() < self.ttl);
        self.entries
            .iter()
            .find(|(id, _, _)| id == key)
            .map(|(_, _, value)| value.clone())
    }
    fn put(&mut self, key: String, value: T) {
        self.entries
            .retain(|(id, at, _)| id != &key && at.elapsed() < self.ttl);
        while self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back((key, Instant::now(), value));
    }
}

static SEARCHES: LazyLock<Mutex<Cache<SearchResults>>> =
    LazyLock::new(|| Mutex::new(Cache::new(32, 60)));
static REPOSITORIES: LazyLock<Mutex<Cache<Repository>>> =
    LazyLock::new(|| Mutex::new(Cache::new(8, 1800)));
static PREVIEWS: LazyLock<Mutex<Cache<GithubSource>>> =
    LazyLock::new(|| Mutex::new(Cache::new(32, 1800)));

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicSkill {
    id: String,
    source: String,
    skill_id: String,
    name: String,
    installs: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    skills: Vec<PublicSkill>,
    unsupported: usize,
}

fn repository_name(source: &str) -> Result<github::GithubRef, String> {
    let parsed = github::parse_github_url(&format!("https://github.com/{source}"))?;
    if source != format!("{}/{}", parsed.owner, parsed.repo) {
        return Err("Only GitHub repository sources are supported.".into());
    }
    Ok(parsed)
}

fn clean_text(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(256).collect()
}

fn parse_results(bytes: &[u8]) -> Result<SearchResults, String> {
    #[derive(Deserialize)]
    struct Response {
        skills: Vec<serde_json::Value>,
    }
    let response: Response = serde_json::from_slice(bytes).map_err(|_| {
        "The public search service returned an invalid response. Please retry.".to_string()
    })?;
    let mut seen = HashSet::new();
    let mut result = SearchResults {
        skills: Vec::new(),
        unsupported: 0,
    };
    for row in response.skills.into_iter().take(100) {
        let source = row["source"].as_str().unwrap_or("");
        if repository_name(source).is_err() {
            result.unsupported += 1;
            continue;
        }
        let slug = row["skillId"]
            .as_str()
            .or_else(|| row["id"].as_str()?.strip_prefix(&format!("{source}/")));
        let Some(slug) = slug.filter(|s| {
            !s.is_empty()
                && s.len() <= 256
                && !s.contains(['/', '\\'])
                && !s.chars().any(char::is_control)
                && *s != "."
                && *s != ".."
        }) else {
            result.unsupported += 1;
            continue;
        };
        let id = format!("{source}/{slug}");
        if !seen.insert(id.clone()) {
            continue;
        }
        result.skills.push(PublicSkill {
            id,
            source: source.into(),
            skill_id: slug.into(),
            name: clean_text(
                row["name"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(slug),
            ),
            installs: row["installs"].as_u64().unwrap_or(0),
        });
    }
    Ok(result)
}

fn search_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(429, response) => format!(
            "Public search is rate limited. Retry after {} seconds.",
            response.header("Retry-After").unwrap_or("60")
        ),
        ureq::Error::Status(code, _) => {
            format!("Public search is unavailable (HTTP {code}). Please retry.")
        }
        ureq::Error::Transport(_) => {
            "Could not reach public search. Check your connection and retry.".into()
        }
    }
}

pub fn search_public_skills(query: String) -> Result<SearchResults, String> {
    let query = query.trim();
    if !(2..=120).contains(&query.chars().count()) || query.chars().any(char::is_control) {
        return Err("Enter between 2 and 120 characters to search.".into());
    }
    if let Some(result) = SEARCHES.lock().map_err(|e| e.to_string())?.get(query) {
        return Ok(result);
    }
    // Deliberately do not use api_get_json: GitHub credentials must never go to skills.sh.
    let response = github::build_agent()?
        .get("https://skills.sh/api/search")
        .query("q", query)
        .query("limit", "30")
        .set("User-Agent", "skills-manage")
        .timeout(Duration::from_secs(20))
        .call()
        .map_err(search_error)?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_RESPONSE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_RESPONSE {
        return Err("Public search returned too much data.".into());
    }
    let result = parse_results(&bytes)?;
    SEARCHES
        .lock()
        .map_err(|e| e.to_string())?
        .put(query.into(), result.clone());
    Ok(result)
}

#[derive(Clone)]
struct Repository {
    source: String,
    branch: String,
    commit: String,
    files: Vec<TreeFile>,
    paths: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicResolution {
    repository_id: String,
    paths: Vec<String>,
    suggested_path: Option<String>,
}

fn skill_paths(files: &[TreeFile], truncated: bool) -> Result<Vec<String>, String> {
    if truncated || files.len() > snapshot::MAX_ENTRIES {
        return Err("This repository is too large to resolve safely. Use a specific GitHub skill URL instead.".into());
    }
    let mut paths = Vec::new();
    for file in files {
        if !fs_ops::is_safe_relative(&file.path)
            || file.path.split('/').any(fs_ops::is_excluded_entry)
        {
            continue;
        }
        if file.path == "SKILL.md" {
            paths.push(String::new());
        } else if let Some(path) = file.path.strip_suffix("/SKILL.md") {
            paths.push(path.into());
        }
    }
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        return Err(
            "No SKILL.md was found in this repository. The search index may be outdated.".into(),
        );
    }
    Ok(paths)
}

fn suggested_path(paths: &[String], slug: &str) -> Option<String> {
    let matches: Vec<_> = paths
        .iter()
        .filter(|path| path.rsplit('/').next() == Some(slug))
        .collect();
    if matches.len() == 1 {
        return Some(matches[0].clone());
    }
    // A root skill has no folder name. Its actual name and content are still reviewed next.
    if paths.len() == 1 && paths[0].is_empty() {
        return Some(String::new());
    }
    None
}

fn identifier(parts: &[&str]) -> String {
    let bytes = serde_json::to_vec(parts).expect("strings serialize");
    format!("{:x}", Sha256::digest(bytes))
}

pub fn resolve_public_skill(source: String, skill_id: String) -> Result<PublicResolution, String> {
    let parsed = repository_name(&source)?;
    let agent = github::build_agent()?;
    let branch = github::resolve_default_branch(&agent, &parsed.owner, &parsed.repo)
        .map_err(|e| e.message())?;
    let commit = github::latest_commit(&agent, &parsed.owner, &parsed.repo, &branch, "")
        .map_err(|e| e.message())?;
    let (files, truncated) = github::repo_tree_files(&agent, &parsed.owner, &parsed.repo, &commit)
        .map_err(|e| e.message())?;
    let paths = skill_paths(&files, truncated)?;
    let suggested_path = suggested_path(&paths, &skill_id);
    let id = identifier(&[&source, &branch, &commit]);
    REPOSITORIES.lock().map_err(|e| e.to_string())?.put(
        id.clone(),
        Repository {
            source,
            branch,
            commit,
            files,
            paths: paths.clone(),
        },
    );
    Ok(PublicResolution {
        repository_id: id,
        paths,
        suggested_path,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicPreview {
    preview_id: String,
    name: String,
    description: String,
    source: String,
    path: String,
    commit: String,
    url: String,
    markdown: String,
    files: Vec<String>,
    installed_path: Option<String>,
}

fn installed_source(source: &GithubSource, data_root: &Path) -> Option<PathBuf> {
    fs::read_dir(data_root.join("library"))
        .ok()?
        .flatten()
        .filter(|entry| !fs_ops::is_link(entry) && entry.path().join("SKILL.md").is_file())
        .find_map(|entry| {
            let known = github::read_source_file(&entry.path())?;
            (known.owner.eq_ignore_ascii_case(&source.owner)
                && known.repo.eq_ignore_ascii_case(&source.repo)
                && known.subdir == source.subdir)
                .then(|| entry.path())
        })
}

pub fn preview_public_skill(repository_id: String, path: String) -> Result<PublicPreview, String> {
    let repo = REPOSITORIES
        .lock()
        .map_err(|e| e.to_string())?
        .get(&repository_id)
        .ok_or("This repository selection expired. Select the search result again.")?;
    if !repo.paths.contains(&path) {
        return Err("Select a skill folder from the repository list.".into());
    }
    let parsed = repository_name(&repo.source)?;
    let prefix = if path.is_empty() {
        String::new()
    } else {
        format!("{path}/")
    };
    let file = repo
        .files
        .iter()
        .find(|file| file.path == format!("{prefix}SKILL.md"))
        .ok_or("SKILL.md is missing.")?;
    if file.size > MAX_MARKDOWN {
        return Err("SKILL.md is too large to preview.".into());
    }
    let bytes = github::fetch_blob(
        &github::build_agent()?,
        &parsed.owner,
        &parsed.repo,
        &file.sha,
    )
    .map_err(|e| e.message())?;
    if bytes.len() as u64 > MAX_MARKDOWN {
        return Err("SKILL.md is too large to preview.".into());
    }
    let markdown = String::from_utf8(bytes).map_err(|_| "SKILL.md is not valid UTF-8.")?;
    let metadata = crate::scanner::parse_skill_metadata(&markdown);
    // Update checks compare the last commit touching this folder. Resolve it
    // against the pinned tree, not a moving branch; its folder contents match
    // the preview, while unrelated repository changes do not signal updates.
    let commit = github::latest_commit(
        &github::build_agent()?,
        &parsed.owner,
        &parsed.repo,
        &repo.commit,
        &path,
    )
    .map_err(|e| e.message())?;
    let files = repo
        .files
        .iter()
        .filter_map(|file| file.path.strip_prefix(&prefix))
        .filter(|relative| {
            fs_ops::is_safe_relative(relative)
                && !relative
                    .split('/')
                    .any(|part| fs_ops::is_excluded_entry(part) || fs_ops::is_internal_marker(part))
        })
        .map(str::to_string)
        .collect();
    let url = github::canonical_url(&parsed.owner, &parsed.repo, &repo.branch, &path);
    let source = GithubSource {
        kind: "github".into(),
        owner: parsed.owner,
        repo: parsed.repo,
        git_ref: repo.branch,
        subdir: path.clone(),
        synced_commit: commit.clone(),
        synced_at: fs_ops::unix_ms(),
        url: url.clone(),
        variants: BTreeMap::new(),
    };
    let installed_path =
        installed_source(&source, &default_data_root()?).map(|path| display_path(&path));
    let preview_id = identifier(&[&repository_id, &path]);
    PREVIEWS
        .lock()
        .map_err(|e| e.to_string())?
        .put(preview_id.clone(), source);
    Ok(PublicPreview {
        preview_id,
        name: clean_text(&metadata.name.unwrap_or_else(|| {
            path.rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or(&repo.source)
                .into()
        })),
        description: metadata.description.unwrap_or_default(),
        source: repo.source,
        path,
        commit,
        url,
        markdown,
        files,
        installed_path,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicImportResult {
    pub library_path: String,
    already_installed: bool,
}

pub fn import_public_skill(preview_id: String) -> Result<PublicImportResult, String> {
    let source = PREVIEWS
        .lock()
        .map_err(|e| e.to_string())?
        .get(&preview_id)
        .ok_or("This preview expired. Preview the skill again before importing.")?;
    let data_root = default_data_root()?;
    if let Some(path) = installed_source(&source, &data_root) {
        return Ok(PublicImportResult {
            library_path: display_path(&path),
            already_installed: true,
        });
    }
    let result = github::import_previewed_skill(&source, &data_root)?;
    Ok(PublicImportResult {
        library_path: result.library_path,
        already_installed: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_normalizes_deduplicates_and_rejects_non_github_sources() {
        let result = parse_results(br#"{"skills":[{"id":"owner/repo/pdf","source":"owner/repo","name":"PDF","installs":5},{"source":"owner/repo","skillId":"pdf"},{"source":"domain.com","skillId":"pdf"},{"source":"owner/repo/../../other","skillId":"pdf"}]}"#).unwrap();
        assert_eq!(result.skills.len(), 1);
        assert_eq!(result.skills[0].skill_id, "pdf");
        assert_eq!(result.skills[0].installs, 5);
        assert_eq!(result.unsupported, 2);
        assert!(parse_results(br#"{"error":"service unavailable"}"#).is_err());
        assert!(parse_results(b"<html>error</html>").is_err());
        assert!(search_public_skills(" ".into()).is_err());
        assert!(search_public_skills("x".repeat(121)).is_err());
    }

    #[test]
    fn resolves_real_paths_without_guessing_ambiguous_or_missing_names() {
        let files = [
            "skills/pdf/SKILL.md",
            ".claude/skills/pdf/SKILL.md",
            "SKILL.md",
            "node_modules/pdf/SKILL.md",
            "../escape/SKILL.md",
        ]
        .map(|path| TreeFile {
            path: path.into(),
            sha: "a".repeat(40),
            size: 10,
        });
        let paths = skill_paths(&files, false).unwrap();
        assert_eq!(paths, ["", ".claude/skills/pdf", "skills/pdf"]);
        assert_eq!(suggested_path(&paths, "pdf"), None);
        assert_eq!(
            suggested_path(&["skills/pdf".into()], "pdf"),
            Some("skills/pdf".into())
        );
        assert_eq!(suggested_path(&["skills/other".into()], "pdf"), None);
        assert!(skill_paths(&files, true).is_err());
        assert!(skill_paths(&[], false).is_err());
    }

    #[test]
    fn cache_is_bounded_and_expires_without_sleeping() {
        let mut cache = Cache::new(2, 60);
        cache.put("a".into(), 1);
        cache.put("b".into(), 2);
        cache.put("c".into(), 3);
        assert_eq!(cache.get("a"), None);
        assert_eq!(cache.get("b"), Some(2));
        cache.ttl = Duration::ZERO;
        assert_eq!(cache.get("c"), None);
    }

    #[test]
    fn installed_identity_uses_repository_and_path_and_preserves_local_edits() {
        let root = fs_ops::test_temp_dir("public-identity");
        let skill = root.join("library/pdf");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "My local edits").unwrap();
        let mut source = GithubSource {
            kind: "github".into(),
            owner: "author".into(),
            repo: "skills".into(),
            git_ref: "main".into(),
            subdir: "skills/pdf".into(),
            synced_commit: "a".repeat(40),
            synced_at: 0,
            url: "https://github.com/author/skills/tree/main/skills/pdf".into(),
            variants: BTreeMap::new(),
        };
        fs::write(
            skill.join(fs_ops::SOURCE_MARKER_FILE),
            serde_json::to_vec(&source).unwrap(),
        )
        .unwrap();
        assert_eq!(installed_source(&source, &root), Some(skill.clone()));
        source.owner = "other-author".into();
        assert!(installed_source(&source, &root).is_none());
        source.owner = "author".into();
        source.subdir = "other/pdf".into();
        assert!(installed_source(&source, &root).is_none());
        assert_eq!(
            fs::read_to_string(skill.join("SKILL.md")).unwrap(),
            "My local edits"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn import_requires_a_live_backend_preview() {
        assert!(import_public_skill("unreviewed".into())
            .unwrap_err()
            .contains("expired"));
        assert!(
            preview_public_skill("unreviewed".into(), "../escape".into())
                .unwrap_err()
                .contains("expired")
        );
    }
}
