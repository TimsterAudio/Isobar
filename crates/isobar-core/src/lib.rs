//! Core of Isobar: everything that does not need a window.
//!
//! * [`dayfile`] parses Cumulus `dayfile.txt` lines (from the file or the MX API).
//! * [`live`] maps the MX `currentdata` JSON onto a typed reading.
//! * [`mx`] is the HTTP client for a Cumulus MX instance.
//! * [`store`] is the local SQLite cache of daily records and live samples.
//! * [`analytics`] turns the daily history into normals, anomalies and records.
//! * [`demo`] is a synthetic station for trying Isobar without one.

pub mod analytics;
pub mod dayfile;
pub mod demo;
pub mod error;
pub mod live;
pub mod mx;
pub mod store;

pub use dayfile::DayRecord;
pub use error::{CoreError, Result};
pub use live::{LiveReading, Units};
pub use mx::MxClient;
pub use store::Store;
