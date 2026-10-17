//! Isobar: a desktop companion for Cumulus MX weather stations.
//!
//! Run `isobar` to use your station (set it up in Settings), or
//! `isobar --demo` to look around with a synthetic one.

// No console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod alerts;
mod app;
mod chart;
mod dial;
mod format;
mod settings;
mod theme;
mod ui;
mod views;
mod worker;

use eframe::egui;
use isobar_core::Store;

use settings::{Paths, Settings};

fn main() -> eframe::Result {
    let demo = std::env::args().any(|a| a == "--demo");
    let (settings, paths, store) = if demo {
        let s = Settings {
            station_name: "Demo station".into(),
            ..Default::default()
        };
        (s, None, None)
    } else {
        let paths = Paths::locate();
        let settings = Settings::load(&paths.settings);
        if let Some(dir) = paths.database.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let store = match Store::open(&paths.database) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "Isobar couldn't open its database at {}: {e}",
                    paths.database.display()
                );
                Store::open_in_memory().expect("an in-memory database always opens")
            }
        };
        (settings, Some(paths), Some(store))
    };

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icons/icon.png")).ok();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Isobar")
        .with_app_id("isobar")
        .with_inner_size([1440.0, 960.0])
        .with_min_inner_size([900.0, 600.0]);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    eframe::run_native(
        "Isobar",
        eframe::NativeOptions {
            viewport,
            persist_window: true,
            ..Default::default()
        },
        Box::new(move |cc| {
            let handle = worker::spawn(cc.egui_ctx.clone(), settings.clone(), store);
            Ok(Box::new(app::IsobarApp::new(cc, settings, paths, handle)))
        }),
    )
}
