//! The seasons: water balance, frost, growing degree days, sunshine and day
//! length.
//!
//! Seasons follow the hemisphere, which comes from the latitude when it's
//! known and otherwise from which half of the year is warmer. Every function
//! takes the days sorted oldest first.

use std::f64::consts::PI;

use chrono::{Datelike, Duration, Months, NaiveDate};

use crate::analytics::{doy_index, in_range, YearSeries};
use crate::dayfile::DayRecord;
use crate::live::Units;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hemisphere {
    North,
    South,
}

/// From the latitude if known, else from the history: a station warmer in
/// January than in July is in the south.
pub fn hemisphere(days: &[DayRecord], latitude: Option<f64>) -> Hemisphere {
    if let Some(lat) = latitude {
        return if lat < 0.0 {
            Hemisphere::South
        } else {
            Hemisphere::North
        };
    }
    let mean_max = |month: u32| {
        let v: Vec<f64> = days
            .iter()
            .filter(|d| d.date.month() == month)
            .filter_map(|d| d.temp_max)
            .collect();
        (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
    };
    match (mean_max(1), mean_max(7)) {
        (Some(jan), Some(jul)) if jan > jul => Hemisphere::South,
        _ => Hemisphere::North,
    }
}

pub fn is_fahrenheit(u: &Units) -> bool {
    u.temp.contains('F')
}

pub fn is_inches(u: &Units) -> bool {
    u.rain.trim().eq_ignore_ascii_case("in")
}

fn to_celsius(t: f64, u: &Units) -> f64 {
    if is_fahrenheit(u) {
        (t - 32.0) * 5.0 / 9.0
    } else {
        t
    }
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

// ---------------------------------------------------------------- sun

/// The sun's declination in radians (Spencer's series, good to a few
/// hundredths of a degree).
pub fn declination(date: NaiveDate) -> f64 {
    let g = 2.0 * PI / 365.0 * date.ordinal0() as f64;
    0.006918 - 0.399912 * g.cos() + 0.070257 * g.sin() - 0.006758 * (2.0 * g).cos()
        + 0.000907 * (2.0 * g).sin()
        - 0.002697 * (3.0 * g).cos()
        + 0.00148 * (3.0 * g).sin()
}

/// Hours from sunrise to sunset at a latitude (degrees, south negative),
/// counting from the sun's top edge with the usual allowance for the air
/// bending its light, as sunrise tables do.
pub fn day_length(latitude: f64, date: NaiveDate) -> f64 {
    let phi = latitude.to_radians();
    let d = declination(date);
    let h0 = (-0.833f64).to_radians();
    let c = (h0.sin() - phi.sin() * d.sin()) / (phi.cos() * d.cos());
    if c <= -1.0 {
        24.0
    } else if c >= 1.0 {
        0.0
    } else {
        2.0 * c.acos().to_degrees() / 15.0
    }
}

/// The latitude that gives this day length on this date. Near the equinoxes
/// every latitude has about twelve hours, so there's no answer then.
pub fn estimate_latitude(date: NaiveDate, hours: f64) -> Option<f64> {
    if declination(date).to_degrees().abs() < 4.0 || !(0.5..23.5).contains(&hours) {
        return None;
    }
    let f = |lat: f64| day_length(lat, date) - hours;
    let (mut a, mut b) = (-65.0, 65.0);
    if f(a).signum() == f(b).signum() {
        return None;
    }
    for _ in 0..60 {
        let m = (a + b) / 2.0;
        if f(m).signum() == f(a).signum() {
            a = m;
        } else {
            b = m;
        }
    }
    Some((a + b) / 2.0)
}

/// Day length for each day of the year, indexed like [`doy_index`].
pub fn day_length_year(latitude: f64) -> Vec<f64> {
    (0..366)
        .map(|i| {
            let d = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap() + Duration::days(i);
            day_length(latitude, d)
        })
        .collect()
}

/// Sunlight reaching the top of the atmosphere over a day, MJ/m² (FAO-56).
fn extraterrestrial(latitude: f64, date: NaiveDate) -> f64 {
    let phi = latitude.to_radians();
    let d = declination(date);
    let j = date.ordinal() as f64;
    let dr = 1.0 + 0.033 * (2.0 * PI * j / 365.0).cos();
    let ws = (-phi.tan() * d.tan()).clamp(-1.0, 1.0).acos();
    24.0 * 60.0 / PI * 0.082 * dr * (ws * phi.sin() * d.sin() + phi.cos() * d.cos() * ws.sin())
}

/// Evaporation from a reference crop in mm a day, from the day's highest and
/// lowest temperatures in °C (Hargreaves). For stations that don't record
/// evapotranspiration themselves.
pub fn hargreaves(max_c: f64, min_c: f64, latitude: f64, date: NaiveDate) -> f64 {
    let ra = extraterrestrial(latitude, date) * 0.408;
    let mean = (max_c + min_c) / 2.0;
    (0.0023 * ra * (mean + 17.8) * (max_c - min_c).max(0.0).sqrt()).max(0.0)
}

// ---------------------------------------------------------------- periods

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Span {
    Month,
    Season,
    Year,
}

impl Span {
    pub fn months(self) -> u32 {
        match self {
            Span::Month => 1,
            Span::Season => 3,
            Span::Year => 12,
        }
    }

    /// The first day of the month, season or calendar year holding `date`.
    /// Seasons are the weather service ones: three whole months from
    /// December, March, June and September.
    pub fn start(self, date: NaiveDate) -> NaiveDate {
        let (y, m) = (date.year(), date.month());
        let (y, m) = match self {
            Span::Month => (y, m),
            Span::Year => (y, 1),
            Span::Season if m == 12 => (y, 12),
            Span::Season if m < 3 => (y - 1, 12),
            Span::Season => (y, m - m % 3),
        };
        NaiveDate::from_ymd_opt(y, m, 1).unwrap()
    }
}

/// The season starting in `month` (12, 3, 6 or 9), by name.
pub fn season_name(month: u32, h: Hemisphere) -> &'static str {
    let north = match month {
        12 | 1 | 2 => "Winter",
        3..=5 => "Spring",
        6..=8 => "Summer",
        _ => "Autumn",
    };
    match (h, north) {
        (Hemisphere::North, n) => n,
        (Hemisphere::South, "Winter") => "Summer",
        (Hemisphere::South, "Spring") => "Autumn",
        (Hemisphere::South, "Summer") => "Winter",
        (Hemisphere::South, _) => "Spring",
    }
}

fn add_months(d: NaiveDate, n: u32) -> NaiveDate {
    d.checked_add_months(Months::new(n)).unwrap_or(d)
}

/// For each year, a running total of `value` over the period of `months`
/// starting on the same day of the year as `start`; `year` is the year each
/// period starts in, and the series is indexed by days from its start.
/// Missing days add nothing. Periods with less than nine tenths of their days
/// on record are left out, except the one starting at `start` (usually the
/// current one), which runs to the last day on record.
pub fn running_totals(
    days: &[DayRecord],
    start: NaiveDate,
    months: u32,
    value: impl Fn(&DayRecord) -> Option<f64>,
) -> Vec<YearSeries> {
    let (Some(first), Some(last)) = (days.first(), days.last()) else {
        return Vec::new();
    };
    let len = (0..4)
        .map(|k| {
            let s = NaiveDate::from_ymd_opt(start.year() + k, start.month(), 1).unwrap();
            (add_months(s, months) - s).num_days() as usize
        })
        .max()
        .unwrap_or(31);
    let mut out = Vec::new();
    for year in first.date.year() - 1..=last.date.year() {
        let Some(s) = NaiveDate::from_ymd_opt(year, start.month(), 1) else {
            continue;
        };
        let end = add_months(s, months);
        let sel = in_range(days, s, end - Duration::days(1));
        let Some(last_day) = sel.last().map(|d| d.date) else {
            continue;
        };
        let period = (end - s).num_days() as usize;
        let current = s == start;
        let counted = sel.iter().filter(|d| value(d).is_some()).count();
        if !current && (counted as f64) < period as f64 * 0.9 {
            continue;
        }
        let mut values = vec![None; len];
        let mut acc = 0.0;
        let mut it = sel.iter().peekable();
        for (i, slot) in values
            .iter_mut()
            .enumerate()
            .take((last_day - s).num_days() as usize + 1)
        {
            let date = s + Duration::days(i as i64);
            while let Some(d) = it.next_if(|d| d.date <= date) {
                acc += value(d).unwrap_or(0.0);
            }
            *slot = Some(round1(acc));
        }
        out.push(YearSeries { year, values });
    }
    out
}

/// The lowest and highest of the other series at each index, where at least
/// two have a value.
pub fn spread(series: &[YearSeries], exclude: Option<i32>) -> (Vec<Option<f64>>, Vec<Option<f64>>) {
    let len = series.first().map_or(0, |s| s.values.len());
    (0..len)
        .map(|i| {
            let vals: Vec<f64> = series
                .iter()
                .filter(|s| Some(s.year) != exclude)
                .filter_map(|s| s.values.get(i).copied().flatten())
                .collect();
            if vals.len() < 2 {
                return (None, None);
            }
            let lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            (Some(lo), Some(hi))
        })
        .unzip()
}

// ---------------------------------------------------------------- water

/// Whether the station records evapotranspiration: most recent days have a
/// value above zero. (Stations without the sensors write 0 or nothing.)
pub fn station_records_et(days: &[DayRecord]) -> bool {
    let recent = &days[days.len().saturating_sub(365)..];
    let with = recent
        .iter()
        .filter(|d| d.et.is_some_and(|v| v > 0.0))
        .count();
    !recent.is_empty() && with * 2 >= recent.len()
}

/// The day's evaporation in the station's rain units: the station's own
/// figure, or with `estimate_at` (a latitude) one worked out from the
/// temperatures.
pub fn day_et(d: &DayRecord, estimate_at: Option<f64>, u: &Units) -> Option<f64> {
    match estimate_at {
        None => d.et,
        Some(lat) => {
            let mm = hargreaves(
                to_celsius(d.temp_max?, u),
                to_celsius(d.temp_min?, u),
                lat,
                d.date,
            );
            Some(if is_inches(u) { mm / 25.4 } else { mm })
        }
    }
}

// ---------------------------------------------------------------- growing

/// Growing degree days for one day: how far the day's mean temperature is
/// above `base`, or nothing below it.
pub fn degree_days(d: &DayRecord, base: f64) -> Option<f64> {
    Some(((d.temp_max? + d.temp_min?) / 2.0 - base).max(0.0))
}

/// The growing year holding `date`: from 1 July in the south (through the
/// summer) and 1 January in the north.
pub fn growing_start(h: Hemisphere, date: NaiveDate) -> NaiveDate {
    let m = if h == Hemisphere::South { 7 } else { 1 };
    let y = if date.month() >= m {
        date.year()
    } else {
        date.year() - 1
    };
    NaiveDate::from_ymd_opt(y, m, 1).unwrap()
}

// ---------------------------------------------------------------- frost

/// The frost year holding `date`, chosen so the cold months sit in the
/// middle of it: from 1 January in the south and 1 July in the north.
pub fn frost_start(h: Hemisphere, date: NaiveDate) -> NaiveDate {
    let m = if h == Hemisphere::South { 1 } else { 7 };
    let y = if date.month() >= m {
        date.year()
    } else {
        date.year() - 1
    };
    NaiveDate::from_ymd_opt(y, m, 1).unwrap()
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrostYear {
    pub start: NaiveDate,
    /// Frost nights, as days from `start`.
    pub nights: Vec<usize>,
    /// Days with a low on record.
    pub days: usize,
    pub coldest: Option<(NaiveDate, f64)>,
    /// Nine tenths of its days are on record.
    pub complete: bool,
}

impl FrostYear {
    pub fn first(&self) -> Option<NaiveDate> {
        self.nights
            .first()
            .map(|&i| self.start + Duration::days(i as i64))
    }

    pub fn last(&self) -> Option<NaiveDate> {
        self.nights
            .last()
            .map(|&i| self.start + Duration::days(i as i64))
    }
}

/// Each frost year on record (starting on the month and day of `start`) with
/// its nights at or below `threshold`.
pub fn frost_years(days: &[DayRecord], start: NaiveDate, threshold: f64) -> Vec<FrostYear> {
    let (Some(first), Some(last)) = (days.first(), days.last()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for year in first.date.year() - 1..=last.date.year() {
        let Some(s) = NaiveDate::from_ymd_opt(year, start.month(), 1) else {
            continue;
        };
        let end = add_months(s, 12);
        let sel = in_range(days, s, end - Duration::days(1));
        let lows: Vec<(NaiveDate, f64)> = sel
            .iter()
            .filter_map(|d| d.temp_min.map(|t| (d.date, t)))
            .collect();
        if lows.is_empty() {
            continue;
        }
        out.push(FrostYear {
            start: s,
            nights: lows
                .iter()
                .filter(|(_, t)| *t <= threshold)
                .map(|(d, _)| (*d - s).num_days() as usize)
                .collect(),
            days: lows.len(),
            coldest: lows.iter().copied().min_by(|a, b| a.1.total_cmp(&b.1)),
            complete: lows.len() as f64 >= (end - s).num_days() as f64 * 0.9,
        });
    }
    out
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrostNormals {
    /// Complete years counted.
    pub years: usize,
    /// Of those, years with no frost at all.
    pub frost_free: usize,
    /// Average days from the year's start to the first and last frost, over
    /// the years that had one.
    pub first: Option<f64>,
    pub last: Option<f64>,
    /// Average frost nights a year.
    pub nights: Option<f64>,
    /// The earliest first frost and the latest last frost, as (date, days
    /// from that year's start).
    pub earliest_first: Option<NaiveDate>,
    pub latest_last: Option<NaiveDate>,
}

/// Averages over the complete frost years, leaving out the one starting at
/// `exclude` (the current one).
pub fn frost_normals(years: &[FrostYear], exclude: Option<NaiveDate>) -> FrostNormals {
    let done: Vec<&FrostYear> = years
        .iter()
        .filter(|y| y.complete && Some(y.start) != exclude)
        .collect();
    let mean = |v: Vec<f64>| (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64);
    let firsts: Vec<&FrostYear> = done
        .iter()
        .copied()
        .filter(|y| !y.nights.is_empty())
        .collect();
    FrostNormals {
        years: done.len(),
        frost_free: done.len() - firsts.len(),
        first: mean(firsts.iter().map(|y| y.nights[0] as f64).collect()),
        last: mean(
            firsts
                .iter()
                .map(|y| *y.nights.last().unwrap() as f64)
                .collect(),
        ),
        nights: mean(done.iter().map(|y| y.nights.len() as f64).collect()),
        earliest_first: firsts
            .iter()
            .min_by_key(|y| y.nights[0])
            .and_then(|y| y.first()),
        latest_last: firsts
            .iter()
            .max_by_key(|y| *y.nights.last().unwrap())
            .and_then(|y| y.last()),
    }
}

// ---------------------------------------------------------------- monthly totals

#[derive(Debug, Clone, PartialEq)]
pub struct MonthTotal {
    /// The first of the month.
    pub month: NaiveDate,
    pub total: f64,
    /// Days with a value.
    pub days: usize,
}

/// A month counts towards normals and records with this many days of data.
pub const FULL_MONTH: usize = 25;

/// The sum of `value` for each calendar month on record.
pub fn monthly_totals(
    days: &[DayRecord],
    value: impl Fn(&DayRecord) -> Option<f64>,
) -> Vec<MonthTotal> {
    let mut out: Vec<MonthTotal> = Vec::new();
    for d in days {
        let Some(v) = value(d) else { continue };
        let m = d.date.with_day(1).unwrap();
        match out.last_mut() {
            Some(t) if t.month == m => {
                t.total += v;
                t.days += 1;
            }
            _ => out.push(MonthTotal {
                month: m,
                total: v,
                days: 1,
            }),
        }
    }
    for t in &mut out {
        t.total = round1(t.total);
    }
    out
}

/// The average total for each calendar month (January first), over full
/// months, leaving out `exclude_year`.
pub fn month_normals(totals: &[MonthTotal], exclude_year: Option<i32>) -> [Option<f64>; 12] {
    let mut sums = [(0.0, 0usize); 12];
    for t in totals
        .iter()
        .filter(|t| t.days >= FULL_MONTH && Some(t.month.year()) != exclude_year)
    {
        let s = &mut sums[t.month.month0() as usize];
        s.0 += t.total;
        s.1 += 1;
    }
    sums.map(|(s, n)| (n > 0).then(|| s / n as f64))
}

/// The average of `value` for each day of the year (indexed like
/// [`doy_index`]), over a window of ±`window` days across every year.
pub fn doy_mean(
    days: &[DayRecord],
    window: usize,
    value: impl Fn(&DayRecord) -> Option<f64>,
) -> Vec<Option<f64>> {
    let mut slots = vec![(0.0, 0usize); 366];
    for d in days {
        if let Some(v) = value(d) {
            let s = &mut slots[doy_index(d.date)];
            s.0 += v;
            s.1 += 1;
        }
    }
    let w = window as isize;
    (0..366isize)
        .map(|i| {
            let (mut s, mut n) = (0.0, 0usize);
            for k in -w..=w {
                let (bs, bn) = slots[(i + k).rem_euclid(366) as usize];
                s += bs;
                n += bn;
            }
            (n >= 5).then(|| s / n as f64)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn day(d: NaiveDate, max: f64, min: f64, rain: f64) -> DayRecord {
        DayRecord {
            date: d,
            temp_max: Some(max),
            temp_min: Some(min),
            rain: Some(rain),
            ..Default::default()
        }
    }

    #[test]
    fn day_length_matches_sunrise_tables() {
        // Sydney (33.87° S) on 21 June: about 9 h 53 min; 21 December about 14 h 25 min.
        let winter = day_length(-33.87, date(2026, 6, 21));
        let summer = day_length(-33.87, date(2026, 12, 21));
        assert!((winter - 9.88).abs() < 0.05, "{winter}");
        assert!((summer - 14.42).abs() < 0.05, "{summer}");
        // The poles in their summer and winter.
        assert_eq!(day_length(80.0, date(2026, 6, 21)), 24.0);
        assert_eq!(day_length(80.0, date(2026, 12, 21)), 0.0);
    }

    #[test]
    fn latitude_from_sunrise_and_sunset() {
        // The owner's station on 18 October 2026: sunrise 6:36, sunset 19:45.
        let lat = estimate_latitude(date(2026, 10, 18), 13.0 + 9.0 / 60.0).unwrap();
        assert!(lat < -30.0 && lat > -45.0, "{lat}");
        let back = day_length(lat, date(2026, 10, 18));
        assert!((back - 13.15).abs() < 0.01);
        // Too close to the equinox to tell.
        assert_eq!(estimate_latitude(date(2026, 3, 21), 12.1), None);
    }

    #[test]
    fn hargreaves_gives_sensible_evaporation() {
        // A warm summer day at 34° S: around 5 to 7 mm.
        let et = hargreaves(30.0, 16.0, -34.0, date(2026, 1, 15));
        assert!((5.0..8.0).contains(&et), "{et}");
        // A cold winter day: around 1 mm.
        let et = hargreaves(13.0, 4.0, -34.0, date(2026, 7, 15));
        assert!((0.5..2.0).contains(&et), "{et}");
    }

    #[test]
    fn seasons_start_on_the_right_months() {
        assert_eq!(Span::Season.start(date(2026, 1, 9)), date(2025, 12, 1));
        assert_eq!(Span::Season.start(date(2026, 12, 9)), date(2026, 12, 1));
        assert_eq!(Span::Season.start(date(2026, 10, 18)), date(2026, 9, 1));
        assert_eq!(Span::Month.start(date(2026, 10, 18)), date(2026, 10, 1));
        assert_eq!(season_name(9, Hemisphere::South), "Spring");
        assert_eq!(season_name(9, Hemisphere::North), "Autumn");
        assert_eq!(season_name(12, Hemisphere::South), "Summer");
        assert_eq!(
            growing_start(Hemisphere::South, date(2026, 3, 1)),
            date(2025, 7, 1)
        );
        assert_eq!(
            frost_start(Hemisphere::North, date(2026, 3, 1)),
            date(2025, 7, 1)
        );
    }

    #[test]
    fn hemisphere_from_the_history() {
        let days = vec![
            day(date(2025, 1, 10), 28.0, 15.0, 0.0),
            day(date(2025, 7, 10), 14.0, 4.0, 0.0),
        ];
        assert_eq!(hemisphere(&days, None), Hemisphere::South);
        assert_eq!(hemisphere(&days, Some(51.5)), Hemisphere::North);
        assert_eq!(hemisphere(&[], None), Hemisphere::North);
    }

    #[test]
    fn running_totals_by_period() {
        let mut days = Vec::new();
        for y in [2024, 2025, 2026] {
            for d in 1..=30 {
                if y == 2026 && d > 10 {
                    break;
                }
                days.push(day(
                    date(y, 9, d),
                    20.0,
                    10.0,
                    if d % 2 == 0 { 1.0 } else { 0.0 },
                ));
            }
        }
        // 2024 is missing a day in four: still nine tenths there.
        days.retain(|d| !(d.date.year() == 2024 && d.date.day() % 10 == 0 && d.date.day() < 30));
        let s = running_totals(&days, date(2026, 9, 1), 1, |d| d.rain);
        assert_eq!(
            s.iter().map(|s| s.year).collect::<Vec<_>>(),
            [2024, 2025, 2026]
        );
        let now = s.last().unwrap();
        assert_eq!(now.values[9], Some(5.0));
        assert_eq!(now.values[10], None, "stops at the last day on record");
        assert_eq!(s[1].values[29], Some(15.0));
        // 2024 lost the 10th and 20th, both wet.
        assert_eq!(s[0].values[29], Some(13.0));
        let (lo, hi) = spread(&s, Some(2026));
        assert_eq!((lo[29], hi[29]), (Some(13.0), Some(15.0)));
        // A sparse past period is left out.
        days.retain(|d| d.date.year() != 2025 || d.date.day() < 20);
        let s = running_totals(&days, date(2026, 9, 1), 1, |d| d.rain);
        assert_eq!(s.iter().map(|s| s.year).collect::<Vec<_>>(), [2024, 2026]);
    }

    #[test]
    fn frost_years_and_normals() {
        let mut days = Vec::new();
        for y in [2024, 2025, 2026] {
            let mut d = date(y, 1, 1);
            while d.year() == y && !(y == 2026 && d > date(2026, 10, 18)) {
                let frosty = (y == 2024 && (d == date(2024, 5, 20) || d == date(2024, 9, 1)))
                    || (y == 2025 && d == date(2025, 6, 10));
                days.push(day(d, 15.0, if frosty { -1.0 } else { 5.0 }, 0.0));
                d += Duration::days(1);
            }
        }
        let years = frost_years(&days, date(2026, 1, 1), 0.0);
        assert_eq!(years.len(), 3);
        assert_eq!(years[0].first(), Some(date(2024, 5, 20)));
        assert_eq!(years[0].last(), Some(date(2024, 9, 1)));
        assert_eq!(years[1].nights.len(), 1);
        assert!(years[2].nights.is_empty() && !years[2].complete);
        let n = frost_normals(&years, Some(date(2026, 1, 1)));
        assert_eq!((n.years, n.frost_free), (2, 0));
        assert_eq!(n.nights, Some(1.5));
        assert_eq!(n.earliest_first, Some(date(2024, 5, 20)));
        assert_eq!(n.latest_last, Some(date(2024, 9, 1)));
        // 2024: 140 and 244 days in; 2025: 160 both.
        assert_eq!(n.first, Some(150.0));
        assert_eq!(n.last, Some(202.0));
    }

    #[test]
    fn degree_days_and_month_totals() {
        let d = day(date(2026, 1, 1), 26.0, 14.0, 0.0);
        assert_eq!(degree_days(&d, 10.0), Some(10.0));
        assert_eq!(degree_days(&day(d.date, 12.0, 2.0, 0.0), 10.0), Some(0.0));
        let mut days = Vec::new();
        for (y, hours) in [(2024, 6.0), (2025, 8.0), (2026, 9.0)] {
            for k in 1..=30 {
                let mut r = day(date(y, 4, k), 20.0, 10.0, 0.0);
                r.sunshine = Some(hours);
                days.push(r);
            }
        }
        let t = monthly_totals(&days, |d| d.sunshine);
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].total, 180.0);
        assert_eq!(t[2].days, 30);
        let n = month_normals(&t, Some(2026));
        assert_eq!(n[3], Some(210.0));
        assert_eq!(n[0], None);
        let avg = doy_mean(&days, 3, |d| d.sunshine);
        assert!((avg[doy_index(date(2026, 4, 15))].unwrap() - 23.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn evaporation_in_the_station_units() {
        let mut d = day(date(2026, 1, 15), 30.0, 16.0, 0.0);
        let mm = Units::default();
        assert_eq!(day_et(&d, None, &mm), None);
        d.et = Some(6.2);
        assert_eq!(day_et(&d, None, &mm), Some(6.2));
        let est = day_et(&d, Some(-34.0), &mm).unwrap();
        let us = Units {
            temp: "°F".into(),
            rain: "in".into(),
            ..Units::default()
        };
        let f = day(d.date, 86.0, 60.8, 0.0);
        let inches = day_et(&f, Some(-34.0), &us).unwrap();
        assert!((inches * 25.4 - est).abs() < 1e-6);
    }
}
