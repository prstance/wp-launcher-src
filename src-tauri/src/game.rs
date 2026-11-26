//! Picking the disc image and running the game in the shipped Dolphin.

use crate::discord;
use crate::i18n::tr;
use crate::mii;
use crate::mods;
use crate::overlay;
use crate::rebuild;
use crate::settings::{self, Disc, Settings, Store};
use crate::tools;
use crate::versions;
use crate::wiimote::Wiimotes;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
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
/// Tells our Dolphin build where to ask for the remotes (IOBridge.h in the fork): a Unix
/// socket's path, or on Windows a named pipe's name.
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

/// On a laptop, the game would otherwise run on the integrated chip (prime-run's variables).
#[cfg(target_os = "linux")]
const NVIDIA_OFFLOAD: [(&str, &str); 3] =
    [("__NV_PRIME_RENDER_OFFLOAD", "1"), ("__GLX_VENDOR_LIBRARY_NAME", "nvidia"), ("__VK_LAYER_NV_optimus", "NVIDIA_only")];

/// An NVIDIA card with its driver loaded.
#[cfg(target_os = "linux")]
fn nvidia_present() -> bool {
    std::fs::read_dir("/proc/driver/nvidia/gpus").is_ok_and(|mut gpus| gpus.next().is_some())
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

/// Dolphin's user folder for the game: the launcher's own, or the player's Dolphin's when they
/// chose it (settings.dolphin_user), for its save, its Miis and the rest of its Wii.
pub fn user_dir(app: &AppHandle, settings: &Settings) -> Result<PathBuf, String> {
    match &settings.dolphin_user {
        Some(dir) if Path::new(dir).is_dir() => Ok(PathBuf::from(dir)),
        Some(dir) => Err(tr!("Le dossier de Dolphin choisi n’existe plus ({dir}) : choisis-en un autre dans Paramètres › Jeu et émulateur.", dir = dir)),
        None => data_dir(app, "dolphin"),
    }
}

/// Wii Party's titles ("SUP" and the region) in a Dolphin NAND, where its save is.
const SAVE_TITLES: [&str; 4] = ["53555050", "53555045", "5355504a", "5355504b"];

fn has_save(user: &Path) -> bool {
    SAVE_TITLES.iter().any(|t| user.join("Wii/title/00010000").join(t).join("data").is_dir())
}

/// A Dolphin user folder holds a Wii: Dolphin made its NAND there.
fn is_dolphin_folder(user: &Path) -> bool {
    user.join("Wii").is_dir()
}

/// Where Dolphin keeps its user folder on this system, installed or as a Flatpak.
fn known_dolphin_folders() -> Vec<PathBuf> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    let mut found = Vec::new();
    if cfg!(target_os = "linux") {
        let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| home.as_ref().map(|h| h.join(".local/share")));
        found.extend(data.map(|d| d.join("dolphin-emu")));
        if let Some(h) = &home {
            found.push(h.join(".var/app/org.DolphinEmu.dolphin-emu/data/dolphin-emu"));
            found.push(h.join(".dolphin-emu"));
        }
    } else if cfg!(windows) {
        found.extend(std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Dolphin Emulator")));
        if let Some(h) = &home {
            found.push(h.join("Documents/Dolphin Emulator"));
        }
    } else if let Some(h) = &home {
        found.push(h.join("Library/Application Support/Dolphin"));
    }
    found
}

#[derive(Serialize)]
pub struct DolphinFolder {
    path: String,
    /// It has a save of Wii Party.
    save: bool,
}

fn describe(user: &Path) -> DolphinFolder {
    DolphinFolder { path: user.to_string_lossy().into_owned(), save: has_save(user) }
}

/// The player's own Dolphin folders found on this system, and the chosen one.
#[tauri::command]
pub fn dolphin_folders(app: AppHandle) -> Vec<DolphinFolder> {
    let chosen = app.state::<Store>().get(&app).dolphin_user.map(PathBuf::from);
    let mut folders: Vec<PathBuf> = known_dolphin_folders().into_iter().filter(|d| is_dolphin_folder(d)).collect();
    if let Some(dir) = chosen.filter(|d| !folders.contains(d)) {
        folders.push(dir);
    }
    folders.iter().map(|d| describe(d)).collect()
}

