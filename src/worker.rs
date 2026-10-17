//! The background thread: polls live conditions, records samples, checks
//! alerts and keeps the daily history in sync. The window reads what it needs
//! from [`Shared`] and sends requests as [`Cmd`]s.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui;
use isobar_core::{dayfile, demo, DayRecord, LiveReading, MxClient, Store};

use crate::alerts::{self, AlertTracker};
use crate::settings::Settings;

const SAMPLE_EVERY_MS: i64 = 60_000;
const HISTORY_CHECK: Duration = Duration::from_secs(30 * 60);
const KEEP_SAMPLES_MS: i64 = 21 * 24 * 60 * 60_000;
const DAY_MS: i64 = 24 * 3_600_000;

pub type Samples = BTreeMap<String, Vec<(i64, f64)>>;

#[derive(Debug, Clone, Default)]
pub struct Status {
    pub connected: bool,
    pub last_ok: Option<i64>,
    pub last_error: Option<String>,
    pub syncing: bool,
    pub sync_progress: Option<(usize, usize)>,
    pub day_count: usize,
    pub last_sync: Option<i64>,
}

/// The outcome of something the person asked for in Settings.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub ok: bool,
    pub text: String,
}

#[derive(Default)]
pub struct Shared {
    pub live: Option<LiveReading>,
    pub status: Status,
    /// All daily records, oldest first. Replaced, never edited, so views can
    /// hold on to a copy cheaply.
    pub days: Arc<Vec<DayRecord>>,
    /// Bumped whenever `days` changes, so views know to recompute.
    pub days_version: u64,
    /// The last 24 hours of samples.
    pub samples: Arc<Samples>,
    pub history_outcome: Option<Outcome>,
}

pub enum Cmd {
    Settings(Settings),
    Sync {
        force: bool,
    },
    Import(PathBuf),
    ClearHistory,
    /// End the thread (when switching to the demo station).
    Stop,
}

#[derive(Clone)]
pub struct Handle {
    pub shared: Arc<Mutex<Shared>>,
    tx: Sender<Cmd>,
}

impl Handle {
    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Start the worker. With `store` it talks to the station in `settings`;
/// without, it runs the demo station.
pub fn spawn(ctx: egui::Context, settings: Settings, store: Option<Store>) -> Handle {
    let shared = Arc::new(Mutex::new(Shared::default()));
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = Handle {
        shared: shared.clone(),
        tx,
    };
    let h = handle.clone();
    std::thread::Builder::new()
        .name("isobar-worker".into())
        .spawn(move || match store {
            Some(store) => Worker::new(h, ctx, settings, store).run(rx),
            None => run_demo(h, ctx, rx),
        })
        .expect("could not start the background thread");
    handle
}

struct Worker {
    h: Handle,
    ctx: egui::Context,
    settings: Settings,
    store: Store,
    alerts: AlertTracker,
    last_sample: i64,
    last_history_check: Option<Instant>,
    backfilled_for: Option<String>,
    file_mtime: Option<SystemTime>,
}

impl Worker {
    fn new(h: Handle, ctx: egui::Context, settings: Settings, store: Store) -> Worker {
        let w = Worker {
            h,
            ctx,
            settings,
            store,
            alerts: AlertTracker::default(),
            last_sample: 0,
            last_history_check: None,
            backfilled_for: None,
            file_mtime: None,
        };
        let last_sync = w
            .store
            .meta_get("last_sync")
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok());
        w.update(|s| s.status.last_sync = last_sync);
        w.reload_days();
        w.reload_samples();
        w
    }

    fn update(&self, f: impl FnOnce(&mut Shared)) {
        f(&mut self.h.lock());
        self.ctx.request_repaint();
    }

    fn reload_days(&self) {
        let days = self.store.days().unwrap_or_default();
        self.update(|s| {
            s.status.day_count = days.len();
            s.days = Arc::new(days);
            s.days_version += 1;
        });
    }

    fn reload_samples(&self) {
        let samples = self
            .store
            .samples_since(now_ms() - DAY_MS)
            .unwrap_or_default();
        self.update(|s| s.samples = Arc::new(samples));
    }

    fn client(&self) -> Result<MxClient, String> {
        let s = &self.settings;
        MxClient::new(&s.station_url, s.username.clone(), s.password.clone())
            .map_err(|e| e.to_string())
    }

