//! Statistics over the daily history: seasonal normals, anomalies, period
//! summaries, aggregation for long ranges, cross-year series, records and the
//! monthly climate table.
//!
//! Every function takes the days sorted oldest first, as [`crate::Store::days`]
//! returns them.

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, NaiveDate};

use crate::dayfile::DayRecord;

/// A day counts as a rain day at or above this (0.2 mm is the smallest reliable tip).
pub const RAIN_DAY: f64 = 0.2;

/// The numeric dayfile fields the analytics use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Field {
    TempMax,
    TempMin,
    Rain,
    RainRateMax,
    GustMax,
    WindAvgMax,
    WindDirDominant,
    PressMin,
    PressMax,
}

impl Field {
    pub fn get(self, d: &DayRecord) -> Option<f64> {
        let v = match self {
            Field::TempMax => d.temp_max,
            Field::TempMin => d.temp_min,
            Field::Rain => d.rain,
            Field::RainRateMax => d.rain_rate_max,
            Field::GustMax => d.gust_max,
            Field::WindAvgMax => d.wind_avg_max,
            Field::WindDirDominant => d.wind_dir_dominant,
            Field::PressMin => d.press_min,
            Field::PressMax => d.press_max,
        };
        v.filter(|x| x.is_finite())
    }

    /// The time of day the value was reached, for fields that record one.
    pub fn time(self, d: &DayRecord) -> Option<&str> {
        match self {
            Field::TempMax => d.temp_max_time.as_deref(),
            Field::TempMin => d.temp_min_time.as_deref(),
            Field::GustMax => d.gust_time.as_deref(),
            _ => None,
        }
    }
}

fn rain(d: &DayRecord) -> f64 {
    Field::Rain.get(d).unwrap_or(0.0)
}

pub fn is_rain_day(d: &DayRecord) -> bool {
    rain(d) >= RAIN_DAY
}

/// The days from `start` to `end` inclusive.
pub fn in_range(days: &[DayRecord], start: NaiveDate, end: NaiveDate) -> &[DayRecord] {
    let a = days.partition_point(|d| d.date < start);
    let b = days.partition_point(|d| d.date <= end);
    &days[a..b.max(a)]
}

// ---------------------------------------------------------------- day of year

/// 0..=365, using a leap year so 29 February gets its own slot.
pub fn doy_index(date: NaiveDate) -> usize {
    NaiveDate::from_ymd_opt(2000, date.month(), date.day())
        .map(|d| d.ordinal0() as usize)
        .unwrap_or(0)
}

