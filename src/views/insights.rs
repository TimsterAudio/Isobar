//! Insights: unusual days measured against the seasonal normal for that date,
//! a records board, a monthly climate table and prevailing-wind roses.

use chrono::{Datelike, Duration, NaiveDate};
use eframe::egui::{self, vec2, Align, Layout, Ui};
use isobar_core::analytics::{
    self as an, Anomaly, Field, MonthClimate, RecordRow, RecordUnit, RecordWhen, Tone, COMPASS16,
};

use crate::app::{Route, Snapshot};
use crate::dial;
use crate::format::{self, nice_date, num, signed, MONTHS};
use crate::theme::{self, Palette};
use crate::ui::{self, cell, col};

struct Cache {
    key: (u64, usize, bool),
    anomalies: Option<Vec<Anomaly>>,
    records: Vec<RecordRow>,
    climate: Vec<MonthClimate>,
    rose_all: ([usize; 16], usize),
    rose_year: ([usize; 16], usize),
}

pub struct Insights {
    window: usize,
    this_year: bool,
    cache: Option<Cache>,
}

impl Default for Insights {
    fn default() -> Self {
        Insights {
            window: 90,
            this_year: false,
            cache: None,
        }
    }
}

impl Insights {
    pub fn ui(&mut self, ui: &mut Ui, snap: &Snapshot, go: &mut Option<Route>) {
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
        self.climate(ui, snap, days[0].date, last);
        self.wind(ui, last);
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

    fn wind(&self, ui: &mut Ui, last: NaiveDate) {
        let p = Palette::of(ui.ctx());
        let Some(c) = self.cache.as_ref() else { return };
        ui::panel(ui, |ui| {
            ui::h2(ui, "Prevailing wind");
            ui::muted(ui, "Share of days by dominant wind direction.");
            let max = dial::shared_max(&[&c.rose_all, &c.rose_year]);
            let top = |r: &([usize; 16], usize)| {
                if r.1 == 0 {
                    "–"
                } else {
                    COMPASS16[(0..16).max_by_key(|i| r.0[*i]).unwrap_or(0)]
                }
            };
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
    }
}
