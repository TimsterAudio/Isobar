//! Reports: NOAA-style monthly and yearly climate summaries and a month in
//! review, previewed here and saved as HTML pages (or text) to open, keep,
//! print or share.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::Datelike;
use eframe::egui::{self, Ui};
use isobar_core::reports::{self as rp, ReportContext};

use crate::app::{Route, Snapshot};
use crate::format::{self, num, MONTHS_LONG};
use crate::settings::Settings;
use crate::theme::{self, Palette};
use crate::ui::{self, Kind, Stat};

#[derive(Clone, Copy, PartialEq)]
enum Report {
    Month,
    Year,
    Review,
    Almanac,
    Export,
}

impl Report {
    const ALL: [Report; 5] = [
        Report::Month,
        Report::Year,
        Report::Review,
        Report::Almanac,
        Report::Export,
    ];

    fn label(self) -> &'static str {
        match self {
            Report::Month => "Monthly climate summary",
            Report::Year => "Yearly climate summary",
            Report::Review => "Month in review",
            Report::Almanac => "Almanac",
            Report::Export => "Daily history (CSV)",
        }
    }

    fn about(self) -> &'static str {
        match self {
            Report::Month => "The NOAA-style table of each day: mean, high and low with their times, heating and cooling degree days, rain, average wind, peak gust and the dominant direction, then the month's totals and its departures from your station's normal.",
            Report::Year => "The NOAA-style year: a line a month for temperature, rain and wind, with departures from normal, extremes and their dates, and the count of hot, frosty and wet days.",
            Report::Review => "One page for the month: the headline figures against normal, charts of temperature and rain, the wind rose, records broken and notable days, and a day-by-day table. Made to print on one or two sheets.",
            Report::Almanac => "The year in review: how warm and wet it was against normal and how it ranks, month by month, station records set, firsts and lasts (frosts and hot days) against their usual dates, the longest spells, and the tank year if you've set up tanks.",
            Report::Export => "Every day's figures for the dates you choose, a row a day, to open in Excel, LibreOffice or any spreadsheet: highs and lows with their times, rain, wind, pressure, humidity, dew point, feels like, sunshine, solar, UV and degree days, each column headed with its unit.",
        }
    }
}

pub struct Reports {
    report: Report,
    year: i32,
    month: u32,
    /// The text preview, for (report, year, month, days version).
    preview: Option<((u8, i32, u32, u64), String)>,
    /// The dates to export, once set.
    export: Option<(chrono::NaiveDate, chrono::NaiveDate)>,
    /// The almanac and its page, for (year, days version, tank version,
    /// tank settings).
    almanac: Option<(AlmanacKey, Arc<(isobar_core::almanac::Almanac, String)>)>,
    outcome: Option<(bool, String)>,
    /// What the last "Open in browser" came to, filled in by the thread
    /// that asked the system to open it.
    opened: Arc<Mutex<Option<(bool, String)>>>,
}

impl Default for Reports {
    fn default() -> Self {
        Reports {
            report: Report::Month,
            year: 0,
            month: 0,
            preview: None,
            export: None,
            almanac: None,
            outcome: None,
            opened: Arc::default(),
        }
    }
}

fn context(snap: &Snapshot, settings: &Settings) -> ReportContext {
    let name = if snap.demo {
        "Demo station".to_string()
    } else if settings.station_name.trim().is_empty() {
        "My station".to_string()
    } else {
        settings.station_name.clone()
    };
    ReportContext {
        station: name,
        units: snap.units(),
        latitude: settings.latitude,
        made: format::time_and_date(snap.now_ms),
    }
}

/// Opens a folder (or file) in the system's file manager, on a thread of
/// its own.
pub fn open_in_background(path: std::path::PathBuf) {
    std::thread::spawn(move || {
        if let Err(e) = open_file(&path) {
            eprintln!("Couldn't open {}: {e}", path.display());
        }
    });
}

