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

    /// Rain yesterday, in the last 7 days and in the last 24 hours.
    pub rain_yesterday: Option<f64>,
    pub rain_week: Option<f64>,
    pub rain_24h: Option<f64>,
    /// Rain in the current storm, and when it started (station text).
    pub storm_rain: Option<f64>,
    pub storm_start: Option<String>,
    /// When the rain gauge last tipped, "2026-10-12 02:02".
    pub last_rain_tip: Option<String>,

    /// Station times of day, as sent ("6:36 AM").
    pub sunrise: Option<String>,
    pub sunset: Option<String>,
    pub moonrise: Option<String>,
    pub moonset: Option<String>,
    /// Hours of sunshine so far today.
    pub sunshine_hours: Option<f64>,
    /// The clear-sky solar radiation expected right now.
    pub solar_max_now: Option<f64>,

    pub indoor_temp: Option<f64>,
    pub indoor_hum: Option<f64>,

    /// MX has stopped getting data from the station, though it still answers.
    pub data_stopped: Option<bool>,
    /// MX's alarms and whether each is set off.
    pub alarms: Vec<Alarm>,
    /// MX's version ("5.1.5") and build ("5012").
    pub mx_version: Option<String>,
    pub mx_build: Option<String>,
    /// The station's date of the last reading, as sent ("18/10/2026").
    pub station_date: Option<String>,

    pub units: Units,
}

/// One of MX's alarms, as `currentdata` lists them.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Alarm {
    /// MX's name for it ("AlarmSensor").
    pub id: String,
    pub triggered: bool,
}

impl Alarm {
    /// The alarm in plain words.
    pub fn label(&self) -> String {
        let known = match self.id.as_str() {
            "AlarmRecord" | "AlarmNewRecord" => "New record set",
            "AlarmDataStopped" => "Data stopped",
            "AlarmSensor" => "Sensor contact lost",
            "AlarmSpike" => "Spike removed from the data",
            "AlarmUpgrade" => "Cumulus MX update available",
            "AlarmFirmware" => "Station firmware update available",
            "AlarmBattery" => "Station battery low",
            "AlarmLowTemp" => "Low temperature",
            "AlarmHighTemp" => "High temperature",
            "AlarmTempUp" => "Temperature rising fast",
            "AlarmTempDn" => "Temperature falling fast",
            "AlarmLowPress" => "Low pressure",
            "AlarmHighPress" => "High pressure",
            "AlarmPressUp" => "Pressure rising fast",
            "AlarmPressDn" => "Pressure falling fast",
            "AlarmHighRainToday" => "High rain today",
            "AlarmHighRainRate" => "High rain rate",
            "AlarmHighGust" => "High gust",
            "AlarmHighWind" => "High wind",
            "AlarmHttp" => "Web upload failing",
            "AlarmMySql" => "Database upload failing",
            "AlarmFtp" => "File upload failing",
            _ => "",
        };
        if !known.is_empty() {
            return known.into();
        }
        // "AlarmSomethingNew" becomes "Something new".
        let name = self.id.strip_prefix("Alarm").unwrap_or(&self.id);
        let mut out = String::new();
        for (i, c) in name.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                out.push(' ');
                out.extend(c.to_lowercase());
            } else {
                out.push(c);
            }
        }
        out
    }
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
    (!s.is_empty() && !s.chars().all(|c| c == '-')).then_some(s)
}

