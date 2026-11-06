//! Core of Isobar: everything that does not need a window.
//!
//! * [`dayfile`] parses Cumulus `dayfile.txt` lines (from the file or the MX API).
//! * [`live`] maps the MX `currentdata` JSON onto a typed reading.
//! * [`logfile`] reads MX's monthly log lines (a reading every few minutes).
//! * [`mx`] is the HTTP client for a Cumulus MX instance.
//! * [`store`] is the local SQLite cache of daily records and live samples.
//! * [`analytics`] turns the daily history into normals, anomalies and records.
//! * [`seasons`] covers water balance, frost, growing degree days and the sun.
//! * [`trends`] compares years: the temperature trend and degree days.
//! * [`units`] converts to the units chosen in Settings.
//! * [`tanks`] models rainwater tanks from the rain.
//! * [`health`] checks the data itself: missing days and gaps in readings.
//! * [`almanac`] is a calendar year in review, as a page to print.
//! * [`backup`] makes nightly copies of the history for a backup program.
//! * [`demo`] is a synthetic station for trying Isobar without one.

pub mod almanac;
pub mod analytics;
pub mod backup;
pub mod dayfile;
pub mod demo;
pub mod error;
pub mod export;
pub mod forecast;
pub mod health;
pub mod indoor;
pub mod live;
pub mod logfile;
pub mod mx;
pub mod outlook;
pub mod reports;
pub mod seasons;
pub mod store;
pub mod tanks;
pub mod trends;
pub mod units;

pub use dayfile::DayRecord;
pub use error::{CoreError, Result};
pub use live::{Alarm, LiveReading, Units};
pub use mx::MxClient;
pub use store::Store;
