//! The player's Miis: the launcher's library, their heads, and the bridge with
//! the Wii's own Mii database in Dolphin's NAND, where Wii Party and the Mii
//! Channel find them. The player's account (pseudo and Mii) is in the settings.

pub mod data;
pub mod disc;
pub mod render;

use crate::game::{self, data_dir};
use crate::i18n::tr;
use crate::settings::Store;
use base64::Engine;
use data::Raw;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

/// As many as the Wii's database holds.
pub const MAX: usize = 100;
/// The game's own Miis (mii_rsd.bin).
const DISC_MIIS: i64 = 100;
const LIBRARY: &str = "library.json";
const HEAD_SIZE: u32 = 192;
/// Changes with the renderer, so that the heads drawn before are drawn again.
const HEAD_VERSION: u64 = 1;
const CHANGED: &str = "miis://changed";
const FILE_TYPES: [&str; 6] = ["mii", "miigx", "rsd", "rcd", "dat", "bin"];
const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// RFLGetFavoriteColor, as CSS.
const FAVORITE_COLORS: [&str; 12] =
    ["#b84030", "#f07828", "#f8d820", "#80c828", "#007428", "#204898", "#40a0d8", "#e86078", "#702ca8", "#483818", "#e0e0e0", "#181814"];

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    /// Each Mii's 74 bytes in base64, in the library's order.
    miis: Vec<String>,
    /// The console half of the IDs of the Miis the launcher makes, in hex.
    system: String,
}

struct Book {
    miis: Vec<Raw>,
    system: [u8; 4],
}

#[derive(Default)]
pub struct Library {
    book: Mutex<Option<Book>>,
    faces: Mutex<Option<Arc<render::Resources>>>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MiiView {
    id: String,
    name: String,
    creator: String,
    female: bool,
    /// The Mii's favorite color, as CSS.
    color: &'static str,
    birthday: Option<(u16, u16)>,
    /// How a "mii" setting of a package names it (setting_key).
    key: i64,
    /// PNG data URL; None until the face parts are taken from the disc.
    head: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MiisState {
    miis: Vec<MiiView>,
    /// The face parts and the game's Miis are taken from the disc.
    faces: bool,
    max: usize,
    /// The system's user name, to suggest a pseudo.
    suggested_name: String,
}

#[derive(Serialize)]
pub struct Imported {
    state: MiisState,
    /// IDs of the Miis added or updated.
    added: Vec<String>,
    refused: Vec<String>,
}

fn io(e: impl ToString) -> String {
    e.to_string()
}

fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    data_dir(app, "miis")
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn load(app: &AppHandle) -> Book {
    let saved: Saved = dir(app)
        .ok()
        .and_then(|d| std::fs::read(d.join(LIBRARY)).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let miis = saved.miis.iter().filter_map(|m| B64.decode(m).ok()?.try_into().ok()).filter(data::is_valid).collect();
    let system = (0..4).map(|i| u8::from_str_radix(saved.system.get(i * 2..i * 2 + 2)?, 16).ok()).collect::<Option<Vec<u8>>>();
    // A console of its own, picked once: the time is as good as anything.
    let system = system.and_then(|s| s.try_into().ok()).unwrap_or_else(|| (now() as u32 | 0x0100_0000).to_be_bytes());
    Book { miis, system }
}

fn save(app: &AppHandle, book: &Book) -> Result<(), String> {
    let saved = Saved { miis: book.miis.iter().map(|m| B64.encode(m)).collect(), system: book.system.iter().map(|b| format!("{b:02x}")).collect() };
    let path = dir(app)?.join(LIBRARY);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&saved).map_err(io)?).map_err(io)?;
    std::fs::rename(&tmp, &path).map_err(io)
}

impl Library {
    /// Reads or changes the library, which is saved after a change.
    fn with<T>(&self, app: &AppHandle, change: impl FnOnce(&mut Book) -> Result<T, String>) -> Result<T, String> {
        let mut guard = self.book.lock().unwrap();
        let book = guard.get_or_insert_with(|| load(app));
        let result = change(book)?;
        save(app, book)?;
        Ok(result)
    }

    fn miis(&self, app: &AppHandle) -> Vec<Raw> {
        self.book.lock().unwrap().get_or_insert_with(|| load(app)).miis.clone()
    }

