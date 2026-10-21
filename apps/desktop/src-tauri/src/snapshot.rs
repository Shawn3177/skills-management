//! A GitHub repository at one commit, as the files GitHub hands over.
//!
//! Usually that is the zipball from codeload. But GitHub builds archives with
//! `git archive`, which leaves out every path `.gitattributes` marks
//! `export-ignore` — often `.agents/`, `.claude/` and the like, exactly where
//! skills live. For those repositories the file list comes from the git trees
//! API instead and each file the archive left out is read as a blob (see
//! `github::open_snapshot`). Both sources implement `RepoSnapshot`, so the
//! importer and the per-tool version detection do not care which one they got.
//!
//! Every path is checked with `is_safe_relative` before it is written, and
//! reads are bounded (entry count, total size, and blob requests).

use crate::fs_ops::is_safe_relative;
use base64::Engine;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;
use zip::ZipArchive;

pub(crate) const MAX_ENTRIES: usize = 10_000;
pub(crate) const MAX_TOTAL_BYTES: u64 = 100 * 1024 * 1024; // 100 MB uncompressed
/// Each blob is one API request (60 an hour without a GitHub account).
const MAX_BLOB_FETCHES: usize = 500;

/// A repository's files at one commit.
pub(crate) trait RepoSnapshot {
    /// Every file: (path from the repository root, size in bytes).
    fn files(&self) -> &[(String, u64)];
    /// One file's contents.
    fn read(&mut self, path: &str) -> Result<Vec<u8>, String>;
}

/// A downloaded zipball. Paths drop the archive's top-level folder.
pub(crate) struct ZipSnapshot<'a> {
    archive: ZipArchive<Cursor<&'a [u8]>>,
    top: String,
    files: Vec<(String, u64)>,
    paths: HashSet<String>,
}

impl<'a> ZipSnapshot<'a> {
    pub(crate) fn new(zip_bytes: &'a [u8]) -> Result<Self, String> {
        let mut archive = ZipArchive::new(Cursor::new(zip_bytes))
            .map_err(|error| format!("Could not read the downloaded archive: {error}"))?;
        if archive.is_empty() {
            return Err("The downloaded archive was empty.".to_string());
        }
        if archive.len() > MAX_ENTRIES {
            return Err("The downloaded archive has too many entries.".to_string());
        }
        // The top-level folder name is read from the archive, not reconstructed.
        let top = archive.name_for_index(0).and_then(|name| name.split('/').next()).unwrap_or("").to_string();
        if top.is_empty() {
            return Err("The downloaded archive has an unexpected layout.".to_string());
        }
        let prefix = format!("{top}/");
        let mut files = Vec::new();
        for index in 0..archive.len() {
            let entry = archive.by_index_raw(index).map_err(|error| format!("Could not read the archive: {error}"))?;
            if entry.is_dir() {
                continue;
            }
            if let Some(path) = entry.name().strip_prefix(&prefix).filter(|path| !path.is_empty()) {
                files.push((path.to_string(), entry.size()));
            }
        }
        let paths = files.iter().map(|(path, _)| path.clone()).collect();
        Ok(ZipSnapshot { archive, top, files, paths })
    }

    fn contains(&self, path: &str) -> bool {
        self.paths.contains(path)
    }
}

impl RepoSnapshot for ZipSnapshot<'_> {
    fn files(&self) -> &[(String, u64)] {
        &self.files
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>, String> {
        let name = format!("{}/{path}", self.top);
        let mut entry = self.archive.by_name(&name).map_err(|error| format!("Could not read {path}: {error}"))?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|error| format!("Could not read {path}: {error}"))?;
        Ok(bytes)
    }
}

/// A file in a git tree listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TreeFile {
    pub path: String,
    pub sha: String,
    pub size: u64,
}

/// Reads one blob by id.
pub(crate) type BlobFetcher<'a> = Box<dyn FnMut(&str) -> Result<Vec<u8>, String> + 'a>;

