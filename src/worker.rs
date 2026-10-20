//! The background thread: polls live conditions, records samples, checks
//! alerts and keeps the daily history in sync. The window reads what it needs
//! from [`Shared`] and sends requests as [`Cmd`]s.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use chrono::{Datelike, NaiveDate, TimeZone};
use eframe::egui;
use isobar_core::logfile::{self, BurstYear, Fall24};
use isobar_core::tanks::{self, LogDay};
use isobar_core::{dayfile, demo, DayRecord, LiveReading, MxClient, Store};

use crate::alerts::{self, AlertTracker};
use crate::settings::Settings;

const SAMPLE_EVERY_MS: i64 = 60_000;
const HISTORY_CHECK: Duration = Duration::from_secs(30 * 60);
const KEEP_SAMPLES_MS: i64 = 21 * 24 * 60 * 60_000;
const DAY_MS: i64 = 24 * 3_600_000;
const BACKFILL_AFTER_GAP_MS: i64 = 15 * 60_000;

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
    /// The last fill of recent detail from MX's graph data.
    pub backfill: Option<BackfillStatus>,
    /// Fetching MX's monthly log files.
    pub logs: Option<LogStatus>,
}

#[derive(Debug, Clone, Default)]
pub struct LogStatus {
    /// Months fetched, and months to fetch in all.
    pub done: usize,
    pub total: usize,
    /// Log readings stored, and the first and last times (Unix ms).
    pub records: usize,
    pub first: Option<i64>,
    pub last: Option<i64>,
    pub error: Option<String>,
}

/// What the stored log readings show, worked out after each batch of
/// months arrives.
#[derive(Debug, Default)]
pub struct LogSummary {
    pub falls: Vec<Fall24>,
    pub bursts: Vec<BurstYear>,
    /// Inside against outside, from the logs, or Isobar's own readings until
    /// the logs have indoor temperatures.
    pub indoor: Option<isobar_core::indoor::IndoorSummary>,
}

/// Readings for a stretch of time, asked for by a view (the chart on Now).
#[derive(Debug)]
pub struct Detail {
    pub key: String,
    pub series: Samples,
    pub at: Instant,
}

#[derive(Debug, Clone, Default)]
pub struct BackfillStatus {
    /// When it ran (Unix ms).
    pub at: i64,
    /// Readings received across all series.
    pub points: usize,
    /// Graph-data files MX didn't serve, with the reason.
    pub failed: Vec<(String, String)>,
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
    /// The imported tank record, oldest first, and a count bumped when it
    /// changes.
    pub tank_log: Arc<Vec<LogDay>>,
    pub tank_version: u64,
    pub tank_outcome: Option<Outcome>,
    pub log_summary: Arc<LogSummary>,
    pub detail: Option<Arc<Detail>>,
}

