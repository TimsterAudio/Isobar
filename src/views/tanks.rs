//! Tanks: how full the rainwater tanks are, what the roof caught and the
//! house used, and how long the water lasts, modelled from the rain.

use chrono::{Datelike, Duration, Local, NaiveDate};
use eframe::egui::{pos2, vec2, Align2, Rect, Sense, Stroke, StrokeKind, Ui};
use isobar_core::seasons::is_inches;
use isobar_core::tanks::{self as tk, Outlook, TankDay, TankYear, Tanks as Setup};

use crate::app::{Route, Snapshot};
use crate::chart::{self, Chart, Series, XAxis};
use crate::format::{nice_date, num, short_date};
use crate::settings::Settings;
use crate::theme::{self, alpha, Palette};
use crate::ui::{self, cell, col, KeyMark, Stat};

/// Days the outlook looks ahead.
const AHEAD: usize = 365;

/// "38,650".
pub fn litres(v: f64) -> String {
    let n = v.round() as i64;
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

struct Model {
    key: (u64, u64, String, bool),
    sim: Vec<TankDay>,
    /// The level now and the day it's for.
    level: f64,
    at: NaiveDate,
    outlook: Outlook,
    years: Vec<TankYear>,
    /// Days of the model the station's rain covers, and the record's.
    from_station: usize,
    from_record: usize,
}

#[derive(Default)]
pub struct TanksView {
    all_years: bool,
    model: Option<Model>,
}

impl TanksView {
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        snap: &Snapshot,
        settings: &Settings,
        go: &mut Option<Route>,
    ) {
        let p = Palette::of(ui.ctx());
        ui::h1(ui, "Tanks");
        let Some(t) = settings.tanks.as_ref().filter(|t| t.is_set_up()) else {
            ui::panel(ui, |ui| {
                if ui::empty(
                    ui,
                    "No tanks set up",
                    "If your house runs on rainwater, add your roof area and tanks in Settings. Isobar then works out how full they are from the rain, and how long the water lasts.",
                    Some("Open settings"),
                ) {
                    *go = Some(Route::Settings);
                }
            });
            return;
        };
        let inches = is_inches(&snap.units());
        let key = (
            snap.days_version,
            snap.tank_version,
            serde_json::to_string(t).unwrap_or_default(),
            inches,
        );
        if self.model.as_ref().is_none_or(|m| m.key != key) {
            let inputs = tk::inputs(&snap.days, &snap.tank_log, inches);
            let sim = tk::simulate(&inputs, t);
            let (mut level, mut at) = sim
                .last()
                .map_or((t.capacity(), Local::now().date_naive()), |d| {
                    (d.level, d.date)
                });
            // A level known for a day after the last rain on record.
            if let Some(k) = t
                .known
                .filter(|k| sim.last().is_none_or(|d| k.date > d.date))
            {
                (level, at) = (k.litres, k.date);
            }
            self.model = Some(Model {
                outlook: tk::outlook(&inputs, at, level, t, AHEAD),
                years: tk::years(&sim, t.keep_above),
                from_station: inputs.iter().filter(|i| i.station).count(),
                from_record: inputs
                    .iter()
                    .filter(|i| !i.station && i.rain.is_some())
                    .count(),
                key,
                sim,
                level,
                at,
            });
        }
        let Some(m) = self.model.as_ref() else { return };
        if m.sim.is_empty() && t.known.is_none() {
            ui::panel(ui, |ui| {
                ui::text(ui, "No rain on record yet. The tanks fill in once Isobar has your station's history, or a tank record you've imported in Settings.", theme::body(15.0), p.ink);
            });
            return;
        }
        Self::now(ui, &p, t, m);
        self.level_chart(ui, &p, t);
        if let Some(m) = self.model.as_ref() {
            Self::year_table(ui, &p, t, m);
        }
    }

    fn now(ui: &mut Ui, p: &Palette, t: &Setup, m: &Model) {
        let cap = t.capacity();
        let today = Local::now().date_naive();
        ui::panel(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 32.0;
                tank_drawing(ui, p, t, m.level);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 12.0;
                    let o = &m.outlook;
                    let days_left = |d: NaiveDate| (d - today).num_days().max(0);
                    let keep = if t.keep_above > 0.0 {
                        format!("{} L", litres(t.keep_above))
                    } else {
                        "empty".into()
                    };
                    let past = o.years.len();
                    let low: Vec<NaiveDate> = o.years.iter().filter_map(|y| y.1).collect();
                    let this_year = m.years.last().filter(|y| y.year == today.year());
                    let last30: Vec<&TankDay> = m.sim.iter().rev().take(30).collect();
                    let caught30: f64 = last30.iter().map(|d| d.caught).sum();
                    let used30: f64 = last30.iter().map(|d| d.used).sum();
                    let stats = vec![
                        Stat::new("In the tanks", format!("{} L", litres(m.level)), p.rain).detail(
                            format!("{}% full at the end of {}", num(Some(m.level / cap * 100.0), 0), short_date(m.at)),
                            p.ink2,
                        ),
                        match o.dry_low {
                            Some(d) => Stat::new("With no rain", format!("{} days", days_left(d)), p.ink)
                                .detail(format!("Down to {keep} on {}", nice_date(d)), p.ink2),
                            None if t.use_per_day <= 0.0 => Stat::new("With no rain", "–", p.ink)
                                .detail("Set the daily use in Settings", p.ink2),
                            None => Stat::new("With no rain", "Over a year", p.ok)
                                .detail(format!("Stays above {keep}"), p.ink2),
                        },
                        if past == 0 {
                            Stat::new("With rain like past years", "–", p.ink)
                                .detail("Needs a full year of rain on record", p.ink2)
                        } else if low.is_empty() {
                            Stat::new("With rain like past years", "Lasts", p.ok).detail(
                                format!("Stays above {keep} for a year in all {past} years' weather"),
                                p.ink2,
                            )
                        } else {
                            let earliest = low.iter().min().copied().unwrap();
                            Stat::new(
                                "With rain like past years",
                                format!("Low in {} of {past}", low.len()),
                                p.warn,
                            )
                            .detail(format!("Earliest down to {keep}: {}", nice_date(earliest)), p.ink2)
                        },
                        Stat::new("Last 30 days", format!("{} L in", litres(caught30)), p.rain)
                            .detail(format!("{} L used", litres(used30)), p.ink2),
                    ];
                    ui::stats(ui, &stats);
                    if let Some(y) = this_year {
                        ui::small(
                            ui,
                            format!(
                                "So far this year the roof caught {} L from {} mm of rain; {} L overflowed and the house used {} L.",
                                litres(y.caught),
                                num(Some(y.rain), 1),
                                litres(y.overflow),
                                litres(y.used)
                            ),
                            p.ink2,
                        );
                    }
                    let from = match t.known {
                        Some(k) => format!("Worked out from the rain since the known level on {} ({} L).", nice_date(k.date), litres(k.litres)),
                        None => "Worked out from the rain, starting full at the first day on record. Set a known level in Settings to anchor it.".into(),
                    };
                    ui::small(ui, from, p.ink2);
                });
            });
        });
    }

    fn level_chart(&mut self, ui: &mut Ui, p: &Palette, t: &Setup) {
        let mut all = self.all_years;
        let Some(m) = self.model.as_ref() else { return };
        ui::panel(ui, |ui| {
            ui.horizontal(|ui| {
                ui::h2(ui, "Level");
                ui.with_layout(
                    eframe::egui::Layout::right_to_left(eframe::egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if ui::chip(ui, "Everything", all).clicked() {
                            all = true;
                        }
                        if ui::chip(ui, "Last year and next", !all).clicked() {
                            all = false;
                        }
                    },
                );
            });
            ui::para(
                ui,
                "The level day by day, then a year ahead: with no rain, and with the rain of each past year from this date (the shaded range, with the middle of them dashed).",
                theme::body(14.0),
                p.ink2,
                760.0,
            );
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                ui::key(ui, "Level", p.trace, KeyMark::Line);
                ui::key(ui, "Past years' rain", alpha(p.rain, 0.4), KeyMark::Square);
                ui::key(ui, "Middle of them", p.ink2, KeyMark::Dashed);
                ui::key(ui, "No rain", p.heat, KeyMark::Dashed);
                if t.keep_above > 0.0 {
                    ui::key(ui, "Keep above", p.warn, KeyMark::Dashed);
                }
            });
            let hist: Vec<&TankDay> = if all {
                m.sim.iter().collect()
            } else {
                m.sim
                    .iter()
                    .filter(|d| (m.at - d.date).num_days() < 365)
                    .collect()
            };
            let first = hist.first().map_or(m.at, |d| d.date);
            let n = (m.at - first).num_days() as usize + 1 + AHEAD;
            let dates: Vec<NaiveDate> = (0..n).map(|k| first + Duration::days(k as i64)).collect();
            let xs: Vec<f64> = dates.iter().map(|d| chart::day_ts(*d)).collect();
            let split = n - AHEAD;
            let mut level = vec![None; n];
            for d in &hist {
                level[(d.date - first).num_days() as usize] = Some(d.level);
            }
            level[split - 1] = Some(m.level);
            let ahead = |v: &[f64]| -> Vec<Option<f64>> {
                let mut out = vec![None; n];
                if !v.is_empty() {
                    out[split - 1] = Some(m.level);
                    for (k, x) in v.iter().enumerate() {
                        out[split + k] = Some(*x);
                    }
                }
                out
            };
            let o = &m.outlook;
            let mut c = Chart::new(("tank-level", all), 300.0, xs, XAxis::Time)
                .left_from_zero()
                .left_ticks(|v, _| {
                    if v == 0.0 {
                        "0".into()
                    } else {
                        format!("{}k", num(Some(v / 1000.0), 0))
                    }
                })
                .title(move |x| nice_date(chart::ts_day(x)))
                .marker(chart::day_ts(m.at), "Now")
                .series(Series::edge("Lowest", ahead(&o.low)))
                .series(Series::edge("Highest", ahead(&o.high)))
                .band(0, 1, alpha(p.rain, 0.14))
                .series(
                    Series::line("Middle of past years", ahead(&o.mid), p.ink2)
                        .width(1.5)
                        .dashed()
                        .no_points()
                        .unit(" L", 0),
                )
                .series(
                    Series::line("No rain", ahead(&o.dry), p.heat)
                        .width(1.5)
                        .dashed()
                        .no_points()
                        .unit(" L", 0),
                )
                .series(
                    Series::line("Level", level, p.trace)
                        .width(2.0)
                        .no_points()
                        .unit(" L", 0),
                )
                .zoomable();
            if t.keep_above > 0.0 {
                c = c.series(
                    Series::line("Keep above", vec![Some(t.keep_above); n], p.warn)
                        .width(1.0)
                        .dashed()
                        .no_points()
                        .hidden_in_readout(),
                );
            }
            c = c.series(
                Series::line("Capacity", vec![Some(t.capacity()); n], alpha(p.ink2, 0.5))
                    .width(1.0)
                    .no_points()
                    .hidden_in_readout(),
            );
            c.show(ui);
        });
        self.all_years = all;
    }

    fn year_table(ui: &mut Ui, p: &Palette, t: &Setup, m: &Model) {
        ui::panel(ui, |ui| {
            ui::h2(ui, "Each year");
            let mut rows = Vec::new();
            for y in m.years.iter().rev() {
                rows.push(vec![
                    cell(if y.days < 365 {
                        format!("{} (part)", y.year)
                    } else {
                        y.year.to_string()
                    })
                    .strong(),
                    cell(num(Some(y.rain), 0)).color(p.rain),
                    cell(litres(y.caught)),
                    cell(litres(y.used)),
                    cell(litres(y.overflow)).color(p.rain),
                    cell(litres(y.lowest.1)).color(if y.lowest.1 <= t.keep_above {
                        p.heat
                    } else {
                        p.ink
                    }),
                    cell(short_date(y.lowest.0)).color(p.ink2),
                    cell(if y.days_low == 0 {
                        "–".to_string()
                    } else {
                        y.days_low.to_string()
                    })
                    .color(if y.days_low > 0 { p.heat } else { p.ink2 }),
                ]);
            }
            let cols = [
                col("Year", 110.0, false),
                col("Rain (mm)", 0.0, true),
                col("Caught (L)", 0.0, true),
                col("Used (L)", 0.0, true),
                col("Overflow (L)", 0.0, true),
                col("Lowest (L)", 0.0, true),
                col("On", 0.0, true),
                col("Days low", 0.0, true),
            ];
            ui::table(ui, &cols, &rows, 14.0);
            ui::small(
                ui,
                format!(
                    "Rain from your station on {} days and from your tank record on {}. Days low are at or below the keep-above level. A day of 0.1 mm or less only wets the roof.",
                    litres(m.from_station as f64),
                    litres(m.from_record as f64)
                ),
                p.ink2,
            );
        });
    }
}