/// Plays in that Dolphin user folder (None: the launcher's own), once its Miis are in the library.
#[tauri::command]
pub fn dolphin_folder_set(app: AppHandle, path: Option<String>) -> Result<Settings, String> {
    if is_playing(&app) {
        return Err(tr!("Arrête la partie avant de changer de dossier Dolphin."));
    }
    if let Some(dir) = &path {
        let user = Path::new(dir);
        if !is_dolphin_folder(user) {
            return Err(tr!("Ce n’est pas un dossier utilisateur de Dolphin : il n’a pas de dossier Wii. Lance une fois un jeu Wii avec ce Dolphin, ou choisis le dossier qui contient Wii, Config et GameSettings."));
        }
        mii::adopt_folder(&app, user)?;
    }
    let saved = app.state::<Store>().update(&app, |s| s.dolphin_user = path)?;
    Ok(settings::for_ui(&app, saved))
}

/// The same through the folder picker; None when the player cancels.
#[tauri::command]
pub async fn dolphin_folder_pick(app: AppHandle) -> Result<Option<Settings>, String> {
    let dialog = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.dialog().file().set_title(tr!("Choisis le dossier utilisateur de Dolphin")).blocking_pick_folder())
        .await
        .map_err(|e| e.to_string())?;
    let Some(path) = picked.and_then(|f| f.into_path().ok()) else { return Ok(None) };
    dolphin_folder_set(app, Some(path.to_string_lossy().into_owned())).map(Some)
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
        format!("Dolphin.Display.Fullscreen={}", bool_text(s.window_mode != 2)),
        format!("Graphics.Settings.BorderlessFullscreen={}", bool_text(s.window_mode == 1)),
        format!("Dolphin.DSP.Volume={}", s.volume.min(100)),
        // With the bridge, Dolphin's scanner only asks the launcher for the slots (its own
        // Bluetooth scanners are off): it picks up a remote back in its slot at once.
        "Dolphin.Core.WiimoteContinuousScanning=True".to_string(),
        "Dolphin.Interface.ConfirmStop=False".to_string(),
        format!("SYSCONF.IPL.LNG={}", s.console_language.unwrap_or_else(crate::i18n::console_language)),
        format!("SYSCONF.IPL.SND={}", s.sound_mode.min(2)),
        // Surround: the game mixes Dolby Pro Logic II, which only Dolphin's LLE DSP can decode to 5.1.
        format!("Dolphin.Core.DPL2Decoder={}", bool_text(s.sound_mode == 2)),
        format!("Dolphin.Core.DSPHLE={}", bool_text(s.sound_mode != 2)),
        // Where the game expects the bar's dots: 1 on top of the screen, 0 below it.
        format!("SYSCONF.BT.BAR={}", s.sensor_bar_top as u8),
        format!("Graphics.Settings.InternalResolution={}", s.resolution),
        format!("Graphics.Settings.AspectRatio={}", s.aspect),
        format!("Graphics.Hardware.VSync={}", bool_text(s.vsync)),
        format!("Graphics.Settings.ShowFPS={}", bool_text(s.show_fps)),
        // Ubershaders while a new effect's shader compiles, instead of the game
        // freezing for it; the cached ones compiled before the game starts.
        "Graphics.Settings.ShaderCompilationMode=2".to_string(),
        "Graphics.Settings.WaitForShadersBeforeStarting=True".to_string(),
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

async fn run(app: AppHandle, disc: Disc, settings: Settings, force: bool, look: Option<serde_json::Value>) {
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
    // The mods' code and texts, linked into the game's files (on top of the core's, made from the
    // disc the first time).
    if mods::core_to_make(&app, &disc, &packages) {
        set_state(&app, "preparing", None);
    }
    if let Err(e) = mods::link_code(&app, &disc, &packages).await {
        set_state(&app, "error", Some(e));
        return;
    }
    if let Err(e) = mii::write_nand(&app) {
        eprintln!("Mii: {e}");
    }
    if let Err(e) = play(&app, &boot, &settings, look).await {
        set_state(&app, "error", Some(e));
    } else {
        set_state(&app, "idle", None);
    }
}

