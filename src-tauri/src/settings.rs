//! Launcher settings, saved as JSON in the app's config folder.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

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
    pub fullscreen: bool,
    /// Wii console language (0 Japanese, 1 English, 2 German, 3 French, 4 Spanish, 5 Italian, 6 Dutch).
    pub game_language: u32,
    /// Dolphin's internal resolution multiplier.
    pub resolution: u32,
    /// Dolphin's aspect mode: 0 auto, 1 16:9, 2 4:3, 3 stretched.
    pub aspect: u32,
    pub vsync: bool,
    pub show_fps: bool,
    pub volume: u32,
    pub minimize_in_game: bool,
    /// The launcher's language: "fr", "en", "es", or "auto" for the system's.
    pub language: String,
    /// Mods the game starts with, in load order.
    pub enabled_mods: Vec<String>,
    /// Values chosen for the mods' and versions' settings (mods.rs).
    pub package_settings: crate::mods::Values,
    /// The version the game and the launcher are dressed in. None: Wii Party as it came.
    pub active_version: Option<String>,
    /// Lives in the system's autostart entry; only mirrored here for the UI.
    #[serde(skip_deserializing)]
    pub autostart: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            player: None,
            iso: None,
            recent: Vec::new(),
            always_use_iso: false,
            verify_iso: true,
            fullscreen: true,
            game_language: 3,
            resolution: 3,
            aspect: 0,
            vsync: true,
            show_fps: false,
            volume: 100,
            minimize_in_game: true,
            language: "auto".to_string(),
            enabled_mods: Vec::new(),
            package_settings: Default::default(),
            active_version: Some(crate::versions::FIRST.to_string()),
            autostart: false,
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
    saved.and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
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
    Ok(for_ui(&app, saved))
}
