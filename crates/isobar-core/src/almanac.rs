//! The almanac: one calendar year in review. The year against normal and
//! how it ranks, month by month, station records set, firsts and lasts
//! (frosts and hot days) against their usual dates, the longest spells, and
//! the tank year. As a page to print, like the month in review.

use std::fmt::Write as _;

use chrono::{Datelike, Duration, NaiveDate};

use crate::analytics::{self as an, doy_index, Field, Run};
use crate::dayfile::DayRecord;
use crate::live::Units;
use crate::reports::{
    days_in_month, dir, esc, h, nice_step, noaa_year, page, sc, short, NoaaYear, ReportContext,
    MONTH_NAMES,
};
use crate::seasons::Hemisphere;
use crate::tanks::TankYear;
use crate::trends::{temperature_trend, YearTemp};

/// A first or last of the year (the last frost of spring, say), with when it
/// usually comes.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub label: String,
    /// When it came this year; `None` if it didn't (or hasn't yet).
    pub date: Option<NaiveDate>,
    /// The window it's looked for in is still open, so it may change.
    pub open: bool,
    /// The average date in other years, put in this year.
    pub normal: Option<NaiveDate>,
    /// The earliest and latest in other years.
    pub earliest: Option<NaiveDate>,
    pub latest: Option<NaiveDate>,
    /// Other years it came in, of those looked at.
    pub years: (usize, usize),
}

/// A spell this year, with the longest on record for comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct Spell {
    pub label: String,
    pub this: Option<Run>,
    pub record: Option<Run>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Almanac {
    pub year: i32,
    pub noaa: NoaaYear,
    /// The year doesn't run to 31 December (yet).
    pub partial: bool,
    pub last: NaiveDate,
    /// The year's temperature against normal, measured day by day.
    pub anomaly: Option<f64>,
    /// Rank among whole years, warmest first, and how many.
    pub warm_rank: Option<(usize, usize)>,
    /// Rank among whole years, wettest first, and how many.
    pub wet_rank: Option<(usize, usize)>,
    /// The usual rain to this date (all year when it's whole).
    pub normal_rain: Option<f64>,
    /// Station records the year set, as sentences.
    pub records: Vec<String>,
    /// Days that were the warmest or coolest on record for their date
    /// (highs above, lows below), and the years before this one counted.
    pub date_records: Option<(usize, usize, usize)>,
    pub events: Vec<Event>,
    pub spells: Vec<Spell>,
    pub wettest_week: Option<(NaiveDate, f64)>,
    pub tank: Option<TankYear>,
}

fn frost(d: &DayRecord, t: f64) -> bool {
    d.temp_min.is_some_and(|v| v <= t)
}

fn hot(d: &DayRecord, t: f64) -> bool {
    d.temp_max.is_some_and(|v| v >= t)
}

/// The window an event is looked for in, in `year`: (start, end).
fn window(year: i32, first_half: bool, whole: bool) -> (NaiveDate, NaiveDate) {
    let ymd = |m, d| NaiveDate::from_ymd_opt(year, m, d).unwrap();
    match (whole, first_half) {
        (true, _) => (ymd(1, 1), ymd(12, 31)),
        (false, true) => (ymd(1, 1), ymd(6, 30)),
        (false, false) => (ymd(7, 1), ymd(12, 31)),
    }
}

#[allow(clippy::too_many_arguments)]
fn event(
    days: &[DayRecord],
    year: i32,
    last: NaiveDate,
    label: &str,
    pred: &dyn Fn(&DayRecord) -> bool,
    first_half: bool,
    whole: bool,
    want_last: bool,
) -> Event {
    let find = |y: i32| -> Option<(Option<NaiveDate>, NaiveDate)> {
        let (s, e) = window(y, first_half, whole);
        let sel = an::in_range(days, s, e);
        let want = (e - s).num_days() + 1;
        let mut hits = sel.iter().filter(|d| pred(d)).map(|d| d.date);
        let found = if want_last { hits.last() } else { hits.next() };
        // Another year counts only with nine tenths of the window on record.
        if y != year && (sel.len() as f64) < want as f64 * 0.9 {
            return None;
        }
        Some((found, s))
    };
    let (date, _) = find(year).unwrap_or((None, window(year, first_half, whole).0));
    let (s, e) = window(year, first_half, whole);
    // A first, once found, is settled; a last can still move.
    let open = last < e && (want_last || date.is_none());
    let first_year = days.first().map_or(year, |d| d.date.year());
    let last_year = days.last().map_or(year, |d| d.date.year());
    let mut offsets = Vec::new();
    let mut looked = 0;
    for y in first_year..=last_year {
        if y == year {
            continue;
        }
        if let Some((found, ws)) = find(y) {
            looked += 1;
            if let Some(f) = found {
                offsets.push((f - ws).num_days());
            }
        }
    }
    let at = |o: i64| s + Duration::days(o);
    Event {
        label: label.to_string(),
        date,
        open,
        normal: (!offsets.is_empty()).then(|| {
            at((offsets.iter().sum::<i64>() as f64 / offsets.len() as f64).round() as i64)
        }),
        earliest: offsets.iter().min().map(|o| at(*o)),
        latest: offsets.iter().max().map(|o| at(*o)),
        years: (offsets.len(), looked),
    }
    .clip(e)
}

