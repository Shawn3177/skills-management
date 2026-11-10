//! Explicit, project-scoped skill snapshots. Updating the library does not
//! change a running project's skills until the user synchronizes that copy.
use crate::fs_ops::{self, default_data_root, display_path, unique_timestamped_path};
use crate::{library, scanner, targets, variants};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const PROJECT_MARKER: &str = ".skills-manage-project.json";
const PROJECT_TOOLS: [(&str, &str); 3] = [
    ("codex", ".agents/skills"),
    ("claude-code", ".claude/skills"),
    ("vs-code", ".github/skills"),
];

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub name: String,
    pub path: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Deployment {
    source_path: String,
    target_id: String,
    fingerprint: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTarget {
    pub id: String,
    pub path: String,
    pub state: String,
    pub fingerprint: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSkill {
    pub skill: scanner::ScannedSkill,
    pub targets: Vec<ProjectTarget>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSnapshot {
    pub library: Vec<ProjectSkill>,
    pub local: Vec<scanner::ScannedSkill>,
}

pub fn list_projects() -> Result<Vec<Project>, String> {
    load(&default_data_root()?)
}

fn load(data_root: &Path) -> Result<Vec<Project>, String> {
    match fs::read(data_root.join("projects.json")) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|e| format!("Could not read projects: {e}"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("Could not read projects: {e}")),
    }
}

fn save(data_root: &Path, projects: &[Project]) -> Result<(), String> {
    fs::create_dir_all(data_root).map_err(|e| e.to_string())?;
    let temp = unique_timestamped_path(data_root, ".projects");
    let bytes = serde_json::to_vec_pretty(projects).map_err(|e| e.to_string())?;
    fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    let result = fs::rename(&temp, data_root.join("projects.json")).map_err(|e| e.to_string());
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

fn validate_project(path: &Path, data_root: &Path) -> Result<PathBuf, String> {
    let root = path
        .canonicalize()
        .map_err(|e| format!("Project folder is unavailable: {e}"))?;
    if !root.is_dir() || root.parent().is_none() {
        return Err("Choose a project directory.".into());
    }
    if fs_ops::home_dir()
        .and_then(|p| p.canonicalize().ok())
        .as_ref()
        == Some(&root)
    {
        return Err("Choose a project folder, not your home directory.".into());
    }
    if let Ok(data) = data_root.canonicalize() {
        if root.starts_with(&data) || data.starts_with(&root) {
            return Err(
                "The project must be separate from the Skills Manage data directory.".into(),
            );
        }
    }
    Ok(root)
}

pub fn add_project(path: String) -> Result<Project, String> {
    add_with_root(Path::new(&path), &default_data_root()?)
}

fn add_with_root(path: &Path, data_root: &Path) -> Result<Project, String> {
    fs::create_dir_all(data_root).map_err(|e| e.to_string())?;
    let root = validate_project(path, data_root)?;
    let path = display_path(&root);
    let mut projects = load(data_root)?;
    if let Some(project) = projects.iter().find(|project| project.path == path) {
        return Ok(project.clone());
    }
    let project = Project {
        name: root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        path,
    };
    projects.push(project.clone());
    save(data_root, &projects)?;
    Ok(project)
}

pub fn forget_project(path: String) -> Result<Vec<Project>, String> {
    let data_root = default_data_root()?;
    let mut projects = load(&data_root)?;
    projects.retain(|project| project.path != path);
    save(&data_root, &projects)?;
    Ok(projects)
}

fn registered(path: &str, data_root: &Path) -> Result<PathBuf, String> {
    if !load(data_root)?.iter().any(|project| project.path == path) {
        return Err("Add this project before managing its skills.".into());
    }
    let root = validate_project(Path::new(path), data_root)?;
    if display_path(&root) != path {
        return Err(
            "The project folder now points to another location. Add that location explicitly."
                .into(),
        );
    }
    Ok(root)
}

fn is_redirect(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

/// Check existing components, including dangling links, before any read/write.
fn plain_path(base: &Path, relative: &Path) -> Result<PathBuf, String> {
    let mut current = base.to_path_buf();
    for part in relative.components() {
        if !matches!(part, std::path::Component::Normal(_)) {
            return Err("Invalid project skill path.".into());
        }
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) if is_redirect(&meta) => {
                return Err(format!(
                    "Linked project paths are read-only: {}",
                    display_path(&current)
                ))
            }
            Ok(meta) if !meta.is_dir() => {
                return Err(format!("Expected a folder: {}", display_path(&current)))
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(current)
}

fn target_root(project: &Path, target: &str) -> Result<PathBuf, String> {
    let (_, folder) = PROJECT_TOOLS
        .iter()
        .find(|(id, _)| *id == target)
        .ok_or_else(|| {
            "Project deployment is supported for Codex, Claude Code and VS Code.".to_string()
        })?;
    plain_path(project, Path::new(folder))
}

fn fingerprint(dir: &Path, source: bool) -> Result<String, String> {
    fn visit(dir: &Path, relative: &Path, source: bool, digest: &mut Sha256) -> Result<(), String> {
        let mut entries = fs::read_dir(dir)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry.file_name();
            let text = name.to_string_lossy();
            if fs_ops::is_internal_marker(&text) || (source && fs_ops::is_excluded_entry(&text)) {
                continue;
            }
            if fs_ops::is_link(&entry) {
                return Err("A skill contains a symbolic link or junction.".into());
            }
            let path = relative.join(&name);
            if entry.path().is_dir() {
                visit(&entry.path(), &path, source, digest)?;
            } else {
                let name_bytes = path.to_string_lossy().replace('\\', "/").into_bytes();
                let bytes = fs::read(entry.path()).map_err(|e| e.to_string())?;
                digest.update((name_bytes.len() as u64).to_le_bytes());
                digest.update(name_bytes);
                digest.update((bytes.len() as u64).to_le_bytes());
                digest.update(bytes);
            }
        }
        Ok(())
    }
    let mut digest = Sha256::new();
    visit(dir, Path::new(""), source, &mut digest)?;
    Ok(format!("{:x}", digest.finalize()))
}

fn deployment(dir: &Path, library: &Path, target: &str) -> Option<Deployment> {
    let marker: Deployment =
        serde_json::from_slice(&fs::read(dir.join(PROJECT_MARKER)).ok()?).ok()?;
    if marker.source_path != display_path(&library.canonicalize().ok()?)
        || marker.target_id != target
    {
        return None;
    }
    Some(marker)
}

pub fn scan_project(path: String) -> Result<ProjectSnapshot, String> {
    scan_with_root(&path, &default_data_root()?)
}

fn scan_with_root(path: &str, data_root: &Path) -> Result<ProjectSnapshot, String> {
    let project = registered(path, data_root)?;
    let roots = PROJECT_TOOLS
        .iter()
        .map(|(id, _)| target_root(&project, id).map(|root| (*id, root)))
        .collect::<Result<Vec<_>, _>>()?;
    let library = scanner::scan_skill_roots_with_target_roots(&[data_root.join("library")], &[])
        .into_iter()
        .map(|skill| {
            let library = Path::new(&skill.source_path);
            let folder = library.file_name().ok_or("Invalid skill folder")?;
            let targets = roots
                .iter()
                .map(|(id, root)| {
                    let destination = plain_path(root, Path::new(folder))?;
                    let current = destination
                        .exists()
                        .then(|| fingerprint(&destination, false))
                        .transpose()
                        .ok()
                        .flatten();
                    let state = if !destination.exists() {
                        "available"
                    } else if let Some(marker) = deployment(&destination, library, id) {
                        if current.as_ref() != Some(&marker.fingerprint) {
                            "modified"
                        } else if fingerprint(&variants::effective_source(library, id), true).ok()
                            != Some(marker.fingerprint)
                        {
                            "outdated"
                        } else {
                            "deployed"
                        }
                    } else {
                        "conflict"
                    };
                    Ok(ProjectTarget {
                        id: id.to_string(),
                        path: display_path(&destination),
                        state: state.into(),
                        fingerprint: current,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(ProjectSkill { skill, targets })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let paths = roots
        .iter()
        .map(|(_, root)| root.clone())
        .collect::<Vec<_>>();
    let local = scanner::scan_skill_roots_with_target_roots(&paths, &roots)
        .into_iter()
        .map(|mut skill| {
            skill.source_path = display_path(Path::new(&skill.source_path));
            skill
        })
        .collect();
    Ok(ProjectSnapshot { library, local })
}

pub fn deploy_project_skill(
    project_path: String,
    library_path: String,
    target_id: String,
    enabled: bool,
    expected_fingerprint: Option<String>,
) -> Result<(), String> {
    deploy_with_root(
        &project_path,
        Path::new(&library_path),
        &target_id,
        enabled,
        expected_fingerprint.as_deref(),
        &default_data_root()?,
    )
}

fn deploy_with_root(
    project_path: &str,
    library: &Path,
    target: &str,
    enabled: bool,
    expected_fingerprint: Option<&str>,
    data_root: &Path,
) -> Result<(), String> {
    let project = registered(project_path, data_root)?;
    let library = targets::validate_library_skill(library, data_root)?;
    let root = target_root(&project, target)?;
    let folder = library.file_name().ok_or("Invalid skill folder")?;
    let destination = plain_path(&root, Path::new(folder))?;
    let before = if destination.exists() {
        let marker = deployment(&destination, &library, target)
            .ok_or("This project folder is not managed. Import it into the library first.")?;
        let actual = fingerprint(&destination, false)?;
        if expected_fingerprint.is_some_and(|expected| expected != actual) {
            return Err(
                "The project copy changed after confirmation. Reload and review it again.".into(),
            );
        }
        if actual != marker.fingerprint && expected_fingerprint != Some(actual.as_str()) {
            return Err("The project copy has local changes. Import it into the library to preserve those changes first.".into());
        }
        Some(actual)
    } else {
        None
    };
    if !enabled && before.is_none() {
        return Ok(());
    }
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let staging = unique_timestamped_path(&root, ".skills-manage-project-stage");
    let staged_skill = staging.join(folder);
    if enabled {
        let preparation = (|| {
            fingerprint(&variants::effective_source(&library, target), true)?;
            targets::set_skill_target_enabled_with_root(
                &library, target, true, data_root, &staging,
            )?;
            let marker = Deployment {
                source_path: display_path(&library),
                target_id: target.into(),
                fingerprint: fingerprint(&staged_skill, false)?,
            };
            fs::write(
                staged_skill.join(PROJECT_MARKER),
                serde_json::to_vec_pretty(&marker).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())
        })();
        if let Err(error) = preparation {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    }
    let outcome = publish(
        &root,
        &destination,
        enabled.then_some(staged_skill.as_path()),
        before.as_deref(),
        data_root,
    );
    let _ = fs::remove_dir_all(&staging);
    outcome
}

fn publish(
    root: &Path,
    destination: &Path,
    staged: Option<&Path>,
    before: Option<&str>,
    data_root: &Path,
) -> Result<(), String> {
    let holding = unique_timestamped_path(root, ".skills-manage-project-old");
    if let Some(before) = before {
        let backup = crate::trash::unique_trash_path(data_root, "project-skill")?;
        fs_ops::copy_dir_excluding(destination, &backup, |_| false)?;
        if fingerprint(destination, false)? != before || fingerprint(&backup, false)? != before {
            return Err(
                "The project copy changed while preparing the operation. Please reload.".into(),
            );
        }
        fs::rename(destination, &holding).map_err(|e| e.to_string())?;
    } else if fs::symlink_metadata(destination).is_ok() {
        return Err("A project skill appeared during deployment. Please reload.".into());
    }
    if let Some(staged) = staged {
        if let Err(error) = fs::rename(staged, destination) {
            if before.is_some() {
                fs::rename(&holding, destination).map_err(|rollback| {
                    format!(
                        "Deployment failed: {error}. Original retained at {}: {rollback}",
                        display_path(&holding)
                    )
                })?;
            }
            return Err(format!("Could not deploy project skill: {error}"));
        }
    }
    if before.is_some() {
        fs::remove_dir_all(&holding).map_err(|e| {
            format!(
                "Operation completed; backup cleanup needs attention at {}: {e}",
                display_path(&holding)
            )
        })?;
    }
    Ok(())
}

pub fn import_project_skill(
    project_path: String,
    source_path: String,
) -> Result<library::ImportResult, String> {
    let data_root = default_data_root()?;
    let project = registered(&project_path, &data_root)?;
    let source = Path::new(&source_path)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    for (id, _) in PROJECT_TOOLS {
        let root = target_root(&project, id)?;
        if source.parent() == Some(root.as_path()) {
            plain_path(
                &root,
                Path::new(source.file_name().ok_or("Invalid skill path")?),
            )?;
            fingerprint(&source, true)?;
            return library::import_skill_to_library(source_path);
        }
    }
    Err("Choose a skill directly inside this project's supported skills folders.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        root: PathBuf,
        data: PathBuf,
        skill: PathBuf,
        a: String,
        b: String,
    }

    impl Fixture {
        fn new() -> Self {
            let root = fs_ops::test_temp_dir("project-deploy");
            let data = root.join("data");
            let skill = data.join("library/review");
            fs::create_dir_all(&skill).unwrap();
            fs::write(
                skill.join("SKILL.md"),
                "---\nname: review\ndescription: Review code\n---\nOriginal",
            )
            .unwrap();
            let a = root.join("项目 A");
            let b = root.join("Project B");
            fs::create_dir_all(&a).unwrap();
            fs::create_dir_all(&b).unwrap();
            let a = add_with_root(&a, &data).unwrap().path;
            let b = add_with_root(&b, &data).unwrap().path;
            Self {
                root,
                data,
                skill,
                a,
                b,
            }
        }
        fn deploy(&self, project: &str, target: &str, enabled: bool) -> Result<(), String> {
            deploy_with_root(project, &self.skill, target, enabled, None, &self.data)
        }
        fn state(&self, project: &str, target: &str) -> String {
            scan_with_root(project, &self.data).unwrap().library[0]
                .targets
                .iter()
                .find(|item| item.id == target)
                .unwrap()
                .state
                .clone()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn persists_projects_and_isolates_scopes_and_tools() {
        let f = Fixture::new();
        assert_eq!(load(&f.data).unwrap().len(), 2);
        assert_eq!(add_with_root(Path::new(&f.a), &f.data).unwrap().path, f.a);
        assert_eq!(load(&f.data).unwrap().len(), 2);
        for (tool, relative) in PROJECT_TOOLS {
            f.deploy(&f.a, tool, true).unwrap();
            assert!(Path::new(&f.a)
                .join(relative)
                .join("review/SKILL.md")
                .is_file());
            assert_eq!(f.state(&f.a, tool), "deployed");
            assert_eq!(f.state(&f.b, tool), "available");
        }
        assert!(!Path::new(&f.a).join(".codex").exists());
        assert!(!Path::new(&f.a).join(".copilot").exists());
        f.deploy(&f.a, "codex", false).unwrap();
        assert_eq!(f.state(&f.a, "codex"), "available");
        assert_eq!(f.state(&f.a, "claude-code"), "deployed");
        assert!(f.skill.join("SKILL.md").is_file());
        assert!(fs::read_dir(f.data.join("trash")).unwrap().next().is_some());
    }

    #[test]
    fn snapshots_sync_explicitly_and_modified_content_requires_current_confirmation() {
        let f = Fixture::new();
        f.deploy(&f.a, "codex", true).unwrap();
        let copy = Path::new(&f.a).join(".agents/skills/review");
        let old = fs::read(copy.join("SKILL.md")).unwrap();
        fs::write(f.skill.join("SKILL.md"), "---\nname: review\n---\nNew").unwrap();
        assert_eq!(f.state(&f.a, "codex"), "outdated");
        assert_eq!(fs::read(copy.join("SKILL.md")).unwrap(), old);
        f.deploy(&f.a, "codex", true).unwrap();
        assert_eq!(f.state(&f.a, "codex"), "deployed");
        fs::write(copy.join("SKILL.md"), "My project edits").unwrap();
        fs::write(copy.join(".env"), "local-only test content").unwrap();
        assert_eq!(f.state(&f.a, "codex"), "modified");
        assert!(f.deploy(&f.a, "codex", false).is_err());
        let approved = fingerprint(&copy, false).unwrap();
        fs::write(copy.join("SKILL.md"), "Later edits").unwrap();
        assert!(deploy_with_root(&f.a, &f.skill, "codex", true, Some(&approved), &f.data).is_err());
        let approved = fingerprint(&copy, false).unwrap();
        deploy_with_root(&f.a, &f.skill, "codex", false, Some(&approved), &f.data).unwrap();
        assert!(!copy.exists());
        assert!(fs::read_dir(f.data.join("trash"))
            .unwrap()
            .flatten()
            .any(|e| e.path().join(".env").exists()));
    }

    #[test]
    fn refuses_unowned_folders_and_unsupported_targets() {
        let f = Fixture::new();
        let copy = Path::new(&f.a).join(".claude/skills/review");
        fs::create_dir_all(&copy).unwrap();
        fs::write(copy.join("SKILL.md"), "User content").unwrap();
        assert_eq!(f.state(&f.a, "claude-code"), "conflict");
        assert!(f.deploy(&f.a, "claude-code", true).is_err());
        assert!(f.deploy(&f.a, "claude-code", false).is_err());
        assert_eq!(
            fs::read_to_string(copy.join("SKILL.md")).unwrap(),
            "User content"
        );
        assert!(f.deploy(&f.a, "hermes", true).is_err());
        assert!(add_with_root(&f.data, &f.data).is_err());
        assert!(registered(&display_path(&f.root), &f.data).is_err());
    }

    #[test]
    fn failed_publish_restores_original_after_backup() {
        let f = Fixture::new();
        f.deploy(&f.a, "codex", true).unwrap();
        let root = Path::new(&f.a).join(".agents/skills");
        let copy = root.join("review");
        let before = fingerprint(&copy, false).unwrap();
        assert!(publish(
            &root,
            &copy,
            Some(&root.join("missing-stage")),
            Some(&before),
            &f.data
        )
        .is_err());
        assert_eq!(fingerprint(&copy, false).unwrap(), before);
        assert_eq!(f.state(&f.a, "codex"), "deployed");
    }

    #[test]
    fn corrupt_registry_is_reported_and_not_overwritten() {
        let f = Fixture::new();
        fs::write(f.data.join("projects.json"), "broken").unwrap();
        assert!(add_with_root(Path::new(&f.a), &f.data).is_err());
        assert_eq!(
            fs::read_to_string(f.data.join("projects.json")).unwrap(),
            "broken"
        );
    }

    #[cfg(windows)]
    #[test]
    fn junction_targets_never_escape_project() {
        use std::os::windows::fs::symlink_dir;
        let f = Fixture::new();
        let link = Path::new(&f.a).join(".agents");
        // Directory junctions work without Developer Mode or elevated rights.
        let junction = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(&f.b)
            .output()
            .unwrap();
        assert!(junction.status.success());
        assert!(f.deploy(&f.a, "codex", true).is_err());
        assert!(!Path::new(&f.b).join("skills").exists());
        fs::remove_dir(&link).unwrap();
        // Also cover a dangling symlink when this Windows host permits it.
        if symlink_dir(f.root.join("missing"), &link).is_ok() {
            assert!(f.deploy(&f.a, "codex", true).is_err());
            fs::remove_dir(&link).unwrap();
        }
    }
}
