//! Desktop notifications for notable conditions. Each alert has a cooldown
//! so a gusty afternoon produces one notification, not forty.

use chrono::{Local, NaiveDate};
use isobar_core::LiveReading;

use crate::settings::Alerts;

const GUST_COOLDOWN_MS: i64 = 3 * 60 * 60 * 1000;
const DRY_SPELL_MS: i64 = 3 * 60 * 60 * 1000;

#[derive(Default)]
pub struct AlertTracker {
    last_gust: Option<i64>,
    frost_day: Option<NaiveDate>,
    heat_day: Option<NaiveDate>,
    last_wet: Option<i64>,
    seen_dry_since: Option<i64>,
}

impl AlertTracker {
    /// The notifications this reading calls for, as `(title, body)`.
    pub fn check(&mut self, cfg: &Alerts, station: &str, r: &LiveReading) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if !cfg.enabled {
            return out;
        }
        let now = r.fetched_at;
        let today = Local::now().date_naive();
        let tu = &r.units.temp;
        let wu = &r.units.wind;

        if let (Some(limit), Some(g)) = (cfg.gust_above, r.gust_recent) {
            let cooled = self.last_gust.is_none_or(|t| now - t > GUST_COOLDOWN_MS);
            if g >= limit && cooled {
                self.last_gust = Some(now);
                out.push((
                    format!("Gust of {g:.0} {wu}"),
                    format!("{station}: strong wind, above your {limit:.0} {wu} alert."),
                ));
            }
        }

        if let (Some(limit), Some(t)) = (cfg.frost_below, r.temp) {
            if t <= limit && self.frost_day != Some(today) {
                self.frost_day = Some(today);
                out.push((
                    format!("Down to {t:.1}{tu}"),
                    format!("{station}: frost risk tonight."),
                ));
            }
        }

        if let (Some(limit), Some(t)) = (cfg.heat_above, r.temp) {
            if t >= limit && self.heat_day != Some(today) {
                self.heat_day = Some(today);
                out.push((
                    format!("Up to {t:.1}{tu}"),
                    format!("{station}: above your {limit:.0}{tu} heat alert."),
                ));
            }
        }

        if cfg.rain_start {
            let rate = r.rain_rate.unwrap_or(0.0);
            if rate > 0.0 {
                // Only alert if Isobar has actually watched it be dry for a while.
                let dry_long_enough = match (self.last_wet, self.seen_dry_since) {
                    (Some(lw), _) => now - lw > DRY_SPELL_MS,
                    (None, Some(since)) => now - since > DRY_SPELL_MS,
                    (None, None) => false,
                };
                if dry_long_enough {
                    out.push((
                        "Rain has started".into(),
                        format!("{station}: {rate:.1} {}/h.", r.units.rain),
                    ));
                }
                self.last_wet = Some(now);
            } else if self.seen_dry_since.is_none() {
                self.seen_dry_since = Some(now);
            }
        }
        out
    }
}

pub fn notify(title: &str, body: &str) {
    let _ = notify_rust::Notification::new()
        .appname("Isobar")
        .summary(title)
        .body(body)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(at: i64, temp: f64, gust: f64, rate: f64) -> LiveReading {
        LiveReading {
            fetched_at: at,
            temp: Some(temp),
            gust_recent: Some(gust),
            rain_rate: Some(rate),
            ..Default::default()
        }
    }

    #[test]
    fn gusts_cool_down() {
        let cfg = Alerts::default();
        let mut t = AlertTracker::default();
        assert_eq!(t.check(&cfg, "Home", &reading(0, 20.0, 65.0, 0.0)).len(), 1);
        assert!(t
            .check(&cfg, "Home", &reading(60_000, 20.0, 70.0, 0.0))
            .is_empty());
        assert_eq!(
            t.check(
                &cfg,
                "Home",
                &reading(GUST_COOLDOWN_MS + 1, 20.0, 70.0, 0.0)
            )
            .len(),
            1
        );
    }

    #[test]
    fn rain_needs_a_dry_spell_first() {
        let cfg = Alerts::default();
        let mut t = AlertTracker::default();
        assert!(t
            .check(&cfg, "Home", &reading(0, 20.0, 0.0, 1.0))
            .is_empty());
        assert!(t
            .check(&cfg, "Home", &reading(60_000, 20.0, 0.0, 0.0))
            .is_empty());
        let later = DRY_SPELL_MS + 120_000;
        let got = t.check(&cfg, "Home", &reading(later, 20.0, 0.0, 2.0));
        assert_eq!(got[0].0, "Rain has started");
    }
}
