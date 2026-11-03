//! The weather forecast for the station's location, from Open-Meteo
//! (open-meteo.com: free, no key, CC BY 4.0), and its place search to find
//! the location. Calls block, so run them off the UI thread.

use std::time::Duration;

use chrono::{DateTime, NaiveDate};
use serde_json::Value;

use crate::error::{CoreError, Result};
use crate::live::Units;

pub const FORECAST_URL: &str = "https://api.open-meteo.com/v1/forecast";
pub const SEARCH_URL: &str = "https://geocoding-api.open-meteo.com/v1/search";

/// A place from the search, to set the station's location.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub name: String,
    /// The state or region, if any.
    pub region: Option<String>,
    pub country: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
}

impl Place {
    /// "Leongatha, Victoria, Australia".
    pub fn label(&self) -> String {
        [
            Some(self.name.clone()),
            self.region.clone(),
            self.country.clone(),
        ]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForecastDay {
    pub date: NaiveDate,
    /// WMO weather code.
    pub code: Option<u32>,
    pub max: Option<f64>,
    pub min: Option<f64>,
    pub rain: Option<f64>,
    /// Highest chance of rain in the day, in percent.
    pub rain_chance: Option<f64>,
    pub wind: Option<f64>,
    pub gust: Option<f64>,
    /// Where the wind mostly comes from, in degrees.
    pub dir: Option<f64>,
    pub uv: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForecastHour {
    /// Unix ms.
    pub ts: i64,
    pub temp: Option<f64>,
    pub rain_chance: Option<f64>,
    pub rain: Option<f64>,
    pub code: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Forecast {
    /// When Isobar fetched it (Unix ms).
    pub fetched_at: i64,
    pub latitude: f64,
    pub longitude: f64,
    pub days: Vec<ForecastDay>,
    pub hours: Vec<ForecastHour>,
}

/// Open-Meteo's names for the station's units.
fn unit_params(u: &Units) -> String {
    let temp = if u.temp.contains('F') {
        "fahrenheit"
    } else {
        "celsius"
    };
    let rain = if u.rain.trim().eq_ignore_ascii_case("in") {
        "inch"
    } else {
        "mm"
    };
    let wind = match u.wind.trim().to_ascii_lowercase().as_str() {
        "m/s" => "ms",
        "mph" => "mph",
        "kts" | "kt" | "knots" => "kn",
        _ => "kmh",
    };
    format!("temperature_unit={temp}&precipitation_unit={rain}&wind_speed_unit={wind}")
}

pub fn forecast_url(base: &str, latitude: f64, longitude: f64, u: &Units) -> String {
    format!(
        "{base}?latitude={latitude:.4}&longitude={longitude:.4}\
         &daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_sum,\
         precipitation_probability_max,wind_speed_10m_max,wind_gusts_10m_max,\
         wind_direction_10m_dominant,uv_index_max\
         &hourly=temperature_2m,precipitation_probability,precipitation,weather_code\
         &forecast_days=7&forecast_hours=48&timezone=auto&timeformat=unixtime&{}",
        unit_params(u)
    )
}

fn http() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(8))
        .user_agent(concat!("Isobar/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

fn reach(e: reqwest::Error) -> CoreError {
    CoreError::Service(if e.is_timeout() {
        "Open-Meteo didn't answer in time".into()
    } else if e.is_connect() {
        "couldn't reach Open-Meteo. Is this computer online?".into()
    } else {
        format!("couldn't reach Open-Meteo ({e})")
    })
}

fn get(url: &str) -> Result<Value> {
    let resp = http()?.get(url).send().map_err(reach)?;
    let status = resp.status();
    let body = resp.text().map_err(reach)?;
    if !status.is_success() {
        // Open-Meteo explains a bad request in {"reason": "..."}.
        let reason = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|v| v.get("reason").and_then(Value::as_str).map(str::to_string));
        return Err(CoreError::Service(match reason {
            Some(r) => format!("Open-Meteo said: {r}"),
            None => format!("Open-Meteo replied with HTTP {}", status.as_u16()),
        }));
    }
    serde_json::from_str(&body)
        .map_err(|e| CoreError::Service(format!("Open-Meteo sent something that isn't JSON ({e})")))
}

/// The forecast at a location, in the station's units. `base` is
/// [`FORECAST_URL`] (or a test server).
pub fn fetch(
    base: &str,
    latitude: f64,
    longitude: f64,
    u: &Units,
    now_ms: i64,
) -> Result<Forecast> {
    parse(&get(&forecast_url(base, latitude, longitude, u))?, now_ms)
}

/// Places matching a name, best first.
pub fn search(base: &str, name: &str) -> Result<Vec<Place>> {
    let q: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c.to_string()
            } else if c == ' ' {
                "%20".into()
            } else {
                format!("%{:02X}", c as u32 & 0xff)
            }
        })
        .collect();
    let v = get(&format!("{base}?name={q}&count=8&language=en&format=json"))?;
    Ok(parse_places(&v))
}

pub fn parse_places(v: &Value) -> Vec<Place> {
    v.get("results")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|r| {
                    Some(Place {
                        name: r.get("name")?.as_str()?.to_string(),
                        region: r.get("admin1").and_then(Value::as_str).map(str::to_string),
                        country: r.get("country").and_then(Value::as_str).map(str::to_string),
                        latitude: r.get("latitude")?.as_f64()?,
                        longitude: r.get("longitude")?.as_f64()?,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn column<'a>(block: &'a Value, key: &str) -> Vec<&'a Value> {
    block
        .get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

fn at(col: &[&Value], i: usize) -> Option<f64> {
    col.get(i).and_then(|v| v.as_f64())
}

/// A forecast response (`timeformat=unixtime`, `timezone=auto`).
pub fn parse(v: &Value, now_ms: i64) -> Result<Forecast> {
    let offset = v
        .get("utc_offset_seconds")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let daily = v
        .get("daily")
        .ok_or_else(|| CoreError::Service("Open-Meteo's forecast had no daily figures".into()))?;
    let times = column(daily, "time");
    let cols: Vec<Vec<&Value>> = [
        "weather_code",
        "temperature_2m_max",
        "temperature_2m_min",
        "precipitation_sum",
        "precipitation_probability_max",
        "wind_speed_10m_max",
        "wind_gusts_10m_max",
        "wind_direction_10m_dominant",
        "uv_index_max",
    ]
    .iter()
    .map(|k| column(daily, k))
    .collect();
    let days = times
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            // Midnight local time, as Unix seconds.
            let date = DateTime::from_timestamp(t.as_i64()? + offset, 0)?.date_naive();
            Some(ForecastDay {
                date,
                code: at(&cols[0], i).map(|c| c as u32),
                max: at(&cols[1], i),
                min: at(&cols[2], i),
                rain: at(&cols[3], i),
                rain_chance: at(&cols[4], i),
                wind: at(&cols[5], i),
                gust: at(&cols[6], i),
                dir: at(&cols[7], i),
                uv: at(&cols[8], i),
            })
        })
        .collect();
    let hourly = v.get("hourly").cloned().unwrap_or(Value::Null);
    let htimes = column(&hourly, "time");
    let hcols: Vec<Vec<&Value>> = [
        "temperature_2m",
        "precipitation_probability",
        "precipitation",
        "weather_code",
    ]
    .iter()
    .map(|k| column(&hourly, k))
    .collect();
    let hours = htimes
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            Some(ForecastHour {
                ts: t.as_i64()? * 1000,
                temp: at(&hcols[0], i),
                rain_chance: at(&hcols[1], i),
                rain: at(&hcols[2], i),
                code: at(&hcols[3], i).map(|c| c as u32),
            })
        })
        .collect();
    Ok(Forecast {
        fetched_at: now_ms,
        latitude: v.get("latitude").and_then(Value::as_f64).unwrap_or(0.0),
        longitude: v.get("longitude").and_then(Value::as_f64).unwrap_or(0.0),
        days,
        hours,
    })
}

