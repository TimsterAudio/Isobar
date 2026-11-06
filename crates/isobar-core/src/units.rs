//! Units of the owner's choosing. Isobar stores everything in the station's
//! own units, as MX sends them; a [`Converter`] turns readings, days and
//! series into the units chosen in Settings, just before they're shown.

use crate::{DayRecord, LiveReading, Units};

pub const TEMPS: [&str; 2] = ["°C", "°F"];
pub const RAINS: [&str; 2] = ["mm", "in"];
pub const WINDS: [&str; 4] = ["km/h", "m/s", "mph", "kts"];
pub const PRESSES: [&str; 4] = ["hPa", "mb", "inHg", "kPa"];

fn fahrenheit(u: &str) -> bool {
    u.to_ascii_uppercase().contains('F')
}

fn inches(u: &str) -> bool {
    u.trim().eq_ignore_ascii_case("in")
}

/// km/h in one of the speed unit.
fn kmh_per(u: &str) -> f64 {
    match u.trim().to_ascii_lowercase().as_str() {
        "m/s" => 3.6,
        "mph" => 1.609344,
        "kts" | "kt" | "knots" => 1.852,
        _ => 1.0,
    }
}

/// hPa in one of the pressure unit.
fn hpa_per(u: &str) -> f64 {
    match u.trim().to_ascii_lowercase().as_str() {
        "inhg" | "in" => 33.863_886,
        "kpa" => 10.0,
        "mmhg" => 1.333_224,
        _ => 1.0,
    }
}

/// The distance unit of a day's wind run, from the speed unit.
pub fn run_unit(wind: &str) -> &'static str {
    match wind.trim().to_ascii_lowercase().as_str() {
        "mph" => "miles",
        "kts" | "kt" | "knots" => "nm",
        _ => "km",
    }
}

/// km in one of the distance unit.
fn km_per(u: &str) -> f64 {
    match u.trim().to_ascii_lowercase().as_str() {
        "miles" | "mi" => 1.609344,
        "nm" | "nautical miles" => 1.852,
        "m" => 0.001,
        _ => 1.0,
    }
}

/// From the station's units to the ones shown.
#[derive(Debug, Clone, PartialEq)]
pub struct Converter {
    pub from: Units,
    pub to: Units,
}

impl Converter {
    pub fn new(from: Units, to: Units) -> Converter {
        Converter { from, to }
    }

    /// Nothing to change.
    pub fn same(&self) -> bool {
        fahrenheit(&self.from.temp) == fahrenheit(&self.to.temp)
            && inches(&self.from.rain) == inches(&self.to.rain)
            && kmh_per(&self.from.wind) == kmh_per(&self.to.wind)
            && hpa_per(&self.from.press) == hpa_per(&self.to.press)
    }

    pub fn temp(&self, v: f64) -> f64 {
        match (fahrenheit(&self.from.temp), fahrenheit(&self.to.temp)) {
            (false, true) => v * 1.8 + 32.0,
            (true, false) => (v - 32.0) / 1.8,
            _ => v,
        }
    }

    /// A difference of temperatures (a range, a trend, degree days).
    pub fn temp_delta(&self, v: f64) -> f64 {
        match (fahrenheit(&self.from.temp), fahrenheit(&self.to.temp)) {
            (false, true) => v * 1.8,
            (true, false) => v / 1.8,
            _ => v,
        }
    }

    pub fn rain(&self, v: f64) -> f64 {
        match (inches(&self.from.rain), inches(&self.to.rain)) {
            (false, true) => v / 25.4,
            (true, false) => v * 25.4,
            _ => v,
        }
    }

    pub fn wind(&self, v: f64) -> f64 {
        v * kmh_per(&self.from.wind) / kmh_per(&self.to.wind)
    }

    pub fn press(&self, v: f64) -> f64 {
        v * hpa_per(&self.from.press) / hpa_per(&self.to.press)
    }

