//! Seasons: the water balance, frost, growing degree days, sunshine and day
//! length, from the daily history.

use chrono::{Datelike, Duration, Local, NaiveDate};
use eframe::egui::{pos2, vec2, Align, Align2, Layout, Rect, Sense, Stroke, Ui};
use isobar_core::analytics::{doy_index, mean_series, YearSeries, MONTH_STARTS};
use isobar_core::seasons::{self as se, FrostNormals, FrostYear, Hemisphere, MonthTotal, Span};
use isobar_core::{DayRecord, LiveReading, Units};

use crate::app::{Route, Snapshot};
use crate::chart::{Chart, Series, XAxis};
use crate::format::{self, month_year, nice_date, num, short_date, signed, MONTHS, MONTHS_LONG};
use crate::settings::Settings;
use crate::theme::{self, alpha, Palette};
use crate::ui::{self, cell, col, KeyMark, Stat};

/// Where the latitude came from.
#[derive(Clone, Copy, PartialEq)]
pub enum LatitudeFrom {
    Settings,
    Sunrise,
}

/// The station's latitude: from Settings, or else worked out from the day's
/// sunrise and sunset.
pub fn latitude(settings: &Settings, live: Option<&LiveReading>) -> Option<(f64, LatitudeFrom)> {
    if let Some(l) = settings.latitude {
        return Some((l, LatitudeFrom::Settings));
    }
    let r = live?;
    let (a, b) = (
        format::parse_hm(r.sunrise.as_deref()?)?,
        format::parse_hm(r.sunset.as_deref()?)?,
    );
    let hours = ((b.0 * 60 + b.1) as f64 - (a.0 * 60 + a.1) as f64) / 60.0;
    se::estimate_latitude(Local::now().date_naive(), hours).map(|l| (l, LatitudeFrom::Sunrise))
}

/// "33.9° S".
fn lat_text(l: f64) -> String {
    format!("{:.1}° {}", l.abs(), if l < 0.0 { "S" } else { "N" })
}

/// "13 h 09 min".
fn hours_text(h: f64) -> String {
    let m = (h * 60.0).round() as i64;
    format!("{} h {:02} min", m / 60, m % 60)
}

/// "2025–26" for a year starting mid-year, else "2026".
fn year_label(start: NaiveDate) -> String {
    if start.month() == 1 {
        start.year().to_string()
    } else {
        format!("{}–{:02}", start.year(), (start.year() + 1) % 100)
    }
}

