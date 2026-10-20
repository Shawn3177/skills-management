//! Importing skills directly from public GitHub repositories.
//!
//! The flow is: parse + validate a `github.com` URL, resolve the branch and the
//! latest commit for the (optional) subdir, download the repo zipball from
//! `codeload.github.com`, extract just the requested subdir into a staging dir,
//! reuse the library importer to place it, then record the origin in a
//! `.skills-manage-source.json` file so a later "check for updates" can compare.
//!
//! Security notes (this is the app's first network capability):
//! - Only `github.com` input URLs are accepted; every outbound request goes to a
//!   pinned host (`api.github.com` / `codeload.github.com`) rebuilt from the
//!   validated owner/repo/ref — the raw user URL is never passed to the client.
//! - Every archive entry is checked with `is_safe_relative` before writing, and
//!   the download + extraction are bounded (size and entry-count caps).

use crate::fs_ops::{
    default_data_root, is_safe_relative, safe_folder_name, unique_timestamped_path, unix_ms,
    SOURCE_MARKER_FILE,
};
use crate::library::import_skill_to_library_with_root;
use crate::series;
use crate::targets;
use crate::trash::unique_trash_path;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use zip::ZipArchive;

const USER_AGENT: &str = concat!("skills-manage/", env!("CARGO_PKG_VERSION"));
const MAX_ENTRIES: usize = 10_000;
const MAX_TOTAL_BYTES: u64 = 100 * 1024 * 1024; // 100 MB uncompressed
const MAX_DOWNLOAD_BYTES: u64 = 100 * 1024 * 1024; // 100 MB compressed

/// A parsed `github.com` URL pointing at a repo, a `/tree/<ref>/<subdir>`, or
/// a `/blob/<ref>/<path to a file>` inside the skill folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubRef {
    pub owner: String,
    pub repo: String,
    /// `None` when the URL had no `/tree/<ref>` segment (resolve default branch).
    /// Otherwise the first guess: the segment right after `tree`/`blob`.
    pub ref_name: Option<String>,
    /// Empty for a whole-repo URL. First guess, paired with `ref_name`.
    pub subdir: String,
    /// Decoded segments after `tree`/`blob`. A branch name may itself contain
    /// `/`, so the ref/subdir split is only known after asking GitHub.
    ref_path: Vec<String>,
    /// True for `/blob/` URLs: the last segment is a file, not a folder.
    points_at_file: bool,
}

impl GithubRef {
    /// Every (ref, subdir) split of the path, shortest ref first. Git forbids
    /// refs `a` and `a/b` from coexisting, so the first split GitHub accepts
    /// is the right one.
    fn ref_candidates(&self) -> Vec<(String, String)> {
        // For a `/blob/` URL the last segment is a file; its folder is the skill.
        let folder_len = if self.points_at_file {
            self.ref_path.len().saturating_sub(1)
        } else {
            self.ref_path.len()
        };

        (1..=folder_len.max(1).min(self.ref_path.len()))
            .map(|ref_len| {
                let git_ref = self.ref_path[..ref_len].join("/");
                let subdir = self
                    .ref_path
                    .get(ref_len..folder_len)
                    .map(|rest| rest.join("/"))
                    .unwrap_or_default();
                (git_ref, subdir)
            })
            .collect()
    }
}

/// Parse and validate a GitHub URL. Rejects any host other than github.com.
/// Query strings and fragments (`?tab=readme`, `#readme`) are ignored and
/// percent-encoded segments (e.g. copied Chinese folder names) are decoded.
pub fn parse_github_url(url: &str) -> Result<GithubRef, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("Enter a GitHub URL.".to_string());
    }

    let without_suffix = trimmed
        .split(|character| character == '?' || character == '#')
        .next()
        .unwrap_or("");
    let without_scheme = without_suffix
        .strip_prefix("https://")
        .or_else(|| without_suffix.strip_prefix("http://"))
        .unwrap_or(without_suffix);
    let without_host = without_scheme
        .strip_prefix("www.github.com/")
        .or_else(|| without_scheme.strip_prefix("github.com/"))
        .ok_or_else(|| "Only github.com URLs are supported.".to_string())?;

    let parts = without_host
        .split('/')
        .filter(|part| !part.is_empty())
        .map(percent_decode)
        .collect::<Result<Vec<_>, _>>()?;
    if parts.len() < 2 {
        return Err("URL must include an owner and repository.".to_string());
    }

    let owner = parts[0].clone();
    let repo = parts[1].trim_end_matches(".git").to_string();
    let valid_owner = !owner.is_empty()
        && owner
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-');
    let valid_repo = !repo.is_empty()
        && repo != "."
        && repo != ".."
        && repo.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        });
    if !valid_owner || !valid_repo {
        return Err("URL must include a valid owner and repository.".to_string());
    }

    // https://github.com/<owner>/<repo>/(tree|blob)/<ref>/<path...>
    let kind = parts.get(2).map(String::as_str);
    if parts.len() >= 4 && matches!(kind, Some("tree") | Some("blob")) {
        let ref_path = parts[3..].to_vec();
        let unsafe_segment = ref_path.iter().any(|segment| {
            segment == "."
                || segment == ".."
                || segment
                    .chars()
                    .any(|character| character == '\\' || character.is_control())
        });
        if unsafe_segment {
            return Err("URL contains an invalid path segment.".to_string());
        }

        let mut parsed = GithubRef {
            owner,
            repo,
            ref_name: None,
            subdir: String::new(),
            ref_path,
            points_at_file: kind == Some("blob"),
        };
        if let Some((git_ref, subdir)) = parsed.ref_candidates().into_iter().next() {
            parsed.ref_name = Some(git_ref);
            parsed.subdir = subdir;
        }
        return Ok(parsed);
    }

    // Bare repo URL (or /tree with no ref) -> whole repo, default branch.
    Ok(GithubRef {
        owner,
        repo,
        ref_name: None,
        subdir: String::new(),
        ref_path: Vec::new(),
        points_at_file: false,
    })
}

fn percent_decode(segment: &str) -> Result<String, String> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(value) = u8::from_str_radix(hex, 16) {
                decoded.push(value);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(decoded).map_err(|_| "URL contains invalid percent-encoding.".to_string())
}

/// Origin metadata persisted as `.skills-manage-source.json` in the library dir.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubSource {
    pub kind: String,
    pub owner: String,
    pub repo: String,
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub subdir: String,
    pub synced_commit: String,
    pub synced_at: u128,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubImportResult {
    pub skill_name: String,
    pub library_path: String,
    pub owner: String,
    pub repo: String,
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub subdir: String,
    pub synced_commit: String,
    pub url: String,
    pub message: String,
}

pub fn import_from_github(url: String) -> Result<GithubImportResult, String> {
    let data_root = default_data_root()?;
    import_from_github_with_root(&url, &data_root)
}

