//! The months ahead, from the station's own history: a pattern analysis,
//! not a weather forecast.
//!
//! The last three full months are compared with the same three months in
//! every other year. The years most like them (by how warm and how wet they
//! were against normal) are the analogues, and what followed in those years
//! is the outlook. Temperatures are taken against the station's long-term
//! trend, so a warming station's early years still compare fairly, and the
//! trend is added back for the months ahead. The same method is tried on
//! every past year, leaving that year out, to measure how often it would
//! have been right: the hit rate.

use std::collections::BTreeMap;

use chrono::Datelike;

use crate::dayfile::DayRecord;

/// Months need this many days on record to count.
const FULL: usize = 25;
/// How many analogue years to take.
const ANALOGUES: usize = 5;
/// Months in the window compared with other years.
const WINDOW: i32 = 3;
/// Rain within this share of normal, and temperatures within this many
/// degrees, count as near normal.
pub const NEAR_RAIN: f64 = 0.15;
pub const NEAR_TEMP: f64 = 0.3;

/// A month as a count from year 0, so months can be added and compared.
type Mi = i32;

fn mi(year: i32, month: u32) -> Mi {
    year * 12 + month as i32 - 1
}

fn ym(m: Mi) -> (i32, u32) {
    (m.div_euclid(12), (m.rem_euclid(12) + 1) as u32)
}

#[derive(Debug, Clone, Copy)]
struct Month {
    temp: f64,
    rain: f64,
}

/// One past year like this one.
#[derive(Debug, Clone, PartialEq)]
pub struct Analogue {
    /// The year its window ended in.
    pub year: i32,
    /// How far it was from now (smaller is closer), in standard deviations.
    pub distance: f64,
    /// Its window against normal: degrees, and rain as a share of normal.
    pub temp: f64,
    pub rain: f64,
}

/// One month ahead.
#[derive(Debug, Clone, PartialEq)]
pub struct AheadMonth {
    pub year: i32,
    pub month: u32,
    /// The analogues' middle value: degrees against normal (trend
    /// included), and rain as a share of normal.
    pub temp: Option<f64>,
    pub rain: Option<f64>,
    /// How many analogues were warmer or colder than normal, wetter or
    /// drier, and how many had the month on record.
    pub warmer: usize,
    pub colder: usize,
    pub wetter: usize,
    pub drier: usize,
    pub n: usize,
    /// The station's normal for the month: mean temperature, rain total.
    pub normal_temp: f64,
    pub normal_rain: f64,
}

/// A stretch ahead summed up: the next 3, 6 or 12 months.
#[derive(Debug, Clone, PartialEq)]
pub struct Stretch {
    pub months: usize,
    pub temp: Option<f64>,
    pub rain: Option<f64>,
    /// Analogues warmer and wetter than normal over the whole stretch.
    pub warmer: usize,
    pub wetter: usize,
    pub n: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Skill {
    pub temp_hits: usize,
    pub temp_tries: usize,
    pub rain_hits: usize,
    pub rain_tries: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outlook {
    /// The window: its last month, and how it compared with normal.
    pub window_end: (i32, u32),
    pub recent_temp: f64,
    pub recent_rain: f64,
    pub analogues: Vec<Analogue>,
    pub months: Vec<AheadMonth>,
    pub stretches: Vec<Stretch>,
    /// Degrees a decade, from each year's mean against normal.
    pub trend: Option<f64>,
    /// How often the method was right about the next three months.
    pub skill: Option<Skill>,
    /// Years with full months on record.
    pub years: usize,
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    })
}

struct Model {
    months: BTreeMap<Mi, Month>,
    /// Normal mean temperature and rain for each calendar month.
    normal: [Option<(f64, f64)>; 12],
    /// Degrees a month, and the month the trend is centred on.
    slope: f64,
    centre: f64,
}

