use serde::Serialize;

mod scanner;
use scanner::{scan_default_skills, ScannedSkill};

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![get_app_status, scan_skills])
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