    /// The face parts, once taken from the disc.
    fn faces(&self, app: &AppHandle) -> Option<Arc<render::Resources>> {
        let mut faces = self.faces.lock().unwrap();
        if faces.is_none() {
            let bytes = std::fs::read(disc::dir(app).ok()?.join(disc::RESOURCES_FILE)).ok()?;
            *faces = render::Resources::parse(bytes).map(Arc::new).map_err(|e| eprintln!("RFL_Res.dat: {e}")).ok();
        }
        faces.clone()
    }
}

/// FNV-1a, to name a head after what it shows.
fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325 ^ HEAD_VERSION, |h, &b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

/// The head as a data URL, drawn once and kept in miis/heads.
fn head(app: &AppHandle, faces: Option<&render::Resources>, raw: &Raw) -> Option<String> {
    let faces = faces?;
    let cache = dir(app).ok()?.join("heads");
    let file = cache.join(format!("{:016x}.png", hash(raw)));
    let png = match std::fs::read(&file) {
        Ok(png) => png,
        Err(_) => {
            let png = render::head_png(faces, raw, HEAD_SIZE).map_err(|e| eprintln!("Mii {}: {e}", data::id(raw))).ok()?;
            let _ = std::fs::create_dir_all(&cache).and_then(|_| std::fs::write(&file, &png));
            png
        }
    };
    Some(format!("data:image/png;base64,{}", B64.encode(png)))
}

/// How a "mii" setting names a Mii, for the mod that reads it (wp-core mods/bgi-settings): the
/// game's own Miis (mii_rsd.bin), which all have the same create ID, by their place, 1 to 100; the
/// library's (the console's database in the game) by their create ID: 1 << 48, the two halves of
/// the console's 4 bytes xor'ed, the Mii's own 4 bytes. Below 2^53: the page keeps it exact.
pub fn setting_key(raw: &Raw, disc_index: Option<usize>) -> i64 {
    if let Some(index) = disc_index {
        return index as i64 + 1;
    }
    let id = data::create_id(raw);
    let console = u16::from_be_bytes([id[4], id[5]]) ^ u16::from_be_bytes([id[6], id[7]]);
    1 << 48 | (console as i64) << 32 | u32::from_be_bytes(id[..4].try_into().unwrap()) as i64
}

/// A value a "mii" setting may take (setting_key, or 0: the game's choice).
pub fn is_setting_key(value: i64) -> bool {
    (0..=DISC_MIIS).contains(&value) || value >> 48 == 1
}

fn view(id: String, raw: &Raw, head: Option<String>, disc_index: Option<usize>) -> MiiView {
    MiiView {
        key: setting_key(raw, disc_index),
        id,
        name: data::name(raw),
        creator: data::creator(raw),
        female: data::female(raw),
        color: FAVORITE_COLORS[data::favorite_color(raw) as usize],
        birthday: data::birthday(raw),
        head,
    }
}

fn suggested_name() -> String {
    let user = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default();
    let mut chars = user.chars().filter(|c| c.is_alphanumeric());
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).take(10).collect(),
        None => String::new(),
    }
}

/// Draws what's missing; can take a while the first time.
async fn state(app: &AppHandle) -> Result<MiisState, String> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let library = app.state::<Library>();
        let faces = library.faces(&app);
        let miis = library.miis(&app).iter().map(|m| view(data::id(m), m, head(&app, faces.as_deref(), m), None)).collect();
        MiisState { miis, faces: faces.is_some(), max: MAX, suggested_name: suggested_name() }
    })
    .await
    .map_err(io)
}

fn changed(app: &AppHandle) {
    let _ = app.emit(CHANGED, ());
}

/// Takes what the Miis need from the disc in the background, if not done yet.
pub fn prepare(app: &AppHandle) {
    let Some(disc) = app.state::<Store>().get(app).iso else { return };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let had = disc::dir(&app).is_ok_and(|d| [disc::RESOURCES_FILE, disc::MIIS_FILE, disc::NAMES_FILE].iter().all(|f| d.join(f).is_file()));
        match disc::extract_faces(&app, &disc).await {
            Ok(()) if !had => changed(&app),
            Ok(()) => {}
            Err(e) => eprintln!("Mii: {e}"),
        }
    });
}

fn nand_database(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app, "dolphin")?.join("Wii/shared2/menu/FaceLib/RFL_DB.dat"))
}

/// Puts the library in Dolphin's Mii database, the player's Mii first, before
/// Dolphin starts: Wii Party and the Mii Channel see exactly the library.
pub fn write_nand(app: &AppHandle) -> Result<(), String> {
    let mut miis = app.state::<Library>().miis(app);
    let mine = app.state::<Store>().get(app).player.and_then(|p| p.mii);
    if let Some(at) = mine.and_then(|id| miis.iter().position(|m| data::id(m) == id)) {
        let me = miis.remove(at);
        miis.insert(0, me);
    }
    let path = nand_database(app)?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(io)?;
    std::fs::write(&path, data::write_database(std::fs::read(&path).ok(), &miis)).map_err(io)
}