pub fn import_from_github_with_root(
    url: &str,
    data_root: &Path,
) -> Result<GithubImportResult, String> {
    let parsed = parse_github_url(url)?;
    let agent = build_agent()?;

    // Resolving the commit also validates that the subdir exists at this ref.
    let (git_ref, subdir, synced_commit) =
        resolve_ref_and_commit(&agent, &parsed).map_err(|error| error.message())?;

    // Download exactly the commit we record, so `syncedCommit` always matches
    // the content even if the branch moves between the two requests.
    let zip_bytes = download_zipball(&agent, &parsed.owner, &parsed.repo, &synced_commit)
        .map_err(|error| error.message())?;

    // Name the staging skill folder after the subdir's last segment (or repo) so
    // the library folder is derived from a meaningful name, not the staging id.
    let skill_name = subdir
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or(&parsed.repo)
        .to_string();

    let staging_parent = unique_staging(data_root)?;
    let skill_staging = staging_parent.join(safe_folder_name(&skill_name, "github-skill"));

    let prepared = (|| {
        extract_subdir_from_zip(&zip_bytes, &subdir, &skill_staging)?;
        if !skill_staging.join("SKILL.md").is_file() {
            return Err(if subdir.is_empty() {
                "The repository root has no SKILL.md. Use a /tree/<branch>/<subdir> URL that points at the skill folder.".to_string()
            } else {
                format!("No SKILL.md was found at '{subdir}' in the repository.")
            });
        }
        Ok(())
    })();

    if let Err(error) = prepared {
        let _ = fs::remove_dir_all(&staging_parent);
        return Err(error);
    }

    let import = import_skill_to_library_with_root(&skill_staging, data_root);
    let _ = fs::remove_dir_all(&staging_parent);
    let import = import?;

    let library_path = PathBuf::from(&import.library_path);
    let url = canonical_url(&parsed.owner, &parsed.repo, &git_ref, &subdir);
    let source = GithubSource {
        kind: "github".to_string(),
        owner: parsed.owner.clone(),
        repo: parsed.repo.clone(),
        git_ref: git_ref.clone(),
        subdir: subdir.clone(),
        synced_commit: synced_commit.clone(),
        synced_at: unix_ms(),
        url: url.clone(),
    };
    write_source_file(&library_path, &source)?;

    Ok(GithubImportResult {
        skill_name: import.skill_name,
        library_path: import.library_path,
        owner: parsed.owner,
        repo: parsed.repo,
        git_ref,
        subdir,
        synced_commit,
        url,
        message: "Skill imported from GitHub into the shared library.".to_string(),
    })
}

/// Find the ref, subdir and latest commit for a parsed URL. For `/tree/a/b/c`
/// the split between branch name and folder is tried shortest-ref first.
fn resolve_ref_and_commit(
    agent: &ureq::Agent,
    parsed: &GithubRef,
) -> Result<(String, String, String), GithubError> {
    let candidates = parsed.ref_candidates();
    if candidates.is_empty() {
        let git_ref = resolve_default_branch(agent, &parsed.owner, &parsed.repo)?;
        let commit = latest_commit(agent, &parsed.owner, &parsed.repo, &git_ref, "")?;
        return Ok((git_ref, String::new(), commit));
    }

    for (git_ref, subdir) in candidates {
        match latest_commit(agent, &parsed.owner, &parsed.repo, &git_ref, &subdir) {
            Ok(commit) => return Ok((git_ref, subdir, commit)),
            Err(GithubError::NotFound) => continue,
            Err(error) => return Err(error),
        }
    }
    Err(GithubError::NotFound)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillUpdateStatus {
    pub library_path: String,
    pub skill_name: String,
    /// up-to-date | update-available | source-unavailable | rate-limited | error
    pub state: String,
    pub has_update: bool,
    pub current: String,
    pub latest: String,
    pub url: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateResult {
    pub skill_name: String,
    pub library_path: String,
    pub previous_commit: String,
    pub new_commit: String,
    pub refreshed_targets: Vec<String>,
    pub failed_targets: Vec<String>,
    pub message: String,
}

pub fn check_skill_updates() -> Result<Vec<SkillUpdateStatus>, String> {
    let data_root = default_data_root()?;
    check_skill_updates_with_root(&data_root)
}

pub fn check_skill_updates_with_root(data_root: &Path) -> Result<Vec<SkillUpdateStatus>, String> {
    let library_root = data_root.join("library");
    let mut sources: Vec<(PathBuf, GithubSource)> = Vec::new();
    let mut detected: Vec<(PathBuf, String)> = Vec::new();
    let links = series::load_links(data_root);
    if library_root.is_dir() {
        let entries = fs::read_dir(&library_root)
            .map_err(|error| format!("Could not read the shared library: {error}"))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("Could not read a library entry: {error}"))?;
            let dir = entry.path();
            if dir.is_dir() {
                if let Some(source) = read_source_file(&dir) {
                    sources.push((dir, source));
                } else if let Some(url) = detected_source_url(&dir, &links) {
                    detected.push((dir, url));
                }
            }
        }
    }

    // Skills whose SKILL.md names a GitHub repository but that were never
    // linked to it: offer to fetch the latest version (no network needed here).
    let mut results: Vec<SkillUpdateStatus> = detected
        .into_iter()
        .map(|(dir, url)| SkillUpdateStatus {
            skill_name: dir_skill_name(&dir),
            library_path: dir.display().to_string(),
            state: "source-detected".to_string(),
            has_update: true,
            current: String::new(),
            latest: String::new(),
            url,
            message: String::new(),
        })
        .collect();
    if sources.is_empty() {
        return Ok(results);
    }

    let agent = build_agent()?;
    let mut rate_limited = false;
    for (dir, source) in sources {
        let skill_name = dir_skill_name(&dir);
        let library_path = dir.display().to_string();
        let current = short_sha(&source.synced_commit);

        if rate_limited {
            results.push(rate_limited_status(library_path, skill_name, current, source.url));
            continue;
        }

        match latest_commit(&agent, &source.owner, &source.repo, &source.git_ref, &source.subdir) {
            Ok(latest) => {
                let has_update = latest != source.synced_commit;
                results.push(SkillUpdateStatus {
                    library_path,
                    skill_name,
                    state: if has_update { "update-available" } else { "up-to-date" }.to_string(),
                    has_update,
                    current,
                    latest: short_sha(&latest),
                    url: source.url,
                    message: String::new(),
                });
            }
            Err(GithubError::NotFound) => results.push(SkillUpdateStatus {
                library_path,
                skill_name,
                state: "source-unavailable".to_string(),
                has_update: false,
                current,
                latest: String::new(),
                url: source.url,
                message: "The source was not found on GitHub (it may have moved or been deleted)."
                    .to_string(),
            }),
            Err(GithubError::RateLimited) => {
                rate_limited = true;
                results.push(rate_limited_status(library_path, skill_name, current, source.url));
            }
            Err(GithubError::Other(detail)) => results.push(SkillUpdateStatus {
                library_path,
                skill_name,
                state: "error".to_string(),
                has_update: false,
                current,
                latest: String::new(),
                url: source.url,
                message: detail,
            }),
        }
    }

    Ok(results)
}

