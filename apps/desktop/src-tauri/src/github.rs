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
        None => resolve_default_branch(&agent, &parsed.owner, &parsed.repo)?,
    };

    // Resolving the commit also validates that the subdir exists at this ref.
    let synced_commit =
        latest_commit(&agent, &parsed.owner, &parsed.repo, &git_ref, &parsed.subdir)?;

    let zip_bytes = download_zipball(&agent, &parsed.owner, &parsed.repo, &git_ref)?;

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

fn build_agent() -> Result<ureq::Agent, String> {
    let connector =
        native_tls::TlsConnector::new().map_err(|error| format!("Could not initialize TLS: {error}"))?;
    Ok(ureq::AgentBuilder::new()
        .redirects(5)
        .timeout(Duration::from_secs(120))
        .tls_connector(Arc::new(connector))
        .build())
}

fn resolve_default_branch(agent: &ureq::Agent, owner: &str, repo: &str) -> Result<String, String> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}");
    let json = api_get_json(agent, &url)?;
    json.get("default_branch")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .ok_or_else(|| "Could not determine the repository's default branch.".to_string())
}

fn latest_commit(
    agent: &ureq::Agent,
    owner: &str,
    repo: &str,
    git_ref: &str,
    subdir: &str,
) -> Result<String, String> {
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
        .ok_or_else(|| "Unexpected response from GitHub commits API.".to_string())?;
    let first = commits.first().ok_or_else(|| {
        if subdir.is_empty() {
            "The repository has no commits.".to_string()
        } else {
            format!("Path '{subdir}' was not found in the repository.")
        }
    })?;
    first
        .get("sha")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .ok_or_else(|| "GitHub commit response was missing a sha.".to_string())
}

fn download_zipball(
    agent: &ureq::Agent,
    owner: &str,
    repo: &str,
    git_ref: &str,
) -> Result<Vec<u8>, String> {
    let url = format!("https://codeload.github.com/{owner}/{repo}/zip/{git_ref}");
    let response = agent
        .get(&url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(describe_ureq_error)?;

    let mut buffer = Vec::new();
    response
        .into_reader()
        .take(MAX_DOWNLOAD_BYTES + 1)
        .read_to_end(&mut buffer)
        .map_err(|error| format!("Could not download the repository archive: {error}"))?;

    if buffer.is_empty() {
        return Err("The downloaded repository archive was empty.".to_string());
    }
    if buffer.len() as u64 > MAX_DOWNLOAD_BYTES {
        return Err("The repository archive is too large to import.".to_string());
    }
    Ok(buffer)
}

fn api_get_json(agent: &ureq::Agent, url: &str) -> Result<serde_json::Value, String> {
    let response = agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(describe_ureq_error)?;
    let body = response
        .into_string()
        .map_err(|error| format!("Could not read the GitHub response: {error}"))?;
    serde_json::from_str(&body).map_err(|error| format!("Could not parse the GitHub response: {error}"))
}

fn describe_ureq_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, response) => match code {
            404 => "Repository or path not found. Only public repositories are supported.".to_string(),
            401 | 403 => {
                if response.header("x-ratelimit-remaining") == Some("0") {
                    "GitHub's hourly rate limit was reached. Try again later.".to_string()
                } else {
                    "GitHub denied the request. Only public repositories are supported.".to_string()
                }
            }
            _ => format!("GitHub returned an unexpected status ({code})."),
        },
        ureq::Error::Transport(transport) => format!("Network error reaching GitHub: {transport}"),
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