impl Event {
    /// Keep made-up normal dates inside the window.
    fn clip(mut self, end: NaiveDate) -> Event {
        for d in [&mut self.normal, &mut self.earliest, &mut self.latest] {
            *d = d.map(|x| x.min(end));
        }
        self
    }
}

/// The usual rain from 1 January to the same date as `last`, from other
/// years with nine tenths of those days.
fn rain_to_date(days: &[DayRecord], year: i32, last: NaiveDate) -> Option<f64> {
    let first_year = days.first()?.date.year();
    let last_year = days.last()?.date.year();
    let mut totals = Vec::new();
    for y in first_year..=last_year {
        if y == year {
            continue;
        }
        let s = NaiveDate::from_ymd_opt(y, 1, 1)?;
        let e = NaiveDate::from_ymd_opt(y, last.month(), last.day())
            .or_else(|| NaiveDate::from_ymd_opt(y, last.month(), 28))?;
        let sel = an::in_range(days, s, e);
        if (sel.len() as f64) >= ((e - s).num_days() + 1) as f64 * 0.9 {
            totals.push(sel.iter().filter_map(|d| d.rain).sum::<f64>());
        }
    }
    (!totals.is_empty()).then(|| totals.iter().sum::<f64>() / totals.len() as f64)
}

/// The wettest seven days in a row.
fn wettest_week(days: &[DayRecord]) -> Option<(NaiveDate, f64)> {
    let mut best: Option<(NaiveDate, f64)> = None;
    for (i, d) in days.iter().enumerate() {
        let end = d.date + Duration::days(6);
        let total: f64 = days[i..]
            .iter()
            .take_while(|x| x.date <= end)
            .filter_map(|x| x.rain)
            .sum();
        if total > 0.0 && best.is_none_or(|b| total > b.1) {
            best = Some((d.date, total));
        }
    }
    best
}

/// Station records the year set: its extreme beat every day before it.
fn records_set(days: &[DayRecord], year: i32, u: &Units) -> Vec<String> {
    let start = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
    let before: Vec<DayRecord> = days.iter().filter(|d| d.date < start).cloned().collect();
    if before.len() < 365 {
        return Vec::new();
    }
    let this: Vec<DayRecord> = days
        .iter()
        .filter(|d| d.date.year() == year)
        .cloned()
        .collect();
    let checks: [(Field, an::Kind, &str, &str, usize); 6] = [
        (Field::TempMax, an::Kind::Max, "Hottest day", &u.temp, 1),
        (Field::TempMin, an::Kind::Min, "Coldest night", &u.temp, 1),
        (Field::Rain, an::Kind::Max, "Wettest day", &u.rain, 1),
        (Field::GustMax, an::Kind::Max, "Strongest gust", &u.wind, 0),
        (
            Field::PressMin,
            an::Kind::Min,
            "Lowest pressure",
            &u.press,
            1,
        ),
        (
            Field::PressMax,
            an::Kind::Max,
            "Highest pressure",
            &u.press,
            1,
        ),
    ];
    let mut out = Vec::new();
    for (field, kind, what, unit, digits) in checks {
        let (Some(now), Some(was)) = (
            an::extreme(&this, field, kind, false),
            an::extreme(&before, field, kind, false),
        ) else {
            continue;
        };
        let beat = match kind {
            an::Kind::Max => now.value > was.value,
            an::Kind::Min => now.value < was.value,
        };
        if beat {
            let sep = if unit.starts_with('°') { "" } else { " " };
            out.push(format!(
                "{what} on record: {:.digits$}{sep}{unit} on {} (before, {:.digits$}{sep}{unit} on {} {}).",
                now.value,
                short(now.date),
                was.value,
                short(was.date),
                was.date.year()
            ));
        }
    }
    out
}