fn rate_limited_status(
    library_path: String,
    skill_name: String,
    current: String,
    url: String,
) -> SkillUpdateStatus {
    SkillUpdateStatus {
        library_path,
        skill_name,
        state: "rate-limited".to_string(),
        has_update: false,
        current,
        latest: String::new(),
        url,
        message: GithubError::RateLimited.message(),
    }
}

pub fn update_skill_from_github(library_path: String) -> Result<UpdateResult, String> {
    let data_root = default_data_root()?;
    let target_profiles = real_toggleable_targets()?;
    update_skill_from_github_inner(Path::new(&library_path), &data_root, &target_profiles)
}

fn real_toggleable_targets() -> Result<Vec<(String, String, PathBuf)>, String> {
    let mut profiles = Vec::new();
    for id in targets::toggleable_target_ids() {
        let root = targets::target_root_for(id)?;
        let name = targets::target_name_for(id).unwrap_or(id).to_string();
        profiles.push((id.to_string(), name, root));
    }
    Ok(profiles)
}

fn update_skill_from_github_inner(
    library_path: &Path,
    data_root: &Path,
    target_profiles: &[(String, String, PathBuf)],
) -> Result<UpdateResult, String> {
    let library_root = data_root.join("library");
    let library_canonical = library_root
        .canonicalize()
        .map_err(|error| format!("Could not read the shared library: {error}"))?;
    let path_canonical = library_path
        .canonicalize()
        .map_err(|_| "Skill folder was not found in the shared library.".to_string())?;
    if !path_canonical.starts_with(&library_canonical) {
        return Err("Only shared-library skills can be updated.".to_string());
    }

    let Some(source) = read_source_file(library_path) else {
        // Not added from GitHub, but its SKILL.md may name the repository.
        return update_from_detected_source(library_path, data_root, target_profiles);
    };

    let agent = build_agent()?;
    let new_commit =
        latest_commit(&agent, &source.owner, &source.repo, &source.git_ref, &source.subdir)
            .map_err(|error| error.message())?;
    let zip_bytes = download_zipball(&agent, &source.owner, &source.repo, &new_commit)
        .map_err(|error| error.message())?;

    let skill_name = dir_skill_name(library_path);
    let staging_parent = unique_staging(data_root)?;
    let staging_skill = staging_parent.join(&skill_name);

    let prepared = (|| {
        extract_subdir_from_zip(&zip_bytes, &source.subdir, &staging_skill)?;
        if !staging_skill.join("SKILL.md").is_file() {
            return Err(format!(
                "No SKILL.md was found at '{}' in the repository.",
                source.subdir
            ));
        }
        Ok(())
    })();
    if let Err(error) = prepared {
        let _ = fs::remove_dir_all(&staging_parent);
        return Err(error);
    }

    let outcome = apply_skill_update(
        library_path,
        &staging_skill,
        &source,
        &new_commit,
        data_root,
        target_profiles,
    );
    let _ = fs::remove_dir_all(&staging_parent);
    outcome
}

/// Replace the library skill in place with already-validated new content, then
/// refresh any tool copies that were enabled. No network here, so it is unit
/// tested directly. The previous library dir is soft-deleted to trash.
fn apply_skill_update(
    library_path: &Path,
    new_skill_dir: &Path,
    source: &GithubSource,
    new_commit: &str,
    data_root: &Path,
    target_profiles: &[(String, String, PathBuf)],
) -> Result<UpdateResult, String> {
    let skill_name = dir_skill_name(library_path);
    let previous_commit = source.synced_commit.clone();

    // Write the refreshed source file into staging so the swap is atomic.
    let updated_source = GithubSource {
        synced_commit: new_commit.to_string(),
        synced_at: unix_ms(),
        ..source.clone()
    };
    write_source_file(new_skill_dir, &updated_source)?;

    // Soft-delete the current copy, then move the new content into place.
    let trash_path = unique_trash_path(data_root, &format!("library-{skill_name}"))?;
    fs::rename(library_path, &trash_path)
        .map_err(|error| format!("Could not move the current skill to trash: {error}"))?;
    if let Err(error) = fs::rename(new_skill_dir, library_path) {
        let _ = fs::rename(&trash_path, library_path);
        return Err(format!("Could not install the updated skill: {error}"));
    }

    // Re-copy the new content into each tool that had it enabled. The library
    // path is unchanged, so the markers still match: disable removes the stale
    // copy, enable lays down the fresh one.
    let roots: Vec<(&str, PathBuf)> = target_profiles
        .iter()
        .map(|(id, _name, root)| (id.as_str(), root.clone()))
        .collect();
    let (refreshed_targets, failed_targets) =
        targets::refresh_managed_copies(library_path, data_root, &roots);

    let message = if failed_targets.is_empty() {
        format!("Updated {skill_name} to the latest version.")
    } else {
        format!(
            "Updated {skill_name}, but could not refresh: {}.",
            failed_targets.join(", ")
        )
    };

    Ok(UpdateResult {
        skill_name,
        library_path: library_path.display().to_string(),
        previous_commit: short_sha(&previous_commit),
        new_commit: short_sha(new_commit),
        refreshed_targets,
        failed_targets,
        message,
    })
}

/// A GitHub repository named in SKILL.md's frontmatter (`homepage:`,
/// `repository:`, `source:` …, at any nesting depth). Many published skills
/// carry one even when they were installed by copying a folder.
pub(crate) fn detect_github_url(skill_dir: &Path) -> Option<String> {
    let contents = fs::read_to_string(skill_dir.join("SKILL.md")).ok()?;
    let contents = contents.strip_prefix('\u{feff}').unwrap_or(&contents);
    let mut lines = contents.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return None;
    }
    lines
        .take_while(|line| line.trim_end() != "---")
        .find_map(github_url_in_line)
}

