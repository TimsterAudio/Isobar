mod alerts;
mod commands;
mod poller;
mod settings;
mod state;

use std::sync::Arc;

use isobar_core::Store;
use tauri::Manager;

use settings::Settings;
use state::AppState;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let config_dir = app.path().app_config_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            std::fs::create_dir_all(&config_dir)?;

            let settings_path = config_dir.join("settings.json");
            let settings = Settings::load(&settings_path);
            let store = Store::open(&data_dir.join("isobar.sqlite"))?;

            let state = Arc::new(AppState::new(settings, settings_path, store));
            app.manage(state.clone());
            poller::spawn(app.handle().clone(), state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::test_station,
            commands::get_live,
            commands::get_status,
            commands::get_days,
            commands::get_samples,
            commands::sync_history,
            commands::import_dayfile,
            commands::clear_history,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Isobar");
}
