//! Links mods' code like the launcher does before a launch (src/link), to try it out of the launcher:
//!
//!   cargo run --release --example wplink -- <core dir> <out dir> <unpacked mod dir>[,setting=value…] …
//!
//! $WP_DISC is an extracted disc (sys/main.dol, files/…): the core's files are made from it (its deltas,
//! once, into <out dir>-core beside the out dir, made again for another core or disc), the texts are
//! applied to its message files, and the "sound" patches to its sound archives.
//!
//! Writes <out dir>/linked.xml (Riivolution: section "PartyLauncher", option "Code") and its files
//! (main.dol and rel/<name>.rel.lz of the core, rel/<module>.rel.lz linked, wp/minigames.bin,
//! locale/<language>/mess.bin, sound/wp_sound.brsar…); prints the sound warnings. A core file that a mod
//! replaces whole (its "files" patches, the <file disc=…> of its Riivolution XMLs) is left to it, as the
//! launcher does.

#[path = "../src/delta.rs"]
mod delta;
#[path = "../src/link/mod.rs"]
mod link;
#[path = "../src/lz.rs"]
mod lz;
#[path = "../src/sound/mod.rs"]
mod sound;

use std::collections::BTreeMap;
use std::path::PathBuf;

fn fail(message: &str) -> ! {
    eprintln!("erreur : {message}");
    std::process::exit(1)
}

/// The disc files a package replaces whole: its "files" patches and the <file disc=…/> of its Riivolution
/// XMLs (all their options: the launcher reads the chosen ones only).
fn replaced(dir: &std::path::Path, m: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    for p in m["patches"].as_array().unwrap() {
        if p["type"] == "files" {
            out.extend(p["files"].as_array().unwrap().iter().filter_map(|f| f["disc"].as_str().map(String::from)));
        }
        if p["type"] == "riivolution" {
            let text = std::fs::read_to_string(dir.join(p["xml"].as_str().unwrap())).unwrap_or_default();
            for tag in text.split("<file").skip(1) {
                let Some(at) = tag.find("disc=\"") else { continue };
                let rest = &tag[at + 6..];
                let disc = &rest[..rest.find('"').unwrap_or(rest.len())];
                if !tag[..at].contains('>') && !tag.contains("offset=") {
                    out.push(disc.to_string());
                }
            }
        }
    }
    out.iter().map(|d| format!("/{}", d.trim_start_matches('/').to_lowercase())).collect()
}

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
    let root = PathBuf::from(std::env::var("WP_DISC").unwrap_or_else(|_| {
        eprintln!("WP_DISC : le disque extrait");
        std::process::exit(1)
    }));
    let disc = |path: &str| {
        let file = if path.eq_ignore_ascii_case("main.dol") { root.join("sys/main.dol") } else { root.join("files").join(path.trim_start_matches('/')) };
        std::fs::read(&file).map_err(|e| format!("{} : {e}", file.display()))
    };
    let base = if link::links(&mods) {
        let package = link::corepkg::read(&core).unwrap_or_else(|e| fail(&e));
        let cache = PathBuf::from(format!("{}-core", out.display().to_string().trim_end_matches('/')));
        let id = root.canonicalize().unwrap_or(root.clone()).display().to_string();
        package.make(&core, &cache, &id, &disc).unwrap_or_else(|e| fail(&format!("cœur des mods : {e}")));
        package.files(&cache)
    } else {
        Vec::new()
    };
    let replaced: Vec<String> = dirs.iter().zip(&manifests).flat_map(|((dir, _), m)| replaced(dir, m)).collect();
    match link::link_game(&core, &disc, &base, &replaced, &mods, &out) {
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