/// Day-of-year index of the first of each month in a leap year.
pub const MONTH_STARTS: [usize; 12] = [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Normal {
    pub mean: f64,
    pub sd: f64,
    pub n: usize,
}

/// Climatology for one field: for each day of the year, the mean and standard
/// deviation of all values within ±`window` days of that date, across every
/// year except `exclude_year` (so a day isn't compared against itself).
pub fn climatology(
    days: &[DayRecord],
    field: Field,
    window: usize,
    exclude_year: Option<i32>,
) -> Vec<Option<Normal>> {
    let mut buckets: Vec<Vec<f64>> = vec![Vec::new(); 366];
    for d in days {
        if Some(d.date.year()) == exclude_year {
            continue;
        }
        if let Some(v) = field.get(d) {
            buckets[doy_index(d.date)].push(v);
        }
    }
    // Per-slot sums, so each window is a cheap sum of 2w+1 slots.
    let sums: Vec<(usize, f64, f64)> = buckets
        .iter()
        .map(|b| (b.len(), b.iter().sum(), b.iter().map(|v| v * v).sum()))
        .collect();
    let w = window as isize;
    (0..366isize)
        .map(|i| {
            let (mut n, mut s, mut s2) = (0usize, 0.0, 0.0);
            for k in -w..=w {
                let (bn, bs, bs2) = sums[(i + k).rem_euclid(366) as usize];
                n += bn;
                s += bs;
                s2 += bs2;
            }
            if n < 10 {
                return None;
            }
            let mean = s / n as f64;
            let sd = (s2 / n as f64 - mean * mean).max(0.0).sqrt();
            Some(Normal { mean, sd, n })
        })
        .collect()
}

pub fn years_covered(days: &[DayRecord]) -> f64 {
    match (days.first(), days.last()) {
        (Some(a), Some(b)) => (b.date - a.date).num_days() as f64 / 365.25,
        _ => 0.0,
    }
}

// ---------------------------------------------------------------- anomalies

#[derive(Debug, Clone, PartialEq)]
pub struct Anomaly {
    pub date: NaiveDate,
    /// [`Field::TempMax`] or [`Field::TempMin`].
    pub field: Field,
    pub value: f64,
    pub normal: f64,
    pub z: f64,
}

/// Days in `start..=end` whose max or min sits at least `threshold` standard
/// deviations from the seasonal normal, most unusual first.
pub fn anomalies(
    days: &[DayRecord],
    start: NaiveDate,
    end: NaiveDate,
    threshold: f64,
) -> Vec<Anomaly> {
    let mut out = Vec::new();
    for field in [Field::TempMax, Field::TempMin] {
        let normals = climatology(days, field, 7, None);
        for d in in_range(days, start, end) {
            let (Some(v), Some(nm)) = (field.get(d), normals[doy_index(d.date)]) else {
                continue;
            };
            if nm.sd < 0.1 {
                continue;
            }
            let z = (v - nm.mean) / nm.sd;
            if z.abs() >= threshold {
                out.push(Anomaly {
                    date: d.date,
                    field,
                    value: v,
                    normal: nm.mean,
                    z,
                });
            }
        }
    }
    out.sort_by(|a, b| b.z.abs().total_cmp(&a.z.abs()));
    out
}

// ---------------------------------------------------------------- summaries

#[derive(Debug, Clone, PartialEq)]
pub struct Extreme {
    pub value: f64,
    pub date: NaiveDate,
    pub time: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Max,
    Min,
}

/// The highest or lowest value of `field`, with its date (the first such day
/// on a tie) and, if `with_time`, the time it was reached.
pub fn extreme(days: &[DayRecord], field: Field, kind: Kind, with_time: bool) -> Option<Extreme> {
    let mut best: Option<(f64, &DayRecord)> = None;
    for d in days {
        let Some(v) = field.get(d) else { continue };
        let better = match best {
            None => true,
            Some((b, _)) => match kind {
                Kind::Max => v > b,
                Kind::Min => v < b,
            },
        };
        if better {
            best = Some((v, d));
        }
    }
    best.map(|(value, d)| Extreme {
        value,
        date: d.date,
        time: if with_time {
            field.time(d).map(str::to_string)
        } else {
            None
        },
    })
}

pub fn mean(days: &[DayRecord], field: Field) -> Option<f64> {
    let (n, s) = days
        .iter()
        .filter_map(|d| field.get(d))
        .fold((0usize, 0.0), |(n, s), v| (n + 1, s + v));
    (n > 0).then(|| s / n as f64)
}

pub fn total(days: &[DayRecord], field: Field) -> f64 {
    days.iter().filter_map(|d| field.get(d)).sum()
}

pub fn rain_days(days: &[DayRecord]) -> usize {
    days.iter().filter(|d| is_rain_day(d)).count()
}

/// Expected values for the same calendar days, from other years.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PeriodNormal {
    pub mean_max: Option<f64>,
    pub mean_min: Option<f64>,
    pub rain: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PeriodSummary {
    pub count: usize,
    pub hottest: Option<Extreme>,
    pub coldest: Option<Extreme>,
    pub mean_max: Option<f64>,
    pub mean_min: Option<f64>,
    pub rain: f64,
    pub rain_days: usize,
    pub wettest: Option<Extreme>,
    pub gust: Option<Extreme>,
    pub press_low: Option<Extreme>,
    pub press_high: Option<Extreme>,
    pub normal: PeriodNormal,
}

pub fn summarize(all: &[DayRecord], start: NaiveDate, end: NaiveDate) -> PeriodSummary {
    let days = in_range(all, start, end);
    let year = Some(end.year());
    let c_max = climatology(all, Field::TempMax, 7, year);
    let c_min = climatology(all, Field::TempMin, 7, year);
    let c_rain = climatology(all, Field::Rain, 7, year);
    let (mut n_max, mut s_max, mut n_min, mut s_min, mut n_rain, mut s_rain) =
        (0usize, 0.0, 0usize, 0.0, 0usize, 0.0);
    let mut date = start;
    while date <= end {
        let i = doy_index(date);
        if let Some(c) = c_max[i] {
            n_max += 1;
            s_max += c.mean;
        }
        if let Some(c) = c_min[i] {
            n_min += 1;
            s_min += c.mean;
        }
        if let Some(c) = c_rain[i] {
            n_rain += 1;
            s_rain += c.mean;
        }
        date += Duration::days(1);
    }
    // A normal only counts if it covers most of the period.
    let enough = |n: usize| n as f64 > ((end - start).num_days() + 1) as f64 * 0.8;
    PeriodSummary {
        count: days.len(),
        hottest: extreme(days, Field::TempMax, Kind::Max, true),
        coldest: extreme(days, Field::TempMin, Kind::Min, true),
        mean_max: mean(days, Field::TempMax),
        mean_min: mean(days, Field::TempMin),
        rain: total(days, Field::Rain),
        rain_days: rain_days(days),
        wettest: extreme(days, Field::Rain, Kind::Max, false),
        gust: extreme(days, Field::GustMax, Kind::Max, true),
        press_low: extreme(days, Field::PressMin, Kind::Min, false),
        press_high: extreme(days, Field::PressMax, Kind::Max, false),
        normal: PeriodNormal {
            mean_max: enough(n_max).then(|| s_max / n_max as f64),
            mean_min: enough(n_min).then(|| s_min / n_min as f64),
            rain: enough(n_rain).then_some(s_rain),
        },
    }
}

// ---------------------------------------------------------------- aggregation

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    Day,
    Week,
    Month,
}

/// Ranges longer than about 200 days switch to weekly values, and longer than
/// three years to monthly values.
pub fn bucket_for(span_days: i64) -> Bucket {
    if span_days > 3 * 365 {
        Bucket::Month
    } else if span_days > 200 {
        Bucket::Week
    } else {
        Bucket::Day
    }
}

/// One day, week or month: extremes for temperature, wind and pressure,
/// totals for rain.
#[derive(Debug, Clone, PartialEq)]
pub struct Agg {
    /// First date in the bucket.
    pub key: NaiveDate,
    pub t_max: Option<f64>,
    pub t_min: Option<f64>,
    pub rain: f64,
    pub gust: Option<f64>,
    pub wind_max: Option<f64>,
    pub p_min: Option<f64>,
    pub p_max: Option<f64>,
}

fn bucket_key(date: NaiveDate, b: Bucket) -> NaiveDate {
    match b {
        Bucket::Day => date,
        Bucket::Month => date.with_day(1).unwrap_or(date),
        // Weeks start on Monday.
        Bucket::Week => date - Duration::days(date.weekday().num_days_from_monday() as i64),
    }
}

pub fn aggregate(days: &[DayRecord], b: Bucket) -> Vec<Agg> {
    let mut groups: BTreeMap<NaiveDate, Vec<&DayRecord>> = BTreeMap::new();
    for d in days {
        groups.entry(bucket_key(d.date, b)).or_default().push(d);
    }
    let fold = |arr: &[&DayRecord], f: Field, kind: Kind| {
        arr.iter()
            .filter_map(|d| f.get(d))
            .reduce(|a, v| match kind {
                Kind::Max => a.max(v),
                Kind::Min => a.min(v),
            })
    };
    groups
        .into_iter()
        .map(|(key, arr)| Agg {
            key,
            t_max: fold(&arr, Field::TempMax, Kind::Max),
            t_min: fold(&arr, Field::TempMin, Kind::Min),
            rain: arr.iter().map(|d| rain(d)).sum(),
            gust: fold(&arr, Field::GustMax, Kind::Max),
            wind_max: fold(&arr, Field::WindAvgMax, Kind::Max),
            p_min: fold(&arr, Field::PressMin, Kind::Min),
            p_max: fold(&arr, Field::PressMax, Kind::Max),
        })
        .collect()
}

