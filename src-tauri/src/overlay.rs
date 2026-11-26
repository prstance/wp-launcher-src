//! What the launcher shows in Dolphin's window, through our build (wp-dolphin's
//! Source/Core/VideoCommon/WPOverlay): a loading screen in the launcher's colours instead of a
//! black window, then notifications at the top right, one JSON object per line on Dolphin's stdin.

use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tokio::io::AsyncWriteExt;
use tokio::process::ChildStdin;
use tokio::sync::mpsc;

use crate::i18n::tr;

/// The loading screen: the launcher's colours and texts (from its page), and its logo.
pub const ENV: &str = "WP_OVERLAY";
const LOGO: &[u8] = include_bytes!("../icons/128x128@2x.png");

/// Below this, a remote's battery is low (%). It's said once, and again only after the remote
/// was above RECOVERED (new batteries, a phone charged).
pub const LOW_BATTERY: u8 = 15;
pub const RECOVERED: u8 = 20;

/// Where the notifications go while Dolphin runs.
#[derive(Default)]
pub struct Overlay(Mutex<Option<mpsc::UnboundedSender<String>>>);

#[derive(Serialize)]
pub struct Toast {
    /// "remotes", "battery" or "info".
    icon: &'static str,
    /// "normal" or "warning".
    tone: &'static str,
    title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    rows: Vec<Row>,
    seconds: f32,
}

/// A player's line: their colour and number, their remote, what's left in it.
#[derive(Serialize)]
struct Row {
    player: usize,
    badge: String,
    label: String,
    value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    battery: Option<u8>,
}

/// WP_OVERLAY's value: what the page sent (its colours, the loading screen's texts) and the logo,
/// written in `dir`.
pub fn env(dir: &Path, page: Option<serde_json::Value>) -> String {
    let mut look = match page {
        Some(serde_json::Value::Object(o)) => o,
        _ => serde_json::Map::new(),
    };
    let logo = dir.join("launcher-logo.png");
    if std::fs::write(&logo, LOGO).is_ok() {
        look.insert("logo".to_string(), logo.to_string_lossy().into_owned().into());
    }
    serde_json::Value::Object(look).to_string()
}

/// Dolphin's stdin, for as long as it runs: the notifications are written there.
pub fn attach(app: &AppHandle, mut stdin: ChildStdin) {
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    *app.state::<Overlay>().0.lock().unwrap() = Some(tx);
    tauri::async_runtime::spawn(async move {
        while let Some(line) = rx.recv().await {
            if stdin.write_all(line.as_bytes()).await.is_err() || stdin.flush().await.is_err() {
                break;
            }
        }
    });
}

pub fn detach(app: &AppHandle) {
    app.state::<Overlay>().0.lock().unwrap().take();
}

/// Shown in the game's window, if it's open.
pub fn toast(app: &AppHandle, toast: &Toast) {
    let Some(overlay) = app.try_state::<Overlay>() else { return };
    let Some(tx) = overlay.0.lock().unwrap().clone() else { return };
    if let Ok(json) = serde_json::to_string(toast) {
        let _ = tx.send(json + "\n");
    }
}

fn row(slot: usize, model: &str, battery: Option<u8>) -> Row {
    Row {
        player: slot,
        badge: tr!("J{n}", n = slot + 1),
        label: if model == "phone" { tr!("Téléphone") } else { model.to_string() },
        // A phone whose browser doesn't tell (iOS).
        value: battery.map_or_else(|| "—".to_string(), |b| tr!("{n} %", n = b)),
        battery,
    }
}

