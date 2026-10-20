//! Indoor against outdoor: how the house holds its heat. From inside and
//! outside temperatures recorded together (MX's logs, or Isobar's own
//! samples), day by day: how far each swings, how long the inside lags the
//! outside, and how fast the house gives up its warmth overnight.

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone, Timelike};

/// One day's inside and outside readings, summed up.
#[derive(Debug, Clone, PartialEq)]
pub struct IndoorDay {
    pub date: NaiveDate,
    pub in_mean: f64,
    pub out_mean: f64,
    /// Highest less lowest.
    pub in_range: f64,
    pub out_range: f64,
    /// Local hour (with fraction) of the day's warmest reading.
    pub in_peak: f64,
    pub out_peak: f64,
    /// Overnight, from 22:00 to 05:00: hours for the inside's lead over the
    /// outside to halve. None when the house was heated, gained warmth, or
    /// barely led the outside to begin with.
    pub half_life: Option<f64>,
}

/// A calendar month across every year, from the days in [`IndoorDay`].
#[derive(Debug, Clone, PartialEq)]
pub struct IndoorMonth {
    /// 1 to 12.
    pub month: u32,
    pub days: usize,
    pub in_mean: f64,
    pub out_mean: f64,
    /// Median daily swings.
    pub in_range: f64,
    pub out_range: f64,
    /// Median hours from the outside's warmest to the inside's.
    pub lag: Option<f64>,
    /// Median overnight half-life, in hours.
    pub half_life: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndoorSummary {
    pub first: NaiveDate,
    pub last: NaiveDate,
    pub days: usize,
    pub in_range: f64,
    pub out_range: f64,
    pub lag: Option<f64>,
    pub half_life: Option<f64>,
    pub nights: usize,
    pub months: Vec<IndoorMonth>,
}

/// Readings 15 minutes apart or closer count as taken together.
const PAIR_MS: i64 = 15 * 60_000;

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

/// Inside readings each with the outside reading nearest in time.
fn pairs(inside: &[(i64, f64)], outside: &[(i64, f64)]) -> Vec<(i64, f64, f64)> {
    let mut out = Vec::with_capacity(inside.len());
    let mut j = 0;
    for &(t, v) in inside {
        while j + 1 < outside.len() && (outside[j + 1].0 - t).abs() <= (outside[j].0 - t).abs() {
            j += 1;
        }
        if let Some(&(to, o)) = outside.get(j) {
            if (to - t).abs() <= PAIR_MS {
                out.push((t, v, o));
            }
        }
    }
    out
}

fn local(ms: i64) -> Option<chrono::DateTime<Local>> {
    Local.timestamp_millis_opt(ms).single()
}

fn hour_of(ms: i64) -> f64 {
    local(ms).map_or(0.0, |d| d.hour() as f64 + d.minute() as f64 / 60.0)
}

fn at_hour(date: NaiveDate, hour: u32) -> Option<i64> {
    Local
        .from_local_datetime(&date.and_hms_opt(hour, 0, 0)?)
        .earliest()
        .map(|d| d.timestamp_millis())
}

/// Each day with readings in at least 20 of its hours, both inside and out.
pub fn indoor_days(inside: &[(i64, f64)], outside: &[(i64, f64)]) -> Vec<IndoorDay> {
    let all = pairs(inside, outside);
    let mut by_day: BTreeMap<NaiveDate, Vec<(i64, f64, f64)>> = BTreeMap::new();
    for p in &all {
        if let Some(d) = local(p.0) {
            by_day.entry(d.date_naive()).or_default().push(*p);
        }
    }
    let mut out = Vec::new();
    for (date, ps) in &by_day {
        let mut hours = [false; 24];
        for p in ps {
            hours[hour_of(p.0) as usize % 24] = true;
        }
        if hours.iter().filter(|h| **h).count() < 20 {
            continue;
        }
        let n = ps.len() as f64;
        let max_by = |f: fn(&(i64, f64, f64)) -> f64| {
            ps.iter()
                .copied()
                .max_by(|a, b| f(a).total_cmp(&f(b)))
                .unwrap()
        };
        let min_of = |f: fn(&(i64, f64, f64)) -> f64| ps.iter().map(f).fold(f64::MAX, f64::min);
        let in_hi = max_by(|p| p.1);
        let out_hi = max_by(|p| p.2);
        out.push(IndoorDay {
            date: *date,
            in_mean: ps.iter().map(|p| p.1).sum::<f64>() / n,
            out_mean: ps.iter().map(|p| p.2).sum::<f64>() / n,
            in_range: in_hi.1 - min_of(|p| p.1),
            out_range: out_hi.2 - min_of(|p| p.2),
            in_peak: hour_of(in_hi.0),
            out_peak: hour_of(out_hi.0),
            half_life: night_half_life(&all, *date),
        });
    }
    out
}

/// From 22:00 on `date` to 05:00 the next morning (before any morning
/// heating): how many hours the inside's lead over the night's average
/// outside takes to halve, taking the cooling as exponential (Newton's law).
fn night_half_life(all: &[(i64, f64, f64)], date: NaiveDate) -> Option<f64> {
    let from = at_hour(date, 22)?;
    let to = at_hour(date + Duration::days(1), 5)?;
    let a = all.partition_point(|p| p.0 < from);
    let b = all.partition_point(|p| p.0 <= to);
    let night = all.get(a..b)?;
    let (first, last) = (night.first()?, night.last()?);
    // Most of the night has to be there.
    if first.0 - from > 30 * 60_000 || to - last.0 > 30 * 60_000 {
        return None;
    }
    let outside = night.iter().map(|p| p.2).sum::<f64>() / night.len() as f64;
    let (d0, d1) = (first.1 - outside, last.1 - outside);
    let hours = (last.0 - first.0) as f64 / 3_600_000.0;
    if d0 < 2.0 || d1 <= 0.2 || d1 >= d0 * 0.95 || hours < 6.0 {
        return None;
    }
    Some((hours * std::f64::consts::LN_2 / (d0 / d1).ln()).min(96.0))
}

/// Calendar months across every year.
pub fn indoor_months(days: &[IndoorDay]) -> Vec<IndoorMonth> {
    let mut out = Vec::new();
    for m in 1..=12 {
        let ds: Vec<&IndoorDay> = days.iter().filter(|d| d.date.month() == m).collect();
        if ds.is_empty() {
            continue;
        }
        let n = ds.len() as f64;
        out.push(IndoorMonth {
            month: m,
            days: ds.len(),
            in_mean: ds.iter().map(|d| d.in_mean).sum::<f64>() / n,
            out_mean: ds.iter().map(|d| d.out_mean).sum::<f64>() / n,
            in_range: median(ds.iter().map(|d| d.in_range).collect()).unwrap_or(0.0),
            out_range: median(ds.iter().map(|d| d.out_range).collect()).unwrap_or(0.0),
            lag: median(lags(&ds)),
            half_life: median(ds.iter().filter_map(|d| d.half_life).collect()),
        });
    }
    out
}

/// Hours from the outside's warmest to the inside's, where the inside
/// peaked within 12 hours after it.
fn lags(ds: &[&IndoorDay]) -> Vec<f64> {
    ds.iter()
        .map(|d| (d.in_peak - d.out_peak).rem_euclid(24.0))
        .filter(|l| *l <= 12.0)
        .collect()
}

pub fn indoor_summary(inside: &[(i64, f64)], outside: &[(i64, f64)]) -> Option<IndoorSummary> {
    let days = indoor_days(inside, outside);
    let (first, last) = (days.first()?.date, days.last()?.date);
    let all: Vec<&IndoorDay> = days.iter().collect();
    let halves: Vec<f64> = days.iter().filter_map(|d| d.half_life).collect();
    Some(IndoorSummary {
        first,
        last,
        days: days.len(),
        in_range: median(days.iter().map(|d| d.in_range).collect())?,
        out_range: median(days.iter().map(|d| d.out_range).collect())?,
        lag: median(lags(&all)),
        nights: halves.len(),
        half_life: median(halves),
        months: indoor_months(&days),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three days of a house that damps the outside swing and cools
    /// overnight with a half-life of about 8 hours.
    #[test]
    fn sums_up_a_house() {
        let start = at_hour(NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(), 0).unwrap();
        let step = 10 * 60_000;
        let mut inside = Vec::new();
        let mut outside = Vec::new();
        for k in 0..(3 * 144) {
            let t = start + k * step;
            let h = hour_of(t);
            // Outside: 4 to 14°, warmest at 15:00.
            let o = 9.0 + 5.0 * ((h - 9.0) / 24.0 * std::f64::consts::TAU).sin();
            // Inside: heated to 20° in the day, then cooling towards the
            // night's outside with a half-life of 8 hours from 22:00.
            let i = if (6.0..22.0).contains(&h) {
                20.0 + 0.5 * ((h - 12.0) / 24.0 * std::f64::consts::TAU).sin()
            } else {
                let since = if h >= 22.0 { h - 22.0 } else { h + 2.0 };
                6.0 + 14.5 * 0.5f64.powf(since / 8.0)
            };
            inside.push((t, i));
            outside.push((t + 60_000, o));
        }
        let s = indoor_summary(&inside, &outside).unwrap();
        assert_eq!(s.days, 3);
        assert!(s.out_range > 9.5 && s.out_range < 10.1, "{}", s.out_range);
        assert!(s.nights >= 2);
        let hl = s.half_life.unwrap();
        // The night's average outside isn't the 6° the inside heads for, so
        // the estimate runs a little long.
        assert!(hl > 6.0 && hl < 12.0, "{hl}");
        assert_eq!(s.months.len(), 1);
        assert_eq!(s.months[0].month, 7);
    }
}
