//! Launcher settings, saved as JSON in the app's config folder.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

/// The launcher runs in development (`npm run tauri dev`, a debug build): it then has its Debug page,
/// where the packages' debug settings are (mods.rs).
pub const DEV: bool = cfg!(debug_assertions);

/// A Wii Party disc image the player picked.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Disc {
    pub path: String,
    pub name: String,
    pub game_id: String,
    pub country: String,
}

/// The player's account in the launcher.
#[derive(Serialize, Deserialize, Clone)]
pub struct Player {
    pub name: String,
    /// ID of their Mii in the library (mii.rs).
    pub mii: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// None until the welcome screen is done.
    pub player: Option<Player>,
    pub iso: Option<Disc>,
    pub recent: Vec<Disc>,
    /// Jouer starts right away with `iso`, without showing the ISO screen.
    pub always_use_iso: bool,
    pub verify_iso: bool,
    /// How the game opens: 0 full screen, 1 full screen in a borderless window, 2 in a window.
    pub window_mode: u32,
    /// Before `window_mode`: only read from older settings, then forgotten.
    #[serde(skip_serializing)]
    pub fullscreen: Option<bool>,
    /// Wii console language (0 Japanese, 1 English, 2 German, 3 French, 4 Spanish, 5 Italian, 6 Dutch);
    /// None: the computer's (i18n::console_language).
    pub console_language: Option<u32>,
    /// Before `console_language`, when French was everyone's default: only read from older
    /// settings, then forgotten. Another language was picked on purpose and stays.
    #[serde(rename = "gameLanguage", skip_serializing)]
    pub legacy_game_language: Option<u32>,
    /// The Wii's sound setting (SYSCONF IPL.SND): 0 mono, 1 stereo, 2 surround (Dolby Pro Logic II,
    /// which Dolphin then decodes to 5.1).
    pub sound_mode: u32,
    /// Dolphin's internal resolution multiplier.
    pub resolution: u32,
    /// Dolphin's aspect mode: 0 auto, 1 16:9, 2 4:3, 3 stretched.
    pub aspect: u32,
    pub vsync: bool,
    pub show_fps: bool,
    pub volume: u32,
    pub minimize_in_game: bool,
    /// The sensor bar sits on top of the screen (false: below it), for the launcher's
    /// pointer and the game's.
    pub sensor_bar_top: bool,
    /// What's being played, on the player's Discord profile (discord.rs).
    pub discord_presence: bool,
    /// Phones on the local network can be Wii Remotes (wiimote/phone.rs).
    pub phone_remotes: bool,
    /// The launcher's language: "fr", "en", "es", or "auto" for the system's.
    pub language: String,
    /// The launcher's colours: "light" (the default), "dark", or "system" to follow the computer.
    pub appearance: String,
    /// Mods the game starts with, in load order.
    pub enabled_mods: Vec<String>,
    /// Values chosen for the mods' and versions' settings (mods.rs).
    pub package_settings: crate::mods::Values,
    /// Dolphin's user folder the game runs with: None for the launcher's own (`<data>/dolphin`), or
    /// the player's Dolphin's, to play on its save and with its Miis (game.rs, user_dir).
    pub dolphin_user: Option<String>,
    /// The version the game and the launcher are dressed in. None: Wii Party as it came.
    pub active_version: Option<String>,
    /// Lives in the system's autostart entry; only mirrored here for the UI.
    #[serde(skip_deserializing)]
    pub autostart: bool,
    /// DEV, for the UI.
    #[serde(skip_deserializing)]
    pub dev: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            player: None,
            iso: None,
            recent: Vec::new(),
            always_use_iso: false,
            verify_iso: true,
            window_mode: 0,
            fullscreen: None,
            console_language: None,
            legacy_game_language: None,
            sound_mode: 1,
            resolution: 3,
            aspect: 0,
            vsync: true,
            show_fps: false,
            volume: 100,
            minimize_in_game: true,
            sensor_bar_top: false,
            discord_presence: true,
            phone_remotes: false,
            language: "auto".to_string(),
            appearance: "light".to_string(),
            enabled_mods: Vec::new(),
            package_settings: Default::default(),
            dolphin_user: None,
            active_version: Some(crate::versions::FIRST.to_string()),
            autostart: false,
            dev: false,
        }
    }
}

#[derive(Default)]
pub struct Store(Mutex<Option<Settings>>);

fn file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_config_dir().map_err(|e| e.to_string())?.join("settings.json"))
}

fn load(app: &AppHandle) -> Settings {
    let saved = file(app).ok().and_then(|f| std::fs::read(f).ok());
    let mut settings: Settings = saved.and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
    if settings.fullscreen.take() == Some(false) {
        settings.window_mode = 2;
    }
    if let Some(n) = settings.legacy_game_language.take().filter(|&n| n != 3) {
        settings.console_language = Some(n);
    }
    settings
}

fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = file(app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

impl Store {
    pub fn get(&self, app: &AppHandle) -> Settings {
        self.0.lock().unwrap().get_or_insert_with(|| load(app)).clone()
    }

    /// Changes the settings, saves them and returns the result.
    pub fn update(&self, app: &AppHandle, change: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
        let mut guard = self.0.lock().unwrap();
        let settings = guard.get_or_insert_with(|| load(app));
        change(settings);
        save(app, settings)?;
        Ok(settings.clone())
    }
}

/// What the UI sees: forgets images that have since been moved or deleted.
pub fn for_ui(app: &AppHandle, mut settings: Settings) -> Settings {
    settings.recent.retain(|d| std::path::Path::new(&d.path).is_file());
    if settings.iso.as_ref().is_some_and(|d| !std::path::Path::new(&d.path).is_file()) {
        settings.iso = None;
    }
    settings.autostart = app.autolaunch().is_enabled().unwrap_or(false);
    settings.dev = DEV;
    settings
}

#[tauri::command]
pub fn settings_get(app: AppHandle, store: State<'_, Store>) -> Settings {
    for_ui(&app, store.get(&app))
}

/// Merges `patch` (some fields of Settings) into the settings.
#[tauri::command]
pub fn settings_set(app: AppHandle, store: State<'_, Store>, patch: Value) -> Result<Settings, String> {
    if let Some(on) = patch.get("autostart").and_then(Value::as_bool) {
        let auto = app.autolaunch();
        if on { auto.enable() } else { auto.disable() }.map_err(|e| e.to_string())?;
    }
    let mut merged = serde_json::to_value(store.get(&app)).map_err(|e| e.to_string())?;
    if let (Some(into), Some(from)) = (merged.as_object_mut(), patch.as_object()) {
        into.extend(from.clone());
    }
    let next: Settings = serde_json::from_value(merged).map_err(|e| e.to_string())?;
    let saved = store.update(&app, |s| *s = next)?;
    if patch.get("discordPresence").is_some() {
        crate::discord::refresh(&app);
    }
    if patch.get("sensorBarTop").is_some() {
        app.state::<crate::wiimote::Wiimotes>().set_bar(saved.sensor_bar_top);
    }
    if patch.get("phoneRemotes").is_some() {
        app.state::<crate::wiimote::Wiimotes>().set_phones(saved.phone_remotes);
    }
    Ok(for_ui(&app, saved))
}
