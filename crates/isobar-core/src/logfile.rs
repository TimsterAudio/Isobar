//! MX's monthly log files: a line every logging interval (10 minutes unless
//! changed), served page by page from `/api/data/logfile`.
//!
//! Field positions follow `LogFileRec.CurrentToCsv` in the Cumulus MX source
//! (zero-based, date and time first). MX 4 and later write the Unix time in
//! the second field; the API puts the line number in front of the fields.

use serde_json::Value;

/// The readings Isobar keeps from each log line, by the names the live
/// samples use, and where each is in the line.
pub const LOG_METRICS: [(&str, usize); 16] = [
    ("temp", 2),
    ("hum", 3),
    ("dew", 4),
    ("wind", 5),
    ("gust", 6),
    ("bearing", 7),
    ("rain_rate", 8),
    ("rain_today", 9),
    ("press", 10),
    ("intemp", 12),
    ("inhum", 13),
    ("uv", 17),
    ("solar", 18),
    ("solar_max", 22),
    ("rain_midnight", 26),
    ("feels", 27),
];

/// One line of a log file.
#[derive(Debug, Clone, PartialEq)]
pub struct LogRecord {
    /// Unix ms.
    pub ts: i64,
    /// Values in the order of [`LOG_METRICS`].
    pub values: [Option<f64>; LOG_METRICS.len()],
}

impl LogRecord {
    pub fn get(&self, metric: &str) -> Option<f64> {
        let i = LOG_METRICS.iter().position(|m| m.0 == metric)?;
        self.values[i]
    }
}

fn number(v: &Value) -> Option<f64> {
    let x = match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() || t.chars().all(|c| c == '-') {
                return None;
            }
            t.replace(',', ".").parse().ok()
        }
        _ => None,
    }?;
    x.is_finite().then_some(x)
}