/// The Miis of Dolphin's database, unless it is being written.
pub fn nand_miis(app: &AppHandle) -> Option<Vec<Raw>> {
    let bytes = std::fs::read(nand_database(app).ok()?).ok()?;
    data::is_whole(&bytes).then(|| data::read_database(&bytes))
}

/// Tells when the Mii Channel has saved a Mii, from what its database holds over time.
pub struct SaveWatch {
    before: Vec<Raw>,
    seen: Option<Vec<Raw>>,
}

impl SaveWatch {
    pub fn new(before: Vec<Raw>) -> SaveWatch {
        SaveWatch { before, seen: None }
    }

    /// `now`: the database's Miis, None while it is being written. `settling`: the
    /// channel is still starting and may write its database without the player.
    pub fn saved(&mut self, now: Option<Vec<Raw>>, settling: bool) -> bool {
        let Some(now) = now else {
            self.seen = None;
            return false;
        };
        if settling || now == self.before {
            self.before = now;
            self.seen = None;
            return false;
        }
        // Different, and the same twice in a row: the channel is done writing.
        if self.seen.as_ref() == Some(&now) {
            return true;
        }
        self.seen = Some(now);
        false
    }
}

/// After the Mii Channel: takes in the Miis made or changed there. The ones
/// deleted there stay in the library, which only the launcher empties.
pub fn read_nand(app: &AppHandle) -> Result<usize, String> {
    let found = data::read_database(&std::fs::read(nand_database(app)?).unwrap_or_default());
    let count = add(app, found)?.len();
    changed(app);
    Ok(count)
}

fn full() -> String {
    tr!("La bibliothèque est pleine : {max} Mii au plus, comme sur la Wii.", max = MAX)
}

