//! A synthetic station for trying Isobar without Cumulus MX (`isobar --demo`).
//! Southern-hemisphere temperate climate: seasonal temperatures, bursty rain
//! and fronts, from March 2018 to yesterday.

use std::collections::BTreeMap;
use std::f64::consts::{PI, TAU};

use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone, Timelike};

use crate::dayfile::DayRecord;
use crate::live::{Alarm, LiveReading, Units};
use crate::seasons;

/// The demo house's rainwater tanks.
pub fn tanks() -> crate::tanks::Tanks {
    crate::tanks::Tanks {
        roof_area: 400.0,
        tanks: vec![
            crate::tanks::Tank {
                litres: 22_500.0,
                diameter_mm: Some(3530.0),
            },
            crate::tanks::Tank {
                litres: 10_000.0,
                diameter_mm: Some(2400.0),
            },
        ],
        use_per_day: 220.0,
        keep_above: 3000.0,
        known: None,
    }
}

/// Where the demo station is, near Sydney.
pub const LATITUDE: f64 = -33.9;

/// A small deterministic generator, so the demo looks the same every run.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0 as f64 / 4_294_967_296.0
    }

    fn gauss(&mut self) -> f64 {
        let u = self.next().max(1e-9);
        (-2.0 * u.ln()).sqrt() * (TAU * self.next()).cos()
    }
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

pub fn days() -> Vec<DayRecord> {
    let mut r = Rng(42);
    let mut out = Vec::new();
    let end = Local::now().date_naive() - Duration::days(1);
    let mut d = NaiveDate::from_ymd_opt(2018, 3, 1).unwrap();
    let (mut anomaly, mut wetness) = (0.0f64, 0.0f64);
    while d <= end {
        // +1 in mid-January, -1 in mid-July.
        let season = (TAU * (d.ordinal() as f64 - 20.0) / 365.25).cos();
        anomaly = anomaly * 0.7 + r.gauss() * 2.2;
        let hot = if r.next() < 0.04 * season.max(0.0) {
            9.0 + r.next() * 8.0
        } else {
            0.0
        };
        let tmax = 20.0 + 6.5 * season + anomaly + hot;
        let tmin = tmax - (8.0 + 2.0 * season + r.gauss() * 1.5) - hot * 0.5;
        // Clear, still winter nights get cold enough for frost now and then.
        let tmin = tmin
            - if wetness < 0.1 {
                4.0 * (-season).max(0.0)
            } else {
                0.0
            };
        wetness = wetness * 0.55
            + if r.next() < 0.3 + 0.12 * -season {
                r.next()
            } else {
                0.0
            };
        let rain = if wetness > 0.35 {
            round1(-(r.next().max(1e-6)).ln() * 4.0 * wetness)
        } else {
            0.0
        };
        let gust =
            round1(28.0 + 12.0 * wetness + r.gauss() * 8.0 + if hot > 0.0 { 15.0 } else { 0.0 });
        let dirs: &[f64] = if hot > 0.0 {
            &[0.0, 22.5, 337.5]
        } else if wetness > 0.3 {
            &[202.5, 225.0, 247.5, 270.0]
        } else {
            &[0.0, 180.0, 202.5, 225.0, 337.5, 157.5]
        };
        let dir = (dirs[(r.next() * dirs.len() as f64) as usize] + (r.gauss() * 8.0).round())
            .rem_euclid(360.0);
        let p_base = 1016.0 - 10.0 * wetness + r.gauss() * 4.0;
        let dew_max = (tmax - 1.0).min(tmin + 4.0 + r.gauss() * 2.0);
        let minute = |r: &mut Rng| format!("{:02}", (r.next() * 60.0) as u32);
        let et = seasons::hargreaves(tmax, tmin, LATITUDE, d) * (1.0 - wetness * 0.6).max(0.3);
        // A few days lost in a power cut, as real stations have.
        let lost = d >= NaiveDate::from_ymd_opt(2021, 8, 14).unwrap()
            && d <= NaiveDate::from_ymd_opt(2021, 8, 16).unwrap();
        if lost {
            d += Duration::days(1);
            continue;
        }
        out.push(DayRecord {
            date: d,
            temp_max: Some(round1(tmax.max(tmin + 2.0))),
            temp_max_time: Some(format!("14:{}", minute(&mut r))),
            temp_min: Some(round1(tmin)),
            temp_min_time: Some(format!("06:{}", minute(&mut r))),
            temp_avg: Some(round1((tmax + tmin) / 2.0)),
            rain: Some(rain.max(0.0)),
            rain_rate_max: Some(if rain > 0.0 {
                round1(rain * (0.8 + r.next() * 3.0))
            } else {
                0.0
            }),
            gust_max: Some(gust.max(5.0)),
            gust_bearing: Some(dir),
            gust_time: Some(format!("13:{}", minute(&mut r))),
            // Steadier wind in the wet westerlies, gustier on hot northerly
            // days, so the gust factor has some character.
            wind_avg_max: Some(round1(
                gust * (0.55 + 0.05 * wetness - if hot > 0.0 { 0.07 } else { 0.0 }
                    + 0.04 * (d.ordinal() as f64 * 2.3).sin()),
            )),
            wind_run: Some(round1(gust * 6.0 + r.next() * 60.0)),
            wind_dir_dominant: Some(dir.round()),
            press_min: Some(round1(p_base - 2.0 - r.next() * 5.0)),
            press_max: Some(round1(p_base + 2.0 + r.next() * 4.0)),
            hum_min: Some((35.0 + 20.0 * -season + wetness * 30.0 + r.gauss() * 6.0).round()),
            hum_max: Some((85.0 + wetness * 12.0 + r.gauss() * 3.0).min(99.0).round()),
            dew_max: Some(round1(dew_max)),
            dew_min: Some(round1(dew_max - 4.0 - r.next() * 3.0)),
            solar_max: Some(
                (650.0 + 350.0 * season - wetness * 400.0 + r.gauss() * 60.0)
                    .max(80.0)
                    .round(),
            ),
            uv_max: Some(round1((6.0 + 5.0 * season - wetness * 4.0).max(0.5))),
            sunshine: Some(round1((7.0 + 3.0 * season - wetness * 8.0).max(0.0))),
            et: Some(round1(et)),
            ..Default::default()
        });
        d += Duration::days(1);
    }
    out
}

