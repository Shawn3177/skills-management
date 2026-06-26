use crate::fs_ops::{
    default_data_root, excluded_entries, is_excluded_entry, is_internal_marker, is_safe_relative,
    safe_folder_name, unix_ms,
};
use crate::library::import_skill_to_library_with_root;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

const FORMAT_VERSION: u32 = 1;
const SOURCE_APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportTargetInput {
    pub id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSkillInput {
    pub source_path: String,
    pub name: String,
    #[serde(default)]
    pub targets: Vec<ExportTargetInput>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub skill_count: usize,
    pub destination: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPackResult {
    pub imported: usize,
    pub skill_count: usize,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestSkill {
    name: String,
    path: String,
    #[serde(default)]
    target_states: BTreeMap<String, bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format_version: u32,
    package_id: String,
    created_at: u128,
    source_app_version: String,
    exclusions: Vec<String>,
    skills: Vec<ManifestSkill>,
    checksums: BTreeMap<String, String>,
}

pub fn export_skillpack(
    sources: Vec<ExportSkillInput>,
    destination: String,
) -> Result<ExportResult, String> {
    if sources.is_empty() {
        return Err("No skills selected for export.".to_string());
    }

    let file =
        File::create(&destination).map_err(|error| format!("Could not create package: {error}"))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut checksums: BTreeMap<String, String> = BTreeMap::new();
    let mut manifest_skills: Vec<ManifestSkill> = Vec::new();
    let mut used_names: BTreeMap<String, usize> = BTreeMap::new();

    for source in &sources {
        let source_path = Path::new(&source.source_path);
        if !source_path.is_dir() {
            return Err(format!(
                "Skill folder is not a directory: {}",
                source.source_path
            ));
        }
        if !source_path.join("SKILL.md").is_file() {
            return Err(format!(
                "Skill folder must contain SKILL.md: {}",
                source.source_path
            ));
        }

        let safe_name = dedupe_name(&mut used_names, safe_folder_name(&source.name, "imported-skill"));
        let prefix = format!("skills/{safe_name}");

        add_dir_to_zip(&mut zip, options, source_path, &prefix, &mut checksums)?;

        let mut target_states = BTreeMap::new();
        for target in &source.targets {
            target_states.insert(target.id.clone(), target.enabled);
        }

        manifest_skills.push(ManifestSkill {
            name: source.name.clone(),
            path: prefix,
            target_states,
        });
    }

    let manifest = Manifest {
        format_version: FORMAT_VERSION,
        package_id: format!("skillpack-{}", unix_ms()),
        created_at: unix_ms(),
        source_app_version: SOURCE_APP_VERSION.to_string(),
        exclusions: excluded_entries().iter().map(|value| value.to_string()).collect(),
        skills: manifest_skills,
        checksums,
    };

    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|error| format!("Could not serialize manifest: {error}"))?;
    zip.start_file("manifest.json", options)
        .map_err(|error| format!("Could not write manifest: {error}"))?;
    zip.write_all(manifest_json.as_bytes())
        .map_err(|error| format!("Could not write manifest: {error}"))?;

    zip.finish()
        .map_err(|error| format!("Could not finalize package: {error}"))?;

    Ok(ExportResult {
        skill_count: sources.len(),
        destination,
        message: format!("Exported {} skill(s).", sources.len()),
    })
}

fn dedupe_name(used: &mut BTreeMap<String, usize>, name: String) -> String {
    let count = used.entry(name.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        name
    } else {
        format!("{name}-{}", *count)
    }
}

fn add_dir_to_zip(
    zip: &mut ZipWriter<File>,
    options: SimpleFileOptions,
    source: &Path,
    prefix: &str,
    checksums: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    let entries =
        fs::read_dir(source).map_err(|error| format!("Could not read {}: {error}", source.display()))?;

    for entry in entries {
        let entry = entry.map_err(|error| format!("Could not read entry: {error}"))?;
        let name = entry.file_name();
        let name_text = name.to_string_lossy();

        if is_excluded_entry(&name_text) || is_internal_marker(&name_text) {
            continue;
        }

        let entry_path = entry.path();
        let zip_path = format!("{prefix}/{name_text}");

        if entry_path.is_dir() {
            add_dir_to_zip(zip, options, &entry_path, &zip_path, checksums)?;
        } else if entry_path.is_file() {
            let bytes = fs::read(&entry_path)
                .map_err(|error| format!("Could not read {}: {error}", entry_path.display()))?;
            zip.start_file(zip_path.as_str(), options)
                .map_err(|error| format!("Could not add {zip_path}: {error}"))?;
            zip.write_all(&bytes)
                .map_err(|error| format!("Could not write {zip_path}: {error}"))?;
            checksums.insert(zip_path, sha256_hex(&bytes));
        }
    }

    Ok(())
}

pub fn import_skillpack(package_path: String) -> Result<ImportPackResult, String> {
    let data_root = default_data_root()?;
    import_skillpack_with_root(Path::new(&package_path), &data_root)
}

pub fn import_skillpack_with_root(
    package_path: &Path,
    data_root: &Path,
) -> Result<ImportPackResult, String> {
    let file =
        File::open(package_path).map_err(|error| format!("Could not open package: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("Could not read package: {error}"))?;

    let manifest = read_manifest(&mut archive)?;
    if manifest.format_version > FORMAT_VERSION {
        return Err(format!(
            "Package format version {} is newer than supported ({FORMAT_VERSION}).",
            manifest.format_version
        ));
    }

    let staging = unique_staging(data_root)?;
    if let Err(error) = extract_and_verify(&mut archive, &manifest, &staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    let mut imported = 0usize;
    let mut import_error: Option<String> = None;

    for skill in &manifest.skills {
        let skill_dir = staging.join(&skill.path);
        match import_skill_to_library_with_root(&skill_dir, data_root) {
            Ok(result) => {
                if result.imported || result.already_managed {
                    imported += 1;
                }
            }
            Err(error) => import_error = Some(error),
        }
    }

    let _ = fs::remove_dir_all(&staging);

    if imported == 0 {
        return Err(import_error.unwrap_or_else(|| "Package contained no importable skills.".to_string()));
    }

    Ok(ImportPackResult {
        imported,
        skill_count: manifest.skills.len(),
        message: format!("Imported {imported} skill(s) into the shared library."),
    })
}

fn read_manifest(archive: &mut ZipArchive<File>) -> Result<Manifest, String> {
    let mut manifest_file = archive
        .by_name("manifest.json")
        .map_err(|_| "Package is missing manifest.json.".to_string())?;
    let mut contents = String::new();
    manifest_file
        .read_to_string(&mut contents)
        .map_err(|error| format!("Could not read manifest: {error}"))?;
    serde_json::from_str(&contents).map_err(|error| format!("Invalid manifest: {error}"))
}

fn extract_and_verify(
    archive: &mut ZipArchive<File>,
    manifest: &Manifest,
    staging: &Path,
) -> Result<(), String> {
    let mut computed: BTreeMap<String, String> = BTreeMap::new();

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Could not read package entry: {error}"))?;
        let name = entry.name().to_string();

        if name == "manifest.json" || entry.is_dir() {
            continue;
        }
        if !is_safe_relative(&name) {
            return Err(format!("Package contains an unsafe path: {name}"));
        }

        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Could not read {name}: {error}"))?;
        computed.insert(name.clone(), sha256_hex(&bytes));

        let destination = staging.join(&name);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
        }
        fs::write(&destination, &bytes)
            .map_err(|error| format!("Could not extract {name}: {error}"))?;
    }

    for (path, expected) in &manifest.checksums {
        match computed.get(path) {
            Some(actual) if actual == expected => {}
            Some(_) => return Err(format!("Checksum mismatch for {path}.")),
            None => return Err(format!("Package is missing file {path}.")),
        }
    }

    Ok(())
}

fn unique_staging(data_root: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(data_root).map_err(|error| format!("Could not create data root: {error}"))?;
    for index in 0.. {
        let suffix = if index == 0 {
            String::new()
        } else {
            format!("-{index}")
        };
        let candidate = data_root.join(format!(".skillpack-import-{}{suffix}", unix_ms()));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    unreachable!("unbounded staging index should always find an available folder")
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("skills-manage-pack-{prefix}-{stamp}"))
    }

    fn make_skill(dir: &Path, name: &str) {
        fs::create_dir_all(dir.join("scripts")).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: demo\n---\n# {name}\n"),
        )
        .unwrap();
        fs::write(dir.join("scripts").join("run.ps1"), "Write-Output ok").unwrap();
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::write(dir.join(".git").join("config"), "git").unwrap();
        fs::write(dir.join(".env"), "SECRET=1").unwrap();
        fs::write(dir.join(".skills-manage-source.json"), "{}").unwrap();
    }

    #[test]
    fn round_trip_export_then_import() {
        let work = unique_temp_dir("roundtrip");
        let skill_dir = work.join("alpha-skill");
        make_skill(&skill_dir, "alpha-skill");
        let pkg = work.join("alpha.skillpack");

        let export = export_skillpack(
            vec![ExportSkillInput {
                source_path: skill_dir.display().to_string(),
                name: "alpha-skill".to_string(),
                targets: vec![ExportTargetInput {
                    id: "codex".to_string(),
                    enabled: true,
                }],
            }],
            pkg.display().to_string(),
        )
        .unwrap();
        assert_eq!(export.skill_count, 1);
        assert!(pkg.is_file());

        let data_root = work.join("data");
        let import = import_skillpack_with_root(&pkg, &data_root).unwrap();
        assert_eq!(import.imported, 1);

        let imported_dir = data_root.join("library").join("alpha-skill");
        assert!(imported_dir.join("SKILL.md").is_file());
        assert!(imported_dir.join("scripts").join("run.ps1").is_file());
        assert!(!imported_dir.join(".git").exists());
        assert!(!imported_dir.join(".env").exists());
        assert!(!imported_dir.join(".skills-manage-source.json").exists());

        let leftover_staging = fs::read_dir(&data_root)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".skillpack-import-")
            });
        assert!(!leftover_staging, "staging directory should be cleaned up");

        fs::remove_dir_all(work).unwrap();
    }

    #[test]
    fn import_rejects_checksum_mismatch() {
        let work = unique_temp_dir("badsum");
        fs::create_dir_all(&work).unwrap();
        let pkg = work.join("bad.skillpack");

        {
            let file = File::create(&pkg).unwrap();
            let mut zip = ZipWriter::new(file);
            let options = SimpleFileOptions::default();
            zip.start_file("skills/x/SKILL.md", options).unwrap();
            zip.write_all(b"real content").unwrap();
            let manifest = serde_json::json!({
                "formatVersion": 1,
                "packageId": "p",
                "createdAt": 0,
                "sourceAppVersion": "0.1.0",
                "exclusions": [],
                "skills": [{ "name": "x", "path": "skills/x", "targetStates": {} }],
                "checksums": {
                    "skills/x/SKILL.md":
                        "0000000000000000000000000000000000000000000000000000000000000000"
                }
            });
            zip.start_file("manifest.json", options).unwrap();
            zip.write_all(manifest.to_string().as_bytes()).unwrap();
            zip.finish().unwrap();
        }

        let data_root = work.join("data");
        let error = import_skillpack_with_root(&pkg, &data_root).unwrap_err();
        assert!(error.contains("Checksum mismatch"), "got: {error}");
        assert!(!data_root.join("library").join("x").exists());

        fs::remove_dir_all(work).unwrap();
    }

    #[test]
    fn exports_multiple_skills_into_one_package() {
        let work = unique_temp_dir("multi");
        let first = work.join("first-skill");
        let second = work.join("second-skill");
        make_skill(&first, "first-skill");
        make_skill(&second, "second-skill");
        let pkg = work.join("two.skillpack");

        export_skillpack(
            vec![
                ExportSkillInput {
                    source_path: first.display().to_string(),
                    name: "first-skill".to_string(),
                    targets: vec![],
                },
                ExportSkillInput {
                    source_path: second.display().to_string(),
                    name: "second-skill".to_string(),
                    targets: vec![],
                },
            ],
            pkg.display().to_string(),
        )
        .unwrap();

        let data_root = work.join("data");
        let import = import_skillpack_with_root(&pkg, &data_root).unwrap();
        assert_eq!(import.imported, 2);
        assert!(data_root
            .join("library")
            .join("first-skill")
            .join("SKILL.md")
            .is_file());
        assert!(data_root
            .join("library")
            .join("second-skill")
            .join("SKILL.md")
            .is_file());

        fs::remove_dir_all(work).unwrap();
    }
}
