//! Links mods' code like the launcher does before a launch (src/link), to try it out of the launcher:
//!
//!   cargo run --release --example wplink -- <core dir> <out dir> <unpacked mod dir>[,setting=value…] …
//!
//! The texts are applied to the disc's message files, and the "sound" patches to the disc's sound
//! archives, read from $WP_DISC/files (an extracted disc).
//!
//! Writes <out dir>/linked.xml (Riivolution: section "PartyLauncher", option "Code") and its files
//! (rel/<module>.rel.lz, locale/<language>/mess.bin, sound/wp_sound.brsar…); prints the sound warnings.

#[path = "../src/link/mod.rs"]
mod link;
#[path = "../src/lz.rs"]
mod lz;
#[path = "../src/sound/mod.rs"]
mod sound;

use std::collections::BTreeMap;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (core, out) = (PathBuf::from(&args[0]), PathBuf::from(&args[1]));
    let mut dirs = Vec::new();
    let mut manifests = Vec::new();
    for spec in &args[2..] {
        let mut it = spec.split(',');
        let dir = PathBuf::from(it.next().unwrap());
        let values: BTreeMap<String, i64> = it.map(|kv| {
            let (k, v) = kv.split_once('=').unwrap();
            (k.to_string(), if let Some(h) = v.strip_prefix("0x") { i64::from_str_radix(h, 16).unwrap() } else { v.parse().unwrap() })
        }).collect();
        let m: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("mod.json")).or_else(|_| std::fs::read_to_string(dir.join("version.json"))).unwrap()).unwrap();
        dirs.push((dir, values));
        manifests.push(m);
    }
    let mut mods = Vec::new();
    for ((dir, values), m) in dirs.iter().zip(&manifests) {
        let mut modules = BTreeMap::new();
        for p in m["patches"].as_array().unwrap().iter().filter(|p| p["type"] == "code") {
            let parts: BTreeMap<String, link::CodeModule> = serde_json::from_value(p["modules"].clone()).unwrap();
            modules.extend(parts);
        }
        let mut writes = Vec::new();
        for s in m["settings"].as_array().into_iter().flatten() {
            let Some(sym) = s.get("symbol") else { continue };
            let v = values.get(s["id"].as_str().unwrap()).copied().unwrap_or(s["default"].as_i64().unwrap());
            let size = sym["size"].as_u64().unwrap() as usize;
            let bytes = v.to_be_bytes()[8 - size..].to_vec();
            writes.push((sym["module"].as_str().unwrap().to_string(), sym["name"].as_str().unwrap().to_string(), sym.get("offset").and_then(|o| o.as_u64()).unwrap_or(0) as u32, bytes));
        }
        let messages = serde_json::from_value(m.get("messages").cloned().unwrap_or_default()).unwrap_or_default();
        let messages_with = serde_json::from_value(m.get("messages_with").cloned().unwrap_or_default()).unwrap_or_default();
        let minigames = m.get("minigames").filter(|v| !v.is_null()).map(|v| serde_json::from_value(v.clone()).unwrap()).unwrap_or_default();
        let sounds = m["patches"].as_array().unwrap().iter().filter(|p| p["type"] == "sound").map(|p| serde_json::from_value(p.clone()).unwrap()).collect();
        mods.push(link::CodeMod { id: m["id"].as_str().unwrap(), dir, modules, writes, messages, messages_with, minigames, sounds });
    }
    let disc = |path: &str| {
        let root = std::env::var("WP_DISC").map_err(|_| "WP_DISC : le disque extrait".to_string())?;
        std::fs::read(PathBuf::from(root).join("files").join(path.trim_start_matches('/'))).map_err(|e| format!("{path} : {e}"))
    };
    match link::link_game(&core, &disc, &mods, &out) {
        Ok(Some(l)) => {
            for w in &l.warnings {
                eprintln!("son : {w}");
            }
            println!("{} ({:?})", l.xml.display(), l.files)
        }
        Ok(None) => println!("aucun code"),
        Err(e) => {
            eprintln!("erreur : {} ({:?})", e.message, e.mods);
            std::process::exit(1);
        }
    }
}