// ---------------------------------------------------------------- cross-year

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareMetric {
    TempMax,
    TempMin,
    /// Rain, as a running total through the period.
    RainCum,
    GustMax,
}

impl CompareMetric {
    pub const ALL: [CompareMetric; 4] = [
        CompareMetric::TempMax,
        CompareMetric::TempMin,
        CompareMetric::RainCum,
        CompareMetric::GustMax,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CompareMetric::TempMax => "Daily maximum",
            CompareMetric::TempMin => "Daily minimum",
            CompareMetric::RainCum => "Rain, running total",
            CompareMetric::GustMax => "Peak gust",
        }
    }

    fn field(self) -> Field {
        match self {
            CompareMetric::TempMax => Field::TempMax,
            CompareMetric::TempMin => Field::TempMin,
            CompareMetric::RainCum => Field::Rain,
            CompareMetric::GustMax => Field::GustMax,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct YearSeries {
    pub year: i32,
    pub values: Vec<Option<f64>>,
}

/// One series per year, indexed by day of month (`month` given, 1–12) or by
/// day of year (`month` is `None`). Rain is cumulative, and stops where that
/// year's data stops.
pub fn cross_year(
    days: &[DayRecord],
    month: Option<u32>,
    metric: CompareMetric,
) -> Vec<YearSeries> {
    let length = if month.is_some() { 31 } else { 366 };
    let mut by_year: BTreeMap<i32, Vec<Option<f64>>> = BTreeMap::new();
    for d in days {
        if month.is_some_and(|m| d.date.month() != m) {
            continue;
        }
        let idx = match month {
            Some(_) => d.date.day0() as usize,
            None => doy_index(d.date),
        };
        by_year
            .entry(d.date.year())
            .or_insert_with(|| vec![None; length])[idx] = metric.field().get(d);
    }
    let mut out = Vec::new();
    for (year, mut values) in by_year {
        if metric == CompareMetric::RainCum {
            let last_seen = values.iter().rposition(Option::is_some);
            let mut acc = 0.0;
            for (i, v) in values.iter_mut().enumerate() {
                if last_seen.is_none_or(|l| i > l) {
                    *v = None;
                    continue;
                }
                acc += v.unwrap_or(0.0);
                *v = Some((acc * 10.0).round() / 10.0);
            }
        }
        if values.iter().any(Option::is_some) {
            out.push(YearSeries { year, values });
        }
    }
    out
}

/// Mean across series at each index, ignoring `exclude`. Needs at least two
/// years at an index.
pub fn mean_series(series: &[YearSeries], exclude: Option<i32>) -> Vec<Option<f64>> {
    let len = series.first().map_or(0, |s| s.values.len());
    (0..len)
        .map(|i| {
            let vals: Vec<f64> = series
                .iter()
                .filter(|s| Some(s.year) != exclude)
                .filter_map(|s| s.values[i])
                .collect();
            (vals.len() >= 2)
                .then(|| (vals.iter().sum::<f64>() / vals.len() as f64 * 10.0).round() / 10.0)
        })
        .collect()
}

// ---------------------------------------------------------------- records

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordUnit {
    Temp,
    Rain,
    Rate,
    Wind,
    Press,
    Days,
}

/// What a record's date means.
#[derive(Debug, Clone, PartialEq)]
pub enum RecordWhen {
    None,
    Day(NaiveDate, Option<String>),
    Month(NaiveDate),
    Span(NaiveDate, NaiveDate),
}

/// Whether a record reads as hot or cold, for colouring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Warm,
    Cool,
    Plain,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordRow {
    pub label: &'static str,
    pub value: Option<f64>,
    pub unit: RecordUnit,
    pub when: RecordWhen,
    pub tone: Tone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub length: usize,
}

/// The longest stretch of consecutive days matching `pred`. A gap in the data
/// ends a run.
pub fn longest_run(days: &[DayRecord], pred: impl Fn(&DayRecord) -> bool) -> Option<Run> {
    let mut best: Option<Run> = None;
    let mut cur: Option<Run> = None;
    let mut prev: Option<NaiveDate> = None;
    for d in days {
        let contiguous = prev.is_some_and(|p| (d.date - p).num_days() == 1);
        if pred(d) {
            let run = match cur {
                Some(r) if contiguous => Run {
                    end: d.date,
                    length: r.length + 1,
                    ..r
                },
                _ => Run {
                    start: d.date,
                    end: d.date,
                    length: 1,
                },
            };
            if best.is_none_or(|b| run.length > b.length) {
                best = Some(run);
            }
            cur = Some(run);
        } else {
            cur = None;
        }
        prev = Some(d.date);
    }
    best
}

/// Rain per calendar month: `(first of month, rain, days of data)`.
pub fn month_totals(days: &[DayRecord]) -> Vec<(NaiveDate, f64, usize)> {
    let mut m: BTreeMap<NaiveDate, (f64, usize)> = BTreeMap::new();
    for d in days {
        let e = m.entry(bucket_key(d.date, Bucket::Month)).or_default();
        e.0 += rain(d);
        e.1 += 1;
    }
    m.into_iter().map(|(k, (r, n))| (k, r, n)).collect()
}

pub fn records(days: &[DayRecord]) -> Vec<RecordRow> {
    let day_row = |label, e: Option<Extreme>, unit, tone| RecordRow {
        label,
        value: e.as_ref().map(|e| e.value),
        unit,
        when: e.map_or(RecordWhen::None, |e| RecordWhen::Day(e.date, e.time)),
        tone,
    };
    let span_row = |label, r: Option<Run>| RecordRow {
        label,
        value: r.map(|r| r.length as f64),
        unit: RecordUnit::Days,
        when: r.map_or(RecordWhen::None, |r| RecordWhen::Span(r.start, r.end)),
        tone: Tone::Plain,
    };
    let wettest_month = month_totals(days)
        .into_iter()
        .reduce(|b, m| if m.1 > b.1 { m } else { b });
    use RecordUnit as U;
    use Tone::*;
    vec![
        day_row(
            "Hottest day",
            extreme(days, Field::TempMax, Kind::Max, true),
            U::Temp,
            Warm,
        ),
        day_row(
            "Coldest night",
            extreme(days, Field::TempMin, Kind::Min, true),
            U::Temp,
            Cool,
        ),
        day_row(
            "Warmest night",
            extreme(days, Field::TempMin, Kind::Max, false),
            U::Temp,
            Warm,
        ),
        day_row(
            "Coldest day",
            extreme(days, Field::TempMax, Kind::Min, false),
            U::Temp,
            Cool,
        ),
        day_row(
            "Wettest day",
            extreme(days, Field::Rain, Kind::Max, false),
            U::Rain,
            Plain,
        ),
        day_row(
            "Highest rain rate",
            extreme(days, Field::RainRateMax, Kind::Max, false),
            U::Rate,
            Plain,
        ),
        RecordRow {
            label: "Wettest month",
            value: wettest_month.map(|m| m.1),
            unit: U::Rain,
            when: wettest_month.map_or(RecordWhen::None, |m| RecordWhen::Month(m.0)),
            tone: Plain,
        },
        day_row(
            "Strongest gust",
            extreme(days, Field::GustMax, Kind::Max, true),
            U::Wind,
            Plain,
        ),
        day_row(
            "Lowest pressure",
            extreme(days, Field::PressMin, Kind::Min, false),
            U::Press,
            Plain,
        ),
        day_row(
            "Highest pressure",
            extreme(days, Field::PressMax, Kind::Max, false),
            U::Press,
            Plain,
        ),
        span_row("Longest dry spell", longest_run(days, |d| !is_rain_day(d))),
        span_row("Longest wet spell", longest_run(days, is_rain_day)),
    ]
}

// ---------------------------------------------------------------- climate table

#[derive(Debug, Clone, PartialEq)]
pub struct MonthClimate {
    /// 1–12.
    pub month: u32,
    pub mean_max: Option<f64>,
    pub mean_min: Option<f64>,
    pub mean_rain: Option<f64>,
    pub mean_rain_days: Option<f64>,
    pub record_high: Option<Extreme>,
    pub record_low: Option<Extreme>,
    /// Months with enough data to count towards the rain figures.
    pub years: usize,
}

/// Only months with at least 25 days of data count towards rain figures.
pub fn monthly_climate(days: &[DayRecord]) -> Vec<MonthClimate> {
    (1..=12)
        .map(|month| {
            let sel: Vec<DayRecord> = days
                .iter()
                .filter(|d| d.date.month() == month)
                .cloned()
                .collect();
            let mut per_month: BTreeMap<i32, Vec<&DayRecord>> = BTreeMap::new();
            for d in &sel {
                per_month.entry(d.date.year()).or_default().push(d);
            }
            let complete: Vec<&Vec<&DayRecord>> =
                per_month.values().filter(|a| a.len() >= 25).collect();
            let n = complete.len();
            let avg = |f: &dyn Fn(&[&DayRecord]) -> f64| {
                (n > 0).then(|| complete.iter().map(|a| f(a)).sum::<f64>() / n as f64)
            };
            MonthClimate {
                month,
                mean_max: mean(&sel, Field::TempMax),
                mean_min: mean(&sel, Field::TempMin),
                mean_rain: avg(&|a| a.iter().map(|d| rain(d)).sum()),
                mean_rain_days: avg(&|a| a.iter().filter(|d| is_rain_day(d)).count() as f64),
                record_high: extreme(&sel, Field::TempMax, Kind::Max, false),
                record_low: extreme(&sel, Field::TempMin, Kind::Min, false),
                years: n,
            }
        })
        .collect()
}

// ---------------------------------------------------------------- rain today

/// Rain since midnight from a series of the station's "rain today" readings
/// `(time, total)`. The station's own total may reset at another hour (9am
/// is common in Australia), so this adds up each rise and starts again when
/// `day_of` says the day has changed. A drop in the station's total is its
/// reset: the new total is all fresh rain.
pub fn rain_since_midnight<D: PartialEq>(
    pts: &[(i64, f64)],
    day_of: impl Fn(i64) -> D,
) -> Vec<(i64, f64)> {
    let mut out = Vec::with_capacity(pts.len());
    let mut prev: Option<f64> = None;
    let mut day: Option<D> = None;
    let mut acc = 0.0;
    for &(t, v) in pts {
        let d = day_of(t);
        if day.as_ref() != Some(&d) {
            acc = 0.0;
            day = Some(d);
        }
        if let Some(p) = prev {
            acc += if v >= p { v - p } else { v };
        }
        prev = Some(v);
        out.push((t, (acc * 10.0).round() / 10.0));
    }
    out
}

// ---------------------------------------------------------------- rain intensity

/// Daily rain classes, by the lower edge of each in mm: light, moderate,
/// heavy and very heavy.
pub const INTENSITY_MM: [f64; 4] = [RAIN_DAY, 2.0, 10.0, 25.0];
pub const INTENSITY_NAMES: [&str; 4] = ["Light", "Moderate", "Heavy", "Very heavy"];

#[derive(Debug, Clone, PartialEq)]
pub struct IntensityYear {
    pub year: i32,
    pub total: f64,
    /// Rain that fell on days of each class, and how many such days.
    pub rain: [f64; 4],
    pub days: [usize; 4],
}

/// Each year's rain split by how heavy the day was. `mm_per_unit` turns the
/// station's rain unit into mm (25.4 for inches).
pub fn rain_intensity(days: &[DayRecord], mm_per_unit: f64) -> Vec<IntensityYear> {
    let mut out: Vec<IntensityYear> = Vec::new();
    for d in days {
        let Some(r) = d.rain else { continue };
        if out.last().is_none_or(|y| y.year != d.date.year()) {
            out.push(IntensityYear {
                year: d.date.year(),
                total: 0.0,
                rain: [0.0; 4],
                days: [0; 4],
            });
        }
        let y = out.last_mut().unwrap();
        y.total += r;
        let mm = r * mm_per_unit;
        if let Some(k) = INTENSITY_MM.iter().rposition(|edge| mm >= *edge) {
            y.rain[k] += r;
            y.days[k] += 1;
        }
    }
    out
}

/// The days with the most rain in an hour, wettest first.
pub fn wettest_hours(days: &[DayRecord], n: usize) -> Vec<(NaiveDate, f64)> {
    let mut v: Vec<(NaiveDate, f64)> = days
        .iter()
        .filter_map(|d| d.rain_hour_max.filter(|r| *r > 0.0).map(|r| (d.date, r)))
        .collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    v.truncate(n);
    v
}

// ---------------------------------------------------------------- dry spells

/// Consecutive days without a rain day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrySpell {
    pub first: NaiveDate,
    pub last: NaiveDate,
}

impl DrySpell {
    pub fn days(&self) -> i64 {
        (self.last - self.first).num_days() + 1
    }
}

/// Every dry spell on record. A day missing from the history ends a spell,
/// since there's no knowing whether it rained.
pub fn dry_spells(days: &[DayRecord]) -> Vec<DrySpell> {
    let mut out = Vec::new();
    let mut cur: Option<DrySpell> = None;
    let mut prev: Option<NaiveDate> = None;
    for d in days {
        let gap = prev.is_some_and(|p| (d.date - p).num_days() > 1);
        if gap || d.rain.is_none() || is_rain_day(d) {
            out.extend(cur.take());
        }
        if d.rain.is_some() && !is_rain_day(d) {
            match cur.as_mut() {
                Some(c) => c.last = d.date,
                None => {
                    cur = Some(DrySpell {
                        first: d.date,
                        last: d.date,
                    })
                }
            }
        }
        prev = Some(d.date);
    }
    out.extend(cur);
    out
}

/// The dry spell running up to the last day on record, if that day was dry.
pub fn current_dry_spell(days: &[DayRecord]) -> Option<DrySpell> {
    let last = days.last()?.date;
    dry_spells(days).into_iter().rev().find(|s| s.last == last)
}

/// Whether a spell covers any day within `window` days either side of
/// `date`'s day of the year, in the year it falls in.
fn spell_near(s: &DrySpell, date: NaiveDate, window: i64) -> bool {
    let mut d = s.first;
    while d <= s.last {
        let a = doy_index(d) as i64;
        let b = doy_index(date) as i64;
        let diff = (a - b).rem_euclid(366);
        if diff.min(366 - diff) <= window {
            return true;
        }
        d += Duration::days(1);
    }
    false
}

/// The longest dry spell on record, other than `current`, that took in this
/// time of year (within a fortnight of `date`), and how many years had one
/// at least `len` days long then, out of the years on record.
pub fn dry_spell_context(
    spells: &[DrySpell],
    current: Option<DrySpell>,
    date: NaiveDate,
    len: i64,
) -> (Option<DrySpell>, usize) {
    let near: Vec<&DrySpell> = spells
        .iter()
        .filter(|s| Some(**s) != current && spell_near(s, date, 14))
        .collect();
    let longest = near.iter().max_by_key(|s| s.days()).map(|s| **s);
    let mut years: Vec<i32> = near
        .iter()
        .filter(|s| s.days() >= len)
        .map(|s| s.last.year())
        .collect();
    years.sort_unstable();
    years.dedup();
    (longest, years.len())
}

// ---------------------------------------------------------------- pressure falls

#[derive(Debug, Clone, PartialEq)]
pub struct PressureFall {
    /// The day of the low.
    pub date: NaiveDate,
    /// The day before's highest and the day's lowest.
    pub from: f64,
    pub to: f64,
    pub gust: Option<f64>,
    pub rain: Option<f64>,
}

impl PressureFall {
    pub fn fall(&self) -> f64 {
        self.from - self.to
    }
}

/// The biggest falls from one day's highest pressure to the next day's
/// lowest, biggest first, at most one per storm (falls within two days of a
/// bigger one are left out).
pub fn biggest_pressure_falls(days: &[DayRecord], n: usize) -> Vec<PressureFall> {
    let mut all: Vec<PressureFall> = days
        .windows(2)
        .filter(|w| (w[1].date - w[0].date).num_days() == 1)
        .filter_map(|w| {
            Some(PressureFall {
                date: w[1].date,
                from: w[0].press_max?,
                to: w[1].press_min?,
                gust: w[1].gust_max,
                rain: w[1].rain,
            })
        })
        .filter(|f| f.fall() > 0.0)
        .collect();
    all.sort_by(|a, b| b.fall().total_cmp(&a.fall()));
    let mut out: Vec<PressureFall> = Vec::new();
    for f in all {
        if out.iter().all(|o| (o.date - f.date).num_days().abs() > 2) {
            out.push(f);
            if out.len() == n {
                break;
            }
        }
    }
    out
}

// ---------------------------------------------------------------- pressure

/// Hectopascals in one of the station's pressure units.
pub fn hpa_per_unit(unit: &str) -> f64 {
    match unit.trim().to_ascii_lowercase().as_str() {
        "inhg" | "in" => 33.8639,
        "kpa" => 10.0,
        "mmhg" => 1.33322,
        _ => 1.0,
    }
}

/// The three-hour pressure tendency in the words forecasters use, from the
/// change in hPa: steady below 0.1, then slowly, plainly, quickly and very
/// rapidly at 1.6, 3.6 and over 6.
pub fn pressure_tendency(change_hpa: f64) -> String {
    let a = change_hpa.abs();
    if a < 0.1 {
        return "Steady".into();
    }
    let dir = if change_hpa > 0.0 {
        "Rising"
    } else {
        "Falling"
    };
    let how = if a <= 1.5 {
        " slowly"
    } else if a <= 3.5 {
        ""
    } else if a <= 6.0 {
        " quickly"
    } else {
        " very rapidly"
    };
    format!("{dir}{how}")
}

/// The change over the last three hours: the latest reading against the one
/// nearest three hours before it, if that's within 15 minutes of it.
pub fn three_hour_change(pts: &[(i64, f64)]) -> Option<f64> {
    let &(t, now) = pts.last()?;
    let want = t - 3 * 3_600_000;
    let i = pts.partition_point(|p| p.0 < want);
    let near = [i.checked_sub(1), Some(i)]
        .into_iter()
        .flatten()
        .filter_map(|k| pts.get(k))
        .min_by_key(|p| (p.0 - want).abs())?;
    ((near.0 - want).abs() <= 15 * 60_000).then_some(now - near.1)
}

// ---------------------------------------------------------------- wind

pub const COMPASS16: [&str; 16] = [
    "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW",
    "NNW",
];

pub fn compass_index(bearing: f64) -> usize {
    ((bearing.rem_euclid(360.0) / 22.5).round() as usize) % 16
}

pub fn compass(bearing: f64) -> &'static str {
    COMPASS16[compass_index(bearing)]
}

