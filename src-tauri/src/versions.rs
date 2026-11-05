//! Versions: .wpver packages. Underneath they are mods (mods.rs), but big enough to
//! be the game the player starts from: one at a time, loaded before any mod, and
//! they bring the launcher's theme. The ones in src-tauri/versions/ ship with the
//! launcher and install themselves on its first start.

use crate::game::{data_dir, is_playing};
use crate::i18n::{tr, Text};
use crate::mods::{check_image, data_url, install, installed, peek, satisfies, size_of, Installed, Kind, Manifest, Values, MAX_IMAGE};
use crate::settings::{Settings, Store};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::OnceCell;

/// What a fresh install starts on.
pub const FIRST: &str = "prstance.mode-hiver";

pub const WPVER: Kind = Kind { manifest: "version.json", extension: "wpver", check: check_theme };
/// The launcher's CSS variables (app.css) a theme may set.
const COLORS: [&str; 19] = [
    "bg", "tint", "accent", "accent-soft", "accent-deep", "label", "grad-top", "grad-bottom", "text", "muted", "surface",
    "surface-soft", "line", "off", "shadow", "scrim", "rail", "rail-line", "dot",
];
/// The two colors used inside rgba(): three numbers instead of #rrggbb.
const TRIPLES: [&str; 2] = ["shadow", "scrim"];
/// The launcher's movements for an image (.motion-* in app.css).
const MOTIONS: [&str; 4] = ["fall", "float", "wobble", "bob"];

