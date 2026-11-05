//! Year-on-year comparisons: each year's temperature against normal (the
//! temperature trend), and heating and cooling degree days against normal
//! and last year.

use chrono::{Datelike, Duration, NaiveDate};

use crate::analytics::doy_index;
use crate::seasons::{season_name, Hemisphere};
use crate::DayRecord;

/// The day's mean temperature: MX's average, or halfway between the high
/// and low.
pub fn day_mean(d: &DayRecord) -> Option<f64> {
    d.temp_avg
        .or_else(|| Some((d.temp_max? + d.temp_min?) / 2.0))
}

// ---------------------------------------------------------------- trend

/// One year's temperature against normal.
#[derive(Debug, Clone, PartialEq)]
pub struct YearTemp {
    pub year: i32,
    /// The average of the year's daily means.
    pub mean: f64,
    /// The average of each day's mean against that date's normal, so a part
    /// year compares fairly with a whole one.
    pub anomaly: f64,
    pub days: usize,
}

impl YearTemp {
    /// Fewer than 330 days on record (the first and current years, mostly).
    pub fn partial(&self) -> bool {
        self.days < 330
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TempTrend {
    pub years: Vec<YearTemp>,
    /// The average daily mean over every day on record.
    pub normal: f64,
    /// The straight-line fit through the years with at least 180 days, in
    /// degrees a decade, and its standard error. `None` with fewer than five
    /// such years.
    pub per_decade: Option<(f64, f64)>,
    /// The fitted line's value for each year, when there is one.
    pub fit: Vec<Option<f64>>,
}

impl TempTrend {
    /// Whether the fit is clearly more than year-to-year noise: at least
    /// twice its standard error from flat.
    pub fn clear(&self) -> bool {
        self.per_decade.is_some_and(|(s, e)| s.abs() >= 2.0 * e)
    }
}

/// Each year's temperature against normal. Each day is measured against the
/// average for its date (±7 days, all years), smoothing out the seasons.
pub fn temperature_trend(days: &[DayRecord]) -> Option<TempTrend> {
    let mut buckets = vec![(0usize, 0.0f64); 366];
    let mut all = (0usize, 0.0f64);
    for d in days {
        if let Some(m) = day_mean(d) {
            let b = &mut buckets[doy_index(d.date)];
            b.0 += 1;
            b.1 += m;
            all.0 += 1;
            all.1 += m;
        }
    }
    if all.0 < 365 {
        return None;
    }
    let normal_at: Vec<Option<f64>> = (0..366isize)
        .map(|i| {
            let (mut n, mut s) = (0usize, 0.0);
            for k in -7..=7isize {
                let b = buckets[(i + k).rem_euclid(366) as usize];
                n += b.0;
                s += b.1;
            }
            (n >= 10).then(|| s / n as f64)
        })
        .collect();
    let mut years: Vec<YearTemp> = Vec::new();
    for d in days {
        let (Some(m), Some(norm)) = (day_mean(d), normal_at[doy_index(d.date)]) else {
            continue;
        };
        let y = d.date.year();
        if years.last().is_none_or(|l| l.year != y) {
            years.push(YearTemp {
                year: y,
                mean: 0.0,
                anomaly: 0.0,
                days: 0,
            });
        }
        let t = years.last_mut().unwrap();
        t.mean += m;
        t.anomaly += m - norm;
        t.days += 1;
    }
    for t in &mut years {
        t.mean /= t.days as f64;
        t.anomaly /= t.days as f64;
    }
    years.retain(|t| t.days >= 30);
    let pts: Vec<(f64, f64)> = years
        .iter()
        .filter(|t| t.days >= 180)
        .map(|t| (t.year as f64, t.anomaly))
        .collect();
    let line = fit_line(&pts);
    let fit = years
        .iter()
        .map(|t| line.map(|(a, b, _)| a + b * t.year as f64))
        .collect();
    Some(TempTrend {
        normal: all.1 / all.0 as f64,
        per_decade: line.map(|(_, b, se)| (b * 10.0, se * 10.0)),
        fit,
        years,
    })
}

/// Least squares: (intercept, slope, slope's standard error), with at
/// least five points.
fn fit_line(pts: &[(f64, f64)]) -> Option<(f64, f64, f64)> {
    let n = pts.len() as f64;
    if pts.len() < 5 {
        return None;
    }
    let mx = pts.iter().map(|p| p.0).sum::<f64>() / n;
    let my = pts.iter().map(|p| p.1).sum::<f64>() / n;
    let sxx: f64 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
    if sxx == 0.0 {
        return None;
    }
    let sxy: f64 = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    let b = sxy / sxx;
    let a = my - b * mx;
    let rss: f64 = pts.iter().map(|p| (p.1 - a - b * p.0).powi(2)).sum();
    let se = (rss / (n - 2.0) / sxx).sqrt();
    Some((a, b, se))
}

// ---------------------------------------------------------------- degree days

/// A day's heating and cooling degree days: MX's own when the dayfile has
/// them (with the base set in MX), otherwise how far the day's mean is
/// below or above `base`.
pub fn day_degrees(d: &DayRecord, base: f64) -> Option<(f64, f64)> {
    if d.hdd.is_some() || d.cdd.is_some() {
        return Some((d.hdd.unwrap_or(0.0), d.cdd.unwrap_or(0.0)));
    }
    let m = day_mean(d)?;
    Some(((base - m).max(0.0), (m - base).max(0.0)))
}

/// Heating and cooling degree days over one stretch, with last year's and
/// the normal for the same dates.
#[derive(Debug, Clone, PartialEq)]
pub struct DegreeRow {
    pub label: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// The stretch runs on past the last day on record (this month so far).
    pub partial: bool,
    pub this: Option<(f64, f64)>,
    pub last_year: Option<(f64, f64)>,
    /// The average over every other year with nine tenths of these days.
    pub normal: Option<(f64, f64)>,
    pub years: usize,
}

/// Totals from `start` to `end` inclusive, if nine tenths of the days are
/// on record.
fn degree_total(
    days: &[DayRecord],
    start: NaiveDate,
    end: NaiveDate,
    base: f64,
) -> Option<(f64, f64)> {
    let want = (end - start).num_days() + 1;
    let slice = crate::analytics::in_range(days, start, end);
    let mut n = 0;
    let (mut h, mut c) = (0.0, 0.0);
    for d in slice {
        if let Some((a, b)) = day_degrees(d, base) {
            n += 1;
            h += a;
            c += b;
        }
    }
    (n as f64 >= want as f64 * 0.9).then_some((h, c))
}

/// The same day `years` years later (28 February for a missing 29th).
fn shift(d: NaiveDate, years: i32) -> NaiveDate {
    let y = d.year() + years;
    NaiveDate::from_ymd_opt(y, d.month(), d.day())
        .or_else(|| NaiveDate::from_ymd_opt(y, d.month(), 28))
        .unwrap_or(d)
}

fn degree_row(
    days: &[DayRecord],
    label: String,
    start: NaiveDate,
    end: NaiveDate,
    last: NaiveDate,
    base: f64,
) -> DegreeRow {
    let partial = end > last;
    let to = end.min(last);
    let (Some(first), Some(newest)) = (days.first(), days.last()) else {
        return DegreeRow {
            label,
            start,
            end,
            partial,
            this: None,
            last_year: None,
            normal: None,
            years: 0,
        };
    };
    let mut sum = (0.0, 0.0);
    let mut years = 0;
    let mut k = 1;
    loop {
        let (s, e) = (shift(start, -k), shift(to, -k));
        if e < first.date {
            break;
        }
        if let Some(t) = degree_total(days, s, e, base) {
            sum.0 += t.0;
            sum.1 += t.1;
            years += 1;
        }
        k += 1;
    }
    // Years after this one (when looking back at an earlier season) count
    // towards the normal too.
    let mut k = 1;
    while shift(to, k) <= newest.date {
        if let Some(t) = degree_total(days, shift(start, k), shift(to, k), base) {
            sum.0 += t.0;
            sum.1 += t.1;
            years += 1;
        }
        k += 1;
    }
    DegreeRow {
        label,
        start,
        end,
        partial,
        this: degree_total(days, start, to, base),
        last_year: degree_total(days, shift(start, -1), shift(to, -1), base),
        normal: (years > 0).then(|| (sum.0 / years as f64, sum.1 / years as f64)),
        years,
    }
}

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

fn month_end(start: NaiveDate) -> NaiveDate {
    start
        .checked_add_months(chrono::Months::new(1))
        .map_or(start, |d| d - Duration::days(1))
}

/// Each month of `last`'s year to date, the last one so far.
pub fn degree_months(days: &[DayRecord], last: NaiveDate, base: f64) -> Vec<DegreeRow> {
    (1..=last.month())
        .map(|m| {
            let start = NaiveDate::from_ymd_opt(last.year(), m, 1).unwrap();
            degree_row(
                days,
                MONTH_NAMES[m as usize - 1].to_string(),
                start,
                month_end(start),
                last,
                base,
            )
        })
        .collect()
}

/// The last four seasons, newest (the one under way) first.
pub fn degree_seasons(
    days: &[DayRecord],
    last: NaiveDate,
    h: Hemisphere,
    base: f64,
) -> Vec<DegreeRow> {
    let mut start = crate::seasons::Span::Season.start(last);
    let mut out = Vec::new();
    for _ in 0..4 {
        let end = start
            .checked_add_months(chrono::Months::new(3))
            .map_or(start, |d| d - Duration::days(1));
        let label = format!(
            "{} {}",
            season_name(start.month(), h),
            if start.month() == 12 {
                format!("{}–{}", start.year(), (start.year() + 1) % 100)
            } else {
                start.year().to_string()
            }
        );
        out.push(degree_row(days, label, start, end, last, base));
        start = start
            .checked_sub_months(chrono::Months::new(3))
            .unwrap_or(start);
    }
    out
}

/// The calendar year to `last`.
pub fn degree_year(days: &[DayRecord], last: NaiveDate, base: f64) -> DegreeRow {
    let start = NaiveDate::from_ymd_opt(last.year(), 1, 1).unwrap();
    let end = NaiveDate::from_ymd_opt(last.year(), 12, 31).unwrap();
    degree_row(
        days,
        format!("{} so far", last.year()),
        start,
        end,
        last,
        base,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(d: NaiveDate, mean: f64) -> DayRecord {
        DayRecord {
            date: d,
            temp_max: Some(mean + 5.0),
            temp_min: Some(mean - 5.0),
            ..Default::default()
        }
    }

    /// Ten years with a seasonal swing, each 0.1° warmer than the last.
    fn years(step: f64) -> Vec<DayRecord> {
        let mut out = Vec::new();
        let mut d = NaiveDate::from_ymd_opt(2015, 1, 1).unwrap();
        while d <= NaiveDate::from_ymd_opt(2024, 12, 31).unwrap() {
            let season = (d.ordinal() as f64 / 365.25 * std::f64::consts::TAU).cos() * 6.0;
            let mean = 14.0 + season + (d.year() - 2015) as f64 * step;
            out.push(day(d, mean));
            d += Duration::days(1);
        }
        out
    }

    #[test]
    fn trend_finds_the_slope_either_way() {
        let t = temperature_trend(&years(0.1)).unwrap();
        assert_eq!(t.years.len(), 10);
        let (slope, se) = t.per_decade.unwrap();
        assert!((slope - 1.0).abs() < 0.01, "{slope}");
        assert!(t.clear() && se < 0.01);
        assert!(t.years[0].anomaly < 0.0 && t.years[9].anomaly > 0.0);
        let t = temperature_trend(&years(-0.05)).unwrap();
        assert!((t.per_decade.unwrap().0 + 0.5).abs() < 0.01);
        let flat = temperature_trend(&years(0.0)).unwrap();
        // Leap years shift the made-up seasons a little; near enough flat.
        assert!(flat.per_decade.unwrap().0.abs() < 0.02);
    }

    #[test]
    fn a_part_year_compares_fairly() {
        // A flat climate, with the last year only to March (summer in the
        // north of this made-up climate): its anomaly stays near zero.
        let mut d = years(0.0);
        d.retain(|r| r.date < NaiveDate::from_ymd_opt(2024, 4, 1).unwrap());
        let t = temperature_trend(&d).unwrap();
        let last = t.years.last().unwrap();
        assert!(last.partial());
        assert!(last.anomaly.abs() < 0.05, "{}", last.anomaly);
    }

    #[test]
    fn degree_days_against_normal_and_last_year() {
        let d = years(0.1);
        let last = NaiveDate::from_ymd_opt(2024, 7, 15).unwrap();
        let d: Vec<DayRecord> = d.into_iter().filter(|r| r.date <= last).collect();
        let months = degree_months(&d, last, 15.5);
        assert_eq!(months.len(), 7);
        let jan = &months[0];
        let (h, c) = jan.this.unwrap();
        let (lh, _) = jan.last_year.unwrap();
        let (nh, _) = jan.normal.unwrap();
        // January is the warm end of this climate; heating is lower each
        // year as it warms.
        assert!(c > 0.0);
        assert!(h <= lh && lh <= nh + 1e-9);
        assert_eq!(jan.years, 9);
        let july = &months[6];
        assert!(july.partial);
        assert_eq!(july.end, NaiveDate::from_ymd_opt(2024, 7, 31).unwrap());
        // July so far is compared with 1–15 July in other years.
        let full = degree_total(
            &d,
            NaiveDate::from_ymd_opt(2023, 7, 1).unwrap(),
            NaiveDate::from_ymd_opt(2023, 7, 15).unwrap(),
            15.5,
        );
        assert_eq!(july.last_year, full);
        let s = degree_seasons(&d, last, Hemisphere::South, 15.5);
        assert_eq!(s[0].label, "Winter 2024");
        assert_eq!(s[1].label, "Autumn 2024");
        assert_eq!(s[2].label, "Summer 2023–24");
        let y = degree_year(&d, last, 15.5);
        assert!(y.partial && y.this.is_some());
    }

    #[test]
    fn mx_degree_days_win() {
        let mut r = day(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(), 10.0);
        assert_eq!(day_degrees(&r, 15.5), Some((5.5, 0.0)));
        r.hdd = Some(8.0);
        assert_eq!(day_degrees(&r, 15.5), Some((8.0, 0.0)));
    }
}
