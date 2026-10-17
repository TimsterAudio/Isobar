//! Parsing of Cumulus `dayfile.txt` records.
//!
//! Field positions follow the Cumulus wiki (zero-based, date first). Older
//! lines have fewer fields; anything missing or blank becomes `None`, so a
//! 15-field line from 2008 and a 55-field line from MX 4 both parse.
//!
//! The same parser handles rows from the MX API (`/api/data/dayfile`), which
//! returns each line as an array of strings preceded by a line number.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// One meteorological day.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DayRecord {
    pub date: NaiveDate,
    pub temp_max: Option<f64>,
    pub temp_max_time: Option<String>,
    pub temp_min: Option<f64>,
    pub temp_min_time: Option<String>,
    pub temp_avg: Option<f64>,
    pub rain: Option<f64>,
    pub rain_rate_max: Option<f64>,
    pub rain_hour_max: Option<f64>,
    pub gust_max: Option<f64>,
    pub gust_bearing: Option<f64>,
    pub gust_time: Option<String>,
    pub wind_avg_max: Option<f64>,
    pub wind_run: Option<f64>,
    pub wind_dir_dominant: Option<f64>,
    pub press_min: Option<f64>,
    pub press_max: Option<f64>,
    pub hum_min: Option<f64>,
    pub hum_max: Option<f64>,
    pub dew_min: Option<f64>,
    pub dew_max: Option<f64>,
    pub apparent_max: Option<f64>,
    pub apparent_min: Option<f64>,
    pub feels_max: Option<f64>,
    pub feels_min: Option<f64>,
    pub sunshine: Option<f64>,
    pub et: Option<f64>,
    pub solar_max: Option<f64>,
    pub uv_max: Option<f64>,
    pub hdd: Option<f64>,
    pub cdd: Option<f64>,
}

/// Zero-based field positions within a dayfile line.
mod f {
    pub const DATE: usize = 0;
    pub const GUST: usize = 1;
    pub const GUST_BEARING: usize = 2;
    pub const GUST_TIME: usize = 3;
    pub const TEMP_MIN: usize = 4;
    pub const TEMP_MIN_TIME: usize = 5;
    pub const TEMP_MAX: usize = 6;
    pub const TEMP_MAX_TIME: usize = 7;
    pub const PRESS_MIN: usize = 8;
    pub const PRESS_MAX: usize = 10;
    pub const RAIN_RATE_MAX: usize = 12;
    pub const RAIN: usize = 14;
    pub const TEMP_AVG: usize = 15;
    pub const WIND_RUN: usize = 16;
    pub const WIND_AVG_MAX: usize = 17;
    pub const HUM_MIN: usize = 19;
    pub const HUM_MAX: usize = 21;
    pub const ET: usize = 23;
    pub const SUNSHINE: usize = 24;
    pub const APPARENT_MAX: usize = 27;
    pub const APPARENT_MIN: usize = 29;
    pub const RAIN_HOUR_MAX: usize = 31;
    pub const DEW_MAX: usize = 35;
    pub const DEW_MIN: usize = 37;
    pub const WIND_DIR_DOMINANT: usize = 39;
    pub const HDD: usize = 40;
    pub const CDD: usize = 41;
    pub const SOLAR_MAX: usize = 42;
    pub const UV_MAX: usize = 44;
    pub const FEELS_MAX: usize = 46;
    pub const FEELS_MIN: usize = 48;
}

/// Parse a Cumulus date. Accepts `dd/mm/yy` with any separator, `dd/mm/yyyy`,
/// and ISO `yyyy-mm-dd`.
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    let parts: Vec<&str> = s
        .trim()
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() != 3 {
        return None;
    }
    let n: Vec<u32> = parts
        .iter()
        .map(|p| p.parse().ok())
        .collect::<Option<Vec<u32>>>()?;
    if parts[0].len() == 4 {
        return NaiveDate::from_ymd_opt(n[0] as i32, n[1], n[2]);
    }
    let year = if parts[2].len() == 4 {
        n[2] as i32
    } else if n[2] >= 70 {
        1900 + n[2] as i32
    } else {
        2000 + n[2] as i32
    };
    NaiveDate::from_ymd_opt(year, n[1], n[0])
}

