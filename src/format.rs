//! Text for numbers, dates and times. Dates are day first ("14 Oct 2026").

use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{Datelike, Local, NaiveDate, TimeZone, Timelike};

pub const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
pub const MONTHS_LONG: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// "12.5", or "–" when missing.
pub fn num(v: Option<f64>, digits: usize) -> String {
    match v {
        Some(x) if x.is_finite() => {
            let s = format!("{x:.digits$}");
            // Avoid "-0.0".
            if s.trim_start_matches('-')
                .chars()
                .all(|c| c == '0' || c == '.')
            {
                s.trim_start_matches('-').to_string()
            } else {
                s.replace('-', "−")
            }
        }
        _ => "–".into(),
    }
}

/// "+1.2" or "−1.2", or "–" when missing.
pub fn signed(v: Option<f64>, digits: usize) -> String {
    match v {
        Some(x) if x.is_finite() => {
            let s = num(Some(x), digits);
            if x > 0.0 && s.chars().any(|c| c.is_ascii_digit() && c != '0') {
                format!("+{s}")
            } else {
                s
            }
        }
        _ => "–".into(),
    }
}

/// "14 Oct 2026".
pub fn nice_date(d: NaiveDate) -> String {
    format!("{} {} {}", d.day(), MONTHS[d.month0() as usize], d.year())
}

/// "14 Oct".
pub fn short_date(d: NaiveDate) -> String {
    format!("{} {}", d.day(), MONTHS[d.month0() as usize])
}

/// "October 2026".
pub fn month_year(d: NaiveDate) -> String {
    format!("{} {}", MONTHS_LONG[d.month0() as usize], d.year())
}

static CLOCK_24H: AtomicBool = AtomicBool::new(true);

/// Choose 24-hour ("16:52") or 12-hour ("4:52 PM") times everywhere.
pub fn set_24h(on: bool) {
    CLOCK_24H.store(on, Ordering::Relaxed);
}

fn is_24h() -> bool {
    CLOCK_24H.load(Ordering::Relaxed)
}

fn hm(h: u32, m: u32) -> String {
    if is_24h() {
        format!("{h:02}:{m:02}")
    } else {
        let h12 = if h.is_multiple_of(12) { 12 } else { h % 12 };
        format!("{h12}:{m:02} {}", if h < 12 { "AM" } else { "PM" })
    }
}

/// An hour on a chart axis: "15:00", or "3pm".
pub fn axis_hour(h: u32) -> String {
    if is_24h() {
        format!("{h:02}:00")
    } else {
        format!(
            "{}{}",
            if h.is_multiple_of(12) { 12 } else { h % 12 },
            if h < 12 { "am" } else { "pm" }
        )
    }
}

/// Hour and minute from a station's time of day: "4:52 AM", "16:52" or
/// "16:52:07".
pub fn parse_hm(s: &str) -> Option<(u32, u32)> {
    let lower = s.trim().to_ascii_lowercase();
    let (body, pm) = if let Some(b) = lower
        .strip_suffix("pm")
        .or_else(|| lower.strip_suffix("p.m."))
    {
        (b.trim(), Some(true))
    } else if let Some(b) = lower
        .strip_suffix("am")
        .or_else(|| lower.strip_suffix("a.m."))
    {
        (b.trim(), Some(false))
    } else {
        (lower.as_str(), None)
    };
    let mut parts = body.split(':');
    let h: u32 = parts.next()?.trim().parse().ok()?;
    let m: u32 = parts.next()?.trim().parse().ok()?;
    let h = match pm {
        Some(true) if h < 12 => h + 12,
        Some(false) if h == 12 => 0,
        _ => h,
    };
    (h <= 23 && m <= 59).then_some((h, m))
}

/// A station's time of day ("4:52 AM", "16:52" or "16:52:07") in the chosen
/// clock. Text that isn't a time comes back as it was.
pub fn clock(s: &str) -> String {
    match parse_hm(s) {
        Some((h, m)) => hm(h, m),
        None => s.trim().to_string(),
    }
}

/// The time between two station times of day: "13 h 09 min".
pub fn span_between(from: &str, to: &str) -> Option<String> {
    let (a, b) = (parse_hm(from)?, parse_hm(to)?);
    let mins = (b.0 * 60 + b.1) as i64 - (a.0 * 60 + a.1) as i64;
    (mins > 0).then(|| format!("{} h {:02} min", mins / 60, mins % 60))
}