    /// A wind run from `unit` (or the station's speed unit's distance).
    pub fn run(&self, v: f64, unit: Option<&str>) -> f64 {
        let from = unit.unwrap_or(run_unit(&self.from.wind));
        v * km_per(from) / km_per(run_unit(&self.to.wind))
    }

    /// A value of one of the recorded series (`"temp"`, `"press"`, ...).
    pub fn metric(&self, key: &str, v: f64) -> f64 {
        match key {
            "temp" | "dew" | "feels" | "intemp" | "apparent" | "chill" => self.temp(v),
            "press" => self.press(v),
            "wind" | "gust" => self.wind(v),
            "rain_today" | "rain_rate" | "rain_midnight" => self.rain(v),
            _ => v,
        }
    }

    pub fn day(&self, d: &DayRecord) -> DayRecord {
        let t = |v: Option<f64>| v.map(|x| self.temp(x));
        let dt = |v: Option<f64>| v.map(|x| self.temp_delta(x));
        let r = |v: Option<f64>| v.map(|x| self.rain(x));
        let w = |v: Option<f64>| v.map(|x| self.wind(x));
        let p = |v: Option<f64>| v.map(|x| self.press(x));
        DayRecord {
            temp_max: t(d.temp_max),
            temp_min: t(d.temp_min),
            temp_avg: t(d.temp_avg),
            rain: r(d.rain),
            rain_rate_max: r(d.rain_rate_max),
            rain_hour_max: r(d.rain_hour_max),
            gust_max: w(d.gust_max),
            wind_avg_max: w(d.wind_avg_max),
            wind_run: d.wind_run.map(|x| self.run(x, None)),
            press_min: p(d.press_min),
            press_max: p(d.press_max),
            dew_min: t(d.dew_min),
            dew_max: t(d.dew_max),
            apparent_max: t(d.apparent_max),
            apparent_min: t(d.apparent_min),
            feels_max: t(d.feels_max),
            feels_min: t(d.feels_min),
            et: r(d.et),
            hdd: dt(d.hdd),
            cdd: dt(d.cdd),
            ..d.clone()
        }
    }

    pub fn live(&self, x: &LiveReading) -> LiveReading {
        let t = |v: Option<f64>| v.map(|y| self.temp(y));
        let r = |v: Option<f64>| v.map(|y| self.rain(y));
        let w = |v: Option<f64>| v.map(|y| self.wind(y));
        let p = |v: Option<f64>| v.map(|y| self.press(y));
        LiveReading {
            temp: t(x.temp),
            temp_high: t(x.temp_high),
            temp_low: t(x.temp_low),
            temp_trend: x.temp_trend.map(|y| self.temp_delta(y)),
            feels_like: t(x.feels_like),
            apparent: t(x.apparent),
            wind_chill: t(x.wind_chill),
            dew_point: t(x.dew_point),
            pressure: p(x.pressure),
            press_high: p(x.press_high),
            press_low: p(x.press_low),
            press_trend: p(x.press_trend),
            wind_avg: w(x.wind_avg),
            wind_latest: w(x.wind_latest),
            gust_recent: w(x.gust_recent),
            gust_high: w(x.gust_high),
            rain_today: r(x.rain_today),
            rain_rate: r(x.rain_rate),
            rain_rate_high: r(x.rain_rate_high),
            rain_last_hour: r(x.rain_last_hour),
            rain_month: r(x.rain_month),
            rain_year: r(x.rain_year),
            rain_yesterday: r(x.rain_yesterday),
            rain_week: r(x.rain_week),
            rain_24h: r(x.rain_24h),
            storm_rain: r(x.storm_rain),
            indoor_temp: t(x.indoor_temp),
            dew_high: t(x.dew_high),
            dew_low: t(x.dew_low),
            chill_low: t(x.chill_low),
            heat_index: t(x.heat_index),
            heat_index_high: t(x.heat_index_high),
            humidex: t(x.humidex),
            humidex_high: t(x.humidex_high),
            apparent_high: t(x.apparent_high),
            apparent_low: t(x.apparent_low),
            wind_run_today: x
                .wind_run_today
                .map(|y| self.run(y, x.wind_run_unit.as_deref())),
            wind_run_unit: x
                .wind_run_today
                .map(|_| run_unit(&self.to.wind).to_string()),
            units: self.to.clone(),
            ..x.clone()
        }
    }
}