/// Where a skill not added from GitHub can be updated from: a repository named
/// in its SKILL.md, else the folder recorded when the user linked a repository.
pub(crate) fn detected_source_url(skill_dir: &Path, links: &series::SeriesLinks) -> Option<String> {
    detect_github_url(skill_dir).or_else(|| series::linked_url(links, &dir_skill_name(skill_dir)))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSeriesResult {
    /// `owner/repo`.
    pub repository: String,
    /// The GitHub folder the skills were found in.
    pub url: String,
    /// Skill folders found there.
    pub skill_count: usize,
    /// Installed skill folders (library or tool folders) that now belong to it.
    pub matched: Vec<String>,
    /// GitHub cut the file listing short (very large repository).
    pub truncated: bool,
}

/// Link a repository (or a folder in it) that holds several skills: every
/// installed skill folder named like one of its skill folders is grouped under
/// it and can be updated from it. Nothing but `series.json` is written.
pub fn link_github_series(url: String) -> Result<LinkSeriesResult, String> {
    let data_root = default_data_root()?;
    let installed = installed_skill_folders(&crate::scanner::candidate_skill_roots());
    link_github_series_with_root(&url, &data_root, &installed)
}

fn link_github_series_with_root(
    url: &str,
    data_root: &Path,
    installed: &[String],
) -> Result<LinkSeriesResult, String> {
    let parsed = parse_github_url(url)?;
    let agent = build_agent()?;
    let (git_ref, mut subdir, commit) =
        resolve_ref_and_commit(&agent, &parsed).map_err(|error| error.message())?;
    let (paths, truncated) =
        repo_tree(&agent, &parsed.owner, &parsed.repo, &commit).map_err(|error| error.message())?;

    let mut folders = series::skill_folders_in_tree(&paths, &subdir, &parsed.repo);
    // A link to one skill of a series means the series: use its parent folder.
    if folders.len() == 1 && !subdir.is_empty() && folders[0].1 == subdir {
        subdir = subdir.rsplit_once('/').map(|(parent, _)| parent.to_string()).unwrap_or_default();
        folders = series::skill_folders_in_tree(&paths, &subdir, &parsed.repo);
    }
    if folders.is_empty() {
        return Err("No skills (folders with a SKILL.md) were found there.".to_string());
    }

    let entries: Vec<(String, String)> = folders
        .iter()
        .map(|(name, dir)| (name.clone(), canonical_url(&parsed.owner, &parsed.repo, &git_ref, dir)))
        .collect();
    series::replace_repo_links(data_root, &parsed.owner, &parsed.repo, &entries)?;

    let mut matched: Vec<String> = entries
        .iter()
        .filter(|(name, _)| installed.iter().any(|folder| folder.eq_ignore_ascii_case(name)))
        .map(|(name, _)| name.clone())
        .collect();
    matched.sort_by_key(|name| name.to_lowercase());
    Ok(LinkSeriesResult {
        repository: format!("{}/{}", parsed.owner, parsed.repo),
        url: canonical_url(&parsed.owner, &parsed.repo, &git_ref, &subdir),
        skill_count: entries.len(),
        matched,
        truncated,
    })
}

/// Names of the skill folders in the given roots.
fn installed_skill_folders(roots: &[PathBuf]) -> Vec<String> {
    roots
        .iter()
        .filter_map(|root| fs::read_dir(root).ok())
        .flat_map(|entries| entries.flatten())
        .filter(|entry| entry.path().join("SKILL.md").is_file())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| !name.starts_with('.'))
        .collect()
}

/// Every file path in the repository at `commit` (one API request).
fn repo_tree(
    agent: &ureq::Agent,
    owner: &str,
    repo: &str,
    commit: &str,
) -> Result<(Vec<String>, bool), GithubError> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/git/trees/{commit}?recursive=1");
    let json = api_get_json(agent, &url)?;
    let tree = json
        .get("tree")
        .and_then(|value| value.as_array())
        .ok_or_else(|| GithubError::Other("Unexpected response from GitHub trees API.".to_string()))?;
    let paths = tree
        .iter()
        .filter(|item| item.get("type").and_then(|value| value.as_str()) == Some("blob"))
        .filter_map(|item| item.get("path").and_then(|value| value.as_str()).map(str::to_string))
        .collect();
    let truncated = json.get("truncated").and_then(|value| value.as_bool()).unwrap_or(false);
    Ok((paths, truncated))
}

fn github_url_in_line(line: &str) -> Option<String> {
    let start = line.find("github.com/")?;
    let rest: String = line[start..]
        .chars()
        .take_while(|character| !character.is_whitespace() && !matches!(character, '"' | '\'' | ')' | '>' | ',' | ']'))
        .collect();
    let url = format!("https://{}", rest.trim_end_matches(['.', '/']));
    parse_github_url(&url).ok().map(|_| url)
}

/// The skill's name as written in a SKILL.md, if it has one.
fn skill_md_name(contents: &str) -> Option<String> {
    let contents = contents.strip_prefix('\u{feff}').unwrap_or(contents);
    let mut lines = contents.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return None;
    }
    lines
        .take_while(|line| line.trim_end() != "---")
        .find_map(|line| line.strip_prefix("name:"))
        .map(|value| value.trim().trim_matches(['"', '\'']).to_string())
        .filter(|value| !value.is_empty())
}

/// Find the folder (relative to the repository root) that holds the skill:
/// a SKILL.md whose `name` matches, else a folder with the same name, else the
/// only SKILL.md in the repository.
fn find_skill_subdir(zip_bytes: &[u8], skill_name: &str, folder_name: &str) -> Result<String, String> {
    let mut archive = ZipArchive::new(Cursor::new(zip_bytes))
        .map_err(|error| format!("Could not read the downloaded archive: {error}"))?;
    let mut candidates: Vec<(String, Option<String>)> = Vec::new();
    for index in 0..archive.len().min(MAX_ENTRIES) {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Could not read the archive: {error}"))?;
        let name = entry.name().to_string();
        let Some((top_and_dir, file)) = name.rsplit_once('/') else {
            continue;
        };
        if file != "SKILL.md" {
            continue;
        }
        let dir = top_and_dir.split_once('/').map(|(_, dir)| dir.to_string()).unwrap_or_default();
        let mut contents = String::new();
        let _ = (&mut entry).take(256 * 1024).read_to_string(&mut contents);
        candidates.push((dir, skill_md_name(&contents)));
    }

    let wanted = skill_name.to_lowercase();
    let folder = folder_name.to_lowercase();
    let by_name = candidates
        .iter()
        .find(|(_, name)| name.as_deref().map(str::to_lowercase).as_deref() == Some(wanted.as_str()));
    let by_folder = candidates
        .iter()
        .find(|(dir, _)| dir.rsplit('/').next().map(str::to_lowercase).as_deref() == Some(folder.as_str()));
    let only = (candidates.len() == 1).then(|| &candidates[0]);
    by_name
        .or(by_folder)
        .or(only)
        .map(|(dir, _)| dir.clone())
        .ok_or_else(|| format!("Couldn't find the skill \"{skill_name}\" in that repository."))
}

