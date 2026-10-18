//! Health: whether it's the station or the weather. The link to MX, MX's own
//! flags and alarms, the rain gauge, and gaps in the daily history and the
//! recent readings.

use chrono::{Local, NaiveDate, TimeZone};
use eframe::egui::{self, pos2, vec2, Color32, Sense, Ui};
use isobar_core::health::{self as hl, MissingDays};
use isobar_core::Alarm;

use crate::app::Snapshot;
use crate::format::{self, nice_date, num};
use crate::settings::Settings;
use crate::theme::{self, Palette};
use crate::ui::{self, Kind};
use crate::worker::{Cmd, Handle};

/// Alarms that are news rather than a fault.
fn informational(a: &Alarm) -> bool {
    matches!(
        a.id.as_str(),
        "AlarmUpgrade" | "AlarmFirmware" | "AlarmRecord" | "AlarmNewRecord"
    )
}

/// How many things need looking at, for the dot beside Health in the rail.
pub fn issues(snap: &Snapshot, settings: &Settings) -> usize {
    let mut n = 0;
    if (snap.demo || settings.has_station())
        && !snap.status.connected
        && snap.status.last_ok.is_some()
    {
        n += 1;
    }
    if let Some(r) = &snap.live {
        n += (r.data_stopped == Some(true)) as usize;
        n += r
            .alarms
            .iter()
            .filter(|a| a.triggered && !informational(a))
            .count();
    }
    n
}

#[derive(Clone, Copy)]
enum Level {
    Ok,
    Note,
    Warn,
    Bad,
}

struct Cache {
    version: u64,
    missing: Vec<MissingDays>,
    incomplete: Vec<NaiveDate>,
}

#[derive(Default)]
pub struct Health {
    cache: Option<Cache>,
    show_all_gaps: bool,
}

/// A status line: a coloured dot, a label, then the words.
fn row(ui: &mut Ui, p: &Palette, level: Level, label: &str, text: &str) -> egui::Response {
    let color = match level {
        Level::Ok => p.ok,
        Level::Note => p.ink2,
        Level::Warn => p.warn,
        Level::Bad => p.heat,
    };
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let (r, _) = ui.allocate_exact_size(vec2(10.0, 20.0), Sense::hover());
        ui.painter()
            .circle_filled(pos2(r.center().x, r.top() + 10.0), 4.0, color);
        ui.allocate_ui(vec2(200.0, 20.0), |ui| {
            ui.set_min_width(200.0);
            ui::text(ui, label, theme::semi(14.0), p.ink);
        });
        ui::para(
            ui,
            text,
            theme::body(14.0),
            p.ink,
            (ui.available_width() - 8.0).max(200.0),
        );
    })
    .response
}

impl Health {
    pub fn ui(&mut self, ui: &mut Ui, snap: &Snapshot, settings: &Settings, worker: &Handle) {
        let p = Palette::of(ui.ctx());
        ui::h1(ui, "Health");
        ui::para(
            ui,
            "Whether it's the station or the weather: the link to Cumulus MX, its alarms, the rain gauge, and any gaps in what Isobar has recorded.",
            theme::body(14.0),
            p.ink2,
            900.0,
        );
        if self
            .cache
            .as_ref()
            .is_none_or(|c| c.version != snap.days_version)
        {
            self.cache = Some(Cache {
                version: snap.days_version,
                missing: hl::missing_days(&snap.days),
                incomplete: hl::incomplete_days(&snap.days),
            });
        }
        let wide = ui.available_width() >= 1100.0;
        if wide {
            ui::split_with(
                ui,
                0.5,
                24.0,
                self,
                |ui, s| s.station(ui, snap, settings),
                |ui, s| s.alarms(ui, snap),
            );
        } else {
            self.station(ui, snap, settings);
            self.alarms(ui, snap);
        }
        self.history(ui, snap, worker, &p);
        self.recent(ui, snap, &p);
    }