/// "3rd".
fn ordinal(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// An axis for a period from `start`: month names at each month's start, or
/// day numbers for a single month.
fn period_axis(start: NaiveDate, len: usize) -> XAxis {
    if len <= 31 {
        return XAxis::Index {
            splits: None,
            label: Box::new(|i| format!("{}", i.round() as i64 + 1)),
        };
    }
    let mut splits = Vec::new();
    let mut names = Vec::new();
    let mut m = start;
    while ((m - start).num_days() as usize) < len {
        splits.push((m - start).num_days() as f64);
        names.push(MONTHS[m.month0() as usize]);
        m = m.checked_add_months(chrono::Months::new(1)).unwrap_or(m);
        if m == start {
            break;
        }
    }
    let at = splits.clone();
    XAxis::Index {
        splits: Some(splits),
        label: Box::new(move |x| {
            at.iter()
                .position(|s| *s == x)
                .map_or(String::new(), |i| names[i].to_string())
        }),
    }
}

/// The value at `i`, or the last one before it.
fn at_or_before(v: &[Option<f64>], i: usize) -> Option<f64> {
    v.get(..=i.min(v.len().saturating_sub(1)))?
        .iter()
        .rev()
        .find_map(|x| *x)
}

struct Water {
    start: NaiveDate,
    label: String,
    len: usize,
    current: Option<YearSeries>,
    normal: Vec<Option<f64>>,
    lo: Vec<Option<f64>>,
    hi: Vec<Option<f64>>,
    years: usize,
    rain: f64,
    et: f64,
    upto: NaiveDate,
    estimated: bool,
}

struct Frost {
    years: Vec<FrostYear>,
    normals: FrostNormals,
    current: Option<FrostYear>,
}

struct Growing {
    start: NaiveDate,
    len: usize,
    series: Vec<YearSeries>,
    normal: Vec<Option<f64>>,
    upto: NaiveDate,
}

struct Sun {
    totals: Vec<MonthTotal>,
    normals: [Option<f64>; 12],
    /// Per calendar month: the sunniest and dullest full month.
    best: [Option<MonthTotal>; 12],
    worst: [Option<MonthTotal>; 12],
    /// Average sunshine for each day of the year.
    by_day: Vec<Option<f64>>,
}

struct Cache {
    key: (u64, Span, usize, usize, Option<i64>, String),
    hemi: Hemisphere,
    /// Why there's no water balance, if there isn't one.
    water: Result<Water, String>,
    frost: Frost,
    growing: Growing,
    sun: Option<Sun>,
}

pub struct Seasons {
    span: Span,
    /// 0: at or below 0°; 1: at or below 2° (or 32° and 36° F).
    frost_at: usize,
    /// 0: base 5°; 1: base 10° (or 41° and 50° F).
    base: usize,
    cache: Option<Cache>,
}

impl Default for Seasons {
    fn default() -> Self {
        Seasons {
            span: Span::Season,
            frost_at: 0,
            base: 1,
            cache: None,
        }
    }
}

fn frost_threshold(i: usize, u: &Units) -> f64 {
    match (se::is_fahrenheit(u), i) {
        (false, 0) => 0.0,
        (false, _) => 2.0,
        (true, 0) => 32.0,
        (true, _) => 36.0,
    }
}

fn gdd_base(i: usize, u: &Units) -> f64 {
    match (se::is_fahrenheit(u), i) {
        (false, 0) => 5.0,
        (false, _) => 10.0,
        (true, 0) => 41.0,
        (true, _) => 50.0,
    }
}

fn water(
    days: &[DayRecord],
    span: Span,
    hemi: Hemisphere,
    lat: Option<f64>,
    u: &Units,
) -> Result<Water, String> {
    let last = days.last().ok_or_else(String::new)?.date;
    let station_et = se::station_records_et(days);
    let estimate_at = match (station_et, lat) {
        (true, _) => None,
        (false, Some(l)) => Some(l),
        (false, None) => {
            return Err("Your station doesn't record evaporation (evapotranspiration), and Isobar needs your latitude to estimate it. Add it in Settings → Station.".into())
        }
    };
    let et = |d: &DayRecord| se::day_et(d, estimate_at, u);
    let start = span.start(last);
    let series = se::running_totals(days, start, span.months(), |d| Some(d.rain? - et(d)?));
    let current = series.iter().find(|s| s.year == start.year()).cloned();
    let normal = mean_series(&series, Some(start.year()));
    let (lo, hi) = se::spread(&series, Some(start.year()));
    let sel = isobar_core::analytics::in_range(days, start, last);
    let len = series.first().map_or(31, |s| s.values.len());
    let label = match span {
        Span::Month => month_year(start),
        Span::Season => {
            let name = se::season_name(start.month(), hemi);
            if start.month() == 12 {
                format!(
                    "{name} {}",
                    year_label(NaiveDate::from_ymd_opt(start.year(), 7, 1).unwrap())
                )
            } else {
                format!("{name} {}", start.year())
            }
        }
        Span::Year => start.year().to_string(),
    };
    Ok(Water {
        start,
        label,
        len,
        years: series.len().saturating_sub(current.is_some() as usize),
        current,
        normal,
        lo,
        hi,
        rain: sel.iter().filter_map(|d| d.rain).sum(),
        et: sel.iter().filter_map(et).sum(),
        upto: last,
        estimated: estimate_at.is_some(),
    })
}

fn sun(days: &[DayRecord]) -> Option<Sun> {
    let value = |d: &DayRecord| d.sunshine;
    let totals = se::monthly_totals(days, value);
    if !totals.iter().any(|t| t.total > 0.0) {
        return None;
    }
    let normals = se::month_normals(&totals, None);
    let mut best: [Option<MonthTotal>; 12] = Default::default();
    let mut worst: [Option<MonthTotal>; 12] = Default::default();
    for t in totals.iter().filter(|t| t.days >= se::FULL_MONTH) {
        let m = t.month.month0() as usize;
        if best[m].as_ref().is_none_or(|b| t.total > b.total) {
            best[m] = Some(t.clone());
        }
        if worst[m].as_ref().is_none_or(|b| t.total < b.total) {
            worst[m] = Some(t.clone());
        }
    }
    Some(Sun {
        normals,
        best,
        worst,
        by_day: se::doy_mean(days, 7, value),
        totals,
    })
}

impl Seasons {
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        snap: &Snapshot,
        settings: &Settings,
        go: &mut Option<Route>,
    ) {
        let p = Palette::of(ui.ctx());
        ui::h1(ui, "Seasons");
        let days = snap.days.as_slice();
        let Some(last) = days.last().map(|d| d.date) else {
            ui::panel(ui, |ui| {
                if ui::empty(
                    ui,
                    "No history yet",
                    "Water balance, frost, growing degree days and sunshine appear once Isobar has your station's daily history.",
                    Some("Open settings"),
                ) {
                    *go = Some(Route::Settings);
                }
            });
            return;
        };
        let u = snap.units();
        let lat = latitude(settings, snap.live.as_ref());
        let key = (
            snap.days_version,
            self.span,
            self.frost_at,
            self.base,
            lat.map(|l| (l.0 * 10.0).round() as i64),
            u.temp.clone() + &u.rain,
        );
        if self.cache.as_ref().is_none_or(|c| c.key != key) {
            let hemi = se::hemisphere(days, lat.map(|l| l.0));
            let fstart = se::frost_start(hemi, last);
            let years = se::frost_years(days, fstart, frost_threshold(self.frost_at, &u));
            let gstart = se::growing_start(hemi, last);
            let base = gdd_base(self.base, &u);
            let gseries = se::running_totals(days, gstart, 12, |d| se::degree_days(d, base));
            self.cache = Some(Cache {
                key,
                hemi,
                water: water(days, self.span, hemi, lat.map(|l| l.0), &u),
                frost: Frost {
                    normals: se::frost_normals(&years, Some(fstart)),
                    current: years.iter().find(|y| y.start == fstart).cloned(),
                    years,
                },
                growing: Growing {
                    start: gstart,
                    len: gseries.first().map_or(366, |s| s.values.len()),
                    normal: mean_series(&gseries, Some(gstart.year())),
                    series: gseries,
                    upto: last,
                },
                sun: sun(days),
            });
        }

        let where_from = match lat {
            Some((l, LatitudeFrom::Settings)) => format!(" (latitude {})", lat_text(l)),
            Some((l, LatitudeFrom::Sunrise)) => format!(
                " (latitude about {}, worked out from today's sunrise and sunset; set it in Settings to be exact)",
                lat_text(l)
            ),
            None => String::new(),
        };
        let hemi = self.cache.as_ref().map_or(Hemisphere::North, |c| c.hemi);
        ui::para(
            ui,
            format!(
                "Water, frost, warmth and sun through the year, from your station's daily history to {}. Seasons follow the {} hemisphere{where_from}.",
                nice_date(last),
                if hemi == Hemisphere::South { "southern" } else { "northern" },
            ),
            theme::body(14.0),
            p.ink2,
            1200.0,
        );
        self.water_panel(ui, &u, go);
        self.frost_panel(ui, &u, last);
        self.growing_panel(ui, &u);
        self.sun_panel(ui, last);
        self.day_length_panel(ui, lat, go);
    }

    fn water_panel(&mut self, ui: &mut Ui, u: &Units, go: &mut Option<Route>) {
        let p = Palette::of(ui.ctx());
        let mut span = self.span;
        let Some(c) = self.cache.as_ref() else { return };
        ui::panel(ui, |ui| {
            ui.horizontal(|ui| {
                ui::h2(ui, "Water balance");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for (s, label) in [
                        (Span::Year, "Year"),
                        (Span::Season, "Season"),
                        (Span::Month, "Month"),
                    ] {
                        if ui::chip(ui, label, span == s).clicked() {
                            span = s;
                        }
                    }
                });
            });
            let w = match &c.water {
                Ok(w) => w,
                Err(e) => {
                    ui::para(ui, e, theme::body(15.0), p.ink, 700.0);
                    if ui::button(ui, "Open settings", ui::Kind::Secondary).clicked() {
                        *go = Some(Route::Settings);
                    }
                    return;
                }
            };
            ui::para(
                ui,
                format!(
                    "Rain less what the ground and plants give back to the air. Above zero the soil is gaining water; below, it's drying out. {}",
                    if w.estimated {
                        "Your station doesn't record evaporation, so Isobar estimates it from each day's highest and lowest temperatures (the Hargreaves method)."
                    } else {
                        "Evaporation is your station's own figure."
                    }
                ),
                theme::body(14.0),
                p.ink2,
                760.0,
            );
            let unit = format!(" {}", u.rain);
            let idx = (w.upto - w.start).num_days().max(0) as usize;
            let now = w
                .current
                .as_ref()
                .and_then(|s| at_or_before(&s.values, idx));
            let normal = at_or_before(&w.normal, idx);
            let balance_color = |v: f64| if v >= 0.0 { p.rain } else { p.heat };
            let mut stats = vec![
                Stat::new("Rain", format!("{}{unit}", num(Some(w.rain), 1)), p.rain)
                    .detail(format!("{} to {}", w.label, short_date(w.upto)), p.ink2),
                Stat::new(
                    "Evaporation",
                    format!("{}{unit}", num(Some(w.et), 1)),
                    p.ink,
                )
                .detail(
                    if w.estimated {
                        "Estimated"
                    } else {
                        "From the station"
                    },
                    p.ink2,
                ),
                Stat::new(
                    "Balance",
                    format!("{}{unit}", signed(now, 1)),
                    now.map_or(p.ink, balance_color),
                )
                .detail(
                    match now {
                        Some(v) if v >= 0.0 => "More rain than evaporation",
                        Some(_) => "Drying out",
                        None => "",
                    },
                    p.ink2,
                ),
            ];
            stats.push(match normal {
                Some(n) => Stat::new(
                    "Normal by now",
                    format!("{}{unit}", signed(Some(n), 1)),
                    p.ink,
                )
                .detail(
                    match now {
                        Some(v) => format!(
                            "{}{unit} against normal, from {} years",
                            signed(Some(v - n), 1),
                            w.years
                        ),
                        None => format!("From {} years", w.years),
                    },
                    now.map_or(p.ink2, |v| if v >= n { p.rain } else { p.heat }),
                ),
                None => Stat::new("Normal by now", "–", p.ink)
                    .detail("Needs two earlier years with full records", p.ink2),
            });
            ui::stats(ui, &stats);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                ui::key(ui, &w.label, p.trace, KeyMark::Line);
                ui::key(ui, "Normal", p.ink2, KeyMark::Dashed);
                ui::key(
                    ui,
                    "Range of other years",
                    alpha(p.rain, 0.35),
                    KeyMark::Square,
                );
            });
            let xs: Vec<f64> = (0..w.len).map(|i| i as f64).collect();
            let start = w.start;
            let chart = Chart::new(("water", w.start), 280.0, xs, period_axis(w.start, w.len))
                .title(move |x| short_date(start + Duration::days(x as i64)))
                .series(Series::edge("Lowest", w.lo.clone()))
                .series(Series::edge("Highest", w.hi.clone()))
                .band(0, 1, alpha(p.rain, 0.12))
                .series(
                    Series::line("Normal", w.normal.clone(), p.ink2)
                        .width(1.5)
                        .dashed()
                        .no_points()
                        .unit(&unit, 1),
                )
                .series(
                    Series::line(
                        w.label.clone(),
                        w.current
                            .as_ref()
                            .map_or_else(Vec::new, |s| s.values.clone()),
                        p.trace,
                    )
                    .width(2.0)
                    .no_points()
                    .unit(&unit, 1),
                );
            chart.show(ui);
        });
        self.span = span;
    }

    fn frost_panel(&mut self, ui: &mut Ui, u: &Units, last: NaiveDate) {
        let p = Palette::of(ui.ctx());
        let mut at = self.frost_at;
        let Some(c) = self.cache.as_ref() else { return };
        let f = &c.frost;
        let deg = |v: f64| format!("{}{}", num(Some(v), 0), u.temp);
        ui::panel(ui, |ui| {
            ui.horizontal(|ui| {
                ui::h2(ui, "Frost");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for i in [1, 0] {
                        let label = format!("At or below {}", deg(frost_threshold(i, u)));
                        if ui::chip(ui, &label, at == i).clicked() {
                            at = i;
                        }
                    }
                });
            });
            let t = frost_threshold(self.frost_at, u);
            ui::para(
                ui,
                format!(
                    "Nights when the low reached {} or below.{}",
                    deg(t),
                    if self.frost_at == 1 {
                        " Grass and low plants often frost on nights like these even when the air stays just above freezing."
                    } else {
                        " The low is the air temperature at the sensor; the ground is often a little colder."
                    }
                ),
                theme::body(14.0),
                p.ink2,
                760.0,
            );
            let n = &f.normals;
            let in_year = |offset: Option<f64>| {
                offset
                    .zip(f.current.as_ref())
                    .map(|(o, cur)| short_date(cur.start + Duration::days(o.round() as i64)))
            };
            let cur = f.current.as_ref();
            let cur_nights = cur.map_or(0, |y| y.nights.len());
            let stats = vec![
                Stat::new(
                    "Average first frost",
                    in_year(n.first).unwrap_or("–".into()),
                    p.cold,
                )
                .detail(
                    n.earliest_first
                        .map_or(String::new(), |d| format!("Earliest {}", nice_date(d))),
                    p.ink2,
                ),
                Stat::new(
                    "Average last frost",
                    in_year(n.last).unwrap_or("–".into()),
                    p.cold,
                )
                .detail(
                    n.latest_last
                        .map_or(String::new(), |d| format!("Latest {}", nice_date(d))),
                    p.ink2,
                ),
                Stat::new("Frost nights a year", num(n.nights, 1), p.ink).detail(
                    if n.years == 0 {
                        "Needs a full year of records".to_string()
                    } else if n.frost_free > 0 {
                        format!("From {} years, {} with none", n.years, n.frost_free)
                    } else {
                        format!("From {} years", n.years)
                    },
                    p.ink2,
                ),
                Stat::new(
                    format!(
                        "This year ({})",
                        cur.map_or(String::new(), |y| year_label(y.start))
                    ),
                    format!(
                        "{cur_nights} {}",
                        if cur_nights == 1 { "night" } else { "nights" }
                    ),
                    p.ink,
                )
                .detail(
                    match cur.and_then(|y| y.first().zip(y.last())) {
                        Some((a, b)) if a == b => format!("On {}", short_date(a)),
                        Some((a, b)) => {
                            format!("First {}, latest {}", short_date(a), short_date(b))
                        }
                        None => "None so far".into(),
                    },
                    p.ink2,
                ),
            ];
            ui::stats(ui, &stats);
            if f.years.iter().all(|y| y.nights.is_empty()) {
                ui::text(
                    ui,
                    format!("No nights at or below {} on record.", deg(t)),
                    theme::body(15.0),
                    p.ink,
                );
            } else {
                frost_calendar(ui, &p, &f.years, n, last, u);
            }
        });
        self.frost_at = at;
    }

    fn growing_panel(&mut self, ui: &mut Ui, u: &Units) {
        let p = Palette::of(ui.ctx());
        let mut base = self.base;
        let Some(c) = self.cache.as_ref() else { return };
        let g = &c.growing;
        let deg = |v: f64| format!("{}{}", num(Some(v), 0), u.temp);
        ui::panel(ui, |ui| {
            ui.horizontal(|ui| {
                ui::h2(ui, "Growing degree days");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for i in [1, 0] {
                        if ui::chip(ui, &format!("Base {}", deg(gdd_base(i, u))), base == i)
                            .clicked()
                        {
                            base = i;
                        }
                    }
                });
            });
            ui::para(
                ui,
                format!(
                    "The warmth that drives growth: each day adds how far its average temperature was above {}. {} The growing year runs from 1 {}.",
                    deg(gdd_base(self.base, u)),
                    if self.base == 1 {
                        "A base of 10° suits most fruit, vegetables and vines."
                    } else {
                        "A base of 5° suits grass, cereals and cool-season crops."
                    },
                    MONTHS_LONG[g.start.month0() as usize]
                ),
                theme::body(14.0),
                p.ink2,
                760.0,
            );
            let idx = (g.upto - g.start).num_days().max(0) as usize;
            let current = g.series.iter().find(|s| s.year == g.start.year());
            let now = current.and_then(|s| at_or_before(&s.values, idx));
            let normal = at_or_before(&g.normal, idx);
            // Where this year stands among every year at the same point.
            let mut at_point: Vec<(i32, f64)> = g
                .series
                .iter()
                .filter_map(|s| s.values.get(idx).copied().flatten().map(|v| (s.year, v)))
                .collect();
            at_point.sort_by(|a, b| b.1.total_cmp(&a.1));
            let rank = at_point.iter().position(|(y, _)| *y == g.start.year());
            let last_full = g
                .series
                .iter()
                .rev()
                .find(|s| s.year < g.start.year())
                .and_then(|s| s.values.iter().rev().find_map(|v| *v).map(|v| (s.year, v)));
            let stats = vec![
                Stat::new(
                    format!("This growing year ({})", year_label(g.start)),
                    num(now, 0),
                    p.trace,
                )
                .detail(format!("To {}", short_date(g.upto)), p.ink2),
                match normal {
                    Some(n) => Stat::new("Normal by now", num(Some(n), 0), p.ink).detail(
                        now.map_or(String::new(), |v| {
                            if n > 0.0 {
                                format!("{}% of normal", num(Some(v / n * 100.0), 0))
                            } else {
                                String::new()
                            }
                        }),
                        now.map_or(p.ink2, |v| if v >= n { p.heat } else { p.cold }),
                    ),
                    None => Stat::new("Normal by now", "–", p.ink)
                        .detail("Needs two earlier years with full records", p.ink2),
                },
                Stat::new(
                    "Warmest so far",
                    rank.map_or("–".into(), |r| ordinal(r + 1)),
                    p.ink,
                )
                .detail(format!("Of {} years at this point", at_point.len()), p.ink2),
                Stat::new(
                    "Last growing year",
                    last_full.map_or("–".into(), |(_, v)| num(Some(v), 0)),
                    p.ink,
                )
                .detail(
                    last_full.map_or(String::new(), |(y, _)| {
                        format!(
                            "{} in full",
                            year_label(NaiveDate::from_ymd_opt(y, g.start.month(), 1).unwrap())
                        )
                    }),
                    p.ink2,
                ),
            ];
            ui::stats(ui, &stats);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                ui::key(ui, &year_label(g.start), p.trace, KeyMark::Line);
                ui::key(ui, "Normal", p.ink2, KeyMark::Dashed);
                ui::key(ui, "Other years", alpha(p.ink2, 0.4), KeyMark::Line);
            });
            let xs: Vec<f64> = (0..g.len).map(|i| i as f64).collect();
            let start = g.start;
            let mut chart = Chart::new(
                ("gdd", g.start, self.base),
                300.0,
                xs,
                period_axis(g.start, g.len),
            )
            .left_from_zero()
            .title(move |x| short_date(start + Duration::days(x as i64)));
            for s in g.series.iter().filter(|s| s.year != g.start.year()) {
                chart = chart.series(
                    Series::line(
                        year_label(NaiveDate::from_ymd_opt(s.year, g.start.month(), 1).unwrap()),
                        s.values.clone(),
                        alpha(p.ink2, 0.28),
                    )
                    .width(1.0)
                    .no_points()
                    .hidden_in_readout()
                    .unit("", 0),
                );
            }
            chart = chart.series(
                Series::line("Normal", g.normal.clone(), p.ink2)
                    .width(1.5)
                    .dashed()
                    .no_points()
                    .unit("", 0),
            );
            if let Some(cur) = current {
                chart = chart.series(
                    Series::line(year_label(g.start), cur.values.clone(), p.trace)
                        .width(2.0)
                        .no_points()
                        .unit("", 0),
                );
            }
            chart.show(ui);
        });
        self.base = base;
    }

    fn sun_panel(&self, ui: &mut Ui, last: NaiveDate) {
        let p = Palette::of(ui.ctx());
        let Some(c) = self.cache.as_ref() else { return };
        ui::panel(ui, |ui| {
            ui::h2(ui, "Sunshine");
            let Some(s) = c.sun.as_ref() else {
                ui::para(
                    ui,
                    "Your station doesn't record sunshine hours. Cumulus MX works them out from a solar radiation sensor.",
                    theme::body(15.0),
                    p.ink,
                    700.0,
                );
                return;
            };
            ui::para(
                ui,
                "Hours of sunshine each month, against the normal for that month.",
                theme::body(14.0),
                p.ink2,
                760.0,
            );
            let full: Vec<&MonthTotal> = s
                .totals
                .iter()
                .filter(|t| t.days >= se::FULL_MONTH)
                .collect();
            let sunniest = full.iter().max_by(|a, b| a.total.total_cmp(&b.total));
            let dullest = full.iter().min_by(|a, b| a.total.total_cmp(&b.total));
            let this = s
                .totals
                .last()
                .filter(|t| t.month == last.with_day(1).unwrap());
            let year: Vec<&MonthTotal> = full.iter().rev().take(12).copied().collect();
            let year_normal: f64 = year
                .iter()
                .filter_map(|t| s.normals[t.month.month0() as usize])
                .sum();
            let year_total: f64 = year.iter().map(|t| t.total).sum();
            let stats = vec![
                Stat::new(
                    "This month so far",
                    this.map_or("–".into(), |t| format!("{} h", num(Some(t.total), 0))),
                    p.warn,
                )
                .detail(
                    this.and_then(|t| s.normals[t.month.month0() as usize].map(|n| (t, n)))
                        .map_or(String::new(), |(t, n)| {
                            format!(
                                "{} days in; the whole month normally has {} h",
                                t.days,
                                num(Some(n), 0)
                            )
                        }),
                    p.ink2,
                ),
                Stat::new(
                    "Last 12 full months",
                    format!("{} h", num(Some(year_total), 0)),
                    p.ink,
                )
                .detail(
                    if year.len() == 12 && year_normal > 0.0 {
                        format!(
                            "{}% of normal",
                            num(Some(year_total / year_normal * 100.0), 0)
                        )
                    } else {
                        String::new()
                    },
                    p.ink2,
                ),
                Stat::new(
                    "Sunniest month",
                    sunniest.map_or("–".into(), |t| format!("{} h", num(Some(t.total), 0))),
                    p.warn,
                )
                .detail(
                    sunniest.map_or(String::new(), |t| month_year(t.month)),
                    p.ink2,
                ),
                Stat::new(
                    "Dullest month",
                    dullest.map_or("–".into(), |t| format!("{} h", num(Some(t.total), 0))),
                    p.ink,
                )
                .detail(
                    dullest.map_or(String::new(), |t| month_year(t.month)),
                    p.ink2,
                ),
            ];
            ui::stats(ui, &stats);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                ui::key(ui, "Sunshine", p.warn, KeyMark::Square);
                ui::key(ui, "Normal for the month", p.ink2, KeyMark::Dashed);
            });
            // The last two years, a bar a month.
            let recent: Vec<&MonthTotal> = s.totals.iter().rev().take(24).rev().collect();
            let xs: Vec<f64> = (0..recent.len()).map(|i| i as f64).collect();
            let months: Vec<NaiveDate> = recent.iter().map(|t| t.month).collect();
            let labels = months.clone();
            let chart = Chart::new(
                "sun-months",
                240.0,
                xs,
                XAxis::Index {
                    splits: None,
                    label: Box::new(move |i| {
                        labels.get(i.round() as usize).map_or(String::new(), |m| {
                            if m.month() == 1 {
                                format!("Jan {}", m.year())
                            } else {
                                MONTHS[m.month0() as usize].to_string()
                            }
                        })
                    }),
                },
            )
            .left_from_zero()
            .title(move |i| {
                months
                    .get(i.round() as usize)
                    .map_or(String::new(), |m| month_year(*m))
            })
            .series(
                Series::line(
                    "Sunshine",
                    recent.iter().map(|t| Some(t.total)).collect(),
                    p.warn,
                )
                .bars()
                .unit(" h", 0),
            )
            .series(
                Series::line(
                    "Normal",
                    recent
                        .iter()
                        .map(|t| s.normals[t.month.month0() as usize])
                        .collect(),
                    p.ink2,
                )
                .width(1.5)
                .dashed()
                .no_points()
                .unit(" h", 0),
            );
            chart.show(ui);

            let tip = |t: &Option<MonthTotal>| {
                t.as_ref()
                    .map_or(String::new(), |t| t.month.year().to_string())
            };
            let rows = vec![
                {
                    let mut r = vec![cell("Normal (h)").strong().color(p.ink2)];
                    r.extend(s.normals.iter().map(|n| cell(num(*n, 0)).color(p.ink)));
                    r
                },
                {
                    let mut r = vec![cell("Sunniest").strong().color(p.ink2)];
                    r.extend(s.best.iter().map(|t| {
                        cell(num(t.as_ref().map(|t| t.total), 0))
                            .color(p.warn)
                            .tooltip(tip(t))
                    }));
                    r
                },
                {
                    let mut r = vec![cell("Dullest").strong().color(p.ink2)];
                    r.extend(s.worst.iter().map(|t| {
                        cell(num(t.as_ref().map(|t| t.total), 0))
                            .color(p.ink)
                            .tooltip(tip(t))
                    }));
                    r
                },
            ];
            let mut cols = vec![col("", 120.0, false)];
            cols.extend(MONTHS.iter().map(|m| col(m, 0.0, true)));
            ui::table(ui, &cols, &rows, 14.0);
            ui::small(
                ui,
                "Months with at least 25 days of records. Point at a record to see its year.",
                p.ink2,
            );
        });
    }

    fn day_length_panel(
        &self,
        ui: &mut Ui,
        lat: Option<(f64, LatitudeFrom)>,
        go: &mut Option<Route>,
    ) {
        let p = Palette::of(ui.ctx());
        let Some(c) = self.cache.as_ref() else { return };
        ui::panel(ui, |ui| {
            ui::h2(ui, "Day length");
            let Some((lat, _)) = lat else {
                ui::para(
                    ui,
                    "Isobar needs your latitude for this. It usually works it out from the day's sunrise and sunset, but can't close to the equinoxes, when every place has about twelve hours. Add it in Settings → Station.",
                    theme::body(15.0),
                    p.ink,
                    700.0,
                );
                if ui::button(ui, "Open settings", ui::Kind::Secondary).clicked() {
                    *go = Some(Route::Settings);
                }
                return;
            };
            let sun = c.sun.as_ref();
            ui::para(
                ui,
                format!(
                    "Hours from sunrise to sunset through the year at {}{}.",
                    lat_text(lat),
                    if sun.is_some() {
                        ", with the sunshine your station records at each time of year"
                    } else {
                        ""
                    }
                ),
                theme::body(14.0),
                p.ink2,
                760.0,
            );
            let today = Local::now().date_naive();
            let len = se::day_length(lat, today);
            let change = (len - se::day_length(lat, today - Duration::days(1))) * 60.0;
            // The longest and shortest days this year.
            let year: Vec<(NaiveDate, f64)> = (0..365)
                .filter_map(|i| {
                    let d = NaiveDate::from_ymd_opt(today.year(), 1, 1)? + Duration::days(i);
                    Some((d, se::day_length(lat, d)))
                })
                .collect();
            let longest = year.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1));
            let shortest = year.iter().copied().min_by(|a, b| a.1.total_cmp(&b.1));
            let by_doy = se::day_length_year(lat);
            let share = sun.map(|s| {
                let (mut a, mut b) = (0.0, 0.0);
                for (i, v) in s.by_day.iter().enumerate() {
                    if let Some(v) = v {
                        a += v;
                        b += by_doy[i];
                    }
                }
                if b > 0.0 {
                    a / b * 100.0
                } else {
                    0.0
                }
            });
            let stats = vec![
                Stat::new("Today", hours_text(len), p.ink).detail(
                    if change.abs() < 0.5 {
                        "About the same as yesterday".to_string()
                    } else {
                        format!(
                            "{} min {} than yesterday",
                            num(Some(change.abs()), 0),
                            if change > 0.0 { "longer" } else { "shorter" }
                        )
                    },
                    p.ink2,
                ),
                Stat::new(
                    "Longest day",
                    longest.map_or("–".into(), |l| hours_text(l.1)),
                    p.warn,
                )
                .detail(longest.map_or(String::new(), |l| nice_date(l.0)), p.ink2),
                Stat::new(
                    "Shortest day",
                    shortest.map_or("–".into(), |s| hours_text(s.1)),
                    p.cold,
                )
                .detail(shortest.map_or(String::new(), |s| nice_date(s.0)), p.ink2),
                Stat::new(
                    "Sunny share of daylight",
                    share.map_or("–".into(), |s| format!("{}%", num(Some(s), 0))),
                    p.trace,
                )
                .detail(
                    if share.is_some() {
                        "Averaged over the year"
                    } else {
                        "Needs sunshine records"
                    },
                    p.ink2,
                ),
            ];
            ui::stats(ui, &stats);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                ui::key(ui, "Day length", p.ink, KeyMark::Line);
                if sun.is_some() {
                    ui::key(ui, "Average sunshine", p.trace, KeyMark::Line);
                }
            });
            let xs: Vec<f64> = (0..366).map(|i| i as f64).collect();
            let doy_date = |x: f64| {
                NaiveDate::from_ymd_opt(2000, 1, 1).unwrap() + Duration::days(x.round() as i64)
            };
            let mut chart = Chart::new(
                "day-length",
                260.0,
                xs,
                XAxis::Index {
                    splits: Some(MONTH_STARTS.iter().map(|s| *s as f64).collect()),
                    label: Box::new(|i| {
                        MONTH_STARTS
                            .iter()
                            .position(|s| *s as f64 == i)
                            .map_or(String::new(), |m| MONTHS[m].to_string())
                    }),
                },
            )
            .left_from_zero()
            .title(move |x| short_date(doy_date(x)))
            .marker(doy_index(today) as f64, "Today")
            .series(
                Series::line(
                    "Day length",
                    by_doy
                        .iter()
                        .map(|v| Some((v * 10.0).round() / 10.0))
                        .collect(),
                    p.ink,
                )
                .width(1.75)
                .no_points()
                .unit(" h", 1),
            );
            if let Some(s) = sun {
                chart = chart.series(
                    Series::line("Average sunshine", s.by_day.clone(), p.trace)
                        .fill(alpha(p.trace, 0.12))
                        .width(1.75)
                        .no_points()
                        .unit(" h", 1),
                );
            }
            chart.show(ui);
        });
    }
}

