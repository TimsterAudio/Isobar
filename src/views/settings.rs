//! Settings: station address and login, a dayfile for offline use, history
//! tools, alerts and theme.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{Datelike, NaiveDate};
use eframe::egui::{self, Align, Layout, Margin, Ui};
use isobar_core::tanks::{KnownLevel, Tank, Tanks, MAX_TANKS};
use isobar_core::{LiveReading, MxClient};

use crate::app::Snapshot;
use crate::format::{self, num};
use crate::settings::{Alerts, NowCard, Settings, ThemeChoice};
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
    latitude: String,
    dayfile: String,
    alerts_on: bool,
    gust: String,
    frost: String,
    heat: String,
    rain_start: bool,
    stopped: bool,
    theme: ThemeChoice,
    clock_24h: bool,
    tanks: TankDraft,
    /// Cards turned off on Now. Their order is set by dragging on Now.
    hidden_cards: Vec<NowCard>,
    /// Put the cards back in the usual order on save.
    reset_order: bool,
}

/// The rainwater tank fields, as typed.
#[derive(Clone, PartialEq)]
struct TankDraft {
    roof: String,
    /// How many tanks, 1 to 4, and each one's litres and inside diameter.
    count: usize,
    litres: [String; MAX_TANKS],
    diameter: [String; MAX_TANKS],
    use_per_day: String,
    keep_above: String,
    /// "full", litres, or a depth such as "1350 mm".
    known: String,
    known_date: NaiveDate,
}

impl TankDraft {
    fn from(t: Option<&Tanks>) -> TankDraft {
        let blank = |v: f64| {
            if v > 0.0 {
                format!("{v}")
            } else {
                String::new()
            }
        };
        let today = chrono::Local::now().date_naive();
        match t {
            None => TankDraft {
                roof: String::new(),
                count: 1,
                litres: Default::default(),
                diameter: Default::default(),
                use_per_day: String::new(),
                keep_above: String::new(),
                known: String::new(),
                known_date: today,
            },
            Some(t) => TankDraft {
                roof: blank(t.roof_area),
                count: t.tanks.len().clamp(1, MAX_TANKS),
                litres: std::array::from_fn(|i| {
                    t.tanks.get(i).map_or(String::new(), |k| blank(k.litres))
                }),
                diameter: std::array::from_fn(|i| {
                    t.tanks
                        .get(i)
                        .map_or(String::new(), |k| opt_num(k.diameter_mm))
                }),
                use_per_day: blank(t.use_per_day),
                keep_above: blank(t.keep_above),
                known: t.known.map_or(String::new(), |k| {
                    if (k.litres - t.capacity()).abs() < 0.5 {
                        "full".into()
                    } else {
                        format!("{}", k.litres.round())
                    }
                }),
                known_date: t.known.map_or(today, |k| k.date),
            },
        }
    }

