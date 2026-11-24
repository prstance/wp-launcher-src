//! Discord's Rich Presence: what the player's friends see on their profile, from
//! the launcher down to the minigame being played ("Board Game Island", then
//! "Minigame: Timber Topple"). Discord's local socket, or named pipe on Windows,
//! takes frames of JSON; without Discord, nothing is shown and nothing fails.

use crate::i18n::{self, tr};
use crate::minigames;
use crate::modes;
use crate::mods;
use crate::settings::{Settings, Store};
use crate::versions;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::watch;
use tokio::time::{sleep, timeout};

/// The application made for the launcher on discord.com/developers: its name is
/// the game Discord shows ("Playing Wii Party"), and its art assets hold LARGE_IMAGE.
const APP_ID: &str = "1567945956723269693";
const LARGE_IMAGE: &str = "logo";
const HANDSHAKE: u32 = 0;
const FRAME: u32 = 1;
const CLOSE: u32 = 2;
/// Discord may open after the game, or restart during it.
const RETRY: Duration = Duration::from_secs(15);
const REPLY: Duration = Duration::from_secs(5);
/// Discord takes 5 activities every 20 seconds; scenes can change faster.
const GAP: Duration = Duration::from_secs(4);
/// The game's scenes before and around its main menu.
const MENUS: [&str; 4] = ["boot", "loading", "openmess", "menu"];

trait Duplex: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Duplex for T {}
type Pipe = Box<dyn Duplex>;

pub struct Presence {
    wanted: watch::Sender<Option<Value>>,
    opened: u64,
    now: Mutex<Now>,
}

#[derive(Default)]
struct Now {
    /// What Dolphin runs ("game" or "miis") and since when, in Unix seconds; none:
    /// the launcher alone.
    playing: Option<(&'static str, u64)>,
    /// Where the game is, as our Dolphin tells.
    spot: Option<Spot>,
    /// The names of the game modes and of the minigames, by id, once read.
    modes: BTreeMap<i32, String>,
    minigames: BTreeMap<i32, String>,
}

/// What our Dolphin prints when the game moves (WPGameState.h in the fork):
/// "wp-state <scene> <mode> <minigame>".
#[derive(Clone, PartialEq, Debug)]
struct Spot {
    scene: String,
    mode: i32,
    minigame: i32,
}

impl Spot {
    fn parse(line: &str) -> Option<Spot> {
        let mut words = line.strip_prefix("wp-state ")?.split(' ');
        let spot = Spot { scene: words.next()?.to_string(), mode: words.next()?.parse().ok()?, minigame: words.next()?.parse().ok()? };
        // "-" between two scenes: the last one still counts.
        (spot.scene != "-").then_some(spot)
    }