/// Link a library skill that names a GitHub repository in its SKILL.md to that
/// repository, and update it to the latest version there. Afterwards it is a
/// regular GitHub-sourced skill (update checks work as usual).
fn update_from_detected_source(
    library_path: &Path,
    data_root: &Path,
    target_profiles: &[(String, String, PathBuf)],
) -> Result<UpdateResult, String> {
    let url = detected_source_url(library_path, &series::load_links(data_root))
        .ok_or_else(|| "This skill has no GitHub source to update from.".to_string())?;
    let parsed = parse_github_url(&url)?;
    let agent = build_agent()?;

    let (git_ref, mut subdir, mut commit) = if parsed.ref_name.is_some() {
        resolve_ref_and_commit(&agent, &parsed).map_err(|error| error.message())?
    } else {
        let git_ref = resolve_default_branch(&agent, &parsed.owner, &parsed.repo).map_err(|error| error.message())?;
        let commit =
            latest_commit(&agent, &parsed.owner, &parsed.repo, &git_ref, "").map_err(|error| error.message())?;
        (git_ref, String::new(), commit)
    };
    let zip_bytes =
        download_zipball(&agent, &parsed.owner, &parsed.repo, &commit).map_err(|error| error.message())?;

    let folder_name = dir_skill_name(library_path);
    if subdir.is_empty() {
        let skill_name = fs::read_to_string(library_path.join("SKILL.md"))
            .ok()
            .and_then(|contents| skill_md_name(&contents))
            .unwrap_or_else(|| folder_name.clone());
        subdir = find_skill_subdir(&zip_bytes, &skill_name, &folder_name)?;
        if !subdir.is_empty() {
            // Record the last commit that touched the skill's own folder, so
            // later checks only report changes to this skill.
            if let Ok(path_commit) = latest_commit(&agent, &parsed.owner, &parsed.repo, &git_ref, &subdir) {
                commit = path_commit;
            }
        }
    }

    let staging_parent = unique_staging(data_root)?;
    let staging_skill = staging_parent.join(safe_folder_name(&folder_name, "github-skill"));
    let prepared = (|| {
        extract_subdir_from_zip(&zip_bytes, &subdir, &staging_skill)?;
        if !staging_skill.join("SKILL.md").is_file() {
            return Err(format!("No SKILL.md was found at '{subdir}' in the repository."));
        }
        Ok(())
    })();
    if let Err(error) = prepared {
        let _ = fs::remove_dir_all(&staging_parent);
        return Err(error);
    }

    let source = GithubSource {
        kind: "github".to_string(),
        owner: parsed.owner.clone(),
        repo: parsed.repo.clone(),
        git_ref: git_ref.clone(),
        subdir: subdir.clone(),
        synced_commit: String::new(),
        synced_at: unix_ms(),
        url: canonical_url(&parsed.owner, &parsed.repo, &git_ref, &subdir),
    };
    let outcome = apply_skill_update(library_path, &staging_skill, &source, &commit, data_root, target_profiles);
    let _ = fs::remove_dir_all(&staging_parent);
    outcome
}

fn read_source_file(dir: &Path) -> Option<GithubSource> {
    let contents = fs::read_to_string(dir.join(SOURCE_MARKER_FILE)).ok()?;
    let source: GithubSource = serde_json::from_str(&contents).ok()?;
    if source.kind == "github" {
        Some(source)
    } else {
        None
    }
}

fn dir_skill_name(dir: &Path) -> String {
    dir.file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("skill")
        .to_string()
}

fn short_sha(sha: &str) -> String {
    sha.chars().take(7).collect()
}

fn build_agent() -> Result<ureq::Agent, String> {
    let connector =
        native_tls::TlsConnector::new().map_err(|error| format!("Could not initialize TLS: {error}"))?;
    Ok(ureq::AgentBuilder::new()
        .redirects(5)
        .timeout(Duration::from_secs(120))
        .tls_connector(Arc::new(connector))
        .build())
}

fn resolve_default_branch(
    agent: &ureq::Agent,
    owner: &str,
    repo: &str,
) -> Result<String, GithubError> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}");
    let json = api_get_json(agent, &url)?;
    json.get("default_branch")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .ok_or_else(|| {
            GithubError::Other("Could not determine the repository's default branch.".to_string())
        })
}

fn latest_commit(
    agent: &ureq::Agent,
    owner: &str,
    repo: &str,
    git_ref: &str,
    subdir: &str,
) -> Result<String, GithubError> {
    let mut url = format!(
        "https://api.github.com/repos/{owner}/{repo}/commits?per_page=1&sha={}",
        encode_query(git_ref)
    );
    if !subdir.is_empty() {
        url.push_str(&format!("&path={}", encode_query(subdir)));
    }

    let json = api_get_json(agent, &url)?;
    let commits = json
        .as_array()
        .ok_or_else(|| GithubError::Other("Unexpected response from GitHub commits API.".to_string()))?;
    let first = commits.first().ok_or_else(|| {
        if subdir.is_empty() {
            GithubError::Other("The repository has no commits.".to_string())
        } else {
            // An empty path-filtered history means the subdir is gone at this ref.
            GithubError::NotFound
        }
    })?;
    first
        .get("sha")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .ok_or_else(|| GithubError::Other("GitHub commit response was missing a sha.".to_string()))
}

