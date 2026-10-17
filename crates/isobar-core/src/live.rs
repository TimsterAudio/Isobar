//! Mapping of the MX `/api/data/currentdata` JSON onto a typed reading.
//!
//! MX has renamed a few keys over the years and some people serve their own
//! JSON built from web tags, so every field is looked up from a list of
//! candidate keys, case-insensitively, and values may be numbers or strings.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Units {
    pub temp: String,
    pub wind: String,
    pub press: String,
    pub rain: String,
}

impl Default for Units {
    fn default() -> Self {
        Units {
            temp: "°C".into(),
            wind: "km/h".into(),
            press: "hPa".into(),
            rain: "mm".into(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LiveReading {
    /// When Isobar received this reading (Unix ms).
    pub fetched_at: i64,
    /// Station's own date/time string, if it sent one.
    pub station_time: Option<String>,

    pub temp: Option<f64>,
    pub temp_high: Option<f64>,
    pub temp_high_time: Option<String>,
    pub temp_low: Option<f64>,
    pub temp_low_time: Option<String>,
    /// Degrees per hour.
    pub temp_trend: Option<f64>,
    pub feels_like: Option<f64>,
    pub apparent: Option<f64>,
    pub wind_chill: Option<f64>,
    pub dew_point: Option<f64>,

    pub humidity: Option<f64>,
    pub hum_high: Option<f64>,
    pub hum_low: Option<f64>,

    pub pressure: Option<f64>,
    pub press_high: Option<f64>,
    pub press_low: Option<f64>,
    /// Pressure units per hour, when the station reports it numerically.
    pub press_trend: Option<f64>,
    /// Pressure trend as text, when the station reports it that way.
    pub press_trend_text: Option<String>,

    pub wind_avg: Option<f64>,
    pub wind_latest: Option<f64>,
    pub gust_recent: Option<f64>,
    pub gust_high: Option<f64>,
    pub gust_high_time: Option<String>,
    pub bearing: Option<f64>,
    pub bearing_avg: Option<f64>,
    pub bearing_compass: Option<String>,

    pub rain_today: Option<f64>,
    pub rain_rate: Option<f64>,
    pub rain_rate_high: Option<f64>,
    pub rain_last_hour: Option<f64>,
    pub rain_month: Option<f64>,
    pub rain_year: Option<f64>,

    pub solar: Option<f64>,
    pub solar_high: Option<f64>,
    pub uv: Option<f64>,
    pub uv_high: Option<f64>,

    pub forecast: Option<String>,
    pub units: Units,
}

fn find<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    let obj = v.as_object()?;
    for k in keys {
        if let Some(x) = obj.get(*k) {
            if !x.is_null() {
                return Some(x);
            }
        }
    }
    // Fall back to a case-insensitive match.
    for k in keys {
        let lk = k.to_ascii_lowercase();
        if let Some((_, x)) = obj
            .iter()
            .find(|(name, x)| name.to_ascii_lowercase() == lk && !x.is_null())
        {
            return Some(x);
        }
    }
    None
}

fn number_from(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            // Keep the leading numeric part: "1013.2 hPa" or "-0,4" both work.
            let cleaned: String = s
                .trim()
                .replace(',', ".")
                .chars()
                .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+'))
                .collect();
            cleaned.parse::<f64>().ok()
        }
        _ => None,
    }
    .filter(|x| x.is_finite())
}

fn num(v: &Value, keys: &[&str]) -> Option<f64> {
    find(v, keys).and_then(number_from)
}

fn text(v: &Value, keys: &[&str]) -> Option<String> {
    let s = match find(v, keys)? {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    (!s.is_empty() && s != "-").then_some(s)
}

fn temp_unit(raw: Option<String>) -> String {
    match raw.as_deref().map(str::trim) {
        Some(u) if u.eq_ignore_ascii_case("c") => "°C".into(),
        Some(u) if u.eq_ignore_ascii_case("f") => "°F".into(),
        Some(u) if !u.is_empty() => u.replace("&deg;", "°"),
        _ => "°C".into(),
    }
}

impl LiveReading {
    pub fn from_json(v: &Value, fetched_at: i64) -> LiveReading {
        let defaults = Units::default();
        let press_trend_raw = find(v, &["PressTrend", "presstrendval", "presstrend"]);
        let (press_trend, press_trend_text) = match press_trend_raw {
            Some(x) => match number_from(x) {
                Some(n) => (Some(n), None),
                None => (None, x.as_str().map(|s| s.to_string())),
            },
            None => (None, None),
        };

        LiveReading {
            fetched_at,
            station_time: text(
                v,
                &[
                    "LastDataRead",
                    "LastDataReadT",
                    "Date",
                    "timehhmmss",
                    "date",
                ],
            ),

            temp: num(v, &["OutdoorTemp", "temp"]),
            temp_high: num(v, &["HighTempToday", "tempTH", "tempH"]),
            temp_high_time: text(v, &["HighTempTodayTime", "TtempTH", "tempHT"]),
            temp_low: num(v, &["LowTempToday", "tempTL", "tempL"]),
            temp_low_time: text(v, &["LowTempTodayTime", "TtempTL", "tempLT"]),
            temp_trend: num(v, &["TempTrend", "temptrend"]),
            feels_like: num(v, &["FeelsLike", "feelslike"]),
            apparent: num(v, &["AppTemp", "apptemp"]),
            wind_chill: num(v, &["WindChill", "wchill"]),
            dew_point: num(v, &["OutdoorDewpoint", "dew"]),

            humidity: num(v, &["OutdoorHum", "hum"]),
            hum_high: num(v, &["HighHumToday", "humTH"]),
            hum_low: num(v, &["LowHumToday", "humTL"]),

            pressure: num(v, &["Pressure", "press"]),
            press_high: num(v, &["HighPressToday", "pressTH", "pressH"]),
            press_low: num(v, &["LowPressToday", "pressTL", "pressL"]),
            press_trend,
            press_trend_text,

            wind_avg: num(v, &["WindAverage", "wspeed", "wspd"]),
            wind_latest: num(v, &["WindLatest", "wlatest"]),
            gust_recent: num(v, &["Recentmaxgust", "wgust"]),
            gust_high: num(v, &["HighGustToday", "wgustTM", "wgustMax"]),
            gust_high_time: text(v, &["HighGustTodayTime", "TwgustTM", "gustTime"]),
            bearing: num(v, &["Bearing", "bearing"]),
            bearing_avg: num(v, &["Avgbearing", "avgbearing"]),
            bearing_compass: text(v, &["BearingCP", "wdir"]),

            rain_today: num(v, &["RainToday", "rfall"]),
            rain_rate: num(v, &["RainRate", "rrate"]),
            rain_rate_high: num(v, &["HighRainRateToday", "rrateTM"]),
            rain_last_hour: num(v, &["RainLastHour", "rhour"]),
            rain_month: num(v, &["RainMonth", "rmonth"]),
            rain_year: num(v, &["RainYear", "ryear"]),

            solar: num(v, &["SolarRad", "SolarRadiation", "SolarRad"]),
            solar_high: num(v, &["HighSolarRadToday", "SolarTM"]),
            uv: num(v, &["UVindex", "UV"]),
            uv_high: num(v, &["HighUVindexToday", "UVTH"]),

            forecast: text(v, &["Forecast", "forecast"]),
            units: Units {
                temp: temp_unit(text(v, &["TempUnit", "tempunit"])),
                wind: text(v, &["WindUnit", "windunit"]).unwrap_or(defaults.wind),
                press: text(v, &["PressUnit", "pressunit"]).unwrap_or(defaults.press),
                rain: text(v, &["RainUnit", "rainunit"]).unwrap_or(defaults.rain),
            },
        }
    }

    /// Metric/value pairs for the local sample history.
    pub fn sample_metrics(&self) -> Vec<(&'static str, f64)> {
        [
            ("temp", self.temp),
            ("dew", self.dew_point),
            ("feels", self.feels_like),
            ("hum", self.humidity),
            ("press", self.pressure),
            ("wind", self.wind_avg),
            ("gust", self.gust_recent),
            ("bearing", self.bearing),
            ("rain_today", self.rain_today),
            ("rain_rate", self.rain_rate),
            ("solar", self.solar),
            ("uv", self.uv),
        ]
        .into_iter()
        .filter_map(|(k, v)| v.map(|x| (k, x)))
        .collect()
    }

    /// True if the reading carries at least a temperature or a pressure.
    pub fn looks_valid(&self) -> bool {
        self.temp.is_some() || self.pressure.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_mx_keys() {
        let v = json!({
            "OutdoorTemp": 18.4, "HighTempToday": 21.2, "HighTempTodayTime": "14:05",
            "LowTempToday": 9.8, "TempTrend": "0.6", "OutdoorHum": 62, "Pressure": 1016.3,
            "PressTrend": -0.4, "WindAverage": 12.1, "Recentmaxgust": 27.4, "Bearing": 205,
            "BearingCP": "SSW", "RainToday": "1,2", "RainRate": 0, "TempUnit": "&deg;C",
            "WindUnit": "km/h", "PressUnit": "hPa", "RainUnit": "mm", "SolarRad": 512, "UVindex": 4.1
        });
        let r = LiveReading::from_json(&v, 1);
        assert_eq!(r.temp, Some(18.4));
        assert_eq!(r.temp_trend, Some(0.6));
        assert_eq!(r.press_trend, Some(-0.4));
        assert_eq!(r.rain_today, Some(1.2));
        assert_eq!(r.bearing_compass.as_deref(), Some("SSW"));
        assert_eq!(r.units.temp, "°C");
        assert!(r.looks_valid());
        assert!(r
            .sample_metrics()
            .iter()
            .any(|(k, v)| *k == "solar" && *v == 512.0));
    }

    #[test]
    fn maps_webtag_style_keys_and_text_trend() {
        let v = json!({"temp": "17.0", "press": "1009.1 hPa", "presstrend": "Falling slowly", "rfall": "0.0"});
        let r = LiveReading::from_json(&v, 1);
        assert_eq!(r.temp, Some(17.0));
        assert_eq!(r.pressure, Some(1009.1));
        assert_eq!(r.press_trend, None);
        assert_eq!(r.press_trend_text.as_deref(), Some("Falling slowly"));
        assert_eq!(r.units, Units::default());
    }

    /// A real `currentdata` response from Cumulus MX 5.1.5 (build 5012), on
    /// the owner's station on 18 October 2026: a calm, cool morning.
    #[test]
    fn maps_real_mx_5_response() {
        let v: Value =
            serde_json::from_str(include_str!("../tests/fixtures/currentdata-mx-5.1.5.json"))
                .unwrap();
        let r = LiveReading::from_json(&v, 1);
        assert!(r.looks_valid());
        assert_eq!(r.station_time.as_deref(), Some("7:35:32 AM"));
        assert_eq!(r.temp, Some(9.9));
        assert_eq!(r.temp_high, Some(10.1));
        assert_eq!(r.temp_high_time.as_deref(), Some("12:00 AM"));
        assert_eq!(r.temp_low, Some(6.7));
        assert_eq!(r.temp_low_time.as_deref(), Some("4:52 AM"));
        assert_eq!(r.temp_trend, Some(0.8));
        assert_eq!(r.feels_like, Some(9.9));
        assert_eq!(r.apparent, Some(9.2));
        assert_eq!(r.wind_chill, Some(9.9));
        assert_eq!(r.dew_point, Some(7.0));
        assert_eq!(r.humidity, Some(82.0));
        assert_eq!((r.hum_low, r.hum_high), (Some(82.0), Some(97.0)));
        assert_eq!(r.pressure, Some(1033.8));
        assert_eq!((r.press_low, r.press_high), (Some(1032.7), Some(1034.1)));
        assert_eq!(r.press_trend, Some(0.3));
        assert_eq!(r.wind_avg, Some(0.0));
        assert_eq!(r.wind_latest, Some(0.0));
        assert_eq!(r.gust_recent, Some(2.0));
        assert_eq!(r.gust_high, Some(4.0));
        assert_eq!(r.gust_high_time.as_deref(), Some("5:42 AM"));
        assert_eq!(r.bearing, Some(89.0));
        assert_eq!(r.bearing_avg, Some(221.0));
        // MX 5 sends no compass point; the Now screen works it out from the bearing.
        assert_eq!(r.bearing_compass, None);
        assert_eq!(r.rain_today, Some(0.0));
        assert_eq!(r.rain_rate, Some(0.0));
        assert_eq!(r.rain_rate_high, Some(0.0));
        assert_eq!(r.rain_last_hour, Some(0.0));
        assert_eq!(r.rain_month, Some(43.6));
        assert_eq!(r.rain_year, Some(613.9));
        assert_eq!(r.solar, Some(30.0));
        assert_eq!(r.solar_high, Some(30.0));
        assert_eq!((r.uv, r.uv_high), (Some(0.0), Some(0.0)));
        assert_eq!(r.forecast.as_deref(), Some("Settled fine"));
        assert_eq!(
            r.units,
            Units {
                temp: "°C".into(),
                wind: "km/h".into(),
                press: "hPa".into(),
                rain: "mm".into()
            }
        );
    }

    #[test]
    fn empty_object_is_not_valid() {
        assert!(!LiveReading::from_json(&json!({}), 0).looks_valid());
    }
}