impl Model {
    fn new(days: &[DayRecord]) -> Option<Model> {
        let mut sums: BTreeMap<Mi, (usize, f64, usize, f64)> = BTreeMap::new();
        for d in days {
            let e = sums.entry(mi(d.date.year(), d.date.month())).or_default();
            let mean = d
                .temp_avg
                .or_else(|| Some((d.temp_max? + d.temp_min?) / 2.0));
            if let Some(t) = mean {
                e.0 += 1;
                e.1 += t;
            }
            if let Some(r) = d.rain {
                e.2 += 1;
                e.3 += r;
            }
        }
        let months: BTreeMap<Mi, Month> = sums
            .into_iter()
            .filter(|(_, s)| s.0 >= FULL && s.2 >= FULL)
            .map(|(m, s)| {
                (
                    m,
                    Month {
                        temp: s.1 / s.0 as f64,
                        rain: s.3,
                    },
                )
            })
            .collect();
        let mut normal = [None; 12];
        for (k, slot) in normal.iter_mut().enumerate() {
            let these: Vec<&Month> = months
                .iter()
                .filter(|(m, _)| m.rem_euclid(12) as usize == k)
                .map(|(_, v)| v)
                .collect();
            if these.len() >= 2 {
                let n = these.len() as f64;
                *slot = Some((
                    these.iter().map(|m| m.temp).sum::<f64>() / n,
                    these.iter().map(|m| m.rain).sum::<f64>() / n,
                ));
            }
        }
        // A straight line through the monthly temperature anomalies.
        let pts: Vec<(f64, f64)> = months
            .iter()
            .filter_map(|(m, v)| Some((*m as f64, v.temp - normal[m.rem_euclid(12) as usize]?.0)))
            .collect();
        let n = pts.len() as f64;
        let (slope, centre) = if n >= 24.0 {
            let mx = pts.iter().map(|p| p.0).sum::<f64>() / n;
            let my = pts.iter().map(|p| p.1).sum::<f64>() / n;
            let sxx: f64 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
            let sxy: f64 = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
            ((if sxx > 0.0 { sxy / sxx } else { 0.0 }), mx)
        } else {
            (0.0, 0.0)
        };
        Some(Model {
            months,
            normal,
            slope,
            centre,
        })
    }

    fn trend_at(&self, m: Mi) -> f64 {
        self.slope * (m as f64 - self.centre)
    }

    /// The month against normal, its temperature with the trend taken out.
    fn anomaly(&self, m: Mi) -> Option<(f64, f64)> {
        let v = self.months.get(&m)?;
        let (nt, nr) = self.normal[m.rem_euclid(12) as usize]?;
        Some((
            v.temp - nt - self.trend_at(m),
            if nr > 0.0 { v.rain / nr } else { 1.0 },
        ))
    }

    /// A window ending at `end` against normal: mean detrended anomaly and
    /// rain as a share of normal, if every month is on record.
    fn window(&self, end: Mi) -> Option<(f64, f64)> {
        let mut t = 0.0;
        let (mut r, mut nr) = (0.0, 0.0);
        for m in end - WINDOW + 1..=end {
            let (a, _) = self.anomaly(m)?;
            t += a;
            r += self.months.get(&m)?.rain;
            nr += self.normal[m.rem_euclid(12) as usize]?.1;
        }
        Some((t / WINDOW as f64, if nr > 0.0 { r / nr } else { 1.0 }))
    }

    /// Spreads, for weighing temperature and rain against each other.
    fn spreads(&self) -> (f64, f64) {
        let mut t = Vec::new();
        let mut r = Vec::new();
        for m in self.months.keys() {
            if let Some((a, b)) = self.window(*m) {
                t.push(a);
                r.push((b + 0.05).ln());
            }
        }
        let sd = |v: &[f64]| {
            let n = v.len().max(1) as f64;
            let m = v.iter().sum::<f64>() / n;
            (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / n)
                .sqrt()
                .max(1e-6)
        };
        (sd(&t), sd(&r))
    }

    /// The closest windows ending in the same calendar month as `end`, in
    /// other years (and not `skip`).
    fn analogues(&self, end: Mi, skip: Option<i32>, spreads: (f64, f64)) -> Vec<Analogue> {
        let Some((t0, r0)) = self.window(end) else {
            return Vec::new();
        };
        let mut out: Vec<Analogue> = Vec::new();
        let mut m = end - 12;
        let first = self.months.keys().next().copied().unwrap_or(end);
        while m >= first {
            let year = ym(m).0;
            if Some(year) != skip {
                if let Some((t, r)) = self.window(m) {
                    let d = (((t - t0) / spreads.0).powi(2)
                        + (((r + 0.05).ln() - (r0 + 0.05).ln()) / spreads.1).powi(2))
                    .sqrt();
                    out.push(Analogue {
                        year,
                        distance: d,
                        temp: t,
                        rain: r,
                    });
                }
            }
            m -= 12;
        }
        // Later years too, for the hindcast.
        let last = self.months.keys().next_back().copied().unwrap_or(end);
        let mut m = end + 12;
        while m <= last {
            let year = ym(m).0;
            if Some(year) != skip {
                if let Some((t, r)) = self.window(m) {
                    let d = (((t - t0) / spreads.0).powi(2)
                        + (((r + 0.05).ln() - (r0 + 0.05).ln()) / spreads.1).powi(2))
                    .sqrt();
                    out.push(Analogue {
                        year,
                        distance: d,
                        temp: t,
                        rain: r,
                    });
                }
            }
            m += 12;
        }
        out.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        out.truncate(ANALOGUES);
        out
    }

