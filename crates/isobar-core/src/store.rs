//! Local SQLite cache.
//!
//! * `days`: one row per meteorological day, stored as JSON so new dayfile
//!   fields never need a migration.
//! * `samples`: live readings (one per minute) plus anything backfilled from
//!   MX graph data, in long format `(metric, ts, value)`.
//! * `tank_log`: an imported tank record (rain, use and adjustments by day).
//! * `log`: MX's log file readings, at most one row per ten minutes (shorter
//!   logging intervals are combined), values in hundredths as integers
//!   (small on disk), kept for good.
//! * `meta`: small key/value pairs such as the last sync time.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::dayfile::DayRecord;
use crate::error::{CoreError, Result};
use crate::logfile::{self, LogRecord, LOG_METRICS, LOG_STEP_MS};
use crate::tanks::LogDay;

pub struct Store {
    conn: Connection,
}

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA journal_size_limit = 8388608;
CREATE TABLE IF NOT EXISTS days (
    date TEXT PRIMARY KEY,
    json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS samples (
    metric TEXT NOT NULL,
    ts INTEGER NOT NULL,
    value REAL NOT NULL,
    PRIMARY KEY (metric, ts)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS samples_ts ON samples(ts);
CREATE TABLE IF NOT EXISTS tank_log (
    date TEXT PRIMARY KEY,
    json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS log (
    ts INTEGER PRIMARY KEY,
    temp INTEGER, hum INTEGER, dew INTEGER, wind INTEGER, gust INTEGER,
    bearing INTEGER, rain_rate INTEGER, rain_today INTEGER, press INTEGER,
    intemp INTEGER, inhum INTEGER, uv INTEGER, solar INTEGER, solar_max INTEGER,
    rain_midnight INTEGER, feels INTEGER
);
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    /// Insert or replace daily records. Returns how many were written.
    pub fn upsert_days(&mut self, days: &[DayRecord]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt =
                tx.prepare("INSERT OR REPLACE INTO days (date, json) VALUES (?1, ?2)")?;
            for d in days {
                let json = serde_json::to_string(d).map_err(|e| CoreError::Parse(e.to_string()))?;
                stmt.execute(params![d.date.to_string(), json])?;
            }
        }
        tx.commit()?;
        Ok(days.len())
    }

    /// All daily records, oldest first.
    pub fn days(&self) -> Result<Vec<DayRecord>> {
        let mut stmt = self.conn.prepare("SELECT json FROM days ORDER BY date")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for json in rows {
            let json = json?;
            if let Ok(d) = serde_json::from_str::<DayRecord>(&json) {
                out.push(d);
            }
        }
        Ok(out)
    }

    pub fn day_count(&self) -> Result<usize> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM days", [], |r| r.get::<_, i64>(0))?
            as usize)
    }

    pub fn clear_days(&mut self) -> Result<()> {
        self.conn.execute("DELETE FROM days", [])?;
        Ok(())
    }

    /// Record one live reading's metrics at `ts` (Unix ms).
    pub fn insert_samples(&mut self, ts: i64, metrics: &[(&str, f64)]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO samples (metric, ts, value) VALUES (?1, ?2, ?3)",
            )?;
            for (m, v) in metrics {
                stmt.execute(params![m, ts, v])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Insert a whole series for one metric (used for backfill). Existing
    /// points at the same timestamps are kept.
    pub fn insert_series(&mut self, metric: &str, points: &[(i64, f64)]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx
                .prepare("INSERT OR IGNORE INTO samples (metric, ts, value) VALUES (?1, ?2, ?3)")?;
            for (ts, v) in points {
                if v.is_finite() {
                    stmt.execute(params![metric, ts, v])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Samples newer than `since_ms`, grouped by metric and sorted by time.
    pub fn samples_since(&self, since_ms: i64) -> Result<BTreeMap<String, Vec<(i64, f64)>>> {
        let mut stmt = self
            .conn
            .prepare("SELECT metric, ts, value FROM samples WHERE ts >= ?1 ORDER BY metric, ts")?;
        let rows = stmt.query_map(params![since_ms], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, f64>(2)?,
            ))
        })?;
        let mut out: BTreeMap<String, Vec<(i64, f64)>> = BTreeMap::new();
        for row in rows {
            let (m, ts, v) = row?;
            out.entry(m).or_default().push((ts, v));
        }
        Ok(out)
    }

    pub fn prune_samples(&self, before_ms: i64) -> Result<usize> {
        Ok(self
            .conn
            .execute("DELETE FROM samples WHERE ts < ?1", params![before_ms])?)
    }

    /// Store log file lines, replacing any at the same times, then combine
    /// the ten-minute steps they fall in, and the step before (the end of the
    /// last page). Only the step holding the newest reading is left as it
    /// is, as it may still be filling.
    pub fn upsert_log(&mut self, recs: &[LogRecord]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        insert_log(&tx, recs)?;
        if let Some(lo) = recs.iter().map(|r| r.ts).min() {
            let newest: Option<i64> = tx.query_row("SELECT MAX(ts) FROM log", [], |r| r.get(0))?;
            thin_log(&tx, lo - LOG_STEP_MS, newest.unwrap_or(lo))?;
        }
        tx.commit()?;
        Ok(recs.len())
    }

    /// Combine every stored log reading to one per ten minutes, a month at a
    /// time, and give the space back. Returns how many rows went.
    pub fn thin_all_log(&mut self) -> Result<usize> {
        let (_, first, last) = self.log_extent()?;
        let (Some(first), Some(last)) = (first, last) else {
            return Ok(0);
        };
        let chunk = 30 * 24 * 3_600_000;
        let mut removed = 0;
        let mut from = first.div_euclid(LOG_STEP_MS) * LOG_STEP_MS;
        while from <= last {
            let to = (from + chunk).min(last);
            let tx = self.conn.transaction()?;
            removed += thin_log(&tx, from, to)?;
            tx.commit()?;
            from += chunk;
        }
        if removed > 0 {
            // VACUUM rewrites the file through the WAL; empty that too.
            self.conn
                .execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);")?;
        }
        Ok(removed)
    }

    /// Log readings of `metrics` from `from_ms` to `to_ms`, by metric.
    pub fn log_series(
        &self,
        metrics: &[&str],
        from_ms: i64,
        to_ms: i64,
    ) -> Result<BTreeMap<String, Vec<(i64, f64)>>> {
        let known: Vec<&str> = metrics
            .iter()
            .copied()
            .filter(|m| LOG_METRICS.iter().any(|k| k.0 == *m))
            .collect();
        let mut out: BTreeMap<String, Vec<(i64, f64)>> = BTreeMap::new();
        if known.is_empty() {
            return Ok(out);
        }
        let mut stmt = self.conn.prepare(&format!(
            "SELECT ts, {} FROM log WHERE ts >= ?1 AND ts <= ?2 ORDER BY ts",
            known.join(", ")
        ))?;
        let mut rows = stmt.query(params![from_ms, to_ms])?;
        while let Some(row) = rows.next()? {
            let ts: i64 = row.get(0)?;
            for (k, m) in known.iter().enumerate() {
                if let Some(v) = row.get::<_, Option<i64>>(k + 1)? {
                    out.entry(m.to_string())
                        .or_default()
                        .push((ts, v as f64 / 100.0));
                }
            }
        }
        Ok(out)
    }

    /// How many log lines are stored, and the first and last times.
    pub fn log_extent(&self) -> Result<(usize, Option<i64>, Option<i64>)> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*), MIN(ts), MAX(ts) FROM log", [], |r| {
                Ok((r.get::<_, i64>(0)? as usize, r.get(1)?, r.get(2)?))
            })?)
    }

    /// The newest stored log time within `from_ms` to `to_ms`.
    pub fn log_last_between(&self, from_ms: i64, to_ms: i64) -> Result<Option<i64>> {
        Ok(self.conn.query_row(
            "SELECT MAX(ts) FROM log WHERE ts >= ?1 AND ts <= ?2",
            params![from_ms, to_ms],
            |r| r.get(0),
        )?)
    }

    /// Replace the tank record with `log`.
    pub fn replace_tank_log(&mut self, log: &[LogDay]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM tank_log", [])?;
        {
            let mut stmt =
                tx.prepare("INSERT OR REPLACE INTO tank_log (date, json) VALUES (?1, ?2)")?;
            for d in log {
                let json = serde_json::to_string(d).map_err(|e| CoreError::Parse(e.to_string()))?;
                stmt.execute(params![d.date.to_string(), json])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The tank record, oldest first.
    pub fn tank_log(&self) -> Result<Vec<LogDay>> {
        let mut stmt = self
            .conn
            .prepare("SELECT json FROM tank_log ORDER BY date")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for json in rows {
            if let Ok(d) = serde_json::from_str::<LogDay>(&json?) {
                out.push(d);
            }
        }
        Ok(out)
    }

    pub fn meta_get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn meta_set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }
}

fn insert_log(tx: &Transaction, recs: &[LogRecord]) -> Result<()> {
    let cols: Vec<&str> = LOG_METRICS.iter().map(|m| m.0).collect();
    let sql = format!(
        "INSERT OR REPLACE INTO log (ts, {}) VALUES (?1, {})",
        cols.join(", "),
        (2..=cols.len() + 1)
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut stmt = tx.prepare_cached(&sql)?;
    for r in recs {
        let mut vals: Vec<Option<i64>> = vec![Some(r.ts)];
        vals.extend(
            r.values
                .iter()
                .map(|v| v.map(|x| (x * 100.0).round() as i64)),
        );
        stmt.execute(rusqlite::params_from_iter(vals))?;
    }
    Ok(())
}

/// Combine the readings in each ten-minute step from the one holding
/// `from_ms` up to, but not including, the one holding `to_ms`, which may
/// still be filling. Returns how many rows went.
fn thin_log(tx: &Transaction, from_ms: i64, to_ms: i64) -> Result<usize> {
    let start = from_ms.div_euclid(LOG_STEP_MS) * LOG_STEP_MS;
    let end = to_ms.div_euclid(LOG_STEP_MS) * LOG_STEP_MS;
    if end <= start {
        return Ok(0);
    }
    let cols: Vec<&str> = LOG_METRICS.iter().map(|m| m.0).collect();
    let mut steps: BTreeMap<i64, Vec<LogRecord>> = BTreeMap::new();
    {
        let mut stmt = tx.prepare_cached(&format!(
            "SELECT ts, {} FROM log WHERE ts >= ?1 AND ts < ?2 ORDER BY ts",
            cols.join(", ")
        ))?;
        let mut rows = stmt.query(params![start, end])?;
        while let Some(row) = rows.next()? {
            let ts: i64 = row.get(0)?;
            let mut values = [None; LOG_METRICS.len()];
            for (k, v) in values.iter_mut().enumerate() {
                *v = row.get::<_, Option<i64>>(k + 1)?.map(|x| x as f64 / 100.0);
            }
            steps
                .entry(ts.div_euclid(LOG_STEP_MS))
                .or_default()
                .push(LogRecord { ts, values });
        }
    }
    let mut removed = 0;
    for (step, recs) in steps {
        if recs.len() < 2 {
            continue;
        }
        let Some(one) = logfile::combine(&recs) else {
            continue;
        };
        let from = step * LOG_STEP_MS;
        tx.execute(
            "DELETE FROM log WHERE ts >= ?1 AND ts < ?2",
            params![from, from + LOG_STEP_MS],
        )?;
        insert_log(tx, &[one])?;
        removed += recs.len() - 1;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn day(y: i32, m: u32, d: u32, max: f64) -> DayRecord {
        DayRecord {
            date: NaiveDate::from_ymd_opt(y, m, d).unwrap(),
            temp_max: Some(max),
            ..Default::default()
        }
    }

    #[test]
    fn days_roundtrip_and_replace() {
        let mut s = Store::open_in_memory().unwrap();
        s.upsert_days(&[day(2024, 1, 2, 30.0), day(2024, 1, 1, 25.0)])
            .unwrap();
        s.upsert_days(&[day(2024, 1, 2, 31.5)]).unwrap();
        let all = s.days().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].date.to_string(), "2024-01-01");
        assert_eq!(all[1].temp_max, Some(31.5));
        assert_eq!(s.day_count().unwrap(), 2);
    }

    #[test]
    fn samples_group_and_prune() {
        let mut s = Store::open_in_memory().unwrap();
        s.insert_samples(1000, &[("temp", 10.0), ("hum", 50.0)])
            .unwrap();
        s.insert_samples(2000, &[("temp", 11.0)]).unwrap();
        s.insert_series("temp", &[(1000, 99.0), (3000, 12.0)])
            .unwrap();
        let got = s.samples_since(0).unwrap();
        assert_eq!(got["temp"], vec![(1000, 10.0), (2000, 11.0), (3000, 12.0)]);
        assert_eq!(s.prune_samples(2500).unwrap(), 3);
        assert_eq!(s.samples_since(0).unwrap()["temp"], vec![(3000, 12.0)]);
    }

    #[test]
    fn log_roundtrip() {
        let mut s = Store::open_in_memory().unwrap();
        let rec = |ts: i64, t: f64| {
            let mut values = [None; LOG_METRICS.len()];
            values[0] = Some(t);
            values[8] = Some(1013.25);
            LogRecord { ts, values }
        };
        s.upsert_log(&[rec(1000, 12.3), rec(2000, -4.56), rec(3000, 15.0)])
            .unwrap();
        s.upsert_log(&[rec(2000, -4.5)]).unwrap();
        let got = s
            .log_series(&["temp", "press", "nonsense"], 1500, 3000)
            .unwrap();
        assert_eq!(got["temp"], vec![(2000, -4.5), (3000, 15.0)]);
        assert_eq!(got["press"][0], (2000, 1013.25));
        assert_eq!(s.log_extent().unwrap(), (3, Some(1000), Some(3000)));
        assert_eq!(s.log_last_between(0, 2500).unwrap(), Some(2000));
    }

    #[test]
    fn log_keeps_one_reading_per_ten_minutes() {
        let mut s = Store::open_in_memory().unwrap();
        let min = 60_000;
        let rec = |k: i64| {
            let mut values = [None; LOG_METRICS.len()];
            values[0] = Some(k as f64);
            values[7] = Some(k as f64 / 10.0);
            LogRecord {
                ts: k * min,
                values,
            }
        };
        // A one-minute log for 25 minutes, in two pages: the last step is
        // still filling, so it stays as it is.
        let all: Vec<LogRecord> = (0..25).map(rec).collect();
        s.upsert_log(&all[..13]).unwrap();
        s.upsert_log(&all[13..]).unwrap();
        let got = s.log_series(&["temp", "rain_today"], 0, i64::MAX).unwrap();
        let mut want = vec![(9 * min, 4.5), (19 * min, 14.5)];
        want.extend((20..25).map(|k| (k * min, k as f64)));
        assert_eq!(got["temp"], want);
        assert_eq!(got["rain_today"][0], (9 * min, 0.9), "latest total");
        // Fetching the same day again gives the same result.
        s.upsert_log(&all).unwrap();
        assert_eq!(s.log_series(&["temp"], 0, i64::MAX).unwrap()["temp"], want);
        // Thinning everything leaves only the newest, unfinished step.
        assert_eq!(s.thin_all_log().unwrap(), 0);
        s.upsert_log(&[rec(31)]).unwrap();
        assert_eq!(s.log_extent().unwrap().0, 4);
    }

    #[test]
    fn thins_an_old_one_minute_log() {
        let mut s = Store::open_in_memory().unwrap();
        // As an earlier Isobar stored it: every line, unthinned.
        let tx = s.conn.transaction().unwrap();
        let recs: Vec<LogRecord> = (0..100_000)
            .map(|k| {
                let mut values = [None; LOG_METRICS.len()];
                values[0] = Some(20.0);
                LogRecord {
                    ts: k * 60_000,
                    values,
                }
            })
            .collect();
        insert_log(&tx, &recs).unwrap();
        tx.commit().unwrap();
        // All but the newest step, which may still be filling.
        assert_eq!(s.thin_all_log().unwrap(), 9_999 * 9);
        assert_eq!(s.log_extent().unwrap().0, 9_999 + 10);
    }

    #[test]
    fn tank_log_replaces() {
        let mut s = Store::open_in_memory().unwrap();
        let d = |k: u32, rain: f64| LogDay {
            date: NaiveDate::from_ymd_opt(2026, 9, k).unwrap(),
            rain: Some(rain),
            used: None,
            adjust: None,
        };
        s.replace_tank_log(&[d(2, 1.0), d(1, 0.5)]).unwrap();
        assert_eq!(s.tank_log().unwrap(), [d(1, 0.5), d(2, 1.0)]);
        s.replace_tank_log(&[d(3, 2.0)]).unwrap();
        assert_eq!(s.tank_log().unwrap(), [d(3, 2.0)]);
    }

    #[test]
    fn meta() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(s.meta_get("x").unwrap(), None);
        s.meta_set("x", "1").unwrap();
        s.meta_set("x", "2").unwrap();
        assert_eq!(s.meta_get("x").unwrap().as_deref(), Some("2"));
    }
}
