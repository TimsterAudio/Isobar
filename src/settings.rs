use std::path::{Path, PathBuf};

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark];

    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::System => "Match system",
            ThemeChoice::Light => "Light",
            ThemeChoice::Dark => "Dark",
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
    /// Optional path to a `dayfile.txt`, used when no station address is set.
    pub dayfile_path: Option<String>,
    pub poll_seconds: u64,
    pub theme: ThemeChoice,
    /// 24-hour times ("16:52") rather than 12-hour ("4:52 PM").
    pub clock_24h: bool,
    /// The side rail stays open; otherwise it slides out from the left edge.
    pub rail_pinned: bool,
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
            theme: ThemeChoice::System,
            clock_24h: true,
            rail_pinned: true,
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

    pub fn dayfile(&self) -> Option<&str> {
        self.dayfile_path
            .as_deref()
            .filter(|p| !p.trim().is_empty())
    }
}

/// Where Isobar keeps its files. The same folders the first (Tauri) version
/// used, so settings and history carry over.
pub struct Paths {
    pub settings: PathBuf,
    pub database: PathBuf,
}

impl Paths {
    pub fn locate() -> Paths {
        const ID: &str = "app.isobar.desktop";
        let config = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(ID);
        let data = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(ID);
        Paths {
            settings: config.join("settings.json"),
            database: data.join("isobar.sqlite"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_settings_from_the_first_version() {
        let json = r#"{"stationName":"Home","stationUrl":"http://pi:8998","username":null,
            "password":null,"dayfilePath":null,"pollSeconds":15,"theme":"dark",
            "alerts":{"enabled":true,"gustAbove":70,"frostBelow":null,"heatAbove":38,"rainStart":false}}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.station_name, "Home");
        assert_eq!(s.theme, ThemeChoice::Dark);
        assert_eq!(s.alerts.gust_above, Some(70.0));
        assert_eq!(s.alerts.frost_below, None);
        assert!(s.has_station());
    }

    #[test]
    fn missing_fields_take_defaults() {
        let s: Settings = serde_json::from_str(r#"{"stationUrl":"x"}"#).unwrap();
        assert_eq!(s.poll_seconds, 10);
        assert!(s.clock_24h);
        assert!(s.rail_pinned);
        assert_eq!(s.theme, ThemeChoice::System);
    }
}