    /// The tanks this describes, none if it's all empty, or what's wrong.
    fn to_tanks(&self) -> Result<Option<Tanks>, String> {
        let num = |label: &str, s: &str| -> Result<Option<f64>, String> {
            let t = s.trim().replace([',', ' '], "");
            if t.is_empty() {
                return Ok(None);
            }
            t.parse::<f64>()
                .ok()
                .filter(|v| *v >= 0.0)
                .map(Some)
                .ok_or_else(|| format!("{label} needs a number."))
        };
        let roof = num("Roof area", &self.roof)?;
        let n = self.count.clamp(1, MAX_TANKS);
        let mut tanks = Vec::new();
        for i in 0..n {
            let litres = num(&format!("Tank {} litres", i + 1), &self.litres[i])?;
            let diameter = num(&format!("Tank {} diameter", i + 1), &self.diameter[i])?;
            tanks.push((litres, diameter.filter(|v| *v > 0.0)));
        }
        if roof.is_none() && tanks.iter().all(|t| t.0.is_none()) {
            return Ok(None);
        }
        let Some(roof) = roof.filter(|v| *v > 0.0) else {
            return Err(
                "For the tanks, fill in the roof area, or leave all the tank boxes empty.".into(),
            );
        };
        let tanks = tanks
            .into_iter()
            .enumerate()
            .map(
                |(i, (litres, diameter_mm))| match litres.filter(|v| *v > 0.0) {
                    Some(litres) => Ok(Tank {
                        litres,
                        diameter_mm,
                    }),
                    None => Err(format!("Fill in how many litres tank {} holds.", i + 1)),
                },
            )
            .collect::<Result<Vec<_>, String>>()?;
        let mut t = Tanks {
            roof_area: roof,
            tanks,
            use_per_day: num("Use a day", &self.use_per_day)?.unwrap_or(0.0),
            keep_above: num("Keep above", &self.keep_above)?.unwrap_or(0.0),
            known: None,
        };
        let k = self.known.trim().to_ascii_lowercase();
        if !k.is_empty() {
            let litres = if k == "full" {
                t.capacity()
            } else if let Some(depth) = k.strip_suffix("mm") {
                let d = num("Known level", depth)?
                    .ok_or("Known level needs a depth, such as 1350 mm.")?;
                t.litres_at_depth(d)
                    .ok_or("To give the level as a depth, fill in every tank's diameter.")?
            } else {
                num("Known level", &k)?
                    .ok_or("Known level needs litres, a depth such as 1350 mm, or \"full\".")?
            };
            t.known = Some(KnownLevel {
                date: self.known_date,
                litres: litres.min(t.capacity()),
            });
        }
        Ok(Some(t))
    }
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
            latitude: opt_num(s.latitude),
            dayfile: s.dayfile_path.clone().unwrap_or_default(),
            alerts_on: s.alerts.enabled,
            gust: opt_num(s.alerts.gust_above),
            frost: opt_num(s.alerts.frost_below),
            heat: opt_num(s.alerts.heat_above),
            rain_start: s.alerts.rain_start,
            stopped: s.alerts.stopped,
            theme: s.theme,
            clock_24h: s.clock_24h,
            tanks: TankDraft::from(s.tanks.as_ref()),
            hidden_cards: s
                .cards()
                .into_iter()
                .filter(|c| !c.1)
                .map(|c| c.0)
                .collect(),
            reset_order: false,
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
        let latitude = parse_latitude(&self.latitude)?;
        let tanks = self.tanks.to_tanks()?;
        let order: Vec<NowCard> = if self.reset_order {
            NowCard::ALL.to_vec()
        } else {
            base.cards().into_iter().map(|c| c.0).collect()
        };
        let cards: Vec<(NowCard, bool)> = order
            .into_iter()
            .map(|c| (c, !self.hidden_cards.contains(&c)))
            .collect();
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
            latitude,
            tanks,
            now_cards: Vec::new(),
            alerts: Alerts {
                enabled: self.alerts_on,
                gust_above: alert("Gust at or above", &self.gust)?,
                frost_below: alert("Temperature at or below", &self.frost)?,
                heat_above: alert("Temperature at or above", &self.heat)?,
                rain_start: self.rain_start,
                stopped: self.stopped,
            },
        })
        .map(|mut s| {
            s.set_cards(&cards);
            s
        })
    }
}

