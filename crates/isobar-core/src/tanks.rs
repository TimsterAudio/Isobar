//! Rainwater tanks: what the roof catches, what the house uses, and how full
//! the tanks are, modelled day by day from the rain.
//!
//! The model is the one tank owners keep in a spreadsheet: each day the level
//! goes up by the rain times the roof area (a millimetre on a square metre is
//! a litre), down by the day's use, and anything above the tanks' capacity
//! overflows. A day of 0.1 mm or less only wets the roof and catches nothing.
//! A known level (the tanks seen full, or a measured depth) resets it.

use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::path::Path;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

use crate::dayfile::{self, DayRecord};

/// Rain at or below this wets the roof and catches nothing.
pub const FIRST_FLUSH_MM: f64 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownLevel {
    pub date: NaiveDate,
    /// Litres in the tanks at the end of that day.
    pub litres: f64,
}

/// The most tanks Settings offers.
pub const MAX_TANKS: usize = 4;

/// One tank.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tank {
    /// Litres it holds.
    pub litres: f64,
    /// Inside diameter in mm, for working out litres from a depth.
    pub diameter_mm: Option<f64>,
}

/// The tanks and the house, as set in Settings. The tanks are taken as
/// linked, filling and emptying together.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Tanks {
    /// Roof area draining to the tanks, m².
    pub roof_area: f64,
    pub tanks: Vec<Tank>,
    /// Litres the house uses a day.
    pub use_per_day: f64,
    /// The level to stay above, litres (where the pump draws air, say).
    pub keep_above: f64,
    pub known: Option<KnownLevel>,
}

impl Tanks {
    pub fn capacity(&self) -> f64 {
        self.tanks.iter().map(|t| t.litres).sum()
    }

    /// Enough is set to run the model.
    pub fn is_set_up(&self) -> bool {
        self.roof_area > 0.0 && self.capacity() > 0.0
    }

    /// Litres across all the tanks with water `depth_mm` deep in each (they
    /// sit level and are linked), each no more than it holds. Needs every
    /// tank's diameter.
    pub fn litres_at_depth(&self, depth_mm: f64) -> Option<f64> {
        self.tanks
            .iter()
            .map(|t| {
                let r = t.diameter_mm? / 2.0;
                Some((PI * r * r * depth_mm / 1_000_000.0).min(t.litres))
            })
            .sum()
    }

    /// Litres a day of rain puts in the tanks.
    pub fn caught(&self, rain_mm: f64) -> f64 {
        if rain_mm <= FIRST_FLUSH_MM + 1e-9 {
            0.0
        } else {
            rain_mm * self.roof_area
        }
    }
}

/// One day from an imported tank record (a spreadsheet the owner kept).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogDay {
    pub date: NaiveDate,
    /// Rain in mm.
    pub rain: Option<f64>,
    /// Litres used that day.
    pub used: Option<f64>,
    /// Litres added or taken out by hand (a delivery, a leak, blocked filters).
    pub adjust: Option<f64>,
}

/// What the model takes for one day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Input {
    pub date: NaiveDate,
    /// Rain in mm, or none when there's no record of the day.
    pub rain: Option<f64>,
    pub used: Option<f64>,
    pub adjust: f64,
    /// The rain came from the station rather than the imported record.
    pub station: bool,
}