/// A smooth last 24 hours of samples, every five minutes, ending at `now_ms`.
pub fn samples(now_ms: i64) -> BTreeMap<String, Vec<(i64, f64)>> {
    samples_for(now_ms, 1)
}

/// Samples like [`samples`], `days` back.
pub fn samples_for(now_ms: i64, days: i64) -> BTreeMap<String, Vec<(i64, f64)>> {
    let mut r = Rng(7);
    let mut out: BTreeMap<String, Vec<(i64, f64)>> = BTreeMap::new();
    let mut push = |k: &str, t: i64, v: f64| out.entry(k.to_string()).or_default().push((t, v));
    let mut rain_acc = 0.0;
    let start = now_ms - days * 24 * 3_600_000;
    let mut t = start;
    while t <= now_ms {
        let local = Local.timestamp_millis_opt(t).unwrap();
        let h = local.hour() as f64 + local.minute() as f64 / 60.0;
        // Coolest about 3am, warmest about 3pm.
        let diurnal = -(((h - 3.0) / 24.0) * TAU).cos();
        let tf = t as f64;
        push(
            "temp",
            t,
            round1(15.0 + 5.0 * diurnal + (tf / 3.1e6).sin() * 0.6),
        );
        push("dew", t, round1(8.0 + (tf / 9e6).sin() * 1.5));
        // Highs and lows passing every few days.
        push(
            "press",
            t,
            round1(1012.0 + 8.0 * (tf / 6.9e7).sin() + (tf / 5e6).sin() * 0.4),
        );
        let wind = (10.0 + 6.0 * diurnal.max(0.0) + (r.next() - 0.5) * 6.0).max(0.0);
        push("wind", t, round1(wind));
        push("gust", t, round1(wind * (1.5 + r.next() * 0.6)));
        push(
            "hum",
            t,
            (65.0 - 20.0 * diurnal + (r.next() - 0.5) * 4.0).round(),
        );
        if h > 4.0 && h < 6.0 {
            rain_acc += 0.2;
        }
        if local.hour() == 0 && local.minute() < 5 {
            rain_acc = 0.0;
        }
        push("rain_today", t, round1(rain_acc));
        let sun = (((h - 6.0) / 13.0) * PI).sin().max(0.0);
        push("solar", t, (sun * 780.0 * (0.8 + r.next() * 0.2)).round());
        push("solar_max", t, clear_sky(t));
        push("bearing", t, (200.0 + (tf / 4e6).sin() * 25.0).round());
        // Indoors swings less and lags the outside: heated from 07:00 to
        // 22:00, then cooling towards the night outside with a half-life of
        // about ten hours.
        let lag = -(((h - 6.0) / 24.0) * TAU).cos();
        let outside = 15.0 + 5.0 * diurnal;
        let inside = if (7.0..22.0).contains(&h) {
            19.8 + 0.8 * lag
        } else {
            let since = if h >= 22.0 { h - 22.0 } else { h + 2.0 };
            let night = outside.min(13.0);
            night + (20.4 - night) * 0.5f64.powf(since / 10.0)
        };
        push("intemp", t, round1(inside));
        push("inhum", t, (54.0 + 4.0 * lag).round());
        t += 5 * 60_000;
    }
    out
}

