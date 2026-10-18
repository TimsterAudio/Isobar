//! HTTP client for a Cumulus MX instance (the admin interface port, 8998 by
//! default). Calls block, so run them off the UI thread.
//!
//! Endpoints used:
//! * `/api/data/currentdata` for live conditions.
//! * `/api/data/dayfile?draw=1&start=N&length=M` for the full daily history.
//! * `/api/graphdata/*.json` (best effort) to backfill the last day or so of
//!   detailed readings when Isobar first connects.

use std::time::Duration;

use serde_json::Value;

use crate::dayfile::{self, DayRecord};
use crate::error::{CoreError, Result};
use crate::live::LiveReading;

const PAGE: usize = 1000;

#[derive(Clone)]
pub struct MxClient {
    http: reqwest::blocking::Client,
    base: String,
    auth: Option<(String, String)>,
}

/// Normalise what a person types into a base URL: add a scheme, drop a
/// trailing slash, and strip a pasted path such as `/now.html`.
pub fn normalise_base(input: &str) -> String {
    let mut s = input.trim().to_string();
    if !s.contains("://") {
        s = format!("http://{s}");
    }
    if let Some(scheme_end) = s.find("://") {
        let rest = &s[scheme_end + 3..];
        if let Some(slash) = rest.find('/') {
            s.truncate(scheme_end + 3 + slash);
        }
    }
    s.trim_end_matches('/').to_string()
}

impl MxClient {
    pub fn new(base: &str, username: Option<String>, password: Option<String>) -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .user_agent(concat!("Isobar/", env!("CARGO_PKG_VERSION")))
            .build()?;
        let auth = match username {
            Some(u) if !u.trim().is_empty() => Some((u, password.unwrap_or_default())),
            _ => None,
        };
        Ok(MxClient {
            http,
            base: normalise_base(base),
            auth,
        })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    fn get_json(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.base, path);
        let mut req = self.http.get(&url);
        if let Some((u, p)) = &self.auth {
            req = req.basic_auth(u, Some(p));
        }
        let resp = req.send()?;
        let status = resp.status();
        if !status.is_success() {
            return Err(CoreError::Status(status.as_u16(), path.to_string()));
        }
        let body = resp.text()?;
        serde_json::from_str(&body)
            .map_err(|e| CoreError::Parse(format!("{path} did not return JSON ({e})")))
    }

    /// Current conditions.
    pub fn current(&self) -> Result<LiveReading> {
        let v = self.get_json("/api/data/currentdata")?;
        let now = chrono::Utc::now().timestamp_millis();
        let r = LiveReading::from_json(&v, now);
        if r.looks_valid() {
            Ok(r)
        } else {
            Err(CoreError::Parse(
                "currentdata had no temperature or pressure".into(),
            ))
        }
    }

    fn dayfile_page(&self, start: usize, length: usize) -> Result<(usize, Vec<Value>)> {
        let v = self.get_json(&format!(
            "/api/data/dayfile?draw=1&start={start}&length={length}"
        ))?;
        let total = v
            .get("recordsTotal")
            .and_then(Value::as_u64)
            .ok_or_else(|| CoreError::Parse("dayfile response had no recordsTotal".into()))?
            as usize;
        let rows = v
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok((total, rows))
    }

    /// Number of lines in the station's dayfile.
    pub fn dayfile_total(&self) -> Result<usize> {
        Ok(self.dayfile_page(0, 1)?.0)
    }

    /// Every daily record. `progress(done, total)` is called after each page.
    pub fn dayfile_all<F: FnMut(usize, usize)>(&self, mut progress: F) -> Result<Vec<DayRecord>> {
        let mut out = Vec::new();
        let mut start = 0;
        loop {
            let (total, rows) = self.dayfile_page(start, PAGE)?;
            if rows.is_empty() {
                break;
            }
            start += rows.len();
            out.extend(rows.iter().filter_map(dayfile::from_api_row));
            progress(start.min(total), total);
            if start >= total {
                break;
            }
        }
        Ok(out)
    }

    /// Best-effort backfill of recent detailed readings from the graph-data
    /// endpoints (MX keeps up to a few days, a reading a minute). Files that
    /// are missing or disabled are skipped and listed in the report.
    pub fn recent_series(&self) -> Backfill {
        const FILES: [&str; 7] = [
            "tempdata",
            "pressdata",
            "winddata",
            "wdirdata",
            "raindata",
            "humdata",
            "solardata",
        ];
        let mut out = Backfill::default();
        for file in FILES {
            match self.get_json(&format!("/api/graphdata/{file}.json")) {
                Ok(v) => out.series.extend(parse_graph_series(&v)),
                Err(e) => out.failed.push((file.to_string(), e.to_string())),
            }
        }
        out
    }
}

