//! The game's modes on the home screen: their picture (the thumbnails of
//! layout/inst/thumbnail.arc) and their name (mess.bin), in the launcher's language,
//! from the player's own disc. The active version changes them by replacing those
//! files, as it does in the game, or with pictures of its theme (versions.rs).

use crate::i18n::Text;
use crate::mii::{disc, render};
use crate::mods::Installed;
use crate::settings::{Disc, Store};
use crate::versions;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use tauri::{AppHandle, Manager};

struct Mode {
    id: &'static str,
    /// Its picture in thumbnail.arc.
    thumbnail: &'static str,
    /// Its name in mess.bin, as the main menu writes it.
    name: &'static str,
    /// Its family in the main menu: "party", "pair" or "house".
    group: &'static str,
}

const fn mode(id: &'static str, thumbnail: &'static str, name: &'static str, group: &'static str) -> Mode {
    Mode { id, thumbnail, name, group }
}

/// In the main menu's order.
const MODES: [Mode; 13] = [
    mode("board-game-island", "mr001", "mn_gtex013", "party"),
    mode("globe-trot", "mr031", "mn_gtex014", "party"),
    mode("mii-of-a-kind", "mr041", "mn_gtex015", "party"),
    mode("spin-off", "mr011", "mn_gtex016", "party"),
    mode("bingo", "mr741", "mn_gtex017", "party"),
    mode("friend-connection", "mr062", "mn_gtex018", "pair"),
    mode("balance-boat", "mr051", "mn_gtex019", "pair"),
    mode("match-up", "mr083", "mn_gtex022", "pair"),
    mode("animal-tracker", "mr721", "mn_gtex023", "house"),
    mode("hide-n-hunt", "mr731", "mn_gtex024", "house"),
    mode("time-bomb", "mr711", "mn_gtex025", "house"),
    mode("word-bomb", "mr713", "mn_gtex026", "house"),
    mode("buddy-quiz", "mr701", "mn_gtex027", "house"),
];

const THUMBNAILS: &str = "layout/inst/thumbnail.arc.lz";
const TEXTS: &str = "mess.bin";

/// The game modes by their id in the save block (wii_party_notes.txt), and their
/// name in mess.bin; those of the Minigames menu follow its own name.
const GAME_MODES: [(i32, &str); 20] = [
    (0, "mn_gtex013"),
    (1, "mn_gtex016"),
    (3, "mn_gtex014"),
    (4, "mn_gtex015"),
    (5, "mn_gtex019"),
    (6, "mn_gtex019"),
    (7, "mn_gtex018"),
    (8, "mn_gtex022"),
    (9, "mn_gtex027"),
    (11, "mn_gtex025"),
    (12, "mn_gtex026"),
    (13, "mn_gtex023"),
    (15, "mn_gtex024"),
    (16, "mn_gtex017"),
    (17, "mn_gtex012"),
    (18, "mn_gtex007"),
    (19, "mn_gtex009"),
    (20, "mn_gtex009"),
    (22, "mn_gtex011"),
    (23, "mn_gtex010"),
];
const MINIGAMES_MENU: &str = "mn_gtex004";
/// The first of the Minigames menu's modes.
const FIRST_MINIGAME_MODE: i32 = 17;