/// "2026-10-12 02:02" in local time, as MX writes its last rain tip.
fn local_iso(ms: i64) -> String {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

/// Sunrise (`side` -1) or sunset (+1) today at the demo station, as MX
/// writes them ("6:36 AM"), around a local noon of 12:50.
fn sun_time(ms: i64, side: f64) -> String {
    let date = Local.timestamp_millis_opt(ms).unwrap().date_naive();
    let half = seasons::day_length(LATITUDE, date) / 2.0;
    let mins = ((12.0 + 50.0 / 60.0 + side * half) * 60.0).round() as u32;
    let (h, m) = (mins / 60, mins % 60);
    let h12 = if h % 12 == 0 { 12 } else { h % 12 };
    format!("{h12}:{m:02} {}", if h < 12 { "AM" } else { "PM" })
}

/// A rough clear-sky maximum for the time of day, peaking near 950 W/m².
fn clear_sky(ms: i64) -> f64 {
    let t = Local.timestamp_millis_opt(ms).unwrap();
    let h = t.hour() as f64 + t.minute() as f64 / 60.0;
    ((((h - 6.0) / 13.0) * PI).sin().max(0.0) * 950.0).round()
}

/// A live reading consistent with the latest samples. `jitter` in -1..1
/// moves the temperature a little so the display visibly updates.
pub fn live(samples: &BTreeMap<String, Vec<(i64, f64)>>, now_ms: i64, jitter: f64) -> LiveReading {
    let last = |k: &str| samples.get(k).and_then(|v| v.last()).map(|p| p.1);
    LiveReading {
        fetched_at: now_ms,
        station_time: None,
        temp: last("temp").map(|t| round1(t + jitter * 0.1)),
        temp_high: Some(19.8),
        temp_high_time: Some("14:52".into()),
        temp_low: Some(9.6),
        temp_low_time: Some("05:41".into()),
        temp_trend: Some(0.4),
        feels_like: last("temp").map(|t| round1(t - 1.4)),
        apparent: last("temp").map(|t| round1(t - 1.9)),
        wind_chill: last("temp").map(|t| round1(t - 0.8)),
        dew_point: last("dew"),
        humidity: last("hum"),
        hum_high: Some(88.0),
        hum_low: Some(44.0),
        pressure: last("press"),
        press_high: Some(1018.4),
        press_low: Some(1011.9),
        press_trend: Some(-0.6),
        press_trend_text: None,
        wind_avg: last("wind"),
        wind_latest: last("wind"),
        gust_recent: last("gust"),
        gust_high: Some(47.2),
        gust_high_time: Some("13:18".into()),
        bearing: last("bearing"),
        bearing_avg: Some(205.0),
        bearing_compass: Some("SSW".into()),
        rain_today: last("rain_today"),
        rain_rate: Some(0.0),
        rain_rate_high: Some(3.6),
        rain_last_hour: Some(0.0),
        rain_month: Some(41.8),
        rain_year: Some(512.4),
        solar: last("solar"),
        solar_high: Some(812.0),
        uv: Some(3.2),
        uv_high: Some(6.1),
        forecast: Some("Fine, becoming less settled".into()),
        rain_yesterday: Some(3.2),
        rain_week: Some(9.6),
        rain_24h: Some(4.8),
        storm_rain: Some(8.0),
        storm_start: Some("16/10/2026 4:10 PM".into()),
        last_rain_tip: Some(local_iso(now_ms - 26 * 3_600_000)),
        sunrise: Some(sun_time(now_ms, -1.0)),
        sunset: Some(sun_time(now_ms, 1.0)),
        moonrise: Some("11:30 AM".into()),
        moonset: Some("2:08 AM".into()),
        sunshine_hours: Some(5.4),
        solar_max_now: Some(clear_sky(now_ms)),
        indoor_temp: last("intemp"),
        indoor_hum: last("inhum"),
        dew_high: Some(8.9),
        dew_high_time: Some("13:40".into()),
        dew_low: Some(4.7),
        dew_low_time: Some("05:30".into()),
        chill_low: Some(7.8),
        chill_low_time: Some("05:41".into()),
        heat_index: last("temp").map(round1),
        heat_index_high: Some(19.8),
        heat_index_high_time: Some("14:52".into()),
        humidex: last("temp").map(|t| round1(t + 0.6)),
        humidex_high: Some(20.9),
        humidex_high_time: Some("14:50".into()),
        apparent_high: Some(18.1),
        apparent_high_time: Some("14:55".into()),
        apparent_low: Some(7.2),
        apparent_low_time: Some("05:44".into()),
        cloud_base: Some(1050.0),
        cloud_base_unit: Some("m".into()),
        data_stopped: Some(false),
        alarms: ["AlarmSensor", "AlarmSpike", "AlarmUpgrade", "AlarmBattery"]
            .into_iter()
            .map(|id| Alarm {
                id: id.into(),
                triggered: id == "AlarmSpike",
            })
            .collect(),
        mx_version: Some("5.1.5".into()),
        mx_build: Some("5012".into()),
        station_date: None,
        units: Units::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_history_is_plausible() {
        let d = days();
        assert!(d.len() > 3000);
        assert!(d.windows(2).all(|w| w[0].date < w[1].date));
        assert!(d.iter().all(|r| r.temp_max >= r.temp_min));
        let s = samples(1_700_000_000_000);
        assert_eq!(s["temp"].len(), 24 * 12 + 1);
        assert!(live(&s, 1_700_000_000_000, 0.0).looks_valid());
    }
}
