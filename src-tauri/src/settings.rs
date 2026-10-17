use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Alerts {
    pub enabled: bool,
    /// Notify when a gust reaches this speed (station wind units).
    pub gust_above: Option<f64>,
    /// Notify once a day when the temperature drops to this value.
    pub frost_below: Option<f64>,
    /// Notify once a day when the temperature reaches this value.
    pub heat_above: Option<f64>,
    /// Notify when rain starts after a dry spell of at least three hours.
    pub rain_start: bool,
}

impl Default for Alerts {
    fn default() -> Self {
        Alerts {
            enabled: true,
            gust_above: Some(60.0),
            frost_below: Some(2.0),
            heat_above: Some(35.0),
            rain_start: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub station_name: String,
    /// Base address of the MX interface, e.g. `http://192.168.1.20:8998`.
    pub station_url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    /// Optional path to a `dayfile.txt`, used when no station address is set
    /// (or to import history from a copy).
    pub dayfile_path: Option<String>,
    pub poll_seconds: u64,
    /// "system", "light" or "dark".
    pub theme: String,
    pub alerts: Alerts,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            station_name: "My station".into(),
            station_url: String::new(),
            username: None,
            password: None,
            dayfile_path: None,
            poll_seconds: 10,
            theme: "system".into(),
            alerts: Alerts::default(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Settings {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    pub fn has_station(&self) -> bool {
        !self.station_url.trim().is_empty()
    }
}
