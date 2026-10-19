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

/// A card under the thermograph on Now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NowCard {
    Wind,
    Pressure,
    Rain,
    Humidity,
    Sun,
    Indoor,
    Tanks,
}

impl NowCard {
    /// The usual order.
    pub const ALL: [NowCard; 7] = [
        NowCard::Wind,
        NowCard::Pressure,
        NowCard::Rain,
        NowCard::Humidity,
        NowCard::Sun,
        NowCard::Indoor,
        NowCard::Tanks,
    ];

    pub fn label(self) -> &'static str {
        match self {
            NowCard::Wind => "Wind",
            NowCard::Pressure => "Pressure",
            NowCard::Rain => "Rain",
            NowCard::Humidity => "Humidity",
            NowCard::Sun => "Sun",
            NowCard::Indoor => "Indoor",
            NowCard::Tanks => "Tanks",
        }
    }

    /// The name kept in settings.json.
    pub fn id(self) -> &'static str {
        match self {
            NowCard::Wind => "wind",
            NowCard::Pressure => "pressure",
            NowCard::Rain => "rain",
            NowCard::Humidity => "humidity",
            NowCard::Sun => "sun",
            NowCard::Indoor => "indoor",
            NowCard::Tanks => "tanks",
        }
    }

    pub fn from_id(id: &str) -> Option<NowCard> {
        NowCard::ALL.into_iter().find(|c| c.id() == id)
    }
}

/// A card's place on Now and whether it shows. Kept by name, so a settings
/// file from a later version with cards this one doesn't know still loads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CardChoice {
    pub card: String,
    pub shown: bool,
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
    /// The station's latitude in degrees, south negative. Without it, Isobar
    /// works it out from the day's sunrise and sunset.
    pub latitude: Option<f64>,
    /// Rainwater tanks, once set up.
    pub tanks: Option<isobar_core::tanks::Tanks>,
    /// The cards on Now, in order. Empty means the usual order, all shown.
    pub now_cards: Vec<CardChoice>,
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
            latitude: None,
            tanks: None,
            now_cards: Vec::new(),
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

    /// Every card in its order on Now, and whether it shows. Cards missing
    /// from the settings (new in this version) go at the end, shown.
    pub fn cards(&self) -> Vec<(NowCard, bool)> {
        let mut out: Vec<(NowCard, bool)> = Vec::new();
        for c in &self.now_cards {
            if let Some(card) = NowCard::from_id(&c.card) {
                if !out.iter().any(|(k, _)| *k == card) {
                    out.push((card, c.shown));
                }
            }
        }
        for card in NowCard::ALL {
            if !out.iter().any(|(k, _)| *k == card) {
                out.push((card, true));
            }
        }
        out
    }

    pub fn set_cards(&mut self, cards: &[(NowCard, bool)]) {
        self.now_cards = cards
            .iter()
            .map(|(c, shown)| CardChoice {
                card: c.id().to_string(),
                shown: *shown,
            })
            .collect();
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
    fn card_order_survives_unknown_and_missing_cards() {
        let s: Settings = serde_json::from_str(
            r#"{"nowCards":[{"card":"rain","shown":true},{"card":"radar","shown":true},
            {"card":"wind","shown":false},{"card":"rain","shown":false}]}"#,
        )
        .unwrap();
        let cards = s.cards();
        assert_eq!(cards.len(), NowCard::ALL.len());
        assert_eq!(cards[0], (NowCard::Rain, true));
        assert_eq!(cards[1], (NowCard::Wind, false));
        assert_eq!(cards[2], (NowCard::Pressure, true));
        assert_eq!(Settings::default().cards()[0], (NowCard::Wind, true));
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
