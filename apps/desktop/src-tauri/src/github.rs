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
use crate::variants;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    /// Tools with their own version of the skill in the repository: tool id ->
    /// folder (see `variants`). Older source files have none.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variants: BTreeMap<String, String>,
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
    install_from_zip(&zip_bytes, &parsed, &git_ref, &subdir, &synced_commit, data_root)
}

/// Place the skill at `subdir` of a downloaded archive in the library and
/// record where it came from.
fn install_from_zip(
    zip_bytes: &[u8],
    parsed: &GithubRef,
    git_ref: &str,
    subdir: &str,
    synced_commit: &str,
    data_root: &Path,
) -> Result<GithubImportResult, String> {
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
        extract_subdir_from_zip(zip_bytes, subdir, &skill_staging)?;
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
    let url = canonical_url(&parsed.owner, &parsed.repo, git_ref, subdir);
    let source = GithubSource {
        kind: "github".to_string(),
        owner: parsed.owner.clone(),
        repo: parsed.repo.clone(),
        git_ref: git_ref.to_string(),
        subdir: subdir.to_string(),
        synced_commit: synced_commit.to_string(),
        synced_at: unix_ms(),
        url: url.clone(),
        variants: install_variants(zip_bytes, subdir, &library_path, data_root),
    };
    write_source_file(&library_path, &source)?;

    Ok(GithubImportResult {
        skill_name: import.skill_name,
        library_path: import.library_path,
        owner: parsed.owner.clone(),
        repo: parsed.repo.clone(),
        git_ref: git_ref.to_string(),
        subdir: subdir.to_string(),
        synced_commit: synced_commit.to_string(),
        url,
        message: "Skill imported from GitHub into the shared library.".to_string(),
    })
}

/// Other tools' versions of a newly imported skill, kept beside it. Best
/// effort: the import itself never fails over them.
fn install_variants(zip_bytes: &[u8], subdir: &str, library_path: &Path, data_root: &Path) -> BTreeMap<String, String> {
    let Ok(found) = variants::detect_in_zip(zip_bytes, subdir) else {
        return BTreeMap::new();
    };
    let folder = dir_skill_name(library_path);
    let placed = variants::stage(zip_bytes, &found, data_root)
        .and_then(|staged| variants::put_in_place(data_root, &folder, staged.as_deref()));
    if placed.is_err() {
        return BTreeMap::new();
    }
    found.into_iter().collect()
}

