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
    range: Range,
    /// The chosen dates for `Range::Dates`, once picked.
    dates: Option<(NaiveDate, NaiveDate)>,
    model: Option<Model>,
}

/// What the level chart covers.
#[derive(Clone, Copy, PartialEq, Eq, Default, Hash, Debug)]
enum Range {
    Months3,
    #[default]
    Months12,
    All,
    Dates,
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
                    let over30: f64 = last30.iter().map(|d| d.overflow).sum();
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
                            .detail(
                                if over30 > 0.0 {
                                    format!("{} L used, {} L overflowed", litres(used30), litres(over30))
                                } else {
                                    format!("{} L used, none overflowed", litres(used30))
                                },
                                p.ink2,
                            ),
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
                    if !o.overflow.is_empty() {
                        let mut v = o.overflow.clone();
                        v.sort_by(f64::total_cmp);
                        let mid = v[v.len() / 2];
                        let (lo, hi) = (v[0], v[v.len() - 1]);
                        let text = if hi <= 0.0 {
                            "In the year ahead, none of the past years' rain would fill the tanks to overflowing.".to_string()
                        } else {
                            format!(
                                "In the year ahead, the past years' rain would overflow about {} L (from {} L to {} L), water the tanks can't hold.",
                                litres(mid),
                                litres(lo),
                                litres(hi)
                            )
                        };
                        ui::small(ui, text, p.ink2);
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
        let mut range = self.range;
        let Some(m) = self.model.as_ref() else { return };
        let first_day = m.sim.first().map_or(m.at, |d| d.date);
        let (mut from, mut to) = self.dates.unwrap_or((m.at - Duration::days(90), m.at));
        ui::panel(ui, |ui| {
            ui.horizontal(|ui| {
                ui::h2(ui, "Level");
                ui.with_layout(
                    eframe::egui::Layout::right_to_left(eframe::egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for (r, label) in [
                            (Range::Dates, "Dates"),
                            (Range::All, "Everything"),
                            (Range::Months12, "12 months"),
                            (Range::Months3, "3 months"),
                        ] {
                            if ui::chip(ui, label, range == r).clicked() {
                                range = r;
                            }
                        }
                    },
                );
            });
            if range == Range::Dates {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let years = first_day.year()..=m.at.year() + 1;
                    ui::small(ui, "From", p.ink2);
                    ui::date_input(ui, "tank-from", &mut from, years.clone());
                    ui::small(ui, "to", p.ink2);
                    ui::date_input(ui, "tank-to", &mut to, years);
                });
                if to < from {
                    std::mem::swap(&mut from, &mut to);
                }
            }
            // The days of history shown, and how far ahead to look.
            let (start, end, ahead_days) = match range {
                Range::Months3 => (m.at - Duration::days(91), m.at, 90),
                Range::Months12 => (m.at - Duration::days(365), m.at, AHEAD),
                Range::All => (first_day, m.at, AHEAD),
                Range::Dates => (
                    from.min(m.at),
                    to.min(m.at),
                    (to - m.at).num_days().clamp(0, AHEAD as i64) as usize,
                ),
            };
            let start = start.max(first_day).min(end);
            ui::para(
                ui,
                if ahead_days > 0 {
                    "The level day by day, then ahead: with no rain, and with the rain of each past year from this date (the shaded range, with the middle of them dashed).".to_string()
                } else {
                    format!(
                        "The level day by day. Choose an end date after {} to see the outlook too.",
                        short_date(m.at)
                    )
                },
                theme::body(14.0),
                p.ink2,
                760.0,
            );
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                ui::key(ui, "Level", p.trace, KeyMark::Line);
                if ahead_days > 0 {
                    ui::key(ui, "Past years' rain", alpha(p.rain, 0.4), KeyMark::Square);
                    ui::key(ui, "Middle of them", p.ink2, KeyMark::Dashed);
                    ui::key(ui, "No rain", p.heat, KeyMark::Dashed);
                }
                ui::key(ui, "Overflow (right)", alpha(p.rain, 0.6), KeyMark::Square);
                if t.keep_above > 0.0 {
                    ui::key(ui, "Keep above", p.warn, KeyMark::Dashed);
                }
            });
            let hist: Vec<&TankDay> = m
                .sim
                .iter()
                .filter(|d| d.date >= start && d.date <= end)
                .collect();
            let n = (end - start).num_days() as usize + 1 + ahead_days;
            let dates: Vec<NaiveDate> = (0..n).map(|k| start + Duration::days(k as i64)).collect();
            let xs: Vec<f64> = dates.iter().map(|d| chart::day_ts(*d)).collect();
            let split = n - ahead_days;
            let mut level = vec![None; n];
            for d in &hist {
                level[(d.date - start).num_days() as usize] = Some(d.level);
            }
            if end == m.at {
                level[split - 1] = Some(m.level);
            }
            let mut spill = vec![None; n];
            for d in hist.iter().filter(|d| d.overflow > 0.0) {
                spill[(d.date - start).num_days() as usize] = Some(d.overflow);
            }
            let ahead = |v: &[f64]| -> Vec<Option<f64>> {
                let mut out = vec![None; n];
                if ahead_days > 0 && !v.is_empty() {
                    out[split - 1] = Some(m.level);
                    for (k, x) in v.iter().take(ahead_days).enumerate() {
                        out[split + k] = Some(*x);
                    }
                }
                out
            };
            let o = &m.outlook;
            let mut c = Chart::new(("tank-level", range, start, end), 300.0, xs, XAxis::Time)
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
                    Series::line("Overflow", spill, p.rain)
                        .bars()
                        .right()
                        .unit(" L", 0),
                )
                .right_from_zero()
                .right_ticks(|v, _| {
                    if v == 0.0 {
                        "0".into()
                    } else {
                        format!("{}k", num(Some(v / 1000.0), 0))
                    }
                })
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
        self.range = range;
        if range == Range::Dates {
            self.dates = Some((from, to));
        }
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

/// The tanks side by side, each as wide as its share of the water, filled to
/// the level (linked, so all to the same fraction), with the keep-above line.
fn tank_drawing(ui: &mut Ui, p: &Palette, t: &Setup, level: f64) {
    let cap = t.capacity();
    let biggest = t.tanks.iter().map(|k| k.litres).fold(0.0, f64::max);
    let widths: Vec<f32> = t
        .tanks
        .iter()
        .map(|k| (90.0 * (k.litres / biggest).sqrt()).max(40.0) as f32)
        .collect();
    let gap = 14.0;
    let h = 170.0;
    let w = widths.iter().sum::<f32>() + gap * widths.len().saturating_sub(1) as f32;
    let (r, _) = ui.allocate_exact_size(vec2(w.max(120.0), h + 40.0), Sense::hover());
    let painter = ui.painter_at(r.expand(2.0));
    let frac = (level / cap).clamp(0.0, 1.0) as f32;
    let keep = (t.keep_above / cap).clamp(0.0, 1.0) as f32;
    let mut x = r.left();
    for (k, tw) in t.tanks.iter().zip(&widths) {
        let body = Rect::from_min_size(pos2(x, r.top() + 6.0), vec2(*tw, h));
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
        painter.text(
            pos2(body.center().x, r.top() + h + 16.0),
            Align2::CENTER_TOP,
            format!("{} L", litres(k.litres)),
            theme::body(13.0),
            p.ink2,
        );
        x += tw + gap;
    }
}
