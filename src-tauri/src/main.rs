#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod game;
mod i18n;
mod mods;
mod settings;
mod tools;
mod versions;
mod wiimote;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .manage(settings::Store::default())
        .manage(game::Game::default())
        .manage(versions::Shipped::default())
        .setup(|app| {
            i18n::init(&app.state::<settings::Store>().get(app.handle()).language);
            app.manage(wiimote::start(app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            i18n::locale_set,
            tools::tools_status,
            settings::settings_get,
            settings::settings_set,
            game::iso_pick,
            game::iso_choose,
            game::game_state,
            game::game_launch,
            game::game_stop,
            game::game_dismiss,
            game::folders_info,
            game::folder_open,
            mods::mods_list,
            mods::mods_import,
            mods::mods_pick,
            mods::mods_set_enabled,
            mods::mods_remove,
            mods::package_detail,
            mods::package_settings_set,
            versions::versions_state,
            versions::versions_activate,
            versions::versions_import,
            versions::versions_pick,
            versions::versions_remove,
            wiimote::wiimote_state,
            wiimote::wiimote_scan,
            wiimote::wiimote_disconnect,
            wiimote::wiimote_identify,
            wiimote::wiimote_move,
            wiimote::wiimote_release,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