/// The tools' versions in a freshly downloaded archive, staged for an update.
/// `None` (keep the current ones) when they could not be read.
fn stage_variant_update(zip_bytes: &[u8], subdir: &str, data_root: &Path) -> Option<variants::VariantUpdate> {
    let found = variants::detect_in_zip(zip_bytes, subdir).ok()?;
    let staged = variants::stage(zip_bytes, &found, data_root).ok()?;
    Some(variants::VariantUpdate { found: found.into_iter().collect(), staged })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedImport {
    pub url: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportResult {
    pub imported: Vec<GithubImportResult>,
    pub failed: Vec<FailedImport>,
}

/// Import several skills from GitHub. Skills from the same repository and
/// branch share one download; each records the last commit of its own folder.
pub fn import_github_skills(urls: Vec<String>) -> Result<BatchImportResult, String> {
    let data_root = default_data_root()?;
    let agent = build_agent()?;
    let mut archives: Vec<(String, Vec<u8>)> = Vec::new();
    let mut result = BatchImportResult { imported: Vec::new(), failed: Vec::new() };

    for url in urls {
        let outcome = (|| {
            let parsed = parse_github_url(&url)?;
            let (git_ref, subdir, path_commit) =
                resolve_ref_and_commit(&agent, &parsed).map_err(|error| error.message())?;
            let key = format!("{}/{}@{git_ref}", parsed.owner, parsed.repo).to_lowercase();
            if !archives.iter().any(|(known, _)| *known == key) {
                let head = latest_commit(&agent, &parsed.owner, &parsed.repo, &git_ref, "")
                    .map_err(|error| error.message())?;
                let bytes = download_zipball(&agent, &parsed.owner, &parsed.repo, &head)
                    .map_err(|error| error.message())?;
                archives.push((key.clone(), bytes));
            }
            let zip_bytes = &archives.iter().find(|(known, _)| *known == key).expect("archive was just added").1;
            // The branch head holds the same folder content as the folder's own
            // last commit, which is what update checks compare against.
            install_from_zip(zip_bytes, &parsed, &git_ref, &subdir, &path_commit, &data_root)
        })();
        match outcome {
            Ok(imported) => result.imported.push(imported),
            Err(error) => result.failed.push(FailedImport { url, error }),
        }
    }
    Ok(result)
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

    let variant_update = stage_variant_update(&zip_bytes, &source.subdir, data_root);
    let outcome = apply_skill_update(
        library_path,
        &staging_skill,
        &source,
        &new_commit,
        data_root,
        target_profiles,
        variant_update,
    );
    let _ = fs::remove_dir_all(&staging_parent);
    outcome
}

/// Replace the library skill in place with already-validated new content, then
/// refresh any tool copies that were enabled. No network here, so it is unit
/// tested directly. The previous library dir is soft-deleted to trash.
///
/// `variant_update` replaces the tools' own versions too (before the copies
/// are refreshed, so each tool gets its new version); `None` keeps them.
fn apply_skill_update(
    library_path: &Path,
    new_skill_dir: &Path,
    source: &GithubSource,
    new_commit: &str,
    data_root: &Path,
    target_profiles: &[(String, String, PathBuf)],
    variant_update: Option<variants::VariantUpdate>,
) -> Result<UpdateResult, String> {
    let skill_name = dir_skill_name(library_path);
    let previous_commit = source.synced_commit.clone();
    let discard_staged = |update: &Option<variants::VariantUpdate>| {
        if let Some(staged) = update.as_ref().and_then(|update| update.staged.as_ref()) {
            let _ = fs::remove_dir_all(staged);
        }
    };

    // Write the refreshed source file into staging so the swap is atomic.
    let updated_source = GithubSource {
        synced_commit: new_commit.to_string(),
        synced_at: unix_ms(),
        variants: variant_update
            .as_ref()
            .map_or_else(|| source.variants.clone(), |update| update.found.clone()),
        ..source.clone()
    };
    if let Err(error) = write_source_file(new_skill_dir, &updated_source) {
        discard_staged(&variant_update);
        return Err(error);
    }

    // Soft-delete the current copy, then move the new content into place.
    let trash_path = match unique_trash_path(data_root, &format!("library-{skill_name}")) {
        Ok(path) => path,
        Err(error) => {
            discard_staged(&variant_update);
            return Err(error);
        }
    };
    if let Err(error) = fs::rename(library_path, &trash_path) {
        discard_staged(&variant_update);
        return Err(format!("Could not move the current skill to trash: {error}"));
    }
    if let Err(error) = fs::rename(new_skill_dir, library_path) {
        let _ = fs::rename(&trash_path, library_path);
        discard_staged(&variant_update);
        return Err(format!("Could not install the updated skill: {error}"));
    }

    // Tools whose own version the repository no longer has fall back to the
    // library skill.
    let mut variants_note = String::new();
    if let Some(update) = &variant_update {
        let dropped: Vec<&str> = source
            .variants
            .keys()
            .filter(|tool| !update.found.contains_key(*tool))
            .map(|tool| targets::target_name_for(tool).unwrap_or(tool))
            .collect();
        match variants::put_in_place(data_root, &skill_name, update.staged.as_deref()) {
            Ok(()) if !dropped.is_empty() => {
                variants_note = format!(" Using the main version again for: {}.", dropped.join(", "));
            }
            Ok(()) => {}
            Err(error) => {
                discard_staged(&variant_update);
                variants_note = format!(" The tools' own versions were not updated: {error}");
            }
        }
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
        format!("Updated {skill_name} to the latest version.{variants_note}")
    } else {
        format!(
            "Updated {skill_name}, but could not refresh: {}.{variants_note}",
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
    if lines.next().map(str::trim_end) == Some("---") {
        if let Some(url) = lines.by_ref().take_while(|line| line.trim_end() != "---").find_map(github_url_in_line) {
            return Some(url);
        }
    }
    // The body links to many projects; only trust a repository named exactly
    // like the skill's folder (e.g. a `git clone …/<skill>.git` line).
    let folder = skill_dir.file_name()?.to_str()?.to_lowercase();
    contents
        .lines()
        .flat_map(github_urls_in_line)
        .map(|url| url.trim_end_matches(".git").to_string())
        .find(|url| parse_github_url(url).is_ok_and(|parsed| parsed.repo.to_lowercase() == folder))
}

/// Every GitHub URL in a line.
fn github_urls_in_line(line: &str) -> Vec<String> {
    line.match_indices("github.com/")
        .filter_map(|(start, _)| github_url_in_line(&line[start..]))
        .collect()
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
    let (files, truncated) = repo_files(agent, owner, repo, commit)?;
    Ok((files.into_iter().map(|(path, _)| path).collect(), truncated))
}

/// Every file in the repository at a commit or branch: (path, blob id).
pub(crate) fn repo_files(
    agent: &ureq::Agent,
    owner: &str,
    repo: &str,
    tree_ish: &str,
) -> Result<(Vec<(String, String)>, bool), GithubError> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/git/trees/{}?recursive=1",
        encode_query(tree_ish)
    );
    let json = api_get_json(agent, &url)?;
    let tree = json
        .get("tree")
        .and_then(|value| value.as_array())
        .ok_or_else(|| GithubError::Other("Unexpected response from GitHub trees API.".to_string()))?;
    let files = tree
        .iter()
        .filter(|item| item.get("type").and_then(|value| value.as_str()) == Some("blob"))
        .filter_map(|item| {
            let path = item.get("path")?.as_str()?.to_string();
            let sha = item.get("sha")?.as_str()?.to_string();
            Some((path, sha))
        })
        .collect();
    let truncated = json.get("truncated").and_then(|value| value.as_bool()).unwrap_or(false);
    Ok((files, truncated))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundSource {
    pub folder: String,
    pub repository: String,
    pub url: String,
    /// Identical SKILL.md upstream (false: matched by folder name only, so the
    /// installed copy is probably an older or edited version).
    pub exact: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindSourcesResult {
    /// Installed skills that had no source and were linked now.
    pub found: Vec<FoundSource>,
    /// Installed skills still without a source.
    pub unmatched: Vec<String>,
    /// Skills whose name exists in several repositories (none identical).
    pub ambiguous: Vec<String>,
    /// Repositories that were checked.
    pub repositories: Vec<String>,
    /// Repositories that could not be read (moved, private, network error).
    pub failed: Vec<String>,
    /// GitHub's hourly limit stopped the search early.
    pub rate_limited: bool,
}

/// Find where installed skills came from: look for each one's SKILL.md in
/// well-known skill repositories and the ones linked by hand, and link the
/// skills it recognises. Skills that already name a source are left alone.
pub fn find_skill_sources() -> Result<FindSourcesResult, String> {
    find_skill_sources_with(&crate::scanner::candidate_skill_roots(), &default_data_root()?)
}

fn find_skill_sources_with(roots: &[PathBuf], data_root: &Path) -> Result<FindSourcesResult, String> {
    let installed = unsourced_skills(roots);
    let repositories = series::candidate_repositories(data_root);
    let agent = build_agent()?;

    let mut remotes = Vec::new();
    let mut checked = Vec::new();
    let mut failed = Vec::new();
    let mut rate_limited = false;
    for repository in &repositories {
        let Some((owner, repo)) = repository.split_once('/') else {
            continue;
        };
        match repo_skills(&agent, owner, repo) {
            Ok(mut skills) => {
                checked.push(repository.clone());
                remotes.append(&mut skills);
            }
            Err(GithubError::RateLimited) => {
                rate_limited = true;
                break;
            }
            Err(_) => failed.push(repository.clone()),
        }
    }

    // The connected account's own and starred repositories (already read).
    for remote in crate::catalog::cached_remote_skills(data_root) {
        if !checked.iter().any(|repo| repo.eq_ignore_ascii_case(&remote.repository)) {
            remotes.push(remote);
        }
    }

    let links = series::load_links(data_root);
    let mut result = match_installed(&installed, &remotes, &links);

    // Still unknown: search all of GitHub for them (code search needs sign-in).
    let unknown: Vec<series::LocalSkill> = installed
        .iter()
        .filter(|skill| result.unmatched.contains(&skill.folder) || result.ambiguous.contains(&skill.folder))
        .cloned()
        .collect();
    if !unknown.is_empty() && !rate_limited && github_token().is_some() {
        let (found_remotes, searched, limited) = search_sources(&agent, &unknown, &checked);
        rate_limited |= limited;
        if !found_remotes.is_empty() {
            let second = match_installed(&unknown, &found_remotes, &links);
            let mut still_unknown = second.unmatched;
            for found in second.found {
                // Search turns up mirrors and collections, so a name alone is
                // only trusted when the repository holds several of the skills.
                if found.exact || shared_names(&found_remotes, &unknown, &found.repository) >= 2 {
                    result.found.push(found);
                } else {
                    still_unknown.push(found.folder);
                }
            }
            still_unknown.extend(second.ambiguous.iter().cloned());
            still_unknown.sort_by_key(|folder| folder.to_lowercase());
            result.unmatched = still_unknown;
            result.ambiguous = Vec::new();
        }
        checked.extend(searched);
    }

    let entries: Vec<(String, String)> =
        result.found.iter().map(|found| (found.folder.clone(), found.url.clone())).collect();
    if !entries.is_empty() {
        series::set_links(data_root, &entries)?;
    }
    result.repositories = checked;
    result.failed = failed;
    result.rate_limited = rate_limited;
    Ok(result)
}

/// How many of `locals` have a skill folder of the same name in `repository`.
fn shared_names(remotes: &[series::RemoteSkill], locals: &[series::LocalSkill], repository: &str) -> usize {
    locals
        .iter()
        .filter(|local| {
            remotes
                .iter()
                .any(|remote| remote.repository == repository && remote.folder.eq_ignore_ascii_case(&local.folder))
        })
        .count()
}

/// At most this many code searches per run (GitHub allows 10 a minute).
const MAX_SEARCHES: usize = 8;
/// At most this many repositories from the search results are read.
const MAX_SEARCHED_REPOS: usize = 15;

/// Search GitHub for repositories that publish the given skills, then read
/// them. Skills that share a name prefix (`remotion-*`) are searched once.
/// Returns the skills found, the repositories read, and whether a limit hit.
fn search_sources(
    agent: &ureq::Agent,
    unknown: &[series::LocalSkill],
    already_read: &[String],
) -> (Vec<series::RemoteSkill>, Vec<String>, bool) {
    let mut queries: Vec<&str> = Vec::new();
    let mut prefixes: Vec<String> = Vec::new();
    for skill in unknown {
        let prefix = series_prefix(&skill.folder, unknown);
        if let Some(prefix) = prefix {
            if prefixes.contains(&prefix) {
                continue;
            }
            prefixes.push(prefix);
        }
        queries.push(&skill.folder);
    }

    let mut repositories: Vec<String> = Vec::new();
    let mut rate_limited = false;
    for folder in queries.into_iter().take(MAX_SEARCHES) {
        let query = format!("\"name: {folder}\" filename:SKILL.md");
        let url = format!("https://api.github.com/search/code?per_page=20&q={}", encode_query(&query));
        match api_get_json(agent, &url) {
            Ok(json) => {
                let items = json.get("items").and_then(|value| value.as_array()).cloned().unwrap_or_default();
                for item in items {
                    let Some(name) = item.pointer("/repository/full_name").and_then(|value| value.as_str()) else {
                        continue;
                    };
                    let known = repositories.iter().chain(already_read).any(|repo| repo.eq_ignore_ascii_case(name));
                    if !known && repositories.len() < MAX_SEARCHED_REPOS {
                        repositories.push(name.to_string());
                    }
                }
            }
            Err(GithubError::RateLimited) => {
                rate_limited = true;
                break;
            }
            // Search's own (secondary) limit or a bad query: keep what we have.
            Err(_) => break,
        }
    }

    let remotes: std::sync::Mutex<Vec<series::RemoteSkill>> = std::sync::Mutex::new(Vec::new());
    let read: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for repository in &repositories {
            let (remotes, read) = (&remotes, &read);
            scope.spawn(move || {
                let Some((owner, repo)) = repository.split_once('/') else { return };
                if let Ok(mut skills) = repo_skills(agent, owner, repo) {
                    remotes.lock().unwrap().append(&mut skills);
                    read.lock().unwrap().push(repository.clone());
                }
            });
        }
    });
    // Keep the search order (best match first) for ties.
    let mut remotes = remotes.into_inner().unwrap_or_default();
    remotes.sort_by_key(|remote| repositories.iter().position(|repo| *repo == remote.repository));
    (remotes, read.into_inner().unwrap_or_default(), rate_limited)
}

/// The part before the first `-` when another of `skills` shares it.
fn series_prefix(folder: &str, skills: &[series::LocalSkill]) -> Option<String> {
    let (prefix, _) = folder.split_once('-')?;
    let prefix = format!("{}-", prefix.to_lowercase());
    let shared = skills.iter().filter(|skill| skill.folder.to_lowercase().starts_with(&prefix)).count();
    (shared >= 2).then_some(prefix)
}

/// Decide a source for each installed skill. Skills already linked keep their
/// link unless identical content is found elsewhere.
fn match_installed(
    installed: &[series::LocalSkill],
    remotes: &[series::RemoteSkill],
    links: &series::SeriesLinks,
) -> FindSourcesResult {
    let mut result = FindSourcesResult {
        found: Vec::new(),
        unmatched: Vec::new(),
        ambiguous: Vec::new(),
        repositories: Vec::new(),
        failed: Vec::new(),
        rate_limited: false,
    };
    for local in installed {
        let existing = series::linked_url(links, &local.folder);
        let (index, exact) = match series::match_source(local, remotes) {
            series::SourceMatch::Exact(index) => (index, true),
            series::SourceMatch::ByName(index) if existing.is_none() => (index, false),
            series::SourceMatch::ByName(_) => continue,
            series::SourceMatch::Ambiguous if existing.is_none() => {
                result.ambiguous.push(local.folder.clone());
                continue;
            }
            series::SourceMatch::None if existing.is_none() => {
                result.unmatched.push(local.folder.clone());
                continue;
            }
            _ => continue,
        };
        let remote = &remotes[index];
        if existing.as_deref() == Some(remote.url.as_str()) {
            continue;
        }
        result.found.push(FoundSource {
            folder: local.folder.clone(),
            repository: remote.repository.clone(),
            url: remote.url.clone(),
            exact,
        });
    }

    // A name found in several repositories most likely came with its
    // siblings: pick the repository most other skills were found in.
    let ambiguous = std::mem::take(&mut result.ambiguous);
    for folder in ambiguous {
        let count = |repository: &str| result.found.iter().filter(|found| found.repository == repository).count();
        let mut options: Vec<(usize, &series::RemoteSkill)> = remotes
            .iter()
            .filter(|remote| remote.folder.eq_ignore_ascii_case(&folder))
            .map(|remote| (count(&remote.repository), remote))
            .collect();
        options.sort_by(|left, right| right.0.cmp(&left.0));
        match options.as_slice() {
            [(best, remote), rest @ ..] if *best > 0 && rest.iter().all(|(other, _)| other < best) => {
                let remote = (*remote).clone();
                result.found.push(FoundSource { folder, repository: remote.repository, url: remote.url, exact: false });
            }
            _ => result.ambiguous.push(folder),
        }
    }
    result
}

/// The skill folders of a repository's default branch (two API requests).
fn repo_skills(agent: &ureq::Agent, owner: &str, repo: &str) -> Result<Vec<series::RemoteSkill>, GithubError> {
    let branch = resolve_default_branch(agent, owner, repo)?;
    let (files, _truncated) = repo_files(agent, owner, repo, &branch)?;
    let repository = format!("{owner}/{repo}");
    Ok(crate::catalog::skills_in_files(&files, owner, repo, &branch)
        .into_iter()
        .map(|skill| series::RemoteSkill {
            repository: repository.clone(),
            folder: skill.folder,
            url: skill.url,
            blob: skill.blob,
        })
        .collect())
}

/// Installed skill folders that have no source yet: not added from GitHub and
/// no repository named in SKILL.md. Copies with the same folder name (the same
/// skill in several tools) are merged.
fn unsourced_skills(roots: &[PathBuf]) -> Vec<series::LocalSkill> {
    let mut skills: Vec<series::LocalSkill> = Vec::new();
    let mut sourced: Vec<String> = Vec::new();
    for entry in roots.iter().filter_map(|root| fs::read_dir(root).ok()).flat_map(|entries| entries.flatten()) {
        let dir = entry.path();
        let folder = entry.file_name().to_string_lossy().to_string();
        let Ok(bytes) = fs::read(dir.join("SKILL.md")) else {
            continue;
        };
        if folder.starts_with('.') {
            continue;
        }
        if read_source_file(&dir).is_some() || detect_github_url(&dir).is_some() {
            sourced.push(folder.to_lowercase());
            continue;
        }
        let blobs = series::local_blob_ids(&bytes);
        match skills.iter_mut().find(|skill| skill.folder.eq_ignore_ascii_case(&folder)) {
            Some(skill) => skill.blobs.extend(blobs),
            None => skills.push(series::LocalSkill { folder, blobs }),
        }
    }
    skills.retain(|skill| !sourced.contains(&skill.folder.to_lowercase()));
    skills.sort_by_key(|skill| skill.folder.to_lowercase());
    skills
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
        variants: BTreeMap::new(),
    };
    let variant_update = stage_variant_update(&zip_bytes, &subdir, data_root);
    let outcome =
        apply_skill_update(library_path, &staging_skill, &source, &commit, data_root, target_profiles, variant_update);
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

pub(crate) fn build_agent() -> Result<ureq::Agent, String> {
    let connector =
        native_tls::TlsConnector::new().map_err(|error| format!("Could not initialize TLS: {error}"))?;
    Ok(ureq::AgentBuilder::new()
        .redirects(5)
        .timeout(Duration::from_secs(120))
        .tls_connector(Arc::new(connector))
        .build())
}

pub(crate) fn resolve_default_branch(
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

pub(crate) fn latest_commit(
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

pub(crate) fn download_zipball(
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

pub(crate) fn api_get_json(agent: &ureq::Agent, url: &str) -> Result<serde_json::Value, GithubError> {
    api_get_json_with_token(agent, url, github_token().as_deref())
}

/// GET a GitHub API URL as JSON, with an explicit token (or none).
pub(crate) fn api_get_json_with_token(
    agent: &ureq::Agent,
    url: &str,
    token: Option<&str>,
) -> Result<serde_json::Value, GithubError> {
    let mut request = agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28");
    // An optional token raises the API limit from 60 to 5000 requests/hour. It
    // is only sent to api.github.com (ureq drops auth headers on redirects).
    if let Some(token) = token {
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

/// A token from the environment, else the account connected in the app.
pub(crate) fn github_token() -> Option<String> {
    crate::account::environment_token().or_else(crate::account::stored_token)
}

/// Classified failure from a GitHub request, so callers can distinguish a
/// missing/moved source and a rate limit from a generic error.
#[derive(Debug)]
pub(crate) enum GithubError {
    NotFound,
    RateLimited,
    Other(String),
}

impl GithubError {
    pub(crate) fn message(&self) -> String {
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
pub(crate) fn extract_subdir_from_zip(zip_bytes: &[u8], subdir: &str, dest: &Path) -> Result<(), String> {
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

pub(crate) fn canonical_url(owner: &str, repo: &str, git_ref: &str, subdir: &str) -> String {
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
            variants: BTreeMap::new(),
            url: "https://github.com/o/r/tree/main/skills/foo".to_string(),
        }
    }

    #[test]
    fn imports_keep_other_tools_versions_and_updates_refresh_them() {
        use crate::variants::tests::build_zip as repo_zip;
        let data_root = unique_temp_dir("import-variants");
        let main = |body: &str| format!("---\nname: dev\ndescription: Main\n---\n{body}\n");
        let parsed = parse_github_url("https://github.com/o/r/tree/main/skills/dev").unwrap();
        let first = repo_zip(&[
            ("skills/dev/SKILL.md", &main("Main version")),
            (".codex/skills/dev/SKILL.md", "---\nname: dev\ndescription: Codex\n---\nCodex version\n"),
        ]);
        let imported = install_from_zip(&first, &parsed, "main", "skills/dev", "c1", &data_root).unwrap();
        let library = PathBuf::from(&imported.library_path);
        let source = read_source_file(&library).unwrap();
        assert_eq!(source.variants.get("codex").map(String::as_str), Some(".codex/skills/dev"));
        assert!(fs::read_to_string(library.join("SKILL.md")).unwrap().contains("Main version"));
        assert!(data_root.join("variants").join("dev").join("codex").join("SKILL.md").is_file());

        let codex_root = data_root.join("codex-root");
        crate::targets::set_skill_target_enabled_with_root(&library, "codex", true, &data_root, &codex_root).unwrap();
        let codex_copy = codex_root.join("dev").join("SKILL.md");
        assert!(fs::read_to_string(&codex_copy).unwrap().contains("Codex version"));
        let profiles = vec![("codex".to_string(), "Codex".to_string(), codex_root.clone())];

        let update = |zip: &[u8], body: &str, commit: &str| {
            let staging = unique_temp_dir("import-variants-staging");
            let new_skill = staging.join("dev");
            fs::create_dir_all(&new_skill).unwrap();
            fs::write(new_skill.join("SKILL.md"), main(body)).unwrap();
            let source = read_source_file(&library).unwrap();
            let staged = stage_variant_update(zip, "skills/dev", &data_root);
            let result = apply_skill_update(&library, &new_skill, &source, commit, &data_root, &profiles, staged).unwrap();
            let _ = fs::remove_dir_all(staging);
            result
        };

        // The repository changed its Codex version: Codex gets the new one.
        let second = repo_zip(&[
            ("skills/dev/SKILL.md", &main("Main v2")),
            (".codex/skills/dev/SKILL.md", "---\nname: dev\ndescription: Codex\n---\nCodex v2\n"),
        ]);
        update(&second, "Main v2", "c2");
        assert!(fs::read_to_string(&codex_copy).unwrap().contains("Codex v2"));

        // The repository dropped it: Codex falls back to the main version.
        let third = repo_zip(&[("skills/dev/SKILL.md", &main("Main v3"))]);
        let result = update(&third, "Main v3", "c3");
        assert!(result.message.contains("Using the main version again for: Codex."), "{}", result.message);
        assert!(fs::read_to_string(&codex_copy).unwrap().contains("Main v3"));
        assert!(read_source_file(&library).unwrap().variants.is_empty());
        assert!(!data_root.join("variants").join("dev").exists());

        let _ = fs::remove_dir_all(&data_root);
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
            None,
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
            apply_skill_update(&library, &new_skill, &source, "abc1234def", &data_root, &profiles, None)
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
    fn find_sources_skips_skills_that_already_name_one_and_merges_copies() {
        let codex = unique_temp_dir("unsourced-codex");
        let claude = unique_temp_dir("unsourced-claude");
        for (root, folder, body) in [
            (&codex, "tdd", "---\nname: tdd\n---\nv1\n"),
            (&claude, "TDD", "---\nname: tdd\n---\nv2\n"),
            (&codex, "reach", "---\nname: reach\nhomepage: https://github.com/a/reach\n---\n"),
            (&codex, ".system", "---\nname: hidden\n---\n"),
        ] {
            fs::create_dir_all(root.join(folder)).unwrap();
            fs::write(root.join(folder).join("SKILL.md"), body).unwrap();
        }
        let skills = unsourced_skills(&[codex.clone(), claude.clone()]);
        let _ = fs::remove_dir_all(codex);
        let _ = fs::remove_dir_all(claude);

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].folder.to_lowercase(), "tdd");
        assert_eq!(skills[0].blobs.len(), 2);
    }

    #[test]
    fn find_sources_keeps_hand_made_links_unless_content_is_identical_elsewhere() {
        let remote = |repository: &str, folder: &str, blob: &str| series::RemoteSkill {
            repository: repository.to_string(),
            folder: folder.to_string(),
            url: format!("https://github.com/{repository}/tree/main/{folder}"),
            blob: blob.to_string(),
        };
        let remotes = vec![remote("a/one", "tdd", "x"), remote("b/two", "docx", "y"), remote("c/three", "lone", "z")];
        let local = |folder: &str, blob: &str| series::LocalSkill { folder: folder.to_string(), blobs: vec![blob.to_string()] };
        let mut links = series::SeriesLinks::new();
        links.insert("tdd".to_string(), "https://github.com/mine/fork/tree/main/tdd".to_string());
        links.insert("docx".to_string(), "https://github.com/mine/fork/tree/main/docx".to_string());

        let result = match_installed(
            &[local("tdd", "old"), local("docx", "y"), local("lone", "edited"), local("mystery", "?")],
            &remotes,
            &links,
        );

        let found: Vec<(&str, bool)> = result.found.iter().map(|item| (item.folder.as_str(), item.exact)).collect();
        assert_eq!(found, vec![("docx", true), ("lone", false)]);
        assert_eq!(result.unmatched, vec!["mystery".to_string()]);
    }

    #[test]
    fn trusts_a_body_link_only_to_the_skills_own_repository() {
        let root = unique_temp_dir("detect-body");
        let dir = root.join("wallstreet-explainer");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            "---
name: wallstreet-explainer
---
Uses https://github.com/babel/babel.
Install: git clone https://github.com/HeyChengC/wallstreet-explainer.git
",
        )
        .unwrap();
        assert_eq!(
            detect_github_url(&dir).as_deref(),
            Some("https://github.com/HeyChengC/wallstreet-explainer")
        );
        fs::write(dir.join("SKILL.md"), "---
name: x
---
See https://github.com/babel/babel
").unwrap();
        assert_eq!(detect_github_url(&dir), None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn skills_sharing_a_prefix_are_searched_once() {
        let local = |folder: &str| series::LocalSkill { folder: folder.to_string(), blobs: Vec::new() };
        let skills = [local("remotion-render"), local("Remotion-docs"), local("wallstreet-explainer"), local("solo")];
        assert_eq!(series_prefix("remotion-render", &skills).as_deref(), Some("remotion-"));
        assert_eq!(series_prefix("wallstreet-explainer", &skills), None);
        assert_eq!(series_prefix("solo", &skills), None);
    }

    #[test]
    fn a_name_in_several_repositories_goes_with_its_siblings() {
        let remote = |repository: &str, folder: &str| series::RemoteSkill {
            repository: repository.to_string(),
            folder: folder.to_string(),
            url: format!("https://github.com/{repository}/tree/main/{folder}"),
            blob: format!("{repository}/{folder}"),
        };
        let remotes = vec![
            remote("anthropics/skills", "pdf"),
            remote("anthropics/skills", "docx"),
            remote("openai/skills", "pdf"),
            remote("openai/skills", "sheets"),
        ];
        let local = |folder: &str| series::LocalSkill { folder: folder.to_string(), blobs: vec!["edited".to_string()] };

        let result = match_installed(&[local("docx"), local("pdf")], &remotes, &series::SeriesLinks::new());
        let pdf = result.found.iter().find(|found| found.folder == "pdf").unwrap();
        assert_eq!(pdf.repository, "anthropics/skills");
        assert!(result.ambiguous.is_empty());

        // With no siblings to go by, it stays undecided.
        let alone = match_installed(&[local("pdf")], &remotes, &series::SeriesLinks::new());
        assert_eq!(alone.ambiguous, vec!["pdf".to_string()]);
    }

    #[test]
    #[ignore = "hits the live GitHub API"]
    fn live_imports_several_skills_from_one_repository() {
        let data_root = unique_temp_dir("live-batch");
        let agent = build_agent().unwrap();
        let mut archives = Vec::new();
        let mut imported = Vec::new();
        for url in [
            "https://github.com/anthropics/skills/tree/main/skills/pdf",
            "https://github.com/anthropics/skills/tree/main/skills/docx",
        ] {
            let parsed = parse_github_url(url).unwrap();
            let (git_ref, subdir, commit) = resolve_ref_and_commit(&agent, &parsed).unwrap();
            if archives.is_empty() {
                let head = latest_commit(&agent, &parsed.owner, &parsed.repo, &git_ref, "").unwrap();
                archives.push(download_zipball(&agent, &parsed.owner, &parsed.repo, &head).unwrap());
            }
            imported.push(install_from_zip(&archives[0], &parsed, &git_ref, &subdir, &commit, &data_root).unwrap());
        }
        assert_eq!(archives.len(), 1);
        for result in &imported {
            assert!(Path::new(&result.library_path).join("SKILL.md").is_file());
            // Recorded per folder, so an update check finds nothing new.
            let source = read_source_file(Path::new(&result.library_path)).unwrap();
            let latest = latest_commit(&agent, "anthropics", "skills", &source.git_ref, &source.subdir).unwrap();
            assert_eq!(source.synced_commit, latest);
        }
        let _ = fs::remove_dir_all(data_root);
    }

    #[test]
    #[ignore = "hits the live GitHub API"]
    fn live_finds_sources_in_the_known_repositories() {
        let root = unique_temp_dir("live-find-root");
        let data_root = unique_temp_dir("live-find-data");
        for (folder, body) in [
            ("grill-me", "---\nname: grill-me\n---\nan edited copy\n"),
            ("pdf", "---\nname: pdf\n---\nan edited copy\n"),
            ("not-a-published-skill-xyz", "---\nname: x\n---\n"),
        ] {
            fs::create_dir_all(root.join(folder)).unwrap();
            fs::write(root.join(folder).join("SKILL.md"), body).unwrap();
        }

        let result = find_skill_sources_with(&[root.clone()], &data_root).unwrap();
        let _ = fs::remove_dir_all(&root);

        assert!(!result.rate_limited, "rate limited");
        assert!(result.repositories.iter().any(|repo| repo == "mattpocock/skills"), "{:?}", result.failed);
        assert_eq!(result.found.len(), 1);
        assert_eq!(result.found[0].repository, "mattpocock/skills");
        assert!(!result.found[0].exact);
        assert_eq!(result.ambiguous, vec!["pdf".to_string()]);
        assert_eq!(result.unmatched, vec!["not-a-published-skill-xyz".to_string()]);
        assert!(series::linked_url(&series::load_links(&data_root), "grill-me").is_some());
        let _ = fs::remove_dir_all(data_root);
    }

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