    fn run(mut self, rx: Receiver<Cmd>) {
        loop {
            let wait = self.tick();
            let deadline = Instant::now() + wait;
            // Handle requests until it's time to poll again.
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                match rx.recv_timeout(left) {
                    Ok(Cmd::Stop) => return,
                    Ok(cmd) => {
                        if self.handle(cmd) {
                            break;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        }
    }

    /// Returns true if the poll should run straight away.
    fn handle(&mut self, cmd: Cmd) -> bool {
        match cmd {
            Cmd::Settings(s) => {
                let station_changed = s.station_url != self.settings.station_url
                    || s.username != self.settings.username
                    || s.password != self.settings.password
                    || s.dayfile_path != self.settings.dayfile_path;
                self.settings = s;
                if station_changed {
                    self.backfilled_for = None;
                    self.last_history_check = None;
                    self.file_mtime = None;
                }
                true
            }
            Cmd::Sync { force } => {
                let outcome = match self.sync_history(force) {
                    Ok(n) => Outcome {
                        ok: true,
                        text: format!("Downloaded {n} days."),
                    },
                    Err(e) => Outcome { ok: false, text: e },
                };
                self.update(|s| s.history_outcome = Some(outcome));
                false
            }
            Cmd::Import(path) => {
                let outcome = match self.import(&path) {
                    Ok(n) => Outcome {
                        ok: true,
                        text: format!("Imported {n} days from {}.", path.display()),
                    },
                    Err(e) => Outcome { ok: false, text: e },
                };
                self.update(|s| s.history_outcome = Some(outcome));
                false
            }
            Cmd::Stop => false,
            Cmd::ClearHistory => {
                let r = self.store.clear_days();
                let _ = self.store.meta_set("mx_total", "0");
                self.reload_days();
                let outcome = match r {
                    Ok(()) => Outcome {
                        ok: true,
                        text: "Local history deleted.".into(),
                    },
                    Err(e) => Outcome {
                        ok: false,
                        text: format!("Couldn't delete the history: {e}"),
                    },
                };
                self.update(|s| s.history_outcome = Some(outcome));
                false
            }
        }
    }

    /// One round of polling. Returns how long to wait before the next.
    fn tick(&mut self) -> Duration {
        if !self.settings.has_station() {
            // File mode: re-import the dayfile whenever it changes.
            if let Some(path) = self.settings.dayfile().map(str::to_string) {
                let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                if mtime.is_some() && mtime != self.file_mtime {
                    self.file_mtime = mtime;
                    if let Err(e) = self.sync_history(true) {
                        self.update(|s| s.status.last_error = Some(e));
                    }
                }
            }
            self.update(|s| {
                s.status.connected = false;
                if self.settings.dayfile().is_none() {
                    s.status.last_error = None;
                }
            });
            return Duration::from_secs(15);
        }

        let client = match self.client() {
            Ok(c) => c,
            Err(e) => {
                self.update(|s| {
                    s.status.connected = false;
                    s.status.last_error = Some(e);
                });
                return Duration::from_secs(15);
            }
        };

        if self.backfilled_for.as_deref() != Some(client.base()) {
            self.backfilled_for = Some(client.base().to_string());
            let series = client.recent_series();
            if !series.is_empty() {
                for (metric, pts) in &series {
                    let _ = self.store.insert_series(metric, pts);
                }
                self.reload_samples();
            }
        }

        match client.current() {
            Ok(reading) => {
                let now = reading.fetched_at;
                if now - self.last_sample >= SAMPLE_EVERY_MS {
                    self.last_sample = now;
                    let metrics = reading.sample_metrics();
                    let _ = self.store.insert_samples(now, &metrics);
                    let _ = self.store.prune_samples(now - KEEP_SAMPLES_MS);
                    self.update(|s| {
                        let samples = Arc::make_mut(&mut s.samples);
                        for (k, v) in &metrics {
                            let series = samples.entry(k.to_string()).or_default();
                            series.push((now, *v));
                            let cut = series.partition_point(|p| p.0 < now - DAY_MS);
                            series.drain(..cut);
                        }
                    });
                }
                let s = &self.settings;
                for (title, body) in self.alerts.check(&s.alerts, &s.station_name, &reading) {
                    alerts::notify(&title, &body);
                }
                self.update(|s| {
                    s.live = Some(reading);
                    s.status.connected = true;
                    s.status.last_ok = Some(now);
                    s.status.last_error = None;
                });
            }
            Err(e) => self.update(|s| {
                s.status.connected = false;
                s.status.last_error = Some(e.to_string());
            }),
        }

        if self
            .last_history_check
            .is_none_or(|t| t.elapsed() > HISTORY_CHECK)
        {
            self.last_history_check = Some(Instant::now());
            if let Err(e) = self.sync_history(false) {
                self.update(|s| s.status.last_error = Some(e));
            }
        }

        Duration::from_secs(self.settings.poll_seconds.clamp(3, 600))
    }

    /// Bring the local history up to date. With `force`, download everything;
    /// otherwise only when the station's line count has changed. Returns the
    /// number of records written.
    fn sync_history(&mut self, force: bool) -> Result<usize, String> {
        let records = if self.settings.has_station() {
            let client = self.client()?;
            let total = client.dayfile_total().map_err(|e| e.to_string())?;
            let known: usize = self
                .store
                .meta_get("mx_total")
                .ok()
                .flatten()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let have_any = self.store.day_count().unwrap_or(0) > 0;
            if !force && have_any && total == known {
                return Ok(0);
            }
            self.update(|s| {
                s.status.syncing = true;
                s.status.sync_progress = Some((0, total));
            });
            let h = self.h.clone();
            let ctx = self.ctx.clone();
            let result = client.dayfile_all(move |done, total| {
                h.lock().status.sync_progress = Some((done, total));
                ctx.request_repaint();
            });
            self.update(|s| {
                s.status.syncing = false;
                s.status.sync_progress = None;
            });
            let records = result.map_err(|e| e.to_string())?;
            let _ = self.store.meta_set("mx_total", &total.to_string());
            records
        } else if let Some(path) = self.settings.dayfile() {
            let text =
                std::fs::read_to_string(path).map_err(|e| format!("Couldn't read {path}: {e}"))?;
            dayfile::parse_file(&text).0
        } else {
            return Err("Add a station address or a dayfile in Settings first.".into());
        };

        let n = self
            .store
            .upsert_days(&records)
            .map_err(|e| e.to_string())?;
        let now = now_ms();
        let _ = self.store.meta_set("last_sync", &now.to_string());
        self.update(|s| s.status.last_sync = Some(now));
        self.reload_days();
        Ok(n)
    }

    /// Merge a dayfile copy (an archive from an earlier station, say) into
    /// the local history.
    fn import(&mut self, path: &Path) -> Result<usize, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
        let (records, skipped) = dayfile::parse_file(&text);
        if records.is_empty() {
            return Err(format!(
                "No dayfile lines found in {} ({skipped} lines skipped).",
                path.display()
            ));
        }
        let n = self
            .store
            .upsert_days(&records)
            .map_err(|e| e.to_string())?;
        self.reload_days();
        Ok(n)
    }
}

/// The demo station: synthetic history, and a live reading every five
/// seconds.
fn run_demo(h: Handle, ctx: egui::Context, rx: Receiver<Cmd>) {
    let days = demo::days();
    let samples = demo::samples(now_ms());
    {
        let mut s = h.lock();
        s.status.day_count = days.len();
        s.status.last_sync = Some(now_ms() - 600_000);
        s.days = Arc::new(days);
        s.days_version += 1;
        s.samples = Arc::new(samples.clone());
    }
    let mut n = 0u32;
    loop {
        n += 1;
        let jitter = ((n * 7919) % 21) as f64 / 10.0 - 1.0;
        let reading = demo::live(&samples, now_ms(), jitter);
        {
            let mut s = h.lock();
            s.status.connected = true;
            s.status.last_ok = Some(reading.fetched_at);
            s.live = Some(reading);
        }
        ctx.request_repaint();
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Cmd::Sync { .. }) | Ok(Cmd::Import(_)) | Ok(Cmd::ClearHistory) => {
                h.lock().history_outcome = Some(Outcome {
                    ok: false,
                    text: "The demo station's history can't be changed.".into(),
                });
                ctx.request_repaint();
            }
            Ok(Cmd::Settings(_)) | Err(RecvTimeoutError::Timeout) => {}
            Ok(Cmd::Stop) | Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}
