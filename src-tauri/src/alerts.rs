//! Desktop notifications for notable conditions. Each alert has a cooldown
//! so a gusty afternoon produces one notification, not forty.

use chrono::{Local, NaiveDate};
use isobar_core::LiveReading;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

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
    pub fn check(&mut self, app: &AppHandle, cfg: &Alerts, station: &str, r: &LiveReading) {
        if !cfg.enabled {
            return;
        }
        let now = r.fetched_at;
        let today = Local::now().date_naive();
        let tu = &r.units.temp;
        let wu = &r.units.wind;

        if let (Some(limit), Some(g)) = (cfg.gust_above, r.gust_recent) {
            let cooled = self.last_gust.map_or(true, |t| now - t > GUST_COOLDOWN_MS);
            if g >= limit && cooled {
                self.last_gust = Some(now);
                notify(app, &format!("Gust of {g:.0} {wu}"), &format!("{station}: strong wind, above your {limit:.0} {wu} alert."));
            }
        }

        if let (Some(limit), Some(t)) = (cfg.frost_below, r.temp) {
            if t <= limit && self.frost_day != Some(today) {
                self.frost_day = Some(today);
                notify(app, &format!("Down to {t:.1}{tu}"), &format!("{station}: frost risk tonight."));
            }
        }

        if let (Some(limit), Some(t)) = (cfg.heat_above, r.temp) {
            if t >= limit && self.heat_day != Some(today) {
                self.heat_day = Some(today);
                notify(app, &format!("Up to {t:.1}{tu}"), &format!("{station}: above your {limit:.0}{tu} heat alert."));
            }
        }

        if cfg.rain_start {
            let wet = r.rain_rate.unwrap_or(0.0) > 0.0;
            if wet {
                // Only alert if we have actually watched it be dry for a while.
                let dry_long_enough = match (self.last_wet, self.seen_dry_since) {
                    (Some(lw), _) => now - lw > DRY_SPELL_MS,
                    (None, Some(since)) => now - since > DRY_SPELL_MS,
                    (None, None) => false,
                };
                if dry_long_enough {
                    let rate = r.rain_rate.unwrap_or(0.0);
                    notify(app, "Rain has started", &format!("{station}: {rate:.1} {}/h.", r.units.rain));
                }
                self.last_wet = Some(now);
            } else if self.seen_dry_since.is_none() {
                self.seen_dry_since = Some(now);
            }
        }
    }
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}
