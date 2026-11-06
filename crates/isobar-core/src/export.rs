//! The daily history as CSV, to open in a spreadsheet.

use chrono::NaiveDate;

use crate::{DayRecord, Units};

pub use crate::units::run_unit;

fn quote(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn num(v: Option<f64>) -> String {
    v.map(|x| {
        let s = format!("{:.2}", x);
        let s = s.trim_end_matches('0').trim_end_matches('.');
        if s == "-0" {
            "0".to_string()
        } else {
            s.to_string()
        }
    })
    .unwrap_or_default()
}

/// Every day from `from` to `to` (inclusive) as CSV, a row a day with a
/// heading row naming each column and its unit. Dates are YYYY-MM-DD, which
/// spreadsheets read as dates whatever the region; missing values are left
/// empty. Starts with a byte-order mark so Excel reads the ° signs.
pub fn daily_csv(days: &[DayRecord], from: NaiveDate, to: NaiveDate, u: &Units) -> String {
    let t = &u.temp;
    let r = &u.rain;
    let w = &u.wind;
    let p = &u.press;
    let heads: Vec<String> = vec![
        "Date".into(),
        format!("High ({t})"),
        "High at".into(),
        format!("Low ({t})"),
        "Low at".into(),
        format!("Average ({t})"),
        format!("Rain ({r})"),
        format!("Highest rain rate ({r}/hr)"),
        format!("Wettest hour ({r})"),
        format!("Peak gust ({w})"),
        "Gust bearing (°)".into(),
        "Gust at".into(),
        format!("Highest average wind ({w})"),
        format!("Wind run ({})", run_unit(w)),
        "Dominant wind direction (°)".into(),
        format!("Lowest pressure ({p})"),
        format!("Highest pressure ({p})"),
        "Lowest humidity (%)".into(),
        "Highest humidity (%)".into(),
        format!("Lowest dew point ({t})"),
        format!("Highest dew point ({t})"),
        format!("Highest feels like ({t})"),
        format!("Lowest feels like ({t})"),
        format!("Highest apparent ({t})"),
        format!("Lowest apparent ({t})"),
        "Sunshine (hours)".into(),
        format!("Evapotranspiration ({r})"),
        "Highest solar (W/m²)".into(),
        "Highest UV".into(),
        "Heating degree days".into(),
        "Cooling degree days".into(),
    ];
    let mut out = String::from("\u{feff}");
    out += &heads.iter().map(|h| quote(h)).collect::<Vec<_>>().join(",");
    out += "\r\n";
    let time = |s: &Option<String>| s.as_deref().map(quote).unwrap_or_default();
    for d in crate::analytics::in_range(days, from, to) {
        let row = [
            d.date.format("%Y-%m-%d").to_string(),
            num(d.temp_max),
            time(&d.temp_max_time),
            num(d.temp_min),
            time(&d.temp_min_time),
            num(d.temp_avg),
            num(d.rain),
            num(d.rain_rate_max),
            num(d.rain_hour_max),
            num(d.gust_max),
            num(d.gust_bearing),
            time(&d.gust_time),
            num(d.wind_avg_max),
            num(d.wind_run),
            num(d.wind_dir_dominant),
            num(d.press_min),
            num(d.press_max),
            num(d.hum_min),
            num(d.hum_max),
            num(d.dew_min),
            num(d.dew_max),
            num(d.feels_max),
            num(d.feels_min),
            num(d.apparent_max),
            num(d.apparent_min),
            num(d.sunshine),
            num(d.et),
            num(d.solar_max),
            num(d.uv_max),
            num(d.hdd),
            num(d.cdd),
        ];
        out += &row.join(",");
        out += "\r\n";
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_for_the_range() {
        let d = |day, max| DayRecord {
            date: NaiveDate::from_ymd_opt(2026, 1, day).unwrap(),
            temp_max: Some(max),
            temp_max_time: Some("14:05".into()),
            rain: Some(0.0),
            ..Default::default()
        };
        let days = vec![d(1, 25.0), d(2, 31.25), d(3, -0.001)];
        let csv = daily_csv(
            &days,
            NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 3).unwrap(),
            &Units::default(),
        );
        let lines: Vec<&str> = csv.trim_start_matches('\u{feff}').lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("Date,High (°C),High at,Low (°C)"));
        assert!(lines[0].contains("Wind run (km)"));
        assert!(lines[1].starts_with("2026-01-02,31.25,14:05,,,,0,"));
        assert!(lines[2].starts_with("2026-01-03,0,"));
        assert_eq!(lines[1].split(',').count(), lines[0].split(',').count());
        assert_eq!(quote("a, \"b\""), "\"a, \"\"b\"\"\"");
    }
}
