use serde::Serialize;
use std::sync::Mutex;

mod fs_ops;
mod github;
mod library;
mod metadata;
mod pack;
mod scanner;
mod series;
mod targets;
mod trash;
use github::{
    check_skill_updates as check_skill_updates_impl, import_from_github as import_from_github_impl,
    link_github_series as link_github_series_impl,
    update_skill_from_github as update_skill_from_github_impl, GithubImportResult, LinkSeriesResult,
    SkillUpdateStatus, UpdateResult,
};
use library::{import_skill_to_library as import_skill_to_library_impl, ImportResult};
use pack::{
    export_skillpack as export_skillpack_impl, import_skillpack as import_skillpack_impl,
    ExportResult, ExportSkillInput, ImportPackResult,
};
use scanner::{scan_default_skills, ScannedSkill};
use std::path::Path;
use targets::{
    remove_skill_from_library as remove_skill_from_library_impl,
    set_skill_target_enabled as set_skill_target_enabled_impl,
    set_skill_targets_bulk as set_skill_targets_bulk_impl, BulkToggleResult, RemoveResult,
    TargetToggleResult,
};
use trash::{PurgeResult, TrashSummary};

/// Commands that write to the library, tool folders, or trash run one at a
/// time. They now run off the UI thread, so without this two of them (say a
/// bulk enable and a GitHub update) could interleave on the same folders.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    app_name: String,
    version: String,
    data_root: String,
    safe_write_mode: String,
    github_token_configured: bool,
    /// Lets the UI show paths as `~/…`.
    home_dir: String,
}

pub fn default_app_status() -> AppStatus {
    let data_root = fs_ops::default_data_root()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "~\\.skills-manage".to_string());

    AppStatus {
        app_name: "Skills Manage".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_root,
        safe_write_mode: "copy-with-trash".to_string(),
        github_token_configured: github::has_github_token(),
        home_dir: fs_ops::home_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
    }
}

/// Run blocking filesystem/network work on Tauri's blocking pool so the
/// window stays responsive (sync commands would run on the main thread).
async fn run_blocking<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| format!("Background task failed: {error}"))?
}

/// Like `run_blocking`, but holds the write lock for the whole operation.
async fn run_write<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    run_blocking(move || {
        // A panic in an earlier write poisons the lock; the data it guards is
        // on disk, so recovering the guard is safe.
        let _guard = WRITE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        work()
    })
    .await
}

#[tauri::command]
fn get_app_status() -> AppStatus {
    default_app_status()
}

#[tauri::command]
async fn scan_skills() -> Result<Vec<ScannedSkill>, String> {
    run_blocking(|| Ok(scan_default_skills())).await
}

#[tauri::command]
async fn scan_skill_folder(folder: String) -> Result<Vec<ScannedSkill>, String> {
    run_blocking(move || Ok(scanner::scan_folder(Path::new(&folder)))).await
}

#[tauri::command]
async fn import_skill_to_library(source_path: String) -> Result<ImportResult, String> {
    run_write(move || {
        let result = import_skill_to_library_impl(source_path.clone())?;
        if result.imported {
            // A tool's own folder that was just copied into the library becomes
            // that tool's managed copy, so it shows as enabled there.
            targets::adopt_imported_original(Path::new(&source_path), Path::new(&result.library_path));
        }
        Ok(result)
    })
    .await
}

#[tauri::command]
async fn update_skill_metadata(
    library_path: String,
    name: String,
    description: String,
) -> Result<metadata::MetadataResult, String> {
    run_write(move || metadata::update_skill_metadata(library_path, name, description)).await
}

#[tauri::command]
async fn remove_skill_from_library(library_path: String) -> Result<RemoveResult, String> {
    run_write(move || remove_skill_from_library_impl(library_path)).await
}

#[tauri::command]
async fn import_from_github(url: String) -> Result<GithubImportResult, String> {
    run_write(move || import_from_github_impl(url)).await
}

#[tauri::command]
async fn link_github_series(url: String) -> Result<LinkSeriesResult, String> {
    run_write(move || link_github_series_impl(url)).await
}

#[tauri::command]
async fn unlink_github_series(repository: String) -> Result<usize, String> {
    run_write(move || series::unlink_github_series(repository)).await
}

#[tauri::command]
async fn check_skill_updates() -> Result<Vec<SkillUpdateStatus>, String> {
    run_blocking(check_skill_updates_impl).await
}

#[tauri::command]
async fn update_skill_from_github(library_path: String) -> Result<UpdateResult, String> {
    run_write(move || update_skill_from_github_impl(library_path)).await
}

#[tauri::command]
async fn set_skill_target_enabled(
    source_path: String,
    target_id: String,
    enabled: bool,
    replace_existing: Option<bool>,
) -> Result<TargetToggleResult, String> {
    let replace = replace_existing.unwrap_or(false);
    run_write(move || set_skill_target_enabled_impl(source_path, target_id, enabled, replace)).await
}

#[tauri::command]
async fn set_skill_targets_bulk(
    source_paths: Vec<String>,
    target_id: String,
    enabled: bool,
) -> Result<BulkToggleResult, String> {
    run_write(move || set_skill_targets_bulk_impl(source_paths, target_id, enabled)).await
}

#[tauri::command]
async fn export_skillpack(
    sources: Vec<ExportSkillInput>,
    destination: String,
) -> Result<ExportResult, String> {
    run_blocking(move || export_skillpack_impl(sources, destination)).await
}

#[tauri::command]
async fn import_skillpack(package_path: String) -> Result<ImportPackResult, String> {
    run_write(move || import_skillpack_impl(package_path)).await
}

#[tauri::command]
async fn get_trash_summary(older_than_days: u32) -> Result<TrashSummary, String> {
    run_blocking(move || trash::trash_summary(older_than_days)).await
}

#[tauri::command]
async fn purge_trash(older_than_days: u32) -> Result<PurgeResult, String> {
    run_write(move || trash::purge_trash(older_than_days)).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_app_status,
            scan_skills,
            scan_skill_folder,
            import_skill_to_library,
            remove_skill_from_library,
            update_skill_metadata,
            import_from_github,
            link_github_series,
            unlink_github_series,
            check_skill_updates,
            update_skill_from_github,
            set_skill_target_enabled,
            set_skill_targets_bulk,
            export_skillpack,
            import_skillpack,
            get_trash_summary,
            purge_trash
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_status_reports_data_root() {
        let status = default_app_status();

        assert_eq!(status.app_name, "Skills Manage");
        assert!(!status.data_root.is_empty());
        assert_eq!(status.safe_write_mode, "copy-with-trash");
    }
}