/// One at a time: the first call takes the files from the disc.
static BUSY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A mode of the game, or one a version brings ("version").
pub fn is_known(id: &str) -> bool {
    MODES.iter().any(|m| m.id == id)
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ModeView {
    id: String,
    name: String,
    group: String,
    /// A `data:` URL.
    image: String,
    /// The picture is the disc's own: the page may show its own screenshot instead.
    stock: bool,
}

/// The texts' folder for the launcher's language (`sp_EU` for Spanish), else English.
fn locale_folder(files: &[String], lang: &str) -> Option<String> {
    let code = if lang == "es" { "sp" } else { lang };
    let folders: Vec<&str> = files.iter().filter_map(|f| f.strip_suffix(&format!("/{TEXTS}"))).filter(|f| f.starts_with("locale/")).collect();
    [format!("locale/{code}_"), "locale/en_".into()]
        .iter()
        .find_map(|prefix| folders.iter().find(|f| f.starts_with(prefix.as_str())))
        .or(folders.first())
        .map(|f| f.to_string())
}

/// The game's markup out of a text: colors and ruby ([c:#…], [r:…]…[/r]), line breaks.
fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in text.chars() {
        match c {
            '[' => tag = true,
            ']' if tag => tag = false,
            '\n' if !tag => out.push(' '),
            _ if !tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// The first image of a TPL file, as PNG.
fn tpl_png(tpl: &[u8]) -> Result<Vec<u8>, String> {
    let word = |at: usize| tpl.get(at..at + 4).map(|b| u32::from_be_bytes(b.try_into().unwrap()) as usize).ok_or("TPL tronqué");
    if word(0)? != 0x0020_AF30 || word(4)? == 0 {
        return Err("TPL invalide".into());
    }
    let header = word(word(8)?)?;
    let size = word(header)?;
    let (height, width, format, data) = (size >> 16, size & 0xFFFF, word(header + 4)?, word(header + 8)?);
    let texels = render::texture::decode_gx(format as u8, width, height, tpl.get(data..).ok_or("TPL tronqué")?)?;
    render::png(width as u32, height as u32, texels.as_flattened())
}

fn data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
}

/// The modes found in a thumbnail.arc.lz and a mess.bin. `disc`: the disc's own
/// thumbnail.arc.lz when a version replaces it, to tell which pictures it changed.
fn read(thumbnails: &[u8], texts: &[u8], disc: Option<&[u8]>) -> Result<Vec<ModeView>, String> {
    let arc = disc::decompress_lz(thumbnails)?;
    let original = disc.map(disc::decompress_lz).transpose()?;
    let texts = disc::xmsg_texts(texts).ok_or("mess.bin illisible")?;
    Ok(MODES
        .iter()
        .filter_map(|m| {
            let file = format!("{}.tpl", m.thumbnail);
            let tpl = disc::u8_file(&arc, &file)?;
            let png = tpl_png(&tpl).map_err(|e| eprintln!("{}: {e}", m.thumbnail)).ok()?;
            let name = texts.get(m.name).map(|t| plain(t)).filter(|n| !n.is_empty())?;
            let stock = original.as_ref().is_none_or(|o| disc::u8_file(o, &file).as_ref() == Some(&tpl));
            Some(ModeView { id: m.id.into(), name, group: m.group.into(), image: data_url(&png), stock })
        })
        .collect())
}

/// The disc's files for the launcher's language: the folder of its texts and their
/// bytes, taken from the disc once.
struct FromDisc {
    folder: String,
    thumbnails: Vec<u8>,
    texts: Vec<u8>,
}

async fn from_disc(app: &AppHandle, disc: &Disc, lang: &str) -> Result<FromDisc, String> {
    let cache = disc::dir(app)?.join("modes").join(format!("{}-{lang}", disc.game_id));
    let folder_file = cache.join("folder");
    if let (Ok(folder), Ok(thumbnails), Ok(texts)) =
        (std::fs::read_to_string(&folder_file), std::fs::read(cache.join("thumbnail.arc.lz")), std::fs::read(cache.join(TEXTS)))
    {
        return Ok(FromDisc { folder, thumbnails, texts });
    }
    let files = disc::list(app, disc, "DATA", Some("locale")).await?;
    let folder = locale_folder(&files, lang).ok_or("le disque n’a pas de textes (locale/*/mess.bin)")?;
    let texts = disc::extract(app, disc, "DATA", &format!("{folder}/{TEXTS}")).await?;
    // Each language has its own, with its own words on the pictures.
    let thumbnails = match disc::extract(app, disc, "DATA", &format!("{folder}/{THUMBNAILS}")).await {
        Ok(bytes) => bytes,
        Err(_) => disc::extract(app, disc, "DATA", THUMBNAILS).await?,
    };
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    for (name, bytes) in [("thumbnail.arc.lz", &thumbnails), (TEXTS, &texts), ("folder", &folder.clone().into_bytes())] {
        std::fs::write(cache.join(name), bytes).map_err(|e| e.to_string())?;
    }
    Ok(FromDisc { folder, thumbnails, texts })
}

/// What the version replaces of those files, as the game would load it: the
/// thumbnails, the disc's ones if the version replaced them, and the texts.
fn with_version(found: FromDisc, version: Option<&Installed>) -> (Vec<u8>, Option<Vec<u8>>, Vec<u8>) {
    let replaced = |path: String| version.and_then(|v| v.footprint.source(&path)).and_then(|p| std::fs::read(p).ok());
    let texts = replaced(format!("/{}/{TEXTS}", found.folder)).unwrap_or(found.texts);
    match replaced(format!("/{}/{THUMBNAILS}", found.folder)) {
        Some(thumbnails) => (thumbnails, Some(found.thumbnails), texts),
        None => (found.thumbnails, None, texts),
    }
}

/// The version's own pictures, first and in its order: in place of a mode's, or for
/// modes of its own.
fn with_theme(mut modes: Vec<ModeView>, art: Vec<versions::ModeArt>) -> Vec<ModeView> {
    let mut first = Vec::new();
    for art in art {
        let name = art.name.as_ref().map(Text::to_string);
        let mode = match modes.iter().position(|m| m.id == art.mode) {
            Some(at) => ModeView { image: art.image, stock: false, ..modes.remove(at) },
            None => ModeView { id: art.mode, name: String::new(), group: "version".into(), image: art.image, stock: false },
        };
        first.push(ModeView { name: name.unwrap_or(mode.name.clone()), ..mode });
    }
    first.extend(modes);
    first
}

/// The modes to show, in `lang`. Without an ISO, or when the version's theme says
/// so (modesOnly), only those of its theme.
#[tauri::command]
pub async fn modes_get(app: AppHandle, lang: String) -> Result<Vec<ModeView>, String> {
    let _busy = BUSY.lock().await;
    let settings = app.state::<Store>().get(&app);
    let version = versions::active(&app, &settings)?;
    let art = version.as_ref().map(versions::mode_art).unwrap_or_default();
    let Some(disc) = settings.iso.as_ref().filter(|_| !version.as_ref().is_some_and(versions::only_its_modes)) else {
        return Ok(with_theme(Vec::new(), art));
    };
    let (thumbnails, original, texts) = with_version(from_disc(&app, disc, &lang).await?, version.as_ref());
    let modes = tauri::async_runtime::spawn_blocking(move || read(&thumbnails, &texts, original.as_deref())).await.map_err(|e| e.to_string())??;
    Ok(with_theme(modes, art))
}

/// The game modes' names by their id, as the menu writes them with the active
/// version, in `lang` ("Minigames · Free Play" for those of the Minigames menu).
pub async fn game_mode_names(app: &AppHandle, lang: &str) -> Result<BTreeMap<i32, String>, String> {
    let _busy = BUSY.lock().await;
    let settings = app.state::<Store>().get(app);
    let disc = settings.iso.as_ref().ok_or("pas d’ISO")?;
    let version = versions::active(app, &settings)?;
    let (_, _, texts) = with_version(from_disc(app, disc, lang).await?, version.as_ref());
    let texts = disc::xmsg_texts(&texts).ok_or("mess.bin illisible")?;
    Ok(mode_names(&texts))
}

fn mode_names(texts: &HashMap<String, String>) -> BTreeMap<i32, String> {
    let name = |key: &str| texts.get(key).map(|t| plain(t)).filter(|n| !n.is_empty());
    let menu = name(MINIGAMES_MENU);
    GAME_MODES
        .iter()
        .filter_map(|&(id, key)| {
            let mode = name(key)?;
            Some((id, match &menu {
                Some(menu) if id >= FIRST_MINIGAME_MODE => format!("{menu} · {mode}"),
                _ => mode,
            }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_markup_out_of_a_name() {
        assert_eq!(plain("[c:#dc2814ff]Board Game Island[c:#3c3c3cff]"), "Board Game Island");
        assert_eq!(plain("Tú eres el\nprotagonista"), "Tú eres el protagonista");
        assert_eq!(plain("ミニゲーム[r:たいかい]大会[/r]"), "ミニゲーム大会");
    }

    #[test]
    fn puts_the_versions_pictures_first() {
        let mode = |id: &str| ModeView { id: id.into(), name: id.into(), group: "party".into(), image: "disc".into(), stock: true };
        let art = |mode: &str, name: Option<&str>| versions::ModeArt { mode: mode.into(), image: "theme".into(), name: name.map(|n| Text::One(n.into())) };
        let modes = with_theme(vec![mode("a"), mode("b")], vec![art("new", Some("Neuf")), art("b", None)]);
        let shown: Vec<_> = modes.iter().map(|m| (m.id.as_str(), m.name.as_str(), m.image.as_str(), m.stock)).collect();
        assert_eq!(shown, [("new", "Neuf", "theme", false), ("b", "b", "theme", false), ("a", "a", "disc", true)]);
    }

    #[test]
    fn names_the_game_modes() {
        let texts: HashMap<String, String> =
            [("mn_gtex004", "Mini-jeux"), ("mn_gtex013", "[c:#dc2814ff]Ile aux défis"), ("mn_gtex007", "Libre")].map(|(k, v)| (k.into(), v.into())).into();
        let names = mode_names(&texts);
        assert_eq!((names[&0].as_str(), names[&18].as_str()), ("Ile aux défis", "Mini-jeux · Libre"));
        assert!(!names.contains_key(&3));
    }

    #[test]
    fn picks_the_texts_of_the_language() {
        let files: Vec<String> = ["locale/en_EU/mess.bin", "locale/fr_EU/mess.bin", "locale/sp_EU/mess.bin", "locale/fr_EU/layout/inst/thumbnail.arc.lz"]
            .map(String::from)
            .into();
        assert_eq!(locale_folder(&files, "fr").as_deref(), Some("locale/fr_EU"));
        assert_eq!(locale_folder(&files, "es").as_deref(), Some("locale/sp_EU"));
        assert_eq!(locale_folder(&files[1..], "en").as_deref(), Some("locale/fr_EU"));
    }

    /// Against the files extracted from the real disc, when they are there.
    #[test]
    #[ignore]
    fn reads_the_disc_modes() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../original/fst/files/locale/fr_EU");
        let thumbnails = std::fs::read(root.join(THUMBNAILS)).unwrap();
        let modes = read(&thumbnails, &std::fs::read(root.join(TEXTS)).unwrap(), Some(&thumbnails)).unwrap();
        assert_eq!(modes.len(), MODES.len());
        assert_eq!((modes[0].name.as_str(), modes[1].name.as_str()), ("Ile aux défis", "Globe-trotter"));
        assert!(modes[0].image.starts_with("data:image/png;base64,"));
        assert!(modes.iter().all(|m| m.stock));
    }
}