/// Opens a file with the system's usual program for it, a browser for HTML.
/// Blocks until the opener is done (on Linux it can run the browser itself),
/// so it runs on its own thread.
fn open_file(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        // Explorer hands the file to the default browser; its exit code
        // means nothing.
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        use std::process::{Command, Stdio};
        #[cfg(target_os = "macos")]
        let openers: &[(&str, &[&str])] = &[("open", &[])];
        // xdg-open first; then GLib's opener and Debian's default-browser
        // links, for desktops where xdg-open isn't set up.
        #[cfg(not(target_os = "macos"))]
        let openers: &[(&str, &[&str])] = &[
            ("xdg-open", &[]),
            ("gio", &["open"]),
            ("sensible-browser", &[]),
            ("x-www-browser", &[]),
        ];
        let mut tried = Vec::new();
        for (cmd, args) in openers {
            match Command::new(cmd)
                .args(*args)
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
            {
                Ok(out) if out.status.success() => return Ok(()),
                Ok(out) => {
                    // The first line says what went wrong; the rest is advice.
                    let err = String::from_utf8_lossy(&out.stderr);
                    let why: String = err
                        .lines()
                        .next()
                        .unwrap_or("")
                        .trim()
                        .chars()
                        .take(120)
                        .collect();
                    tried.push(if why.is_empty() {
                        format!("{cmd} failed ({})", out.status)
                    } else {
                        format!("{cmd}: {why}")
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => tried.push(format!("{cmd}: {e}")),
            }
        }
        Err(if tried.is_empty() {
            "this system has no xdg-open, gio or default browser to open it with".into()
        } else {
            tried.join("; ")
        })
    }
}

impl Reports {
    fn file_stem(&self) -> String {
        match self.report {
            Report::Month => format!("Climate summary {}-{:02}", self.year, self.month),
            Report::Year => format!("Climate summary {}", self.year),
            Report::Review => format!(
                "{} {} in review",
                MONTHS_LONG[self.month as usize - 1],
                self.year
            ),
            Report::Almanac => format!("{} almanac", self.year),
            Report::Export => match self.export {
                Some((a, b)) => format!(
                    "Daily history {} to {}",
                    a.format("%Y-%m-%d"),
                    b.format("%Y-%m-%d")
                ),
                None => "Daily history".into(),
            },
        }
    }

    /// The report as an HTML page, and as text for the NOAA ones.
    fn make(
        &mut self,
        snap: &Snapshot,
        settings: &Settings,
        cx: &ReportContext,
    ) -> Option<(String, Option<String>)> {
        let days = snap.days.as_slice();
        match self.report {
            Report::Almanac => {
                let key: AlmanacKey = (
                    self.year,
                    snap.days_version,
                    snap.tank_version,
                    serde_json::to_string(&settings.tanks).unwrap_or_default(),
                );
                if self.almanac.as_ref().is_none_or(|(k, _)| *k != key) {
                    let a = almanac(snap, settings, self.year, &cx.units)?;
                    let html = isobar_core::almanac::almanac_html(&a, cx);
                    self.almanac = Some((key, Arc::new((a, html))));
                }
                let made = self.almanac.as_ref()?.1.clone();
                Some((made.1.clone(), None))
            }
            Report::Export => None,
            Report::Month => {
                let m = rp::noaa_month(days, self.year, self.month, &cx.units)?;
                Some((
                    rp::noaa_month_html(&m, cx),
                    Some(rp::noaa_month_text(&m, cx)),
                ))
            }
            Report::Year => {
                let y = rp::noaa_year(days, self.year, &cx.units)?;
                Some((rp::noaa_year_html(&y, cx), Some(rp::noaa_year_text(&y, cx))))
            }
            Report::Review => {
                let r = rp::month_review(days, self.year, self.month, &cx.units)?;
                Some((rp::month_review_html(&r, cx), None))
            }
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        snap: &Snapshot,
        settings: &Settings,
        reports_dir: &Path,
        go: &mut Option<Route>,
    ) {
        let p = Palette::of(ui.ctx());
        ui::h1(ui, "Reports");
        let days = snap.days.as_slice();
        let (Some(first), Some(last)) = (days.first(), days.last()) else {
            ui::panel(ui, |ui| {
                if ui::empty(
                    ui,
                    "No history yet",
                    "Reports are made from your station's daily history. Connect a station or import a dayfile in Settings.",
                    Some("Open settings"),
                ) {
                    *go = Some(Route::Settings);
                }
            });
            return;
        };
        let (first, last) = (first.date, last.date);
        if self.year == 0 {
            self.year = last.year();
            self.month = last.month();
        }
        ui::para(
            ui,
            "Reports to keep, print or share, from your station's daily history, and the history itself to take into a spreadsheet. Each report saves as a single HTML page that opens in any browser; print it from there, or choose Save as PDF in the print dialog.",
            theme::body(14.0),
            p.ink2,
            900.0,
        );

        ui::panel(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for r in Report::ALL {
                    if ui::chip(ui, r.label(), self.report == r).clicked() {
                        self.report = r;
                        self.outcome = None;
                    }
                }
            });
            ui::para(ui, self.report.about(), theme::body(14.0), p.ink2, 900.0);
            if self.report == Report::Export {
                return;
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                if !matches!(self.report, Report::Year | Report::Almanac) {
                    ui::small(ui, "Month", p.ink2);
                    let months: Vec<(u32, String)> = (1..=12)
                        .map(|m| (m, MONTHS_LONG[m as usize - 1].to_string()))
                        .collect();
                    if ui::select(ui, "report-month", &mut self.month, &months, 130.0) {
                        self.outcome = None;
                    }
                }
                ui::small(ui, "Year", p.ink2);
                let years: Vec<(i32, String)> = (first.year()..=last.year())
                    .rev()
                    .map(|y| (y, y.to_string()))
                    .collect();
                if ui::select(ui, "report-year", &mut self.year, &years, 90.0) {
                    self.outcome = None;
                }
            });
        });

        if self.report == Report::Export {
            self.export_panel(ui, snap, first, last);
            return;
        }
        let cx = context(snap, settings);
        let key = (self.report as u8, self.year, self.month, snap.days_version);
        let made = self.make(snap, settings, &cx);
        let Some((html, text)) = made else {
            ui::panel(ui, |ui| {
                ui::text(
                    ui,
                    "No days on record for this period. Choose another month or year.",
                    theme::body(15.0),
                    p.ink,
                );
            });
            return;
        };

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let stem = self.file_stem();
            if ui::button(ui, "Open in browser", Kind::Primary).clicked() {
                let path = reports_dir.join(format!("{stem}.html"));
                match std::fs::create_dir_all(reports_dir).and_then(|_| std::fs::write(&path, &html)) {
                    Err(e) => {
                        self.outcome = Some((
                            false,
                            format!("Couldn't write the report to {}: {e}.", reports_dir.display()),
                        ))
                    }
                    Ok(()) => {
                        self.outcome = Some((
                            true,
                            format!("Opening in your browser. Saved as {}", path.display()),
                        ));
                        *self.opened.lock().unwrap() = None;
                        let (slot, ctx) = (self.opened.clone(), ui.ctx().clone());
                        std::thread::spawn(move || {
                            if let Err(e) = open_file(&path) {
                                *slot.lock().unwrap() = Some((
                                    false,
                                    format!(
                                        "Couldn't open your browser ({e}). The report is saved as {}: open it from there.",
                                        path.display()
                                    ),
                                ));
                                ctx.request_repaint();
                            }
                        });
                    }
                }
            }
            if let Some(o) = self.opened.lock().unwrap().take() {
                self.outcome = Some(o);
            }
            if ui::button(ui, "Save as HTML…", Kind::Secondary).clicked() {
                self.outcome = save(&stem, "html", "Web page", &html);
            }
            if let Some(t) = text.as_ref() {
                if ui::button(ui, "Save as text…", Kind::Secondary)
                    .on_hover_text(
                        "The classic fixed-width layout, as Cumulus writes its NOAA reports.",
                    )
                    .clicked()
                {
                    self.outcome = save(&stem, "txt", "Text", t);
                }
            }
        });
        if let Some((ok, msg)) = &self.outcome {
            ui::para(
                ui,
                msg,
                theme::body(13.0),
                if *ok { p.ok } else { p.heat },
                1100.0,
            );
        }

        ui::panel(ui, |ui| match (self.report, text) {
            (Report::Review, _) => self.review_preview(ui, snap, &cx),
            (Report::Almanac, _) => self.almanac_preview(ui, &cx),
            (_, Some(t)) => {
                if self.preview.as_ref().is_none_or(|(k, _)| *k != key) {
                    self.preview = Some((key, t));
                }
                let t = &self.preview.as_ref().unwrap().1;
                ui::small(ui, "Preview", p.ink2);
                egui::ScrollArea::horizontal()
                    .id_salt("report-text")
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(t)
                                    .font(egui::FontId::monospace(12.5))
                                    .color(p.ink),
                            )
                            .extend(),
                        );
                    });
            }
            _ => {}
        });
    }

    fn export_panel(
        &mut self,
        ui: &mut Ui,
        snap: &Snapshot,
        first: chrono::NaiveDate,
        last: chrono::NaiveDate,
    ) {
        let p = Palette::of(ui.ctx());
        let (mut from, mut to) = self.export.unwrap_or((first, last));
        ui::panel(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                ui::small(ui, "From", p.ink2);
                ui::date_input(ui, "export-from", &mut from, first.year()..=last.year());
                ui::small(ui, "to", p.ink2);
                ui::date_input(ui, "export-to", &mut to, first.year()..=last.year());
                ui.add_space(8.0);
                for (label, a, b) in [
                    ("Everything", first, last),
                    (
                        "This year",
                        chrono::NaiveDate::from_ymd_opt(last.year(), 1, 1).unwrap(),
                        last,
                    ),
                    (
                        "Last year",
                        chrono::NaiveDate::from_ymd_opt(last.year() - 1, 1, 1).unwrap(),
                        chrono::NaiveDate::from_ymd_opt(last.year() - 1, 12, 31).unwrap(),
                    ),
                ] {
                    if ui::chip(ui, label, from == a.max(first) && to == b.min(last)).clicked() {
                        (from, to) = (a.max(first), b.min(last));
                    }
                }
            });
            if to < from {
                std::mem::swap(&mut from, &mut to);
            }
            let n = isobar_core::analytics::in_range(snap.days.as_slice(), from, to).len();
            ui::small(
                ui,
                format!(
                    "{n} days on record from {} to {}, 31 columns.",
                    format::nice_date(from),
                    format::nice_date(to)
                ),
                p.ink2,
            );
            ui.add_space(4.0);
            self.export = Some((from, to));
            if ui::button(ui, "Save as CSV…", Kind::Primary).clicked() && n > 0 {
                let csv =
                    isobar_core::export::daily_csv(snap.days.as_slice(), from, to, &snap.units());
                self.outcome = save(&self.file_stem(), "csv", "CSV spreadsheet", &csv);
            }
            if let Some((ok, msg)) = &self.outcome {
                ui::para(
                    ui,
                    msg,
                    theme::body(13.0),
                    if *ok { p.ok } else { p.heat },
                    1100.0,
                );
            }
        });
    }

    fn almanac_preview(&self, ui: &mut Ui, cx: &ReportContext) {
        use isobar_core::almanac::{event_words, rank_words};
        let p = Palette::of(ui.ctx());
        let Some((_, made)) = self.almanac.as_ref() else {
            return;
        };
        let a = &made.0;
        let u = &cx.units;
        let x = &a.noaa.totals;
        ui::small(ui, "The page's headline figures", p.ink2);
        let temp = match (a.anomaly, a.warm_rank) {
            (Some(d), Some(r)) => format!(
                "{} against normal; {}",
                format::signed(Some(d), 1),
                rank_words(r, "warmest", "coolest")
            ),
            (Some(d), None) => format!("{} against normal", format::signed(Some(d), 1)),
            _ => String::new(),
        };
        let rain = match (a.normal_rain.filter(|n| *n > 0.0), a.wet_rank) {
            (Some(n), Some(r)) => format!(
                "{:.0}% of normal; {}",
                x.rain / n * 100.0,
                rank_words(r, "wettest", "driest")
            ),
            (Some(n), None) => format!(
                "{:.0}% of normal{}",
                x.rain / n * 100.0,
                if a.partial { " to date" } else { "" }
            ),
            _ => String::new(),
        };
        ui::stats(
            ui,
            &[
                Stat::new(
                    "Mean temperature",
                    format!("{}{}", num(x.mean, 1), u.temp),
                    p.ink,
                )
                .detail(temp, p.ink2),
                Stat::new(
                    "Rain",
                    format!("{} {}", num(Some(x.rain), 0), u.rain),
                    p.rain,
                )
                .detail(rain, p.ink2),
                Stat::new(
                    "Hottest day",
                    x.high
                        .map_or("–".into(), |h| format!("{}{}", num(Some(h.0), 1), u.temp)),
                    p.heat,
                )
                .detail(
                    x.high.map_or(String::new(), |h| format::nice_date(h.1)),
                    p.ink2,
                ),
                Stat::new(
                    "Coldest night",
                    x.low
                        .map_or("–".into(), |l| format!("{}{}", num(Some(l.0), 1), u.temp)),
                    p.cold,
                )
                .detail(
                    x.low.map_or(String::new(), |l| format::nice_date(l.1)),
                    p.ink2,
                ),
            ],
        );
        for rec in &a.records {
            ui::text(ui, rec, theme::semi(14.0), p.ink);
        }
        for e in &a.events {
            let line = match e.date {
                Some(d) => format!("{}: {}, {}.", e.label, format::nice_date(d), event_words(e)),
                None => format!("{}: {}.", e.label, {
                    let w = event_words(e);
                    let mut c = w.chars();
                    c.next()
                        .map_or(String::new(), |f| f.to_lowercase().chain(c).collect())
                }),
            };
            ui::small(ui, line, p.ink2);
        }
        ui::small(
            ui,
            "The full page adds charts and a table month by month, the longest spells and the tank year. Open it in the browser to see it.",
            p.ink2,
        );
    }

    fn review_preview(&self, ui: &mut Ui, snap: &Snapshot, cx: &ReportContext) {
        let p = Palette::of(ui.ctx());
        let Some(r) = rp::month_review(snap.days.as_slice(), self.year, self.month, &cx.units)
        else {
            return;
        };
        let x = &r.noaa.totals;
        let u = &cx.units;
        ui::small(ui, "The page's headline figures", p.ink2);
        let dep = match (x.mean, x.normal_mean) {
            (Some(a), Some(n)) => format!("{} against normal", format::signed(Some(a - n), 1)),
            _ => String::new(),
        };
        let pct = x
            .normal_rain
            .filter(|n| *n > 0.0)
            .map_or(String::new(), |n| {
                format!("{:.0}% of normal", x.rain / n * 100.0)
            });
        ui::stats(
            ui,
            &[
                Stat::new(
                    "Mean temperature",
                    format!("{}{}", num(x.mean, 1), u.temp),
                    p.ink,
                )
                .detail(dep, p.ink2),
                Stat::new(
                    "Rain",
                    format!("{} {}", num(Some(x.rain), 1), u.rain),
                    p.rain,
                )
                .detail(pct, p.ink2),
                Stat::new(
                    "Hottest day",
                    x.high
                        .map_or("–".into(), |h| format!("{}{}", num(Some(h.0), 1), u.temp)),
                    p.heat,
                )
                .detail(
                    x.high.map_or(String::new(), |h| format::nice_date(h.1)),
                    p.ink2,
                ),
                Stat::new(
                    "Coldest night",
                    x.low
                        .map_or("–".into(), |l| format!("{}{}", num(Some(l.0), 1), u.temp)),
                    p.cold,
                )
                .detail(
                    x.low.map_or(String::new(), |l| format::nice_date(l.1)),
                    p.ink2,
                ),
            ],
        );
        for rec in &r.records {
            ui::text(ui, rec, theme::semi(14.0), p.ink);
        }
        ui::small(
            ui,
            format!(
                "The full page adds charts of temperature and rain against normal, the wind rose, {} notable days and a day-by-day table. Open it in the browser to see it.",
                r.notable.len()
            ),
            p.ink2,
        );
    }
}

