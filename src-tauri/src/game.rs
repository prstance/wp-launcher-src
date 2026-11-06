//! Picking the disc image and running the game in the shipped Dolphin.

use crate::discord;
use crate::i18n::tr;
use crate::mii;
use crate::mods;
use crate::rebuild;
use crate::settings::{self, Disc, Settings, Store};
use crate::tools;
use crate::wiimote::Wiimotes;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::async_runtime::spawn;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{ChildStdout, Command};
use tokio::sync::oneshot;

const STATE_EVENT: &str = "game://state";
/// Every image Dolphin reads that can hold a Wii disc.
const DISC_FORMATS: [&str; 6] = ["iso", "wbfs", "rvz", "gcz", "wia", "ciso"];
const RECENT_MAX: usize = 5;
/// Tells our Dolphin build where to ask for the remotes (IOBridge.h in the fork).
const WIIMOTE_BRIDGE: &str = "WP_WIIMOTE_BRIDGE";
/// Asks our Dolphin build to print where the game is (WPGameState.h in the fork).
const GAME_STATE: &str = "WP_GAME_STATE";

#[derive(Serialize, Clone)]
pub struct GameState {
    /// "idle", "verifying", "problems" (damaged copy, waiting for the player's
    /// decision), "preparing" (the packages' files made from the disc, rebuild.rs),
    /// "running" or "error".
    phase: &'static str,
    message: Option<String>,
    /// What Dolphin runs, or ran: "game", or "miis" for the Mii Channel.
    title: &'static str,
}

pub struct Game {
    state: Mutex<GameState>,
    stop: Mutex<Option<oneshot::Sender<()>>>,
}

impl Default for Game {
    fn default() -> Self {
        Game { state: Mutex::new(GameState { phase: "idle", message: None, title: "game" }), stop: Mutex::new(None) }
    }
}

pub fn is_playing(app: &AppHandle) -> bool {
    matches!(app.state::<Game>().state.lock().unwrap().phase, "verifying" | "preparing" | "running")
}

fn set_state(app: &AppHandle, phase: &'static str, message: Option<String>) {
    let game = app.state::<Game>();
    let mut current = game.state.lock().unwrap();
    *current = GameState { phase, message, title: current.title };
    let _ = app.emit(STATE_EVENT, current.clone());
}

/// Takes Dolphin for `title`, unless it already runs something.
fn begin(app: &AppHandle, title: &'static str, phase: &'static str) -> Result<(), String> {
    {
        let game = app.state::<Game>();
        let mut state = game.state.lock().unwrap();
        if matches!(state.phase, "verifying" | "preparing" | "running") {
            return Err(tr!("Le jeu est déjà en cours."));
        }
        state.title = title;
        state.phase = phase;
    }
    // Shown at once, so the screen never flashes the previous result.
    set_state(app, phase, None);
    Ok(())
}

/// The launcher's own environment (library paths, Qt plugins…) must not leak
/// into Dolphin, which runs from its own folder.
pub fn command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    for var in ["LD_LIBRARY_PATH", "LD_PRELOAD", "QT_PLUGIN_PATH", "QT_QPA_PLATFORM_PLUGIN_PATH", "APPDIR", "APPIMAGE"] {
        cmd.env_remove(var);
    }
    #[cfg(target_os = "linux")]
    cmd.env("QT_QPA_PLATFORM", "xcb");
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW, for dolphin-tool
    cmd.stdin(Stdio::null());
    cmd
}

fn dolphin_tool(app: &AppHandle) -> Result<PathBuf, String> {
    tools::dolphin_tool(app).ok_or_else(|| tr!("Dolphin est absent de ce build du launcher."))
}

/// Reads a disc image's header and accepts it only if it is Wii Party.
async fn inspect(app: &AppHandle, path: &str) -> Result<Disc, String> {
    let output = command(&dolphin_tool(app)?)
        .args(["header", "-j", "-i", path])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    let header: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| tr!("Ce fichier n’est pas une image de disque lisible."))?;
    let text = |key: &str| header.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let game_id = text("game_id");
    if game_id.is_empty() {
        return Err(tr!("Ce fichier n’est pas une image de disque lisible."));
    }
    if !game_id.starts_with("SUP") {
        return Err(tr!("Ce n’est pas Wii Party : ce disque contient « {name} » ({id}).", name = text("internal_name"), id = game_id));
    }
    let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(Disc { path: path.to_string(), name, game_id, country: text("country") })
}

