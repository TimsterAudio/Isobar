//! Insights: unusual days measured against the seasonal normal for that date,
//! a records board, a monthly climate table and prevailing-wind roses.

use chrono::{Datelike, Duration, NaiveDate, TimeZone};
use eframe::egui::{self, vec2, Align, Layout, Ui};
use isobar_core::analytics::{
    self as an, Anomaly, Field, MonthClimate, RecordRow, RecordUnit, RecordWhen, Tone, COMPASS16,
};

use isobar_core::seasons::{self as se, Hemisphere};

use crate::app::{Route, Snapshot};
use crate::chart::{Chart, Series, XAxis};
use crate::dial;
use crate::format::{self, nice_date, num, signed, MONTHS};
use crate::settings::Settings;
use crate::theme::{self, Palette};
use crate::ui::{self, cell, col};

struct Cache {
    key: (u64, usize, bool),
    anomalies: Option<Vec<Anomaly>>,
    records: Vec<RecordRow>,
    climate: Vec<MonthClimate>,
    rose_all: ([usize; 16], usize),
    rose_year: ([usize; 16], usize),
    intensity: Vec<an::IntensityYear>,
    wettest_hours: Vec<(NaiveDate, f64)>,
    spells: Vec<an::DrySpell>,
    dry_now: Option<an::DrySpell>,
    last_rain: Option<(NaiveDate, f64)>,
    falls: Vec<an::PressureFall>,
    rose_seasons: [([usize; 16], usize); 4],
    gusts: Vec<an::GustDay>,
    gust_months: ([Option<f64>; 12], Option<f64>),
    hemisphere: Hemisphere,
}

pub struct Insights {
    window: usize,
    this_year: bool,
    /// The wind roses by season rather than by year.
    by_season: bool,
    cache: Option<Cache>,
}

impl Default for Insights {
    fn default() -> Self {
        Insights {
            window: 90,
            this_year: false,
            by_season: false,
            cache: None,
        }
    }
}