/// A row from the API: `[line number, "dd/MM/yy HH:mm", "unix", ...]`.
pub fn from_api_row(row: &Value) -> Option<LogRecord> {
    let a = row.as_array()?;
    let field = |i: usize| a.get(i + 1);
    let ts = number(field(1)?)? as i64;
    // Old MX 3 lines had a time here instead; those can't be placed.
    if ts < 946_684_800 {
        return None;
    }
    let mut values = [None; LOG_METRICS.len()];
    for (k, (_, pos)) in LOG_METRICS.iter().enumerate() {
        values[k] = field(*pos).and_then(number);
    }
    Some(LogRecord {
        ts: ts * 1000,
        values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_an_api_row() {
        // A line as MX 5 writes it, with the API's line number in front.
        let row = json!([
            42,
            "18/10/26 07:30",
            "1792358400",
            "9.8",
            "82",
            "7.0",
            "0.0",
            "2.0",
            "221",
            "0.0",
            "0.0",
            "1033.8",
            "613.9",
            "17.9",
            "51",
            "0.0",
            "9.8",
            "9.8",
            "0.0",
            "28",
            "0.12",
            "843.2",
            "9.2",
            "83",
            "0.0",
            "89",
            "0.0",
            "0.0",
            "9.8",
            "9.8",
            "-",
            "-"
        ]);
        let r = from_api_row(&row).unwrap();
        assert_eq!(r.ts, 1_792_358_400_000);
        assert_eq!(r.get("temp"), Some(9.8));
        assert_eq!(r.get("hum"), Some(82.0));
        assert_eq!(r.get("bearing"), Some(221.0));
        assert_eq!(r.get("press"), Some(1033.8));
        assert_eq!(r.get("intemp"), Some(17.9));
        assert_eq!(r.get("solar"), Some(28.0));
        assert_eq!(r.get("solar_max"), Some(83.0));
        assert_eq!(r.get("feels"), Some(9.8));
        // An MX 3 line has a time where the Unix time goes.
        assert_eq!(from_api_row(&json!([1, "18/10/26", "07:30", "9.8"])), None);
    }
}

// ---------------------------------------------------------------- analysis

use std::collections::VecDeque;

use chrono::{Datelike, TimeZone};

/// A fall in pressure within 24 hours: from the highest reading in the day
/// before `to_ts` down to the reading at `to_ts`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fall24 {
    pub from_ts: i64,
    pub to_ts: i64,
    pub from: f64,
    pub to: f64,
}

impl Fall24 {
    pub fn fall(&self) -> f64 {
        self.from - self.to
    }
}

const DAY_MS: i64 = 24 * 3_600_000;

/// The `n` biggest 24-hour falls in a pressure series (oldest first), one
/// per storm: falls ending within two days of a bigger one are left out.
pub fn biggest_falls_24h(press: &[(i64, f64)], n: usize) -> Vec<Fall24> {
    // The highest reading in the trailing day, kept with a falling queue.
    let mut q: VecDeque<(i64, f64)> = VecDeque::new();
    let mut best: Vec<Fall24> = Vec::new();
    for &(t, v) in press {
        while q.front().is_some_and(|f| f.0 < t - DAY_MS) {
            q.pop_front();
        }
        while q.back().is_some_and(|b| b.1 <= v) {
            q.pop_back();
        }
        q.push_back((t, v));
        let (ft, fv) = q[0];
        if fv - v > 0.0 {
            best.push(Fall24 {
                from_ts: ft,
                to_ts: t,
                from: fv,
                to: v,
            });
        }
    }
    best.sort_by(|a, b| b.fall().total_cmp(&a.fall()));
    let mut out: Vec<Fall24> = Vec::new();
    for f in best {
        if out.iter().all(|o| (o.to_ts - f.to_ts).abs() > 2 * DAY_MS) {
            out.push(f);
            if out.len() == n {
                break;
            }
        }
    }
    out
}

/// Rain-rate classes by their lower edge in mm an hour: light, moderate,
/// heavy and violent, as forecasters grade rain.
pub const RATE_MM_H: [f64; 4] = [0.0, 2.5, 10.0, 50.0];

/// Rain in a year by how hard it fell.
#[derive(Debug, Clone, PartialEq)]
pub struct BurstYear {
    pub year: i32,
    /// Rain in each rate class, in the station's units.
    pub rain: [f64; 4],
    /// Rain that fell across a gap in the log, so its rate isn't known.
    pub unknown: f64,
}

/// Split the rain in a series of "rain so far today" readings by how fast it
/// fell between readings. A drop in the total is the day's reset. Rain
/// across a gap of more than an hour can't be timed and counts as unknown.
/// `mm_per_unit` converts the station's rain unit to mm (25.4 for inches).
pub fn rain_by_rate(rain_today: &[(i64, f64)], mm_per_unit: f64) -> Vec<BurstYear> {
    let mut out: Vec<BurstYear> = Vec::new();
    for w in rain_today.windows(2) {
        let ((t0, a), (t1, b)) = (w[0], w[1]);
        let fell = if b >= a { b - a } else { b };
        if fell <= 0.0 || t1 <= t0 {
            continue;
        }
        let year = chrono::Local
            .timestamp_millis_opt(t1)
            .single()
            .map_or(1970, |d| d.year());
        if out.last().is_none_or(|y| y.year != year) {
            out.push(BurstYear {
                year,
                rain: [0.0; 4],
                unknown: 0.0,
            });
        }
        let y = out.last_mut().unwrap();
        let hours = (t1 - t0) as f64 / 3_600_000.0;
        if hours > 1.0 {
            y.unknown += fell;
            continue;
        }
        let rate = fell * mm_per_unit / hours;
        let k = RATE_MM_H.iter().rposition(|e| rate >= *e).unwrap_or(0);
        y.rain[k] += fell;
    }
    out
}

/// Isobar keeps at most one log reading per ten minutes: MX's usual
/// interval, close enough for 24-hour falls and rain rates, and a tenth of
/// the size of a one-minute log.
pub const LOG_STEP_MS: i64 = 600_000;

/// Combine readings from one ten-minute step into one, timed at the last.
/// Most readings are averaged; gust and rain rate keep the highest; the
/// bearing and the running rain totals keep the latest.
pub fn combine(recs: &[LogRecord]) -> Option<LogRecord> {
    let last = recs.iter().max_by_key(|r| r.ts)?;
    let mut values = [None; LOG_METRICS.len()];
    for (k, (name, _)) in LOG_METRICS.iter().enumerate() {
        let mut have: Vec<(i64, f64)> = recs
            .iter()
            .filter_map(|r| r.values[k].map(|v| (r.ts, v)))
            .collect();
        if have.is_empty() {
            continue;
        }
        have.sort_by_key(|h| h.0);
        values[k] = Some(match *name {
            "gust" | "rain_rate" => have.iter().map(|h| h.1).fold(f64::MIN, f64::max),
            "bearing" | "rain_today" | "rain_midnight" => have[have.len() - 1].1,
            _ => have.iter().map(|h| h.1).sum::<f64>() / have.len() as f64,
        });
    }
    Some(LogRecord {
        ts: last.ts,
        values,
    })
}

#[cfg(test)]
mod analysis_tests {
    use super::*;

    #[test]
    fn combines_a_step() {
        let rec = |ts: i64, temp: f64, gust: f64, rain: f64| {
            let mut values = [None; LOG_METRICS.len()];
            values[0] = Some(temp);
            values[4] = Some(gust);
            values[7] = Some(rain);
            LogRecord { ts, values }
        };
        let c = combine(&[
            rec(60_000, 10.0, 5.0, 1.0),
            rec(0, 12.0, 9.0, 0.8),
            rec(120_000, 14.0, 3.0, 1.2),
        ])
        .unwrap();
        assert_eq!(c.ts, 120_000, "timed at the last reading");
        assert_eq!(c.get("temp"), Some(12.0));
        assert_eq!(c.get("gust"), Some(9.0));
        assert_eq!(c.get("rain_today"), Some(1.2));
        assert_eq!(c.get("press"), None);
        assert_eq!(combine(&[]), None);
    }

    #[test]
    fn finds_24_hour_falls() {
        let h = 3_600_000;
        // Steady at 1020, then a 30-hour fall of 1 hPa an hour, then steady.
        let mut p: Vec<(i64, f64)> = (0..48).map(|k| (k * h, 1020.0)).collect();
        p.extend((1..=30).map(|k| ((47 + k) * h, 1020.0 - k as f64)));
        p.extend((1..=48).map(|k| ((77 + k) * h, 990.0)));
        let f = biggest_falls_24h(&p, 3);
        assert_eq!(f.len(), 1, "one storm");
        assert_eq!(f[0].fall(), 24.0, "at most 24 hours of the 30-hour fall");
        assert_eq!(f[0].to_ts - f[0].from_ts, 24 * h);
    }

    #[test]
    fn splits_rain_by_rate() {
        let m = 60_000;
        let t0 = 1_780_000_000_000;
        // 10-minute readings: 0.2 mm (1.2 mm/h, light), then 5 mm (30 mm/h,
        // heavy), then a reset and 1 mm, then 3 mm across a two-hour gap.
        let pts = [
            (t0, 1.0),
            (t0 + 10 * m, 1.2),
            (t0 + 20 * m, 6.2),
            (t0 + 30 * m, 1.0),
            (t0 + 150 * m, 4.0),
        ];
        let y = rain_by_rate(&pts, 1.0);
        assert_eq!(y.len(), 1);
        assert!((y[0].rain[0] - 0.2).abs() < 1e-9);
        assert!(
            (y[0].rain[1] - 1.0).abs() < 1e-9,
            "1 mm in 10 minutes is 6 mm/h"
        );
        assert!((y[0].rain[2] - 5.0).abs() < 1e-9);
        assert!((y[0].unknown - 3.0).abs() < 1e-9);
    }
}
