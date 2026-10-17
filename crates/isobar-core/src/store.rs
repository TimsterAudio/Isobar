//! Local SQLite cache.
//!
//! * `days`: one row per meteorological day, stored as JSON so new dayfile
//!   fields never need a migration.
//! * `samples`: live readings (one per minute) plus anything backfilled from
//!   MX graph data, in long format `(metric, ts, value)`.
//! * `meta`: small key/value pairs such as the last sync time.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use crate::dayfile::DayRecord;
use crate::error::{CoreError, Result};

pub struct Store {
    conn: Connection,
}

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
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
                let json = serde_json::to_string(d)
                    .map_err(|e| CoreError::Parse(e.to_string()))?;
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
            .query_row("SELECT COUNT(*) FROM days", [], |r| r.get::<_, i64>(0))? as usize)
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
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO samples (metric, ts, value) VALUES (?1, ?2, ?3)",
            )?;
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
        let mut stmt = self.conn.prepare(
            "SELECT metric, ts, value FROM samples WHERE ts >= ?1 ORDER BY metric, ts",
        )?;
        let rows = stmt.query_map(params![since_ms], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, f64>(2)?))
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

    pub fn meta_get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| r.get(0))
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
        s.upsert_days(&[day(2024, 1, 2, 30.0), day(2024, 1, 1, 25.0)]).unwrap();
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
        s.insert_samples(1000, &[("temp", 10.0), ("hum", 50.0)]).unwrap();
        s.insert_samples(2000, &[("temp", 11.0)]).unwrap();
        s.insert_series("temp", &[(1000, 99.0), (3000, 12.0)]).unwrap();
        let got = s.samples_since(0).unwrap();
        assert_eq!(got["temp"], vec![(1000, 10.0), (2000, 11.0), (3000, 12.0)]);
        assert_eq!(s.prune_samples(2500).unwrap(), 3);
        assert_eq!(s.samples_since(0).unwrap()["temp"], vec![(3000, 12.0)]);
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