impl Insights {
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        snap: &Snapshot,
        settings: &Settings,
        go: &mut Option<Route>,
    ) {
        ui::h1(ui, "Insights");
        let days = snap.days.as_slice();
        let Some(last) = days.last().map(|d| d.date) else {
            ui::panel(ui, |ui| {
                if ui::empty(
                    ui,
                    "No history yet",
                    "Records, anomalies and climate summaries appear once Isobar has your station's daily history.",
                    Some("Open settings"),
                ) {
                    *go = Some(Route::Settings);
                }
            });
            return;
        };
        let year_start = format::first_of_year(last);
        let key = (snap.days_version, self.window, self.this_year);
        if self.cache.as_ref().is_none_or(|c| c.key != key) {
            let year_days = an::in_range(days, year_start, last);
            let gusts = an::gust_days(days, min_average(&snap.units().wind));
            self.cache = Some(Cache {
                key,
                anomalies: (an::years_covered(days) >= 2.0).then(|| {
                    let mut a = an::anomalies(
                        days,
                        last - Duration::days(self.window as i64 - 1),
                        last,
                        1.5,
                    );
                    a.truncate(14);
                    a
                }),
                records: an::records(if self.this_year { year_days } else { days }),
                climate: an::monthly_climate(days),
                rose_all: an::wind_rose(days),
                rose_year: an::wind_rose(year_days),
                intensity: an::rain_intensity(
                    days,
                    if snap.units().rain.trim().eq_ignore_ascii_case("in") {
                        25.4
                    } else {
                        1.0
                    },
                ),
                wettest_hours: an::wettest_hours(days, 5),
                spells: an::dry_spells(days),
                dry_now: an::current_dry_spell(days),
                last_rain: days
                    .iter()
                    .rev()
                    .find(|d| an::is_rain_day(d))
                    .map(|d| (d.date, d.rain.unwrap_or(0.0))),
                falls: an::biggest_pressure_falls(days, 10),
                rose_seasons: an::seasonal_wind_roses(days),
                gust_months: an::gust_factor_by_month(&gusts),
                gusts,
                hemisphere: se::hemisphere(days, settings.latitude),
            });
        }

        let wide = ui.available_width() >= 1100.0;
        if wide {
            ui::split_with(
                ui,
                0.6,
                24.0,
                self,
                |ui, s| s.unusual(ui, snap),
                |ui, s| s.records(ui, snap),
            );
        } else {
            self.unusual(ui, snap);
            self.records(ui, snap);
        }
        if wide {
            ui::split_with(
                ui,
                0.6,
                24.0,
                self,
                |ui, s| s.intensity(ui, snap),
                |ui, s| s.dry_spell(ui, snap, last),
            );
        } else {
            self.intensity(ui, snap);
            self.dry_spell(ui, snap, last);
        }
        self.pressure_falls(ui, snap);
        self.climate(ui, snap, days[0].date, last);
        self.wind(ui, last);
        self.gust_factor(ui, snap);
    }

    fn unusual(&mut self, ui: &mut Ui, snap: &Snapshot) {
        let p = Palette::of(ui.ctx());
        let u = snap.units();
        ui::panel(ui, |ui| {
            ui.horizontal(|ui| {
                ui::h2(ui, "Unusual days");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let opts = [(365, "12 months"), (90, "90 days"), (30, "30 days")];
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for (n, label) in opts {
                        if ui::chip(ui, label, self.window == n).clicked() {
                            self.window = n;
                        }
                    }
                });
            });
            ui::para(
                ui,
                "Each day is compared with the normal for that time of year: the average of the same dates, a week either side, in every other year.",
                theme::body(14.0),
                p.ink2,
                640.0,
            );
            let Some(list) = self.cache.as_ref().and_then(|c| c.anomalies.as_ref()) else {
                ui::text(
                    ui,
                    "This needs at least two years of history to know what normal looks like.",
                    theme::body(15.0),
                    p.ink,
                );
                return;
            };
            if list.is_empty() {
                ui::text(ui, "Nothing unusual. Every day in this window was within 1.5 standard deviations of normal.", theme::body(15.0), p.ink);
                return;
            }
            ui.spacing_mut().item_spacing.y = 0.0;
            for a in list {
                let warm = a.z > 0.0;
                let what = match (a.field, warm) {
                    (Field::TempMax, true) => "Hot day",
                    (Field::TempMax, false) => "Cold day",
                    (_, true) => "Warm night",
                    (_, false) => "Cold night",
                };
                let row_w = ui.available_width();
                ui.horizontal(|ui| {
                    ui.set_height(34.0);
                    ui.spacing_mut().item_spacing.x = 12.0;
                    let fixed = |ui: &mut Ui, w: f32, right: bool, add: &dyn Fn(&mut Ui)| {
                        let layout = if right {
                            Layout::right_to_left(Align::Center)
                        } else {
                            Layout::left_to_right(Align::Center)
                        };
                        ui.allocate_ui_with_layout(vec2(w, 34.0), layout, |ui| {
                            ui.set_min_width(w);
                            add(ui);
                        });
                    };
                    fixed(ui, 92.0, false, &|ui| {
                        ui::text(ui, nice_date(a.date), theme::body(14.0), p.ink);
                    });
                    fixed(ui, 84.0, false, &|ui| {
                        ui::text(ui, what, theme::body(14.0), p.ink2);
                    });
                    fixed(ui, 60.0, true, &|ui| {
                        ui::text(
                            ui,
                            format!("{}{}", num(Some(a.value), 1), u.temp),
                            theme::semi(14.0),
                            p.ink,
                        );
                    });
                    let rest = (row_w - 92.0 - 84.0 - 60.0 - 200.0 - 12.0 * 4.0).max(120.0);
                    fixed(ui, rest, false, &|ui| {
                        ui::text(
                            ui,
                            format!(
                                "normal {}{}, {}",
                                num(Some(a.normal), 1),
                                u.temp,
                                signed(Some(a.value - a.normal), 1)
                            ),
                            theme::body(13.0),
                            p.ink2,
                        );
                    });
                    ui::meter(
                        ui,
                        140.0,
                        (a.z.abs() / 3.5) as f32,
                        if warm { p.heat } else { p.cold },
                    );
                    fixed(ui, 48.0, true, &|ui| {
                        ui::text(
                            ui,
                            format!("{}σ", signed(Some(a.z), 1)),
                            theme::body(13.0),
                            p.ink,
                        );
                    });
                });
                ui::rule(ui, p.faint);
            }
        });
    }

    fn records(&mut self, ui: &mut Ui, snap: &Snapshot) {
        let p = Palette::of(ui.ctx());
        let u = snap.units();
        ui::panel(ui, |ui| {
            ui.horizontal(|ui| {
                ui::h2(ui, "Records");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if ui::chip(ui, "This year", self.this_year).clicked() {
                        self.this_year = true;
                    }
                    if ui::chip(ui, "All time", !self.this_year).clicked() {
                        self.this_year = false;
                    }
                });
            });
            let Some(c) = self.cache.as_ref() else { return };
            let rows = &c.records;
            for pair in rows.chunks(2) {
                ui::columns(ui, 2, 24.0, false, |i, ui| {
                    let Some(r) = pair.get(i) else { return };
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui::small(ui, r.label, p.ink2);
                    let unit = match r.unit {
                        RecordUnit::Temp => u.temp.clone(),
                        RecordUnit::Rain => format!(" {}", u.rain),
                        RecordUnit::Rate => format!(" {}/h", u.rain),
                        RecordUnit::Wind => format!(" {}", u.wind),
                        RecordUnit::Press => format!(" {}", u.press),
                        RecordUnit::Days => " days".into(),
                    };
                    let digits = if matches!(r.unit, RecordUnit::Days | RecordUnit::Wind) {
                        0
                    } else {
                        1
                    };
                    let color = match (r.tone, r.unit) {
                        (Tone::Warm, _) => p.heat,
                        (Tone::Cool, _) => p.cold,
                        (_, RecordUnit::Rain | RecordUnit::Rate) => p.rain,
                        (_, RecordUnit::Wind) => p.wind,
                        _ => p.ink,
                    };
                    let value = r
                        .value
                        .map_or("–".into(), |v| format!("{}{unit}", num(Some(v), digits)));
                    ui::text(ui, value, theme::display(25.0), color);
                    let when = match &r.when {
                        RecordWhen::None => String::new(),
                        RecordWhen::Day(d, Some(t)) => {
                            format!("{} at {}", nice_date(*d), format::clock(t))
                        }
                        RecordWhen::Day(d, None) => nice_date(*d),
                        RecordWhen::Month(d) => format::month_year(*d),
                        RecordWhen::Span(a, b) => format!("{} to {}", nice_date(*a), nice_date(*b)),
                    };
                    ui::small(ui, when, p.ink2);
                    ui.add_space(6.0);
                    ui::rule(ui, p.faint);
                });
            }
        });
    }

    fn intensity(&mut self, ui: &mut Ui, snap: &Snapshot) {
        let p = Palette::of(ui.ctx());
        let u = snap.units();
        let Some(c) = self.cache.as_ref() else { return };
        ui::panel(ui, |ui| {
            ui::h2(ui, "Rain intensity");
            let inches = u.rain.trim().eq_ignore_ascii_case("in");
            let edges: Vec<String> = an::INTENSITY_MM
                .iter()
                .map(|mm| {
                    if inches {
                        num(Some(mm / 25.4), 2)
                    } else {
                        num(Some(*mm), if *mm < 1.0 { 1 } else { 0 })
                    }
                })
                .collect();
            ui::para(
                ui,
                format!(
                    "How each year's rain fell: the share that came on light days (under {} {u}), moderate ({} to {}), heavy ({} to {}) and very heavy days ({} or more). Heavy days do the most for tanks and dams; light ones mostly wet the surface.",
                    edges[1], edges[1], edges[2], edges[2], edges[3], edges[3],
                    u = u.rain
                ),
                theme::body(14.0),
                p.ink2,
                640.0,
            );
            let pct = |part: f64, total: f64| {
                if total > 0.0 {
                    format!("{}%", num(Some(part / total * 100.0), 0))
                } else {
                    "–".into()
                }
            };
            let mut rows = Vec::new();
            for y in c.intensity.iter().rev() {
                let mut r = vec![
                    cell(y.year.to_string()).strong(),
                    cell(format!("{} {}", num(Some(y.total), 0), u.rain)).color(p.rain),
                ];
                for k in 0..4 {
                    r.push(
                        cell(pct(y.rain[k], y.total))
                            .color(if k >= 2 { p.rain } else { p.ink })
                            .tooltip(format!(
                                "{} {} on {} days",
                                num(Some(y.rain[k]), 1),
                                u.rain,
                                y.days[k]
                            )),
                    );
                }
                rows.push(r);
            }
            let cols = [
                col("Year", 70.0, false),
                col("Rain", 0.0, true),
                col("Light", 0.0, true),
                col("Moderate", 0.0, true),
                col("Heavy", 0.0, true),
                col("Very heavy", 0.0, true),
            ];
            ui::table(ui, &cols, &rows, 14.0);
            if !c.wettest_hours.is_empty() {
                let list: Vec<String> = c
                    .wettest_hours
                    .iter()
                    .map(|(d, r)| format!("{} {} on {}", num(Some(*r), 1), u.rain, nice_date(*d)))
                    .collect();
                ui::para(
                    ui,
                    format!("Wettest hours on record: {}.", list.join("; ")),
                    theme::body(14.0),
                    p.ink,
                    640.0,
                );
            }
            let bursts = &snap.log_summary.bursts;
            if !bursts.is_empty() {
                ui.add_space(6.0);
                ui::text(ui, "By how hard it fell", theme::semi(15.0), p.ink);
                ui::para(
                    ui,
                    "From MX's detailed logs: the share of each year's rain that fell at under 2.5 mm an hour (light), up to 10 (moderate), up to 50 (heavy) and over 50 (violent). Rain logged across a gap of over an hour can't be timed and is left out.",
                    theme::body(14.0),
                    p.ink2,
                    640.0,
                );
                let mut rows = Vec::new();
                for y in bursts.iter().rev() {
                    let timed: f64 = y.rain.iter().sum();
                    let mut r = vec![
                        cell(y.year.to_string()).strong(),
                        cell(format!("{} {}", num(Some(timed), 0), u.rain)).color(p.rain),
                    ];
                    for k in 0..4 {
                        r.push(
                            cell(pct(y.rain[k], timed))
                                .color(if k >= 2 { p.rain } else { p.ink })
                                .tooltip(format!("{} {}", num(Some(y.rain[k]), 1), u.rain)),
                        );
                    }
                    rows.push(r);
                }
                let cols = [
                    col("Year", 70.0, false),
                    col("Timed rain", 0.0, true),
                    col("Light", 0.0, true),
                    col("Moderate", 0.0, true),
                    col("Heavy", 0.0, true),
                    col("Violent", 0.0, true),
                ];
                ui::table(ui, &cols, &rows, 14.0);
            }
        });
    }

    fn dry_spell(&mut self, ui: &mut Ui, snap: &Snapshot, last: NaiveDate) {
        let p = Palette::of(ui.ctx());
        let u = snap.units();
        let Some(c) = self.cache.as_ref() else { return };
        ui::panel(ui, |ui| {
            ui::h2(ui, "Dry spell");
            let len = c.dry_now.map_or(0, |s| s.days());
            let (near, years) = an::dry_spell_context(&c.spells, c.dry_now, last, len.max(1));
            let longest = c
                .spells
                .iter()
                .filter(|s| Some(**s) != c.dry_now)
                .max_by_key(|s| s.days())
                .copied();
            let span = |s: an::DrySpell| format!("{} to {}", nice_date(s.first), nice_date(s.last));
            let n_years = c.intensity.len();
            let stats = vec![
                ui::Stat::new(
                    "Dry days in a row",
                    format!("{len} {}", if len == 1 { "day" } else { "days" }),
                    if len >= 14 { p.heat } else { p.ink },
                )
                .detail(
                    match c.last_rain {
                        Some((d, r)) if len > 0 => format!(
                            "To {}; last rain {} {} on {}",
                            nice_date(last),
                            num(Some(r), 1),
                            u.rain,
                            nice_date(d)
                        ),
                        Some((d, r)) => {
                            format!("{} {} on {}", num(Some(r), 1), u.rain, nice_date(d))
                        }
                        None => String::new(),
                    },
                    p.ink2,
                ),
                ui::Stat::new(
                    "Longest at this time of year",
                    near.map_or("–".into(), |s| format!("{} days", s.days())),
                    p.ink,
                )
                .detail(near.map_or(String::new(), span), p.ink2),
                ui::Stat::new(
                    "This long, this time of year",
                    if len < 3 {
                        "–".into()
                    } else {
                        format!("{years} of {n_years} years")
                    },
                    p.ink,
                )
                .detail(
                    match len {
                        0 => "Rain on the last day on record",
                        1 | 2 => "Too short a spell to compare",
                        _ => "Years with a dry spell at least as long, within a fortnight of now",
                    },
                    p.ink2,
                ),
                ui::Stat::new(
                    "Longest on record",
                    longest.map_or("–".into(), |s| format!("{} days", s.days())),
                    p.ink,
                )
                .detail(longest.map_or(String::new(), span), p.ink2),
            ];
            ui::stats(ui, &stats);
            if snap
                .live
                .as_ref()
                .and_then(|r| r.rain_today)
                .is_some_and(|r| r >= an::RAIN_DAY)
            {
                ui::small(ui, "It has rained today, so this spell ends today.", p.rain);
            }
            ui::small(
                ui,
                format!(
                    "A dry day has less than {} {} of rain.",
                    num(Some(an::RAIN_DAY), 1),
                    u.rain
                ),
                p.ink2,
            );
        });
    }

    fn pressure_falls(&mut self, ui: &mut Ui, snap: &Snapshot) {
        let p = Palette::of(ui.ctx());
        let u = snap.units();
        let Some(c) = self.cache.as_ref() else { return };
        let logged = &snap.log_summary.falls;
        if !logged.is_empty() {
            let digits = if an::hpa_per_unit(&u.press) > 5.0 {
                2
            } else {
                1
            };
            let when = |ms: i64| {
                chrono::Local.timestamp_millis_opt(ms).single().map(|t| {
                    (
                        t.date_naive(),
                        format!(
                            "{} {}",
                            nice_date(t.date_naive()),
                            format::clock(&t.format("%H:%M").to_string())
                        ),
                    )
                })
            };
            ui::panel(ui, |ui| {
                ui::h2(ui, "Biggest pressure falls");
                ui::para(
                    ui,
                    "Your stormiest days: the deepest falls within 24 hours, from MX's detailed logs. Falls within two days of a bigger one count once, as one storm.",
                    theme::body(14.0),
                    p.ink2,
                    760.0,
                );
                let rows: Vec<Vec<ui::Cell>> = logged
                    .iter()
                    .map(|f| {
                        let end = when(f.to_ts);
                        let day = end
                            .as_ref()
                            .and_then(|(d, _)| snap.days.iter().find(|r| r.date == *d));
                        let hours = (f.to_ts - f.from_ts) as f64 / 3_600_000.0;
                        vec![
                            cell(end.map_or("–".into(), |e| e.1)).strong(),
                            cell(format!("{} {}", num(Some(f.fall()), digits), u.press))
                                .color(p.heat),
                            cell(format!("{} h", num(Some(hours), 0))),
                            cell(format!(
                                "{} to {}",
                                num(Some(f.from), digits),
                                num(Some(f.to), digits)
                            )),
                            cell(day.and_then(|d| d.gust_max).map_or("–".into(), |g| {
                                format!("{} {}", num(Some(g), 0), u.wind)
                            }))
                            .color(p.wind),
                            cell(day.and_then(|d| d.rain).map_or("–".into(), |r| {
                                format!("{} {}", num(Some(r), 1), u.rain)
                            }))
                            .color(p.rain),
                        ]
                    })
                    .collect();
                let cols = [
                    col("Lowest at", 170.0, false),
                    col("Fall", 0.0, true),
                    col("Over", 0.0, true),
                    col("From and to", 0.0, true),
                    col("Peak gust that day", 0.0, true),
                    col("Rain that day", 0.0, true),
                ];
                ui::table(ui, &cols, &rows, 14.0);
            });
            return;
        }
        if c.falls.is_empty() {
            return;
        }
        ui::panel(ui, |ui| {
            ui::h2(ui, "Biggest pressure falls");
            ui::para(
                ui,
                "Your stormiest days: the deepest drops from one day's highest pressure to the next day's lowest. Falls a day or two apart count once, as one storm. Once Isobar has MX's detailed logs, this shows true 24-hour falls.",
                theme::body(14.0),
                p.ink2,
                760.0,
            );
            let digits = if an::hpa_per_unit(&u.press) > 5.0 {
                2
            } else {
                1
            };
            let rows: Vec<Vec<ui::Cell>> =
                c.falls
                    .iter()
                    .map(|f| {
                        vec![
                            cell(nice_date(f.date)).strong(),
                            cell(format!("{} {}", num(Some(f.fall()), digits), u.press))
                                .color(p.heat),
                            cell(num(Some(f.from), digits)),
                            cell(num(Some(f.to), digits)),
                            cell(f.gust.map_or("–".into(), |g| {
                                format!("{} {}", num(Some(g), 0), u.wind)
                            }))
                            .color(p.wind),
                            cell(f.rain.map_or("–".into(), |r| {
                                format!("{} {}", num(Some(r), 1), u.rain)
                            }))
                            .color(p.rain),
                        ]
                    })
                    .collect();
            let cols = [
                col("Day", 130.0, false),
                col("Fall", 0.0, true),
                col("From", 0.0, true),
                col("To", 0.0, true),
                col("Peak gust", 0.0, true),
                col("Rain", 0.0, true),
            ];
            ui::table(ui, &cols, &rows, 14.0);
        });
    }

    fn climate(&self, ui: &mut Ui, snap: &Snapshot, first: NaiveDate, last: NaiveDate) {
        let p = Palette::of(ui.ctx());
        let u = snap.units();
        let Some(c) = self.cache.as_ref() else { return };
        let clim = &c.climate;
        ui::panel(ui, |ui| {
            ui::h2(ui, "Your station's climate");
            ui::muted(
                ui,
                format!(
                    "Monthly averages from {} to {}. Rain figures only count months with at least 25 days of data.",
                    nice_date(first),
                    nice_date(last)
                ),
            );
            let row = |label: &str,
                       values: Vec<String>,
                       color: egui::Color32,
                       tips: Option<Vec<String>>| {
                let mut r = vec![cell(label).strong().color(p.ink2)];
                for (i, v) in values.into_iter().enumerate() {
                    let mut c = cell(v).color(color);
                    if let Some(t) = tips
                        .as_ref()
                        .and_then(|t| t.get(i))
                        .filter(|t| !t.is_empty())
                    {
                        c = c.tooltip(t.clone());
                    }
                    r.push(c);
                }
                r
            };
            let date_tips = |f: &dyn Fn(&MonthClimate) -> Option<NaiveDate>| {
                clim.iter()
                    .map(|c| f(c).map(nice_date).unwrap_or_default())
                    .collect::<Vec<_>>()
            };
            let rows = vec![
                row(
                    "Average max",
                    clim.iter().map(|c| num(c.mean_max, 1)).collect(),
                    p.heat,
                    None,
                ),
                row(
                    "Average min",
                    clim.iter().map(|c| num(c.mean_min, 1)).collect(),
                    p.cold,
                    None,
                ),
                row(
                    "Highest",
                    clim.iter()
                        .map(|c| num(c.record_high.as_ref().map(|e| e.value), 1))
                        .collect(),
                    p.ink,
                    Some(date_tips(&|c| c.record_high.as_ref().map(|e| e.date))),
                ),
                row(
                    "Lowest",
                    clim.iter()
                        .map(|c| num(c.record_low.as_ref().map(|e| e.value), 1))
                        .collect(),
                    p.ink,
                    Some(date_tips(&|c| c.record_low.as_ref().map(|e| e.date))),
                ),
                row(
                    &format!("Rain ({})", u.rain),
                    clim.iter().map(|c| num(c.mean_rain, 1)).collect(),
                    p.rain,
                    None,
                ),
                row(
                    "Rain days",
                    clim.iter().map(|c| num(c.mean_rain_days, 0)).collect(),
                    p.ink,
                    None,
                ),
                row(
                    "Years",
                    clim.iter().map(|c| c.years.to_string()).collect(),
                    p.ink2,
                    None,
                ),
            ];
            let mut cols = vec![col("", 120.0, false)];
            cols.extend(MONTHS.iter().map(|m| col(m, 0.0, true)));
            ui::table(ui, &cols, &rows, 14.0);
        });
    }

    fn wind(&mut self, ui: &mut Ui, last: NaiveDate) {
        let p = Palette::of(ui.ctx());
        let Some(c) = self.cache.as_ref() else { return };
        let mut by_season = self.by_season;
        ui::panel(ui, |ui| {
            ui.horizontal(|ui| {
                ui::h2(ui, "Prevailing wind");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if ui::chip(ui, "By season", by_season).clicked() {
                        by_season = true;
                    }
                    if ui::chip(ui, "By year", !by_season).clicked() {
                        by_season = false;
                    }
                });
            });
            let top = |r: &([usize; 16], usize)| {
                if r.1 == 0 {
                    "–"
                } else {
                    COMPASS16[(0..16).max_by_key(|i| r.0[*i]).unwrap_or(0)]
                }
            };
            if by_season {
                ui::muted(
                    ui,
                    "Share of days by dominant wind direction in each season, across every year on record.",
                );
                let roses: Vec<&([usize; 16], usize)> = c.rose_seasons.iter().collect();
                let max = dial::shared_max(&roses);
                // Start the row with summer.
                let first = if c.hemisphere == Hemisphere::South {
                    0
                } else {
                    2
                };
                ui::columns(ui, 4, 18.0, false, |i, ui| {
                    let k = (first + i) % 4;
                    let rose = &c.rose_seasons[k];
                    let name = se::season_name([12, 3, 6, 9][k], c.hemisphere);
                    ui.vertical_centered(|ui| {
                        dial::wind_rose(ui, 200.0, &rose.0, rose.1, max);
                        ui::small(ui, format!("{name}, mostly {}", top(rose)), p.ink2);
                    });
                });
                return;
            }
            ui::muted(ui, "Share of days by dominant wind direction.");
            let max = dial::shared_max(&[&c.rose_all, &c.rose_year]);
            ui::columns(ui, 2, 24.0, false, |i, ui| {
                ui.vertical_centered(|ui| {
                    let (rose, label) = if i == 0 {
                        (
                            &c.rose_all,
                            format!("All time, mostly {}", top(&c.rose_all)),
                        )
                    } else {
                        (
                            &c.rose_year,
                            format!("{}, mostly {}", last.year(), top(&c.rose_year)),
                        )
                    };
                    dial::wind_rose(ui, 250.0, &rose.0, rose.1, max);
                    ui::small(ui, label, p.ink2);
                });
            });
        });
        self.by_season = by_season;
    }

    fn gust_factor(&self, ui: &mut Ui, snap: &Snapshot) {
        let p = Palette::of(ui.ctx());
        let Some(c) = self.cache.as_ref() else { return };
        let u = snap.units();
        ui::panel(ui, |ui| {
            ui::h2(ui, "Gust factor");
            ui::muted(
                ui,
                format!(
                    "Each day's peak gust over its highest average speed. Around 1.4 is usual in open country; trees, buildings, hills and squally storms push it higher. Days with an average under {} {} are left out, as light air makes the ratio meaningless.",
                    num(Some(min_average(&u.wind)), 0),
                    u.wind
                ),
            );
            let (months, all) = &c.gust_months;
            if all.is_none() {
                ui::text(
                    ui,
                    "Not enough windy days on record yet.",
                    theme::body(15.0),
                    p.ink,
                );
                return;
            }
            let ranked: Vec<(usize, f64)> = months
                .iter()
                .enumerate()
                .filter_map(|(m, v)| v.map(|v| (m, v)))
                .collect();
            let gustiest = ranked.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1));
            let steadiest = ranked.iter().copied().min_by(|a, b| a.1.total_cmp(&b.1));
            let name = |m: Option<(usize, f64)>| {
                m.map_or("–".to_string(), |m| {
                    crate::format::MONTHS_LONG[m.0].to_string()
                })
            };
            let factor =
                |m: Option<(usize, f64)>| m.map_or("–".to_string(), |m| format!("{:.2}", m.1));
            ui::stats(
                ui,
                &[
                    ui::Stat::new("Typical day", format!("{:.2}", all.unwrap_or(0.0)), p.ink)
                        .detail(format!("Median of {} windy days", c.gusts.len()), p.ink2),
                    ui::Stat::new("Gustiest month", name(gustiest), p.wind)
                        .detail(format!("Median {}", factor(gustiest)), p.ink2),
                    ui::Stat::new("Steadiest month", name(steadiest), p.ink)
                        .detail(format!("Median {}", factor(steadiest)), p.ink2),
                ],
            );
            let xs: Vec<f64> = (0..12).map(|i| i as f64).collect();
            Chart::new(
                "gust-months",
                200.0,
                xs,
                XAxis::Index {
                    splits: None,
                    label: Box::new(|i| MONTHS[(i.round() as usize).min(11)].to_string()),
                },
            )
            .title(|i| crate::format::MONTHS_LONG[(i.round() as usize).min(11)].to_string())
            .series(
                Series::line("Median gust factor", months.to_vec(), p.wind)
                    .bars()
                    .unit("", 2),
            )
            .series(
                Series::line("All months", vec![*all; 12], p.ink2)
                    .dashed()
                    .no_points()
                    .width(1.5)
                    .unit("", 2),
            )
            .show(ui);
            let mut top: Vec<an::GustDay> = c.gusts.clone();
            top.sort_by(|a, b| b.factor.total_cmp(&a.factor));
            top.truncate(5);
            if !top.is_empty() {
                let list: Vec<String> = top
                    .iter()
                    .map(|g| {
                        format!(
                            "{} ({:.2}: {} gusting {} {})",
                            nice_date(g.date),
                            g.factor,
                            num(Some(g.average), 0),
                            num(Some(g.gust), 0),
                            u.wind
                        )
                    })
                    .collect();
                ui::small(ui, format!("Gustiest days: {}.", list.join("; ")), p.ink2);
            }
        });
    }
}

/// The lightest highest-average that counts for the gust factor: about
/// 10 km/h in the station's units.
fn min_average(wind_unit: &str) -> f64 {
    match wind_unit.trim().to_ascii_lowercase().as_str() {
        "m/s" => 3.0,
        "mph" => 6.0,
        "kts" | "kt" | "knots" => 5.0,
        _ => 10.0,
    }
}
