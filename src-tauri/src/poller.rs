//! Background loop: polls live conditions, records samples, checks alerts and
//! keeps the daily history in sync.

use std::sync::Arc;
use std::time::Duration;

use isobar_core::{dayfile, MxClient};
use tauri::{AppHandle, Emitter};

use crate::alerts::AlertTracker;
use crate::state::AppState;

const SAMPLE_EVERY_MS: i64 = 60_000;
const HISTORY_CHECK_MS: i64 = 30 * 60_000;
const KEEP_SAMPLES_MS: i64 = 21 * 24 * 60 * 60_000;

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

async fn wait(state: &AppState, d: Duration) {
    tokio::select! {
        _ = tokio::time::sleep(d) => {}
        _ = state.wake.notified() => {}
    }
}

pub fn spawn(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut alerts = AlertTracker::default();
        let mut last_sample = 0i64;
        let mut last_history_check = 0i64;
        let mut backfilled_for: Option<String> = None;
        let mut file_mtime: Option<std::time::SystemTime> = None;

        loop {
            let s = state.settings();

            if !s.has_station() {
                // File mode: re-import the dayfile whenever it changes.
                if let Some(path) = s.dayfile_path.as_deref().filter(|p| !p.is_empty()) {
                    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
                    if mtime.is_some() && mtime != file_mtime {
                        file_mtime = mtime;
                        let _ = sync_history(&app, &state, true).await;
                    }
                }
                let st = state.update_status(|st| {
                    st.connected = false;
                    st.last_error = None;
                });
                let _ = app.emit("status", &st);
                wait(&state, Duration::from_secs(15)).await;
                continue;
            }

            let client = match MxClient::new(&s.station_url, s.username.clone(), s.password.clone()) {
                Ok(c) => c,
                Err(e) => {
                    let st = state.update_status(|st| {
                        st.connected = false;
                        st.last_error = Some(e.to_string());
                    });
                    let _ = app.emit("status", &st);
                    wait(&state, Duration::from_secs(15)).await;
                    continue;
                }
            };

            if backfilled_for.as_deref() != Some(client.base()) {
                backfilled_for = Some(client.base().to_string());
                let series = client.recent_series().await;
                if !series.is_empty() {
                    let mut store = state.store.lock().unwrap();
                    for (metric, pts) in &series {
                        let _ = store.insert_series(metric, pts);
                    }
                    drop(store);
                    let _ = app.emit("samples-updated", ());
                }
                last_history_check = 0;
            }

            match client.current().await {
                Ok(reading) => {
                    let now = reading.fetched_at;
                    *state.live.write().unwrap() = Some(reading.clone());
                    let _ = app.emit("live", &reading);

                    if now - last_sample >= SAMPLE_EVERY_MS {
                        last_sample = now;
                        let mut store = state.store.lock().unwrap();
                        let _ = store.insert_samples(now, &reading.sample_metrics());
                        let _ = store.prune_samples(now - KEEP_SAMPLES_MS);
                    }

                    alerts.check(&app, &s.alerts, &s.station_name, &reading);
                    let st = state.update_status(|st| {
                        st.connected = true;
                        st.last_ok = Some(now);
                        st.last_error = None;
                    });
                    let _ = app.emit("status", &st);
                }
                Err(e) => {
                    let st = state.update_status(|st| {
                        st.connected = false;
                        st.last_error = Some(e.to_string());
                    });
                    let _ = app.emit("status", &st);
                }
            }

            if now_ms() - last_history_check > HISTORY_CHECK_MS {
                last_history_check = now_ms();
                let _ = sync_history(&app, &state, false).await;
            }

            wait(&state, Duration::from_secs(s.poll_seconds.clamp(3, 600))).await;
        }
    });
}

/// Bring the local daily history up to date. With `force`, re-download
/// everything; otherwise only when the station's line count has changed.
/// Returns the number of records written.
pub async fn sync_history(app: &AppHandle, state: &AppState, force: bool) -> Result<usize, String> {
    let _guard = state.sync_lock.lock().await;
    let s = state.settings();

    let set_syncing = |on: bool| {
        let st = state.update_status(|st| st.syncing = on);
        let _ = app.emit("status", &st);
    };

    let records = if s.has_station() {
        let client = MxClient::new(&s.station_url, s.username.clone(), s.password.clone())
            .map_err(|e| e.to_string())?;
        let total = client.dayfile_total().await.map_err(|e| e.to_string())?;
        let known: usize = {
            let store = state.store.lock().unwrap();
            store.meta_get("mx_total").ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(0)
        };
        let have_any = state.store.lock().unwrap().day_count().unwrap_or(0) > 0;
        if !force && have_any && total == known {
            return Ok(0);
        }
        set_syncing(true);
        let app2 = app.clone();
        let result = client
            .dayfile_all(move |done, total| {
                let _ = app2.emit("sync-progress", (done, total));
            })
            .await;
        set_syncing(false);
        let records = result.map_err(|e| e.to_string())?;
        state.store.lock().unwrap().meta_set("mx_total", &total.to_string()).ok();
        records
    } else if let Some(path) = s.dayfile_path.as_deref().filter(|p| !p.is_empty()) {
        let text = std::fs::read_to_string(path).map_err(|e| format!("Could not read {path}: {e}"))?;
        dayfile::parse_file(&text).0
    } else {
        return Err("Add a station address or a dayfile in Settings first.".into());
    };

    let written = {
        let mut store = state.store.lock().unwrap();
        let n = store.upsert_days(&records).map_err(|e| e.to_string())?;
        store.meta_set("last_sync", &now_ms().to_string()).ok();
        n
    };
    let count = state.store.lock().unwrap().day_count().unwrap_or(0);
    let st = state.update_status(|st| {
        st.day_count = count;
        st.last_sync = Some(now_ms());
    });
    let _ = app.emit("status", &st);
    let _ = app.emit("history-updated", count);
    Ok(written)
}