/// Days whose high was the warmest, or low the coolest, for their date in
/// every earlier year (with at least five).
fn date_records(days: &[DayRecord], year: i32) -> Option<(usize, usize, usize)> {
    let first = days.first()?.date.year();
    let before = year - first;
    if before < 5 {
        return None;
    }
    let mut hi = vec![f64::NEG_INFINITY; 366];
    let mut lo = vec![f64::INFINITY; 366];
    for d in days.iter().filter(|d| d.date.year() < year) {
        let k = doy_index(d.date);
        if let Some(v) = d.temp_max {
            hi[k] = hi[k].max(v);
        }
        if let Some(v) = d.temp_min {
            lo[k] = lo[k].min(v);
        }
    }
    let (mut warm, mut cool) = (0, 0);
    for d in days.iter().filter(|d| d.date.year() == year) {
        let k = doy_index(d.date);
        if d.temp_max.is_some_and(|v| hi[k].is_finite() && v > hi[k]) {
            warm += 1;
        }
        if d.temp_min.is_some_and(|v| lo[k].is_finite() && v < lo[k]) {
            cool += 1;
        }
    }
    Some((warm, cool, before as usize))
}

fn rank(values: &[(i32, f64)], year: i32) -> Option<(usize, usize)> {
    let mine = values.iter().find(|v| v.0 == year)?.1;
    Some((
        values.iter().filter(|v| v.1 > mine).count() + 1,
        values.len(),
    ))
}

/// The almanac for `year`. `tank` is the tank model's year, when tanks are
/// set up.
pub fn almanac(
    days: &[DayRecord],
    year: i32,
    u: &Units,
    h: Hemisphere,
    tank: Option<TankYear>,
) -> Option<Almanac> {
    let noaa = noaa_year(days, year, u)?;
    let t = noaa.thresholds;
    let mine: Vec<DayRecord> = days
        .iter()
        .filter(|d| d.date.year() == year)
        .cloned()
        .collect();
    let last = mine.last()?.date;
    let partial = last < NaiveDate::from_ymd_opt(year, 12, 31).unwrap();
    let trend = temperature_trend(days);
    let years: Vec<&YearTemp> = trend
        .as_ref()
        .map_or(Vec::new(), |t| t.years.iter().collect());
    let anomaly = years.iter().find(|y| y.year == year).map(|y| y.anomaly);
    let warm_rank = if partial {
        None
    } else {
        let whole: Vec<(i32, f64)> = years
            .iter()
            .filter(|y| !y.partial())
            .map(|y| (y.year, y.anomaly))
            .collect();
        rank(&whole, year).filter(|r| r.1 >= 3)
    };
    let wet_rank = if partial {
        None
    } else {
        let mut by_year: std::collections::BTreeMap<i32, (usize, f64)> = Default::default();
        for d in days {
            let e = by_year.entry(d.date.year()).or_default();
            e.0 += 1;
            e.1 += d.rain.unwrap_or(0.0);
        }
        let whole: Vec<(i32, f64)> = by_year
            .into_iter()
            .filter(|(_, (n, _))| *n >= 330)
            .map(|(y, (_, r))| (y, r))
            .collect();
        rank(&whole, year).filter(|r| r.1 >= 3)
    };
    let south = h == Hemisphere::South;
    let f = t.freezing;
    let hot_at = t.hot;
    let deg = |v: f64| format!("{v}{}", u.temp);
    let frost_pred = move |d: &DayRecord| frost(d, f);
    let hot_pred = move |d: &DayRecord| hot(d, hot_at);
    let fw = format!("({} or below)", deg(f));
    let hw = format!("({} or more)", deg(hot_at));
    let events = if south {
        vec![
            event(
                days,
                year,
                last,
                &format!("Last hot day of summer {hw}"),
                &hot_pred,
                true,
                false,
                true,
            ),
            event(
                days,
                year,
                last,
                &format!("First frost {fw}"),
                &frost_pred,
                false,
                true,
                false,
            ),
            event(
                days,
                year,
                last,
                &format!("Last frost {fw}"),
                &frost_pred,
                false,
                true,
                true,
            ),
            event(
                days,
                year,
                last,
                &format!("First hot day of summer {hw}"),
                &hot_pred,
                false,
                false,
                false,
            ),
        ]
    } else {
        vec![
            event(
                days,
                year,
                last,
                &format!("Last frost of spring {fw}"),
                &frost_pred,
                true,
                false,
                true,
            ),
            event(
                days,
                year,
                last,
                &format!("First hot day {hw}"),
                &hot_pred,
                false,
                true,
                false,
            ),
            event(
                days,
                year,
                last,
                &format!("Last hot day {hw}"),
                &hot_pred,
                false,
                true,
                true,
            ),
            event(
                days,
                year,
                last,
                &format!("First frost of autumn {fw}"),
                &frost_pred,
                false,
                false,
                false,
            ),
        ]
    };
    let spell = |label: &str, p: &dyn Fn(&DayRecord) -> bool| Spell {
        label: label.to_string(),
        this: an::longest_run(&mine, p),
        record: an::longest_run(days, p),
    };
    let spells = vec![
        spell("Longest dry spell", &|d| !an::is_rain_day(d)),
        spell("Most rain days in a row", &an::is_rain_day),
        spell(
            &format!("Most days of {} or more in a row", deg(hot_at)),
            &hot_pred,
        ),
        spell(
            &format!("Most frosts in a row ({} or below)", deg(f)),
            &frost_pred,
        ),
    ];
    Some(Almanac {
        year,
        partial,
        last,
        anomaly,
        warm_rank,
        wet_rank,
        normal_rain: if partial {
            rain_to_date(days, year, last)
        } else {
            noaa.totals
                .normal_rain
                .or_else(|| rain_to_date(days, year, last))
        },
        records: records_set(days, year, u),
        date_records: date_records(days, year),
        events,
        spells,
        wettest_week: wettest_week(&mine),
        tank,
        noaa,
    })
}