/// Makes an image the current one, if it is Wii Party.
#[tauri::command]
pub async fn iso_choose(app: AppHandle, path: String) -> Result<Settings, String> {
    let disc = inspect(&app, &path).await?;
    let saved = app.state::<Store>().update(&app, |s| {
        s.recent.retain(|d| d.path != disc.path);
        s.recent.insert(0, disc.clone());
        s.recent.truncate(RECENT_MAX);
        s.iso = Some(disc);
    })?;
    mii::prepare(&app);
    Ok(settings::for_ui(&app, saved))
}

/// Shows the file picker; None when the player cancels.
#[tauri::command]
pub async fn iso_pick(app: AppHandle) -> Result<Option<Settings>, String> {
    let dialog = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog.dialog().file().set_title(tr!("Choisis ton ISO de Wii Party")).add_filter(tr!("Image de disque Wii"), &DISC_FORMATS).blocking_pick_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(path) = picked.and_then(|f| f.into_path().ok()) else { return Ok(None) };
    iso_choose(app, path.to_string_lossy().into_owned()).await.map(Some)
}

#[tauri::command]
pub fn game_state(game: State<'_, Game>) -> GameState {
    game.state.lock().unwrap().clone()
}

pub fn data_dir(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join(name);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Full check of the image: Ok(false) when it has problems (a modded image can too).
async fn verify(app: &AppHandle, iso: &str) -> Result<bool, String> {
    let scratch = data_dir(app, "verify")?;
    let output = command(&dolphin_tool(app)?)
        .arg("verify")
        .arg("-u")
        .arg(&scratch)
        .args(["-i", iso])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    let report = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || !report.contains("Problems Found:") {
        return Err(tr!("La vérification de l’ISO a échoué."));
    }
    Ok(report.contains("Problems Found: No"))
}

/// Dolphin options for this launch; given on the command line, they never touch
/// its saved configuration.
fn dolphin_args(s: &Settings, iso: &str, user: &Path) -> Vec<String> {
    let bool_text = |b: bool| if b { "True" } else { "False" };
    let mut config = vec![
        format!("Dolphin.Display.Fullscreen={}", bool_text(s.fullscreen)),
        format!("Dolphin.DSP.Volume={}", s.volume.min(100)),
        "Dolphin.Core.WiimoteContinuousScanning=True".to_string(),
        "Dolphin.Interface.ConfirmStop=False".to_string(),
        format!("SYSCONF.IPL.LNG={}", s.game_language),
        format!("Graphics.Settings.InternalResolution={}", s.resolution),
        format!("Graphics.Settings.AspectRatio={}", s.aspect),
        format!("Graphics.Hardware.VSync={}", bool_text(s.vsync)),
        format!("Graphics.Settings.ShowFPS={}", bool_text(s.show_fps)),
    ];
    config.extend((1..=4).map(|n| format!("Wiimote.Wiimote{n}.Source=2")));

    let mut args = vec!["-b".to_string(), "-u".to_string(), user.to_string_lossy().into_owned(), "-e".to_string(), iso.to_string()];
    for option in config {
        args.push("-C".to_string());
        args.push(option);
    }
    args
}

/// Last line Dolphin printed, to explain an early exit.
fn last_log_line(log: &Path) -> Option<String> {
    let text = std::fs::read_to_string(log).ok()?;
    text.lines().rev().find(|l| !l.trim().is_empty() && !l.contains("no version information")).map(str::to_string)
}

async fn run(app: AppHandle, disc: Disc, settings: Settings, force: bool) {
    if !Path::new(&disc.path).is_file() {
        set_state(&app, "error", Some(tr!("L’ISO n’est plus à cet endroit. Choisis-le à nouveau.")));
        return;
    }
    let boot = match mods::boot_file(&app, &disc, &settings) {
        Ok(Some(file)) => file.to_string_lossy().into_owned(),
        Ok(None) => disc.path.clone(),
        Err(e) => {
            set_state(&app, "error", Some(e));
            return;
        }
    };
    if settings.verify_iso && !force {
        set_state(&app, "verifying", None);
        match verify(&app, &disc.path).await {
            Ok(true) => {}
            Ok(false) => {
                set_state(&app, "problems", Some(tr!("Cette copie présente des anomalies : abîmée, ou modifiée par un mod.")));
                return;
            }
            Err(e) => {
                set_state(&app, "error", Some(e));
                return;
            }
        }
    }
    // The packages' files made from the disc, the first time.
    let packages = mods::launch_packages(&app, &settings).unwrap_or_default();
    let packages: Vec<&mods::Installed> = packages.iter().collect();
    if rebuild::needed(&packages) {
        set_state(&app, "preparing", None);
        if let Err(e) = rebuild::prepare(&app, &disc, &packages).await {
            set_state(&app, "error", Some(e));
            return;
        }
    }
    // The mods' code and texts, linked into the game's files.
    if let Err(e) = mods::link_code(&app, &disc, &packages).await {
        set_state(&app, "error", Some(e));
        return;
    }
    if let Err(e) = mii::write_nand(&app) {
        eprintln!("Mii: {e}");
    }
    if let Err(e) = play(&app, &boot, &settings).await {
        set_state(&app, "error", Some(e));
    } else {
        set_state(&app, "idle", None);
    }
}

async fn play(app: &AppHandle, boot: &str, settings: &Settings) -> Result<(), String> {
    let (_, dolphin) = tools::find(app, "dolphin").ok_or_else(|| tr!("Dolphin est absent de ce build du launcher."))?;
    let user = data_dir(app, "dolphin")?;
    let log = user.join("launcher-dolphin.log");
    let out = std::fs::File::create(&log).map_err(|e| e.to_string())?;
    let err = out.try_clone().map_err(|e| e.to_string())?;

    // The remotes stay connected to the launcher, which passes them on to Dolphin.
    let wiimotes = app.state::<Wiimotes>().inner().clone();
    let bridge = wiimotes.lend().await;
    let mut cmd = command(&dolphin);
    if let Some(bridge) = bridge {
        cmd.env(WIIMOTE_BRIDGE, bridge);
    }
    let spawned = cmd
        .env(GAME_STATE, "1")
        .args(dolphin_args(settings, boot, &user))
        .stdout(Stdio::piped())
        .stderr(Stdio::from(err))
        .kill_on_drop(true)
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => {
            wiimotes.take_back();
            return Err(tr!("Impossible de démarrer Dolphin : {e}", e = e));
        }
    };
    let follower = child.stdout.take().map(|stdout| spawn(follow(app.clone(), stdout, out)));

    let window = app.get_webview_window("main");
    if settings.minimize_in_game {
        if let Some(w) = &window {
            let _ = w.minimize();
        }
    }
    let (tx, rx) = oneshot::channel();
    *app.state::<Game>().stop.lock().unwrap() = Some(tx);
    set_state(app, "running", None);
    let title = app.state::<Game>().state.lock().unwrap().title;
    discord::playing(app, title);

    let status = tokio::select! {
        status = child.wait() => status.map(Some),
        _ = rx => child.kill().await.map(|_| None),
    };

    *app.state::<Game>().stop.lock().unwrap() = None;
    discord::stopped(app);
    // Its last lines, in the log before it's read.
    if let Some(follower) = follower {
        let _ = tokio::time::timeout(Duration::from_secs(2), follower).await;
    }
    wiimotes.take_back();
    if let Some(w) = &window {
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    match status {
        Ok(Some(s)) if !s.success() => {
            Err(match last_log_line(&log) {
                Some(why) => tr!("Dolphin s’est arrêté : {why}", why = why),
                None => tr!("Dolphin s’est arrêté de façon inattendue."),
            })
        }
        Ok(_) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// Dolphin's output, into its log; our build's lines on where the game is also go
/// to Discord. Not all of it is UTF-8, and all of it must be read, or Dolphin waits.
async fn follow(app: AppHandle, stdout: ChildStdout, mut log: std::fs::File) {
    use std::io::Write;
    let mut stdout = BufReader::new(stdout);
    let mut line = Vec::new();
    while stdout.read_until(b'\n', &mut line).await.is_ok_and(|n| n > 0) {
        let _ = log.write_all(&line);
        discord::report(&app, String::from_utf8_lossy(&line).trim_end());
        line.clear();
    }
}

/// Starts the game with the current image. `force` skips the image check, once
/// the player has decided to go on despite its problems.
#[tauri::command]
pub async fn game_launch(app: AppHandle, force: bool) -> Result<(), String> {
    if !app.state::<Wiimotes>().has_player().await {
        return Err(tr!("Connecte au moins une Wiimote avant de lancer le jeu."));
    }
    let settings = app.state::<Store>().get(&app);
    let disc = settings.iso.clone().ok_or_else(|| tr!("Choisis d’abord ton ISO de Wii Party."))?;
    let first = if settings.verify_iso && !force { "verifying" } else { "running" };
    begin(&app, "game", first)?;
    spawn(run(app, disc, settings, force));
    Ok(())
}

/// The Mii Channel, from the disc's update partition, with the library in its
/// database; what's made there comes back to the library when it closes.
pub async fn open_mii_channel(app: AppHandle) -> Result<(), String> {
    if !app.state::<Wiimotes>().has_player().await {
        return Err(tr!("Connecte au moins une Wiimote : la Chaîne Mii se joue en pointant l’écran."));
    }
    let settings = app.state::<Store>().get(&app);
    let disc = settings.iso.clone().ok_or_else(|| tr!("Choisis d’abord ton ISO de Wii Party."))?;
    begin(&app, "miis", "running")?;
    spawn(async move {
        let opened = async {
            let wad = mii::disc::channel_wad(&app, &disc).await?;
            mii::write_nand(&app)?;
            spawn(close_after_save(app.clone()));
            play(&app, &wad.to_string_lossy(), &settings).await
        }
        .await;
        let back = mii::read_nand(&app);
        match opened.and(back) {
            Ok(_) => set_state(&app, "idle", None),
            Err(e) => set_state(&app, "error", Some(e)),
        }
    });
    Ok(())
}

/// The Mii Channel has no way back to the launcher: Dolphin closes once a Mii is
/// saved, after a moment to see it done.
async fn close_after_save(app: AppHandle) {
    // Long enough for Dolphin and the channel to start, far too short to make a Mii.
    const SETTLING: Duration = Duration::from_secs(15);
    let start = Instant::now();
    let mut watch = mii::SaveWatch::new(mii::nand_miis(&app).unwrap_or_default());
    loop {
        tokio::time::sleep(Duration::from_millis(700)).await;
        let open = {
            let state = app.state::<Game>().state.lock().unwrap().clone();
            state.title == "miis" && state.phase == "running"
        };
        if !open {
            return;
        }
        if watch.saved(mii::nand_miis(&app), start.elapsed() < SETTLING) {
            tokio::time::sleep(Duration::from_millis(1500)).await;
            game_stop(app.state::<Game>());
            return;
        }
    }
}

#[tauri::command]
pub fn game_stop(game: State<'_, Game>) {
    if let Some(stop) = game.stop.lock().unwrap().take() {
        let _ = stop.send(());
    }
}

/// Back to "idle" once the player has read a result.
#[tauri::command]
pub fn game_dismiss(app: AppHandle, game: State<'_, Game>) {
    let phase = game.state.lock().unwrap().phase;
    if matches!(phase, "problems" | "error") {
        set_state(&app, "idle", None);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folders {
    mods: String,
    data: String,
}

#[tauri::command]
pub fn folders_info(app: AppHandle) -> Result<Folders, String> {
    let mods = data_dir(&app, "mods")?;
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(Folders { mods: mods.to_string_lossy().into_owned(), data: data.to_string_lossy().into_owned() })
}

/// Opens one of the launcher's own folders ("mods" or "data") in the file manager.
#[tauri::command]
pub fn folder_open(app: AppHandle, which: String) -> Result<(), String> {
    let dir = match which.as_str() {
        "mods" => data_dir(&app, "mods")?,
        "data" => app.path().app_data_dir().map_err(|e| e.to_string())?,
        _ => return Err(tr!("dossier inconnu : {which}", which = which)),
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_settings_to_dolphin() {
        let settings = Settings { fullscreen: false, resolution: 4, volume: 40, game_language: 4, ..Default::default() };
        let args = dolphin_args(&settings, "/jeux/wp.rvz", Path::new("/data/dolphin"));
        assert_eq!(args[..5], ["-b", "-u", "/data/dolphin", "-e", "/jeux/wp.rvz"]);
        for expected in [
            "Dolphin.Display.Fullscreen=False",
            "Dolphin.DSP.Volume=40",
            "SYSCONF.IPL.LNG=4",
            "Graphics.Settings.InternalResolution=4",
            "Wiimote.Wiimote4.Source=2",
        ] {
            let at = args.iter().position(|a| a == expected).unwrap_or_else(|| panic!("{expected}"));
            assert_eq!(args[at - 1], "-C");
        }
    }
}