/// The units shown: each chosen one, or the station's where none is.
pub fn chosen(station: &Units, temp: &str, rain: &str, wind: &str, press: &str) -> Units {
    let pick = |c: &str, s: &str| {
        if c.trim().is_empty() {
            s.to_string()
        } else {
            c.to_string()
        }
    };
    Units {
        temp: pick(temp, &station.temp),
        rain: pick(rain, &station.rain),
        wind: pick(wind, &station.wind),
        press: pick(press, &station.press),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn imperial() -> Units {
        Units {
            temp: "°F".into(),
            rain: "in".into(),
            wind: "mph".into(),
            press: "inHg".into(),
        }
    }

    #[test]
    fn conversions() {
        let c = Converter::new(Units::default(), imperial());
        assert!(!c.same());
        assert!((c.temp(100.0) - 212.0).abs() < 1e-9);
        assert!((c.temp(-40.0) + 40.0).abs() < 1e-9);
        assert!((c.temp_delta(10.0) - 18.0).abs() < 1e-9);
        assert!((c.rain(25.4) - 1.0).abs() < 1e-9);
        assert!((c.wind(1.609344) - 1.0).abs() < 1e-9);
        assert!((c.press(1013.25) - 29.92).abs() < 0.01);
        assert!((c.run(1.609344, None) - 1.0).abs() < 1e-9);
        let back = Converter::new(imperial(), Units::default());
        assert!((back.temp(32.0)).abs() < 1e-9);
        let knots = Converter::new(
            Units::default(),
            Units {
                wind: "kts".into(),
                ..Units::default()
            },
        );
        assert!((knots.wind(1.852) - 1.0).abs() < 1e-9);
        assert!((knots.run(1.852, Some("km")) - 1.0).abs() < 1e-9);
        assert!(Converter::new(Units::default(), Units::default()).same());
        let mb = Converter::new(
            Units::default(),
            Units {
                press: "mb".into(),
                ..Units::default()
            },
        );
        assert!(mb.same());
    }

    #[test]
    fn readings_and_days() {
        let c = Converter::new(Units::default(), imperial());
        let r = LiveReading {
            temp: Some(20.0),
            rain_today: Some(12.7),
            gust_high: Some(16.09344),
            wind_run_today: Some(160.9344),
            wind_run_unit: Some("km".into()),
            humidity: Some(50.0),
            ..Default::default()
        };
        let x = c.live(&r);
        assert_eq!(x.temp, Some(68.0));
        assert!((x.rain_today.unwrap() - 0.5).abs() < 1e-9);
        assert!((x.gust_high.unwrap() - 10.0).abs() < 1e-9);
        assert!((x.wind_run_today.unwrap() - 100.0).abs() < 1e-9);
        assert_eq!(x.wind_run_unit.as_deref(), Some("miles"));
        assert_eq!(x.humidity, Some(50.0));
        assert_eq!(x.units, imperial());
        let d = c.day(&DayRecord {
            temp_max: Some(30.0),
            hdd: Some(5.0),
            rain: Some(25.4),
            hum_max: Some(90.0),
            ..Default::default()
        });
        assert_eq!(d.temp_max, Some(86.0));
        assert_eq!(d.hdd, Some(9.0));
        assert!((d.rain.unwrap() - 1.0).abs() < 1e-9);
        assert_eq!(d.hum_max, Some(90.0));
        assert_eq!(c.metric("hum", 50.0), 50.0);
        assert_eq!(c.metric("temp", 0.0), 32.0);
        let u = chosen(&Units::default(), "°F", "", "", "inHg");
        assert_eq!(
            (u.temp.as_str(), u.rain.as_str(), u.press.as_str()),
            ("°F", "mm", "inHg")
        );
    }
}