    fn station(&mut self, ui: &mut Ui, snap: &Snapshot, settings: &Settings) {
        let p = Palette::of(ui.ctx());
        ui::panel(ui, |ui| {
            ui::h2(ui, "Station");
            let s = &snap.status;
            if !snap.demo && !settings.has_station() {
                let text = match settings.dayfile() {
                    Some(f) => format!("No station address; reading the dayfile at {f}. Live readings, alarms and the rain gauge need the MX interface."),
                    None => "No station set up yet. Add your Cumulus MX address in Settings.".into(),
                };
                row(ui, &p, Level::Note, "Connection", &text);
                return;
            }
            if s.connected {
                row(
                    ui,
                    &p,
                    Level::Ok,
                    "Connection",
                    &format!("Live, last reading {}", format::ago(s.last_ok, snap.now_ms)),
                );
            } else {
                let why = s
                    .last_error
                    .clone()
                    .unwrap_or_else(|| "No answer yet".into());
                let since = s.last_ok.map_or(String::new(), |t| {
                    format!(" Last reading {}.", format::ago(Some(t), snap.now_ms))
                });
                row(
                    ui,
                    &p,
                    Level::Bad,
                    "Connection",
                    &format!("Can't reach MX: {}.{since}", format::capitalise(&why)),
                );
            }
            let Some(r) = &snap.live else { return };
            match r.data_stopped {
                Some(false) => row(ui, &p, Level::Ok, "Data from the station", "Arriving"),
                Some(true) => row(ui, &p, Level::Bad, "Data from the station", "Stopped. MX is running but isn't getting readings from the station: check its console, cable or radio link."),
                None => row(ui, &p, Level::Note, "Data from the station", "MX doesn't report this"),
            };
            if let Some(t) = &r.station_time {
                let when = match &r.station_date {
                    Some(d) => format!("{d} at {}", format::clock(t)),
                    None => format::clock(t),
                };
                row(ui, &p, Level::Note, "Station's last reading", &when);
            }
            let version = match (&r.mx_version, &r.mx_build) {
                (Some(v), Some(b)) => format!("{v} (build {b})"),
                (Some(v), None) => v.clone(),
                _ => "Not reported".into(),
            };
            row(ui, &p, Level::Note, "Cumulus MX", &version);
            row(
                ui,
                &p,
                Level::Note,
                "Isobar checks",
                &format!("Every {} seconds", settings.poll_seconds),
            );
        });
    }

    fn alarms(&mut self, ui: &mut Ui, snap: &Snapshot) {
        let p = Palette::of(ui.ctx());
        ui::panel(ui, |ui| {
            ui::h2(ui, "MX alarms and the rain gauge");
            match snap.live.as_ref().map(|r| &r.alarms) {
                None => {
                    ui::muted(ui, "Waiting for a reading from MX.");
                }
                Some(list) if list.is_empty() => {
                    row(
                        ui,
                        &p,
                        Level::Note,
                        "Alarms",
                        "MX didn't send its alarm list.",
                    );
                }
                Some(list) => {
                    let set: Vec<&Alarm> = list.iter().filter(|a| a.triggered).collect();
                    if set.is_empty() {
                        row(ui, &p, Level::Ok, "Alarms", "None set off");
                    }
                    for a in &set {
                        let level = if informational(a) {
                            Level::Note
                        } else {
                            Level::Warn
                        };
                        let then = if informational(a) {
                            "Clear it in MX once you've seen it."
                        } else {
                            "Clear it in MX once it's dealt with."
                        };
                        row(ui, &p, level, "Set off", &format!("{}. {then}", a.label()));
                    }
                    let quiet: Vec<String> = list
                        .iter()
                        .filter(|a| !a.triggered)
                        .map(Alarm::label)
                        .collect();
                    if !quiet.is_empty() {
                        ui::para(
                            ui,
                            format!("Not set off: {}.", quiet.join(", ")),
                            theme::body(13.0),
                            p.ink2,
                            ui.available_width(),
                        );
                    }
                }
            }
            let Some(r) = &snap.live else { return };
            match r
                .last_rain_tip
                .as_deref()
                .and_then(|t| format::station_moment(t, snap.now_ms))
            {
                Some((when, ago)) => {
                    row(
                        ui,
                        &p,
                        Level::Note,
                        "Rain gauge last tipped",
                        &format!("{when}, {ago}"),
                    );
                }
                None => {
                    row(
                        ui,
                        &p,
                        Level::Note,
                        "Rain gauge last tipped",
                        r.last_rain_tip.as_deref().unwrap_or("Not reported"),
                    );
                }
            }
            ui::small(ui, "If it rained and the gauge didn't tip, check the funnel for leaves, dirt or spiders.", p.ink2);
        });
    }