/// Adds Miis, or updates the ones already there; their IDs.
fn add(app: &AppHandle, miis: Vec<Raw>) -> Result<Vec<String>, String> {
    app.state::<Library>().with(app, |book| {
        let mut ids = Vec::new();
        for mii in miis {
            match book.miis.iter().position(|m| data::create_id(m) == data::create_id(&mii)) {
                Some(at) if book.miis[at] == mii => continue,
                Some(at) => book.miis[at] = mii,
                None if book.miis.len() >= MAX => return Err(full()),
                None => book.miis.push(mii),
            }
            ids.push(data::id(&mii));
        }
        Ok(ids)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mii(n: u8) -> Raw {
        let mut raw = [0; data::SIZE];
        raw[0x18] = 0x80;
        raw[0x1F] = n;
        raw
    }

    #[test]
    fn names_a_mii_for_the_settings() {
        let mut raw = mii(1);
        data::set_create_id(&mut raw, [0x87, 0x65, 0x43, 0x21, 0x12, 0x34, 0x00, 0x00]);
        assert_eq!(setting_key(&raw, None), 0x0001_1234_8765_4321);
        assert_eq!(setting_key(&raw, Some(24)), 25);
        assert!(is_setting_key(0) && is_setting_key(100) && is_setting_key(0x0001_1234_8765_4321));
        assert!(!is_setting_key(101) && !is_setting_key(-1) && !is_setting_key(2 << 48));
    }

    #[test]
    fn sees_a_saved_mii_once_it_is_written() {
        let mut watch = SaveWatch::new(vec![mii(1)]);
        // The channel starting up and rewriting its database isn't a save.
        assert!(!watch.saved(Some(vec![mii(1), mii(9)]), true));
        assert!(!watch.saved(Some(vec![mii(1), mii(9)]), false));
        // A new Mii: half written, then whole, then still the same.
        assert!(!watch.saved(None, false));
        assert!(!watch.saved(Some(vec![mii(1), mii(9), mii(2)]), false));
        assert!(watch.saved(Some(vec![mii(1), mii(9), mii(2)]), false));
    }
}

#[tauri::command]
pub async fn miis_state(app: AppHandle) -> Result<MiisState, String> {
    state(&app).await
}

/// The game's Miis, under the names the game gives them (mii_rsd.bin only has Japanese ones).
fn disc_miis(app: &AppHandle) -> Result<Vec<Raw>, String> {
    let dir = disc::dir(app)?;
    let bytes = std::fs::read(dir.join(disc::MIIS_FILE)).map_err(|_| tr!("Choisis d’abord ton ISO de Wii Party."))?;
    let names: Vec<String> = std::fs::read(dir.join(disc::NAMES_FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let mut miis = data::read_store(&bytes);
    for (mii, name) in miis.iter_mut().zip(&names) {
        data::set_name(mii, name);
    }
    Ok(miis)
}

/// The game's own Miis, to start from.
#[tauri::command]
pub async fn miis_disc(app: AppHandle) -> Result<Vec<MiiView>, String> {
    if let Some(disc) = app.state::<Store>().get(&app).iso {
        disc::extract_faces(&app, &disc).await?;
    }
    tauri::async_runtime::spawn_blocking(move || {
        let faces = app.state::<Library>().faces(&app);
        Ok(disc_miis(&app)?.iter().enumerate().map(|(i, m)| view(i.to_string(), m, head(&app, faces.as_deref(), m), Some(i))).collect())
    })
    .await
    .map_err(io)?
}

#[derive(Serialize)]
pub struct Adopted {
    state: MiisState,
    id: String,
}

/// Copies one of the game's Miis into the library as the player's own: new ID, their name.
#[tauri::command]
pub async fn miis_adopt(app: AppHandle, index: usize, name: String) -> Result<Adopted, String> {
    let mut mii = *disc_miis(&app)?.get(index).ok_or_else(|| tr!("Ce Mii n’existe pas."))?;
    data::set_name(&mut mii, &name);
    data::set_creator(&mut mii, &name);
    let id = app.state::<Library>().with(&app, |book| {
        if book.miis.len() >= MAX {
            return Err(full());
        }
        let taken: Vec<[u8; 8]> = book.miis.iter().map(data::create_id).collect();
        data::set_create_id(&mut mii, data::new_create_id(book.system, now(), &taken));
        book.miis.push(mii);
        Ok(data::id(&mii))
    })?;
    Ok(Adopted { state: state(&app).await?, id })
}

/// Takes in Mii files; a file can hold several Miis.
#[tauri::command]
pub async fn miis_import(app: AppHandle, paths: Vec<PathBuf>) -> Result<Imported, String> {
    let (mut added, mut refused) = (Vec::new(), Vec::new());
    for path in paths {
        let file = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let read = std::fs::read(&path).map_err(io).and_then(|bytes| data::read_file(&bytes));
        match read.and_then(|miis| add(&app, miis)) {
            Ok(ids) => added.extend(ids),
            Err(why) => refused.push(tr!("{file} : {why}.", file = file, why = why)),
        }
    }
    Ok(Imported { state: state(&app).await?, added, refused })
}

#[tauri::command]
pub async fn miis_pick(app: AppHandle) -> Result<Option<Imported>, String> {
    let dialog = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog.dialog().file().set_title(tr!("Choisis tes fichiers Mii")).add_filter(tr!("Mii de Wii"), &FILE_TYPES).blocking_pick_files()
    })
    .await
    .map_err(io)?;
    let Some(files) = picked else { return Ok(None) };
    miis_import(app, files.into_iter().filter_map(|f| f.into_path().ok()).collect()).await.map(Some)
}

fn change(app: &AppHandle, id: &str, edit: impl FnOnce(&mut Book, usize)) -> Result<(), String> {
    app.state::<Library>().with(app, |book| {
        let at = book.miis.iter().position(|m| data::id(m) == id).ok_or_else(|| tr!("Ce Mii n’existe pas."))?;
        edit(book, at);
        Ok(())
    })
}

#[tauri::command]
pub async fn miis_rename(app: AppHandle, id: String, name: String) -> Result<MiisState, String> {
    if name.trim().is_empty() {
        return Err(tr!("Donne-lui un nom."));
    }
    change(&app, &id, |book, at| data::set_name(&mut book.miis[at], &name))?;
    state(&app).await
}

#[tauri::command]
pub async fn miis_remove(app: AppHandle, id: String) -> Result<MiisState, String> {
    change(&app, &id, |book, at| {
        book.miis.remove(at);
    })?;
    app.state::<Store>().update(&app, |s| {
        if let Some(player) = s.player.as_mut().filter(|p| p.mii.as_deref() == Some(&id)) {
            player.mii = None;
        }
    })?;
    state(&app).await
}

/// Saves a Mii as a .mii file, which the Wii's tools and other launchers read.
#[tauri::command]
pub async fn miis_export(app: AppHandle, id: String) -> Result<bool, String> {
    let mii = app.state::<Library>().miis(&app).into_iter().find(|m| data::id(m) == id).ok_or_else(|| tr!("Ce Mii n’existe pas."))?;
    let dialog = app.clone();
    let name = format!("{}.mii", data::name(&mii).replace(['/', '\\', ':'], "_"));
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.dialog().file().set_file_name(name).add_filter(tr!("Mii de Wii"), &["mii"]).blocking_save_file())
        .await
        .map_err(io)?;
    let Some(path) = picked.and_then(|f| f.into_path().ok()) else { return Ok(false) };
    std::fs::write(Path::new(&path), mii).map_err(io)?;
    Ok(true)
}

/// Opens the Mii Channel, taken from the disc, in Dolphin; the Miis made there join the library.
#[tauri::command]
pub async fn miis_channel(app: AppHandle) -> Result<(), String> {
    game::open_mii_channel(app).await
}
