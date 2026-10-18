//! Settings: station address and login, a dayfile for offline use, history
//! tools, alerts and theme.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Layout, Margin, Ui};
use isobar_core::{LiveReading, MxClient};

use crate::app::Snapshot;
use crate::format::{self, num};
use crate::settings::{Alerts, Settings, ThemeChoice};
use crate::theme::{self, Palette};
use crate::ui::{self, Kind};
use crate::worker::{Cmd, Handle};

enum Test {
    Running,
    Done(Box<Result<LiveReading, String>>),
}

/// What's in the form, as typed.
#[derive(Clone, PartialEq)]
struct Draft {
    name: String,
    url: String,
    username: String,
    password: String,
    poll: String,
    dayfile: String,
    alerts_on: bool,
    gust: String,
    frost: String,
    heat: String,
    rain_start: bool,
    theme: ThemeChoice,
    clock_24h: bool,
}

fn opt_num(v: Option<f64>) -> String {
    v.map(|x| format!("{x}")).unwrap_or_default()
}

impl Draft {
    fn from(s: &Settings) -> Draft {
        Draft {
            name: s.station_name.clone(),
            url: s.station_url.clone(),
            username: s.username.clone().unwrap_or_default(),
            password: s.password.clone().unwrap_or_default(),
            poll: s.poll_seconds.to_string(),
            dayfile: s.dayfile_path.clone().unwrap_or_default(),
            alerts_on: s.alerts.enabled,
            gust: opt_num(s.alerts.gust_above),
            frost: opt_num(s.alerts.frost_below),
            heat: opt_num(s.alerts.heat_above),
            rain_start: s.alerts.rain_start,
            theme: s.theme,
            clock_24h: s.clock_24h,
        }
    }

    /// The settings this draft describes, on top of `base` for anything the
    /// form doesn't show, or what's wrong with it.
    fn to_settings(&self, base: &Settings) -> Result<Settings, String> {
        let alert = |label: &str, s: &str| -> Result<Option<f64>, String> {
            let t = s.trim().replace(',', ".");
            if t.is_empty() {
                return Ok(None);
            }
            t.parse::<f64>().map(Some).map_err(|_| {
                format!("{label} needs a number, or leave it empty to turn that alert off.")
            })
        };
        let poll: u64 = self
            .poll
            .trim()
            .parse()
            .map_err(|_| "Check every needs a whole number of seconds.".to_string())?;
        let opt = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_string());
        Ok(Settings {
            station_name: if self.name.trim().is_empty() {
                "My station".into()
            } else {
                self.name.trim().into()
            },
            station_url: self.url.trim().into(),
            username: opt(&self.username),
            password: (!self.password.is_empty()).then(|| self.password.clone()),
            dayfile_path: opt(&self.dayfile),
            poll_seconds: poll.clamp(3, 600),
            theme: self.theme,
            clock_24h: self.clock_24h,
            rail_pinned: base.rail_pinned,
            alerts: Alerts {
                enabled: self.alerts_on,
                gust_above: alert("Gust at or above", &self.gust)?,
                frost_below: alert("Temperature at or below", &self.frost)?,
                heat_above: alert("Temperature at or above", &self.heat)?,
                rain_start: self.rain_start,
            },
        })
    }
}

pub struct SettingsView {
    draft: Draft,
    saved: Draft,
    test: Arc<Mutex<Option<Test>>>,
    message: Option<(bool, String, Instant)>,
    confirm_clear: bool,
    busy_since: Option<Instant>,
}

impl SettingsView {
    pub fn new(s: &Settings) -> Self {
        SettingsView {
            draft: Draft::from(s),
            saved: Draft::from(s),
            test: Arc::new(Mutex::new(None)),
            message: None,
            confirm_clear: false,
            busy_since: None,
        }
    }

