//! Poieweave desktop host.
//!
//! The webview only gets a small, explicit command surface: host info, seeded
//! notes/events backed by SQLite, and a size-limited read/write sandbox under
//! the app data directory. Packaging lives in `tauri.conf.json`.

mod db;
mod sandbox;

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{Manager, State};

use crate::db::{Db, EventRow};

struct AppState {
    db: Mutex<Db>,
    data_dir: PathBuf,
    sandbox_root: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HostInfo {
    version: &'static str,
    data_dir: String,
    db_path: String,
    sandbox_root: String,
    platform: &'static str,
}

fn lock_db(state: &AppState) -> Result<std::sync::MutexGuard<'_, Db>, String> {
    state.db.lock().map_err(|_| "database lock poisoned".to_string())
}

#[tauri::command]
fn host_info(state: State<'_, AppState>) -> HostInfo {
    HostInfo {
        version: env!("CARGO_PKG_VERSION"),
        data_dir: state.data_dir.display().to_string(),
        db_path: state.data_dir.join("poieweave.sqlite3").display().to_string(),
        sandbox_root: state.sandbox_root.display().to_string(),
        platform: std::env::consts::OS,
    }
}

#[tauri::command]
fn notes_all(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    lock_db(&state)?.notes()
}

#[tauri::command]
fn events_list(note_id: String, state: State<'_, AppState>) -> Result<Vec<EventRow>, String> {
    lock_db(&state)?.events(&note_id)
}

#[tauri::command]
fn event_append(note_id: String, text: String, at: String, state: State<'_, AppState>) -> Result<EventRow, String> {
    if text.trim().is_empty() {
        return Err("event text must not be empty".into());
    }
    lock_db(&state)?.append_event(&note_id, &at, text.trim())
}

#[tauri::command]
fn sandbox_read(relative: String, state: State<'_, AppState>) -> Result<String, String> {
    sandbox::read(&state.sandbox_root, &relative)
}

#[tauri::command]
fn sandbox_write(relative: String, contents: String, state: State<'_, AppState>) -> Result<String, String> {
    sandbox::write(&state.sandbox_root, &relative, &contents)
}

#[tauri::command]
fn sandbox_root(state: State<'_, AppState>) -> String {
    state.sandbox_root.display().to_string()
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
            std::fs::create_dir_all(&data_dir).map_err(|error| format!("data dir: {error}"))?;
            let sandbox_root = data_dir.join("sandbox");
            std::fs::create_dir_all(&sandbox_root).map_err(|error| format!("sandbox: {error}"))?;
            let mut db = Db::open(&data_dir.join("poieweave.sqlite3"))?;
            db.seed(include_str!("../../src/data/notes.json"))?;
            app.manage(AppState { db: Mutex::new(db), data_dir, sandbox_root });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            host_info,
            notes_all,
            events_list,
            event_append,
            sandbox_read,
            sandbox_write,
            sandbox_root
        ])
        .run(tauri::generate_context!())
        .expect("error while running Poieweave desktop");
}