/// The tanks side by side, filled to the level, with the keep-above line.
fn tank_drawing(ui: &mut Ui, p: &Palette, t: &Setup, level: f64) {
    let n = t.count.clamp(1, 4) as usize;
    let tank_w = if n <= 2 { 90.0 } else { 60.0 };
    let gap = 14.0;
    let h = 170.0;
    let w = n as f32 * tank_w + (n - 1) as f32 * gap;
    let (r, _) = ui.allocate_exact_size(vec2(w.max(120.0), h + 40.0), Sense::hover());
    let painter = ui.painter_at(r.expand(2.0));
    let frac = (level / t.capacity()).clamp(0.0, 1.0) as f32;
    let keep = (t.keep_above / t.capacity()).clamp(0.0, 1.0) as f32;
    for k in 0..n {
        let x = r.left() + k as f32 * (tank_w + gap);
        let body = Rect::from_min_size(pos2(x, r.top() + 6.0), vec2(tank_w, h));
        let water = Rect::from_min_max(
            pos2(body.left(), body.bottom() - body.height() * frac),
            body.max,
        );
        painter.rect_filled(body, 10, p.faint);
        painter.rect_filled(water.shrink(1.0), 9, alpha(p.rain, 0.55));
        if keep > 0.0 {
            let y = body.bottom() - body.height() * keep;
            painter.line_segment(
                [pos2(body.left() + 4.0, y), pos2(body.right() - 4.0, y)],
                Stroke::new(1.5, p.warn),
            );
        }
        painter.rect_stroke(body, 10, Stroke::new(1.5, p.ink2), StrokeKind::Inside);
    }
    painter.text(
        pos2(r.left(), r.top() + h + 16.0),
        Align2::LEFT_TOP,
        if n > 1 {
            format!("{} × {} L", t.count, litres(t.each))
        } else {
            format!("{} L", litres(t.each))
        },
        theme::body(13.0),
        p.ink2,
    );
}
