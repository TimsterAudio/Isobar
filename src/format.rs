//! Text for numbers, dates and times. Dates are day first ("14 Oct 2026").

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

/// "3pm", "12am".
pub fn hour12(h: u32) -> String {
    format!(
        "{}{}",
        if h.is_multiple_of(12) { 12 } else { h % 12 },
        if h < 12 { "am" } else { "pm" }
    )
}

/// "14:05 · 16 Oct" for a Unix-ms timestamp, in local time.
pub fn time_and_date(ms: i64) -> String {
    match Local.timestamp_millis_opt(ms).single() {
        Some(t) => format!(
            "{:02}:{:02} · {}",
            t.hour(),
            t.minute(),
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
    fn dates() {
        let d = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        assert_eq!(nice_date(d), "29 Feb 2024");
        assert_eq!(
            add_years(d, -1),
            NaiveDate::from_ymd_opt(2023, 2, 28).unwrap()
        );
        assert_eq!(hour12(0), "12am");
        assert_eq!(hour12(15), "3pm");
        assert_eq!(ago(Some(1000), 4000), "just now");
        assert_eq!(ago(Some(1000), 1000 + 3 * 3_600_000), "3 h ago");
        assert_eq!(ago(None, 0), "never");
    }
}