fn flag(v: &Value, keys: &[&str]) -> Option<bool> {
    match find(v, keys)? {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => n.as_f64().map(|x| x != 0.0),
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn alarms(v: &Value) -> Vec<Alarm> {
    let Some(list) = find(v, &["Alarms", "alarms"]).and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|a| {
            Some(Alarm {
                id: text(a, &["id", "Id"])?,
                triggered: flag(a, &["triggered", "Triggered"]).unwrap_or(false),
            })
        })
        .collect()
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

            rain_yesterday: num(v, &["RainYesterday", "ryest"]),
            rain_week: num(v, &["RainWeek", "rweek"]),
            rain_24h: num(v, &["RainLast24Hour", "r24hour"]),
            storm_rain: num(v, &["StormRain", "stormrain"]),
            storm_start: text(v, &["StormRainStart", "stormrainstart"]),
            last_rain_tip: text(v, &["LastRainTipISO", "LastRainTip"]),

            sunrise: text(v, &["Sunrise", "sunrise"]),
            sunset: text(v, &["Sunset", "sunset"]),
            moonrise: text(v, &["Moonrise", "moonrise"]),
            moonset: text(v, &["Moonset", "moonset"]),
            sunshine_hours: num(v, &["SunshineHours", "SunshineHoursToday", "sunshinehours"]),
            solar_max_now: num(v, &["CurrentSolarMax", "CurrentSolarMaximum"]),

            indoor_temp: num(v, &["IndoorTemp", "intemp"]),
            indoor_hum: num(v, &["IndoorHum", "inhum"]),

            data_stopped: flag(v, &["DataStopped", "DataStoppedAlarm"]),
            alarms: alarms(v),
            mx_version: text(v, &["Version", "version"]),
            mx_build: text(v, &["Build", "build"]),
            station_date: text(v, &["LastDataReadDate"]),
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
            ("intemp", self.indoor_temp),
            ("inhum", self.indoor_hum),
        ]
        .into_iter()
        .filter_map(|(k, v)| v.map(|x| (k, x)))
        .collect()
    }

    /// The bearing to show, and whether the wind is light. In light air the
    /// latest bearing swings about (or sits wherever the vane stopped), so
    /// the 10-minute average reads truer. "Light" is below about 3 km/h, in
    /// the station's own wind units.
    pub fn shown_bearing(&self) -> Option<(f64, bool)> {
        let light_below = match self.units.wind.trim().to_ascii_lowercase().as_str() {
            "m/s" => 1.0,
            "mph" | "kts" | "kt" | "knots" => 2.0,
            _ => 3.0,
        };
        let light = self
            .wind_avg
            .or(self.wind_latest)
            .is_some_and(|s| s < light_below);
        match (light, self.bearing_avg, self.bearing) {
            (true, Some(avg), _) => Some((avg, true)),
            (_, _, Some(b)) => Some((b, light)),
            (_, avg, None) => avg.map(|a| (a, light)),
        }
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
        assert_eq!(r.rain_yesterday, Some(0.0));
        assert_eq!(r.rain_week, Some(0.3));
        assert_eq!(r.rain_24h, Some(0.0));
        assert_eq!(r.storm_rain, Some(0.0));
        assert_eq!(r.storm_start, None, "\"-----\" means no storm");
        assert_eq!(r.last_rain_tip.as_deref(), Some("2026-10-12 02:02"));
        assert_eq!(r.sunrise.as_deref(), Some("6:36 AM"));
        assert_eq!(r.sunset.as_deref(), Some("7:45 PM"));
        assert_eq!(r.moonrise.as_deref(), Some("11:30 AM"));
        assert_eq!(r.moonset.as_deref(), Some("2:08 AM"));
        assert_eq!(r.sunshine_hours, Some(0.0));
        assert_eq!(r.solar_max_now, Some(83.0));
        assert_eq!(r.indoor_temp, Some(17.9));
        assert_eq!(r.indoor_hum, Some(51.0));
        assert_eq!(r.data_stopped, Some(false));
        assert_eq!(r.mx_version.as_deref(), Some("5.1.5"));
        assert_eq!(r.mx_build.as_deref(), Some("5012"));
        assert_eq!(r.station_date.as_deref(), Some("18/10/2026"));
        assert_eq!(r.alarms.len(), 6);
        let set: Vec<String> = r
            .alarms
            .iter()
            .filter(|a| a.triggered)
            .map(Alarm::label)
            .collect();
        assert_eq!(
            set,
            [
                "Cumulus MX update available",
                "Station firmware update available"
            ]
        );
        assert_eq!(
            Alarm {
                id: "AlarmIsNew".into(),
                triggered: true
            }
            .label(),
            "Is new"
        );
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
    fn light_air_uses_the_average_bearing() {
        let v: Value =
            serde_json::from_str(include_str!("../tests/fixtures/currentdata-mx-5.1.5.json"))
                .unwrap();
        // Calm on the owner's station: latest bearing 89°, average 221°.
        let mut r = LiveReading::from_json(&v, 1);
        assert_eq!(r.shown_bearing(), Some((221.0, true)));
        r.wind_avg = Some(15.0);
        assert_eq!(r.shown_bearing(), Some((89.0, false)));
        r.units.wind = "m/s".into();
        r.wind_avg = Some(2.0);
        assert_eq!(r.shown_bearing(), Some((89.0, false)));
        r.bearing = None;
        assert_eq!(r.shown_bearing(), Some((221.0, false)));
    }

    #[test]
    fn empty_object_is_not_valid() {
        assert!(!LiveReading::from_json(&json!({}), 0).looks_valid());
    }
}
