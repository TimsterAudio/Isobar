use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use isobar_core::{LiveReading, Store};
use serde::Serialize;
use tokio::sync::Notify;

use crate::settings::Settings;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub connected: bool,
    pub last_ok: Option<i64>,
    pub last_error: Option<String>,
    pub syncing: bool,
    pub day_count: usize,
    pub last_sync: Option<i64>,
}

pub struct AppState {
    pub settings: RwLock<Settings>,
    pub settings_path: PathBuf,
    pub store: Mutex<Store>,
    pub live: RwLock<Option<LiveReading>>,
    pub status: RwLock<Status>,
    /// Wakes the poller early (after settings change).
    pub wake: Notify,
    /// Serialises history syncs.
    pub sync_lock: tokio::sync::Mutex<()>,
}

impl AppState {
    pub fn new(settings: Settings, settings_path: PathBuf, store: Store) -> Self {
        let day_count = store.day_count().unwrap_or(0);
        let last_sync = store
            .meta_get("last_sync")
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok());
        AppState {
            settings: RwLock::new(settings),
            settings_path,
            store: Mutex::new(store),
            live: RwLock::new(None),
            status: RwLock::new(Status {
                day_count,
                last_sync,
                ..Default::default()
            }),
            wake: Notify::new(),
            sync_lock: tokio::sync::Mutex::new(()),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    pub fn status(&self) -> Status {
        self.status.read().unwrap().clone()
    }

    pub fn update_status(&self, f: impl FnOnce(&mut Status)) -> Status {
        let mut s = self.status.write().unwrap();
        f(&mut s);
        s.clone()
    }
}