    fn in_minigame(&self) -> bool {
        self.scene.starts_with("mg") || self.scene == "inst"
    }
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn with_now<R>(app: &AppHandle, change: impl FnOnce(&mut Now) -> R) -> R {
    let presence = app.state::<Presence>();
    let mut now = presence.now.lock().unwrap();
    change(&mut now)
}

pub fn start() -> Presence {
    let (wanted, rx) = watch::channel(None);
    tauri::async_runtime::spawn(keep(rx));
    Presence { wanted, opened: unix_now(), now: Mutex::new(Now::default()) }
}

/// Dolphin has started `title`. The names of the game's modes and minigames come
/// a moment later: the modes' are read from the disc.
pub fn playing(app: &AppHandle, title: &'static str) {
    with_now(app, |now| {
        now.playing = Some((title, unix_now()));
        now.spot = None;
    });
    refresh(app);
    if title != "game" {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let modes = modes::game_mode_names(&app, i18n::lang()).await.unwrap_or_else(|e| {
            eprintln!("Discord: {e}");
            BTreeMap::new()
        });
        let settings = app.state::<Store>().get(&app);
        let packages = mods::launch_packages(&app, &settings).unwrap_or_default();
        let minigames = minigames::names(settings.iso.as_ref().map_or("", |d| d.game_id.as_str()), &packages);
        with_now(&app, |now| (now.modes, now.minigames) = (modes, minigames));
        refresh(&app);
    });
}

/// A line of Dolphin's output: where the game now is, if it tells.
pub fn report(app: &AppHandle, line: &str) {
    let Some(spot) = Spot::parse(line) else { return };
    let moved = with_now(app, |now| {
        let moved = now.playing.is_some() && now.spot.as_ref() != Some(&spot);
        if moved {
            now.spot = Some(spot);
        }
        moved
    });
    if moved {
        refresh(app);
    }
}

pub fn stopped(app: &AppHandle) {
    with_now(app, |now| {
        now.playing = None;
        now.spot = None;
    });
    refresh(app);
}

/// Shows where the player is, as the settings now allow.
pub fn refresh(app: &AppHandle) {
    let presence = app.state::<Presence>();
    let settings = app.state::<Store>().get(app);
    if !settings.discord_presence || APP_ID.is_empty() {
        presence.wanted.send_replace(None);
        return;
    }
    let now = presence.now.lock().unwrap();
    let summary = now.playing.filter(|(title, _)| *title == "game").map(|_| summary(app, &settings));
    let (details, state) = place(&now, summary.as_deref());
    let since = now.playing.map_or(presence.opened, |(_, since)| since);
    let mut activity = json!({
        "details": details,
        "timestamps": { "start": since },
        "assets": { "large_image": LARGE_IMAGE, "large_text": summary.as_deref().unwrap_or("PartyLauncher") },
    });
    if let Some(state) = state {
        activity["state"] = state.into();
    }
    presence.wanted.send_replace(Some(activity));
}

/// The version and the number of mods the game runs with: "Mode Hiver · 2 mods".
fn summary(app: &AppHandle, settings: &Settings) -> String {
    let version = versions::active(app, settings).ok().flatten();
    let packages = mods::launch_packages(app, settings).unwrap_or_default();
    let n = packages.iter().filter(|p| Some(&p.manifest.id) != settings.active_version.as_ref() && p.manifest.id != mods::DEBUG_TOOLS).count();
    let mods = match n {
        0 => tr!("sans mod"),
        1 => tr!("1 mod"),
        _ => tr!("{n} mods", n = n),
    };
    let version = version.map_or_else(|| tr!("Jeu d’origine"), |v| v.manifest.name.to_string());
    format!("{version} · {mods}")
}

/// Where the player is, from the widest to the narrowest: the launcher, the game's
/// menu, a mode, a minigame. Discord's two lines: details, then state.
fn place(now: &Now, summary: Option<&str>) -> (String, Option<String>) {
    match (now.playing, &now.spot) {
        (None, _) => (tr!("Dans le launcher"), None),
        (Some(("miis", _)), _) => (tr!("Dans la Chaîne Mii"), Some(tr!("Crée un Mii"))),
        // Our Dolphin hasn't told yet, or doesn't.
        (Some(_), None) => (summary.unwrap_or("Wii Party").to_string(), None),
        (Some(_), Some(spot)) if MENUS.contains(&spot.scene.as_str()) => (tr!("Menu principal"), summary.map(str::to_string)),
        (Some(_), Some(spot)) => {
            let mode = now.modes.get(&spot.mode).cloned();
            if !spot.in_minigame() {
                return (mode.unwrap_or_else(|| summary.unwrap_or("Wii Party").to_string()), summary.map(str::to_string));
            }
            // The minigame the game picked, else the scene's own number.
            let minigame = now.minigames.get(&spot.minigame).or_else(|| spot.scene.strip_prefix("mg")?.parse().ok().and_then(|id| now.minigames.get(&id)));
            let state = minigame.map_or_else(|| tr!("Dans un mini-jeu"), |name| tr!("Mini-jeu : {name}", name = name));
            (mode.unwrap_or_else(|| tr!("Mini-jeu")), Some(state))
        }
    }
}

/// Keeps Discord showing the latest activity; none closes the connection, which
/// clears it.
async fn keep(mut wanted: watch::Receiver<Option<Value>>) {
    let mut pipe: Option<Pipe> = None;
    let mut nonce = 0u64;
    loop {
        let activity = wanted.borrow_and_update().clone();
        match &activity {
            None => pipe = None,
            Some(activity) => {
                if pipe.is_none() {
                    pipe = connect().await;
                }
                if let Some(p) = &mut pipe {
                    nonce += 1;
                    match set_activity(p, activity, nonce).await {
                        Ok(()) => sleep(GAP).await,
                        Err(e) => {
                            eprintln!("Discord: {e}");
                            pipe = None;
                        }
                    }
                }
            }
        }
        let retry = activity.is_some() && pipe.is_none();
        tokio::select! {
            changed = wanted.changed() => if changed.is_err() { return },
            _ = sleep(RETRY), if retry => {}
        }
    }
}

async fn connect() -> Option<Pipe> {
    let mut pipe = open().await?;
    match send(
        &mut pipe,
        HANDSHAKE,
        &json!({ "v": 1, "client_id": APP_ID }),
    )
    .await
    {
        Ok(_) => Some(pipe),
        Err(e) => {
            eprintln!("Discord: {e}");
            None
        }
    }
}

/// Discord's socket: discord-ipc-0 to -9, in a temporary folder, or in the
/// Flatpak's or the Snap's own.
#[cfg(unix)]
async fn open() -> Option<Pipe> {
    let dirs = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .iter()
        .filter_map(std::env::var_os)
        .map(std::path::PathBuf::from)
        .chain(["/tmp".into()]);
    for dir in dirs {
        for sub in ["", "app/com.discordapp.Discord", "snap.discord"] {
            for n in 0..10 {
                if let Ok(socket) =
                    tokio::net::UnixStream::connect(dir.join(sub).join(format!("discord-ipc-{n}")))
                        .await
                {
                    return Some(Box::new(socket));
                }
            }
        }
    }
    None
}

#[cfg(windows)]
async fn open() -> Option<Pipe> {
    use tokio::net::windows::named_pipe::ClientOptions;
    (0..10)
        .find_map(|n| {
            ClientOptions::new()
                .open(format!(r"\\.\pipe\discord-ipc-{n}"))
                .ok()
        })
        .map(|p| Box::new(p) as Pipe)
}

async fn set_activity<P: AsyncRead + AsyncWrite + Unpin>(
    pipe: &mut P,
    activity: &Value,
    nonce: u64,
) -> io::Result<()> {
    let command = json!({ "cmd": "SET_ACTIVITY", "args": { "pid": std::process::id(), "activity": activity }, "nonce": nonce.to_string() });
    let reply = send(pipe, FRAME, &command).await?;
    // Refused (an asset missing, a text too long…): the connection is still good.
    if reply["evt"] == "ERROR" {
        eprintln!("Discord: {}", reply["data"]["message"]);
    }
    Ok(())
}

/// One frame, `op` then the length of the JSON (little-endian), and Discord's
/// answer to it.
async fn send<P: AsyncRead + AsyncWrite + Unpin + ?Sized>(
    pipe: &mut P,
    op: u32,
    body: &Value,
) -> io::Result<Value> {
    let body = serde_json::to_vec(body)?;
    let mut frame = Vec::with_capacity(8 + body.len());
    frame.extend(op.to_le_bytes());
    frame.extend((body.len() as u32).to_le_bytes());
    frame.extend(body);
    pipe.write_all(&frame).await?;

    let read = async {
        let mut head = [0; 8];
        pipe.read_exact(&mut head).await?;
        let (op, len) = (
            u32::from_le_bytes(head[..4].try_into().unwrap()),
            u32::from_le_bytes(head[4..].try_into().unwrap()),
        );
        if len > 64 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "réponse trop longue",
            ));
        }
        let mut reply = vec![0; len as usize];
        pipe.read_exact(&mut reply).await?;
        let reply: Value = serde_json::from_slice(&reply)?;
        if op == CLOSE {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                reply["message"].to_string(),
            ));
        }
        Ok(reply)
    };
    timeout(REPLY, read)
        .await
        .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn frame(pipe: &mut tokio::io::DuplexStream) -> (u32, Value) {
        let mut head = [0; 8];
        pipe.read_exact(&mut head).await.unwrap();
        let mut body = vec![0; u32::from_le_bytes(head[4..].try_into().unwrap()) as usize];
        pipe.read_exact(&mut body).await.unwrap();
        (
            u32::from_le_bytes(head[..4].try_into().unwrap()),
            serde_json::from_slice(&body).unwrap(),
        )
    }

    async fn answer(pipe: &mut tokio::io::DuplexStream, op: u32, body: Value) {
        let body = serde_json::to_vec(&body).unwrap();
        pipe.write_all(&[op.to_le_bytes(), (body.len() as u32).to_le_bytes()].concat())
            .await
            .unwrap();
        pipe.write_all(&body).await.unwrap();
    }

    #[test]
    fn reads_dolphins_lines() {
        assert_eq!(Spot::parse("wp-state mg101 18 101"), Some(Spot { scene: "mg101".into(), mode: 18, minigame: 101 }));
        assert_eq!(Spot::parse("wp-state - 0 0"), None);
        assert_eq!(Spot::parse("Core/Boot: something else"), None);
    }

    #[test]
    fn tells_where_the_player_is_widest_first() {
        let mut now = Now { modes: [(0, "Île aux défis".to_string())].into(), minigames: [(101, "Timber Topple".to_string())].into(), ..Now::default() };
        assert_eq!(place(&now, None), (tr!("Dans le launcher"), None));
        now.playing = Some(("game", 0));
        assert_eq!(place(&now, Some("Mode Hiver · sans mod")), ("Mode Hiver · sans mod".into(), None));
        let at = |scene: &str, minigame| Some(Spot { scene: scene.into(), mode: 0, minigame });
        now.spot = at("menu", 0);
        assert_eq!(place(&now, Some("x")), (tr!("Menu principal"), Some("x".into())));
        now.spot = at("mr001", 0);
        assert_eq!(place(&now, Some("x")), ("Île aux défis".into(), Some("x".into())));
        now.spot = at("mg101", 101);
        assert_eq!(place(&now, Some("x")), ("Île aux défis".into(), Some(tr!("Mini-jeu : {name}", name = "Timber Topple"))));
        now.spot = at("mg447", 447);
        assert_eq!(place(&now, Some("x")).1, Some(tr!("Dans un mini-jeu")));
    }

    #[tokio::test]
    async fn speaks_discords_frames() {
        let (mut launcher, mut discord) = tokio::io::duplex(4096);
        let fake = tokio::spawn(async move {
            assert_eq!(
                frame(&mut discord).await,
                (HANDSHAKE, json!({ "v": 1, "client_id": "42" }))
            );
            answer(
                &mut discord,
                FRAME,
                json!({ "cmd": "DISPATCH", "evt": "READY" }),
            )
            .await;
            let (op, command) = frame(&mut discord).await;
            assert_eq!(
                (op, &command["cmd"], &command["args"]["activity"]["state"]),
                (FRAME, &json!("SET_ACTIVITY"), &json!("Sans mod"))
            );
            answer(
                &mut discord,
                FRAME,
                json!({ "cmd": "SET_ACTIVITY", "evt": "ERROR", "data": { "message": "no asset" } }),
            )
            .await;
            frame(&mut discord).await;
            answer(
                &mut discord,
                CLOSE,
                json!({ "code": 4000, "message": "Invalid Client ID" }),
            )
            .await;
        });
        let ready = send(
            &mut launcher,
            HANDSHAKE,
            &json!({ "v": 1, "client_id": "42" }),
        )
        .await
        .unwrap();
        assert_eq!(ready["evt"], "READY");
        // Refused by Discord, the activity leaves the connection open.
        set_activity(&mut launcher, &json!({ "state": "Sans mod" }), 1)
            .await
            .unwrap();
        let closed = send(&mut launcher, FRAME, &json!({})).await.unwrap_err();
        assert_eq!(closed.kind(), io::ErrorKind::ConnectionRefused);
        fake.await.unwrap();
    }
}