    /// Called after the app has stored the settings.
    pub fn saved(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.saved = self.draft.clone();
                self.message = Some((true, "Saved".into(), Instant::now()));
            }
            Err(e) => self.message = Some((false, format!("Couldn't save: {e}"), Instant::now())),
        }
    }

    /// The bar along the bottom with the save button. Returns settings to
    /// store when it's pressed.
    pub fn save_bar(&mut self, ui: &mut Ui, current: &Settings) -> Option<Settings> {
        let p = Palette::of(ui.ctx());
        let mut out = None;
        egui::Panel::bottom("save-bar")
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(p.paper)
                    .stroke(egui::Stroke::new(1.0, p.grid))
                    .inner_margin(Margin::symmetric(32, 12)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 16.0;
                    let dirty = self.draft != self.saved;
                    if ui::button(ui, "Save settings", Kind::Primary).clicked() {
                        match self.draft.to_settings(current) {
                            Ok(s) => out = Some(s),
                            Err(e) => self.message = Some((false, e, Instant::now())),
                        }
                    }
                    if let Some((ok, text, at)) = &self.message {
                        if !*ok || at.elapsed() < Duration::from_secs(3) {
                            ui::text(
                                ui,
                                text,
                                theme::medium(14.0),
                                if *ok { p.ok } else { p.heat },
                            );
                        } else {
                            self.message = None;
                        }
                    } else if dirty {
                        ui::text(ui, "Unsaved changes", theme::body(14.0), p.ink2);
                    }
                });
            });
        out
    }

    /// Returns true if the person asked to try the demo station.
    pub fn ui(&mut self, ui: &mut Ui, snap: &Snapshot, worker: &Handle) -> bool {
        let mut demo = false;
        let p = Palette::of(ui.ctx());
        ui::h1(ui, "Settings");
        let width = ui.available_width().min(760.0);
        ui.vertical(|ui| {
            ui.set_max_width(width);
            ui.spacing_mut().item_spacing.y = 24.0;
            let first_run = !snap.demo && self.saved.url.trim().is_empty() && self.saved.dayfile.trim().is_empty() && snap.days.is_empty();
            if first_run {
                ui::notice(ui, "Welcome to Isobar. Enter your Cumulus MX address below, test it, then save. To look around first, try the demo station: it has eight years of made-up weather.");
                ui.add_space(-12.0);
                demo = ui::button(ui, "Try the demo station", Kind::Secondary).clicked();
            }
            if snap.demo {
                ui::notice(ui, "You're looking at the demo station. To connect your own, close this window and open Isobar normally. Changes here last until you close it.");
            }
            self.station(ui, &p);
            self.history(ui, snap, worker, &p);
            self.alerts(ui, &p);
            ui::panel(ui, |ui| {
                ui::h2(ui, "Appearance");
                ui::field(ui, "Theme", None, |ui| {
                    let opts: Vec<(ThemeChoice, String)> = ThemeChoice::ALL.iter().map(|t| (*t, t.label().to_string())).collect();
                    if ui::select(ui, "theme", &mut self.draft.theme, &opts, 180.0) {
                        crate::app::apply_theme(ui.ctx(), self.draft.theme);
                    }
                });
                ui::field(ui, "Times", None, |ui| {
                    let opts = [(true, "24-hour (16:52)".to_string()), (false, "12-hour (4:52 PM)".to_string())];
                    if ui::select(ui, "clock", &mut self.draft.clock_24h, &opts, 180.0) {
                        crate::format::set_24h(self.draft.clock_24h);
                    }
                });
            });
        });
        demo
    }

    fn station(&mut self, ui: &mut Ui, p: &Palette) {
        ui::panel(ui, |ui| {
            ui::h2(ui, "Station");
            let w = ui.available_width();
            ui::field(
                ui,
                "Name",
                Some("Shown at the top of the Now screen and in alerts."),
                |ui| {
                    ui::text_input(ui, &mut self.draft.name, "My station", w);
                },
            );
            ui::field(
                ui,
                "Cumulus MX address",
                Some("The address of the MX interface, usually port 8998. Isobar reads live conditions and daily history from it."),
                |ui| {
                    ui::text_input(ui, &mut self.draft.url, "http://192.168.1.20:8998", w);
                },
            );
            ui::columns(ui, 2, 16.0, false, |i, ui| {
                let half = ui.available_width();
                if i == 0 {
                    ui::field(ui, "Username", None, |ui| {
                        ui::text_input(ui, &mut self.draft.username, "", half)
                    });
                } else {
                    ui::field(ui, "Password", None, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.draft.password)
                                .password(true)
                                .font(theme::medium(14.0))
                                .desired_width(half)
                                .margin(Margin::symmetric(10, 8)),
                        )
                    });
                }
            });
            ui.add_space(-6.0);
            ui::para(ui, "Only needed if you've secured the MX interface. Stored unencrypted in Isobar's settings file.", theme::body(13.0), p.ink2, 560.0);
            ui::field(ui, "Check every (seconds)", None, |ui| {
                ui::text_input(ui, &mut self.draft.poll, "10", 120.0)
            });

            ui.horizontal(|ui| {
                let running = matches!(*self.test.lock().unwrap(), Some(Test::Running));
                if ui::button(ui, "Test connection", Kind::Secondary).clicked() && !running {
                    self.start_test(ui.ctx());
                }
            });
            let state = self.test.lock().unwrap();
            match state.as_ref().map(|t| match t {
                Test::Running => None,
                Test::Done(r) => Some(r.as_ref()),
            }) {
                None => {}
                Some(None) => {
                    ui::text(ui, "Connecting…", theme::body(14.0), p.ink2);
                }
                Some(Some(Ok(r))) => {
                    ui::text(
                        ui,
                        format!(
                            "Connected. {}{}, {} {}, {}% humidity.",
                            num(r.temp, 1),
                            r.units.temp,
                            num(r.pressure, 1),
                            r.units.press,
                            num(r.humidity, 0)
                        ),
                        theme::medium(14.0),
                        p.ok,
                    );
                }
                Some(Some(Err(e))) => {
                    ui::para(
                        ui,
                        format!(
                            "{}. Check the address is your MX interface (port 8998 by default) and that this computer can reach it.",
                            format::capitalise(e)
                        ),
                        theme::body(14.0),
                        p.heat,
                        700.0,
                    );
                }
            }
        });
    }

    fn start_test(&mut self, ctx: &egui::Context) {
        let url = self.draft.url.trim().to_string();
        if url.is_empty() {
            *self.test.lock().unwrap() = Some(Test::Done(Box::new(Err(
                "Enter your station's address first".into(),
            ))));
            return;
        }
        let user = (!self.draft.username.trim().is_empty())
            .then(|| self.draft.username.trim().to_string());
        let pass = (!self.draft.password.is_empty()).then(|| self.draft.password.clone());
        *self.test.lock().unwrap() = Some(Test::Running);
        let slot = self.test.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = MxClient::new(&url, user, pass)
                .and_then(|c| c.current())
                .map_err(|e| e.to_string());
            *slot.lock().unwrap() = Some(Test::Done(Box::new(result)));
            ctx.request_repaint();
        });
    }

    fn history(&mut self, ui: &mut Ui, snap: &Snapshot, worker: &Handle, p: &Palette) {
        let outcome = worker.lock().history_outcome.clone();
        if outcome.is_some() {
            self.busy_since = None;
        }
        ui::panel(ui, |ui| {
            ui::h2(ui, "History");
            let s = &snap.status;
            let line = match s.sync_progress {
                Some((d, t)) if s.syncing => format!("Downloading {d} of {t} days…"),
                _ => format!(
                    "{} days stored. Last updated {}.",
                    s.day_count,
                    format::ago(s.last_sync, snap.now_ms)
                ),
            };
            ui::muted(ui, line);
            ui::field(
                ui,
                "Dayfile on this computer",
                Some("Use this instead of a station address to read dayfile.txt directly, for example from a shared folder. Isobar re-reads it when it changes."),
                |ui| {
                    ui.horizontal(|ui| {
                        let w = ui.available_width() - 130.0;
                        ui::text_input(ui, &mut self.draft.dayfile, "Only needed without a station address", w);
                        if ui::button(ui, "Choose file", Kind::Secondary).clicked() {
                            if let Some(path) = pick_dayfile() {
                                self.draft.dayfile = path.display().to_string();
                            }
                        }
                    });
                },
            );
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                if ui::button(ui, "Download history again", Kind::Secondary).clicked() {
                    worker.lock().history_outcome = None;
                    self.busy_since = Some(Instant::now());
                    worker.send(Cmd::Sync { force: true });
                }
                if ui::button(ui, "Import a dayfile", Kind::Secondary).clicked() {
                    if let Some(path) = pick_dayfile() {
                        worker.lock().history_outcome = None;
                        self.busy_since = Some(Instant::now());
                        worker.send(Cmd::Import(path));
                    }
                }
                if ui::button(ui, "Delete local history", Kind::Danger).clicked() {
                    self.confirm_clear = true;
                }
            });
            if self.confirm_clear {
                ui::para(
                    ui,
                    "Delete all daily history stored by Isobar on this computer? Your station's own files are not touched, and the history downloads again from the station.",
                    theme::body(14.0),
                    p.ink,
                    640.0,
                );
                ui.horizontal(|ui| {
                    if ui::button(ui, "Delete it", Kind::Danger).clicked() {
                        worker.lock().history_outcome = None;
                        worker.send(Cmd::ClearHistory);
                        self.confirm_clear = false;
                    }
                    if ui::button(ui, "Keep it", Kind::Secondary).clicked() {
                        self.confirm_clear = false;
                    }
                });
            }
            if self.busy_since.is_some() {
                ui::text(ui, "Working…", theme::body(14.0), p.ink2);
            } else if let Some(o) = outcome {
                ui::para(
                    ui,
                    o.text,
                    theme::medium(14.0),
                    if o.ok { p.ok } else { p.heat },
                    700.0,
                );
            }
        });
    }

    fn alerts(&mut self, ui: &mut Ui, p: &Palette) {
        ui::panel(ui, |ui| {
            ui::h2(ui, "Alerts");
            ui.checkbox(
                &mut self.draft.alerts_on,
                egui::RichText::new("Show desktop notifications").font(theme::body(15.0)),
            );
            ui::columns(ui, 3, 16.0, false, |i, ui| {
                let w = ui.available_width();
                let (label, value, hint) = match i {
                    0 => ("Gust at or above", &mut self.draft.gust, "Off"),
                    1 => ("Temperature at or below", &mut self.draft.frost, "Off"),
                    _ => ("Temperature at or above", &mut self.draft.heat, "Off"),
                };
                ui::field(ui, label, None, |ui| ui::text_input(ui, value, hint, w));
            });
            ui::para(
                ui,
                "In your station's units. Leave a box empty to turn that alert off. Each alerts at most once a day, gusts at most every three hours.",
                theme::body(13.0),
                p.ink2,
                600.0,
            );
            ui.checkbox(
                &mut self.draft.rain_start,
                egui::RichText::new("Rain starting after three dry hours").font(theme::body(15.0)),
            );
            ui.with_layout(Layout::top_down(Align::Min), |_| {});
        });
    }
}

fn pick_dayfile() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .set_title("Choose a Cumulus dayfile")
        .add_filter("Cumulus dayfile", &["txt", "csv"])
        .add_filter("All files", &["*"])
        .pick_file()
}
