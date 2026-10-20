//! Reports to save and print: NOAA-style monthly and yearly climate summaries
//! (as the familiar fixed-width text, and as an HTML page), and a one-page
//! month in review with its charts. Each HTML page stands alone: styles and
//! charts (SVG) are inline, so it opens in any browser and prints well.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use chrono::{Datelike, NaiveDate};

use crate::analytics::{self as an, climatology, doy_index, Field, COMPASS16};
use crate::dayfile::DayRecord;
use crate::live::Units;
use crate::seasons::{is_fahrenheit, is_inches};

const MONTH_ABBR: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];
const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Who the report is for and what it's measured in.
#[derive(Debug, Clone)]
pub struct ReportContext {
    pub station: String,
    pub units: Units,
    pub latitude: Option<f64>,
    /// When the report was made, as words ("20 Oct 2026, 09:14").
    pub made: String,
}

/// The thresholds a NOAA summary counts days against, in the station's units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    pub hot: f64,
    pub freezing: f64,
    pub very_cold: f64,
    pub rain: [f64; 3],
    /// The base for degree days worked out by Isobar, when the dayfile has none.
    pub degree_base: f64,
}

impl Thresholds {
    pub fn for_units(u: &Units) -> Thresholds {
        let (hot, freezing, very_cold, base) = if is_fahrenheit(u) {
            (90.0, 32.0, 0.0, 65.0)
        } else {
            (30.0, 0.0, -18.0, 18.3)
        };
        let rain = if is_inches(u) {
            [0.01, 0.1, 1.0]
        } else {
            [0.2, 2.0, 20.0]
        };
        Thresholds {
            hot,
            freezing,
            very_cold,
            rain,
            degree_base: base,
        }
    }
}

/// One day's line of a monthly summary.
#[derive(Debug, Clone, PartialEq)]
pub struct NoaaDay {
    pub date: NaiveDate,
    pub mean: Option<f64>,
    pub high: Option<f64>,
    pub high_time: Option<String>,
    pub low: Option<f64>,
    pub low_time: Option<String>,
    pub hdd: Option<f64>,
    pub cdd: Option<f64>,
    pub rain: Option<f64>,
    pub wind_avg: Option<f64>,
    pub gust: Option<f64>,
    pub gust_time: Option<String>,
    pub dir: Option<f64>,
}