fn download_zipball(
    agent: &ureq::Agent,
    owner: &str,
    repo: &str,
    git_ref: &str,
) -> Result<Vec<u8>, GithubError> {
    let url = format!("https://codeload.github.com/{owner}/{repo}/zip/{git_ref}");
    let response = agent
        .get(&url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(classify_ureq_error)?;

    let mut buffer = Vec::new();
    response
        .into_reader()
        .take(MAX_DOWNLOAD_BYTES + 1)
        .read_to_end(&mut buffer)
        .map_err(|error| {
            GithubError::Other(format!("Could not download the repository archive: {error}"))
        })?;

    if buffer.is_empty() {
        return Err(GithubError::Other(
            "The downloaded repository archive was empty.".to_string(),
        ));
    }
    if buffer.len() as u64 > MAX_DOWNLOAD_BYTES {
        return Err(GithubError::Other(
            "The repository archive is too large to import.".to_string(),
        ));
    }
    Ok(buffer)
}

fn api_get_json(agent: &ureq::Agent, url: &str) -> Result<serde_json::Value, GithubError> {
    let mut request = agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28");
    // An optional token raises the API limit from 60 to 5000 requests/hour. It
    // is only sent to api.github.com (ureq drops auth headers on redirects).
    if let Some(token) = github_token() {
        request = request.set("Authorization", &format!("Bearer {token}"));
    }
    let response = request.call().map_err(classify_ureq_error)?;
    let body = response
        .into_string()
        .map_err(|error| GithubError::Other(format!("Could not read the GitHub response: {error}")))?;
    serde_json::from_str(&body)
        .map_err(|error| GithubError::Other(format!("Could not parse the GitHub response: {error}")))
}

pub(crate) fn has_github_token() -> bool {
    github_token().is_some()
}

fn github_token() -> Option<String> {
    ["SKILLS_MANAGE_GITHUB_TOKEN", "GITHUB_TOKEN", "GH_TOKEN"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
}

/// Classified failure from a GitHub request, so callers can distinguish a
/// missing/moved source and a rate limit from a generic error.
#[derive(Debug)]
enum GithubError {
    NotFound,
    RateLimited,
    Other(String),
}

impl GithubError {
    fn message(&self) -> String {
        match self {
            GithubError::NotFound => {
                "Repository or path not found. Only public repositories are supported.".to_string()
            }
            GithubError::RateLimited => {
                "GitHub's hourly rate limit was reached. Try again later.".to_string()
            }
            GithubError::Other(detail) => detail.clone(),
        }
    }
}

fn classify_ureq_error(error: ureq::Error) -> GithubError {
    match error {
        ureq::Error::Status(code, response) => match code {
            // 422 is the commits API's answer for a ref that does not exist.
            404 | 422 => GithubError::NotFound,
            401 | 403 => {
                if response.header("x-ratelimit-remaining") == Some("0") {
                    GithubError::RateLimited
                } else {
                    GithubError::Other(
                        "GitHub denied the request. Only public repositories are supported."
                            .to_string(),
                    )
                }
            }
            _ => GithubError::Other(format!("GitHub returned an unexpected status ({code}).")),
        },
        ureq::Error::Transport(transport) => {
            GithubError::Other(format!("Network error reaching GitHub: {transport}"))
        }
    }
}

/// Extract just `subdir` (or the whole repo when empty) from a downloaded
/// zipball into `dest`. The zipball's top-level folder name is read from the
/// archive rather than reconstructed, and every entry path is validated.
fn extract_subdir_from_zip(zip_bytes: &[u8], subdir: &str, dest: &Path) -> Result<(), String> {
    let reader = Cursor::new(zip_bytes);
    let mut archive =
        ZipArchive::new(reader).map_err(|error| format!("Could not read the downloaded archive: {error}"))?;
    if archive.is_empty() {
        return Err("The downloaded archive was empty.".to_string());
    }
    if archive.len() > MAX_ENTRIES {
        return Err("The downloaded archive has too many entries.".to_string());
    }

    let top = {
        let first = archive
            .by_index(0)
            .map_err(|error| format!("Could not read the archive: {error}"))?;
        first
            .name()
            .split('/')
            .next()
            .unwrap_or("")
            .to_string()
    };
    if top.is_empty() {
        return Err("The downloaded archive has an unexpected layout.".to_string());
    }

    let prefix = if subdir.is_empty() {
        format!("{top}/")
    } else {
        format!("{top}/{}/", subdir.trim_matches('/'))
    };

    fs::create_dir_all(dest).map_err(|error| format!("Could not create the staging folder: {error}"))?;

    let mut total_bytes: u64 = 0;
    let mut wrote_any = false;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Could not read the archive: {error}"))?;
        let name = entry.name().to_string();
        if !name.starts_with(&prefix) {
            continue;
        }
        let relative = &name[prefix.len()..];
        if relative.is_empty() {
            continue;
        }
        if !is_safe_relative(relative) {
            return Err(format!("The archive contains an unsafe path: {name}"));
        }

        let out_path = dest.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&out_path)
                .map_err(|error| format!("Could not create {}: {error}", out_path.display()))?;
            continue;
        }

        total_bytes = total_bytes.saturating_add(entry.size());
        if total_bytes > MAX_TOTAL_BYTES {
            return Err("The skill content is too large to import.".to_string());
        }

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Could not read {name}: {error}"))?;
        fs::write(&out_path, &bytes)
            .map_err(|error| format!("Could not write {}: {error}", out_path.display()))?;
        wrote_any = true;
    }

    if !wrote_any {
        return Err(if subdir.is_empty() {
            "The repository appears to be empty.".to_string()
        } else {
            format!("Path '{subdir}' was not found in the repository.")
        });
    }
    Ok(())
}

fn write_source_file(library_path: &Path, source: &GithubSource) -> Result<(), String> {
    let json = serde_json::to_string_pretty(source)
        .map_err(|error| format!("Could not serialize source metadata: {error}"))?;
    fs::write(library_path.join(SOURCE_MARKER_FILE), json)
        .map_err(|error| format!("Could not write source metadata: {error}"))
}

fn canonical_url(owner: &str, repo: &str, git_ref: &str, subdir: &str) -> String {
    if subdir.is_empty() {
        format!("https://github.com/{owner}/{repo}/tree/{git_ref}")
    } else {
        format!("https://github.com/{owner}/{repo}/tree/{git_ref}/{subdir}")
    }
}

fn unique_staging(data_root: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(data_root)
        .map_err(|error| format!("Could not create the data root: {error}"))?;
    Ok(unique_timestamped_path(data_root, ".github-import"))
}