fn num(fields: &[String], i: usize) -> Option<f64> {
    let t = fields.get(i)?.trim();
    if t.is_empty() || t == "-" || t == "---" {
        return None;
    }
    // Locales that use ';' as the list separator write decimal commas.
    t.replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite())
}

fn time(fields: &[String], i: usize) -> Option<String> {
    let t = fields.get(i)?.trim();
    (t.len() >= 4 && t.contains(':')).then(|| t.to_string())
}

/// Build a record from already-split fields (date first).
pub fn from_fields(fields: &[String]) -> Option<DayRecord> {
    let date = parse_date(fields.get(f::DATE)?)?;
    Some(DayRecord {
        date,
        temp_max: num(fields, f::TEMP_MAX),
        temp_max_time: time(fields, f::TEMP_MAX_TIME),
        temp_min: num(fields, f::TEMP_MIN),
        temp_min_time: time(fields, f::TEMP_MIN_TIME),
        temp_avg: num(fields, f::TEMP_AVG),
        rain: num(fields, f::RAIN),
        rain_rate_max: num(fields, f::RAIN_RATE_MAX),
        rain_hour_max: num(fields, f::RAIN_HOUR_MAX),
        gust_max: num(fields, f::GUST),
        gust_bearing: num(fields, f::GUST_BEARING),
        gust_time: time(fields, f::GUST_TIME),
        wind_avg_max: num(fields, f::WIND_AVG_MAX),
        wind_run: num(fields, f::WIND_RUN),
        wind_dir_dominant: num(fields, f::WIND_DIR_DOMINANT),
        press_min: num(fields, f::PRESS_MIN),
        press_max: num(fields, f::PRESS_MAX),
        hum_min: num(fields, f::HUM_MIN),
        hum_max: num(fields, f::HUM_MAX),
        dew_min: num(fields, f::DEW_MIN),
        dew_max: num(fields, f::DEW_MAX),
        apparent_max: num(fields, f::APPARENT_MAX),
        apparent_min: num(fields, f::APPARENT_MIN),
        feels_max: num(fields, f::FEELS_MAX),
        feels_min: num(fields, f::FEELS_MIN),
        sunshine: num(fields, f::SUNSHINE),
        et: num(fields, f::ET),
        solar_max: num(fields, f::SOLAR_MAX),
        uv_max: num(fields, f::UV_MAX),
        hdd: num(fields, f::HDD),
        cdd: num(fields, f::CDD),
    })
}

/// Parse one raw line of `dayfile.txt`.
pub fn parse_line(line: &str) -> Option<DayRecord> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let sep = if line.contains(';') {
        ';'
    } else if line.contains('|') {
        '|'
    } else {
        ','
    };
    let fields: Vec<String> = line.split(sep).map(str::to_string).collect();
    from_fields(&fields)
}

/// Parse a whole file. Returns the records and the number of lines skipped.
pub fn parse_file(text: &str) -> (Vec<DayRecord>, usize) {
    let mut out = Vec::new();
    let mut skipped = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match parse_line(line) {
            Some(r) => out.push(r),
            None => skipped += 1,
        }
    }
    (out, skipped)
}