/// Days in each of 16 dominant-direction sectors, and the total counted.
/// Calm days (direction 0 with no wind) are left out.
pub fn wind_rose(days: &[DayRecord]) -> ([usize; 16], usize) {
    let mut counts = [0usize; 16];
    let mut n = 0;
    for d in days {
        let Some(b) = Field::WindDirDominant.get(d) else {
            continue;
        };
        if b == 0.0 && Field::WindAvgMax.get(d).unwrap_or(1.0) == 0.0 {
            continue;
        }
        counts[compass_index(b)] += 1;
        n += 1;
    }
    (counts, n)
}

/// Wind roses for each season of the year, all years together: December
/// to February, March to May, June to August and September to November.
pub fn seasonal_wind_roses(days: &[DayRecord]) -> [([usize; 16], usize); 4] {
    let mut out = [([0usize; 16], 0usize); 4];
    for (k, rose) in out.iter_mut().enumerate() {
        let months = [[12, 1, 2], [3, 4, 5], [6, 7, 8], [9, 10, 11]][k];
        let in_season: Vec<DayRecord> = days
            .iter()
            .filter(|d| months.contains(&d.date.month()))
            .cloned()
            .collect();
        *rose = wind_rose(&in_season);
    }
    out
}

/// A day's gust factor: its peak gust over its highest average speed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GustDay {
    pub date: NaiveDate,
    pub factor: f64,
    pub gust: f64,
    pub average: f64,
}