/// "12 Oct, 02:02" for a station timestamp like "2026-10-12 02:02", with
/// how long ago it was: "6 days ago".
pub fn station_moment(iso: &str, now_ms: i64) -> Option<(String, String)> {
    let t = chrono::NaiveDateTime::parse_from_str(iso.trim(), "%Y-%m-%d %H:%M").ok()?;
    let ms = Local.from_local_datetime(&t).earliest()?.timestamp_millis();
    Some((
        format!("{}, {}", short_date(t.date()), hm(t.hour(), t.minute())),
        ago(Some(ms), now_ms),
    ))
}

/// "14:05 · 16 Oct" for a Unix-ms timestamp, in local time.
pub fn time_and_date(ms: i64) -> String {
    match Local.timestamp_millis_opt(ms).single() {
        Some(t) => format!(
            "{} · {}",
            hm(t.hour(), t.minute()),
            short_date(t.date_naive())
        ),
        None => "–".into(),
    }
}

/// "just now", "12 s ago", "3 min ago", "5 h ago", "4 days ago" or "never".
pub fn ago(ms: Option<i64>, now: i64) -> String {
    let Some(ms) = ms.filter(|m| *m > 0) else {
        return "never".into();
    };
    let s = ((now - ms) as f64 / 1000.0).round().max(0.0) as i64;
    if s < 5 {
        return "just now".into();
    }
    if s < 60 {
        return format!("{s} s ago");
    }
    let m = (s as f64 / 60.0).round() as i64;
    if m < 60 {
        return format!("{m} min ago");
    }
    let h = (m as f64 / 60.0).round() as i64;
    if h < 48 {
        return format!("{h} h ago");
    }
    format!("{} days ago", (h as f64 / 24.0).round() as i64)
}

/// The same day `n` years away, clamped to the month's end (29 Feb → 28 Feb).
pub fn add_years(d: NaiveDate, n: i32) -> NaiveDate {
    let y = d.year() + n;
    (1..=d.day())
        .rev()
        .find_map(|day| NaiveDate::from_ymd_opt(y, d.month(), day))
        .unwrap_or(d)
}

/// The text with its first letter in capitals, for error messages that
/// start a sentence.
pub fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

pub fn first_of_month(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap_or(d)
}

pub fn first_of_year(d: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(d.year(), 1, 1).unwrap_or(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(num(Some(12.345), 1), "12.3");
        assert_eq!(num(Some(-2.9), 1), "−2.9");
        assert_eq!(num(Some(-0.01), 1), "0.0");
        assert_eq!(num(None, 1), "–");
        assert_eq!(signed(Some(1.24), 1), "+1.2");
        assert_eq!(signed(Some(-1.5), 1), "−1.5");
        assert_eq!(signed(Some(0.01), 1), "0.0");
    }

    #[test]
    fn clocks() {
        // Both settings in one test, since the choice is global.
        set_24h(true);
        assert_eq!(clock("4:52 AM"), "04:52");
        assert_eq!(clock("12:00 AM"), "00:00");
        assert_eq!(clock("12:30 pm"), "12:30");
        assert_eq!(clock("7:45 PM"), "19:45");
        assert_eq!(clock("16:52:07"), "16:52");
        assert_eq!(clock("-----"), "-----");
        assert_eq!(
            span_between("6:36 AM", "7:45 PM").as_deref(),
            Some("13 h 09 min")
        );
        assert_eq!(span_between("7:45 PM", "6:36 AM"), None);
        let now = Local
            .with_ymd_and_hms(2026, 10, 18, 9, 0, 0)
            .unwrap()
            .timestamp_millis();
        assert_eq!(
            station_moment("2026-10-12 02:02", now),
            Some(("12 Oct, 02:02".to_string(), "6 days ago".to_string()))
        );
        assert_eq!(axis_hour(15), "15:00");
        set_24h(false);
        assert_eq!(clock("16:52"), "4:52 PM");
        assert_eq!(clock("00:10"), "12:10 AM");
        assert_eq!(axis_hour(0), "12am");
        assert_eq!(axis_hour(15), "3pm");
        set_24h(true);
    }

    #[test]
    fn dates() {
        let d = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        assert_eq!(nice_date(d), "29 Feb 2024");
        assert_eq!(
            add_years(d, -1),
            NaiveDate::from_ymd_opt(2023, 2, 28).unwrap()
        );
        assert_eq!(ago(Some(1000), 4000), "just now");
        assert_eq!(ago(Some(1000), 1000 + 3 * 3_600_000), "3 h ago");
        assert_eq!(ago(None, 0), "never");
    }
}
