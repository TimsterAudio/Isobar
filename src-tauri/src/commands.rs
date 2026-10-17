use std::collections::BTreeMap;
use std::sync::Arc;

use isobar_core::{dayfile, DayRecord, LiveReading, MxClient};
use tauri::{AppHandle, Emitter, State};

use crate::poller;
use crate::settings::Settings;
use crate::state::{AppState, Status};

type S<'a> = State<'a, Arc<AppState>>;

#[tauri::command]
pub fn get_settings(state: S<'_>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn save_settings(state: S<'_>, settings: Settings) -> Result<(), String> {
    settings.save(&state.settings_path).map_err(|e| e.to_string())?;
    *state.settings.write().unwrap() = settings;
    state.wake.notify_one();
    Ok(())
}

#[tauri::command]
pub async fn test_station(
    url: String,
    username: Option<String>,
    password: Option<String>,
) -> Result<LiveReading, String> {
    let client = MxClient::new(&url, username, password).map_err(|e| e.to_string())?;
    client.current().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_live(state: S<'_>) -> Option<LiveReading> {
    state.live.read().unwrap().clone()
}

#[tauri::command]
pub fn get_status(state: S<'_>) -> Status {
    state.status()
}

#[tauri::command]
pub fn get_days(state: S<'_>) -> Result<Vec<DayRecord>, String> {
    state.store.lock().unwrap().days().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_samples(state: S<'_>, hours: u32) -> Result<BTreeMap<String, Vec<(i64, f64)>>, String> {
    let since = chrono::Utc::now().timestamp_millis() - i64::from(hours) * 3_600_000;
    state.store.lock().unwrap().samples_since(since).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_history(app: AppHandle, state: S<'_>, force: bool) -> Result<usize, String> {
    poller::sync_history(&app, &state, force).await
}

/// One-off import of a dayfile copy (for example, an archive from an old
/// station location), merged into the local history.
#[tauri::command]
pub fn import_dayfile(app: AppHandle, state: S<'_>, path: String) -> Result<usize, String> {
    let text = std::fs::read_to_string(&path).map_err(|e| format!("Could not read {path}: {e}"))?;
    let (records, skipped) = dayfile::parse_file(&text);
    if records.is_empty() {
        return Err(format!("No dayfile lines found in {path} ({skipped} lines skipped)."));
    }
    let mut store = state.store.lock().unwrap();
    let n = store.upsert_days(&records).map_err(|e| e.to_string())?;
    let count = store.day_count().unwrap_or(0);
    drop(store);
    let st = state.update_status(|st| st.day_count = count);
    let _ = app.emit("status", &st);
    let _ = app.emit("history-updated", count);
    Ok(n)
}

#[tauri::command]
pub fn clear_history(app: AppHandle, state: S<'_>) -> Result<(), String> {
    let mut store = state.store.lock().unwrap();
    store.clear_days().map_err(|e| e.to_string())?;
    store.meta_set("mx_total", "0").ok();
    drop(store);
    let st = state.update_status(|st| st.day_count = 0);
    let _ = app.emit("status", &st);
    let _ = app.emit("history-updated", 0);
    Ok(())
}