/// Only data: the launcher's page can call its commands, so a package never gets
/// to put code or markup in it. Images only ever go in an <img>, where an SVG's
/// scripts don't run and it can't load anything.
#[derive(Serialize, Deserialize, Clone)]
pub struct Theme {
    colors: BTreeMap<String, String>,
    /// Sentence under the title of the home screen.
    #[serde(default)]
    tagline: Option<Text>,
    /// Illustration of the home screen, one image per moving part.
    #[serde(default)]
    hero: Vec<Decor>,
    /// Images scattered behind each screen.
    #[serde(default)]
    home: Vec<Decor>,
    #[serde(default)]
    library: Vec<Decor>,
    /// Pictures of the game's modes on the home screen (modes.rs), or of its own modes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    modes: Vec<ModeArt>,
    /// Only those: none of the game's.
    #[serde(default, rename = "modesOnly")]
    modes_only: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ModeArt {
    /// One of the game's (modes.rs: "board-game-island"…), or a new one.
    pub mode: String,
    pub image: String,
    /// Instead of the game's; a new mode needs one.
    #[serde(default)]
    pub name: Option<Text>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Decor {
    image: String,
    x: f64,
    y: f64,
    size: f64,
    #[serde(default = "opaque")]
    opacity: f64,
    #[serde(default)]
    motion: Option<String>,
}

fn opaque() -> f64 {
    1.0
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionView {
    id: String,
    name: Text,
    version: String,
    authors: Vec<String>,
    description: Text,
    games: Vec<String>,
    size: u64,
    active: bool,
    icon: Option<String>,
    settings: usize,
    /// Comes with the launcher, which would put it back: it can't be removed.
    shipped: bool,
}

#[derive(Serialize)]
pub struct VersionsState {
    versions: Vec<VersionView>,
    /// The active version's, with its images inlined. None: the launcher's own.
    theme: Option<Theme>,
    /// Why a version shipped with the launcher couldn't be installed.
    error: Option<String>,
}

#[derive(Serialize)]
pub struct Imported {
    #[serde(flatten)]
    state: VersionsState,
    added: Vec<String>,
    refused: Vec<String>,
}

#[derive(Default, Clone)]
struct Prepared {
    shipped: Vec<String>,
    error: Option<String>,
}

/// Done once, by whoever asks first; the others wait for it.
#[derive(Default)]
pub struct Shipped(OnceCell<Prepared>);

fn io(e: impl ToString) -> String {
    e.to_string()
}

fn color_fits(name: &str, value: &str) -> bool {
    if TRIPLES.contains(&name) {
        let parts: Vec<&str> = value.split(',').collect();
        return parts.len() == 3 && parts.iter().all(|n| n.trim().parse::<u8>().is_ok());
    }
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn decor_of(theme: &Theme) -> impl Iterator<Item = &Decor> {
    theme.hero.iter().chain(&theme.home).chain(&theme.library)
}

fn check_theme(package: &Path, manifest: &Manifest) -> Result<(), String> {
    let theme = manifest.theme.as_ref().ok_or_else(|| tr!("il lui manque son thème"))?;
    for (name, value) in &theme.colors {
        if !COLORS.contains(&name.as_str()) {
            return Err(tr!("son thème définit une couleur inconnue : « {name} »", name = name));
        }
        if !color_fits(name, value) {
            return Err(tr!("la couleur « {name} » de son thème n’est pas valide : « {value} »", name = name, value = value));
        }
    }
    for decor in decor_of(theme) {
        if let Some(motion) = decor.motion.as_ref().filter(|m| !MOTIONS.contains(&m.as_str())) {
            return Err(tr!("son thème demande un mouvement inconnu : « {motion} »", motion = motion));
        }
        check_image(package, &decor.image, MAX_IMAGE)?;
    }
    for art in &theme.modes {
        let unnamed = art.name.as_ref().is_none_or(Text::is_blank);
        if art.mode.trim().is_empty() || (unnamed && (theme.modes_only || !crate::modes::is_known(&art.mode))) {
            return Err(tr!("son thème montre un mode sans nom : « {mode} »", mode = art.mode));
        }
        check_image(package, &art.image, MAX_IMAGE)?;
    }
    Ok(())
}

/// Whether its theme shows only its own modes on the home screen.
pub fn only_its_modes(version: &Installed) -> bool {
    version.manifest.theme.as_ref().is_some_and(|t| t.modes_only)
}

/// The pictures of modes of a version's theme, with `data:` URLs.
pub fn mode_art(version: &Installed) -> Vec<ModeArt> {
    let Some(theme) = &version.manifest.theme else { return Vec::new() };
    theme.modes.iter().filter_map(|art| Some(ModeArt { image: data_url(&version.dir, &art.image)?, ..art.clone() })).collect()
}

/// The theme as the page uses it: `data:` URLs instead of paths inside the package.
fn for_page(version: &Installed) -> Option<Theme> {
    let mut theme = version.manifest.theme.clone()?;
    // The page gets them with the game's (modes_get).
    theme.modes.clear();
    for decor in [&mut theme.hero, &mut theme.home, &mut theme.library] {
        decor.retain_mut(|d| {
            data_url(&version.dir, &d.image).map(|url| d.image = url).is_some()
        });
    }
    Some(theme)
}

fn shipped_packages(app: &AppHandle) -> Vec<PathBuf> {
    // During development they come straight from src-tauri/versions.
    let dir = if cfg!(debug_assertions) {
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("versions"))
    } else {
        app.path().resource_dir().ok().map(|dir| dir.join("versions"))
    };
    let Some(dir) = dir else { return Vec::new() };
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case(WPVER.extension)))
        .collect()
}

/// Installs a shipped version unless the same one or a newer one is there.
/// Says which it is, and whether it was just installed.
fn install_shipped(package: &Path, dir: &Path) -> Result<(String, bool), String> {
    let manifest = peek(&WPVER, package)?;
    let there = installed(&WPVER, dir, &Values::new()).into_iter().find(|v| v.manifest.id == manifest.id);
    if there.is_some_and(|v| satisfies(&v.manifest.version, &format!(">={}", manifest.version))) {
        return Ok((manifest.id, false));
    }
    install(&WPVER, package, dir)?;
    Ok((manifest.id, true))
}

/// A mod that became a version must not load a second time.
fn forget_mod(app: &AppHandle, id: &str) -> Result<(), String> {
    let old = data_dir(app, "mods")?.join(id);
    if old.is_dir() {
        std::fs::remove_dir_all(old).map_err(io)?;
    }
    app.state::<Store>().update(app, |s| s.enabled_mods.retain(|m| m != id))?;
    Ok(())
}

fn prepare(app: &AppHandle) -> Prepared {
    let mut prepared = Prepared::default();
    let mut errors = Vec::new();
    for package in shipped_packages(app) {
        let done = data_dir(app, "versions").and_then(|dir| install_shipped(&package, &dir)).and_then(|(id, fresh)| {
            if fresh {
                forget_mod(app, &id)?;
            }
            Ok(id)
        });
        match done {
            Ok(id) => prepared.shipped.push(id),
            Err(why) => errors.push(tr!("{file} : {why}.", file = package.file_name().unwrap_or_default().to_string_lossy(), why = why)),
        }
    }
    if !errors.is_empty() {
        prepared.error = Some(errors.join(" "));
    }
    prepared
}

async fn prepared(app: &AppHandle) -> Prepared {
    let job = app.clone();
    let cell = app.state::<Shipped>();
    let done = cell.0.get_or_init(|| async move {
        tauri::async_runtime::spawn_blocking(move || prepare(&job))
            .await
            .unwrap_or_else(|e| Prepared { shipped: Vec::new(), error: Some(e.to_string()) })
    });
    done.await.clone()
}

/// The version the game starts on. An uninstalled one counts as none.
pub fn active(app: &AppHandle, settings: &Settings) -> Result<Option<Installed>, String> {
    let Some(id) = &settings.active_version else { return Ok(None) };
    Ok(all(app, &settings.package_settings)?.into_iter().find(|v| &v.manifest.id == id))
}

/// Every installed version, with the saved values of its settings.
pub fn all(app: &AppHandle, saved: &Values) -> Result<Vec<Installed>, String> {
    Ok(installed(&WPVER, &data_dir(app, "versions")?, saved))
}

pub fn installed_ids(app: &AppHandle) -> Result<Vec<String>, String> {
    Ok(all(app, &Values::new())?.into_iter().map(|v| v.manifest.id).collect())
}

/// The versions that come with the launcher.
pub async fn shipped(app: &AppHandle) -> Vec<String> {
    prepared(app).await.shipped
}

async fn current(app: &AppHandle) -> Result<VersionsState, String> {
    let Prepared { shipped, error } = prepared(app).await;
    let settings = app.state::<Store>().get(app);
    let all = all(app, &settings.package_settings)?;
    let is_active = |v: &Installed| settings.active_version.as_ref() == Some(&v.manifest.id);
    Ok(VersionsState {
        theme: all.iter().find(|v| is_active(v)).and_then(for_page),
        versions: all
            .iter()
            .map(|v| VersionView {
                id: v.manifest.id.clone(),
                name: v.manifest.name.clone(),
                version: v.manifest.version.clone(),
                authors: v.manifest.authors.clone(),
                description: v.manifest.description.clone(),
                games: v.manifest.games.clone(),
                size: size_of(&v.dir),
                active: is_active(v),
                shipped: shipped.contains(&v.manifest.id),
                icon: v.icon(),
                settings: v.settings_count(),
            })
            .collect(),
        error,
    })
}

fn refuse_while_playing(app: &AppHandle) -> Result<(), String> {
    if is_playing(app) {
        return Err(tr!("Arrête la partie avant de changer de version."));
    }
    Ok(())
}

#[tauri::command]
pub async fn versions_state(app: AppHandle) -> Result<VersionsState, String> {
    current(&app).await
}

/// `id`: None goes back to Wii Party as it came.
#[tauri::command]
pub async fn versions_activate(app: AppHandle, id: Option<String>) -> Result<VersionsState, String> {
    refuse_while_playing(&app)?;
    if id.as_ref().is_some_and(|id| !installed_ids(&app).is_ok_and(|ids| ids.contains(id))) {
        return Err(tr!("Cette version n’est plus installée."));
    }
    app.state::<Store>().update(&app, |s| s.active_version = id)?;
    current(&app).await
}

/// Installs files; the last one that works becomes the active version.
#[tauri::command]
pub async fn versions_import(app: AppHandle, paths: Vec<PathBuf>) -> Result<Imported, String> {
    refuse_while_playing(&app)?;
    prepared(&app).await;
    let dir = data_dir(&app, "versions")?;
    let results = tauri::async_runtime::spawn_blocking(move || {
        paths.iter().map(|p| (p.clone(), install(&WPVER, p, &dir))).collect::<Vec<_>>()
    })
    .await
    .map_err(io)?;
    let (mut added, mut refused) = (Vec::new(), Vec::new());
    for (path, result) in results {
        match result {
            Ok(manifest) => {
                forget_mod(&app, &manifest.id)?;
                app.state::<Store>().update(&app, |s| s.active_version = Some(manifest.id.clone()))?;
                added.push(manifest.name.to_string());
            }
            Err(why) => refused.push(tr!("{file} : {why}.", file = path.file_name().unwrap_or_default().to_string_lossy(), why = why)),
        }
    }
    Ok(Imported { state: current(&app).await?, added, refused })
}

/// Opens the file picker, for those who don't drag.
#[tauri::command]
pub async fn versions_pick(app: AppHandle) -> Result<Option<Imported>, String> {
    let dialog = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog.dialog().file().set_title(tr!("Choisis une version")).add_filter(tr!("Versions Wii Party"), &[WPVER.extension]).blocking_pick_file()
    })
    .await
    .map_err(io)?;
    let Some(path) = picked.and_then(|f| f.into_path().ok()) else { return Ok(None) };
    versions_import(app, vec![path]).await.map(Some)
}