/// `look`: the page's colours and the loading screen's texts (overlay.rs).
async fn play(app: &AppHandle, boot: &str, settings: &Settings, look: Option<serde_json::Value>) -> Result<(), String> {
    let (_, dolphin) = tools::find(app, "dolphin").ok_or_else(|| tr!("Dolphin est absent de ce build du launcher."))?;
    let user = user_dir(app, settings)?;
    // In the launcher's own folder, even when Dolphin plays in the player's.
    let dir = data_dir(app, "dolphin")?;
    let log = dir.join("launcher-dolphin.log");
    let out = std::fs::File::create(&log).map_err(|e| e.to_string())?;
    let err = out.try_clone().map_err(|e| e.to_string())?;

    // The remotes stay connected to the launcher, which passes them on to Dolphin.
    let wiimotes = app.state::<Wiimotes>().inner().clone();
    let bridge = wiimotes.lend().await;
    let mut cmd = command(&dolphin);
    #[cfg(target_os = "linux")]
    if nvidia_present() {
        cmd.envs(NVIDIA_OFFLOAD);
    }
    // Through the bridge, the game's speaker sound can reach the phones (Dolphin's option is
    // for real remotes, whose Bluetooth can't carry it: the bridge keeps it from them).
    if let Some(bridge) = bridge {
        cmd.env(WIIMOTE_BRIDGE, bridge);
        cmd.args(["-C", "Dolphin.Core.WiimoteEnableSpeaker=True"]);
    }
    let spawned = cmd
        .env(GAME_STATE, "1")
        .env(overlay::ENV, overlay::env(&dir, look))
        .args(dolphin_args(settings, boot, &user))
        .stdin(Stdio::piped())
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
    // "game" or "miis".
    let title = app.state::<Game>().state.lock().unwrap().title;
    let packages = if title == "game" { mods::launch_packages(app, settings).unwrap_or_default() } else { Vec::new() };
    let roots = mods::patch_roots(app, &packages);
    let follower = child.stdout.take().map(|stdout| spawn(follow(app.clone(), stdout, out, roots)));
    if let Some(stdin) = child.stdin.take() {
        overlay::attach(app, stdin);
    }
    // Shown once the game's first screen is there.
    if title == "game" {
        let version = versions::active(app, settings).ok().flatten().map(|v| (v.manifest.name.to_string(), v.manifest.version));
        let names: Vec<String> = settings.enabled_mods.iter().filter_map(|id| packages.iter().find(|m| &m.manifest.id == id)).map(|m| m.manifest.name.to_string()).collect();
        if let Some(toast) = overlay::mods(version, &names) {
            overlay::toast(app, &toast);
        }
    }
    if let Some(toast) = overlay::players(&wiimotes.players().await) {
        overlay::toast(app, &toast);
    }
    let watchers = [spawn(watch_saves(app.clone(), user.clone())), spawn(remind_pauses(app.clone(), title))];

    let window = app.get_webview_window("main");
    if settings.minimize_in_game {
        if let Some(w) = &window {
            let _ = w.minimize();
        }
    }
    let (tx, rx) = oneshot::channel();
    *app.state::<Game>().stop.lock().unwrap() = Some(tx);
    set_state(app, "running", None);
    discord::playing(app, title);

    let status = tokio::select! {
        status = child.wait() => status.map(Some),
        _ = rx => child.kill().await.map(|_| None),
    };

    *app.state::<Game>().stop.lock().unwrap() = None;
    for watcher in &watchers {
        watcher.abort();
    }
    overlay::detach(app);
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
/// `roots`: the packages' folders and names (mods::patch_roots), for the patches it couldn't apply.
async fn follow(app: AppHandle, stdout: ChildStdout, mut log: std::fs::File, roots: Vec<(String, String)>) {
    use std::io::Write;
    let mut stdout = BufReader::new(stdout);
    let mut line = Vec::new();
    // Those of the patches skipped so far, said together a moment after the first.
    let skipped: Arc<Mutex<Option<BTreeSet<String>>>> = Arc::default();
    while stdout.read_until(b'\n', &mut line).await.is_ok_and(|n| n > 0) {
        let _ = log.write_all(&line);
        let text = String::from_utf8_lossy(&line);
        let text = text.trim_end();
        // wp-patch-skipped <address> <folder>
        if let Some((_, root)) = text.strip_prefix("wp-patch-skipped ").and_then(|rest| rest.split_once(' ')) {
            let same = |a: &str, b: &str| a.replace('\\', "/").trim_end_matches('/') == b.replace('\\', "/").trim_end_matches('/');
            let name = roots.iter().find(|(r, _)| same(r, root)).map(|(_, name)| name.clone());
            let mut names = skipped.lock().unwrap();
            if names.is_none() {
                let (app, skipped) = (app.clone(), skipped.clone());
                spawn(async move {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let names: Vec<String> = skipped.lock().unwrap().take().unwrap_or_default().into_iter().collect();
                    overlay::toast(&app, &overlay::patches_skipped(&names));
                });
            }
            names.get_or_insert_with(BTreeSet::new).extend(name);
        } else {
            discord::report(&app, text);
        }
        line.clear();
    }
}

/// Says when the game writes its save in the NAND (Wii/title/00010000/<SUPP or SUPE>/data of
/// Dolphin's user folder), at most every few seconds: it writes several times in a row.
async fn watch_saves(app: AppHandle, user: PathBuf) {
    const EVERY: Duration = Duration::from_secs(2);
    const QUIET: Duration = Duration::from_secs(10);
    // The game reads it at boot; only a new save is said this early.
    const BOOT: Duration = Duration::from_secs(15);
    let dirs = ["53555050", "53555045"].map(|id| user.join("Wii").join("title").join("00010000").join(id).join("data"));
    let latest = || dirs.iter().filter_map(|d| std::fs::read_dir(d).ok()).flatten().flatten().filter_map(|e| e.metadata().ok()?.modified().ok()).max();
    let start = Instant::now();
    let mut last = latest();
    let mut said: Option<Instant> = None;
    loop {
        tokio::time::sleep(EVERY).await;
        let now = latest();
        if now <= last {
            continue;
        }
        let created = last.is_none();
        last = now;
        if (created || start.elapsed() >= BOOT) && said.is_none_or(|t| t.elapsed() >= QUIET) {
            said = Some(Instant::now());
            overlay::toast(&app, &overlay::saved(created));
        }
    }
}

/// Every hour of the game, a word about a break (not in the Mii Channel).
async fn remind_pauses(app: AppHandle, title: &'static str) {
    if title != "game" {
        return;
    }
    for hours in 1.. {
        tokio::time::sleep(Duration::from_secs(3600)).await;
        overlay::toast(&app, &overlay::pause(hours));
    }
}

/// Starts the game with the current image. `force` skips the image check, once
/// the player has decided to go on despite its problems.
#[tauri::command]
pub async fn game_launch(app: AppHandle, force: bool, look: Option<serde_json::Value>) -> Result<(), String> {
    if !app.state::<Wiimotes>().has_player().await {
        return Err(tr!("Connecte au moins une Wiimote avant de lancer le jeu."));
    }
    let settings = app.state::<Store>().get(&app);
    let disc = settings.iso.clone().ok_or_else(|| tr!("Choisis d’abord ton ISO de Wii Party."))?;
    let first = if settings.verify_iso && !force { "verifying" } else { "running" };
    begin(&app, "game", first)?;
    spawn(run(app, disc, settings, force, look));
    Ok(())
}

/// The Mii Channel, from the disc's update partition, with the library in its
/// database; what's made there comes back to the library when it closes.
pub async fn open_mii_channel(app: AppHandle, look: Option<serde_json::Value>) -> Result<(), String> {
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
            play(&app, &wad.to_string_lossy(), &settings, look).await
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
            const CLOSING: u64 = 3;
            overlay::toast(&app, &overlay::mii_saved(CLOSING));
            tokio::time::sleep(Duration::from_secs(CLOSING)).await;
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
    fn finds_wii_partys_save() {
        let user = std::env::temp_dir().join(format!("wp-save-test-{}", std::process::id()));
        std::fs::create_dir_all(user.join("Wii/title/00010000/53555050")).unwrap();
        assert!(is_dolphin_folder(&user) && !has_save(&user));
        std::fs::create_dir_all(user.join("Wii/title/00010000/53555050/data")).unwrap();
        assert!(has_save(&user));
        std::fs::remove_dir_all(&user).unwrap();
    }

    #[test]
    fn passes_settings_to_dolphin() {
        let settings = Settings { window_mode: 2, resolution: 4, volume: 40, console_language: Some(4), sound_mode: 2, sensor_bar_top: true, ..Default::default() };
        let args = dolphin_args(&settings, "/jeux/wp.rvz", Path::new("/data/dolphin"));
        assert_eq!(args[..5], ["-b", "-u", "/data/dolphin", "-e", "/jeux/wp.rvz"]);
        for expected in [
            "Dolphin.Display.Fullscreen=False",
            "Dolphin.DSP.Volume=40",
            "Dolphin.Core.WiimoteContinuousScanning=True",
            "SYSCONF.IPL.LNG=4",
            "SYSCONF.IPL.SND=2",
            "Dolphin.Core.DPL2Decoder=True",
            "Dolphin.Core.DSPHLE=False",
            "SYSCONF.BT.BAR=1",
            "Graphics.Settings.InternalResolution=4",
            "Wiimote.Wiimote4.Source=2",
        ] {
            let at = args.iter().position(|a| a == expected).unwrap_or_else(|| panic!("{expected}"));
            assert_eq!(args[at - 1], "-C");
        }
    }
}