    /// What followed `ahead` months after each analogue's window.
    fn followed(&self, end: Mi, analogues: &[Analogue], ahead: i32) -> Vec<(f64, f64)> {
        analogues
            .iter()
            .filter_map(|a| {
                let their_end = end + (a.year - ym(end).0) * 12;
                self.anomaly(their_end + ahead)
            })
            .collect()
    }

    /// A stretch of `months` after an analogue's window, against normal:
    /// mean detrended temperature, rain as a share of normal.
    fn stretch_of(&self, end: Mi, months: i32) -> Option<(f64, f64)> {
        let mut t = Vec::new();
        let (mut r, mut nr) = (0.0, 0.0);
        for k in 1..=months {
            let m = end + k;
            if let (Some((a, _)), Some(v), Some(n)) = (
                self.anomaly(m),
                self.months.get(&m),
                self.normal[m.rem_euclid(12) as usize],
            ) {
                t.push(a);
                r += v.rain;
                nr += n.1;
            }
        }
        // Most of the stretch has to be on record.
        if (t.len() as i32) * 3 < months * 2 || nr <= 0.0 {
            return None;
        }
        Some((t.iter().sum::<f64>() / t.len() as f64, r / nr))
    }
}

/// The outlook from the last full month on record, or None without at
/// least four years of full months to compare.
pub fn outlook(days: &[DayRecord]) -> Option<Outlook> {
    let model = Model::new(days)?;
    let years = {
        let mut y: Vec<i32> = model.months.keys().map(|m| ym(*m).0).collect();
        y.dedup();
        y.len()
    };
    if years < 4 {
        return None;
    }
    // The last month with its whole window on record.
    let end = *model
        .months
        .keys()
        .rev()
        .find(|m| model.window(**m).is_some())?;
    let spreads = model.spreads();
    let analogues = model.analogues(end, Some(ym(end).0), spreads);
    if analogues.len() < 3 {
        return None;
    }
    let (rt, rr) = model.window(end)?;
    let months: Vec<AheadMonth> = (1..=12)
        .filter_map(|k| {
            let target = end + k;
            let (year, month) = ym(target);
            let (nt, nr) = model.normal[target.rem_euclid(12) as usize]?;
            let f = model.followed(end, &analogues, k);
            let trend = model.trend_at(target);
            Some(AheadMonth {
                year,
                month,
                temp: median(f.iter().map(|x| x.0).collect()).map(|t| t + trend),
                rain: median(f.iter().map(|x| x.1).collect()),
                warmer: f.iter().filter(|x| x.0 + trend > NEAR_TEMP).count(),
                colder: f.iter().filter(|x| x.0 + trend < -NEAR_TEMP).count(),
                wetter: f.iter().filter(|x| x.1 > 1.0 + NEAR_RAIN).count(),
                drier: f.iter().filter(|x| x.1 < 1.0 - NEAR_RAIN).count(),
                n: f.len(),
                normal_temp: nt,
                normal_rain: nr,
            })
        })
        .collect();
    let stretches = [3, 6, 12]
        .iter()
        .map(|&len| {
            let trend = model.trend_at(end + (len + 1) / 2);
            let f: Vec<(f64, f64)> = analogues
                .iter()
                .filter_map(|a| model.stretch_of(end + (a.year - ym(end).0) * 12, len))
                .collect();
            Stretch {
                months: len as usize,
                temp: median(f.iter().map(|x| x.0).collect()).map(|t| t + trend),
                rain: median(f.iter().map(|x| x.1).collect()),
                warmer: f.iter().filter(|x| x.0 + trend > 0.0).count(),
                wetter: f.iter().filter(|x| x.1 > 1.0).count(),
                n: f.len(),
            }
        })
        .collect();
    Some(Outlook {
        window_end: ym(end),
        recent_temp: rt + model.trend_at(end - 1),
        recent_rain: rr,
        analogues,
        months,
        stretches,
        trend: (model.slope != 0.0).then_some(model.slope * 120.0),
        skill: hindcast(&model, spreads),
        years,
    })
}