/// The full file list from the trees API. Files the archive has are read from
/// it; the ones it left out are fetched as blobs.
pub(crate) struct TreeSnapshot<'a> {
    files: Vec<(String, u64)>,
    blobs: BTreeMap<String, String>,
    archive: Option<ZipSnapshot<'a>>,
    fetch: BlobFetcher<'a>,
    fetched: BTreeMap<String, Vec<u8>>,
    /// GitHub cut the listing short (very large repository).
    pub truncated: bool,
}

impl<'a> TreeSnapshot<'a> {
    pub(crate) fn new(
        tree: Vec<TreeFile>,
        truncated: bool,
        archive: Option<ZipSnapshot<'a>>,
        fetch: BlobFetcher<'a>,
    ) -> Self {
        let files = tree.iter().map(|file| (file.path.clone(), file.size)).collect();
        let blobs = tree.into_iter().map(|file| (file.path, file.sha)).collect();
        TreeSnapshot { files, blobs, archive, fetch, fetched: BTreeMap::new(), truncated }
    }
}

impl RepoSnapshot for TreeSnapshot<'_> {
    fn files(&self) -> &[(String, u64)] {
        &self.files
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>, String> {
        if let Some(archive) = self.archive.as_mut().filter(|archive| archive.contains(path)) {
            return archive.read(path);
        }
        let sha = self.blobs.get(path).ok_or_else(|| format!("{path} is not in the repository."))?;
        if let Some(bytes) = self.fetched.get(sha) {
            return Ok(bytes.clone());
        }
        if self.fetched.len() >= MAX_BLOB_FETCHES {
            return Err(format!(
                "GitHub leaves '{path}' out of its download archive (the repository marks it export-ignore in .gitattributes), and there are too many such files to read one by one."
            ));
        }
        let bytes = (self.fetch)(sha).map_err(|error| {
            format!(
                "GitHub leaves '{path}' out of its download archive (the repository marks it export-ignore in .gitattributes), and reading it through the GitHub API failed: {error}"
            )
        })?;
        self.fetched.insert(sha.clone(), bytes.clone());
        Ok(bytes)
    }
}

/// The files of a `GET /repos/{o}/{r}/git/trees/{sha}?recursive=1` response,
/// and whether GitHub truncated the listing. Entries that are not plain files
/// (folders, submodules) or have an odd blob id are skipped.
pub(crate) fn parse_tree(json: &serde_json::Value) -> Result<(Vec<TreeFile>, bool), String> {
    let tree = json
        .get("tree")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "Unexpected response from GitHub trees API.".to_string())?;
    let files = tree
        .iter()
        .filter(|item| item.get("type").and_then(|value| value.as_str()) == Some("blob"))
        .filter_map(|item| {
            let path = item.get("path")?.as_str()?.to_string();
            let sha = item.get("sha")?.as_str()?.to_string();
            let size = item.get("size").and_then(|value| value.as_u64()).unwrap_or(0);
            // The id goes into a request URL later.
            let valid = sha.len() == 40 && sha.bytes().all(|byte| byte.is_ascii_hexdigit());
            valid.then_some(TreeFile { path, sha, size })
        })
        .collect();
    let truncated = json.get("truncated").and_then(|value| value.as_bool()).unwrap_or(false);
    Ok((files, truncated))
}

/// The contents of a `GET /repos/{o}/{r}/git/blobs/{sha}` response.
pub(crate) fn decode_blob(json: &serde_json::Value) -> Result<Vec<u8>, String> {
    let content = json
        .get("content")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "Unexpected response from GitHub blobs API.".to_string())?;
    match json.get("encoding").and_then(|value| value.as_str()) {
        Some("base64") => {
            let packed: String = content.chars().filter(|character| !character.is_ascii_whitespace()).collect();
            base64::engine::general_purpose::STANDARD
                .decode(packed)
                .map_err(|error| format!("Could not decode a file from GitHub: {error}"))
        }
        Some("utf-8") => Ok(content.as_bytes().to_vec()),
        _ => Err("Unexpected file encoding from GitHub blobs API.".to_string()),
    }
}