/// Every day from the first record to the last, the station's rain where it
/// has the day and the imported record's otherwise. `inches` converts the
/// station's rain to mm.
pub fn inputs(days: &[DayRecord], log: &[LogDay], inches: bool) -> Vec<Input> {
    let scale = if inches { 25.4 } else { 1.0 };
    let station: BTreeMap<NaiveDate, f64> = days
        .iter()
        .filter_map(|d| d.rain.map(|r| (d.date, r * scale)))
        .collect();
    let logged: BTreeMap<NaiveDate, &LogDay> = log.iter().map(|l| (l.date, l)).collect();
    let first = [station.keys().next(), logged.keys().next()]
        .into_iter()
        .flatten()
        .min()
        .copied();
    let last = [
        station.keys().next_back(),
        logged
            .iter()
            .rev()
            .find(|(_, l)| l.rain.is_some())
            .map(|(d, _)| d),
    ]
    .into_iter()
    .flatten()
    .max()
    .copied();
    let (Some(first), Some(last)) = (first, last) else {
        return Vec::new();
    };
    // Spreadsheets leave dry days blank: a row with no rain, up to the last
    // row with some, is a dry day.
    let log_last_rain = logged
        .iter()
        .rev()
        .find(|(_, l)| l.rain.is_some())
        .map(|(d, _)| *d);
    let mut out = Vec::with_capacity((last - first).num_days() as usize + 1);
    let mut d = first;
    while d <= last {
        let l = logged.get(&d);
        let (rain, from_station) = match station.get(&d) {
            Some(r) => (Some(*r), true),
            None => (
                l.and_then(|l| match l.rain {
                    None if log_last_rain.is_some_and(|r| d <= r) => Some(0.0),
                    r => r,
                }),
                false,
            ),
        };
        out.push(Input {
            date: d,
            rain,
            used: l.and_then(|l| l.used),
            adjust: l.and_then(|l| l.adjust).unwrap_or(0.0),
            station: from_station,
        });
        d += Duration::days(1);
    }
    out
}

/// One day of the model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TankDay {
    pub date: NaiveDate,
    pub rain: f64,
    pub caught: f64,
    pub used: f64,
    pub overflow: f64,
    /// Litres at the end of the day.
    pub level: f64,
}

/// Run the model over `inputs`, starting from full (a guess that sorts
/// itself out at the first overflow) and resetting to the known level.
pub fn simulate(inputs: &[Input], t: &Tanks) -> Vec<TankDay> {
    let cap = t.capacity();
    let mut level = cap;
    inputs
        .iter()
        .map(|i| {
            let rain = i.rain.unwrap_or(0.0);
            let caught = t.caught(rain);
            let used = i.used.unwrap_or(t.use_per_day);
            let before = level + caught - used;
            let overflow = (before - cap).max(0.0);
            level = (before + i.adjust).clamp(0.0, cap);
            if let Some(k) = t.known.filter(|k| k.date == i.date) {
                level = k.litres.clamp(0.0, cap);
            }
            TankDay {
                date: i.date,
                rain,
                caught,
                used,
                overflow,
                level,
            }
        })
        .collect()
}

/// Today so far, after the last finished day on record: the rain the
/// station has measured since its day began (`rain_mm`), and the day's use
/// in proportion to the time gone (`used` for the whole day if a record has
/// it, else the usual figure). None when the record already reaches today,
/// is more than a day behind, or a known level is set for today or later.
pub fn today(
    last: &TankDay,
    now: NaiveDateTime,
    rain_mm: Option<f64>,
    used: Option<f64>,
    t: &Tanks,
) -> Option<TankDay> {
    let date = last.date + Duration::days(1);
    // A station whose day starts at 9am is still on yesterday's date until
    // then, so its "today" can be a calendar day behind.
    if !(0..=1).contains(&(now.date() - date).num_days()) {
        return None;
    }
    if t.known.is_some_and(|k| k.date >= date) {
        return None;
    }
    let gone = (now - date.and_hms_opt(0, 0, 0)?).num_minutes() as f64 / 1440.0;
    let rain = rain_mm.unwrap_or(0.0).max(0.0);
    let caught = t.caught(rain);
    let used = used.unwrap_or(t.use_per_day) * gone.clamp(0.0, 1.0);
    let cap = t.capacity();
    let before = last.level + caught - used;
    Some(TankDay {
        date,
        rain,
        caught,
        used,
        overflow: (before - cap).max(0.0),
        level: before.clamp(0.0, cap),
    })
}

