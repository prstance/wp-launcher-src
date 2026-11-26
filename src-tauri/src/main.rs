#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod delta;
mod discord;
mod game;
mod i18n;
mod link;
mod lz;
mod minigames;
mod mii;
mod modes;
mod mods;
mod overlay;
mod ranges;
mod rebuild;
mod settings;
mod sound;
mod tools;
mod update;
mod versions;
mod wiimote;
mod window;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(settings::Store::default())
        .manage(game::Game::default())
        .manage(overlay::Overlay::default())
        .manage(versions::Shipped::default())
        .manage(window::Enlarged::default())
        .manage(mii::Library::default())
        .setup(|app| {
            i18n::init(&app.state::<settings::Store>().get(app.handle()).language);
            app.manage(wiimote::start(app.handle().clone()));
            if app.state::<settings::Store>().get(app.handle()).phone_remotes {
                app.state::<wiimote::Wiimotes>().set_phones(true);
            }
            app.manage(discord::start());
            discord::refresh(app.handle());
            mii::prepare(app.handle());
            if let Some(win) = app.get_webview_window("main") {
                window::init(&win);
            }
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
            game::dolphin_folders,
            game::dolphin_folder_set,
            game::dolphin_folder_pick,
            mods::mods_list,
            mods::mods_import,
            mods::mods_pick,
            mods::mods_set_enabled,
            mods::mods_remove,
            mods::package_detail,
            mods::package_settings_set,
            mods::debug_state,
            mods::debug_settings_set,
            mii::miis_state,
            mii::miis_disc,
            mii::miis_adopt,
            mii::miis_import,
            mii::miis_pick,
            mii::miis_rename,
            mii::miis_remove,
            mii::miis_export,
            mii::miis_channel,
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
            modes::modes_get,
            window::window_toggle_size,
            update::launcher_update_check,
            update::launcher_update_install,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
