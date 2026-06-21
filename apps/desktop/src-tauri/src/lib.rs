use serde::Serialize;

mod fs_ops;
mod library;
mod pack;
mod scanner;
mod targets;
use library::{import_skill_to_library as import_skill_to_library_impl, ImportResult};
use pack::{
    export_skillpack as export_skillpack_impl, import_skillpack as import_skillpack_impl,
    ExportResult, ExportSkillInput, ImportPackResult,
};
use scanner::{scan_default_skills, ScannedSkill};
use targets::{
    set_skill_target_enabled as set_skill_target_enabled_impl,
    set_skill_targets_bulk as set_skill_targets_bulk_impl, BulkToggleResult, TargetToggleResult,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppStatus {
    app_name: String,
    data_root: String,
    safe_write_mode: String,
}

pub fn default_app_status() -> AppStatus {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| "~".to_string());

    AppStatus {
        app_name: "Skills Manage".to_string(),
        data_root: format!("{home}\\.skills-manage"),
        safe_write_mode: "preview".to_string(),
    }
}

#[tauri::command]
fn get_app_status() -> AppStatus {
    default_app_status()
}

#[tauri::command]
fn scan_skills() -> Vec<ScannedSkill> {
    scan_default_skills()
}

#[tauri::command]
fn import_skill_to_library(source_path: String) -> Result<ImportResult, String> {
    import_skill_to_library_impl(source_path)
}

#[tauri::command]
fn set_skill_target_enabled(
    source_path: String,
    target_id: String,
    enabled: bool,
) -> Result<TargetToggleResult, String> {
    set_skill_target_enabled_impl(source_path, target_id, enabled)
}

#[tauri::command]
fn set_skill_targets_bulk(
    source_paths: Vec<String>,
    target_id: String,
    enabled: bool,
) -> Result<BulkToggleResult, String> {
    set_skill_targets_bulk_impl(source_paths, target_id, enabled)
}

#[tauri::command]
fn export_skillpack(
    sources: Vec<ExportSkillInput>,
    destination: String,
) -> Result<ExportResult, String> {
    export_skillpack_impl(sources, destination)
}

#[tauri::command]
fn import_skillpack(package_path: String) -> Result<ImportPackResult, String> {
    import_skillpack_impl(package_path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_app_status,
            scan_skills,
            import_skill_to_library,
            set_skill_target_enabled,
            set_skill_targets_bulk,
            export_skillpack,
            import_skillpack
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_status_describes_preview_safe_mode() {
        let status = default_app_status();

        assert_eq!(status.app_name, "Skills Manage");
        assert!(status.data_root.ends_with(".skills-manage"));
        assert_eq!(status.safe_write_mode, "preview");
    }
}