/// A month or year of days summed up, as the bottom lines of a summary.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Totals {
    pub days: usize,
    pub mean_max: Option<f64>,
    pub mean_min: Option<f64>,
    pub mean: Option<f64>,
    pub high: Option<(f64, NaiveDate)>,
    pub low: Option<(f64, NaiveDate)>,
    pub hdd: f64,
    pub cdd: f64,
    pub rain: f64,
    pub max_rain: Option<(f64, NaiveDate)>,
    pub wind_avg: Option<f64>,
    pub gust: Option<(f64, NaiveDate)>,
    pub dir: Option<f64>,
    pub max_hot: usize,
    pub max_freezing: usize,
    pub min_freezing: usize,
    pub min_very_cold: usize,
    pub rain_days: [usize; 3],
    /// From the station's other years: the normal mean temperature and rain
    /// for the same stretch.
    pub normal_mean: Option<f64>,
    pub normal_rain: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NoaaMonth {
    pub year: i32,
    pub month: u32,
    pub days: Vec<NoaaDay>,
    pub totals: Totals,
    pub thresholds: Thresholds,
    /// Degree days from the station's own dayfile figures, or worked out by
    /// Isobar from each day's mean.
    pub dayfile_degree_days: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NoaaYear {
    pub year: i32,
    /// (month, totals) for each month with any days.
    pub months: Vec<(u32, Totals)>,
    pub totals: Totals,
    pub thresholds: Thresholds,
    pub dayfile_degree_days: bool,
}

fn day_mean(d: &DayRecord) -> Option<f64> {
    d.temp_avg
        .or_else(|| Some((d.temp_max? + d.temp_min?) / 2.0))
}

/// The day's average wind speed, from its wind run.
fn day_wind(d: &DayRecord, u: &Units) -> Option<f64> {
    let run = d.wind_run?;
    Some(match u.wind.trim().to_ascii_lowercase().as_str() {
        "m/s" => run / 24.0 / 3.6,
        _ => run / 24.0,
    })
}

fn degree_days(d: &DayRecord, t: &Thresholds) -> (Option<f64>, Option<f64>, bool) {
    if d.hdd.is_some() || d.cdd.is_some() {
        return (d.hdd, d.cdd, true);
    }
    match day_mean(d) {
        Some(m) => (
            Some((t.degree_base - m).max(0.0)),
            Some((m - t.degree_base).max(0.0)),
            false,
        ),
        None => (None, None, false),
    }
}

fn noaa_day(d: &DayRecord, u: &Units, t: &Thresholds) -> (NoaaDay, bool) {
    let (hdd, cdd, from_file) = degree_days(d, t);
    (
        NoaaDay {
            date: d.date,
            mean: day_mean(d),
            high: d.temp_max,
            high_time: d.temp_max_time.clone(),
            low: d.temp_min,
            low_time: d.temp_min_time.clone(),
            hdd,
            cdd,
            rain: d.rain,
            wind_avg: day_wind(d, u),
            gust: d.gust_max,
            gust_time: d.gust_time.clone(),
            dir: d.wind_dir_dominant,
        },
        from_file,
    )
}

fn mean_of(v: impl Iterator<Item = f64>) -> Option<f64> {
    let (n, s) = v.fold((0usize, 0.0), |(n, s), x| (n + 1, s + x));
    (n > 0).then(|| s / n as f64)
}

/// The most common of the days' dominant directions, by 16-point sector.
fn dominant(dirs: impl Iterator<Item = f64>) -> Option<f64> {
    let mut counts = [0usize; 16];
    let mut any = false;
    for b in dirs {
        counts[an::compass_index(b)] += 1;
        any = true;
    }
    any.then(|| (0..16).max_by_key(|i| counts[*i]).unwrap_or(0) as f64 * 22.5)
}

fn totals(rows: &[NoaaDay], t: &Thresholds) -> Totals {
    let max_by = |f: &dyn Fn(&NoaaDay) -> Option<f64>| {
        rows.iter()
            .filter_map(|r| f(r).map(|v| (v, r.date)))
            .max_by(|a, b| a.0.total_cmp(&b.0))
    };
    let min_by = |f: &dyn Fn(&NoaaDay) -> Option<f64>| {
        rows.iter()
            .filter_map(|r| f(r).map(|v| (v, r.date)))
            .min_by(|a, b| a.0.total_cmp(&b.0))
    };
    let count = |f: &dyn Fn(&NoaaDay) -> bool| rows.iter().filter(|r| f(r)).count();
    Totals {
        days: rows.len(),
        mean_max: mean_of(rows.iter().filter_map(|r| r.high)),
        mean_min: mean_of(rows.iter().filter_map(|r| r.low)),
        mean: mean_of(rows.iter().filter_map(|r| r.mean)),
        high: max_by(&|r| r.high),
        low: min_by(&|r| r.low),
        hdd: rows.iter().filter_map(|r| r.hdd).sum(),
        cdd: rows.iter().filter_map(|r| r.cdd).sum(),
        rain: rows.iter().filter_map(|r| r.rain).sum(),
        max_rain: max_by(&|r| r.rain.filter(|v| *v > 0.0)),
        wind_avg: mean_of(rows.iter().filter_map(|r| r.wind_avg)),
        gust: max_by(&|r| r.gust),
        dir: dominant(rows.iter().filter_map(|r| r.dir)),
        max_hot: count(&|r| r.high.is_some_and(|v| v >= t.hot)),
        max_freezing: count(&|r| r.high.is_some_and(|v| v <= t.freezing)),
        min_freezing: count(&|r| r.low.is_some_and(|v| v <= t.freezing)),
        min_very_cold: count(&|r| r.low.is_some_and(|v| v <= t.very_cold)),
        rain_days: [0, 1, 2].map(|k| count(&|r| r.rain.is_some_and(|v| v >= t.rain[k]))),
        normal_mean: None,
        normal_rain: None,
    }
}

/// The station's normals for a month, from its other years: the mean
/// temperature, and the mean rain total of months with 25 or more days.
fn month_normals(days: &[DayRecord], month: u32, exclude: i32) -> (Option<f64>, Option<f64>) {
    let other: Vec<&DayRecord> = days
        .iter()
        .filter(|d| d.date.month() == month && d.date.year() != exclude)
        .collect();
    let mean = mean_of(other.iter().filter_map(|d| day_mean(d)));
    let mut by_year: BTreeMap<i32, (usize, f64)> = BTreeMap::new();
    for d in &other {
        let e = by_year.entry(d.date.year()).or_default();
        e.0 += 1;
        e.1 += d.rain.unwrap_or(0.0);
    }
    let rain = mean_of(by_year.values().filter(|(n, _)| *n >= 25).map(|(_, r)| *r));
    (mean, rain)
}

pub fn noaa_month(days: &[DayRecord], year: i32, month: u32, u: &Units) -> Option<NoaaMonth> {
    let t = Thresholds::for_units(u);
    let mut from_file = false;
    let rows: Vec<NoaaDay> = days
        .iter()
        .filter(|d| d.date.year() == year && d.date.month() == month)
        .map(|d| {
            let (r, f) = noaa_day(d, u, &t);
            from_file |= f;
            r
        })
        .collect();
    if rows.is_empty() {
        return None;
    }
    let mut totals = totals(&rows, &t);
    let (nm, nr) = month_normals(days, month, year);
    totals.normal_mean = nm;
    // A part month is held against the same share of the normal.
    let len = days_in_month(year, month) as f64;
    totals.normal_rain = nr.map(|r| r * rows.len() as f64 / len);
    Some(NoaaMonth {
        year,
        month,
        days: rows,
        totals,
        thresholds: t,
        dayfile_degree_days: from_file,
    })
}

pub fn noaa_year(days: &[DayRecord], year: i32, u: &Units) -> Option<NoaaYear> {
    let t = Thresholds::for_units(u);
    let mut months = Vec::new();
    let mut all = Vec::new();
    let mut from_file = false;
    for m in 1..=12 {
        if let Some(nm) = noaa_month(days, year, m, u) {
            from_file |= nm.dayfile_degree_days;
            all.extend(nm.days.iter().cloned());
            months.push((m, nm.totals));
        }
    }
    if all.is_empty() {
        return None;
    }
    let mut totals = totals(&all, &t);
    let normals: Vec<(Option<f64>, Option<f64>)> = months
        .iter()
        .map(|(_, mt)| (mt.normal_mean, mt.normal_rain))
        .collect();
    totals.normal_mean = mean_of(normals.iter().filter_map(|n| n.0));
    totals.normal_rain = normals
        .iter()
        .all(|n| n.1.is_some())
        .then(|| normals.iter().filter_map(|n| n.1).sum());
    Some(NoaaYear {
        year,
        months,
        totals,
        thresholds: t,
        dayfile_degree_days: from_file,
    })
}

fn days_in_month(y: i32, m: u32) -> u32 {
    let first = NaiveDate::from_ymd_opt(y, m, 1).unwrap();
    let next = if m == 12 {
        NaiveDate::from_ymd_opt(y + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(y, m + 1, 1)
    }
    .unwrap();
    (next - first).num_days() as u32
}

// ------------------------------------------------------------- text output

fn f(v: Option<f64>, width: usize, digits: usize) -> String {
    match v {
        Some(x) => format!("{x:>width$.digits$}"),
        None => format!("{:>width$}", "---"),
    }
}

fn s(v: Option<&str>, width: usize) -> String {
    format!("{:>width$}", v.unwrap_or("---"))
}

/// "16:52" from MX's "4:52 PM" or "16:52".
fn hhmm(t: &Option<String>) -> Option<String> {
    let t = t.as_deref()?.trim();
    let upper = t.to_ascii_uppercase();
    let pm = upper.ends_with("PM");
    let am = upper.ends_with("AM");
    let core = t.trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == ' ');
    let mut parts = core.split(':');
    let mut h: u32 = parts.next()?.trim().parse().ok()?;
    let m: u32 = parts.next()?.trim().get(..2)?.parse().ok()?;
    if pm && h < 12 {
        h += 12;
    }
    if am && h == 12 {
        h = 0;
    }
    Some(format!("{h:02}:{m:02}"))
}

/// A header line: each label right-aligned over its column, as the rows are.
fn header(cols: &[(&str, usize)]) -> String {
    cols.iter()
        .map(|(l, w)| format!("{l:>w$}"))
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end()
        .to_string()
}

fn dir(b: Option<f64>) -> String {
    b.map_or("---".into(), |b| an::compass(b).to_string())
}

/// A value and the day of the month it came on, each padded.
fn day_and(
    v: Option<(f64, NaiveDate)>,
    width: usize,
    digits: usize,
    day_width: usize,
) -> (String, String) {
    match v {
        Some((x, d)) => (
            format!("{x:>width$.digits$}"),
            format!("{:>day_width$}", d.day()),
        ),
        None => (
            format!("{:>width$}", "---"),
            format!("{:>day_width$}", "--"),
        ),
    }
}

fn departure_words(t: &Totals, u: &Units) -> Vec<String> {
    let mut out = Vec::new();
    if let (Some(m), Some(n)) = (t.mean, t.normal_mean) {
        let d = m - n;
        out.push(format!(
            "Mean temperature {m:.1}{}, {:.1}° {} the station's normal.",
            u.temp,
            d.abs(),
            if d >= 0.0 { "above" } else { "below" }
        ));
    }
    if let Some(n) = t.normal_rain.filter(|n| *n > 0.0) {
        out.push(format!(
            "Rain {:.1} {}, {:.0}% of normal ({:.1} {}).",
            t.rain,
            u.rain,
            t.rain / n * 100.0,
            n,
            u.rain
        ));
    }
    out
}

fn degree_note(from_file: bool, t: &Thresholds, u: &Units) -> String {
    if from_file {
        "Degree days from the station's dayfile (its own base temperature).".into()
    } else {
        format!(
            "Degree days from each day's mean against a base of {}{}.",
            t.degree_base, u.temp
        )
    }
}

pub fn noaa_month_text(m: &NoaaMonth, cx: &ReportContext) -> String {
    let u = &cx.units;
    let t = &m.thresholds;
    let mut o = String::new();
    let title = format!(
        "MONTHLY CLIMATOLOGICAL SUMMARY for {}. {}",
        MONTH_ABBR[m.month as usize - 1],
        m.year
    );
    let _ = writeln!(o, "{title:^84}");
    let _ = writeln!(o);
    let _ = writeln!(
        o,
        "NAME: {}{}",
        cx.station,
        cx.latitude.map_or(String::new(), |l| format!(
            "   LAT: {:.1}° {}",
            l.abs(),
            if l < 0.0 { "S" } else { "N" }
        ))
    );
    let _ = writeln!(
        o,
        "{:^84}",
        format!(
            "TEMPERATURE ({}), RAIN ({}), WIND SPEED ({})",
            u.temp, u.rain, u.wind
        )
    );
    let _ = writeln!(o);
    let widths = [3, 5, 6, 6, 5, 6, 5, 5, 5, 6, 5, 6, 5];
    for labels in [
        [
            "", "", "", "", "", "", "HEAT", "COOL", "", "AVG", "", "", "",
        ],
        [
            "", "MEAN", "", "", "", "", "DEG", "DEG", "", "WIND", "", "", "DOM",
        ],
        [
            "DAY", "TEMP", "HIGH", "TIME", "LOW", "TIME", "DAYS", "DAYS", "RAIN", "SPEED", "HIGH",
            "TIME", "DIR",
        ],
    ] {
        let cols: Vec<(&str, usize)> = labels.into_iter().zip(widths).collect();
        let _ = writeln!(o, "{}", header(&cols));
    }
    let rule = "-".repeat(84);
    let _ = writeln!(o, "{rule}");
    for d in &m.days {
        let _ = writeln!(
            o,
            "{:>3} {} {} {} {} {} {} {} {} {} {} {} {:>5}",
            d.date.day(),
            f(d.mean, 5, 1),
            f(d.high, 6, 1),
            s(hhmm(&d.high_time).as_deref(), 6),
            f(d.low, 5, 1),
            s(hhmm(&d.low_time).as_deref(), 6),
            f(d.hdd, 5, 1),
            f(d.cdd, 5, 1),
            f(d.rain, 5, 1),
            f(d.wind_avg, 6, 1),
            f(d.gust, 5, 1),
            s(hhmm(&d.gust_time).as_deref(), 6),
            dir(d.dir),
        );
    }
    let _ = writeln!(o, "{rule}");
    let x = &m.totals;
    let (hi, hi_d) = day_and(x.high, 6, 1, 6);
    let (lo, lo_d) = day_and(x.low, 5, 1, 6);
    let (g, g_d) = day_and(x.gust, 5, 1, 6);
    let _ = writeln!(
        o,
        "    {} {} {} {} {} {:>5.1} {:>5.1} {:>5.1} {} {} {} {:>5}",
        f(x.mean, 5, 1),
        hi,
        hi_d,
        lo,
        lo_d,
        x.hdd,
        x.cdd,
        x.rain,
        f(x.wind_avg, 6, 1),
        g,
        g_d,
        dir(x.dir),
    );
    let _ = writeln!(o);
    let _ = writeln!(o, "Max >= {:>6.1}: {:>3}", t.hot, x.max_hot);
    let _ = writeln!(o, "Max <= {:>6.1}: {:>3}", t.freezing, x.max_freezing);
    let _ = writeln!(o, "Min <= {:>6.1}: {:>3}", t.freezing, x.min_freezing);
    let _ = writeln!(o, "Min <= {:>6.1}: {:>3}", t.very_cold, x.min_very_cold);
    if let Some((r, d)) = x.max_rain {
        let _ = writeln!(
            o,
            "Max rain: {r:.1} {} on {:02}/{:02}/{}",
            u.rain,
            d.day(),
            d.month(),
            d.year()
        );
    }
    let _ = writeln!(
        o,
        "Days of rain: {} (>= {} {}), {} (>= {} {}), {} (>= {} {})",
        x.rain_days[0],
        t.rain[0],
        u.rain,
        x.rain_days[1],
        t.rain[1],
        u.rain,
        x.rain_days[2],
        t.rain[2],
        u.rain
    );
    for line in departure_words(x, u) {
        let _ = writeln!(o, "{line}");
    }
    let _ = writeln!(o, "{}", degree_note(m.dayfile_degree_days, t, u));
    let _ = writeln!(
        o,
        "Average wind speed from the day's wind run. Made by Isobar, {}.",
        cx.made
    );
    o
}

pub fn noaa_year_text(y: &NoaaYear, cx: &ReportContext) -> String {
    let u = &cx.units;
    let t = &y.thresholds;
    let mut o = String::new();
    let rule = "-".repeat(88);
    let _ = writeln!(
        o,
        "{:^88}",
        format!("ANNUAL CLIMATOLOGICAL SUMMARY for {}", y.year)
    );
    let _ = writeln!(o);
    let _ = writeln!(o, "NAME: {}", cx.station);
    let _ = writeln!(o);
    let _ = writeln!(o, "{:^88}", format!("TEMPERATURE ({})", u.temp));
    let limits = [t.hot, t.freezing, t.freezing, t.very_cold].map(|v| format!("{v}"));
    let widths = [6, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 4, 4, 4, 4];
    for labels in [
        [
            "", "", "", "", "DEP.", "HEAT", "COOL", "", "", "", "", "MAX", "MAX", "MIN", "MIN",
        ],
        [
            "", "MEAN", "MEAN", "", "FROM", "DEG", "DEG", "", "", "", "", ">=", "<=", "<=", "<=",
        ],
        [
            " YR MO", "MAX", "MIN", "MEAN", "NORM", "DAYS", "DAYS", "HI", "DATE", "LOW", "DATE",
            &limits[0], &limits[1], &limits[2], &limits[3],
        ],
    ] {
        let cols: Vec<(&str, usize)> = labels.into_iter().zip(widths).collect();
        let _ = writeln!(o, "{}", header(&cols));
    }
    let _ = writeln!(o, "{rule}");
    let temp_row = |o: &mut String, label: String, x: &Totals| {
        let dep = match (x.mean, x.normal_mean) {
            (Some(m), Some(n)) => Some(m - n),
            _ => None,
        };
        let (hi, hi_d) = day_and(x.high, 6, 1, 6);
        let (lo, lo_d) = day_and(x.low, 6, 1, 6);
        let _ = writeln!(
            o,
            "{label} {} {} {} {} {:>6.0} {:>6.0} {} {} {} {} {:>4} {:>4} {:>4} {:>4}",
            f(x.mean_max, 5, 1),
            f(x.mean_min, 5, 1),
            f(x.mean, 5, 1),
            f(dep, 5, 1),
            x.hdd,
            x.cdd,
            hi,
            hi_d,
            lo,
            lo_d,
            x.max_hot,
            x.max_freezing,
            x.min_freezing,
            x.min_very_cold
        );
    };
    for (m, x) in &y.months {
        temp_row(&mut o, format!("{:>3} {:>2}", y.year % 100, m), x);
    }
    let _ = writeln!(o, "{rule}");
    temp_row(&mut o, "      ".into(), &y.totals);
    let _ = writeln!(o);
    let _ = writeln!(o, "{:^88}", format!("RAIN ({})", u.rain));
    let over = t.rain.map(|v| format!(">={v}"));
    let widths = [6, 6, 5, 5, 5, 6, 5, 5];
    for labels in [
        ["", "", "DEP.", "MAX", "", "DAYS", "DAYS", "DAYS"],
        ["", "", "FROM", "OBS.", "", "OF", "OF", "OF"],
        [
            " YR MO", "TOTAL", "NORM", "DAY", "DATE", &over[0], &over[1], &over[2],
        ],
    ] {
        let cols: Vec<(&str, usize)> = labels.into_iter().zip(widths).collect();
        let _ = writeln!(o, "{}", header(&cols));
    }
    let _ = writeln!(o, "{rule}");
    let rain_row = |o: &mut String, label: String, x: &Totals| {
        let dep = x.normal_rain.map(|n| x.rain - n);
        let (mx, mx_d) = day_and(x.max_rain, 5, 1, 5);
        let _ = writeln!(
            o,
            "{label} {:>6.1} {} {} {} {:>6} {:>5} {:>5}",
            x.rain,
            f(dep, 5, 1),
            mx,
            mx_d,
            x.rain_days[0],
            x.rain_days[1],
            x.rain_days[2]
        );
    };
    for (m, x) in &y.months {
        rain_row(&mut o, format!("{:>3} {:>2}", y.year % 100, m), x);
    }
    let _ = writeln!(o, "{rule}");
    rain_row(&mut o, "      ".into(), &y.totals);
    let _ = writeln!(o);
    let _ = writeln!(o, "{:^88}", format!("WIND SPEED ({})", u.wind));
    let widths = [6, 6, 5, 6, 6];
    for labels in [
        ["", "", "", "", "DOM"],
        [" YR MO", "AVG.", "HI", "DATE", "DIR"],
    ] {
        let cols: Vec<(&str, usize)> = labels.into_iter().zip(widths).collect();
        let _ = writeln!(o, "{}", header(&cols));
    }
    let _ = writeln!(o, "{rule}");
    let wind_row = |o: &mut String, label: String, x: &Totals| {
        let (g, g_d) = day_and(x.gust, 5, 1, 6);
        let _ = writeln!(
            o,
            "{label} {} {} {} {:>6}",
            f(x.wind_avg, 6, 1),
            g,
            g_d,
            dir(x.dir)
        );
    };
    for (m, x) in &y.months {
        wind_row(&mut o, format!("{:>3} {:>2}", y.year % 100, m), x);
    }
    let _ = writeln!(o, "{rule}");
    wind_row(&mut o, "      ".into(), &y.totals);
    let _ = writeln!(o);
    for line in departure_words(&y.totals, u) {
        let _ = writeln!(o, "{line}");
    }
    let _ = writeln!(o, "{}", degree_note(y.dayfile_degree_days, t, u));
    let _ = writeln!(
        o,
        "Departures are from the station's own other years. Made by Isobar, {}.",
        cx.made
    );
    o
}

// ------------------------------------------------------------- HTML output

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The page around a report: Isobar's chart-paper look, with print styles.
fn page(title: &str, body: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{t}</title>
<style>
:root {{ --paper:#e9eee8; --sheet:#f6f8f4; --grid:#cdd8cb; --faint:#dfe6dd; --ink:#1c2633; --ink2:#5a6570;
  --trace:#5b3a9e; --heat:#b3362b; --cold:#2a6896; --rain:#2b68ad; --wind:#23796a; --warn:#a8700c; }}
* {{ box-sizing: border-box; }}
body {{ margin: 0; background: var(--paper); color: var(--ink);
  font: 14px/1.5 "Barlow", "Segoe UI", system-ui, sans-serif; }}
main {{ max-width: 1000px; margin: 24px auto; padding: 28px 32px; background: var(--sheet);
  border: 1px solid var(--grid); border-radius: 8px; }}
h1, h2 {{ font-family: "Barlow Condensed", "Arial Narrow", sans-serif; font-weight: 600; margin: 0; }}
h1 {{ font-size: 30px; }}
h2 {{ font-size: 20px; margin-top: 22px; }}
.sub {{ color: var(--ink2); margin: 2px 0 14px; }}
pre {{ font: 12px/1.35 ui-monospace, "Cascadia Mono", Consolas, "DejaVu Sans Mono", monospace;
  overflow-x: auto; background: #fff; border: 1px solid var(--grid); padding: 12px; border-radius: 6px; }}
table {{ border-collapse: collapse; width: 100%; font-size: 12.5px; font-variant-numeric: tabular-nums; }}
th, td {{ padding: 3px 6px; border-bottom: 1px solid var(--faint); text-align: right; white-space: nowrap; }}
th {{ color: var(--ink2); font-weight: 600; border-bottom: 1px solid var(--grid); vertical-align: bottom; }}
td:first-child, th:first-child {{ text-align: left; }}
tr.total td {{ font-weight: 600; border-top: 1px solid var(--ink2); }}
.wrap {{ overflow-x: auto; }}
.tiles {{ display: grid; grid-template-columns: repeat(4, 1fr); gap: 0; border-top: 1px solid var(--grid); border-bottom: 1px solid var(--grid); margin: 12px 0; }}
.tile {{ padding: 10px 12px; border-right: 1px solid var(--faint); }}
.tile:nth-child(4n) {{ border-right: 0; }}
.tile:nth-child(n+5) {{ border-top: 1px solid var(--faint); }}
.tile b {{ display: block; font: 600 24px/1.1 "Barlow Condensed", "Arial Narrow", sans-serif; }}
.tile span {{ color: var(--ink2); font-size: 12.5px; }}
.charts {{ display: grid; grid-template-columns: 2fr 1fr; gap: 18px; align-items: start; }}
svg {{ display: block; width: 100%; height: auto; }}
svg text {{ font: 10px "Barlow", "Segoe UI", sans-serif; fill: var(--ink2); }}
.key {{ display: flex; flex-wrap: wrap; gap: 4px 14px; font-size: 12px; color: var(--ink2); margin: 4px 0; }}
.key i {{ display: inline-block; width: 14px; height: 3px; vertical-align: middle; margin-right: 5px; }}
ul.notes {{ margin: 6px 0; padding-left: 18px; }}
.heat {{ color: var(--heat); }} .cold {{ color: var(--cold); }} .rain {{ color: var(--rain); }}
.foot {{ color: var(--ink2); font-size: 12px; margin-top: 18px; }}
@media (max-width: 700px) {{ main {{ margin: 0; border-radius: 0; padding: 18px 16px; }} .tiles {{ grid-template-columns: repeat(2, 1fr); }} .charts {{ grid-template-columns: 1fr; }} }}
@media print {{
  @page {{ size: A4; margin: 12mm; }}
  body {{ background: #fff; }}
  main {{ margin: 0; padding: 0; border: 0; max-width: none; background: #fff; }}
  pre {{ border: 0; padding: 0; font-size: 10px; }}
  h2 {{ break-after: avoid; }}
  table, svg, .tiles {{ break-inside: avoid; }}
}}
</style>
</head>
<body>
<main>
{b}
</main>
</body>
</html>
"#,
        t = esc(title),
        b = body
    )
}

fn h(v: Option<f64>, digits: usize) -> String {
    v.map_or("–".into(), |x| format!("{x:.digits$}"))
}

fn short(d: NaiveDate) -> String {
    format!("{} {}", d.day(), &MONTH_NAMES[d.month0() as usize][..3])
}

pub fn noaa_month_html(m: &NoaaMonth, cx: &ReportContext) -> String {
    let u = &cx.units;
    let name = format!("{} {}", MONTH_NAMES[m.month as usize - 1], m.year);
    let mut b = String::new();
    let _ = write!(
        b,
        "<h1>Monthly climate summary: {}</h1><p class=\"sub\">{} · temperature {}, rain {}, wind {}</p>",
        esc(&name),
        esc(&cx.station),
        esc(&u.temp),
        esc(&u.rain),
        esc(&u.wind)
    );
    b.push_str("<div class=\"wrap\"><table><thead><tr><th>Day</th><th>Mean</th><th>High</th><th>Time</th><th>Low</th><th>Time</th><th>Heat DD</th><th>Cool DD</th><th>Rain</th><th>Avg wind</th><th>Gust</th><th>Time</th><th>Dir</th></tr></thead><tbody>");
    for d in &m.days {
        let _ = write!(
            b,
            "<tr><td>{}</td><td>{}</td><td class=\"heat\">{}</td><td>{}</td><td class=\"cold\">{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"rain\">{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            d.date.day(),
            h(d.mean, 1),
            h(d.high, 1),
            hhmm(&d.high_time).unwrap_or_default(),
            h(d.low, 1),
            hhmm(&d.low_time).unwrap_or_default(),
            h(d.hdd, 1),
            h(d.cdd, 1),
            h(d.rain, 1),
            h(d.wind_avg, 1),
            h(d.gust, 1),
            hhmm(&d.gust_time).unwrap_or_default(),
            dir(d.dir)
        );
    }
    let x = &m.totals;
    let at = |v: Option<(f64, NaiveDate)>| {
        v.map_or("–".into(), |(v, d)| format!("{v:.1} ({})", d.day()))
    };
    let _ = write!(
        b,
        "<tr class=\"total\"><td>Month</td><td>{}</td><td class=\"heat\">{}</td><td></td><td class=\"cold\">{}</td><td></td><td>{:.1}</td><td>{:.1}</td><td class=\"rain\">{:.1}</td><td>{}</td><td>{}</td><td></td><td>{}</td></tr></tbody></table></div>",
        h(x.mean, 1),
        at(x.high),
        at(x.low),
        x.hdd,
        x.cdd,
        x.rain,
        h(x.wind_avg, 1),
        at(x.gust),
        dir(x.dir)
    );
    let t = &m.thresholds;
    b.push_str("<h2>The month in figures</h2><ul class=\"notes\">");
    for line in departure_words(x, u) {
        let _ = write!(b, "<li>{}</li>", esc(&line));
    }
    let _ = write!(
        b,
        "<li>Days with a high of {h} or more: {}; at or below {f}: {}. Nights at or below {f}: {}; at or below {c}: {}.</li>",
        x.max_hot,
        x.max_freezing,
        x.min_freezing,
        x.min_very_cold,
        h = t.hot,
        f = t.freezing,
        c = t.very_cold
    );
    let _ = write!(
        b,
        "<li>Days of rain: {} of {r0} {u} or more, {} of {r1}, {} of {r2}.{}</li></ul>",
        x.rain_days[0],
        x.rain_days[1],
        x.rain_days[2],
        x.max_rain.map_or(String::new(), |(r, d)| format!(
            " Wettest day {r:.1} {} on {}.",
            esc(&u.rain),
            short(d)
        )),
        r0 = t.rain[0],
        r1 = t.rain[1],
        r2 = t.rain[2],
        u = esc(&u.rain)
    );
    let _ = write!(
        b,
        "<p class=\"foot\">{} Average wind from each day's wind run. Departures are from the station's own other years. Made by Isobar on {}.</p>",
        esc(&degree_note(m.dayfile_degree_days, t, u)),
        esc(&cx.made)
    );
    b.push_str("<h2>As text</h2><pre>");
    b.push_str(&esc(&noaa_month_text(m, cx)));
    b.push_str("</pre>");
    page(&format!("Climate summary {name}, {}", cx.station), &b)
}

pub fn noaa_year_html(y: &NoaaYear, cx: &ReportContext) -> String {
    let u = &cx.units;
    let mut b = String::new();
    let _ = write!(
        b,
        "<h1>Yearly climate summary: {}</h1><p class=\"sub\">{} · temperature {}, rain {}, wind {}</p>",
        y.year,
        esc(&cx.station),
        esc(&u.temp),
        esc(&u.rain),
        esc(&u.wind)
    );
    let t = &y.thresholds;
    let at = |v: Option<(f64, NaiveDate)>| {
        v.map_or("–".into(), |(v, d)| format!("{v:.1} ({})", short(d)))
    };
    b.push_str("<h2>Temperature</h2><div class=\"wrap\"><table><thead><tr><th>Month</th><th>Mean max</th><th>Mean min</th><th>Mean</th><th>From normal</th><th>Heat DD</th><th>Cool DD</th><th>Highest</th><th>Lowest</th>");
    let _ = write!(
        b,
        "<th>Max ≥ {}</th><th>Max ≤ {}</th><th>Min ≤ {}</th><th>Min ≤ {}</th></tr></thead><tbody>",
        t.hot, t.freezing, t.freezing, t.very_cold
    );
    let temp_row = |b: &mut String, label: &str, x: &Totals, class: &str| {
        let dep = match (x.mean, x.normal_mean) {
            (Some(m), Some(n)) => format!("{:+.1}", m - n),
            _ => "–".into(),
        };
        let _ = write!(
            b,
            "<tr class=\"{class}\"><td>{label}</td><td class=\"heat\">{}</td><td class=\"cold\">{}</td><td>{}</td><td>{dep}</td><td>{:.0}</td><td>{:.0}</td><td class=\"heat\">{}</td><td class=\"cold\">{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            h(x.mean_max, 1),
            h(x.mean_min, 1),
            h(x.mean, 1),
            x.hdd,
            x.cdd,
            at(x.high),
            at(x.low),
            x.max_hot,
            x.max_freezing,
            x.min_freezing,
            x.min_very_cold
        );
    };
    for (m, x) in &y.months {
        temp_row(&mut b, MONTH_NAMES[*m as usize - 1], x, "");
    }
    temp_row(&mut b, "Year", &y.totals, "total");
    b.push_str("</tbody></table></div>");
    let _ = write!(
        b,
        "<h2>Rain ({})</h2><div class=\"wrap\"><table><thead><tr><th>Month</th><th>Total</th><th>From normal</th><th>Wettest day</th><th>Days ≥ {}</th><th>Days ≥ {}</th><th>Days ≥ {}</th></tr></thead><tbody>",
        esc(&u.rain),
        t.rain[0],
        t.rain[1],
        t.rain[2]
    );
    let rain_row = |b: &mut String, label: &str, x: &Totals, class: &str| {
        let dep = x
            .normal_rain
            .map_or("–".into(), |n| format!("{:+.1}", x.rain - n));
        let _ = write!(
            b,
            "<tr class=\"{class}\"><td>{label}</td><td class=\"rain\">{:.1}</td><td>{dep}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            x.rain,
            at(x.max_rain),
            x.rain_days[0],
            x.rain_days[1],
            x.rain_days[2]
        );
    };
    for (m, x) in &y.months {
        rain_row(&mut b, MONTH_NAMES[*m as usize - 1], x, "");
    }
    rain_row(&mut b, "Year", &y.totals, "total");
    b.push_str("</tbody></table></div>");
    let _ = write!(
        b,
        "<h2>Wind ({})</h2><div class=\"wrap\"><table><thead><tr><th>Month</th><th>Average</th><th>Peak gust</th><th>Mostly from</th></tr></thead><tbody>",
        esc(&u.wind)
    );
    let wind_row = |b: &mut String, label: &str, x: &Totals, class: &str| {
        let _ = write!(
            b,
            "<tr class=\"{class}\"><td>{label}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            h(x.wind_avg, 1),
            at(x.gust),
            dir(x.dir)
        );
    };
    for (m, x) in &y.months {
        wind_row(&mut b, MONTH_NAMES[*m as usize - 1], x, "");
    }
    wind_row(&mut b, "Year", &y.totals, "total");
    b.push_str("</tbody></table></div><ul class=\"notes\">");
    for line in departure_words(&y.totals, u) {
        let _ = write!(b, "<li>{}</li>", esc(&line));
    }
    let _ = write!(
        b,
        "</ul><p class=\"foot\">{} Average wind from each day's wind run. Departures are from the station's own other years. Made by Isobar on {}.</p>",
        esc(&degree_note(y.dayfile_degree_days, t, u)),
        esc(&cx.made)
    );
    b.push_str("<h2>As text</h2><pre>");
    b.push_str(&esc(&noaa_year_text(y, cx)));
    b.push_str("</pre>");
    page(&format!("Climate summary {}, {}", y.year, cx.station), &b)
}

// ---------------------------------------------------------- month in review

/// A day in the month in review, with the normals for its date.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewDay {
    pub date: NaiveDate,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub normal_high: Option<f64>,
    pub normal_low: Option<f64>,
    pub rain: f64,
    pub normal_rain: Option<f64>,
    pub gust: Option<f64>,
    pub dir: Option<f64>,
    pub sunshine: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonthReview {
    pub noaa: NoaaMonth,
    pub days: Vec<ReviewDay>,
    /// Days well off normal: (date, words).
    pub notable: Vec<(NaiveDate, String)>,
    /// Station records for this month the month set or equalled.
    pub records: Vec<String>,
    pub sunshine: Option<f64>,
    pub normal_sunshine: Option<f64>,
    pub rose: ([usize; 16], usize),
}

pub fn month_review(days: &[DayRecord], year: i32, month: u32, u: &Units) -> Option<MonthReview> {
    let noaa = noaa_month(days, year, month, u)?;
    let nmax = climatology(days, Field::TempMax, 7, Some(year));
    let nmin = climatology(days, Field::TempMin, 7, Some(year));
    let nrain = climatology(days, Field::Rain, 7, Some(year));
    let sel: Vec<DayRecord> = days
        .iter()
        .filter(|d| d.date.year() == year && d.date.month() == month)
        .cloned()
        .collect();
    let rows: Vec<ReviewDay> = sel
        .iter()
        .map(|d| {
            let k = doy_index(d.date);
            ReviewDay {
                date: d.date,
                high: d.temp_max,
                low: d.temp_min,
                normal_high: nmax[k].map(|n| n.mean),
                normal_low: nmin[k].map(|n| n.mean),
                rain: d.rain.unwrap_or(0.0),
                normal_rain: nrain[k].map(|n| n.mean),
                gust: d.gust_max,
                dir: d.wind_dir_dominant,
                sunshine: d.sunshine,
            }
        })
        .collect();
    let first = sel.first()?.date;
    let last = sel.last()?.date;
    let notable: Vec<(NaiveDate, String)> = {
        let mut a = an::anomalies(days, first, last, 1.5);
        a.sort_by_key(|x| x.date);
        a.iter()
            .map(|x| {
                let warm = x.value > x.normal;
                let what = match (x.field, warm) {
                    (Field::TempMax, true) => "hot day",
                    (Field::TempMax, false) => "cold day",
                    (_, true) => "warm night",
                    (_, false) => "cold night",
                };
                (
                    x.date,
                    format!(
                        "A {what}: {:.1}{} against a normal of {:.1}",
                        x.value, u.temp, x.normal
                    ),
                )
            })
            .collect()
    };
    // Records for the calendar month, from the other years.
    let others: Vec<&DayRecord> = days
        .iter()
        .filter(|d| d.date.month() == month && d.date.year() != year)
        .collect();
    let mut records = Vec::new();
    let mut check = |label: &str,
                     mine: Option<(f64, NaiveDate)>,
                     theirs: Option<f64>,
                     higher: bool,
                     unit: &str| {
        if let (Some((v, d)), Some(o)) = (mine, theirs) {
            if (higher && v >= o) || (!higher && v <= o) {
                records.push(format!(
                    "{label} for {} on record: {v:.1}{unit} on {} (the record was {o:.1}).",
                    MONTH_NAMES[month as usize - 1],
                    short(d)
                ));
            }
        }
    };
    let max = |f: fn(&DayRecord) -> Option<f64>| {
        others
            .iter()
            .filter_map(|d| f(d))
            .fold(None, |a: Option<f64>, v| Some(a.map_or(v, |a| a.max(v))))
    };
    let min = |f: fn(&DayRecord) -> Option<f64>| {
        others
            .iter()
            .filter_map(|d| f(d))
            .fold(None, |a: Option<f64>, v| Some(a.map_or(v, |a| a.min(v))))
    };
    let x = &noaa.totals;
    if !others.is_empty() {
        check("Hottest day", x.high, max(|d| d.temp_max), true, &u.temp);
        check("Coldest night", x.low, min(|d| d.temp_min), false, &u.temp);
        check(
            "Wettest day",
            x.max_rain,
            max(|d| d.rain),
            true,
            &format!(" {}", u.rain),
        );
        check(
            "Strongest gust",
            x.gust,
            max(|d| d.gust_max),
            true,
            &format!(" {}", u.wind),
        );
    }
    let sun: Vec<f64> = sel.iter().filter_map(|d| d.sunshine).collect();
    let sunshine = (!sun.is_empty()).then(|| sun.iter().sum());
    let normal_sunshine = {
        let mut by_year: BTreeMap<i32, (usize, f64)> = BTreeMap::new();
        for d in &others {
            if let Some(s) = d.sunshine {
                let e = by_year.entry(d.date.year()).or_default();
                e.0 += 1;
                e.1 += s;
            }
        }
        mean_of(by_year.values().filter(|(n, _)| *n >= 25).map(|(_, s)| *s))
            .map(|s| s * sel.len() as f64 / days_in_month(year, month) as f64)
    };
    Some(MonthReview {
        rose: an::wind_rose(&sel),
        noaa,
        days: rows,
        notable,
        records,
        sunshine,
        normal_sunshine,
    })
}

/// Scale `v` from `lo..hi` onto `a..b`.
fn sc(v: f64, lo: f64, hi: f64, a: f64, b: f64) -> f64 {
    if hi <= lo {
        return (a + b) / 2.0;
    }
    a + (v - lo) / (hi - lo) * (b - a)
}

fn nice_step(range: f64, ticks: f64) -> f64 {
    let raw = (range / ticks).max(1e-9);
    let mag = 10f64.powf(raw.log10().floor());
    let n = raw / mag;
    mag * if n <= 1.0 {
        1.0
    } else if n <= 2.0 {
        2.0
    } else if n <= 5.0 {
        5.0
    } else {
        10.0
    }
}

/// A polyline through the points that have values, broken at gaps.
fn path(points: &[Option<(f64, f64)>]) -> String {
    let mut d = String::new();
    let mut pen = false;
    for p in points {
        match p {
            Some((x, y)) => {
                let _ = write!(d, "{}{x:.1},{y:.1} ", if pen { "L" } else { "M" });
                pen = true;
            }
            None => pen = false,
        }
    }
    d
}

const W: f64 = 640.0;
const PAD_L: f64 = 34.0;
const PAD_R: f64 = 34.0;

#[allow(clippy::too_many_arguments)]
fn y_axis(
    o: &mut String,
    lo: f64,
    hi: f64,
    top: f64,
    bottom: f64,
    x: f64,
    anchor: &str,
    step: f64,
) {
    let mut v = (lo / step).ceil() * step;
    while v <= hi + 1e-9 {
        let y = sc(v, lo, hi, bottom, top);
        if anchor == "end" {
            let _ = write!(
                o,
                "<line x1=\"{PAD_L}\" x2=\"{:.1}\" y1=\"{y:.1}\" y2=\"{y:.1}\" stroke=\"#cdd8cb\" stroke-width=\"0.6\"/>",
                W - PAD_R
            );
        }
        let _ = write!(
            o,
            "<text x=\"{x:.1}\" y=\"{:.1}\" text-anchor=\"{anchor}\">{}</text>",
            y + 3.0,
            if step < 1.0 {
                format!("{v:.1}")
            } else {
                format!("{v:.0}")
            }
        );
        v += step;
    }
}

fn x_days(o: &mut String, n: usize, bottom: f64) {
    for k in 0..n {
        let day = k + 1;
        if day == 1 || day % 5 == 0 {
            let x = sc(
                k as f64,
                0.0,
                (n.max(2) - 1) as f64,
                PAD_L + 6.0,
                W - PAD_R - 6.0,
            );
            let _ = write!(
                o,
                "<text x=\"{x:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{day}</text>",
                bottom + 14.0
            );
        }
    }
}

fn temperature_svg(r: &MonthReview) -> String {
    let (top, bottom) = (10.0, 180.0);
    let n = r.days.len();
    let vals: Vec<f64> = r
        .days
        .iter()
        .flat_map(|d| [d.high, d.low, d.normal_high, d.normal_low])
        .flatten()
        .collect();
    if vals.is_empty() {
        return String::new();
    }
    let (mut lo, mut hi) = vals
        .iter()
        .fold((f64::MAX, f64::MIN), |a, v| (a.0.min(*v), a.1.max(*v)));
    let step = nice_step(hi - lo, 5.0);
    lo = (lo / step).floor() * step;
    hi = (hi / step).ceil() * step;
    let xs = |k: usize| {
        sc(
            k as f64,
            0.0,
            (n.max(2) - 1) as f64,
            PAD_L + 6.0,
            W - PAD_R - 6.0,
        )
    };
    let pt = |k: usize, v: Option<f64>| v.map(|v| (xs(k), sc(v, lo, hi, bottom, top)));
    let mut o = format!("<svg viewBox=\"0 0 {W} 200\" role=\"img\" aria-label=\"Daily high and low temperatures against normal\">");
    y_axis(&mut o, lo, hi, top, bottom, PAD_L - 4.0, "end", step);
    x_days(&mut o, n, bottom);
    // The band between high and low.
    let upper: Vec<String> = (0..n)
        .filter_map(|k| pt(k, r.days[k].high).map(|(x, y)| format!("{x:.1},{y:.1}")))
        .collect();
    let lower: Vec<String> = (0..n)
        .rev()
        .filter_map(|k| pt(k, r.days[k].low).map(|(x, y)| format!("{x:.1},{y:.1}")))
        .collect();
    if !upper.is_empty() && !lower.is_empty() {
        let _ = write!(
            o,
            "<polygon points=\"{} {}\" fill=\"#5b3a9e\" fill-opacity=\"0.08\"/>",
            upper.join(" "),
            lower.join(" ")
        );
    }
    let line =
        |o: &mut String, pts: Vec<Option<(f64, f64)>>, color: &str, dash: bool, width: f64| {
            let _ = write!(
                o,
                "<path d=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{width}\"{}/>",
                path(&pts),
                if dash {
                    " stroke-dasharray=\"4 3\""
                } else {
                    ""
                }
            );
        };
    line(
        &mut o,
        (0..n).map(|k| pt(k, r.days[k].normal_high)).collect(),
        "#b3362b",
        true,
        1.0,
    );
    line(
        &mut o,
        (0..n).map(|k| pt(k, r.days[k].normal_low)).collect(),
        "#2a6896",
        true,
        1.0,
    );
    line(
        &mut o,
        (0..n).map(|k| pt(k, r.days[k].high)).collect(),
        "#b3362b",
        false,
        1.8,
    );
    line(
        &mut o,
        (0..n).map(|k| pt(k, r.days[k].low)).collect(),
        "#2a6896",
        false,
        1.8,
    );
    o.push_str("</svg>");
    o
}

fn rain_svg(r: &MonthReview) -> String {
    let (top, bottom) = (10.0, 150.0);
    let n = r.days.len();
    let max_day = r.days.iter().map(|d| d.rain).fold(0.0, f64::max).max(1.0);
    let mut cum = 0.0;
    let mut ncum = 0.0;
    let cums: Vec<(f64, Option<f64>)> = r
        .days
        .iter()
        .map(|d| {
            cum += d.rain;
            ncum += d.normal_rain.unwrap_or(0.0);
            (cum, d.normal_rain.map(|_| ncum))
        })
        .collect();
    let max_cum = cums
        .iter()
        .map(|c| c.0.max(c.1.unwrap_or(0.0)))
        .fold(0.0, f64::max)
        .max(1.0);
    let ls = nice_step(max_day, 4.0);
    let lhi = (max_day / ls).ceil() * ls;
    let rs = nice_step(max_cum, 4.0);
    let rhi = (max_cum / rs).ceil() * rs;
    let xs = |k: usize| {
        sc(
            k as f64,
            0.0,
            (n.max(2) - 1) as f64,
            PAD_L + 6.0,
            W - PAD_R - 6.0,
        )
    };
    let bar = ((W - PAD_L - PAD_R) / n.max(1) as f64 * 0.6).max(2.0);
    let mut o = format!("<svg viewBox=\"0 0 {W} 170\" role=\"img\" aria-label=\"Daily rain and the running total against normal\">");
    y_axis(&mut o, 0.0, lhi, top, bottom, PAD_L - 4.0, "end", ls);
    y_axis(&mut o, 0.0, rhi, top, bottom, W - PAD_R + 4.0, "start", rs);
    x_days(&mut o, n, bottom);
    for (k, d) in r.days.iter().enumerate() {
        if d.rain > 0.0 {
            let y = sc(d.rain, 0.0, lhi, bottom, top);
            let _ = write!(
                o,
                "<rect x=\"{:.1}\" y=\"{y:.1}\" width=\"{bar:.1}\" height=\"{:.1}\" fill=\"#2b68ad\" fill-opacity=\"0.75\"/>",
                xs(k) - bar / 2.0,
                bottom - y
            );
        }
    }
    let to = |v: f64| sc(v, 0.0, rhi, bottom, top);
    let normal: Vec<Option<(f64, f64)>> = cums
        .iter()
        .enumerate()
        .map(|(k, c)| c.1.map(|v| (xs(k), to(v))))
        .collect();
    let actual: Vec<Option<(f64, f64)>> = cums
        .iter()
        .enumerate()
        .map(|(k, c)| Some((xs(k), to(c.0))))
        .collect();
    let _ = write!(
        o,
        "<path d=\"{}\" fill=\"none\" stroke=\"#5a6570\" stroke-width=\"1.2\" stroke-dasharray=\"4 3\"/><path d=\"{}\" fill=\"none\" stroke=\"#5b3a9e\" stroke-width=\"1.8\"/></svg>",
        path(&normal),
        path(&actual)
    );
    o
}

fn rose_svg(rose: &([usize; 16], usize)) -> String {
    let (cx, cy, rmax) = (110.0, 110.0, 84.0);
    let mut o = String::from("<svg viewBox=\"0 0 220 230\" role=\"img\" aria-label=\"Wind rose\">");
    for f in [0.33, 0.66, 1.0] {
        let _ = write!(
            o,
            "<circle cx=\"{cx}\" cy=\"{cy}\" r=\"{:.1}\" fill=\"none\" stroke=\"#cdd8cb\" stroke-width=\"0.7\"/>",
            rmax * f
        );
    }
    for (label, dx, dy) in [
        ("N", 0.0, -1.0),
        ("E", 1.0, 0.0),
        ("S", 0.0, 1.0),
        ("W", -1.0, 0.0),
    ] {
        let _ = write!(
            o,
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{label}</text>",
            cx + dx * (rmax + 12.0),
            cy + dy * (rmax + 12.0) + 3.5
        );
    }
    let total = rose.1.max(1) as f64;
    let peak = rose.0.iter().copied().max().unwrap_or(0).max(1) as f64;
    for (k, c) in rose.0.iter().enumerate() {
        if *c == 0 {
            continue;
        }
        let r = rmax * (*c as f64 / peak).sqrt();
        let mid = (k as f64 * 22.5).to_radians();
        let half = 9.5f64.to_radians();
        let p = |a: f64| (cx + r * a.sin(), cy - r * a.cos());
        let (a, b) = (p(mid - half), p(mid + half));
        let _ = write!(
            o,
            "<path d=\"M{cx},{cy} L{:.1},{:.1} A{r:.1},{r:.1} 0 0 1 {:.1},{:.1} Z\" fill=\"#23796a\" fill-opacity=\"0.8\"><title>{} {:.0}%</title></path>",
            a.0,
            a.1,
            b.0,
            b.1,
            COMPASS16[k],
            *c as f64 / total * 100.0
        );
    }
    let top = (0..16).max_by_key(|i| rose.0[*i]).unwrap_or(0);
    let _ = write!(
        o,
        "<text x=\"{cx}\" y=\"224\" text-anchor=\"middle\">{}</text></svg>",
        if rose.1 == 0 {
            "No wind directions recorded".to_string()
        } else {
            format!("Mostly from the {}, {} days", COMPASS16[top], rose.1)
        }
    );
    o
}

pub fn month_review_html(r: &MonthReview, cx: &ReportContext) -> String {
    let u = &cx.units;
    let m = &r.noaa;
    let x = &m.totals;
    let name = format!("{} {}", MONTH_NAMES[m.month as usize - 1], m.year);
    let mut b = String::new();
    let part = m.days.len() < days_in_month(m.year, m.month) as usize;
    let _ = write!(
        b,
        "<h1>{} in review</h1><p class=\"sub\">{}{}</p>",
        esc(&name),
        esc(&cx.station),
        if part {
            format!(" · {} days so far", m.days.len())
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
    let dep = match (x.mean, x.normal_mean) {
        (Some(a), Some(n)) => format!("{:+.1}° against normal", a - n),
        _ => "Mean temperature".into(),
    };
    tile(
        &mut b,
        format!("{}{}", h(x.mean, 1), u.temp),
        format!("Mean temperature, {dep}"),
        "",
    );
    let pct = x
        .normal_rain
        .filter(|n| *n > 0.0)
        .map_or("Rain".to_string(), |n| {
            format!("Rain, {:.0}% of normal", x.rain / n * 100.0)
        });
    tile(&mut b, format!("{:.1} {}", x.rain, u.rain), pct, "rain");
    tile(
        &mut b,
        x.rain_days[0].to_string(),
        format!("Days with {} {} or more", m.thresholds.rain[0], u.rain),
        "rain",
    );
    tile(
        &mut b,
        x.gust
            .map_or("–".into(), |g| format!("{:.0} {}", g.0, u.wind)),
        x.gust
            .map_or("Peak gust".into(), |g| format!("Peak gust, {}", short(g.1))),
        "",
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
        format!("Nights at or below {}{}", m.thresholds.freezing, u.temp),
        "cold",
    );
    match (r.sunshine, r.normal_sunshine) {
        (Some(s), Some(n)) if n > 0.0 => tile(
            &mut b,
            format!("{s:.0} h"),
            format!("Sunshine, {:.0}% of normal", s / n * 100.0),
            "",
        ),
        (Some(s), _) => tile(&mut b, format!("{s:.0} h"), "Sunshine".into(), ""),
        _ => tile(&mut b, dir(x.dir), "Wind mostly from".into(), ""),
    }
    b.push_str("</div>");
    b.push_str("<h2>Temperature</h2><div class=\"key\"><span><i style=\"background:#b3362b\"></i>High</span><span><i style=\"background:#2a6896\"></i>Low</span><span><i style=\"border-top:1px dashed #5a6570;height:0\"></i>Normal for each date</span></div>");
    b.push_str(&temperature_svg(r));
    b.push_str("<div class=\"charts\"><div><h2>Rain</h2><div class=\"key\"><span><i style=\"background:#2b68ad\"></i>Each day (left)</span><span><i style=\"background:#5b3a9e\"></i>Running total (right)</span><span><i style=\"border-top:1px dashed #5a6570;height:0\"></i>Normal running total</span></div>");
    b.push_str(&rain_svg(r));
    b.push_str("</div><div><h2>Wind</h2>");
    b.push_str(&rose_svg(&r.rose));
    b.push_str("</div></div>");
    if !r.records.is_empty() || !r.notable.is_empty() {
        b.push_str("<h2>Worth remembering</h2><ul class=\"notes\">");
        for rec in &r.records {
            let _ = write!(b, "<li><b>{}</b></li>", esc(rec));
        }
        for (d, words) in r.notable.iter().take(10) {
            let _ = write!(b, "<li>{}: {}.</li>", short(*d), esc(words));
        }
        b.push_str("</ul>");
    }
    b.push_str("<h2>Day by day</h2><div class=\"wrap\"><table><thead><tr><th>Day</th><th>High</th><th>Low</th><th>Rain</th><th>Gust</th><th>Wind from</th>");
    let has_sun = r.days.iter().any(|d| d.sunshine.is_some());
    if has_sun {
        b.push_str("<th>Sunshine</th>");
    }
    b.push_str("</tr></thead><tbody>");
    for d in &r.days {
        let _ = write!(
            b,
            "<tr><td>{}</td><td class=\"heat\">{}</td><td class=\"cold\">{}</td><td class=\"rain\">{}</td><td>{}</td><td>{}</td>",
            short(d.date),
            h(d.high, 1),
            h(d.low, 1),
            if d.rain > 0.0 { format!("{:.1}", d.rain) } else { String::new() },
            h(d.gust, 0),
            dir(d.dir)
        );
        if has_sun {
            let _ = write!(b, "<td>{}</td>", h(d.sunshine, 1));
        }
        b.push_str("</tr>");
    }
    b.push_str("</tbody></table></div>");
    let _ = write!(
        b,
        "<p class=\"foot\">Normals are the station's own: the average of the same dates, a week either side, in every other year. Units: {}, {}, {}. Made by Isobar on {}.</p>",
        esc(&u.temp),
        esc(&u.rain),
        esc(&u.wind),
        esc(&cx.made)
    );
    page(&format!("{name} in review, {}", cx.station), &b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn days() -> Vec<DayRecord> {
        let mut out = Vec::new();
        for y in [2024, 2025] {
            for d in 1..=31 {
                out.push(DayRecord {
                    date: date(y, 10, d),
                    temp_max: Some(20.0 + (y - 2024) as f64 * 2.0 + (d % 3) as f64),
                    temp_max_time: Some("2:52 PM".into()),
                    temp_min: Some(10.0 - (d % 4) as f64),
                    temp_min_time: Some("05:41".into()),
                    rain: Some(if d % 5 == 0 { 4.0 } else { 0.0 }),
                    gust_max: Some(30.0 + d as f64),
                    gust_time: Some("13:18".into()),
                    wind_run: Some(240.0),
                    wind_dir_dominant: Some(202.5),
                    ..Default::default()
                });
            }
        }
        out
    }

    fn cx() -> ReportContext {
        ReportContext {
            station: "Home".into(),
            units: Units::default(),
            latitude: Some(-38.6),
            made: "20 Oct 2026".into(),
        }
    }

    #[test]
    fn monthly_summary() {
        let all = days();
        let m = noaa_month(&all, 2025, 10, &Units::default()).unwrap();
        assert_eq!(m.days.len(), 31);
        let t = &m.totals;
        assert_eq!(t.rain, 24.0);
        assert_eq!(t.rain_days, [6, 6, 0]);
        assert_eq!(t.high.unwrap().0, 24.0);
        assert_eq!(t.gust.unwrap(), (61.0, date(2025, 10, 31)));
        assert_eq!(
            m.days[0].wind_avg,
            Some(10.0),
            "240 km of wind run is 10 km/h"
        );
        // The other year was 2° cooler by day; its rain was the same.
        assert!((t.mean.unwrap() - t.normal_mean.unwrap() - 1.0).abs() < 1e-9);
        assert_eq!(t.normal_rain, Some(24.0));
        assert!(!m.dayfile_degree_days);
        let text = noaa_month_text(&m, &cx());
        assert!(text.contains("MONTHLY CLIMATOLOGICAL SUMMARY for OCT. 2025"));
        assert!(text.contains("14:52"), "12-hour times become 24-hour");
        assert!(text.contains("100% of normal"));
        let html = noaa_month_html(&m, &cx());
        assert!(html.starts_with("<!doctype html>"));
    }

    #[test]
    fn yearly_summary_and_review() {
        let all = days();
        let y = noaa_year(&all, 2025, &Units::default()).unwrap();
        assert_eq!(y.months.len(), 1);
        assert_eq!(y.totals.rain, 24.0);
        assert!(noaa_year_text(&y, &cx()).contains("ANNUAL CLIMATOLOGICAL SUMMARY for 2025"));
        let r = month_review(&all, 2025, 10, &Units::default()).unwrap();
        assert_eq!(r.days.len(), 31);
        assert!(r.records.iter().any(|s| s.starts_with("Hottest day")));
        assert_eq!(r.rose.1, 31);
        let html = month_review_html(&r, &cx());
        assert!(html.contains("<svg"));
        assert!(noaa_month(&all, 2023, 10, &Units::default()).is_none());
    }
}