/// Gust factors for days whose highest average reached `min_average`
/// (station units); in lighter air the ratio is mostly noise.
pub fn gust_days(days: &[DayRecord], min_average: f64) -> Vec<GustDay> {
    days.iter()
        .filter_map(|d| {
            let (gust, average) = (d.gust_max?, d.wind_avg_max?);
            (average >= min_average && average > 0.0 && gust >= average).then_some(GustDay {
                date: d.date,
                factor: gust / average,
                gust,
                average,
            })
        })
        .collect()
}

/// The median gust factor for each calendar month (January first), and for
/// all days together.
pub fn gust_factor_by_month(g: &[GustDay]) -> ([Option<f64>; 12], Option<f64>) {
    fn median(mut v: Vec<f64>) -> Option<f64> {
        if v.is_empty() {
            return None;
        }
        v.sort_by(f64::total_cmp);
        let n = v.len();
        Some(if n % 2 == 1 {
            v[n / 2]
        } else {
            (v[n / 2 - 1] + v[n / 2]) / 2.0
        })
    }
    let mut months: [Option<f64>; 12] = [None; 12];
    for (m, slot) in months.iter_mut().enumerate() {
        *slot = median(
            g.iter()
                .filter(|d| d.date.month0() as usize == m)
                .map(|d| d.factor)
                .collect(),
        );
    }
    (months, median(g.iter().map(|d| d.factor).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn day(dt: NaiveDate, max: f64, min: f64, rain: f64) -> DayRecord {
        DayRecord {
            date: dt,
            temp_max: Some(max),
            temp_min: Some(min),
            rain: Some(rain),
            ..Default::default()
        }
    }

    /// Five years of days with a seasonal max (warm in January) and steady rain.
    fn years() -> Vec<DayRecord> {
        let mut out = Vec::new();
        let mut d = date(2019, 1, 1);
        while d <= date(2023, 12, 31) {
            let season = ((d.ordinal() as f64 - 15.0) / 365.25 * std::f64::consts::TAU).cos();
            let wobble = (d.num_days_from_ce() as i64 * 7919 % 11) as f64 / 10.0 - 0.5;
            out.push(day(
                d,
                20.0 + 8.0 * season + wobble,
                10.0 + 5.0 * season,
                1.0,
            ));
            d += Duration::days(1);
        }
        out
    }

    #[test]
    fn doy_uses_a_leap_year() {
        assert_eq!(doy_index(date(2023, 1, 1)), 0);
        assert_eq!(doy_index(date(2024, 2, 29)), 59);
        assert_eq!(doy_index(date(2023, 3, 1)), 60);
        assert_eq!(doy_index(date(2023, 12, 31)), 365);
    }

    #[test]
    fn range_slices_sorted_days() {
        let days = years();
        let r = in_range(&days, date(2020, 2, 27), date(2020, 3, 2));
        assert_eq!(r.len(), 5);
        assert_eq!(r[0].date, date(2020, 2, 27));
        assert!(in_range(&days, date(2030, 1, 1), date(2030, 2, 1)).is_empty());
        assert!(in_range(&days, date(2021, 1, 2), date(2021, 1, 1)).is_empty());
    }

    #[test]
    fn climatology_is_seasonal() {
        let days = years();
        let c = climatology(&days, Field::TempMax, 7, None);
        let jan = c[doy_index(date(2000, 1, 15))].unwrap();
        let jul = c[doy_index(date(2000, 7, 15))].unwrap();
        assert!(jan.mean > 27.0 && jul.mean < 13.0, "{jan:?} {jul:?}");
        assert!(jan.n >= 70);
        // Too little data gives no normal.
        assert!(climatology(&days[..5], Field::TempMax, 7, None)[2].is_none());
    }

    #[test]
    fn excluded_year_is_left_out() {
        let mut days = years();
        for d in days.iter_mut().filter(|d| d.date.year() == 2023) {
            d.temp_max = Some(100.0);
        }
        let c = climatology(&days, Field::TempMax, 7, Some(2023));
        assert!(c[100].unwrap().mean < 40.0);
    }

    #[test]
    fn anomalies_find_the_odd_day() {
        let mut days = years();
        let i = days
            .iter()
            .position(|d| d.date == date(2023, 7, 10))
            .unwrap();
        days[i].temp_max = Some(25.0); // a hot day in winter
        let a = anomalies(&days, date(2023, 7, 1), date(2023, 7, 31), 2.0);
        assert_eq!(a[0].date, date(2023, 7, 10));
        assert_eq!(a[0].field, Field::TempMax);
        assert!(a[0].z > 2.0);
    }

    #[test]
    fn summary_against_normal() {
        let days = years();
        let s = summarize(&days, date(2023, 1, 1), date(2023, 1, 31));
        assert_eq!(s.count, 31);
        assert_eq!(s.rain, 31.0);
        assert_eq!(s.rain_days, 31);
        let n = s.normal.rain.unwrap();
        assert!((n - 31.0).abs() < 0.01, "{n}");
        assert!((s.mean_max.unwrap() - s.normal.mean_max.unwrap()).abs() < 0.5);
    }

    #[test]
    fn aggregates_by_week_and_month() {
        let days = years();
        let m = aggregate(
            in_range(&days, date(2022, 1, 1), date(2022, 12, 31)),
            Bucket::Month,
        );
        assert_eq!(m.len(), 12);
        assert_eq!(m[1].key, date(2022, 2, 1));
        assert_eq!(m[1].rain, 28.0);
        let w = aggregate(
            in_range(&days, date(2022, 1, 1), date(2022, 1, 31)),
            Bucket::Week,
        );
        // 1 Jan 2022 was a Saturday, so the first week starts on 27 Dec 2021.
        assert_eq!(w[0].key, date(2021, 12, 27));
        assert_eq!(bucket_for(30), Bucket::Day);
        assert_eq!(bucket_for(365), Bucket::Week);
        assert_eq!(bucket_for(2000), Bucket::Month);
    }

    #[test]
    fn cross_year_rain_is_cumulative_and_stops() {
        let mut days = years();
        days.retain(|d| d.date <= date(2023, 3, 10));
        let s = cross_year(&days, Some(3), CompareMetric::RainCum);
        assert_eq!(s.len(), 5);
        let last = s.last().unwrap();
        assert_eq!(last.year, 2023);
        assert_eq!(last.values[9], Some(10.0));
        assert_eq!(last.values[10], None);
        assert_eq!(s[0].values[30], Some(31.0));
        let avg = mean_series(&s, Some(2023));
        assert_eq!(avg[30], Some(31.0));
    }

    #[test]
    fn records_and_runs() {
        let mut days = years();
        for d in days
            .iter_mut()
            .filter(|d| d.date.year() == 2021 && d.date.month() == 4)
        {
            d.rain = Some(0.0);
        }
        days[400].temp_max = Some(45.0);
        days[400].temp_max_time = Some("14:27".into());
        let r = records(&days);
        assert_eq!(r[0].value, Some(45.0));
        assert_eq!(
            r[0].when,
            RecordWhen::Day(days[400].date, Some("14:27".into()))
        );
        let dry = r.iter().find(|r| r.label == "Longest dry spell").unwrap();
        assert_eq!(dry.value, Some(30.0));
        assert_eq!(
            dry.when,
            RecordWhen::Span(date(2021, 4, 1), date(2021, 4, 30))
        );
    }

    #[test]
    fn a_gap_breaks_a_run() {
        let days = vec![
            day(date(2024, 1, 1), 20.0, 10.0, 0.0),
            day(date(2024, 1, 2), 20.0, 10.0, 0.0),
            day(date(2024, 1, 5), 20.0, 10.0, 0.0),
        ];
        assert_eq!(longest_run(&days, |d| !is_rain_day(d)).unwrap().length, 2);
    }

    fn wet(y: i32, m: u32, d: u32, rain: f64) -> DayRecord {
        DayRecord {
            date: NaiveDate::from_ymd_opt(y, m, d).unwrap(),
            rain: Some(rain),
            ..Default::default()
        }
    }

    #[test]
    fn splits_rain_by_intensity() {
        let days = vec![
            wet(2025, 1, 1, 0.1),
            wet(2025, 1, 2, 1.0),
            wet(2025, 1, 3, 5.0),
            wet(2025, 1, 4, 30.0),
            wet(2026, 1, 1, 12.0),
        ];
        let y = rain_intensity(&days, 1.0);
        assert_eq!(y.len(), 2);
        assert_eq!(y[0].rain, [1.0, 5.0, 0.0, 30.0]);
        assert_eq!(y[0].days, [1, 1, 0, 1]);
        assert!((y[0].total - 36.1).abs() < 1e-9);
        assert_eq!(y[1].days[2], 1);
        // In inches: 0.5 in is 12.7 mm, a heavy day.
        assert_eq!(rain_intensity(&[wet(2026, 1, 1, 0.5)], 25.4)[0].days[2], 1);
    }

    #[test]
    fn finds_dry_spells() {
        let mut days = Vec::new();
        for k in 1..=20 {
            days.push(wet(2025, 10, k, if k == 5 || k == 15 { 3.0 } else { 0.0 }));
        }
        days.retain(|d| d.date.day() != 10);
        let s = dry_spells(&days);
        // 1–4, 6–9 (the 10th is missing), 11–14, 16–20.
        assert_eq!(s.iter().map(|s| s.days()).collect::<Vec<_>>(), [4, 4, 4, 5]);
        let cur = current_dry_spell(&days).unwrap();
        assert_eq!(cur.days(), 5);
        let (longest, years) = dry_spell_context(
            &s,
            Some(cur),
            NaiveDate::from_ymd_opt(2026, 10, 18).unwrap(),
            4,
        );
        assert_eq!(longest.map(|l| l.days()), Some(4));
        assert_eq!(years, 1);
        let mut rained = days.clone();
        rained.push(wet(2025, 10, 21, 1.0));
        assert_eq!(current_dry_spell(&rained), None);
    }

    #[test]
    fn finds_the_biggest_pressure_falls() {
        let p = |y, m, d, lo: f64, hi: f64| DayRecord {
            date: NaiveDate::from_ymd_opt(y, m, d).unwrap(),
            press_min: Some(lo),
            press_max: Some(hi),
            ..Default::default()
        };
        let days = vec![
            p(2025, 6, 1, 1015.0, 1022.0),
            p(2025, 6, 2, 998.0, 1016.0),
            p(2025, 6, 3, 990.0, 1000.0),
            p(2025, 6, 4, 995.0, 1005.0),
            p(2025, 7, 1, 1010.0, 1020.0),
            p(2025, 7, 2, 1000.0, 1012.0),
        ];
        let f = biggest_pressure_falls(&days, 5);
        // 2 Jun (24 hPa), 3 Jun (26 hPa) and 4 Jun (5 hPa) are one storm:
        // keep 3 Jun.
        assert_eq!(f.len(), 2);
        assert_eq!((f[0].date.day(), f[0].fall()), (3, 26.0));
        assert_eq!((f[1].date.month(), f[1].fall()), (7, 20.0));
    }

    #[test]
    fn pressure_tendency_in_words() {
        assert_eq!(pressure_tendency(0.05), "Steady");
        assert_eq!(pressure_tendency(1.2), "Rising slowly");
        assert_eq!(pressure_tendency(-2.0), "Falling");
        assert_eq!(pressure_tendency(-4.1), "Falling quickly");
        assert_eq!(pressure_tendency(6.5), "Rising very rapidly");
        assert!((hpa_per_unit("inHg") - 33.8639).abs() < 1e-9);
        assert_eq!(hpa_per_unit("mb"), 1.0);
        let min = 60_000;
        let pts: Vec<(i64, f64)> = (0..=200)
            .map(|k| (k * min, 1015.0 - k as f64 * 0.02))
            .collect();
        let c = three_hour_change(&pts).unwrap();
        assert!((c + 3.6).abs() < 1e-9, "{c}");
        // Not three hours of readings yet.
        assert_eq!(three_hour_change(&pts[..120]), None);
    }

    #[test]
    fn climate_table_counts_complete_months() {
        let mut days = years();
        days.retain(|d| !(d.date.year() == 2023 && d.date.month() == 6 && d.date.day() > 10));
        let c = monthly_climate(&days);
        assert_eq!(c[5].years, 4);
        assert_eq!(c[5].mean_rain, Some(30.0));
        assert_eq!(c[0].years, 5);
    }

    #[test]
    fn rain_since_midnight_resets_at_midnight_not_the_stations_9am() {
        // Hours since an arbitrary midnight; a "day" is 24 of them.
        let h = |x: i64| x * 3_600_000;
        let pts = [
            (h(20), 1.0), // evening: station total 1.0 since 9am
            (h(22), 3.0), // +2.0
            (h(25), 4.0), // past midnight: +1.0, counted in the new day
            (h(32), 5.0), // +1.0 before the station's 9am reset
            (h(34), 0.4), // after the 9am reset: 0.4 fresh
        ];
        let got = rain_since_midnight(&pts, |t| t / h(24));
        let vals: Vec<f64> = got.iter().map(|p| p.1).collect();
        assert_eq!(vals, vec![0.0, 2.0, 1.0, 2.0, 2.4]);
    }

    #[test]
    fn compass_and_rose() {
        assert_eq!(compass(0.0), "N");
        assert_eq!(compass(359.0), "N");
        assert_eq!(compass(202.5), "SSW");
        assert_eq!(compass(-90.0), "W");
        let mut d = day(date(2024, 1, 1), 20.0, 10.0, 0.0);
        d.wind_dir_dominant = Some(225.0);
        let mut calm = d.clone();
        calm.wind_dir_dominant = Some(0.0);
        calm.wind_avg_max = Some(0.0);
        let (counts, n) = wind_rose(&[d, calm]);
        assert_eq!(n, 1);
        assert_eq!(counts[10], 1);
    }

    #[test]
    fn seasonal_roses_and_gust_factor() {
        let day = |m: u32, dir: f64, gust: f64, avg: f64| DayRecord {
            date: date(2025, m, 10),
            wind_dir_dominant: Some(dir),
            gust_max: Some(gust),
            wind_avg_max: Some(avg),
            ..Default::default()
        };
        let days = [
            day(1, 180.0, 40.0, 20.0),
            day(7, 270.0, 30.0, 20.0),
            day(7, 270.0, 60.0, 30.0),
            day(10, 90.0, 9.0, 4.0),
        ];
        let roses = seasonal_wind_roses(&days);
        assert_eq!(roses[0].1, 1, "January is in December to February");
        assert_eq!(roses[2].0[12], 2, "two July days from the west");
        let g = gust_days(&days, 10.0);
        assert_eq!(g.len(), 3, "the 4 km/h day is too light to count");
        assert_eq!(g[0].factor, 2.0);
        let (months, all) = gust_factor_by_month(&g);
        assert_eq!(months[6], Some(1.75));
        assert_eq!(months[0], Some(2.0));
        assert_eq!(all, Some(2.0));
    }
}