/// Try the method on every past window, leaving its own year out: was the
/// next three months' temperature (against trend) and rain on the side of
/// normal the analogues said?
fn hindcast(model: &Model, spreads: (f64, f64)) -> Option<Skill> {
    let mut s = Skill {
        temp_hits: 0,
        temp_tries: 0,
        rain_hits: 0,
        rain_tries: 0,
    };
    for &end in model.months.keys() {
        if model.window(end).is_none() {
            continue;
        }
        let Some((at, ar)) = model.stretch_of(end, 3) else {
            continue;
        };
        let analogues = model.analogues(end, Some(ym(end).0), spreads);
        if analogues.len() < 3 {
            continue;
        }
        let f: Vec<(f64, f64)> = analogues
            .iter()
            .filter_map(|a| model.stretch_of(end + (a.year - ym(end).0) * 12, 3))
            .collect();
        let (Some(pt), Some(pr)) = (
            median(f.iter().map(|x| x.0).collect()),
            median(f.iter().map(|x| x.1).collect()),
        ) else {
            continue;
        };
        s.temp_tries += 1;
        s.temp_hits += ((pt >= 0.0) == (at >= 0.0)) as usize;
        s.rain_tries += 1;
        s.rain_hits += ((pr >= 1.0) == (ar >= 1.0)) as usize;
    }
    (s.temp_tries >= 12).then_some(s)
}

/// "Warmer", "Cooler" or "Near normal", for a temperature against normal.
pub fn temp_words(t: f64) -> &'static str {
    if t > NEAR_TEMP {
        "Warmer than normal"
    } else if t < -NEAR_TEMP {
        "Cooler than normal"
    } else {
        "Near normal"
    }
}

/// "Wetter", "Drier" or "Near normal", for rain as a share of normal.
pub fn rain_words(r: f64) -> &'static str {
    if r > 1.0 + NEAR_RAIN {
        "Wetter than normal"
    } else if r < 1.0 - NEAR_RAIN {
        "Drier than normal"
    } else {
        "Near normal"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, NaiveDate};

    /// Ten years in which a wet, warm winter is always followed by a wet,
    /// warm spring, and a dry, cool one by a dry, cool spring.
    fn years() -> Vec<DayRecord> {
        let mut out = Vec::new();
        let mut d = NaiveDate::from_ymd_opt(2015, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 8, 31).unwrap();
        while d <= end {
            let wet = d.year() % 2 == 0;
            let season = (d.ordinal() as f64 / 365.0 * std::f64::consts::TAU).cos() * 6.0;
            let shift = if (6..=11).contains(&d.month()) {
                if wet {
                    1.5
                } else {
                    -1.5
                }
            } else {
                0.0
            };
            let rain = if (6..=11).contains(&d.month()) {
                if wet {
                    4.0
                } else {
                    1.0
                }
            } else {
                2.0
            };
            out.push(DayRecord {
                date: d,
                temp_max: Some(20.0 + season + shift + 5.0),
                temp_min: Some(20.0 + season + shift - 5.0),
                rain: Some(rain),
                ..Default::default()
            });
            d += Duration::days(1);
        }
        out
    }

    #[test]
    fn finds_analogues_and_what_followed() {
        let o = outlook(&years()).unwrap();
        // 2024 is a wet year: its winter (Jun to Aug) was wet and warm.
        assert_eq!(o.window_end, (2024, 8));
        assert!(o.recent_rain > 1.2, "{}", o.recent_rain);
        // There are four other wet years, so the fifth closest is a dry one.
        assert!(
            o.analogues[..4].iter().all(|a| a.year % 2 == 0),
            "the closest years are the other wet ones: {:?}",
            o.analogues.iter().map(|a| a.year).collect::<Vec<_>>()
        );
        let sep = &o.months[0];
        assert_eq!((sep.year, sep.month), (2024, 9));
        assert!(sep.rain.unwrap() > 1.2 && sep.wetter == 4);
        assert!(sep.temp.unwrap() > NEAR_TEMP);
        assert_eq!(rain_words(sep.rain.unwrap()), "Wetter than normal");
        let three = &o.stretches[0];
        assert_eq!(three.months, 3);
        assert_eq!(three.wetter, 4);
        // Only windows ending in winter carry the pattern, so the hindcast
        // beats a coin toss without always being right.
        let s = o.skill.unwrap();
        assert!(s.rain_hits * 10 >= s.rain_tries * 6, "{s:?}");
        assert!(outlook(&years()[..900]).is_none(), "under four years");
    }
}