    fn history(&mut self, ui: &mut Ui, snap: &Snapshot, worker: &Handle, p: &Palette) {
        let Some(c) = self.cache.as_ref() else { return };
        let days = &snap.days;
        let mut show_all = self.show_all_gaps;
        ui::panel(ui, |ui| {
            ui::h2(ui, "Daily history");
            let (Some(first), Some(last)) = (days.first(), days.last()) else {
                row(ui, p, Level::Note, "Days stored", "None yet");
                return;
            };
            row(
                ui,
                p,
                Level::Note,
                "Days stored",
                &format!(
                    "{} days, {} to {}. Last updated {}.",
                    days.len(),
                    nice_date(first.date),
                    nice_date(last.date),
                    format::ago(snap.status.last_sync, snap.now_ms)
                ),
            );
            // MX writes a day's line after its rollover, so yesterday is the
            // newest to expect (two days back allows for a 9am rollover).
            let today = Local::now().date_naive();
            let behind = (today - last.date).num_days() - 1;
            if behind > 1 && !snap.demo {
                row(ui, p, Level::Warn, "Up to date", &format!("The newest day is {}, {behind} days behind. Download the history again in Settings, or check MX is writing its dayfile.", nice_date(last.date)));
            } else {
                row(
                    ui,
                    p,
                    Level::Ok,
                    "Up to date",
                    &format!("Newest day {}", nice_date(last.date)),
                );
            }
            let missing: i64 = c.missing.iter().map(|g| g.len()).sum();
            if c.missing.is_empty() {
                row(ui, p, Level::Ok, "Missing days", "None: every day is there");
            } else {
                row(
                    ui,
                    p,
                    Level::Warn,
                    "Missing days",
                    &format!(
                        "{missing} {} in {}",
                        if missing == 1 { "day" } else { "days" },
                        if c.missing.len() == 1 {
                            "one gap:".to_string()
                        } else {
                            format!("{} gaps, most recent first:", c.missing.len())
                        }
                    ),
                );
                let shown = if show_all { c.missing.len() } else { 8 };
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(230.0);
                    ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
                    for g in c.missing.iter().rev().take(shown) {
                        let text = if g.len() == 1 {
                            nice_date(g.from)
                        } else {
                            format!(
                                "{} to {} ({} days)",
                                nice_date(g.from),
                                nice_date(g.to),
                                g.len()
                            )
                        };
                        tag(ui, p, &text);
                    }
                });
                if c.missing.len() > 8 {
                    ui.horizontal(|ui| {
                        ui.add_space(230.0);
                        let label = if show_all {
                            "Show fewer".to_string()
                        } else {
                            format!("Show all {}", c.missing.len())
                        };
                        if ui::button(ui, &label, Kind::Secondary).clicked() {
                            show_all = !show_all;
                        }
                    });
                }
            }
            if c.incomplete.is_empty() {
                row(
                    ui,
                    p,
                    Level::Ok,
                    "Incomplete days",
                    "None: every day has its temperatures and rain",
                );
            } else {
                let recent: Vec<String> = c
                    .incomplete
                    .iter()
                    .rev()
                    .take(12)
                    .map(|d| nice_date(*d))
                    .collect();
                row(
                    ui,
                    p,
                    Level::Note,
                    "Incomplete days",
                    &format!(
                        "{} {} without a temperature or rain figure",
                        c.incomplete.len(),
                        if c.incomplete.len() == 1 {
                            "day"
                        } else {
                            "days"
                        },
                    ),
                )
                .on_hover_text(format!("Most recent: {}", recent.join(", ")));
            }
            if !c.missing.is_empty() && !snap.demo {
                ui.horizontal(|ui| {
                    ui.add_space(230.0);
                    if ui::button(ui, "Download history again", Kind::Secondary).clicked() {
                        worker.lock().history_outcome = None;
                        worker.send(Cmd::Sync { force: true });
                    }
                });
                ui::small(ui, "Gaps that are also in MX's own dayfile stay after a fresh download; MX can only send what it recorded.", p.ink2);
            }
        });
        self.show_all_gaps = show_all;
    }

    fn recent(&self, ui: &mut Ui, snap: &Snapshot, p: &Palette) {
        ui::panel(ui, |ui| {
            ui::h2(ui, "Recent readings");
            let day_ms = 24 * 3_600_000;
            let since = snap.now_ms - day_ms;
            let ts: Vec<i64> = snap
                .samples
                .get("temp")
                .map(|v| v.iter().map(|p| p.0).collect())
                .unwrap_or_default();
            let n = ts.iter().filter(|t| **t >= since).count();
            if n == 0 {
                row(
                    ui,
                    p,
                    Level::Note,
                    "Last 24 hours",
                    "No readings recorded yet",
                );
            } else {
                let gaps = hl::reading_gaps(&ts, since, snap.now_ms, 20 * 60_000);
                let lost: i64 = gaps.iter().map(|g| g.1 - g.0).sum();
                let cover = (100.0 * (1.0 - lost as f64 / day_ms as f64)).clamp(0.0, 100.0);
                row(
                    ui,
                    p,
                    if gaps.is_empty() {
                        Level::Ok
                    } else {
                        Level::Warn
                    },
                    "Last 24 hours",
                    &format!("{n} readings, covering {}% of the day", num(Some(cover), 0)),
                );
                if !gaps.is_empty() {
                    let fmt = |ms: i64| {
                        Local
                            .timestamp_millis_opt(ms)
                            .single()
                            .map(|t| format::clock(&t.format("%H:%M").to_string()))
                            .unwrap_or_default()
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(230.0);
                        ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
                        for g in gaps.iter().rev() {
                            let mins = (g.1 - g.0) / 60_000;
                            let len = if mins >= 90 {
                                format!("{} h", num(Some(mins as f64 / 60.0), 1))
                            } else {
                                format!("{mins} min")
                            };
                            tag(ui, p, &format!("{} to {} ({len})", fmt(g.0), fmt(g.1)));
                        }
                    });
                }
            }
            match &snap.status.backfill {
                Some(b) => {
                    let when = format::ago(Some(b.at), snap.now_ms);
                    let (level, text) = match (b.points, b.failed.len()) {
                        (0, f) if f > 0 => (Level::Warn, format!("MX didn't send its recent detail ({when}), so gaps stay until Isobar records them itself.")),
                        (n, 0) => (Level::Ok, format!("{n} readings from MX's graph data, {when}")),
                        (n, f) => (Level::Warn, format!("{n} readings from MX's graph data, {when}; {f} of its files didn't come")),
                    };
                    let resp = row(ui, p, level, "Gap filling", &text);
                    if !b.failed.is_empty() {
                        let why: Vec<String> = b
                            .failed
                            .iter()
                            .map(|(f, e)| format!("{f}.json: {e}"))
                            .collect();
                        resp.on_hover_text(why.join("\n"));
                    }
                }
                None if snap.demo => {
                    row(
                        ui,
                        p,
                        Level::Note,
                        "Gap filling",
                        "Not needed for the demo station",
                    );
                }
                None => {
                    row(ui, p, Level::Note, "Gap filling", "Not run yet. Isobar fills gaps from MX's graph data when it connects, and after any break over 15 minutes.");
                }
            }
        });
    }
}

/// A small outlined label, for a date range in a list.
fn tag(ui: &mut Ui, p: &Palette, text: &str) {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), theme::body(13.0), p.ink);
    let (r, _) = ui.allocate_exact_size(galley.size() + vec2(16.0, 8.0), Sense::hover());
    ui.painter().rect(
        r,
        4,
        Color32::TRANSPARENT,
        egui::Stroke::new(1.0, p.grid),
        egui::StrokeKind::Inside,
    );
    ui.painter().galley(r.min + vec2(8.0, 4.0), galley, p.ink);
}