/// A WMO weather code in plain words.
pub fn words(code: u32) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 => "Light drizzle",
        53 => "Drizzle",
        55 => "Heavy drizzle",
        56 | 57 => "Freezing drizzle",
        61 => "Light rain",
        63 => "Rain",
        65 => "Heavy rain",
        66 | 67 => "Freezing rain",
        71 => "Light snow",
        73 => "Snow",
        75 => "Heavy snow",
        77 => "Snow grains",
        80 => "Light showers",
        81 => "Showers",
        82 => "Heavy showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorms",
        96 | 99 => "Thunderstorms with hail",
        _ => "Unsettled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_a_forecast_and_places() {
        // As Open-Meteo answers with timeformat=unixtime, timezone=auto
        // (Melbourne, +11:00 in November).
        let v = json!({
            "latitude": -38.5, "longitude": 145.875, "utc_offset_seconds": 39600,
            "timezone": "Australia/Melbourne",
            "daily": {
                "time": [1793624400, 1793710800],
                "weather_code": [61, 3],
                "temperature_2m_max": [17.4, 21.0],
                "temperature_2m_min": [9.1, 8.3],
                "precipitation_sum": [6.2, 0.0],
                "precipitation_probability_max": [85, 10],
                "wind_speed_10m_max": [28.1, 14.0],
                "wind_gusts_10m_max": [55.4, 30.2],
                "wind_direction_10m_dominant": [225, 180],
                "uv_index_max": [5.1, 7.9]
            },
            "hourly": {
                "time": [1793624400, 1793628000],
                "temperature_2m": [11.0, null],
                "precipitation_probability": [40, 55],
                "precipitation": [0.0, 0.4],
                "weather_code": [3, 61]
            }
        });
        let f = parse(&v, 7).unwrap();
        assert_eq!(f.days.len(), 2);
        assert_eq!(
            f.days[0].date,
            NaiveDate::from_ymd_opt(2026, 11, 3).unwrap()
        );
        assert_eq!(f.days[0].rain, Some(6.2));
        assert_eq!(f.days[0].rain_chance, Some(85.0));
        assert_eq!(words(f.days[0].code.unwrap()), "Light rain");
        assert_eq!(f.hours[1].temp, None);
        assert_eq!(f.hours[1].ts, 1_793_628_000_000);
        let url = forecast_url(FORECAST_URL, -38.5, 145.9, &Units::default());
        assert!(url.contains("wind_speed_unit=kmh") && url.contains("latitude=-38.5000"));
        let p = parse_places(&json!({"results": [
            {"name": "Leongatha", "admin1": "Victoria", "country": "Australia", "latitude": -38.48, "longitude": 145.95}
        ]}));
        assert_eq!(p[0].label(), "Leongatha, Victoria, Australia");
        assert!(parse_places(&json!({"generationtime_ms": 0.2})).is_empty());
    }
}