#[tauri::command]
pub async fn versions_remove(app: AppHandle, id: String) -> Result<VersionsState, String> {
    refuse_while_playing(&app)?;
    if prepared(&app).await.shipped.contains(&id) {
        return Err(tr!("Cette version est fournie avec le launcher."));
    }
    let dir = data_dir(&app, "versions")?;
    // Only something listed as a version: an id comes from the page.
    if installed(&WPVER, &dir, &Values::new()).iter().any(|v| v.manifest.id == id) {
        std::fs::remove_dir_all(dir.join(&id)).map_err(io)?;
    }
    app.state::<Store>().update(&app, |s| {
        if s.active_version.as_ref() == Some(&id) {
            s.active_version = None;
        }
    })?;
    current(&app).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wp-versions-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("in")).unwrap();
        std::fs::create_dir_all(dir.join("versions")).unwrap();
        dir
    }

    fn package(path: &Path, version: &str, theme: &str) {
        let manifest = format!(
            r#"{{"format":1,"id":"a.b","name":"A","version":"{version}","games":["SUPP01"],{theme}
            "patches":[{{"type":"riivolution","xml":"m.xml","options":[{{"section":"S","option":"O","choice":1}}]}}]}}"#
        );
        let xml = r#"<wiidisc version="1"><options><section name="S"><option name="O"><choice name="On"><patch id="p"/></choice></option></section></options><patch id="p"/></wiidisc>"#;
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, contents) in [("version.json", manifest.as_str()), ("m.xml", xml), ("theme/hero.svg", "<svg/>"), ("theme/page.html", "<p/>")] {
            zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    const GOOD: &str = r##""theme":{"colors":{"accent":"#1f78c4","shadow":"29, 92, 150"},"hero":[{"image":"theme/hero.svg","x":0,"y":0,"size":9,"motion":"float"}],"home":[{"image":"theme/hero.svg","x":1,"y":2,"size":3}]},"##;

    #[test]
    fn inlines_the_theme_for_the_page() {
        let dir = scratch("inline");
        let path = dir.join("in/a.wpver");
        package(&path, "1.0", GOOD);
        install(&WPVER, &path, &dir.join("versions")).unwrap();
        let version = installed(&WPVER, &dir.join("versions"), &Values::new()).pop().unwrap();
        let theme = for_page(&version).unwrap();
        assert_eq!(theme.hero[0].image, "data:image/svg+xml;base64,PHN2Zy8+");
        assert!(theme.home[0].image.starts_with("data:image/svg+xml"));
        assert_eq!(theme.home[0].opacity, 1.0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_theme_is_only_data() {
        let dir = scratch("refuses");
        let at = dir.join("in/a.wpver");
        let cases = [
            ("", "thème"),
            (r#""theme":{"colors":{"accent":"red; background: url(http://x)"}},"#, "pas valide"),
            (r#""theme":{"colors":{"accent":"url(http://x)"}},"#, "pas valide"),
            (r#""theme":{"colors":{"shadow":"1, 2, 999"}},"#, "pas valide"),
            (r##""theme":{"colors":{"position":"#000000"}},"##, "inconnue"),
            (r#""theme":{"colors":{},"hero":[{"image":"theme/absent.svg","x":0,"y":0,"size":1}]},"#, "introuvable"),
            (r#""theme":{"colors":{},"hero":[{"image":"theme/page.html","x":0,"y":0,"size":1}]},"#, "ni un SVG"),
            (r#""theme":{"colors":{},"hero":[{"image":"../../secret.svg","x":0,"y":0,"size":1}]},"#, "hors du paquet"),
            (r#""theme":{"colors":{},"home":[{"image":"theme/hero.svg","x":0,"y":0,"size":1,"motion":"spin; color: red"}]},"#, "mouvement"),
            (r#""theme":{"colors":{},"library":[{"image":"http://x/a.svg","x":0,"y":0,"size":1}]},"#, "introuvable"),
            (r#""theme":{"colors":{},"modes":[{"mode":"mine","image":"theme/hero.svg"}]},"#, "sans nom"),
            (r#""theme":{"colors":{},"modes":[{"mode":"bingo","image":"theme/absent.png"}]},"#, "introuvable"),
            (r#""theme":{"colors":{},"modesOnly":true,"modes":[{"mode":"bingo","image":"theme/hero.svg"}]},"#, "sans nom"),
        ];
        for (theme, expected) in cases {
            package(&at, "1.0", theme);
            let why = install(&WPVER, &at, &dir.join("versions")).err().unwrap_or_else(|| panic!("{theme} accepté"));
            assert!(why.contains(expected), "{theme} : {why}");
        }
        assert_eq!(std::fs::read_dir(dir.join("versions")).unwrap().count(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_version_and_a_mod_are_not_interchangeable() {
        let dir = scratch("kinds");
        let (mod_file, version_file) = (dir.join("in/a.wpmod"), dir.join("in/a.wpver"));
        package(&mod_file, "1.0", GOOD);
        package(&version_file, "1.0", GOOD);
        assert!(install(&WPVER, &mod_file, &dir.join("versions")).err().unwrap().contains(".wpver"));
        assert!(install(&crate::mods::WPMOD, &version_file, &dir.join("versions")).err().unwrap().contains(".wpmod"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_shipped_version_only_ever_moves_forward() {
        let dir = scratch("shipped");
        let versions = dir.join("versions");
        let at = dir.join("in/a.wpver");
        let there = |dir: &Path| installed(&WPVER, dir, &Values::new()).pop().unwrap().manifest.version;

        package(&at, "2026.11.4", GOOD);
        assert_eq!(install_shipped(&at, &versions).unwrap(), ("a.b".to_string(), true));
        assert_eq!(there(&versions), "2026.11.4");

        // Same one again: left alone, not unpacked at every start.
        let mark = versions.join("a.b/mark");
        std::fs::write(&mark, "").unwrap();
        assert_eq!(install_shipped(&at, &versions).unwrap(), ("a.b".to_string(), false));
        assert!(mark.exists());

        // The player's newer copy stays.
        package(&at, "2026.11.3", GOOD);
        assert!(!install_shipped(&at, &versions).unwrap().1);
        assert_eq!(there(&versions), "2026.11.4");

        package(&at, "2026.12.1", GOOD);
        assert!(install_shipped(&at, &versions).unwrap().1);
        assert_eq!(there(&versions), "2026.12.1");
        assert!(!mark.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