/// Where the tanks could go from here.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Outlook {
    /// The level each day ahead with no rain at all.
    pub dry: Vec<f64>,
    /// The day the level drops to the keep-above level with no rain.
    pub dry_low: Option<NaiveDate>,
    /// Each day ahead: the lowest, middle and highest level across the
    /// past years' rain.
    pub low: Vec<f64>,
    pub mid: Vec<f64>,
    pub high: Vec<f64>,
    /// For each past year used: the day the level would reach the
    /// keep-above level with that year's rain, if it would.
    pub years: Vec<(i32, Option<NaiveDate>)>,
    /// Litres that would overflow over the days ahead with each past year's
    /// rain, in the same order as `years`.
    pub overflow: Vec<f64>,
}

/// From `level` at the end of `from`, `horizon` days ahead: with no rain,
/// and with the rain of each past year that has a full record of those
/// days.
pub fn outlook(
    inputs: &[Input],
    from: NaiveDate,
    level: f64,
    t: &Tanks,
    horizon: usize,
) -> Outlook {
    let cap = t.capacity();
    let run = |rain: &dyn Fn(usize) -> f64| -> (Vec<f64>, Option<NaiveDate>, f64) {
        let mut l = level;
        let mut low = None;
        let mut spilt = 0.0;
        let levels = (0..horizon)
            .map(|k| {
                let before = l + t.caught(rain(k)) - t.use_per_day;
                spilt += (before - cap).max(0.0);
                l = before.clamp(0.0, cap);
                if low.is_none() && l <= t.keep_above {
                    low = Some(from + Duration::days(k as i64 + 1));
                }
                l
            })
            .collect();
        (levels, low, spilt)
    };
    let (dry, dry_low, _) = run(&|_| 0.0);
    let by_date: BTreeMap<NaiveDate, f64> = inputs
        .iter()
        .filter_map(|i| i.rain.map(|r| (i.date, r)))
        .collect();
    let mut runs: Vec<Vec<f64>> = Vec::new();
    let mut years = Vec::new();
    let mut overflow = Vec::new();
    for back in 1..=60 {
        let y = from.year() - back;
        let Some(start) = from
            .with_year(y)
            .or_else(|| NaiveDate::from_ymd_opt(y, from.month(), 28))
        else {
            continue;
        };
        let rains: Vec<Option<f64>> = (1..=horizon as i64)
            .map(|k| by_date.get(&(start + Duration::days(k))).copied())
            .collect();
        let have = rains.iter().filter(|r| r.is_some()).count();
        if (have as f64) < horizon as f64 * 0.9 {
            continue;
        }
        let (levels, low, spilt) = run(&|k| rains[k].unwrap_or(0.0));
        runs.push(levels);
        years.push((y, low));
        overflow.push(spilt);
    }
    years.reverse();
    overflow.reverse();
    let (mut low, mut mid, mut high) = (Vec::new(), Vec::new(), Vec::new());
    if !runs.is_empty() {
        for k in 0..horizon {
            let mut v: Vec<f64> = runs.iter().map(|r| r[k]).collect();
            v.sort_by(f64::total_cmp);
            low.push(v[0]);
            high.push(v[v.len() - 1]);
            mid.push(if v.len() % 2 == 1 {
                v[v.len() / 2]
            } else {
                (v[v.len() / 2 - 1] + v[v.len() / 2]) / 2.0
            });
        }
    }
    Outlook {
        dry,
        dry_low,
        low,
        mid,
        high,
        years,
        overflow,
    }
}

/// A calendar year of the model.
#[derive(Debug, Clone, PartialEq)]
pub struct TankYear {
    pub year: i32,
    pub rain: f64,
    pub caught: f64,
    pub used: f64,
    pub overflow: f64,
    pub lowest: (NaiveDate, f64),
    /// Days at or below the keep-above level.
    pub days_low: usize,
    pub days: usize,
}