/// True when the snapshot has a file inside `dir` ("" = anywhere).
pub(crate) fn has_dir(snapshot: &dyn RepoSnapshot, dir: &str) -> bool {
    let prefix = dir_prefix(dir);
    snapshot.files().iter().any(|(path, _)| path.starts_with(&prefix))
}

fn dir_prefix(dir: &str) -> String {
    let dir = dir.trim_matches('/');
    if dir.is_empty() {
        String::new()
    } else {
        format!("{dir}/")
    }
}

/// Write just `subdir` (or the whole repository when empty) into `dest`.
pub(crate) fn extract_subdir(snapshot: &mut dyn RepoSnapshot, subdir: &str, dest: &Path) -> Result<(), String> {
    let prefix = dir_prefix(subdir);
    let wanted: Vec<(String, u64)> =
        snapshot.files().iter().filter(|(path, _)| path.starts_with(&prefix)).cloned().collect();
    if wanted.is_empty() {
        return Err(if subdir.is_empty() {
            "The repository appears to be empty.".to_string()
        } else {
            format!("Path '{subdir}' was not found in the repository.")
        });
    }
    // Check everything before reading anything: a tree snapshot reads over the network.
    if wanted.len() > MAX_ENTRIES {
        return Err("The skill has too many files to import.".to_string());
    }
    if wanted.iter().map(|(_, size)| *size).fold(0u64, u64::saturating_add) > MAX_TOTAL_BYTES {
        return Err("The skill content is too large to import.".to_string());
    }
    if let Some((path, _)) = wanted.iter().find(|(path, _)| !is_safe_relative(&path[prefix.len()..])) {
        return Err(format!("The repository contains an unsafe path: {path}"));
    }

    fs::create_dir_all(dest).map_err(|error| format!("Could not create the staging folder: {error}"))?;
    let mut total_bytes: u64 = 0;
    for (path, _) in &wanted {
        let bytes = snapshot.read(path)?;
        // Sizes in a listing could understate what is actually read.
        total_bytes = total_bytes.saturating_add(bytes.len() as u64);
        if total_bytes > MAX_TOTAL_BYTES {
            return Err("The skill content is too large to import.".to_string());
        }
        let out_path = dest.join(&path[prefix.len()..]);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
        }
        fs::write(&out_path, &bytes).map_err(|error| format!("Could not write {}: {error}", out_path.display()))?;
    }
    Ok(())
}

/// True when `.gitattributes` text marks folder `dir`, or a folder above it,
/// `export-ignore` — so GitHub's archives leave it out.
pub(crate) fn export_ignored(gitattributes: &str, dir: &str) -> bool {
    let parts: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
    gitattributes
        .lines()
        .filter_map(export_ignore_pattern)
        .any(|pattern| (1..=parts.len()).any(|len| pattern_matches(pattern, &parts[..len])))
}

fn export_ignore_pattern(line: &str) -> Option<&str> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let mut fields = line.split_whitespace();
    let pattern = fields.next()?;
    fields.any(|attribute| attribute == "export-ignore").then_some(pattern)
}

/// gitattributes pattern rules, enough for folders: a pattern with a `/`
/// (other than a trailing one) is anchored at the root, otherwise it matches a
/// folder name at any depth.
fn pattern_matches(pattern: &str, parts: &[&str]) -> bool {
    let pattern = pattern.trim_end_matches('/');
    let pattern = pattern.strip_suffix("/**").unwrap_or(pattern);
    let (pattern, anchored) = match pattern.strip_prefix("**/") {
        Some(rest) => (rest, rest.contains('/')),
        None => (pattern.trim_start_matches('/'), pattern.contains('/')),
    };
    if !anchored {
        return parts.last().is_some_and(|name| glob(pattern.as_bytes(), name.as_bytes()));
    }
    let pattern_parts: Vec<&str> = pattern.split('/').collect();
    pattern_parts.len() == parts.len()
        && pattern_parts.iter().zip(parts).all(|(pattern, part)| glob(pattern.as_bytes(), part.as_bytes()))
}

