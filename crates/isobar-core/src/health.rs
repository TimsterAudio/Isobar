//! Checks on the data itself, for the health page: days missing from the
//! history, and gaps in the recent readings.

use chrono::{Duration, NaiveDate};

use crate::dayfile::DayRecord;

/// A run of missing days, `from` to `to` inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MissingDays {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

impl MissingDays {
    pub fn len(&self) -> i64 {
        (self.to - self.from).num_days() + 1
    }

    pub fn is_empty(&self) -> bool {
        self.to < self.from
    }
}

/// The days absent between the first and last day on record.
pub fn missing_days(days: &[DayRecord]) -> Vec<MissingDays> {
    days.windows(2)
        .filter(|w| (w[1].date - w[0].date).num_days() > 1)
        .map(|w| MissingDays {
            from: w[0].date + Duration::days(1),
            to: w[1].date - Duration::days(1),
        })
        .collect()
}

/// Days on record without a temperature or a rain figure.
pub fn incomplete_days(days: &[DayRecord]) -> Vec<NaiveDate> {
    days.iter()
        .filter(|d| d.temp_max.is_none() || d.temp_min.is_none() || d.rain.is_none())
        .map(|d| d.date)
        .collect()
}

/// Stretches longer than `max_gap` ms with no reading, between `since` and
/// `now`, as (start, end) ms. The time before the first reading counts too.
pub fn reading_gaps(ts: &[i64], since: i64, now: i64, max_gap: i64) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    let mut prev = since;
    for &t in ts.iter().filter(|t| **t >= since && **t <= now) {
        if t - prev > max_gap {
            out.push((prev, t));
        }
        prev = t;
    }
    if now - prev > max_gap {
        out.push((prev, now));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(y: i32, m: u32, d: u32) -> DayRecord {
        DayRecord {
            date: NaiveDate::from_ymd_opt(y, m, d).unwrap(),
            temp_max: Some(20.0),
            temp_min: Some(10.0),
            rain: Some(0.0),
            ..Default::default()
        }
    }

    #[test]
    fn finds_missing_and_incomplete_days() {
        let mut days = vec![
            rec(2026, 2, 26),
            rec(2026, 3, 2),
            rec(2026, 3, 3),
            rec(2026, 3, 5),
        ];
        let gaps = missing_days(&days);
        assert_eq!(gaps.len(), 2);
        assert_eq!(gaps[0].from, NaiveDate::from_ymd_opt(2026, 2, 27).unwrap());
        assert_eq!(gaps[0].len(), 3);
        assert_eq!(gaps[1].len(), 1);
        days[1].rain = None;
        assert_eq!(incomplete_days(&days), [days[1].date]);
    }

    #[test]
    fn finds_gaps_in_readings() {
        let min = 60_000;
        let ts: Vec<i64> = (10..40).chain(70..95).map(|m| m * min).collect();
        let gaps = reading_gaps(&ts, 0, 120 * min, 20 * min);
        assert_eq!(gaps, [(39 * min, 70 * min), (94 * min, 120 * min)]);
        assert!(reading_gaps(&ts, 10 * min, 100 * min, 20 * min).len() == 1);
    }
}