/// What a graph-data backfill brought back.
#[derive(Debug, Clone, Default)]
pub struct Backfill {
    /// `(metric, points)` with Unix-ms timestamps.
    pub series: Vec<(String, Vec<(i64, f64)>)>,
    /// Files that couldn't be read, with the reason.
    pub failed: Vec<(String, String)>,
}

impl Backfill {
    pub fn points(&self) -> usize {
        self.series.iter().map(|s| s.1.len()).sum()
    }
}

/// The series in one graph-data file: `{"temp": [[ms, value], ...], ...}`.
/// Series Isobar doesn't keep are skipped.
pub fn parse_graph_series(v: &Value) -> Vec<(String, Vec<(i64, f64)>)> {
    let Some(obj) = v.as_object() else {
        return Vec::new();
    };
    obj.iter()
        .filter_map(|(key, series)| {
            let metric = metric_for_graph_key(key)?;
            let pts: Vec<(i64, f64)> = series
                .as_array()?
                .iter()
                .filter_map(|p| {
                    let p = p.as_array()?;
                    Some((p.first()?.as_f64()? as i64, p.get(1)?.as_f64()?))
                })
                .collect();
            (!pts.is_empty()).then(|| (metric.to_string(), pts))
        })
        .collect()
}

/// Map MX graph-data series names onto Isobar's sample metric names.
pub fn metric_for_graph_key(key: &str) -> Option<&'static str> {
    Some(match key.to_ascii_lowercase().as_str() {
        "temp" => "temp",
        "dew" => "dew",
        "feelslike" => "feels",
        "hum" => "hum",
        "press" => "press",
        "wspeed" => "wind",
        "wgust" => "gust",
        "bearing" => "bearing",
        "rfall" => "rain_today",
        "rrate" => "rain_rate",
        "solarrad" => "solar",
        "uv" => "uv",
        "intemp" => "intemp",
        "inhum" => "inhum",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_urls() {
        assert_eq!(
            normalise_base("192.168.1.20:8998"),
            "http://192.168.1.20:8998"
        );
        assert_eq!(
            normalise_base("http://pi.local:8998/"),
            "http://pi.local:8998"
        );
        assert_eq!(
            normalise_base(" http://pi:8998/now.html "),
            "http://pi:8998"
        );
        assert_eq!(
            normalise_base("https://wx.example.com"),
            "https://wx.example.com"
        );
    }

    /// The start of a real `tempdata.json` from the owner's MX 5.1.5: one
    /// reading a minute, Unix-ms timestamps in UTC, 72 hours deep.
    #[test]
    fn parses_real_graph_data() {
        let v: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/graphdata-tempdata-mx-5.1.5.json"
        ))
        .unwrap();
        let s = parse_graph_series(&v);
        let (metric, pts) = s.iter().find(|s| s.0 == "intemp").unwrap();
        assert_eq!(metric, "intemp");
        assert_eq!(pts.len(), 8);
        assert_eq!(pts[0], (1_792_047_420_000, 21.2));
        assert_eq!(pts[1].0 - pts[0].0, 60_000);
        assert_eq!(pts[7], (1_792_047_840_000, 21.2));
    }

    #[test]
    fn graph_keys() {
        assert_eq!(metric_for_graph_key("SolarRad"), Some("solar"));
        assert_eq!(metric_for_graph_key("intemp"), Some("intemp"));
        assert_eq!(metric_for_graph_key("heatindex"), None);
    }

    #[test]
    fn unreachable_station_is_a_clean_error() {
        let c = MxClient::new("http://127.0.0.1:9", None, None).unwrap();
        let err = c.current().unwrap_err();
        assert!(matches!(err, CoreError::Http(_)), "{err:?}");
    }
}