pub enum Cmd {
    Settings(Box<Settings>),
    Sync {
        force: bool,
    },
    Import(PathBuf),
    ClearHistory,
    /// Read a tank record from a spreadsheet or CSV, replacing the last one.
    ImportTankLog(PathBuf),
    ClearTankLog,
    /// Readings of `metrics` from `from_ms` to `to_ms`, into `Shared::detail`.
    Detail {
        key: String,
        metrics: Vec<String>,
        from_ms: i64,
        to_ms: i64,
    },
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
    /// Months of MX's log files fetched for good, as (year, month).
    log_done: BTreeSet<(i32, u32)>,
    /// When the current month's log was last topped up.
    log_current_at: Option<Instant>,
    /// Don't try the logs again before this, after an error.
    log_wait_until: Option<Instant>,
    log_summary_at: Option<Instant>,
    log_summary_dirty: bool,
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
            log_done: BTreeSet::new(),
            log_current_at: None,
            log_wait_until: None,
            log_summary_at: None,
            log_summary_dirty: true,
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
        w.reload_tank_log();
        let mut w = w;
        w.log_done = w
            .store
            .meta_get("log_done")
            .ok()
            .flatten()
            .map(|s| {
                s.split(',')
                    .filter_map(|m| {
                        let (y, mo) = m.split_once('-')?;
                        Some((y.parse().ok()?, mo.parse().ok()?))
                    })
                    .collect()
            })
            .unwrap_or_default();
        // Isobar 0.2 stored every log line; from now on it keeps one per ten
        // minutes. Thin what an earlier version stored, once.
        if w.store.meta_get("log_step").ok().flatten().as_deref() != Some("600")
            && w.store.thin_all_log().is_ok()
        {
            let _ = w.store.meta_set("log_step", "600");
        }
        w.refresh_log_summary();
        w
    }

    /// One step of fetching MX's log files: the current month every ten
    /// minutes, otherwise the newest month not yet fetched. One month a step
    /// keeps the live readings coming.
    /// Returns true if there's more to fetch.
    fn sync_logs_step(&mut self, client: &MxClient) -> bool {
        if self.log_wait_until.is_some_and(|t| Instant::now() < t) {
            return false;
        }
        let today = chrono::Local::now().date_naive();
        let first = self
            .h
            .lock()
            .days
            .first()
            .map_or(today - chrono::Duration::days(365), |d| d.date);
        let mut months = Vec::new();
        let (mut y, mut m) = (first.year(), first.month());
        while (y, m) <= (today.year(), today.month()) {
            months.push((y, m));
            (y, m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
        }
        let current = (today.year(), today.month());
        let target = if self
            .log_current_at
            .is_none_or(|t| t.elapsed() > Duration::from_secs(600))
        {
            Some(current)
        } else {
            months
                .iter()
                .rev()
                .find(|mo| **mo != current && !self.log_done.contains(mo))
                .copied()
        };
        if let Some((y, m)) = target {
            let start = NaiveDate::from_ymd_opt(y, m, 1).unwrap();
            let end = start
                .checked_add_months(chrono::Months::new(1))
                .map_or(start, |d| d - chrono::Duration::days(1))
                .min(today);
            let mut from = start;
            if (y, m) == current {
                // Only what's new since the last reading stored.
                let ms = |d: NaiveDate| chart_day_ms(d);
                if let Ok(Some(last)) = self.store.log_last_between(ms(start), ms(today) + DAY_MS) {
                    if let Some(d) = chrono::Local.timestamp_millis_opt(last).single() {
                        from = d.date_naive().max(start);
                    }
                }
            }
            match client.logfile(from, end) {
                Ok(recs) => {
                    let _ = self.store.upsert_log(&recs);
                    if !recs.is_empty() {
                        self.log_summary_dirty = true;
                    }
                    if (y, m) == current {
                        self.log_current_at = Some(Instant::now());
                    } else {
                        // Last month can still gain a day or two with a 9am
                        // rollover; count it done from the 3rd.
                        let last_month = start.checked_add_months(chrono::Months::new(1))
                            == Some(today.with_day(1).unwrap());
                        if !last_month || today.day() > 2 {
                            self.log_done.insert((y, m));
                            let list: Vec<String> = self
                                .log_done
                                .iter()
                                .map(|(y, m)| format!("{y}-{m}"))
                                .collect();
                            let _ = self.store.meta_set("log_done", &list.join(","));
                        }
                    }
                    self.update(|s| {
                        if let Some(l) = s.status.logs.as_mut() {
                            l.error = None;
                        }
                    });
                }
                Err(e) => {
                    self.log_wait_until = Some(Instant::now() + Duration::from_secs(30 * 60));
                    let text = e.to_string();
                    self.update(|s| {
                        s.status.logs.get_or_insert_with(Default::default).error = Some(text)
                    });
                }
            }
        }
        let remaining = months
            .iter()
            .filter(|mo| **mo != current && !self.log_done.contains(mo))
            .count();
        if self.log_summary_dirty
            && (remaining == 0
                || self
                    .log_summary_at
                    .is_none_or(|t| t.elapsed() > Duration::from_secs(300)))
        {
            self.refresh_log_summary();
        }
        let (records, first_ts, last_ts) = self.store.log_extent().unwrap_or((0, None, None));
        let total = months.len();
        let done = total - remaining - 1;
        self.update(|s| {
            let l = s.status.logs.get_or_insert_with(Default::default);
            l.done = done.min(total);
            l.total = total.saturating_sub(1);
            l.records = records;
            l.first = first_ts;
            l.last = last_ts;
        });
        remaining > 0 && self.log_wait_until.is_none_or(|t| Instant::now() >= t)
    }

    fn refresh_log_summary(&mut self) {
        self.log_summary_at = Some(Instant::now());
        self.log_summary_dirty = false;
        let Ok(all) =
            self.store
                .log_series(&["press", "rain_today", "intemp", "temp"], 0, i64::MAX)
        else {
            return;
        };
        let inches = self
            .h
            .lock()
            .live
            .as_ref()
            .is_some_and(|l| l.units.rain.trim().eq_ignore_ascii_case("in"));
        let summary = LogSummary {
            falls: all
                .get("press")
                .map_or_else(Vec::new, |p| logfile::biggest_falls_24h(p, 10)),
            bursts: all.get("rain_today").map_or_else(Vec::new, |r| {
                logfile::rain_by_rate(r, if inches { 25.4 } else { 1.0 })
            }),
            indoor: match (all.get("intemp"), all.get("temp")) {
                (Some(i), Some(o)) if i.len() > 144 => isobar_core::indoor::indoor_summary(i, o),
                _ => {
                    let samples = self.h.lock().samples.clone();
                    match (samples.get("intemp"), samples.get("temp")) {
                        (Some(i), Some(o)) => isobar_core::indoor::indoor_summary(i, o),
                        _ => None,
                    }
                }
            },
        };
        self.update(|s| s.log_summary = Arc::new(summary));
    }

    fn reload_tank_log(&self) {
        let log = self.store.tank_log().unwrap_or_default();
        self.update(|s| {
            s.tank_log = Arc::new(log);
            s.tank_version += 1;
        });
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
                self.settings = *s;
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
            Cmd::ImportTankLog(path) => {
                let today = chrono::Local::now().date_naive();
                let outcome = match tanks::read_log(&path, today).and_then(|log| {
                    self.store
                        .replace_tank_log(&log)
                        .map(|_| log)
                        .map_err(|e| format!("Couldn't save the record: {e}"))
                }) {
                    Ok(log) => Outcome {
                        ok: true,
                        text: format!(
                            "Imported {} days, {} to {}.",
                            log.len(),
                            log[0].date.format("%-d %b %Y"),
                            log[log.len() - 1].date.format("%-d %b %Y")
                        ),
                    },
                    Err(e) => Outcome { ok: false, text: e },
                };
                self.reload_tank_log();
                self.update(|s| s.tank_outcome = Some(outcome));
                false
            }
            Cmd::ClearTankLog => {
                let outcome = match self.store.replace_tank_log(&[]) {
                    Ok(()) => Outcome {
                        ok: true,
                        text: "Tank record deleted.".into(),
                    },
                    Err(e) => Outcome {
                        ok: false,
                        text: format!("Couldn't delete the tank record: {e}"),
                    },
                };
                self.reload_tank_log();
                self.update(|s| s.tank_outcome = Some(outcome));
                false
            }
            Cmd::Detail {
                key,
                metrics,
                from_ms,
                to_ms,
            } => {
                let m: Vec<&str> = metrics.iter().map(String::as_str).collect();
                let mut series = self
                    .store
                    .log_series(&m, from_ms, to_ms)
                    .unwrap_or_default();
                if let Ok(recent) = self.store.samples_since(from_ms) {
                    for (k, pts) in recent {
                        if metrics.contains(&k) {
                            let e = series.entry(k).or_default();
                            e.extend(pts.into_iter().filter(|p| p.0 <= to_ms));
                        }
                    }
                }
                let series = thin(series, from_ms, to_ms);
                self.update(|s| {
                    s.detail = Some(Arc::new(Detail {
                        key,
                        series,
                        at: Instant::now(),
                    }))
                });
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

        // Fill recent detail from MX's graph data on first connect, and again
        // after any gap (the computer slept, or the station was unreachable),
        // so the 24-hour charts have no holes MX could have filled.
        let gap = self.last_sample > 0 && now_ms() - self.last_sample > BACKFILL_AFTER_GAP_MS;
        if gap || self.backfilled_for.as_deref() != Some(client.base()) {
            self.backfilled_for = Some(client.base().to_string());
            let fill = client.recent_series();
            for (metric, pts) in &fill.series {
                let _ = self.store.insert_series(metric, pts);
            }
            if !fill.series.is_empty() {
                self.reload_samples();
            }
            let status = BackfillStatus {
                at: now_ms(),
                points: fill.points(),
                failed: fill.failed,
            };
            self.update(|s| s.status.backfill = Some(status));
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

        if self.h.lock().status.connected {
            // A few months at a time, but never holding up the next reading
            // for more than a couple of seconds.
            let started = Instant::now();
            for _ in 0..12 {
                if !self.sync_logs_step(&client) || started.elapsed() > Duration::from_secs(2) {
                    break;
                }
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

/// Local midnight at the start of `d`, as Unix ms.
fn chart_day_ms(d: NaiveDate) -> i64 {
    chrono::Local
        .from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap())
        .earliest()
        .map_or(0, |t| t.timestamp_millis())
}

/// Sort and merge each series, then cut it to at most about 1,500 points by
/// averaging (gusts keep their highest) across even slices of the range.
fn thin(series: Samples, from_ms: i64, to_ms: i64) -> Samples {
    const MAX: i64 = 1500;
    let span = (to_ms - from_ms).max(1);
    series
        .into_iter()
        .map(|(k, mut pts)| {
            pts.sort_by_key(|p| p.0);
            pts.dedup_by(|b, a| (b.0 - a.0).abs() < 30_000);
            if pts.len() as i64 <= MAX {
                return (k, pts);
            }
            let slice = span / MAX;
            let keep_max = k == "gust" || k == "rain_today" || k == "rain_midnight";
            let mut out: Vec<(i64, f64)> = Vec::new();
            let mut cur: Option<(i64, f64, usize, f64)> = None;
            for (t, v) in pts {
                let b = (t - from_ms) / slice.max(1);
                match cur.as_mut() {
                    Some((cb, sum, n, mx)) if *cb == b => {
                        *sum += v;
                        *n += 1;
                        *mx = mx.max(v);
                    }
                    _ => {
                        if let Some((cb, sum, n, mx)) = cur.take() {
                            out.push((
                                from_ms + cb * slice + slice / 2,
                                if keep_max { mx } else { sum / n as f64 },
                            ));
                        }
                        cur = Some((b, v, 1, v));
                    }
                }
            }
            if let Some((cb, sum, n, mx)) = cur {
                out.push((
                    from_ms + cb * slice + slice / 2,
                    if keep_max { mx } else { sum / n as f64 },
                ));
            }
            (k, out)
        })
        .collect()
}

/// The demo station: synthetic history, and a live reading every five
/// seconds.
fn run_demo(h: Handle, ctx: egui::Context, rx: Receiver<Cmd>) {
    let days = demo::days();
    // A month of readings for the chart on Now to look back over; the last
    // day of it is the live samples.
    let month = demo::samples_for(now_ms(), 30);
    let day_ago = now_ms() - DAY_MS;
    let samples: Samples = month
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                v.iter().copied().filter(|p| p.0 >= day_ago).collect(),
            )
        })
        .collect();
    {
        let mut s = h.lock();
        s.status.day_count = days.len();
        s.status.last_sync = Some(now_ms() - 600_000);
        s.days = Arc::new(days);
        s.days_version += 1;
        s.samples = Arc::new(samples.clone());
        s.log_summary = Arc::new(LogSummary {
            indoor: match (month.get("intemp"), month.get("temp")) {
                (Some(i), Some(o)) => isobar_core::indoor::indoor_summary(i, o),
                _ => None,
            },
            ..Default::default()
        });
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
            Ok(Cmd::Detail {
                key,
                metrics,
                from_ms,
                to_ms,
            }) => {
                let series: Samples = month
                    .iter()
                    .filter(|(k, _)| metrics.contains(k))
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            v.iter()
                                .copied()
                                .filter(|p| p.0 >= from_ms && p.0 <= to_ms)
                                .collect(),
                        )
                    })
                    .collect();
                let series = thin(series, from_ms, to_ms);
                h.lock().detail = Some(Arc::new(Detail {
                    key,
                    series,
                    at: Instant::now(),
                }));
                ctx.request_repaint();
            }
            Ok(Cmd::ImportTankLog(_)) | Ok(Cmd::ClearTankLog) => {
                h.lock().tank_outcome = Some(Outcome {
                    ok: false,
                    text: "The demo station can't keep a tank record. Open Isobar normally to import yours.".into(),
                });
                ctx.request_repaint();
            }
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