/// Percent-encode a query value, leaving unreserved characters and `/` intact.
fn encode_query(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs_ops::test_temp_dir as unique_temp_dir;
    use std::io::Write;
    use zip::write::{SimpleFileOptions, ZipWriter};

    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buffer = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buffer));
            let options = SimpleFileOptions::default();
            for (name, content) in entries {
                zip.start_file(*name, options).unwrap();
                zip.write_all(content).unwrap();
            }
            zip.finish().unwrap();
        }
        buffer
    }

    #[test]
    fn detects_a_github_repository_in_frontmatter_only() {
        let dir = unique_temp_dir("detect-source");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            "---\nname: agent-reach\ndescription: d\nmetadata:\n  openclaw:\n    homepage: https://github.com/Panniantong/Agent-Reach\n---\n# Body links https://github.com/other/repo\n",
        )
        .unwrap();
        assert_eq!(
            detect_github_url(&dir).as_deref(),
            Some("https://github.com/Panniantong/Agent-Reach")
        );

        fs::write(dir.join("SKILL.md"), "---\nname: x\n---\nSee https://github.com/other/repo\n").unwrap();
        assert_eq!(detect_github_url(&dir), None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn finds_the_skill_folder_inside_a_repository_archive() {
        let zip = build_zip(&[
            ("repo-abc/README.md", b"readme"),
            ("repo-abc/skills/agent-reach/SKILL.md", b"---\nname: agent-reach\n---\n"),
            ("repo-abc/skills/other/SKILL.md", b"---\nname: other\n---\n"),
        ]);
        assert_eq!(find_skill_subdir(&zip, "agent-reach", "x").unwrap(), "skills/agent-reach");
        assert_eq!(find_skill_subdir(&zip, "unknown", "other").unwrap(), "skills/other");
        assert!(find_skill_subdir(&zip, "unknown", "missing").is_err());

        let single = build_zip(&[("repo-abc/SKILL.md", b"---\nname: whatever\n---\n")]);
        assert_eq!(find_skill_subdir(&single, "agent-reach", "agent-reach").unwrap(), "");
    }

    #[test]
    fn parses_bare_repo_url() {
        let parsed = parse_github_url("https://github.com/owner/repo").unwrap();
        assert_eq!(parsed.owner, "owner");
        assert_eq!(parsed.repo, "repo");
        assert_eq!(parsed.ref_name, None);
        assert_eq!(parsed.subdir, "");
    }

    #[test]
    fn parses_tree_subdir_url() {
        let parsed =
            parse_github_url("https://github.com/owner/repo/tree/main/skills/foo").unwrap();
        assert_eq!(parsed.owner, "owner");
        assert_eq!(parsed.repo, "repo");
        assert_eq!(parsed.ref_name.as_deref(), Some("main"));
        assert_eq!(parsed.subdir, "skills/foo");
    }

    #[test]
    fn strips_git_suffix_and_trailing_slash() {
        let parsed = parse_github_url("github.com/owner/repo.git/").unwrap();
        assert_eq!(parsed.repo, "repo");
        assert_eq!(parsed.subdir, "");
    }

    #[test]
    fn ignores_query_and_fragment_and_decodes_segments() {
        let parsed = parse_github_url(
            "https://github.com/owner/repo/tree/main/skills/%E4%BB%A3%E7%A0%81%E5%AE%A1%E6%9F%A5?tab=readme-ov-file#readme",
        )
        .unwrap();
        assert_eq!(parsed.repo, "repo");
        assert_eq!(parsed.ref_name.as_deref(), Some("main"));
        assert_eq!(parsed.subdir, "skills/代码审查");

        let bare = parse_github_url("https://github.com/owner/repo?tab=readme-ov-file").unwrap();
        assert_eq!(bare.repo, "repo");
        assert_eq!(bare.ref_name, None);
    }

    #[test]
    fn blob_url_points_at_the_files_folder() {
        let parsed =
            parse_github_url("https://github.com/owner/repo/blob/main/skills/foo/SKILL.md").unwrap();
        assert_eq!(parsed.ref_name.as_deref(), Some("main"));
        assert_eq!(parsed.subdir, "skills/foo");
    }

    #[test]
    fn offers_every_ref_split_for_slashed_branch_names() {
        let parsed =
            parse_github_url("https://github.com/owner/repo/tree/feature/x/skills/foo").unwrap();
        assert_eq!(
            parsed.ref_candidates(),
            vec![
                ("feature".to_string(), "x/skills/foo".to_string()),
                ("feature/x".to_string(), "skills/foo".to_string()),
                ("feature/x/skills".to_string(), "foo".to_string()),
                ("feature/x/skills/foo".to_string(), String::new()),
            ]
        );

        let blob = parse_github_url("https://github.com/o/r/blob/feature/x/SKILL.md").unwrap();
        assert_eq!(
            blob.ref_candidates(),
            vec![
                ("feature".to_string(), "x".to_string()),
                ("feature/x".to_string(), String::new()),
            ]
        );
    }

    #[test]
    fn rejects_traversal_and_invalid_names() {
        assert!(parse_github_url("https://github.com/owner/repo/tree/main/..").is_err());
        assert!(parse_github_url("https://github.com/owner/repo/tree/main/a/%2E%2E/b").is_err());
        assert!(parse_github_url("https://github.com/ow ner/repo").is_err());
        assert!(parse_github_url("https://github.com/owner/..").is_err());
    }

    #[test]
    fn rejects_non_github_host() {
        let error = parse_github_url("https://gitlab.com/owner/repo").unwrap_err();
        assert!(error.contains("github.com"), "got: {error}");
    }

    #[test]
    fn rejects_url_without_repo() {
        assert!(parse_github_url("https://github.com/owner").is_err());
    }

    #[test]
    fn extracts_only_the_requested_subdir() {
        let zip = build_zip(&[
            ("repo-main/README.md", b"root readme"),
            ("repo-main/skills/foo/SKILL.md", b"# foo"),
            ("repo-main/skills/foo/scripts/run.sh", b"echo ok"),
            ("repo-main/skills/bar/SKILL.md", b"# bar"),
        ]);
        let dest = unique_temp_dir("subdir");

        extract_subdir_from_zip(&zip, "skills/foo", &dest).unwrap();

        assert!(dest.join("SKILL.md").is_file());
        assert!(dest.join("scripts").join("run.sh").is_file());
        assert!(!dest.join("README.md").exists());
        assert!(!dest.join("bar").exists());

        fs::remove_dir_all(&dest).unwrap();
    }

    #[test]
    fn extracts_whole_repo_when_subdir_empty() {
        let zip = build_zip(&[
            ("repo-main/SKILL.md", b"# root skill"),
            ("repo-main/scripts/run.sh", b"echo ok"),
        ]);
        let dest = unique_temp_dir("wholerepo");

        extract_subdir_from_zip(&zip, "", &dest).unwrap();

        assert!(dest.join("SKILL.md").is_file());
        assert!(dest.join("scripts").join("run.sh").is_file());

        fs::remove_dir_all(&dest).unwrap();
    }

    #[test]
    fn errors_when_subdir_missing() {
        let zip = build_zip(&[("repo-main/README.md", b"root readme")]);
        let dest = unique_temp_dir("missing");

        let error = extract_subdir_from_zip(&zip, "skills/nope", &dest).unwrap_err();
        assert!(error.contains("not be found") || error.contains("not found"), "got: {error}");

        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn rejects_unsafe_archive_paths() {
        let zip = build_zip(&[
            ("repo-main/skills/foo/SKILL.md", b"# foo"),
            ("repo-main/skills/foo/../../evil.txt", b"pwned"),
        ]);
        let dest = unique_temp_dir("unsafe");

        let error = extract_subdir_from_zip(&zip, "skills/foo", &dest).unwrap_err();
        assert!(error.contains("unsafe path"), "got: {error}");

        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn builds_canonical_urls() {
        assert_eq!(
            canonical_url("o", "r", "main", "skills/foo"),
            "https://github.com/o/r/tree/main/skills/foo"
        );
        assert_eq!(
            canonical_url("o", "r", "main", ""),
            "https://github.com/o/r/tree/main"
        );
    }

    #[test]
    fn encodes_query_values() {
        assert_eq!(encode_query("skills/foo bar"), "skills/foo%20bar");
        assert_eq!(encode_query("main"), "main");
    }

    fn sample_source() -> GithubSource {
        GithubSource {
            kind: "github".to_string(),
            owner: "o".to_string(),
            repo: "r".to_string(),
            git_ref: "main".to_string(),
            subdir: "skills/foo".to_string(),
            synced_commit: "old1111111111111111111111111111111111111".to_string(),
            synced_at: 1,
            url: "https://github.com/o/r/tree/main/skills/foo".to_string(),
        }
    }

    #[test]
    fn apply_update_replaces_library_and_refreshes_enabled_target() {
        let data_root = unique_temp_dir("apply-update");
        let library = data_root.join("library").join("foo");
        fs::create_dir_all(&library).unwrap();
        fs::write(library.join("SKILL.md"), "# old\n").unwrap();
        let source = sample_source();
        write_source_file(&library, &source).unwrap();

        // Enable for codex using a temp target root, then confirm the old copy.
        let codex_root = data_root.join("codex-root");
        crate::targets::set_skill_target_enabled_with_root(
            &library, "codex", true, &data_root, &codex_root,
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(codex_root.join("foo").join("SKILL.md")).unwrap(),
            "# old\n"
        );

        // Build new validated content in a staging dir.
        let staging = unique_temp_dir("apply-update-staging");
        let new_skill = staging.join("foo");
        fs::create_dir_all(&new_skill).unwrap();
        fs::write(new_skill.join("SKILL.md"), "# NEW\n").unwrap();

        let profiles = vec![("codex".to_string(), "Codex".to_string(), codex_root.clone())];
        let result = apply_skill_update(
            &library,
            &new_skill,
            &source,
            "new2222222222222222222222222222222222222",
            &data_root,
            &profiles,
        )
        .unwrap();

        // Library replaced in place.
        assert_eq!(fs::read_to_string(library.join("SKILL.md")).unwrap(), "# NEW\n");
        // Source metadata bumped to the new commit.
        assert_eq!(
            read_source_file(&library).unwrap().synced_commit,
            "new2222222222222222222222222222222222222"
        );
        // The enabled tool copy was refreshed with the new content.
        assert_eq!(
            fs::read_to_string(codex_root.join("foo").join("SKILL.md")).unwrap(),
            "# NEW\n"
        );
        assert_eq!(result.refreshed_targets, vec!["Codex".to_string()]);
        assert!(result.failed_targets.is_empty());
        assert_eq!(result.new_commit, "new2222");

        fs::remove_dir_all(&data_root).unwrap();
        let _ = fs::remove_dir_all(&staging);
    }

    #[test]
    fn apply_update_leaves_unenabled_skill_with_no_tool_copies() {
        let data_root = unique_temp_dir("apply-update-disabled");
        let library = data_root.join("library").join("solo");
        fs::create_dir_all(&library).unwrap();
        fs::write(library.join("SKILL.md"), "# old\n").unwrap();
        let source = sample_source();
        write_source_file(&library, &source).unwrap();

        let staging = unique_temp_dir("apply-update-disabled-staging");
        let new_skill = staging.join("solo");
        fs::create_dir_all(&new_skill).unwrap();
        fs::write(new_skill.join("SKILL.md"), "# NEW\n").unwrap();

        let codex_root = data_root.join("codex-root");
        let profiles = vec![("codex".to_string(), "Codex".to_string(), codex_root)];
        let result =
            apply_skill_update(&library, &new_skill, &source, "abc1234def", &data_root, &profiles)
                .unwrap();

        assert_eq!(fs::read_to_string(library.join("SKILL.md")).unwrap(), "# NEW\n");
        assert!(result.refreshed_targets.is_empty());
        assert!(result.failed_targets.is_empty());

        fs::remove_dir_all(&data_root).unwrap();
        let _ = fs::remove_dir_all(&staging);
    }

    #[test]
    fn check_returns_empty_without_github_sources() {
        let data_root = unique_temp_dir("check-empty");
        let plain = data_root.join("library").join("plain");
        fs::create_dir_all(&plain).unwrap();
        fs::write(plain.join("SKILL.md"), "# plain\n").unwrap();

        let results = check_skill_updates_with_root(&data_root).unwrap();
        assert!(results.is_empty());

        fs::remove_dir_all(&data_root).unwrap();
    }

    // Live: a skill whose SKILL.md names its repository is linked to it and
    // updated from there. Run manually with `cargo test -- --ignored`.
    #[test]
    #[ignore = "hits the live GitHub API"]
    fn live_updates_a_skill_from_its_detected_repository() {
        let data_root = unique_temp_dir("live-detected");
        let library = data_root.join("library").join("agent-reach");
        fs::create_dir_all(&library).unwrap();
        fs::write(
            library.join("SKILL.md"),
            "---\nname: agent-reach\ndescription: old\nmetadata:\n  openclaw:\n    homepage: https://github.com/Panniantong/Agent-Reach\n---\n# old copy\n",
        )
        .unwrap();

        let result = update_skill_from_github_inner(&library, &data_root, &[]).unwrap();

        let source = read_source_file(&library).unwrap();
        assert_eq!(source.subdir, "agent_reach/skill");
        assert_eq!(result.new_commit.len(), 7);
        assert!(!fs::read_to_string(library.join("SKILL.md")).unwrap().contains("# old copy"));
        fs::remove_dir_all(&data_root).unwrap();
    }

    // Exercises the live network path (TLS, GitHub API, codeload download, zip
    // extraction) against GitHub's canonical test repo. Ignored by default;
    // run manually with `cargo test -- --ignored`.
    #[test]
    #[ignore = "hits the live GitHub API"]
    fn live_links_a_skill_series_from_one_skill_url() {
        let data_root = unique_temp_dir("live-series");
        let installed = vec!["PDF".to_string(), "not-there".to_string()];
        let result = link_github_series_with_root(
            "https://github.com/anthropics/skills/tree/main/skills/pdf",
            &data_root,
            &installed,
        )
        .unwrap();
        assert_eq!(result.repository, "anthropics/skills");
        assert_eq!(result.url, "https://github.com/anthropics/skills/tree/main/skills");
        assert!(result.skill_count > 5);
        assert_eq!(result.matched, vec!["pdf".to_string()]);
        let links = series::load_links(&data_root);
        assert_eq!(
            series::linked_url(&links, "docx").as_deref(),
            Some("https://github.com/anthropics/skills/tree/main/skills/docx")
        );
        let _ = fs::remove_dir_all(data_root);
    }

    #[test]
    #[ignore = "hits the live GitHub API"]
    fn live_resolves_and_downloads_octocat_hello_world() {
        let agent = build_agent().unwrap();

        let branch = resolve_default_branch(&agent, "octocat", "Hello-World").unwrap();
        assert_eq!(branch, "master");

        let sha = latest_commit(&agent, "octocat", "Hello-World", &branch, "").unwrap();
        assert_eq!(sha.len(), 40);

        // A tree URL resolves to the same ref and commit as the default branch.
        let parsed = parse_github_url("https://github.com/octocat/Hello-World/tree/master").unwrap();
        let (git_ref, subdir, resolved) = resolve_ref_and_commit(&agent, &parsed).unwrap();
        assert_eq!((git_ref.as_str(), subdir.as_str()), ("master", ""));
        assert_eq!(resolved, sha);

        let zip = download_zipball(&agent, "octocat", "Hello-World", &sha).unwrap();
        let dest = unique_temp_dir("live");
        extract_subdir_from_zip(&zip, "", &dest).unwrap();
        assert!(dest.join("README").exists() || dest.join("README.md").exists());

        fs::remove_dir_all(&dest).unwrap();
    }
}