pub fn years(sim: &[TankDay], keep_above: f64) -> Vec<TankYear> {
    let mut out: Vec<TankYear> = Vec::new();
    for d in sim {
        if out.last().is_none_or(|y| y.year != d.date.year()) {
            out.push(TankYear {
                year: d.date.year(),
                rain: 0.0,
                caught: 0.0,
                used: 0.0,
                overflow: 0.0,
                lowest: (d.date, d.level),
                days_low: 0,
                days: 0,
            });
        }
        let y = out.last_mut().unwrap();
        y.rain += d.rain;
        y.caught += d.caught;
        y.used += d.used;
        y.overflow += d.overflow;
        y.days += 1;
        if d.level < y.lowest.1 {
            y.lowest = (d.date, d.level);
        }
        if d.level <= keep_above {
            y.days_low += 1;
        }
    }
    out
}

// ---------------------------------------------------------------- import

/// A spreadsheet or CSV cell, as the import sees it.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Empty,
    Num(f64),
    Text(String),
    Date(NaiveDate),
}

fn excel_date(serial: f64) -> Option<NaiveDate> {
    (1.0..2_958_466.0)
        .contains(&serial)
        .then(|| NaiveDate::from_ymd_opt(1899, 12, 30).unwrap() + Duration::days(serial as i64))
}

fn cell_date(c: &Cell) -> Option<NaiveDate> {
    match c {
        Cell::Date(d) => Some(*d),
        Cell::Text(t) => dayfile::parse_date(t),
        Cell::Num(n) if (20_000.0..80_000.0).contains(n) => excel_date(*n),
        _ => None,
    }
}

fn cell_num(c: Option<&Cell>) -> Option<f64> {
    match c? {
        Cell::Num(n) if n.is_finite() => Some(*n),
        Cell::Text(t) => t.trim().replace(',', ".").parse().ok(),
        _ => None,
    }
}

/// Find the columns and read the rows: a header row with "Date" and "mm"
/// (or "Rain"), and optionally "Use" (litres a day) and "Adjust" (litres
/// added or taken out). Without headers, the first column of dates and the
/// numbers beside it. Rows after `today` are left out.
pub fn parse_log(rows: &[Vec<Cell>], today: NaiveDate) -> Result<Vec<LogDay>, String> {
    let text = |c: &Cell| match c {
        Cell::Text(t) => t.trim().to_ascii_lowercase(),
        _ => String::new(),
    };
    let find =
        |row: &[Cell], names: &[&str]| row.iter().position(|c| names.contains(&text(c).as_str()));
    let mut cols = None;
    for (i, row) in rows.iter().enumerate().take(100) {
        if let (Some(d), Some(r)) = (
            find(row, &["date"]),
            find(
                row,
                &["mm", "rain", "rainfall", "rain (mm)", "rainfall (mm)"],
            ),
        ) {
            cols = Some((
                i + 1,
                d,
                r,
                find(row, &["use", "usage", "used"]),
                find(row, &["adjust", "adjustment"]),
            ));
            break;
        }
    }
    if cols.is_none() {
        'rows: for (i, row) in rows.iter().enumerate().take(100) {
            for (d, c) in row.iter().enumerate() {
                if cell_date(c).is_some() && cell_num(row.get(d + 1)).is_some() {
                    cols = Some((i, d, d + 1, None, None));
                    break 'rows;
                }
            }
        }
    }
    let Some((from, dc, rc, uc, ac)) = cols else {
        return Err("Couldn't find a column of dates with rain beside it. Isobar looks for headings \"Date\" and \"mm\" (or \"Rain\").".into());
    };
    let mut by_date: BTreeMap<NaiveDate, LogDay> = BTreeMap::new();
    for row in &rows[from.min(rows.len())..] {
        let Some(date) = row.get(dc).and_then(cell_date) else {
            continue;
        };
        if date > today {
            continue;
        }
        let day = LogDay {
            date,
            rain: cell_num(row.get(rc)).filter(|r| *r >= 0.0),
            used: uc.and_then(|c| cell_num(row.get(c))).filter(|u| *u >= 0.0),
            adjust: ac.and_then(|c| cell_num(row.get(c))).filter(|a| *a != 0.0),
        };
        if day.rain.is_some() || day.used.is_some() || day.adjust.is_some() {
            by_date.insert(date, day);
        }
    }
    if by_date.is_empty() {
        return Err(
            "Found the columns but no rows with a date and rain on or before today.".into(),
        );
    }
    Ok(by_date.into_values().collect())
}