/// One row per frost year, newest at the top, a mark for each frost night,
/// with the average frost season shaded behind.
fn frost_calendar(
    ui: &mut Ui,
    p: &Palette,
    years: &[FrostYear],
    n: &FrostNormals,
    today: NaiveDate,
    u: &Units,
) {
    let label_w = 64.0;
    let row_h = 20.0;
    let head = 20.0;
    let width = ui.available_width();
    let height = head + row_h * years.len() as f32 + 4.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let painter = ui.painter_at(rect);
    let track = Rect::from_min_max(
        pos2(rect.left() + label_w, rect.top() + head),
        pos2(rect.right() - 4.0, rect.bottom() - 4.0),
    );
    let Some(first) = years.first() else { return };
    let fx = |offset: f64| track.left() + (offset / 365.0) as f32 * track.width();

    // The average season, shaded.
    if let (Some(a), Some(b)) = (n.first, n.last) {
        painter.rect_filled(
            Rect::from_x_y_ranges(fx(a)..=fx(b).max(fx(a) + 2.0), track.y_range()),
            2,
            alpha(p.cold, 0.10),
        );
    }
    // Month lines and names.
    let mut m = first.start;
    for _ in 0..12 {
        let x = fx((m - first.start).num_days() as f64).round() + 0.5;
        painter.line_segment(
            [pos2(x, track.top()), pos2(x, track.bottom())],
            Stroke::new(1.0, p.faint),
        );
        painter.text(
            pos2(x + 4.0, rect.top() + 2.0),
            Align2::LEFT_TOP,
            MONTHS[m.month0() as usize],
            theme::medium(12.0),
            p.ink2,
        );
        m = m.checked_add_months(chrono::Months::new(1)).unwrap_or(m);
    }
    let hover = resp.hover_pos();
    let mut tip: Option<String> = None;
    for (k, y) in years.iter().rev().enumerate() {
        let top = track.top() + k as f32 * row_h;
        let row = Rect::from_min_max(pos2(rect.left(), top), pos2(track.right(), top + row_h));
        let mid = row.center().y;
        painter.text(
            pos2(rect.left(), mid),
            Align2::LEFT_CENTER,
            year_label(y.start),
            theme::medium(13.0),
            if y.complete { p.ink } else { p.ink2 },
        );
        painter.line_segment(
            [pos2(track.left(), mid), pos2(track.right(), mid)],
            Stroke::new(1.0, p.grid),
        );
        for &i in &y.nights {
            let x = fx(i as f64);
            painter.rect_filled(
                Rect::from_center_size(pos2(x, mid), vec2(2.5, 12.0)),
                1,
                p.cold,
            );
        }
        if today >= y.start && (today - y.start).num_days() < 366 {
            let x = fx((today - y.start).num_days() as f64);
            painter.circle_filled(pos2(x, mid), 3.0, p.ink2);
        }
        if hover.is_some_and(|h| row.contains(h)) {
            painter.rect_filled(row, 2, alpha(p.ink2, 0.06));
            let mut t = format!(
                "{}: {} frost {}",
                year_label(y.start),
                y.nights.len(),
                if y.nights.len() == 1 {
                    "night"
                } else {
                    "nights"
                }
            );
            if let (Some(a), Some(b)) = (y.first(), y.last()) {
                t += &if a == b {
                    format!(", on {}", short_date(a))
                } else {
                    format!(", {} to {}", short_date(a), short_date(b))
                };
            }
            if let Some((d, v)) = y.coldest {
                t += &format!(
                    "\nColdest night {}{} on {}",
                    num(Some(v), 1),
                    u.temp,
                    short_date(d)
                );
            }
            if !y.complete {
                t += &format!("\nOnly {} days on record", y.days);
            }
            tip = Some(t);
        }
    }
    if let Some(t) = tip {
        resp.on_hover_text(t);
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        ui::key(ui, "Frost night", p.cold, KeyMark::Line);
        ui::key(
            ui,
            "Average frost season",
            alpha(p.cold, 0.35),
            KeyMark::Square,
        );
        ui::key(ui, "Today", p.ink2, KeyMark::Square);
    });
}