/// Parse one row from `/api/data/dayfile`: `[lineNo, "dd/mm/yy", "15.0", ...]`.
pub fn from_api_row(row: &serde_json::Value) -> Option<DayRecord> {
    let arr = row.as_array()?;
    let fields: Vec<String> = arr
        .iter()
        .skip(1)
        .map(|v| match v {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => String::new(),
            other => other.to_string(),
        })
        .collect();
    from_fields(&fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIKI_LINE: &str = "01/08/11,19.3,61,10:22,12.5,06:58,23.8,14:49,1014.26,20:46,1018.83,09:28,0.0,00:00,0.0,17.8,21.6,4.6,10:44,36,14:14,86,01:56,3.56,8.9,23.8,14:49,23.1,14:50,12.3,06:59,0.0,00:00,12.5,06:58,11.3,00:16,6.9,14:34,354,2.0,1.5";

    #[test]
    fn parses_legacy_line() {
        let r = parse_line(WIKI_LINE).unwrap();
        assert_eq!(r.date, NaiveDate::from_ymd_opt(2011, 8, 1).unwrap());
        assert_eq!(r.gust_max, Some(19.3));
        assert_eq!(r.gust_bearing, Some(61.0));
        assert_eq!(r.temp_min, Some(12.5));
        assert_eq!(r.temp_max, Some(23.8));
        assert_eq!(r.temp_max_time.as_deref(), Some("14:49"));
        assert_eq!(r.press_min, Some(1014.26));
        assert_eq!(r.rain, Some(0.0));
        assert_eq!(r.temp_avg, Some(17.8));
        assert_eq!(r.hum_min, Some(36.0));
        assert_eq!(r.dew_max, Some(11.3));
        assert_eq!(r.dew_min, Some(6.9));
        assert_eq!(r.wind_dir_dominant, Some(354.0));
        // Line predates solar/UV fields
        assert_eq!(r.solar_max, None);
        assert_eq!(r.uv_max, None);
    }

    #[test]
    fn parses_api_row() {
        let row: serde_json::Value = serde_json::from_str(r#"[3515, "28/08/19", "14.0", "225", "09:46", "10.1", "23:40", "16.7", "00:00", "1010.19", "07:50", "1015.14", "23:53", "9.8", "11:24", "3.4", "14.7", "50.6", "6.0", "16:37", "87", "07:41", "97", "23:43", "0.41", "1.6", "16.7", "00:00", "18.5", "00:00", "9.4", "23:56", "2.6", "12:01", "10.1", "23:40", "15.8", "00:48", "9.4", "23:40", "220", "1.0", "0.0", "123", "12:38", "0.9", "12:39", "", "", "", "", "", ""]"#).unwrap();
        let r = from_api_row(&row).unwrap();
        assert_eq!(r.date, NaiveDate::from_ymd_opt(2019, 8, 28).unwrap());
        assert_eq!(r.rain, Some(3.4));
        assert_eq!(r.rain_rate_max, Some(9.8));
        assert_eq!(r.rain_hour_max, Some(2.6));
        assert_eq!(r.wind_dir_dominant, Some(220.0));
        assert_eq!(r.solar_max, Some(123.0));
        assert_eq!(r.uv_max, Some(0.9));
        assert_eq!(r.feels_max, None);
    }

    #[test]
    fn handles_semicolons_and_decimal_commas() {
        let r = parse_line("05.01.24;12,5;180;14:00;8,1;05:00;21,4;15:00").unwrap();
        assert_eq!(r.date, NaiveDate::from_ymd_opt(2024, 1, 5).unwrap());
        assert_eq!(r.gust_max, Some(12.5));
        assert_eq!(r.temp_max, Some(21.4));
        assert_eq!(r.rain, None);
    }

    #[test]
    fn date_variants() {
        assert_eq!(parse_date("2024-02-29"), NaiveDate::from_ymd_opt(2024, 2, 29));
        assert_eq!(parse_date("29-02-2024"), NaiveDate::from_ymd_opt(2024, 2, 29));
        assert_eq!(parse_date("01/01/99"), NaiveDate::from_ymd_opt(1999, 1, 1));
        assert_eq!(parse_date("31/02/24"), None);
        assert_eq!(parse_date("garbage"), None);
    }

    #[test]
    fn file_skips_bad_lines() {
        let text = format!("{WIKI_LINE}\n\nnot a line\r\n{WIKI_LINE}\r\n");
        let (rows, skipped) = parse_file(&text);
        assert_eq!(rows.len(), 2);
        assert_eq!(skipped, 1);
    }

    #[test]
    fn missing_markers_are_none() {
        let r = parse_line("01/01/24,---,,--,5.0").unwrap();
        assert_eq!(r.gust_max, None);
        assert_eq!(r.temp_min, Some(5.0));
    }
}
