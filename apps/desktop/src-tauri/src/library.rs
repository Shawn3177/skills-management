use crate::fs_ops::{copy_skill_dir, default_data_root, safe_folder_name};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: bool,
    pub already_managed: bool,
    pub skill_name: String,
    pub library_path: String,
    pub message: String,
}

pub fn import_skill_to_library(source_path: String) -> Result<ImportResult, String> {
    let data_root = default_data_root()?;
    import_skill_to_library_with_root(Path::new(&source_path), &data_root)
}

pub fn import_skill_to_library_with_root(
    source_path: &Path,
    data_root: &Path,
) -> Result<ImportResult, String> {
    if !source_path.is_dir() {
        return Err(format!(
            "Source path is not a directory: {}",
            source_path.display()
        ));
    }

    if !source_path.join("SKILL.md").is_file() {
        return Err(format!(
            "Source folder must contain SKILL.md: {}",
            source_path.display()
        ));
    }

    let library_root = data_root.join("library");
    fs::create_dir_all(&library_root)
        .map_err(|error| format!("Could not create library root: {error}"))?;

    let source_canonical = source_path
        .canonicalize()
        .map_err(|error| format!("Could not read source path: {error}"))?;
    let library_canonical = library_root
        .canonicalize()
        .map_err(|error| format!("Could not read library root: {error}"))?;

    let skill_name = source_path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("imported-skill")
        .to_string();

    if source_canonical.starts_with(&library_canonical) {
        return Ok(ImportResult {
            imported: false,
            already_managed: true,
            skill_name,
            library_path: source_canonical.display().to_string(),
            message: "Skill is already in the shared library.".to_string(),
        });
    }

    let destination = unique_destination(&library_root, &skill_name);
    copy_skill_dir(source_path, &destination)?;

    Ok(ImportResult {
        imported: true,
        already_managed: false,
        skill_name,
        library_path: destination.display().to_string(),
        message: "Skill imported into the shared library.".to_string(),
    })
}

pub(crate) fn unique_destination(library_root: &Path, skill_name: &str) -> PathBuf {
    let safe_name = safe_folder_name(skill_name, "imported-skill");
    let first = library_root.join(&safe_name);
    if !first.exists() {
        return first;
    }

    for copy_index in 2.. {
        let candidate = library_root.join(format!("{safe_name}-copy-{copy_index}"));
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!("unbounded copy index should always find an available folder")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn imports_skill_directory_into_shared_library() {
        let source_root = unique_temp_dir("source-skill-root");
        let source = source_root.join("source-skill");
        let data_root = unique_temp_dir("data-root");
        fs::create_dir_all(source.join("scripts")).unwrap();
        fs::write(
            source.join("SKILL.md"),
            "---\nname: import-me\ndescription: Importable skill\n---\n# Import me\n",
        )
        .unwrap();
        fs::write(source.join("scripts").join("run.ps1"), "Write-Output ok").unwrap();

        let result = import_skill_to_library_with_root(&source, &data_root).unwrap();

        assert!(result.imported);
        assert!(!result.already_managed);
        assert_eq!(result.skill_name, "source-skill");
        assert!(data_root
            .join("library")
            .join("source-skill")
            .join("SKILL.md")
            .is_file());
        assert!(data_root
            .join("library")
            .join("source-skill")
            .join("scripts")
            .join("run.ps1")
            .is_file());

        fs::remove_dir_all(source_root).unwrap();
        fs::remove_dir_all(data_root).unwrap();
    }

    #[test]
    fn excludes_sensitive_and_generated_entries() {
        let source_root = unique_temp_dir("source-exclusions-root");
        let source = source_root.join("source-exclusions");
        let data_root = unique_temp_dir("data-root-exclusions");
        fs::create_dir_all(source.join(".git")).unwrap();
        fs::create_dir_all(source.join("node_modules")).unwrap();
        fs::create_dir_all(source.join("dist")).unwrap();
        fs::create_dir_all(source.join("target")).unwrap();
        fs::write(source.join("SKILL.md"), "# Skill\n").unwrap();
        fs::write(source.join(".env"), "SECRET=value").unwrap();
        fs::write(source.join(".git").join("config"), "git").unwrap();
        fs::write(source.join("node_modules").join("pkg.js"), "pkg").unwrap();
        fs::write(source.join("dist").join("bundle.js"), "bundle").unwrap();
        fs::write(source.join("target").join("debug.bin"), "debug").unwrap();

        import_skill_to_library_with_root(&source, &data_root).unwrap();
        let imported = data_root
            .join("library")
            .join(source.file_name().unwrap().to_string_lossy().to_string());

        assert!(imported.join("SKILL.md").is_file());
        assert!(!imported.join(".env").exists());
        assert!(!imported.join(".git").exists());
        assert!(!imported.join("node_modules").exists());
        assert!(!imported.join("dist").exists());
        assert!(!imported.join("target").exists());

        fs::remove_dir_all(source_root).unwrap();
        fs::remove_dir_all(data_root).unwrap();
    }

    #[test]
    fn excludes_internal_marker_files() {
        let source_root = unique_temp_dir("source-markers-root");
        let source = source_root.join("source-markers");
        let data_root = unique_temp_dir("data-root-markers");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), "# Skill\n").unwrap();
        fs::write(source.join(".skills-manage-source.json"), "{}").unwrap();
        fs::write(source.join(".skills-manage-link.json"), "{}").unwrap();

        import_skill_to_library_with_root(&source, &data_root).unwrap();
        let imported = data_root.join("library").join("source-markers");

        assert!(imported.join("SKILL.md").is_file());
        assert!(!imported.join(".skills-manage-source.json").exists());
        assert!(!imported.join(".skills-manage-link.json").exists());

        fs::remove_dir_all(source_root).unwrap();
        fs::remove_dir_all(data_root).unwrap();
    }

    #[test]
    fn imports_conflicting_folder_as_copy_without_overwriting() {
        let source_root = unique_temp_dir("source-conflict-root");
        let source = source_root.join("source-conflict");
        let data_root = unique_temp_dir("data-root-conflict");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), "# Skill\n").unwrap();

        let first = import_skill_to_library_with_root(&source, &data_root).unwrap();
        fs::write(source.join("extra.md"), "second import").unwrap();
        let second = import_skill_to_library_with_root(&source, &data_root).unwrap();

        assert!(first.library_path.ends_with("source-conflict"));
        assert!(second.library_path.ends_with("source-conflict-copy-2"));
        assert!(data_root
            .join("library")
            .join("source-conflict")
            .join("SKILL.md")
            .is_file());
        assert!(data_root
            .join("library")
            .join("source-conflict-copy-2")
            .join("extra.md")
            .is_file());

        fs::remove_dir_all(source_root).unwrap();
        fs::remove_dir_all(data_root).unwrap();
    }

    #[test]
    fn rejects_folder_without_skill_markdown() {
        let source_root = unique_temp_dir("source-invalid-root");
        let source = source_root.join("source-invalid");
        let data_root = unique_temp_dir("data-root-invalid");
        fs::create_dir_all(&source).unwrap();

        let result = import_skill_to_library_with_root(&source, &data_root);

        assert!(result.unwrap_err().contains("SKILL.md"));

        fs::remove_dir_all(source_root).unwrap();
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("skills-manage-{prefix}-{stamp}"))
    }
}
