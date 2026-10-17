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
}