/// "-33.9", "33.9 S" or "33.9°S"; empty is none.
fn parse_latitude(s: &str) -> Result<Option<f64>, String> {
    let t = s.trim().replace(',', ".").replace('°', "");
    if t.is_empty() {
        return Ok(None);
    }
    let (num, sign) = match t.chars().last().map(|c| c.to_ascii_uppercase()) {
        Some('S') => (&t[..t.len() - 1], -1.0),
        Some('N') => (&t[..t.len() - 1], 1.0),
        _ => (t.as_str(), 1.0),
    };
    match num.trim().parse::<f64>() {
        Ok(v) if (-90.0..=90.0).contains(&v) => Ok(Some(v * sign)),
        _ => Err(
            "Latitude needs a number of degrees from -90 to 90, such as -33.9 or 33.9 S.".into(),
        ),
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
                // The order has been put back; don't do it again next save.
                self.draft.reset_order = false;
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
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        snap: &Snapshot,
        settings: &Settings,
        worker: &Handle,
    ) -> bool {
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
            self.tanks(ui, snap, worker, &p);
            self.alerts(ui, &p);
            self.now_cards(ui, settings, &p);
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
            ui::columns(ui, 2, 16.0, false, |i, ui| {
                if i == 0 {
                    ui::field(ui, "Check every (seconds)", None, |ui| {
                        ui::text_input(ui, &mut self.draft.poll, "10", 120.0)
                    });
                } else {
                    ui::field(ui, "Latitude", None, |ui| {
                        ui::text_input(ui, &mut self.draft.latitude, "Worked out", 120.0)
                    });
                }
            });
            ui.add_space(-6.0);
            ui::para(ui, "Latitude is in degrees, south negative: -33.9 or 33.9 S for Sydney. Seasons uses it for day length, and to estimate evaporation if the station doesn't record it. Leave it empty and Isobar works it out from the day's sunrise and sunset.", theme::body(13.0), p.ink2, 560.0);

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
            if let Some(b) = &s.backfill {
                let when = format::ago(Some(b.at), snap.now_ms);
                let text = match (b.points, b.failed.is_empty()) {
                    (0, false) => format!(
                        "MX didn't send its recent detail ({when}), so the 24-hour charts only show what Isobar recorded itself."
                    ),
                    (n, _) => format!("Recent detail: {n} readings from MX's graph data, {when}."),
                };
                let resp = ui::muted(ui, text);
                if !b.failed.is_empty() {
                    let why: Vec<String> = b
                        .failed
                        .iter()
                        .map(|(f, e)| format!("{f}.json: {e}"))
                        .collect();
                    resp.on_hover_text(why.join("\n"));
                }
            }
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

    fn tanks(&mut self, ui: &mut Ui, snap: &Snapshot, worker: &Handle, p: &Palette) {
        let outcome = worker.lock().tank_outcome.clone();
        let unsaved = self.draft.tanks != self.saved.tanks;
        ui::panel(ui, |ui| {
            ui::h2(ui, "Rainwater tanks");

            ui::para(ui, "For the Tanks page: how much the roof catches and how full the tanks are, worked out day by day from the rain and what the house uses. Leave these empty if you don't have tanks.", theme::body(14.0), p.ink2, 600.0);
            let t = &mut self.draft.tanks;
            {
                let mut fields = [
                    ("Roof area (m²)", &mut t.roof, "e.g. 385"),
                    ("Use a day (litres)", &mut t.use_per_day, "e.g. 290"),
                    ("Keep above (litres)", &mut t.keep_above, "e.g. 3000"),
                ]
                .map(Some);
                ui::columns(ui, 3, 16.0, false, |i, ui| {
                    let w = ui.available_width();
                    if let Some((label, value, hint)) = fields[i].take() {
                        ui::field(ui, label, None, |ui| ui::text_input(ui, value, hint, w));
                    }
                });
            }
            ui::field(ui, "Tanks", None, |ui| {
                let opts: Vec<(usize, String)> = (1..=MAX_TANKS)
                    .map(|n| {
                        (
                            n,
                            if n == 1 {
                                "1 tank".to_string()
                            } else {
                                format!("{n} tanks")
                            },
                        )
                    })
                    .collect();
                ui::select(ui, "tank-count", &mut t.count, &opts, 180.0)
            });
            for k in 0..t.count.clamp(1, MAX_TANKS) {
                let (litres, diameter) = (&mut t.litres[k], &mut t.diameter[k]);
                let mut fields = [
                    (format!("Tank {} litres", k + 1), litres, "e.g. 22500"),
                    (
                        format!("Tank {} inside diameter (mm)", k + 1),
                        diameter,
                        "Optional",
                    ),
                ]
                .map(Some);
                ui::columns(ui, 3, 16.0, false, |i, ui| {
                    let w = ui.available_width();
                    if let Some((label, value, hint)) = fields.get_mut(i).and_then(Option::take) {
                        ui::field(ui, &label, None, |ui| ui::text_input(ui, value, hint, w));
                    }
                });
            }
            ui.add_space(-6.0);
            ui::para(ui, "Isobar treats the tanks as linked, filling and emptying together. Keep above is the level to plan around, such as where the pump starts drawing air. The diameters are only needed to give the level as a depth of water.", theme::body(13.0), p.ink2, 600.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                ui.vertical(|ui| {
                    ui::field(ui, "Known level", None, |ui| {
                        ui::text_input(ui, &mut t.known, "e.g. full, 26400 or 1350 mm", 220.0)
                    });
                });
                ui.vertical(|ui| {
                    ui::field(ui, "At the end of", None, |ui| {
                        let y = chrono::Local::now().year();
                        ui::date_input(ui, "tank-known", &mut t.known_date, 2000..=y)
                    });
                });
            });
            ui.add_space(-6.0);
            ui::para(ui, "The level on a day you know it: \"full\" after the tanks overflowed, litres, or a depth you measured such as 1350 mm. Isobar works forward from it.", theme::body(13.0), p.ink2, 600.0);

            // Below the boxes, so appearing mid-typing doesn't move them.
            if unsaved {
                ui::notice(ui, "These tank details aren't saved yet. Press Save settings at the bottom to keep them.");
            }
            let log = &snap.tank_log;
            let line = match (log.first(), log.last()) {
                (Some(a), Some(b)) => format!(
                    "Tank record: {} days, {} to {}{}. Isobar uses its rain wherever your station has no day of its own, and its daily use and adjustments.",
                    crate::views::tanks::litres(log.len() as f64),
                    format::nice_date(a.date),
                    format::nice_date(b.date),
                    log.iter()
                        .rev()
                        .find(|d| d.rain.is_some())
                        .filter(|d| d.date != b.date)
                        .map_or(String::new(), |d| format!(", with rain to {}", format::nice_date(d.date)))
                ),
                _ => "No tank record. Import a spreadsheet you've kept (Excel, LibreOffice or CSV) with a Date column and an mm column, and optionally Use (litres a day) and Adjust (litres added or taken out).".into(),
            };
            ui::para(ui, line, theme::body(14.0), p.ink2, 640.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                if ui::button(ui, "Import a tank record", Kind::Secondary).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_title("Choose your tank or rainfall spreadsheet")
                        .add_filter("Spreadsheet or CSV", &["xlsx", "xlsm", "xls", "ods", "csv"])
                        .add_filter("All files", &["*"])
                        .pick_file()
                    {
                        worker.lock().tank_outcome = None;
                        worker.send(Cmd::ImportTankLog(path));
                    }
                }
                if !log.is_empty() && ui::button(ui, "Delete the record", Kind::Danger).clicked() {
                    worker.send(Cmd::ClearTankLog);
                }
            });
            if let Some(o) = outcome {
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

    fn now_cards(&mut self, ui: &mut Ui, settings: &Settings, p: &Palette) {
        ui::panel(ui, |ui| {
            ui::h2(ui, "Now screen");
            ui::para(ui, "The cards under the thermograph. Turn off the ones you don't need; to move a card, drag it by its name on Now.", theme::body(14.0), p.ink2, 600.0);
            let order: Vec<NowCard> = if self.draft.reset_order {
                NowCard::ALL.to_vec()
            } else {
                settings.cards().into_iter().map(|c| c.0).collect()
            };
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(18.0, 8.0);
                for card in order {
                    let mut on = !self.draft.hidden_cards.contains(&card);
                    let note = match card {
                        NowCard::Indoor => " (when the station has indoor readings)",
                        NowCard::Tanks => " (once tanks are set up)",
                        _ => "",
                    };
                    if ui
                        .checkbox(
                            &mut on,
                            egui::RichText::new(format!("{}{note}", card.label()))
                                .font(theme::body(15.0)),
                        )
                        .changed()
                    {
                        if on {
                            self.draft.hidden_cards.retain(|c| *c != card);
                        } else {
                            self.draft.hidden_cards.push(card);
                            self.draft.hidden_cards.sort();
                        }
                    }
                }
            });
            ui.checkbox(
                &mut self.draft.reset_order,
                egui::RichText::new("Put the cards back in the usual order")
                    .font(theme::body(15.0)),
            );
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
            ui.checkbox(
                &mut self.draft.stopped,
                egui::RichText::new("The station stops sending data to MX").font(theme::body(15.0)),
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

#[cfg(test)]
mod tests {
    use super::parse_latitude;

    #[test]
    fn reads_latitudes() {
        assert_eq!(parse_latitude(""), Ok(None));
        assert_eq!(parse_latitude("-33.9"), Ok(Some(-33.9)));
        assert_eq!(parse_latitude("33,9 S"), Ok(Some(-33.9)));
        assert_eq!(parse_latitude("51.5°N"), Ok(Some(51.5)));
        assert!(parse_latitude("123").is_err());
        assert!(parse_latitude("north").is_err());
    }

    #[test]
    fn reads_tank_fields() {
        use super::TankDraft;
        let mut d = TankDraft::from(None);
        assert_eq!(d.to_tanks(), Ok(None));
        d.roof = "385".into();
        assert!(d.to_tanks().is_err(), "needs the litres too");
        d.count = 2;
        d.litres[0] = "22,500".into();
        assert!(d.to_tanks().is_err(), "tank 2 needs its litres");
        d.litres[1] = "22500".into();
        d.litres[2] = "999".into();
        d.use_per_day = "290".into();
        d.known = "full".into();
        let t = d.to_tanks().unwrap().unwrap();
        assert_eq!(t.capacity(), 45_000.0);
        assert_eq!(t.known.unwrap().litres, 45_000.0);
        d.known = "1350 mm".into();
        assert!(d.to_tanks().is_err(), "a depth needs the diameter");
        d.diameter = ["3530".into(), "3530".into(), String::new(), String::new()];
        let l = d.to_tanks().unwrap().unwrap().known.unwrap().litres;
        assert!((l - 26_424.3).abs() < 0.1);
        assert_eq!(t.tanks.len(), 2, "only the tanks chosen count");
        let back = TankDraft::from(Some(&t));
        assert_eq!((back.known.as_str(), back.count), ("full", 2));
    }
}
