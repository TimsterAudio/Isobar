//! Core of Isobar: everything that does not need a window.
//!
//! * [`dayfile`] parses Cumulus `dayfile.txt` lines (from the file or the MX API).
//! * [`live`] maps the MX `currentdata` JSON onto a typed reading.
//! * [`logfile`] reads MX's monthly log lines (a reading every few minutes).
//! * [`mx`] is the HTTP client for a Cumulus MX instance.
//! * [`store`] is the local SQLite cache of daily records and live samples.
//! * [`analytics`] turns the daily history into normals, anomalies and records.
//! * [`seasons`] covers water balance, frost, growing degree days and the sun.
//! * [`tanks`] models rainwater tanks from the rain.
//! * [`health`] checks the data itself: missing days and gaps in readings.
//! * [`demo`] is a synthetic station for trying Isobar without one.

pub mod analytics;
pub mod dayfile;
pub mod demo;
pub mod error;
pub mod health;
pub mod live;
pub mod logfile;
pub mod mx;
pub mod seasons;
pub mod store;
pub mod tanks;

pub use dayfile::DayRecord;
pub use error::{CoreError, Result};
pub use live::{Alarm, LiveReading, Units};
pub use mx::MxClient;
pub use store::Store;