type AlmanacKey = (i32, u64, u64, String);

/// The almanac for `year`, with the tank year when tanks are set up.
fn almanac(
    snap: &Snapshot,
    settings: &Settings,
    year: i32,
    u: &isobar_core::Units,
) -> Option<isobar_core::almanac::Almanac> {
    use isobar_core::tanks as tk;
    let days = snap.days.as_slice();
    let tank = settings
        .tanks
        .as_ref()
        .filter(|t| t.is_set_up())
        .and_then(|t| {
            let inputs = tk::inputs(days, &snap.tank_log, isobar_core::seasons::is_inches(u));
            let sim = tk::simulate(&inputs, t);
            tk::years(&sim, t.keep_above)
                .into_iter()
                .find(|y| y.year == year)
        });
    let hemi = isobar_core::seasons::hemisphere(days, settings.latitude);
    isobar_core::almanac::almanac(days, year, u, hemi, tank)
}

fn save(stem: &str, ext: &str, kind: &str, body: &str) -> Option<(bool, String)> {
    let path: PathBuf = rfd::FileDialog::new()
        .set_title("Save the report")
        .set_file_name(format!("{stem}.{ext}"))
        .add_filter(kind, &[ext])
        .save_file()?;
    Some(match std::fs::write(&path, body) {
        Ok(()) => (true, format!("Saved {}", path.display())),
        Err(e) => (false, format!("Couldn't save the report: {e}.")),
    })
}

/// Where "Open in browser" keeps its copies: the Reports folder of the
/// folder chosen in Settings, or "Isobar reports" in Documents. Not beside
/// the usual history folder: that's hidden on Linux, and sandboxed browsers
/// (snap and Flatpak Firefox) can't read it. The demo uses the temporary
/// folder.
pub fn folder(demo: bool) -> PathBuf {
    if demo {
        std::env::temp_dir().join("Isobar reports")
    } else {
        crate::settings::Paths::locate().reports
    }
}
