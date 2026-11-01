//! Poieweave desktop host.
//!
//! The webview only gets a small, explicit command surface: host info, seeded
//! notes/events backed by SQLite, and a size-limited read/write sandbox under
//! the app data directory. Packaging lives in `tauri.conf.json`.

mod auth;
mod db;
mod domain;
mod execution;
mod federation_view;
mod http;
mod inference;
mod sandbox;
mod secrets;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::auth::{AuthStatus, SessionState};
use crate::db::{AuditRow, Db, EventRow};
use crate::domain::DomainOverview;
use crate::execution::ExecutionSnapshot;
use crate::federation_view::FederationSnapshot;
use crate::http::CurlTransport;
use crate::inference::{InferenceResult, InferenceStatus};
use crate::secrets::SystemSecretStore;

struct AppState {
    db: Arc<Mutex<Db>>,
    data_dir: PathBuf,
    sandbox_root: PathBuf,
    domain: DomainOverview,
    execution: ExecutionSnapshot,
    federation: FederationSnapshot,
    session: Arc<SessionState>,
    secrets: Arc<dyn secrets::SecretStore>,
    http: Arc<dyn http::HttpTransport>,
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
fn auth_status(state: State<'_, AppState>) -> Result<AuthStatus, String> {
    let db = lock_db(&state)?;
    auth::status(&db, &state.session)
}

#[tauri::command]
fn auth_register(display_name: String, passphrase: String, state: State<'_, AppState>) -> Result<AuthStatus, String> {
    let db = lock_db(&state)?;
    auth::register(&db, &state.session, &display_name, &passphrase)
}

#[tauri::command]
fn auth_sign_in(passphrase: String, state: State<'_, AppState>) -> Result<AuthStatus, String> {
    let db = lock_db(&state)?;
    auth::sign_in(&db, &state.session, &passphrase)
}

#[tauri::command]
fn auth_sign_out(state: State<'_, AppState>) -> Result<AuthStatus, String> {
    let db = lock_db(&state)?;
    auth::sign_out(&db, &state.session)
}

#[tauri::command]
fn audit_list(limit: Option<usize>, state: State<'_, AppState>) -> Result<Vec<AuditRow>, String> {
    auth::require(&state.session)?;
    let db = lock_db(&state)?;
    db.audit(limit.unwrap_or(20).clamp(1, 100))
}

#[tauri::command]
fn inference_status(state: State<'_, AppState>) -> Result<InferenceStatus, String> {
    let db = lock_db(&state)?;
    inference::status(&db)
}

#[tauri::command]
fn inference_configure(
    endpoint: String,
    model: String,
    api_key: String,
    state: State<'_, AppState>,
) -> Result<InferenceStatus, String> {
    let db = lock_db(&state)?;
    inference::configure(&db, state.secrets.as_ref(), &state.session, &endpoint, &model, &api_key)
}

#[tauri::command]
fn inference_forget(state: State<'_, AppState>) -> Result<InferenceStatus, String> {
    let db = lock_db(&state)?;
    inference::forget(&db, state.secrets.as_ref(), &state.session)
}

#[tauri::command]
async fn inference_test(prompt: String, state: State<'_, AppState>) -> Result<InferenceResult, String> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    let http = Arc::clone(&state.http);
    let session = Arc::clone(&state.session);
    tauri::async_runtime::spawn_blocking(move || {
        let db = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        inference::complete(&db, secrets.as_ref(), &session, http.as_ref(), &prompt)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn open_workspace_window(app: tauri::AppHandle, view: String) -> Result<(), String> {
    let allowed = ["overview", "capabilities", "plan", "execution", "federation", "evidence"];
    if !allowed.contains(&view.as_str()) {
        return Err(format!("unknown workspace view {view:?}"));
    }
    let label = format!("poie-view-{view}");
    if let Some(window) = app.get_webview_window(&label) {
        window.set_focus().map_err(|error| error.to_string())?;
        return Ok(());
    }
    let (width, height, title) = match view.as_str() {
        "federation" => (1240.0, 760.0, "Poieweave · federation wall"),
        "execution" => (1100.0, 760.0, "Poieweave · execution monitor"),
        "evidence" => (1100.0, 800.0, "Poieweave · evidence"),
        _ => (1180.0, 820.0, "Poieweave · focused view"),
    };
    WebviewWindowBuilder::new(&app, label, WebviewUrl::App(format!("index.html?view={view}&focus=1").into()))
        .title(title)
        .inner_size(width, height)
        .min_inner_size(900.0, 620.0)
        .build()
        .map_err(|error| format!("open window: {error}"))?;
    Ok(())
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
fn domain_overview(state: State<'_, AppState>) -> DomainOverview {
    state.domain.clone()
}

#[tauri::command]
fn execution_snapshot(state: State<'_, AppState>) -> ExecutionSnapshot {
    state.execution.clone()
}

#[tauri::command]
fn federation_snapshot(state: State<'_, AppState>) -> FederationSnapshot {
    state.federation.clone()
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
            let domain = domain::overview().map_err(|error| format!("domain snapshot: {error}"))?;
            let execution = execution::snapshot(&domain).map_err(|error| format!("execution snapshot: {error}"))?;
            let federation = federation_view::snapshot().map_err(|error| format!("federation snapshot: {error}"))?;
            app.manage(AppState {
                db: Arc::new(Mutex::new(db)),
                data_dir,
                sandbox_root,
                domain,
                execution,
                federation,
                session: Arc::new(SessionState::new(None)),
                secrets: Arc::new(SystemSecretStore),
                http: Arc::new(CurlTransport),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            auth_status,
            auth_register,
            auth_sign_in,
            auth_sign_out,
            audit_list,
            inference_status,
            inference_configure,
            inference_forget,
            inference_test,
            open_workspace_window,
            host_info,
            domain_overview,
            execution_snapshot,
            federation_snapshot,
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