/// `*` and `?` within one path segment.
fn glob(pattern: &[u8], text: &[u8]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some((b'*', rest)) => (0..=text.len()).any(|skip| glob(rest, &text[skip..])),
        Some((b'?', rest)) => !text.is_empty() && glob(rest, &text[1..]),
        Some((byte, rest)) => text.first() == Some(byte) && glob(rest, &text[1..]),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::fs_ops::test_temp_dir;
    use std::cell::Cell;
    use std::rc::Rc;

    /// The git blob id of `content`, as the trees API reports it.
    fn blob_id(content: &str) -> String {
        use sha1::{Digest, Sha1};
        let mut hasher = Sha1::new();
        hasher.update(format!("blob {}\0", content.len()));
        hasher.update(content);
        hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// A trees API response for `files`, and a fetcher that answers blob
    /// requests the way the blobs API does (base64, wrapped at 60 columns),
    /// counting each request.
    pub(crate) fn fake_tree(files: &[(&str, &str)]) -> (serde_json::Value, BlobFetcher<'static>, Rc<Cell<usize>>) {
        let mut tree = vec![serde_json::json!({ "path": ".agents", "type": "tree", "sha": "0".repeat(40) })];
        let mut blobs = BTreeMap::new();
        for (path, content) in files {
            let sha = blob_id(content);
            tree.push(serde_json::json!({ "path": path, "type": "blob", "sha": sha, "size": content.len() }));
            let encoded = base64::engine::general_purpose::STANDARD.encode(content);
            let wrapped: Vec<String> =
                encoded.as_bytes().chunks(60).map(|chunk| String::from_utf8_lossy(chunk).to_string()).collect();
            let response = serde_json::json!({ "sha": sha, "encoding": "base64", "content": wrapped.join("\n") + "\n" });
            blobs.insert(sha, response);
        }
        let requests = Rc::new(Cell::new(0));
        let counter = requests.clone();
        let fetch: BlobFetcher<'static> = Box::new(move |sha: &str| {
            counter.set(counter.get() + 1);
            decode_blob(blobs.get(sha).ok_or("404 Not Found")?)
        });
        (serde_json::json!({ "sha": "f".repeat(40), "tree": tree, "truncated": false }), fetch, requests)
    }

    pub(crate) fn tree_snapshot<'a>(
        files: &[(&str, &str)],
        archive: Option<ZipSnapshot<'a>>,
    ) -> (TreeSnapshot<'a>, Rc<Cell<usize>>) {
        let (json, fetch, requests) = fake_tree(files);
        let (tree, truncated) = parse_tree(&json).unwrap();
        (TreeSnapshot::new(tree, truncated, archive, fetch), requests)
    }

    #[test]
    fn reads_a_tree_listing_and_its_blobs() {
        let (json, mut fetch, _) = fake_tree(&[("a/SKILL.md", "héllo — a longer line that wraps past sixty base64 characters\n")]);
        let (files, truncated) = parse_tree(&json).unwrap();
        assert!(!truncated);
        assert_eq!(files.len(), 1, "folders are not files");
        assert_eq!(files[0].path, "a/SKILL.md");
        assert_eq!(
            String::from_utf8(fetch(&files[0].sha).unwrap()).unwrap(),
            "héllo — a longer line that wraps past sixty base64 characters\n"
        );

        let odd = serde_json::json!({ "tree": [{ "path": "x", "type": "blob", "sha": "../../evil", "size": 1 }] });
        assert!(parse_tree(&odd).unwrap().0.is_empty());
        assert!(decode_blob(&serde_json::json!({ "content": "x", "encoding": "rot13" })).is_err());
    }

    #[test]
    fn extracts_left_out_files_through_blobs_and_the_rest_from_the_archive() {
        let zip = crate::variants::tests::build_zip(&[("README.md", "readme"), ("skills/x/SKILL.md", "x")]);
        let files = [
            ("README.md", "readme"),
            ("skills/x/SKILL.md", "x"),
            (".agents/skills/dev/SKILL.md", "---\nname: dev\n---\n"),
            (".agents/skills/dev/scripts/run.sh", "echo ok\n"),
        ];
        let (mut snapshot, requests) = tree_snapshot(&files, Some(ZipSnapshot::new(&zip).unwrap()));
        assert!(has_dir(&snapshot, ".agents/skills/dev"));

        let dest = test_temp_dir("tree-extract");
        extract_subdir(&mut snapshot, ".agents/skills/dev", &dest).unwrap();
        assert_eq!(fs::read_to_string(dest.join("SKILL.md")).unwrap(), "---\nname: dev\n---\n");
        assert_eq!(fs::read_to_string(dest.join("scripts").join("run.sh")).unwrap(), "echo ok\n");
        assert_eq!(requests.get(), 2);

        // Files the archive has cost no request; blobs already read are kept.
        assert_eq!(snapshot.read("README.md").unwrap(), b"readme");
        assert_eq!(snapshot.read(".agents/skills/dev/SKILL.md").unwrap(), b"---\nname: dev\n---\n");
        assert_eq!(requests.get(), 2);
        let _ = fs::remove_dir_all(dest);
    }

    #[test]
    fn a_failed_blob_read_names_export_ignore() {
        let (json, _, _) = fake_tree(&[(".claude/skills/dev/SKILL.md", "dev")]);
        let (tree, _) = parse_tree(&json).unwrap();
        let fetch: BlobFetcher<'static> =
            Box::new(|_: &str| Err("GitHub's hourly rate limit was reached. Try again later.".to_string()));
        let mut snapshot = TreeSnapshot::new(tree, false, None, fetch);
        let dest = test_temp_dir("tree-fail");
        let error = extract_subdir(&mut snapshot, ".claude/skills/dev", &dest).unwrap_err();
        assert!(error.contains("export-ignore") && error.contains("rate limit"), "{error}");
        let _ = fs::remove_dir_all(dest);
    }

    #[test]
    fn checks_paths_and_sizes_before_any_request() {
        let mut tree = vec![TreeFile { path: "skills/x/../../evil.txt".to_string(), sha: "a".repeat(40), size: 1 }];
        let fetch: BlobFetcher<'static> = Box::new(|_: &str| -> Result<Vec<u8>, String> { panic!("no request expected") });
        let mut snapshot = TreeSnapshot::new(tree.clone(), false, None, fetch);
        let dest = test_temp_dir("tree-unsafe");
        let error = extract_subdir(&mut snapshot, "skills/x", &dest).unwrap_err();
        assert!(error.contains("unsafe path"), "{error}");

        tree[0] = TreeFile { path: "skills/x/big.bin".to_string(), sha: "a".repeat(40), size: MAX_TOTAL_BYTES + 1 };
        let fetch: BlobFetcher<'static> = Box::new(|_: &str| -> Result<Vec<u8>, String> { panic!("no request expected") });
        let mut snapshot = TreeSnapshot::new(tree, false, None, fetch);
        let error = extract_subdir(&mut snapshot, "skills/x", &dest).unwrap_err();
        assert!(error.contains("too large"), "{error}");
        let _ = fs::remove_dir_all(dest);
    }

    #[test]
    fn reads_export_ignore_rules_for_folders() {
        let attributes = "# Distribution ignores\n/.agents/            export-ignore\n/.claude export-ignore\n/tests/ export-ignore\n*.md -export-ignore\nnode_modules export-ignore\n/docs/** export-ignore\n.cod* export-ignore\n";
        assert!(export_ignored(attributes, ".agents/skills/dev"));
        assert!(export_ignored(attributes, ".claude/skills/dev"));
        assert!(export_ignored(attributes, "pkg/node_modules/x"));
        assert!(export_ignored(attributes, "docs/skills/x"));
        assert!(export_ignored(attributes, ".codex/skills/x"));
        assert!(!export_ignored(attributes, "skills/dev"));
        assert!(!export_ignored(attributes, "src/tests/x"), "anchored at the root");
        assert!(!export_ignored(attributes, "README.md"));
        assert!(!export_ignored("", ".claude/skills/dev"));
    }
}