/// When the game starts: each player's remote and its battery.
pub fn players(slots: &[Option<(&'static str, Option<u8>)>]) -> Option<Toast> {
    let rows: Vec<Row> = slots.iter().enumerate().filter_map(|(slot, r)| r.map(|(model, battery)| row(slot, model, battery))).collect();
    if rows.is_empty() {
        return None;
    }
    let low = rows.iter().any(|r| r.battery.is_some_and(|b| b < LOW_BATTERY));
    Some(Toast {
        icon: "remotes",
        tone: if low { "warning" } else { "normal" },
        title: tr!("Manettes"),
        text: if low { tr!("Une batterie est presque vide.") } else { String::new() },
        rows,
        seconds: 7.0,
    })
}

/// A remote's battery went low during the game.
pub fn low_battery(slot: usize, model: &str, level: u8) -> Toast {
    Toast {
        icon: "battery",
        tone: "warning",
        title: tr!("Batterie faible"),
        text: if model == "phone" {
            tr!("Branche le téléphone de J{n} sur un chargeur.", n = slot + 1)
        } else {
            tr!("Les piles de la Wiimote de J{n} sont presque vides.", n = slot + 1)
        },
        rows: vec![row(slot, model, Some(level))],
        seconds: 8.0,
    }
}

/// A player's line without a battery (it isn't known yet, or isn't the point).
fn plain_row(slot: usize, model: &str) -> Row {
    Row { value: String::new(), battery: None, ..row(slot, model, None) }
}

/// A remote dropped out of the game; it keeps its place for when it's back.
pub fn remote_lost(slot: usize, model: &str) -> Toast {
    let phone = model == "phone";
    Toast {
        icon: "remotes",
        tone: "warning",
        title: if phone { tr!("Le téléphone de J{n} est déconnecté", n = slot + 1) } else { tr!("La Wiimote de J{n} est déconnectée", n = slot + 1) },
        text: if phone {
            tr!("Rouvre la page du launcher sur le téléphone : il reprendra sa place.")
        } else {
            tr!("Appuie sur 1 et 2 de la Wiimote : elle reprendra sa place.")
        },
        rows: Vec::new(),
        seconds: 8.0,
    }
}

/// A remote came (back) during the game.
pub fn remote_joined(slot: usize, model: &str, back: bool) -> Toast {
    Toast {
        icon: "remotes",
        tone: "normal",
        title: if back { tr!("J{n} est de retour", n = slot + 1) } else { tr!("J{n} rejoint la partie", n = slot + 1) },
        text: String::new(),
        rows: vec![plain_row(slot, model)],
        seconds: 5.0,
    }
}

/// What points changed for these players (place, model).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Aiming {
    Bar,
    Gyro,
    /// The MotionPlus has to learn its rest value: the remote put down a moment.
    Learning,
    /// It has.
    Ready,
}

pub fn aiming(aim: Aiming, players: &[(usize, &str)]) -> Toast {
    let (title, text) = match aim {
        Aiming::Bar => (tr!("Sensor bar trouvée"), tr!("Le pointeur suit la sensor bar.")),
        Aiming::Gyro => (tr!("Plus de sensor bar"), tr!("Le pointeur suit le gyroscope de la manette.")),
        Aiming::Learning => (tr!("Gyroscope à étalonner"), tr!("Pose la manette à plat sans la toucher un instant : le pointeur se règle.")),
        Aiming::Ready => (tr!("Pointeur prêt"), tr!("Le gyroscope est étalonné.")),
    };
    Toast {
        icon: "pointer",
        tone: if aim == Aiming::Learning { "warning" } else { "normal" },
        title,
        text,
        rows: players.iter().map(|&(slot, model)| plain_row(slot, model)).collect(),
        seconds: if aim == Aiming::Learning { 8.0 } else { 5.0 },
    }
}

/// When the game starts: the version (name, number) and the mods it runs with.
pub fn mods(version: Option<(String, String)>, mods: &[String]) -> Option<Toast> {
    if version.is_none() && mods.is_empty() {
        return None;
    }
    let names = mods.join(", ");
    Some(Toast {
        icon: "mods",
        tone: "normal",
        title: version.as_ref().map_or_else(|| tr!("Mods actifs"), |(name, number)| format!("{name} {number}")),
        text: if mods.is_empty() {
            tr!("Sans autre mod")
        } else if version.is_some() {
            tr!("Avec {names}", names = names)
        } else {
            names
        },
        rows: Vec::new(),
        seconds: 6.0,
    })
}

/// Dolphin couldn't apply some of these packages' patches to the game's memory: what they
/// change isn't where they expect it.
pub fn patches_skipped(names: &[String]) -> Toast {
    Toast {
        icon: "mods",
        tone: "warning",
        title: tr!("Un mod ne s’applique pas entièrement"),
        text: if names.is_empty() {
            tr!("Une partie du jeu n’est pas celle que le mod attend : il n’est peut-être pas fait pour cette version du jeu.")
        } else {
            tr!("{names} : une partie du jeu n’est pas celle qu’il attend. Il n’est peut-être pas fait pour cette version du jeu.", names = names.join(", "))
        },
        rows: Vec::new(),
        seconds: 10.0,
    }
}

/// The game wrote its save.
pub fn saved(created: bool) -> Toast {
    Toast {
        icon: "save",
        tone: "normal",
        title: if created { tr!("Sauvegarde créée") } else { tr!("Partie sauvegardée") },
        text: String::new(),
        rows: Vec::new(),
        seconds: 4.0,
    }
}

/// The Mii Channel closes in `seconds`, a Mii saved.
pub fn mii_saved(seconds: u64) -> Toast {
    Toast {
        icon: "mii",
        tone: "normal",
        title: tr!("Mii enregistré"),
        text: tr!("Retour au launcher dans {n} s : le Mii rejoint ta bibliothèque.", n = seconds),
        rows: Vec::new(),
        seconds: seconds as f32,
    }
}

/// `hours` of play.
pub fn pause(hours: u64) -> Toast {
    Toast {
        icon: "clock",
        tone: "normal",
        title: tr!("Petite pause ?"),
        text: tr!("Vous jouez depuis {n} h : buvez un peu d’eau et dégourdissez-vous les jambes.", n = hours),
        rows: Vec::new(),
        seconds: 9.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_each_players_battery() {
        let toast = players(&[Some(("Wiimote", Some(85))), None, Some(("phone", None)), None]).unwrap();
        let json = serde_json::to_value(&toast).unwrap();
        assert_eq!(json["icon"], "remotes");
        assert_eq!(json["tone"], "normal");
        assert!(json.get("text").is_none());
        let rows = json["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0]["player"].as_u64(), rows[0]["battery"].as_u64()), (Some(0), Some(85)));
        assert_eq!(rows[1]["player"], 2);
        assert!(rows[1].get("battery").is_none());
        assert_eq!(rows[1]["value"], "—");
        assert!(players(&[None, None, None, None]).is_none());
    }

    #[test]
    fn a_low_battery_is_a_warning() {
        let toast = players(&[Some(("Wiimote", Some(10)))]).unwrap();
        assert_eq!(toast.tone, "warning");
        let low = serde_json::to_value(low_battery(1, "phone", 12)).unwrap();
        assert_eq!((low["icon"].as_str(), low["tone"].as_str()), (Some("battery"), Some("warning")));
        assert_eq!(low["rows"][0]["player"], 1);
    }

    #[test]
    fn says_the_version_and_its_mods() {
        assert!(mods(None, &[]).is_none());
        let both = mods(Some(("Winter Mode".into(), "2026.12.5".into())), &["A".into(), "B".into()]).unwrap();
        assert_eq!(both.title, "Winter Mode 2026.12.5");
        assert!(both.text.contains("A, B"));
        assert_eq!(mods(Some(("W".into(), "1".into())), &[]).unwrap().text, tr!("Sans autre mod"));
        assert_eq!(mods(None, &["A".into()]).unwrap().text, "A");
    }

    #[test]
    fn a_players_row_without_battery_says_nothing() {
        let json = serde_json::to_value(remote_joined(2, "phone", true)).unwrap();
        assert_eq!(json["rows"][0]["value"], "");
        assert!(json["rows"][0].get("battery").is_none());
        assert_eq!(json["rows"][0]["player"], 2);
    }

    #[test]
    fn the_loading_screen_gets_the_logo() {
        let dir = std::env::temp_dir().join("wp-overlay-test");
        std::fs::create_dir_all(&dir).unwrap();
        let full: serde_json::Value = serde_json::from_str(&env(&dir, Some(serde_json::json!({"title": "Wii Party"})))).unwrap();
        assert_eq!(full["title"], "Wii Party");
        let logo = full["logo"].as_str().unwrap();
        assert!(std::fs::read(logo).unwrap().starts_with(b"\x89PNG"));
        // Nothing from the page: Dolphin's own colours.
        let bare: serde_json::Value = serde_json::from_str(&env(&dir, None)).unwrap();
        assert!(bare["logo"].is_string());
    }
}
