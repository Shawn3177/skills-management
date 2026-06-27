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

use crate::fs_ops::{default_data_root, is_safe_relative, unix_ms, SOURCE_MARKER_FILE};
use crate::library::import_skill_to_library_with_root;
use crate::targets;
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

/// A parsed `github.com` URL pointing at a repo or a `/tree/<ref>/<subdir>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubRef {
    pub owner: String,
    pub repo: String,
    /// `None` when the URL had no `/tree/<ref>` segment (resolve default branch).
    pub ref_name: Option<String>,
    /// Empty for a whole-repo URL.
    pub subdir: String,
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

/// Parse and validate a GitHub URL. Rejects any host other than github.com.
pub fn parse_github_url(url: &str) -> Result<GithubRef, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("Enter a GitHub URL.".to_string());
    }

    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    let without_host = without_scheme
        .strip_prefix("www.github.com/")
        .or_else(|| without_scheme.strip_prefix("github.com/"))
        .ok_or_else(|| "Only github.com URLs are supported.".to_string())?;

    let path = without_host.trim_end_matches('/');
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    if parts.len() < 2 {
        return Err("URL must include an owner and repository.".to_string());
    }

    let owner = parts[0].to_string();
    let repo = parts[1].trim_end_matches(".git").to_string();
    if owner.is_empty() || repo.is_empty() {
        return Err("URL must include an owner and repository.".to_string());
    }

    // https://github.com/<owner>/<repo>/tree/<ref>/<subdir...>
    if parts.len() >= 4 && parts[2] == "tree" {
        let ref_name = parts[3].to_string();
        let subdir = parts[4..].join("/");
        return Ok(GithubRef {
            owner,
            repo,
            ref_name: Some(ref_name),
            subdir,
        });
    }

    // Bare repo URL (or /tree with no ref) -> whole repo, default branch.
    Ok(GithubRef {
        owner,
        repo,
        ref_name: None,
        subdir: String::new(),
    })
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

    let git_ref = match &parsed.ref_name {
        Some(reference) => reference.clone(),
        None => resolve_default_branch(&agent, &parsed.owner, &parsed.repo)
            .map_err(|error| error.message())?,
    };

    // Resolving the commit also validates that the subdir exists at this ref.
    let synced_commit = latest_commit(&agent, &parsed.owner, &parsed.repo, &git_ref, &parsed.subdir)
        .map_err(|error| error.message())?;

    let zip_bytes = download_zipball(&agent, &parsed.owner, &parsed.repo, &git_ref)
        .map_err(|error| error.message())?;

    // Name the staging skill folder after the subdir's last segment (or repo) so
    // the library folder is derived from a meaningful name, not the staging id.
    let skill_name = if parsed.subdir.is_empty() {
        parsed.repo.clone()
    } else {
        parsed
            .subdir
            .rsplit('/')
            .find(|segment| !segment.is_empty())
            .unwrap_or(&parsed.repo)
            .to_string()
    };

    let staging_parent = unique_staging(data_root)?;
    let skill_staging = staging_parent.join(&skill_name);

    let prepared = (|| {
        extract_subdir_from_zip(&zip_bytes, &parsed.subdir, &skill_staging)?;
        if !skill_staging.join("SKILL.md").is_file() {
            return Err(if parsed.subdir.is_empty() {
                "The repository root has no SKILL.md. Use a /tree/<branch>/<subdir> URL that points at the skill folder.".to_string()
            } else {
                format!("No SKILL.md was found at '{}' in the repository.", parsed.subdir)
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
    let url = canonical_url(&parsed.owner, &parsed.repo, &git_ref, &parsed.subdir);
    let source = GithubSource {
        kind: "github".to_string(),
        owner: parsed.owner.clone(),
        repo: parsed.repo.clone(),
        git_ref: git_ref.clone(),
        subdir: parsed.subdir.clone(),
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
        subdir: parsed.subdir,
        synced_commit,
        url,
        message: "Skill imported from GitHub into the shared library.".to_string(),
    })
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
    if library_root.is_dir() {
        let entries = fs::read_dir(&library_root)
            .map_err(|error| format!("Could not read the shared library: {error}"))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("Could not read a library entry: {error}"))?;
            let dir = entry.path();
            if dir.is_dir() {
                if let Some(source) = read_source_file(&dir) {
                    sources.push((dir, source));
                }
            }
        }
    }

    let mut results = Vec::new();
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

    let source = read_source_file(library_path)
        .ok_or_else(|| "This skill has no GitHub source to update from.".to_string())?;

    let agent = build_agent()?;
    let new_commit =
        latest_commit(&agent, &source.owner, &source.repo, &source.git_ref, &source.subdir)
            .map_err(|error| error.message())?;
    let zip_bytes = download_zipball(&agent, &source.owner, &source.repo, &source.git_ref)
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

    // Which targets currently have this skill enabled? Capture before mutating.
    let enabled: Vec<(String, String, PathBuf)> = target_profiles
        .iter()
        .filter(|(id, _name, root)| {
            let managed = targets::managed_skill_dir(root, library_path);
            managed.exists() && targets::is_managed_target_copy(&managed, library_path, id)
        })
        .cloned()
        .collect();

    // Write the refreshed source file into staging so the swap is atomic.
    let updated_source = GithubSource {
        synced_commit: new_commit.to_string(),
        synced_at: unix_ms(),
        ..source.clone()
    };
    write_source_file(new_skill_dir, &updated_source)?;

    // Soft-delete the current copy, then move the new content into place.
    let trash_path = unique_trash_path(data_root, &skill_name)?;
    fs::rename(library_path, &trash_path)
        .map_err(|error| format!("Could not move the current skill to trash: {error}"))?;
    if let Err(error) = fs::rename(new_skill_dir, library_path) {
        let _ = fs::rename(&trash_path, library_path);
        return Err(format!("Could not install the updated skill: {error}"));
    }

    // Re-copy the new content into each tool that had it enabled. The library
    // path is unchanged, so the markers still match: disable removes the stale
    // copy, enable lays down the fresh one.
    let mut refreshed_targets = Vec::new();
    let mut failed_targets = Vec::new();
    for (id, name, root) in &enabled {
        let result =
            targets::set_skill_target_enabled_with_root(library_path, id, false, data_root, root)
                .and_then(|_| {
                    targets::set_skill_target_enabled_with_root(
                        library_path,
                        id,
                        true,
                        data_root,
                        root,
                    )
                });
        match result {
            Ok(_) => refreshed_targets.push(name.clone()),
            Err(_) => failed_targets.push(name.clone()),
        }
    }

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

fn unique_trash_path(data_root: &Path, skill_name: &str) -> Result<PathBuf, String> {
    let trash_root = data_root.join("trash");
    fs::create_dir_all(&trash_root)
        .map_err(|error| format!("Could not create the trash folder: {error}"))?;
    for index in 0.. {
        let suffix = if index == 0 {
            String::new()
        } else {
            format!("-{index}")
        };
        let candidate = trash_root.join(format!("library-{skill_name}-{}{suffix}", unix_ms()));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    unreachable!("unbounded trash index should always find an available folder")
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
    let response = agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(classify_ureq_error)?;
    let body = response
        .into_string()
        .map_err(|error| GithubError::Other(format!("Could not read the GitHub response: {error}")))?;
    serde_json::from_str(&body)
        .map_err(|error| GithubError::Other(format!("Could not parse the GitHub response: {error}")))
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
            404 => GithubError::NotFound,
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
    for index in 0.. {
        let suffix = if index == 0 {
            String::new()
        } else {
            format!("-{index}")
        };
        let candidate = data_root.join(format!(".github-import-{}{suffix}", unix_ms()));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    unreachable!("unbounded staging index should always find an available folder")
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
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};
    use zip::write::{SimpleFileOptions, ZipWriter};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("skills-manage-github-{prefix}-{stamp}"))
    }

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

    // Exercises the live network path (TLS, GitHub API, codeload download, zip
    // extraction) against GitHub's canonical test repo. Ignored by default;
    // run manually with `cargo test -- --ignored`.
    #[test]
    #[ignore = "hits the live GitHub API"]
    fn live_resolves_and_downloads_octocat_hello_world() {
        let agent = build_agent().unwrap();

        let branch = resolve_default_branch(&agent, "octocat", "Hello-World").unwrap();
        assert_eq!(branch, "master");

        let sha = latest_commit(&agent, "octocat", "Hello-World", &branch, "").unwrap();
        assert_eq!(sha.len(), 40);

        let zip = download_zipball(&agent, "octocat", "Hello-World", &branch).unwrap();
        let dest = unique_temp_dir("live");
        extract_subdir_from_zip(&zip, "", &dest).unwrap();
        assert!(dest.join("README").exists() || dest.join("README.md").exists());

        fs::remove_dir_all(&dest).unwrap();
    }
}