/// Read a tank record from a spreadsheet (.xlsx, .xls, .ods) or a CSV file.
/// The first sheet with recognisable rows is used.
pub fn read_log(path: &Path, today: NaiveDate) -> Result<Vec<LogDay>, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "csv" || ext == "txt" {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("Couldn't read {}: {e}. If it's open in Excel or another program, close it there and try again.", path.display()))?;
        let rows: Vec<Vec<Cell>> = text
            .lines()
            .map(|l| {
                let sep = if l.contains(';') && !l.contains(',') {
                    ';'
                } else {
                    ','
                };
                l.split(sep)
                    .map(|f| {
                        let f = f.trim().trim_matches('"');
                        if f.is_empty() {
                            Cell::Empty
                        } else if let Ok(n) = f.parse::<f64>() {
                            Cell::Num(n)
                        } else {
                            Cell::Text(f.to_string())
                        }
                    })
                    .collect()
            })
            .collect();
        return parse_log(&rows, today);
    }
    use calamine::{open_workbook_auto, Data, Reader};
    let mut book =
        open_workbook_auto(path).map_err(|e| {
        format!(
            "Couldn't open {}: {e}. If it's open in Excel or another program, close it there and try again.",
            path.display()
        )
    })?;
    let mut last_err = String::from("The workbook has no sheets.");
    for (_, range) in book.worksheets() {
        let rows: Vec<Vec<Cell>> = range
            .rows()
            .map(|r| {
                r.iter()
                    .map(|c| match c {
                        Data::Int(i) => Cell::Num(*i as f64),
                        Data::Float(f) => Cell::Num(*f),
                        Data::String(s) => Cell::Text(s.clone()),
                        Data::DateTime(d) => excel_date(d.as_f64()).map_or(Cell::Empty, Cell::Date),
                        Data::DateTimeIso(s) => dayfile::parse_date(s.get(..10).unwrap_or(s))
                            .map_or(Cell::Empty, Cell::Date),
                        _ => Cell::Empty,
                    })
                    .collect()
            })
            .collect();
        match parse_log(&rows, today) {
            Ok(log) => return Ok(log),
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn tanks() -> Tanks {
        let tank = Tank {
            litres: 21_500.0,
            diameter_mm: Some(3530.0),
        };
        Tanks {
            roof_area: 385.0,
            tanks: vec![tank, tank],
            use_per_day: 290.0,
            keep_above: 3000.0,
            known: None,
        }
    }

    fn input(d: NaiveDate, rain: f64) -> Input {
        Input {
            date: d,
            rain: Some(rain),
            used: None,
            adjust: 0.0,
            station: true,
        }
    }

    #[test]
    fn catches_rain_like_the_spreadsheet() {
        let t = tanks();
        assert_eq!(t.capacity(), 43_000.0);
        assert_eq!(t.caught(0.1), 0.0);
        assert!((t.caught(0.2) - 77.0).abs() < 1e-9);
        // The owner's sheet: 1,350 mm deep in two 3,530 mm tanks is 26,424 L.
        assert!((t.litres_at_depth(1350.0).unwrap() - 26_424.27).abs() < 0.1);
        // Tanks of different sizes: a full small one stops at what it holds.
        let mixed = Tanks {
            tanks: vec![
                Tank {
                    litres: 22_500.0,
                    diameter_mm: Some(3530.0),
                },
                Tank {
                    litres: 5_000.0,
                    diameter_mm: Some(2000.0),
                },
            ],
            ..t.clone()
        };
        assert_eq!(mixed.capacity(), 27_500.0);
        let at = mixed.litres_at_depth(2000.0).unwrap();
        assert!((at - (PI * 1.765 * 1.765 * 2.0 * 1000.0 + 5_000.0)).abs() < 0.1);
        let no_diameter = Tanks {
            tanks: vec![Tank {
                litres: 5_000.0,
                diameter_mm: None,
            }],
            ..t
        };
        assert_eq!(no_diameter.litres_at_depth(1000.0), None);
    }

    #[test]
    fn models_the_level_and_overflow() {
        // The owner's sheet, 29 Sep to 3 Oct 2026, from 41,201.5 L.
        let mut t = tanks();
        let start = date(2026, 9, 29);
        t.known = Some(KnownLevel {
            date: start,
            litres: 41_201.5,
        });
        let rains = [0.0, 0.2, 22.0, 5.4, 13.8, 0.0];
        let ins: Vec<Input> = rains
            .iter()
            .enumerate()
            .map(|(k, r)| input(start + Duration::days(k as i64), *r))
            .collect();
        let sim = simulate(&ins, &t);
        assert_eq!(sim[0].level, 41_201.5);
        assert!((sim[1].level - 40_988.5).abs() < 1e-6);
        assert_eq!(sim[2].level, 43_000.0);
        assert!((sim[2].overflow - 6_168.5).abs() < 1e-6);
        assert!((sim[3].overflow - 1_789.0).abs() < 1e-6);
        assert!((sim[4].overflow - 5_023.0).abs() < 1e-6);
        assert!((sim[5].level - 42_710.0).abs() < 1e-6);
        let y = years(&sim, 3000.0);
        assert_eq!(y.len(), 1);
        assert!((y[0].overflow - 12_980.5).abs() < 1e-6);
    }

    #[test]
    fn looks_ahead_with_past_years() {
        let t = tanks();
        let mut ins = Vec::new();
        let mut d = date(2023, 1, 1);
        while d <= date(2026, 1, 1) {
            // 2023 is dry; 2024 and 2025 get 10 mm every tenth day.
            let wet = d.year() > 2023 && d.ordinal().is_multiple_of(10);
            ins.push(input(d, if wet { 10.0 } else { 0.0 }));
            d += Duration::days(1);
        }
        let o = outlook(&ins, date(2026, 1, 1), 20_000.0, &t, 120);
        assert_eq!(o.dry.len(), 120);
        assert_eq!(o.dry[0], 19_710.0);
        // 17,000 L above the keep-above level at 290 L a day: 59 days.
        assert_eq!(o.dry_low, Some(date(2026, 1, 1) + Duration::days(59)));
        assert_eq!(o.years.len(), 3);
        assert_eq!(
            o.years[0],
            (2023, Some(date(2026, 1, 1) + Duration::days(59)))
        );
        assert_eq!(o.years[2].1, None, "a wet year keeps it up");
        // The dry year spills nothing; a wet one spills once the tanks fill.
        assert_eq!(o.overflow, [0.0, 0.0, 0.0], "not full enough to spill");
        let near_full = outlook(&ins, date(2026, 1, 1), 42_500.0, &t, 120);
        assert_eq!(near_full.overflow[0], 0.0);
        assert!(near_full.overflow[2] > 0.0);
        assert!(o.low[119] < o.mid[119] && o.mid[119] <= o.high[119]);
    }

    #[test]
    fn merges_station_rain_over_the_record() {
        let days = vec![DayRecord {
            date: date(2026, 1, 3),
            rain: Some(4.0),
            ..Default::default()
        }];
        let log = vec![
            LogDay {
                date: date(2026, 1, 1),
                rain: Some(1.0),
                used: Some(250.0),
                adjust: None,
            },
            LogDay {
                date: date(2026, 1, 2),
                rain: None,
                used: Some(300.0),
                adjust: None,
            },
            LogDay {
                date: date(2026, 1, 3),
                rain: Some(9.0),
                used: None,
                adjust: Some(-500.0),
            },
        ];
        let i = inputs(&days, &log, false);
        assert_eq!(i.len(), 3);
        assert_eq!(
            (i[0].rain, i[0].used, i[0].station),
            (Some(1.0), Some(250.0), false)
        );
        assert_eq!(i[1].rain, Some(0.0), "a blank row is a dry day");
        assert_eq!(
            (i[2].rain, i[2].adjust, i[2].station),
            (Some(4.0), -500.0, true)
        );
    }

    #[test]
    fn reads_a_sheet_with_headings() {
        let t = |s: &str| Cell::Text(s.into());
        let rows = vec![
            vec![Cell::Empty, t("Roof Area"), Cell::Num(385.0)],
            vec![
                t("N"),
                Cell::Empty,
                t("Date"),
                t("mm"),
                t("Litres"),
                t("Use"),
                t("Balance"),
                t("Adjust"),
            ],
            vec![
                t("N"),
                Cell::Empty,
                Cell::Date(date(2026, 9, 1)),
                Cell::Num(2.3),
                Cell::Num(885.5),
                Cell::Num(290.0),
                Cell::Num(1.0),
                Cell::Empty,
            ],
            vec![
                t("N"),
                Cell::Empty,
                Cell::Num(46_267.0),
                Cell::Empty,
                Cell::Num(0.0),
                Cell::Num(290.0),
                Cell::Num(1.0),
                Cell::Num(-10_000.0),
            ],
            vec![
                t("N"),
                Cell::Empty,
                Cell::Date(date(2027, 1, 1)),
                Cell::Num(5.0),
                Cell::Empty,
                Cell::Num(290.0),
            ],
        ];
        let log = parse_log(&rows, date(2026, 10, 18)).unwrap();
        assert_eq!(log.len(), 2, "a future row is left out");
        assert_eq!(
            log[0],
            LogDay {
                date: date(2026, 9, 1),
                rain: Some(2.3),
                used: Some(290.0),
                adjust: None
            }
        );
        assert_eq!(log[1].date, date(2026, 9, 2));
        assert_eq!((log[1].rain, log[1].adjust), (None, Some(-10_000.0)));
        // No headings: a date column and the rain beside it.
        let rows = vec![
            vec![t("1/9/2026"), Cell::Num(2.0)],
            vec![t("2/9/2026"), Cell::Num(0.0)],
        ];
        assert_eq!(parse_log(&rows, date(2026, 10, 18)).unwrap().len(), 2);
        assert!(parse_log(&[vec![t("hello")]], date(2026, 10, 18)).is_err());
    }

    #[test]
    fn counts_today_so_far() {
        let t = tanks();
        let last = TankDay {
            date: date(2026, 10, 21),
            rain: 0.0,
            caught: 0.0,
            used: 290.0,
            overflow: 0.0,
            level: 30_000.0,
        };
        let at = |d: u32, h: u32| date(2026, 10, d).and_hms_opt(h, 0, 0).unwrap();
        // 6 pm with 2 mm so far: 770 L caught, three quarters of 290 L used.
        let d = today(&last, at(22, 18), Some(2.0), None, &t).unwrap();
        assert_eq!(d.date, date(2026, 10, 22));
        assert_eq!(d.caught, 770.0);
        assert_eq!(d.used, 217.5);
        assert_eq!(d.level, 30_552.5);
        // A 9am station before 9am: its day is still the 22nd, used in full.
        let d = today(&last, at(23, 7), Some(0.1), None, &t).unwrap();
        assert_eq!((d.date, d.caught, d.used), (date(2026, 10, 22), 0.0, 290.0));
        // Too far behind, or a known level today: nothing to add.
        assert!(today(&last, at(25, 7), None, None, &t).is_none());
        let known = Tanks {
            known: Some(KnownLevel {
                date: date(2026, 10, 22),
                litres: 40_000.0,
            }),
            ..tanks()
        };
        assert!(today(&last, at(22, 18), None, None, &known).is_none());
    }
}