// ------------------------------------------------------------------ words

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

/// "3rd warmest of 17 years", "the warmest of 17 years".
pub fn rank_words(r: (usize, usize), most: &str, least: &str) -> String {
    let (k, n) = r;
    if k == 1 {
        format!("the {most} of {n} whole years")
    } else if k == n {
        format!("the {least} of {n} whole years")
    } else if k * 2 <= n + 1 {
        format!("{} {most} of {n} whole years", ordinal(k))
    } else {
        format!("{} {least} of {n} whole years", ordinal(n + 1 - k))
    }
}

/// How an event's date sat against its usual one.
pub fn event_words(e: &Event) -> String {
    let Some(n) = e.normal else {
        return "No other years to compare".into();
    };
    match e.date {
        Some(d) => {
            let off = (d - n).num_days();
            let when = match off {
                0 => "right on the usual date".to_string(),
                o if o < 0 => format!("{} days earlier than usual", -o),
                o => format!("{o} days later than usual"),
            };
            if e.open {
                format!("{when} (so far)")
            } else {
                when
            }
        }
        None if e.open => format!("Not yet; usually around {}", short(n)),
        None => format!(
            "Didn't happen; it did in {} of {} other years",
            e.years.0, e.years.1
        ),
    }
}

/// Whole litres with thousands separators: "132,160".
fn thousands(v: f64) -> String {
    let n = v.round() as i64;
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
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

fn run_words(r: &Run) -> String {
    if r.length == 1 {
        format!("1 day, {}", short(r.start))
    } else {
        format!("{} days, {} to {}", r.length, short(r.start), short(r.end))
    }
}

// ------------------------------------------------------------------- page

fn months_svg(a: &Almanac) -> String {
    const W: f64 = 640.0;
    let (top, mid_t, bottom_t, top_r, bottom_r) = (26.0, 81.0, 136.0, 176.0, 280.0);
    let col = |m: f64| sc(m, 0.0, 11.0, 60.0, W - 20.0);
    let mut o = format!(
        "<svg viewBox=\"0 0 {W} 304\" role=\"img\" aria-label=\"Each month against normal\">"
    );
    // Temperature against normal, up and down from a centre line.
    let deps: Vec<Option<f64>> = (1..=12)
        .map(|m| {
            a.noaa
                .months
                .iter()
                .find(|x| x.0 == m)
                .and_then(|(_, t)| Some(t.mean? - t.normal_mean?))
        })
        .collect();
    let span = deps
        .iter()
        .flatten()
        .fold(1.0f64, |acc, v| acc.max(v.abs()))
        .ceil();
    let _ = write!(
        o,
        "<text x=\"60\" y=\"{:.1}\">Temperature against normal</text>",
        top - 12.0
    );
    let _ = write!(
        o,
        "<line x1=\"56\" x2=\"{:.1}\" y1=\"{mid_t}\" y2=\"{mid_t}\" stroke=\"#5a6570\" stroke-width=\"0.8\"/>",
        W - 16.0
    );
    let _ = write!(
        o,
        "<text x=\"52\" y=\"{:.1}\" text-anchor=\"end\">+{span:.0}°</text><text x=\"52\" y=\"{:.1}\" text-anchor=\"end\">−{span:.0}°</text>",
        top + 4.0,
        bottom_t
    );
    for (i, d) in deps.iter().enumerate() {
        let Some(d) = d else { continue };
        let x = col(i as f64);
        let y = sc(*d, -span, span, bottom_t, top);
        let (y0, y1) = if *d >= 0.0 { (y, mid_t) } else { (mid_t, y) };
        let _ = write!(
            o,
            "<rect x=\"{:.1}\" y=\"{y0:.1}\" width=\"22\" height=\"{:.1}\" fill=\"{}\" opacity=\"0.75\"><title>{} {:+.1}°</title></rect>",
            x - 11.0,
            (y1 - y0).max(0.8),
            if *d >= 0.0 { "#b3362b" } else { "#2a6896" },
            MONTH_NAMES[i],
            d
        );
    }
    // Rain, with each month's normal as a tick.
    let rains: Vec<(Option<f64>, Option<f64>)> = (1..=12)
        .map(|m| {
            a.noaa
                .months
                .iter()
                .find(|x| x.0 == m)
                .map_or((None, None), |(_, t)| (Some(t.rain), t.normal_rain))
        })
        .collect();
    let hi = rains
        .iter()
        .flat_map(|(r, n)| [*r, *n])
        .flatten()
        .fold(1.0f64, f64::max);
    let step = nice_step(hi, 3.0);
    let hi = (hi / step).ceil() * step;
    let _ = write!(
        o,
        "<text x=\"60\" y=\"{:.1}\">Rain against normal</text><text x=\"52\" y=\"{:.1}\" text-anchor=\"end\">{hi:.0}</text><text x=\"52\" y=\"{:.1}\" text-anchor=\"end\">0</text>",
        top_r - 12.0,
        top_r + 4.0,
        bottom_r + 3.0
    );
    let _ = write!(
        o,
        "<line x1=\"56\" x2=\"{:.1}\" y1=\"{bottom_r}\" y2=\"{bottom_r}\" stroke=\"#5a6570\" stroke-width=\"0.8\"/>",
        W - 16.0
    );
    for (i, (r, n)) in rains.iter().enumerate() {
        let x = col(i as f64);
        if let Some(r) = r {
            let y = sc(*r, 0.0, hi, bottom_r, top_r);
            let _ = write!(
                o,
                "<rect x=\"{:.1}\" y=\"{y:.1}\" width=\"22\" height=\"{:.1}\" fill=\"#2b68ad\" opacity=\"0.7\"><title>{} {r:.1}</title></rect>",
                x - 11.0,
                (bottom_r - y).max(0.8),
                MONTH_NAMES[i]
            );
        }
        if let Some(n) = n {
            let y = sc(*n, 0.0, hi, bottom_r, top_r);
            let _ = write!(
                o,
                "<line x1=\"{:.1}\" x2=\"{:.1}\" y1=\"{y:.1}\" y2=\"{y:.1}\" stroke=\"#1c2633\" stroke-width=\"1.6\"/>",
                x - 14.0,
                x + 14.0
            );
        }
        let _ = write!(
            o,
            "<text x=\"{x:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{}</text>",
            bottom_r + 16.0,
            &MONTH_NAMES[i][..3]
        );
    }
    o.push_str("</svg>");
    o
}

pub fn almanac_html(a: &Almanac, cx: &ReportContext) -> String {
    let u = &cx.units;
    let x = &a.noaa.totals;
    let th = a.noaa.thresholds;
    let mut b = String::new();
    let _ = write!(
        b,
        "<h1>{} almanac</h1><p class=\"sub\">{}{}</p>",
        a.year,
        esc(&cx.station),
        if a.partial {
            format!(" · to {} {}", short(a.last), a.year)
        } else {
            String::new()
        }
    );
    let tile = |b: &mut String, value: String, label: String, class: &str| {
        let _ = write!(
            b,
            "<div class=\"tile\"><b class=\"{class}\">{}</b><span>{}</span></div>",
            esc(&value),
            esc(&label)
        );
    };
    b.push_str("<div class=\"tiles\">");
    let mut temp_label = match a.anomaly {
        Some(d) => format!("Mean temperature, {d:+.1}° against normal"),
        None => "Mean temperature".into(),
    };
    if let Some(r) = a.warm_rank {
        let _ = write!(temp_label, "; {}", rank_words(r, "warmest", "coolest"));
    }
    tile(
        &mut b,
        format!("{}{}", h(x.mean, 1), u.temp),
        temp_label,
        "",
    );
    let mut rain_label = match a.normal_rain.filter(|n| *n > 0.0) {
        Some(n) => format!(
            "Rain, {:.0}% of normal{}",
            x.rain / n * 100.0,
            if a.partial { " to date" } else { "" }
        ),
        None => "Rain".into(),
    };
    if let Some(r) = a.wet_rank {
        let _ = write!(rain_label, "; {}", rank_words(r, "wettest", "driest"));
    }
    tile(
        &mut b,
        format!("{:.0} {}", x.rain, u.rain),
        rain_label,
        "rain",
    );
    tile(
        &mut b,
        x.rain_days[0].to_string(),
        format!("Days with {} {} or more", th.rain[0], u.rain),
        "rain",
    );
    tile(
        &mut b,
        x.max_rain
            .map_or("–".into(), |v| format!("{:.1} {}", v.0, u.rain)),
        x.max_rain.map_or("Wettest day".into(), |v| {
            format!("Wettest day, {}", short(v.1))
        }),
        "rain",
    );
    tile(
        &mut b,
        x.high
            .map_or("–".into(), |v| format!("{:.1}{}", v.0, u.temp)),
        x.high.map_or("Hottest day".into(), |v| {
            format!("Hottest day, {}", short(v.1))
        }),
        "heat",
    );
    tile(
        &mut b,
        x.low
            .map_or("–".into(), |v| format!("{:.1}{}", v.0, u.temp)),
        x.low.map_or("Coldest night".into(), |v| {
            format!("Coldest night, {}", short(v.1))
        }),
        "cold",
    );
    tile(
        &mut b,
        x.min_freezing.to_string(),
        format!("Frosts, {}{} or below", th.freezing, u.temp),
        "cold",
    );
    tile(
        &mut b,
        x.gust
            .map_or("–".into(), |g| format!("{:.0} {}", g.0, u.wind)),
        x.gust
            .map_or("Peak gust".into(), |g| format!("Peak gust, {}", short(g.1))),
        "",
    );
    b.push_str("</div>");

    b.push_str("<h2>Month by month</h2><div class=\"key\"><span><i style=\"background:#b3362b\"></i>Warmer than normal</span><span><i style=\"background:#2a6896\"></i>Cooler</span><span><i style=\"background:#2b68ad\"></i>Rain</span><span><i style=\"background:#1c2633\"></i>Normal rain</span></div>");
    b.push_str(&months_svg(a));
    b.push_str("<div class=\"wrap\"><table><tr><th>Month</th><th>Mean</th><th>Against normal</th><th>Mean high</th><th>Mean low</th><th>Hottest</th><th>Coldest</th><th>Rain</th><th>Of normal</th><th>Rain days</th><th>Frosts</th><th>Hot days</th></tr>");
    for (m, t) in &a.noaa.months {
        let part = t.days < days_in_month(a.year, *m) as usize;
        let dep = match (t.mean, t.normal_mean) {
            (Some(v), Some(n)) => {
                let d = v - n;
                format!(
                    "<span class=\"{}\">{d:+.1}°</span>",
                    if d >= 0.0 { "heat" } else { "cold" }
                )
            }
            _ => "–".into(),
        };
        let pct = match t.normal_rain.filter(|n| *n > 0.0) {
            Some(n) if !part => format!("{:.0}%", t.rain / n * 100.0),
            _ => "–".into(),
        };
        let _ = write!(
            b,
            "<tr><td>{}{}</td><td>{}</td><td>{dep}</td><td>{}</td><td>{}</td><td class=\"heat\">{}</td><td class=\"cold\">{}</td><td class=\"rain\">{:.1}</td><td>{pct}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            MONTH_NAMES[*m as usize - 1],
            if part { " (so far)" } else { "" },
            h(t.mean, 1),
            h(t.mean_max, 1),
            h(t.mean_min, 1),
            h(t.high.map(|v| v.0), 1),
            h(t.low.map(|v| v.0), 1),
            t.rain,
            t.rain_days[0],
            t.min_freezing,
            t.max_hot
        );
    }
    let _ = write!(
        b,
        "<tr class=\"total\"><td>Year</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"heat\">{}</td><td class=\"cold\">{}</td><td class=\"rain\">{:.1}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr></table></div>",
        h(x.mean, 1),
        a.anomaly.map_or("–".into(), |d| format!("{d:+.1}°")),
        h(x.mean_max, 1),
        h(x.mean_min, 1),
        h(x.high.map(|v| v.0), 1),
        h(x.low.map(|v| v.0), 1),
        x.rain,
        a.normal_rain
            .filter(|n| *n > 0.0)
            .map_or("–".into(), |n| format!("{:.0}%", x.rain / n * 100.0)),
        x.rain_days[0],
        x.min_freezing,
        x.max_hot
    );
    let _ = write!(
        b,
        "<p class=\"sub\">Units: {t} and {r}. Hot days reach {hot}{t}; frosts are nights of {f}{t} or below; rain days have {rd} {r} or more. Normals are your station's own other years.</p>",
        t = u.temp,
        r = u.rain,
        hot = th.hot,
        f = th.freezing,
        rd = th.rain[0]
    );

    b.push_str("<h2>Records</h2>");
    if a.records.is_empty() && a.date_records.is_none() {
        b.push_str("<p>No station records fell this year, or there aren't enough earlier years to say.</p>");
    } else {
        b.push_str("<ul class=\"notes\">");
        for r in &a.records {
            let _ = write!(b, "<li>{}</li>", esc(r));
        }
        if a.records.is_empty() {
            b.push_str("<li>No all-time station records this year.</li>");
        }
        if let Some((warm, cool, n)) = a.date_records {
            let _ = write!(
                b,
                "<li>{warm} day{} had the warmest high for {} date of the {n} years before, and {cool} the coldest low.</li>",
                if warm == 1 { "" } else { "s" },
                if warm == 1 { "its" } else { "their" }
            );
        }
        b.push_str("</ul>");
    }

    b.push_str("<h2>Firsts and lasts</h2><div class=\"wrap\"><table><tr><th>Event</th><th>This year</th><th>Against usual</th><th>Usual date</th><th>Earliest</th><th>Latest</th></tr>");
    for e in &a.events {
        let _ = write!(
            b,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            esc(&e.label),
            e.date.map_or("–".into(), short),
            esc(&event_words(e)),
            e.normal.map_or("–".into(), short),
            e.earliest.map_or("–".into(), short),
            e.latest.map_or("–".into(), short)
        );
    }
    b.push_str("</table></div>");

    b.push_str("<h2>Spells</h2><div class=\"wrap\"><table><tr><th>Spell</th><th>This year</th><th>Longest on record</th></tr>");
    for s in &a.spells {
        let rec = s.record.map_or("–".into(), |r| {
            format!("{}, {}", run_words(&r), r.start.year())
        });
        let _ = write!(
            b,
            "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
            esc(&s.label),
            s.this.map_or("None".into(), |r| run_words(&r)),
            rec
        );
    }
    if let Some((d, r)) = a.wettest_week {
        let _ = write!(
            b,
            "<tr><td>Wettest seven days</td><td>{r:.1} {}, {} to {}</td><td></td></tr>",
            u.rain,
            short(d),
            short(d + Duration::days(6))
        );
    }
    b.push_str("</table></div>");
    if x.dir.is_some() {
        let _ = write!(
            b,
            "<p>The wind came mostly from the {}, with the strongest gust {}.</p>",
            dir(x.dir),
            x.gust.map_or(String::new(), |g| format!(
                "{:.0} {} on {}",
                g.0,
                u.wind,
                short(g.1)
            ))
        );
    }

    if let Some(t) = &a.tank {
        b.push_str("<h2>The tank year</h2><div class=\"tiles\">");
        tile(
            &mut b,
            format!("{} L", thousands(t.caught)),
            "Caught from the roof".into(),
            "rain",
        );
        tile(
            &mut b,
            format!("{} L", thousands(t.used)),
            "Used".into(),
            "",
        );
        tile(
            &mut b,
            format!("{} L", thousands(t.overflow)),
            "Overflowed".into(),
            "",
        );
        tile(
            &mut b,
            format!("{} L", thousands(t.lowest.1)),
            format!("Lowest, {}", short(t.lowest.0)),
            "",
        );
        b.push_str("</div>");
        if t.days_low > 0 {
            let _ = write!(
                b,
                "<p>{} days at or below the level you keep for emergencies.</p>",
                t.days_low
            );
        }
        b.push_str("<p class=\"sub\">From Isobar's tank model: the roof, tanks and daily use set in Settings, with the rain recorded here.</p>");
    }
    let _ = write!(
        b,
        "<p class=\"foot\">Made by Isobar, {}, from {} days of {}. Normals and usual dates are from the station's other years.</p>",
        cx.made,
        x.days,
        a.year
    );
    page(&format!("{} almanac", a.year), &b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Six years of a southern climate: warm January, frosty July nights,
    /// rain every fifth day.
    fn days(to: NaiveDate) -> Vec<DayRecord> {
        let mut out = Vec::new();
        let mut d = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        while d <= to {
            let season = (d.ordinal() as f64 / 365.25 * std::f64::consts::TAU).cos();
            let max = 22.0 + season * 10.0 + (d.year() - 2020) as f64 * 0.2;
            let min = 8.0 + season * 7.0;
            out.push(DayRecord {
                date: d,
                temp_max: Some(max),
                temp_min: Some(min),
                rain: Some(if d.ordinal().is_multiple_of(5) {
                    4.0
                } else {
                    0.0
                }),
                gust_max: Some(40.0 + (d.ordinal() % 7) as f64),
                ..Default::default()
            });
            d += Duration::days(1);
        }
        out
    }

    #[test]
    fn a_whole_year() {
        let all = days(NaiveDate::from_ymd_opt(2025, 12, 31).unwrap());
        let a = almanac(&all, 2025, &Units::default(), Hemisphere::South, None).unwrap();
        assert!(!a.partial);
        assert_eq!(a.noaa.months.len(), 12);
        // Each year 0.2° warmer: the last is the warmest of six.
        assert_eq!(a.warm_rank, Some((1, 6)));
        assert!(a.anomaly.unwrap() > 0.0);
        // A frost needs min ≤ 0: 8 - 7 = 1 at the coldest, so none.
        let first_frost = &a.events[1];
        assert!(first_frost.date.is_none() && first_frost.normal.is_none());
        // Hot days (30° or more): max reaches 32+ in January.
        let last_hot = &a.events[0];
        assert!(last_hot.date.is_some() && last_hot.normal.is_some());
        assert_eq!(a.spells[0].this.unwrap().length, 4);
        assert_eq!(a.wettest_week.unwrap().1, 8.0);
        assert!(a
            .records
            .iter()
            .any(|r| r.starts_with("Hottest day on record")));
        let html = almanac_html(
            &a,
            &ReportContext {
                station: "Test".into(),
                units: Units::default(),
                latitude: None,
                made: "now".into(),
            },
        );
        assert!(html.contains("<h1>2025 almanac</h1>"));
        assert!(html.contains("the warmest of 6 whole years"));
    }

    #[test]
    fn part_of_a_year() {
        let all = days(NaiveDate::from_ymd_opt(2025, 3, 10).unwrap());
        let a = almanac(&all, 2025, &Units::default(), Hemisphere::South, None).unwrap();
        assert!(a.partial && a.warm_rank.is_none());
        // Rain to 10 March in other years: every fifth day, 4 mm.
        let n = a.normal_rain.unwrap();
        assert!((n - 13.0 * 4.0).abs() < 4.1, "{n}");
        // The first hot day of next summer: not yet.
        let e = &a.events[3];
        assert!(e.open && e.date.is_none());
        assert!(event_words(e).starts_with("Not yet"));
    }

    #[test]
    fn ranks_in_words() {
        assert_eq!(
            rank_words((1, 10), "warmest", "coolest"),
            "the warmest of 10 whole years"
        );
        assert_eq!(
            rank_words((2, 10), "warmest", "coolest"),
            "2nd warmest of 10 whole years"
        );
        assert_eq!(
            rank_words((9, 10), "warmest", "coolest"),
            "2nd coolest of 10 whole years"
        );
        assert_eq!(
            rank_words((10, 10), "warmest", "coolest"),
            "the coolest of 10 whole years"
        );
        assert_eq!(ordinal(11), "11th");
        assert_eq!(ordinal(23), "23rd");
        assert_eq!(thousands(132160.4), "132,160");
        assert_eq!(thousands(950.0), "950");
        assert_eq!(thousands(1000.0), "1,000");
    }
}
