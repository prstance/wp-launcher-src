//! Installed mods: .wpmod packages (a zip with a mod.json at its root), what each
//! one changes in the game, and whether the enabled ones can run together.
//! Versions (versions.rs) are the same packages under another name.

use crate::game::{data_dir, is_playing};
use crate::i18n::{tr, Text};
use crate::settings::{Disc, Settings, Store};
use crate::versions::{self, Theme};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

const FORMAT: u32 = 1;
/// Unpacked, whatever the package claims to weigh.
const MAX_UNPACKED: u64 = 8 << 30;
pub const MAX_IMAGE: u64 = 2 << 20;
const MAX_ICON: u64 = 512 << 10;
/// Where the memory settings go in the Riivolution XML the launcher writes.
const SETTINGS_SECTION: &str = "PartyLauncher";
const SETTINGS_OPTION: &str = "Settings";

/// Saved values of the packages' settings: package id -> setting id -> value.
pub type Values = BTreeMap<String, BTreeMap<String, i64>>;

/// A kind of package: its file, its manifest, and what it must contain on top of its patches.
pub struct Kind {
    pub manifest: &'static str,
    pub extension: &'static str,
    pub check: fn(&Path, &Manifest) -> Result<(), String>,
}

pub const WPMOD: Kind = Kind { manifest: "mod.json", extension: "wpmod", check: |_, _| Ok(()) };

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    format: u32,
    pub id: String,
    pub name: Text,
    pub version: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub description: Text,
    pub games: Vec<String>,
    /// The launcher versions it needs, ">=0.2": an older one asks to be updated.
    #[serde(default)]
    launcher: Option<String>,
    /// SVG or PNG inside the package.
    #[serde(default)]
    icon: Option<String>,
    /// What the player can choose, on the package's page.
    #[serde(default)]
    settings: Vec<Setting>,
    /// Mod id -> versions: "*" or ">=1.2".
    #[serde(default)]
    requires: BTreeMap<String, String>,
    #[serde(default)]
    conflicts: BTreeMap<String, String>,
    patches: Vec<Patch>,
    /// Versions only.
    #[serde(default)]
    pub theme: Option<Theme>,
}

/// "riivolution": an XML of the package and the options it turns on. "delta": the
/// package's files to make from the player's disc before a launch (rebuild.rs).
#[derive(Deserialize, Clone)]
pub struct Patch {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    xml: String,
    #[serde(default)]
    options: Vec<Choice>,
    #[serde(default)]
    pub files: Vec<DeltaFile>,
}

/// A file of the package (`file`, which its Riivolution XML names) made from the
/// disc's `disc` ("/rel/mr001.rel.lz", or "main.dol") by `delta` (delta.rs).
#[derive(Deserialize, Clone)]
pub struct DeltaFile {
    pub disc: String,
    pub file: String,
    pub delta: String,
}

impl Manifest {
    /// The files the package makes from the disc.
    pub fn deltas(&self) -> impl Iterator<Item = &DeltaFile> {
        self.patches.iter().filter(|p| p.kind == "delta").flat_map(|p| &p.files)
    }
}

/// One Riivolution option to turn on. `choice` counts from 1, as in Dolphin.
#[derive(Deserialize, Clone)]
struct Choice {
    section: String,
    option: String,
    choice: usize,
}

/// One thing the player can choose. Its value (a number: 0/1 for a toggle) either
/// picks a choice of a Riivolution option of the package, or is written in the
/// game's memory for the package's own code to read. A preset is a choice whose
/// choices set other settings (`sets`): its value is the first choice that
/// matches them, or the one without `sets` (custom).
#[derive(Deserialize, Clone)]
struct Setting {
    id: String,
    /// "toggle", "choice" or "number".
    #[serde(rename = "type")]
    kind: String,
    label: Text,
    #[serde(default)]
    description: Option<Text>,
    #[serde(default)]
    default: i64,
    /// "choice": what the list offers.
    #[serde(default)]
    choices: Vec<SettingChoice>,
    /// "number": its bounds and steps.
    #[serde(default)]
    min: i64,
    #[serde(default)]
    max: i64,
    #[serde(default = "one")]
    step: i64,
    #[serde(default)]
    riivolution: Option<Binding>,
    #[serde(default)]
    memory: Option<Slot>,
}

#[derive(Deserialize, Serialize, Clone)]
struct SettingChoice {
    value: i64,
    label: Text,
    /// A preset's: the values it gives other settings.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    sets: BTreeMap<String, i64>,
}

/// A Riivolution option of one of the package's XML. A toggle turns on its choice
/// `on`; a choice's value is the number of the choice, 0 turning the option off.
#[derive(Deserialize, Clone)]
struct Binding {
    xml: String,
    section: String,
    option: String,
    #[serde(default = "one")]
    on: i64,
}

/// `size` bytes at `offset` (hex), big-endian like the Wii.
#[derive(Deserialize, Clone)]
struct Slot {
    offset: String,
    #[serde(default = "four")]
    size: u32,
}

fn one() -> i64 {
    1
}

fn four() -> u32 {
    4
}

impl Slot {
    fn address(&self) -> Option<u32> {
        u32::from_str_radix(self.offset.trim_start_matches("0x"), 16).ok()
    }

    fn holds(&self, value: i64) -> bool {
        let bits = self.size * 8;
        (-(1i64 << (bits - 1))..1i64 << bits).contains(&value)
    }

    fn bytes(&self, value: i64) -> Vec<u8> {
        value.to_be_bytes()[(8 - self.size as usize)..].to_vec()
    }
}

impl Setting {
    fn is_preset(&self) -> bool {
        self.kind == "choice" && self.choices.iter().any(|c| !c.sets.is_empty())
    }

    /// A preset's value with the other settings at `values`.
    fn preset_value(&self, values: &BTreeMap<String, i64>) -> i64 {
        let matches = |c: &&SettingChoice| !c.sets.is_empty() && c.sets.iter().all(|(id, v)| values.get(id) == Some(v));
        self.choices.iter().find(matches).or_else(|| self.choices.iter().find(|c| c.sets.is_empty())).map_or(self.default, |c| c.value)
    }

    fn allows(&self, value: i64) -> bool {
        match self.kind.as_str() {
            "toggle" => value == 0 || value == 1,
            "choice" => self.choices.iter().any(|c| c.value == value),
            _ => (self.min..=self.max).contains(&value) && (value - self.min) % self.step == 0,
        }
    }

    /// The saved value, unless the package no longer allows it.
    fn value(&self, saved: Option<&BTreeMap<String, i64>>) -> i64 {
        saved.and_then(|s| s.get(&self.id)).copied().filter(|v| self.allows(*v)).unwrap_or(self.default)
    }

    /// Every value it can take.
    fn values(&self) -> Vec<i64> {
        match self.kind.as_str() {
            "toggle" => vec![0, 1],
            "choice" => self.choices.iter().map(|c| c.value).collect(),
            _ => vec![self.min, self.max],
        }
    }

    /// The Riivolution choice a value turns on; 0: none.
    fn choice(&self, value: i64) -> usize {
        let picked = if self.kind == "toggle" { if value != 0 { self.riivolution.as_ref().map_or(1, |b| b.on) } else { 0 } } else { value };
        picked.max(0) as usize
    }
}

fn check_setting(m: &Manifest, setting: &Setting) -> Result<(), String> {
    let fail = |why: String| Err(tr!("son réglage « {id} » : {why}", id = setting.id, why = why));
    let id_fits = !setting.id.is_empty() && setting.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c));
    if !id_fits {
        return fail(tr!("identifiant invalide"));
    }
    if !["toggle", "choice", "number"].contains(&setting.kind.as_str()) {
        return fail(tr!("type « {kind} » inconnu", kind = setting.kind));
    }
    if setting.label.is_blank() || setting.choices.iter().any(|c| c.label.is_blank()) {
        return fail(tr!("il manque un texte"));
    }
    if setting.kind == "choice" {
        let mut values: Vec<i64> = setting.choices.iter().map(|c| c.value).collect();
        values.sort();
        values.dedup();
        if values.is_empty() || values.len() != setting.choices.len() {
            return fail(tr!("ses choix sont vides ou en double"));
        }
    }
    if setting.kind == "number" && (setting.min > setting.max || setting.step < 1) {
        return fail(tr!("ses bornes ne vont pas"));
    }
    if !setting.allows(setting.default) {
        return fail(tr!("sa valeur par défaut n’est pas permise"));
    }
    match (&setting.riivolution, &setting.memory) {
        _ if setting.is_preset() => check_preset(m, setting).or_else(fail)?,
        (Some(binding), None) if setting.kind != "number" => {
            let Some(patch) = m.patches.iter().find(|p| p.xml == binding.xml) else {
                return fail(tr!("{xml} n’est pas un patch du paquet", xml = binding.xml));
            };
            if patch.options.iter().any(|o| o.section == binding.section && o.option == binding.option) {
                return fail(tr!("l’option « {option} » est déjà choisie par le paquet", option = binding.option));
            }
            if binding.on < 1 || setting.choices.iter().any(|c| c.value < 0) {
                return fail(tr!("un choix Riivolution compte à partir de 1"));
            }
        }
        (None, Some(slot)) => {
            if slot.address().is_none() || ![1, 2, 4].contains(&slot.size) {
                return fail(tr!("son emplacement en mémoire n’est pas valide"));
            }
            if !setting.values().iter().all(|v| slot.holds(*v)) {
                return fail(tr!("ses valeurs ne tiennent pas en {size} octet(s)", size = slot.size));
            }
        }
        _ => return fail(tr!("il lui faut soit une option Riivolution (sauf pour un nombre), soit un emplacement en mémoire")),
    }
    fn bound(s: &Setting) -> Option<(&String, &String, &String)> {
        s.riivolution.as_ref().map(|b| (&b.xml, &b.section, &b.option))
    }
    if bound(setting).is_some() && m.settings.iter().filter(|s| bound(s) == bound(setting)).count() > 1 {
        return fail(tr!("un autre réglage choisit déjà cette option"));
    }
    if m.settings.iter().filter(|s| s.id == setting.id).count() > 1 {
        return fail(tr!("identifiant en double"));
    }
    Ok(())
}

fn check_preset(m: &Manifest, preset: &Setting) -> Result<(), String> {
    if preset.riivolution.is_some() || preset.memory.is_some() {
        return Err(tr!("un préréglage ne choisit rien lui-même : ni option Riivolution, ni emplacement en mémoire"));
    }
    if preset.choices.iter().filter(|c| c.sets.is_empty()).count() != 1 {
        return Err(tr!("il faut à un préréglage un seul choix sans « sets » (personnalisé)"));
    }
    for (id, value) in preset.choices.iter().flat_map(|c| &c.sets) {
        if !m.settings.iter().any(|s| &s.id == id && !s.is_preset() && s.allows(*value)) {
            return Err(tr!("il règle « {id} » sur {value}, que ce réglage ne permet pas", id = id, value = value));
        }
    }
    let defaults = m.settings.iter().map(|s| (s.id.clone(), s.default)).collect();
    if preset.preset_value(&defaults) != preset.default {
        return Err(tr!("sa valeur par défaut n’est pas celle que donnent les autres réglages par défaut"));
    }
    Ok(())
}

/// What a mod changes in the game.
#[derive(Default)]
pub struct Footprint {
    /// Lowercase paths on the disc.
    files: Vec<String>,
    memory: Vec<(u32, Vec<u8>)>,
    /// It also does things that can't be told from its files.
    partial: bool,
    /// The package's file that replaces a disc file, by lowercase disc path.
    sources: BTreeMap<String, PathBuf>,
}

impl Footprint {
    /// The package's file that replaces `disc` ("/layout/inst/thumbnail.arc.lz"), if any.
    pub fn source(&self, disc: &str) -> Option<&Path> {
        self.sources.get(&disc_path(disc)).map(PathBuf::as_path)
    }
}

pub struct Installed {
    pub manifest: Manifest,
    pub dir: PathBuf,
    /// Its settings' values, the saved ones or their defaults.
    pub values: BTreeMap<String, i64>,
    pub footprint: Footprint,
}

impl Installed {
    pub fn icon(&self) -> Option<String> {
        data_url(&self.dir, self.manifest.icon.as_ref()?)
    }

    pub fn settings_count(&self) -> usize {
        self.manifest.settings.len()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModView {
    id: String,
    name: Text,
    version: String,
    authors: Vec<String>,
    description: Text,
    games: Vec<String>,
    size: u64,
    enabled: bool,
    icon: Option<String>,
    settings: usize,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Problem {
    /// "error": the game won't start. "warning": it will, but the player should know.
    level: &'static str,
    message: String,
    /// Ids of the packages it is about.
    packages: Vec<String>,
}

#[derive(Serialize)]
pub struct ModsState {
    mods: Vec<ModView>,
    /// Name of the active version, which the game also starts with.
    version: Option<Text>,
    problems: Vec<Problem>,
}

#[derive(Serialize)]
pub struct Imported {
    #[serde(flatten)]
    state: ModsState,
    added: Vec<String>,
    /// One message per file left out.
    refused: Vec<String>,
}

fn io(e: impl ToString) -> String {
    e.to_string()
}

/// Inside the package, whatever the package says.
pub fn is_inside(path: &str) -> bool {
    Path::new(path.trim_start_matches('/')).components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

fn numbers(version: &str) -> Vec<u64> {
    version.split('.').map(|n| n.trim().parse().unwrap_or(0)).collect()
}

fn range_is_valid(range: &str) -> bool {
    range == "*" || range.strip_prefix(">=").is_some_and(|v| v.split('.').all(|n| n.trim().parse::<u64>().is_ok()))
}

pub fn satisfies(version: &str, range: &str) -> bool {
    let Some(least) = range.strip_prefix(">=") else { return true };
    let (mut have, mut want) = (numbers(version), numbers(least));
    let len = have.len().max(want.len());
    have.resize(len, 0);
    want.resize(len, 0);
    have >= want
}

fn validate(m: &Manifest) -> Result<(), String> {
    // (first: a newer package may have a format or patches this launcher doesn't know)
    if let Some(range) = &m.launcher {
        if !range_is_valid(range) {
            return Err(tr!("la plage de versions « {range} » n’est pas valide", range = range));
        }
        if !satisfies(env!("CARGO_PKG_VERSION"), range) {
            return Err(tr!("il demande une version plus récente du launcher ({range}) : mets PartyLauncher à jour dans Paramètres › À propos", range = range));
        }
    }
    if m.format != FORMAT {
        return Err(tr!("ce paquet utilise le format {format}, que ce launcher ne connaît pas", format = m.format));
    }
    let id_fits = m.id.starts_with(|c: char| c.is_ascii_alphanumeric())
        && m.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c));
    if !id_fits {
        return Err(tr!("son identifiant « {id} » n’est pas valide", id = m.id));
    }
    if m.name.is_blank() || m.version.trim().is_empty() {
        return Err(tr!("il lui manque un nom ou une version"));
    }
    if m.games.is_empty() {
        return Err(tr!("il n’indique pas pour quelle version du jeu il est fait"));
    }
    if let Some(range) = m.requires.values().chain(m.conflicts.values()).find(|r| !range_is_valid(r)) {
        return Err(tr!("la plage de versions « {range} » n’est pas valide", range = range));
    }
    if m.patches.is_empty() {
        return Err(tr!("il ne contient aucun patch"));
    }
    if let Some(patch) = m.patches.iter().find(|p| p.kind != "riivolution" && p.kind != "delta") {
        return Err(tr!("ce launcher ne sait pas encore charger les patchs « {kind} »", kind = patch.kind));
    }
    if !m.patches.iter().any(|p| p.kind == "riivolution") || m.patches.iter().any(|p| p.kind == "delta" && p.files.is_empty()) {
        return Err(tr!("il ne contient aucun patch"));
    }
    for setting in &m.settings {
        check_setting(m, setting)?;
    }
    Ok(())
}

pub fn image_type(path: &str) -> Option<&'static str> {
    match Path::new(path).extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        _ => None,
    }
}

/// An SVG or PNG of the package, no heavier than `max`.
pub fn check_image(package: &Path, image: &str, max: u64) -> Result<(), String> {
    if !is_inside(image) {
        return Err(tr!("l’image « {image} » est hors du paquet", image = image));
    }
    if image_type(image).is_none() {
        return Err(tr!("l’image « {image} » n’est ni un SVG ni un PNG", image = image));
    }
    let weight = std::fs::metadata(package.join(image)).map_err(|_| tr!("l’image « {image} » est introuvable", image = image))?.len();
    if weight > max {
        return Err(tr!("l’image « {image} » est trop lourde", image = image));
    }
    Ok(())
}

pub fn data_url(package: &Path, image: &str) -> Option<String> {
    let bytes = std::fs::read(package.join(image)).ok()?;
    Some(format!("data:{};base64,{}", image_type(image)?, base64::engine::general_purpose::STANDARD.encode(bytes)))
}

fn hex(text: &str) -> Option<Vec<u8>> {
    let text = text.strip_prefix("0x").unwrap_or(text);
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok()).collect()
}

/// Where an `external` path leads: from the package with a leading slash, else from the patch's root.
fn external(package: &Path, patch_root: &str, path: &str) -> PathBuf {
    match path.strip_prefix('/') {
        Some(path) => package.join(path),
        None => package.join(patch_root.trim_start_matches('/')).join(path),
    }
}

fn files_below(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.path().is_dir() {
            files_below(&entry.path(), found);
        } else {
            found.push(entry.path());
        }
    }
}

/// Windows gives paths with backslashes; a disc only ever has slashes.
fn disc_path(path: &str) -> String {
    format!("/{}", path.replace('\\', "/").trim_matches('/').to_lowercase())
}

fn children<'a, 'input>(node: roxmltree::Node<'a, 'input>, tag: &str) -> Vec<roxmltree::Node<'a, 'input>> {
    node.children().filter(|n| n.has_tag_name(tag)).collect()
}

fn option_node<'a, 'input>(root: roxmltree::Node<'a, 'input>, section: &str, option: &str) -> Option<roxmltree::Node<'a, 'input>> {
    children(root, "options")
        .into_iter()
        .flat_map(|o| children(o, "section"))
        .filter(|s| s.attribute("name") == Some(section))
        .flat_map(|s| children(s, "option"))
        .find(|o| o.attribute("name") == Some(option))
}

/// Reads a Riivolution XML: only the patches of the options turned on count, by
/// the manifest or by the settings bound to this XML (with their `values`).
fn read_riivolution(package: &Path, patch: &Patch, settings: &[&Setting], values: &BTreeMap<String, i64>, footprint: &mut Footprint) -> Result<(), String> {
    if !is_inside(&patch.xml) {
        return Err(tr!("le chemin « {xml} » sort du paquet", xml = patch.xml));
    }
    let text = std::fs::read_to_string(package.join(&patch.xml)).map_err(|_| tr!("{xml} est introuvable ou illisible", xml = patch.xml))?;
    let document = roxmltree::Document::parse(&text).map_err(|e| tr!("{xml} est invalide : {e}", xml = patch.xml, e = e))?;
    let root = document.root_element();
    if !root.has_tag_name("wiidisc") {
        return Err(tr!("{xml} n’est pas un fichier Riivolution", xml = patch.xml));
    }

    let mut wanted = Vec::new();
    for choice in &patch.options {
        let picked = option_node(root, &choice.section, &choice.option)
            .and_then(|o| children(o, "choice").get(choice.choice.wrapping_sub(1)).copied())
            .ok_or_else(|| tr!("{xml} n’a pas l’option « {option} » choisie par le paquet", xml = patch.xml, option = choice.option))?;
        wanted.extend(children(picked, "patch").into_iter().filter_map(|p| p.attribute("id")));
    }
    for setting in settings {
        let Some(binding) = &setting.riivolution else { continue };
        let missing = || tr!("{xml} n’a pas l’option « {option} » du réglage « {id} »", xml = patch.xml, option = binding.option, id = setting.id);
        let choices = children(option_node(root, &binding.section, &binding.option).ok_or_else(missing)?, "choice");
        if setting.values().iter().any(|v| setting.choice(*v) > choices.len()) {
            return Err(missing());
        }
        let value = values.get(&setting.id).copied().unwrap_or(setting.default);
        if let Some(picked) = setting.choice(value).checked_sub(1).and_then(|i| choices.get(i)) {
            wanted.extend(children(*picked, "patch").into_iter().filter_map(|p| p.attribute("id")));
        }
    }
    if patch.options.is_empty() && settings.is_empty() {
        return Err(tr!("le paquet n’active aucune option de {xml}", xml = patch.xml));
    }

    for block in children(root, "patch").into_iter().filter(|p| p.attribute("id").is_some_and(|id| wanted.contains(&id))) {
        let patch_root = block.attribute("root").unwrap_or("");
        for node in block.children().filter(|n| n.is_element()) {
            let paths = ["disc", "external", "valuefile"].map(|a| node.attribute(a));
            if !is_inside(patch_root) || paths[1..].iter().flatten().any(|p| !is_inside(p)) {
                return Err(tr!("{xml} désigne un fichier hors du paquet", xml = patch.xml));
            }
            // Parameters are filled in by the options.
            if paths.iter().flatten().any(|p| p.contains("{$")) {
                footprint.partial = true;
                continue;
            }
            match node.tag_name().name() {
                "file" => match node.attribute("disc") {
                    Some(disc) if disc.starts_with('/') => {
                        if let Some(from) = node.attribute("external") {
                            footprint.sources.insert(disc_path(disc), external(package, patch_root, from));
                        }
                        footprint.files.push(disc_path(disc))
                    }
                    // Dolphin's way of replacing the game's executable.
                    Some(disc) if disc.eq_ignore_ascii_case("main.dol") => footprint.files.push(disc_path(disc)),
                    _ => footprint.partial = true,
                },
                "folder" => match (node.attribute("disc"), node.attribute("external")) {
                    (Some(disc), Some(from)) if disc.starts_with('/') => {
                        let from = external(package, patch_root, from);
                        let mut found = Vec::new();
                        files_below(&from, &mut found);
                        for file in found {
                            let Ok(inside) = file.strip_prefix(&from) else { continue };
                            let path = disc_path(&format!("{disc}/{}", inside.to_string_lossy()));
                            footprint.files.push(path.clone());
                            footprint.sources.insert(path, file);
                        }
                    }
                    _ => footprint.partial = true,
                },
                "memory" => {
                    let offset = node.attribute("offset").and_then(|o| u32::from_str_radix(o.trim_start_matches("0x"), 16).ok());
                    let value = match (node.attribute("value"), node.attribute("valuefile")) {
                        (Some(value), _) => hex(value),
                        (None, Some(file)) => std::fs::read(external(package, patch_root, file)).ok(),
                        _ => None,
                    };
                    match (offset, value, node.attribute("search")) {
                        (Some(offset), Some(value), None | Some("false")) => footprint.memory.push((offset, value)),
                        _ => footprint.partial = true,
                    }
                }
                "savegame" => {}
                _ => footprint.partial = true,
            }
        }
    }
    Ok(())
}

fn read_footprint(package: &Path, manifest: &Manifest, values: &BTreeMap<String, i64>) -> Result<Footprint, String> {
    let mut footprint = Footprint::default();
    for patch in manifest.patches.iter().filter(|p| p.kind == "riivolution") {
        let bound: Vec<&Setting> = manifest.settings.iter().filter(|s| s.riivolution.as_ref().is_some_and(|b| b.xml == patch.xml)).collect();
        read_riivolution(package, patch, &bound, values, &mut footprint)?;
    }
    for setting in &manifest.settings {
        if let Some((slot, at)) = setting.memory.as_ref().and_then(|slot| Some((slot, slot.address()?))) {
            footprint.memory.push((at, slot.bytes(values.get(&setting.id).copied().unwrap_or(setting.default))));
        }
    }
    footprint.files.sort();
    footprint.files.dedup();
    Ok(footprint)
}

fn read_manifest(kind: &Kind, text: &str) -> Result<Manifest, String> {
    let manifest: Manifest = serde_json::from_str(text).map_err(|e| tr!("son {manifest} est invalide : {e}", manifest = kind.manifest, e = e))?;
    validate(&manifest)?;
    Ok(manifest)
}

/// The settings' values: the saved ones, or the defaults.
fn resolve(manifest: &Manifest, saved: &Values) -> BTreeMap<String, i64> {
    let mut values: BTreeMap<String, i64> = manifest.settings.iter().filter(|s| !s.is_preset()).map(|s| (s.id.clone(), s.value(saved.get(&manifest.id)))).collect();
    let presets: Vec<_> = manifest.settings.iter().filter(|s| s.is_preset()).map(|s| (s.id.clone(), s.preset_value(&values))).collect();
    values.extend(presets);
    values
}

/// Each delta is in the package, and makes one of its files from a file of the disc.
fn check_deltas(package: &Path, manifest: &Manifest) -> Result<(), String> {
    for d in manifest.deltas() {
        let disc_ok = d.disc.starts_with('/') || d.disc.eq_ignore_ascii_case("main.dol");
        if !disc_ok || !is_inside(&d.file) || !is_inside(&d.delta) || d.file.trim_matches('/').is_empty() {
            return Err(tr!("son delta « {delta} » n’est pas valide", delta = d.delta));
        }
        let mut head = [0u8; 69];
        let read = File::open(package.join(&d.delta)).and_then(|mut f| f.read_exact(&mut head));
        if read.is_err() || crate::delta::source_hash(&head).is_err() {
            return Err(tr!("son delta « {delta} » est introuvable ou abîmé", delta = d.delta));
        }
    }
    Ok(())
}

fn read_contents(kind: &Kind, package: &Path, manifest: &Manifest, values: &BTreeMap<String, i64>) -> Result<Footprint, String> {
    (kind.check)(package, manifest)?;
    check_deltas(package, manifest)?;
    if let Some(icon) = &manifest.icon {
        check_image(package, icon, MAX_ICON)?;
    }
    read_footprint(package, manifest, values)
}

fn open(kind: &Kind, path: &Path) -> Result<(zip::ZipArchive<BufReader<File>>, Manifest), String> {
    if !path.is_file() || path.extension().is_none_or(|e| !e.eq_ignore_ascii_case(kind.extension)) {
        return Err(tr!("seuls les fichiers .{extension} sont acceptés", extension = kind.extension));
    }
    let file = File::open(path).map_err(io)?;
    let mut zip = zip::ZipArchive::new(BufReader::new(file)).map_err(|_| tr!("ce fichier .{extension} est illisible", extension = kind.extension))?;
    let mut text = String::new();
    zip.by_name(kind.manifest)
        .map_err(|_| tr!("il manque le {manifest}", manifest = kind.manifest))?
        .read_to_string(&mut text)
        .map_err(|_| tr!("son {manifest} est illisible", manifest = kind.manifest))?;
    let manifest = read_manifest(kind, &text)?;
    Ok((zip, manifest))
}

/// What a package says it is, without unpacking it.
pub fn peek(kind: &Kind, path: &Path) -> Result<Manifest, String> {
    open(kind, path).map(|(_, manifest)| manifest)
}

/// Unpacks a package into the folder, replacing an older copy of the same one.
pub fn install(kind: &Kind, path: &Path, dir: &Path) -> Result<Manifest, String> {
    let (mut zip, manifest) = open(kind, path)?;
    let staging = dir.join(format!(".{}.part", manifest.id));
    let _ = std::fs::remove_dir_all(&staging);
    let unpacked = unpack(&mut zip, &staging).and_then(|()| read_contents(kind, &staging, &manifest, &resolve(&manifest, &Values::new())));
    if let Err(why) = unpacked {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(why);
    }
    let target = dir.join(&manifest.id);
    let _ = std::fs::remove_dir_all(&target);
    std::fs::rename(&staging, &target).map_err(io)?;
    Ok(manifest)
}

fn unpack(zip: &mut zip::ZipArchive<BufReader<File>>, into: &Path) -> Result<(), String> {
    let mut left = MAX_UNPACKED;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|_| tr!("ce fichier est abîmé"))?;
        let name = entry.enclosed_name().ok_or_else(|| tr!("ce fichier contient un chemin dangereux"))?;
        let out = into.join(name);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(io)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        let mut to = File::create(&out).map_err(io)?;
        // Reads one byte past the limit to notice it.
        let written = std::io::copy(&mut (&mut entry).take(left + 1), &mut to).map_err(|_| tr!("ce fichier est abîmé"))?;
        if written > left {
            return Err(tr!("ce paquet est trop gros une fois décompressé"));
        }
        left -= written;
    }
    Ok(())
}

/// Every package of the folder that reads properly, with the saved values of its
/// settings; anything else is ignored.
pub fn installed(kind: &Kind, dir: &Path, saved: &Values) -> Vec<Installed> {
    let mut mods: Vec<Installed> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| {
            let dir = e.path();
            let manifest = read_manifest(kind, &std::fs::read_to_string(dir.join(kind.manifest)).ok()?).ok()?;
            let values = resolve(&manifest, saved);
            let footprint = read_contents(kind, &dir, &manifest, &values).ok()?;
            Some(Installed { manifest, dir, values, footprint })
        })
        .collect();
    mods.sort_by_key(|m| m.manifest.name.to_string().to_lowercase());
    mods
}

/// What the game starts with: the version first, so that mods apply on top of it.
fn load_order<'a>(version: Option<&'a Installed>, mods: &'a [Installed], enabled: &[String]) -> Vec<&'a Installed> {
    let enabled = enabled.iter().filter_map(|id| mods.iter().find(|m| &m.manifest.id == id));
    version.into_iter().chain(enabled).collect()
}

pub fn size_of(dir: &Path) -> u64 {
    let mut files = Vec::new();
    files_below(dir, &mut files);
    files.iter().filter_map(|f| f.metadata().ok()).map(|m| m.len()).sum()
}

/// Two mods writing different bytes to the same place: its address.
fn memory_clash(a: &Footprint, b: &Footprint) -> Option<u32> {
    for (at_a, bytes_a) in &a.memory {
        for (at_b, bytes_b) in &b.memory {
            let (start, end) = ((*at_a).max(*at_b), (*at_a as u64 + bytes_a.len() as u64).min(*at_b as u64 + bytes_b.len() as u64));
            if (start as u64..end).any(|addr| bytes_a[(addr - *at_a as u64) as usize] != bytes_b[(addr - *at_b as u64) as usize]) {
                return Some(start);
            }
        }
    }
    None
}

/// `mods` in load order: for the same file, the last one wins.
fn check(mods: &[&Installed], game: Option<&str>) -> Vec<Problem> {
    let mut problems = Vec::new();
    let mut error = |message, packages: &[&String]| problems.push(Problem { level: "error", message, packages: packages.iter().map(|p| p.to_string()).collect() });
    for m in mods {
        let (m, name) = (&m.manifest, &m.manifest.name);
        if let Some(game) = game.filter(|g| !m.games.iter().any(|ok| ok == g)) {
            error(tr!("« {name} » ne fonctionne pas sur ton ISO ({game}). Il demande : {games}.", name = name, game = game, games = m.games.join(", ")), &[&m.id]);
        }
        for (id, range) in &m.requires {
            match mods.iter().find(|o| &o.manifest.id == id) {
                None => error(tr!("« {name} » a besoin du mod « {id} », qui n’est pas activé.", name = name, id = id), &[&m.id]),
                Some(o) if !satisfies(&o.manifest.version, range) => {
                    error(tr!("« {name} » demande « {other} » en version {range} (installée : {installed}).", name = name, other = o.manifest.name, range = range, installed = o.manifest.version), &[&m.id, &o.manifest.id])
                }
                _ => {}
            }
        }
    }
    for (i, a) in mods.iter().enumerate() {
        for b in &mods[i + 1..] {
            let (ma, mb) = (&a.manifest, &b.manifest);
            let declared = [(ma, mb), (mb, ma)].into_iter().find(|(x, y)| x.conflicts.get(&y.id).is_some_and(|r| satisfies(&y.version, r)));
            if let Some((x, y)) = declared {
                error(tr!("« {a} » indique être incompatible avec « {b} ».", a = x.name, b = y.name), &[&x.id, &y.id]);
            }
            if let Some(at) = memory_clash(&a.footprint, &b.footprint) {
                error(tr!("« {a} » et « {b} » modifient le même code du jeu ({at}).", a = ma.name, b = mb.name, at = format!("{at:08X}")), &[&ma.id, &mb.id]);
            }
        }
    }
    for (i, a) in mods.iter().enumerate() {
        for b in &mods[i + 1..] {
            let shared: Vec<&String> = a.footprint.files.iter().filter(|f| b.footprint.files.binary_search(f).is_ok()).collect();
            if let Some(first) = shared.first() {
                let more = if shared.len() > 1 { tr!(" et {n} autre(s)", n = shared.len() - 1) } else { String::new() };
                problems.push(Problem {
                    level: "warning",
                    message: tr!(
                        "« {a} » et « {b} » remplacent le même fichier ({file}) : « {b} » passe en priorité, l’autre mod peut mal fonctionner.",
                        a = a.manifest.name,
                        b = b.manifest.name,
                        file = format!("{first}{more}"),
                    ),
                    packages: vec![a.manifest.id.clone(), b.manifest.id.clone()],
                });
            }
        }
    }
    if mods.len() > 1 {
        for m in mods.iter().filter(|m| m.footprint.partial) {
            problems.push(Problem {
                level: "warning",
                message: tr!("Tout ce que modifie « {name} » n’a pas pu être analysé : compatibilité inconnue.", name = m.manifest.name),
                packages: vec![m.manifest.id.clone()],
            });
        }
    }
    problems
}

fn state(dir: &Path, version: Option<&Installed>, settings: &Settings) -> ModsState {
    let mods = installed(&WPMOD, dir, &settings.package_settings);
    let order = load_order(version, &mods, &settings.enabled_mods);
    let problems = check(&order, settings.iso.as_ref().map(|d| d.game_id.as_str()));
    let views = mods
        .iter()
        .map(|m| ModView {
            id: m.manifest.id.clone(),
            name: m.manifest.name.clone(),
            version: m.manifest.version.clone(),
            authors: m.manifest.authors.clone(),
            description: m.manifest.description.clone(),
            games: m.manifest.games.clone(),
            size: size_of(&m.dir),
            enabled: settings.enabled_mods.contains(&m.manifest.id),
            icon: m.icon(),
            settings: m.settings_count(),
        })
        .collect();
    ModsState { mods: views, version: version.map(|v| v.manifest.name.clone()), problems }
}

/// The packages the game starts with, in their order (rebuild.rs).
pub fn launch_packages(app: &AppHandle, settings: &Settings) -> Result<Vec<Installed>, String> {
    let mut mods = installed(&WPMOD, &data_dir(app, "mods")?, &settings.package_settings);
    let mut order: Vec<Installed> = versions::active(app, settings)?.into_iter().collect();
    for id in &settings.enabled_mods {
        if let Some(at) = mods.iter().position(|m| &m.manifest.id == id) {
            order.push(mods.remove(at));
        }
    }
    Ok(order)
}

/// What Dolphin boots: its description of the disc with the active version and
/// the enabled mods, if any. An error when they can't run together.
pub fn boot_file(app: &AppHandle, disc: &Disc, settings: &Settings) -> Result<Option<PathBuf>, String> {
    let dir = data_dir(app, "mods")?;
    let mods = installed(&WPMOD, &dir, &settings.package_settings);
    let version = versions::active(app, settings)?;
    let order = load_order(version.as_ref(), &mods, &settings.enabled_mods);
    if order.is_empty() {
        return Ok(None);
    }
    for m in &order {
        if let Some(xml) = settings_xml(m) {
            std::fs::write(settings_file(&dir, m), xml).map_err(io)?;
        }
    }
    let errors: Vec<String> = check(&order, Some(&disc.game_id)).into_iter().filter(|p| p.level == "error").map(|p| p.message).collect();
    if !errors.is_empty() {
        return Err(errors.join(" "));
    }
    let path = dir.join(".launch.json");
    std::fs::write(&path, descriptor(disc, &order, &dir).to_string()).map_err(io)?;
    Ok(Some(path))
}

fn settings_file(dir: &Path, m: &Installed) -> PathBuf {
    dir.join(format!(".settings-{}.xml", m.manifest.id))
}

/// The Riivolution XML that writes the package's memory settings, if it has any.
fn settings_xml(m: &Installed) -> Option<String> {
    let writes: Vec<String> = m
        .manifest
        .settings
        .iter()
        .filter_map(|s| {
            let slot = s.memory.as_ref()?;
            let bytes = slot.bytes(*m.values.get(&s.id)?);
            Some(format!(r#"<memory offset="0x{:08X}" value="{}"/>"#, slot.address()?, bytes.iter().map(|b| format!("{b:02X}")).collect::<String>()))
        })
        .collect();
    if writes.is_empty() {
        return None;
    }
    Some(format!(
        r#"<wiidisc version="1"><options><section name="{SETTINGS_SECTION}"><option name="{SETTINGS_OPTION}"><choice name="On"><patch id="settings"/></choice></option></section></options><patch id="settings">{}</patch></wiidisc>"#,
        writes.join("")
    ))
}

/// Dolphin's game mod descriptor (DiscIO/GameModDescriptor.cpp in the fork).
/// `dir`: where boot_file wrote the settings XML.
fn descriptor(disc: &Disc, mods: &[&Installed], dir: &Path) -> serde_json::Value {
    let option = |section: &str, option: &str, choice: usize| json!({ "section-name": section, "option-name": option, "choice": choice });
    let patches: Vec<_> = mods
        .iter()
        .flat_map(|m| {
            let own = m.manifest.patches.iter().map(move |p| {
                let fixed = p.options.iter().map(|o| option(&o.section, &o.option, o.choice));
                let chosen = m.manifest.settings.iter().filter_map(|s| {
                    let binding = s.riivolution.as_ref().filter(|b| b.xml == p.xml)?;
                    let choice = s.choice(*m.values.get(&s.id)?);
                    (choice > 0).then(|| option(&binding.section, &binding.option, choice))
                });
                json!({ "xml": m.dir.join(&p.xml), "root": m.dir, "options": fixed.chain(chosen).collect::<Vec<_>>() })
            });
            let memory = settings_xml(m)
                .map(|_| json!({ "xml": settings_file(dir, m), "root": m.dir, "options": [option(SETTINGS_SECTION, SETTINGS_OPTION, 1)] }));
            own.chain(memory)
        })
        .collect();
    json!({
        "type": "dolphin-game-mod-descriptor",
        "version": 1,
        "base-file": disc.path,
        "display-name": "Wii Party",
        "riivolution": { "patches": patches },
    })
}

fn current(app: &AppHandle) -> Result<ModsState, String> {
    let settings = app.state::<Store>().get(app);
    let version = versions::active(app, &settings)?;
    Ok(state(&data_dir(app, "mods")?, version.as_ref(), &settings))
}

fn refuse_while_playing(app: &AppHandle) -> Result<(), String> {
    if is_playing(app) {
        return Err(tr!("Arrête la partie avant de changer les mods installés."));
    }
    Ok(())
}

#[tauri::command]
pub fn mods_list(app: AppHandle) -> Result<ModsState, String> {
    current(&app)
}

/// Installs files; a new mod starts enabled, last in the load order.
#[tauri::command]
pub async fn mods_import(app: AppHandle, paths: Vec<PathBuf>) -> Result<Imported, String> {
    refuse_while_playing(&app)?;
    let dir = data_dir(&app, "mods")?;
    let taken = versions::installed_ids(&app)?;
    let results = tauri::async_runtime::spawn_blocking(move || {
        let add = |path: &Path| {
            let manifest = peek(&WPMOD, path)?;
            if taken.contains(&manifest.id) {
                return Err(tr!("« {name} » est une version : elle se choisit dans l’écran Versions", name = manifest.name));
            }
            install(&WPMOD, path, &dir)
        };
        paths.iter().map(|p| (p.clone(), add(p))).collect::<Vec<_>>()
    })
    .await
    .map_err(io)?;
    let (mut added, mut refused) = (Vec::new(), Vec::new());
    for (path, result) in results {
        match result {
            Ok(manifest) => {
                app.state::<Store>().update(&app, |s| {
                    if !s.enabled_mods.contains(&manifest.id) {
                        s.enabled_mods.push(manifest.id.clone());
                    }
                })?;
                added.push(manifest.name.to_string());
            }
            Err(why) => refused.push(tr!("{file} : {why}.", file = path.file_name().unwrap_or_default().to_string_lossy(), why = why)),
        }
    }
    Ok(Imported { state: current(&app)?, added, refused })
}

/// Opens the file picker, for those who don't drag.
#[tauri::command]
pub async fn mods_pick(app: AppHandle) -> Result<Option<Imported>, String> {
    let dialog = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog.dialog().file().set_title(tr!("Choisis tes mods")).add_filter(tr!("Mods Wii Party"), &[WPMOD.extension]).blocking_pick_files()
    })
    .await
    .map_err(io)?;
    let Some(files) = picked else { return Ok(None) };
    let paths = files.into_iter().filter_map(|f| f.into_path().ok()).collect();
    mods_import(app, paths).await.map(Some)
}

#[tauri::command]
pub fn mods_set_enabled(app: AppHandle, id: String, on: bool) -> Result<ModsState, String> {
    app.state::<Store>().update(&app, |s| {
        s.enabled_mods.retain(|m| m != &id);
        if on {
            s.enabled_mods.push(id.clone());
        }
    })?;
    current(&app)
}

#[tauri::command]
pub fn mods_remove(app: AppHandle, id: String) -> Result<ModsState, String> {
    refuse_while_playing(&app)?;
    let dir = data_dir(&app, "mods")?;
    // Only something listed as a mod: an id comes from the page.
    if installed(&WPMOD, &dir, &Values::new()).iter().any(|m| m.manifest.id == id) {
        std::fs::remove_dir_all(dir.join(&id)).map_err(io)?;
    }
    app.state::<Store>().update(&app, |s| s.enabled_mods.retain(|m| m != &id))?;
    current(&app)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingView {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    label: Text,
    description: Option<Text>,
    default: i64,
    value: i64,
    choices: Vec<SettingChoice>,
    min: i64,
    max: i64,
    step: i64,
}

#[derive(Serialize)]
pub struct Changes {
    /// Paths on the disc.
    files: Vec<String>,
    /// Places in the game's code.
    memory: usize,
    partial: bool,
}

/// Everything about one mod or version, for its page.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    /// "mod" or "version".
    kind: &'static str,
    id: String,
    name: Text,
    version: String,
    authors: Vec<String>,
    description: Text,
    games: Vec<String>,
    size: u64,
    /// A mod enabled, or the active version.
    enabled: bool,
    shipped: bool,
    icon: Option<String>,
    settings: Vec<SettingView>,
    changes: Changes,
    requires: BTreeMap<String, String>,
    conflicts: BTreeMap<String, String>,
    /// Those of the game as it would start now that are about this package.
    problems: Vec<Problem>,
}

/// The installed mods and versions, with what the game would start with.
fn everything(app: &AppHandle, settings: &Settings) -> Result<(Vec<Installed>, Vec<Installed>, Vec<Problem>), String> {
    let mods = installed(&WPMOD, &data_dir(app, "mods")?, &settings.package_settings);
    let versions = versions::all(app, &settings.package_settings)?;
    let version = versions.iter().find(|v| settings.active_version.as_ref() == Some(&v.manifest.id));
    let problems = check(&load_order(version, &mods, &settings.enabled_mods), settings.iso.as_ref().map(|d| d.game_id.as_str()));
    Ok((mods, versions, problems))
}

#[tauri::command]
pub async fn package_detail(app: AppHandle, id: String) -> Result<Detail, String> {
    let shipped = versions::shipped(&app).await;
    let settings = app.state::<Store>().get(&app);
    let (mods, versions, problems) = everything(&app, &settings)?;
    let (kind, package) = match mods.iter().find(|m| m.manifest.id == id) {
        Some(m) => ("mod", m),
        None => ("version", versions.iter().find(|v| v.manifest.id == id).ok_or_else(|| tr!("Ce mod n’est plus installé."))?),
    };
    let m = &package.manifest;
    Ok(Detail {
        kind,
        id: m.id.clone(),
        name: m.name.clone(),
        version: m.version.clone(),
        authors: m.authors.clone(),
        description: m.description.clone(),
        games: m.games.clone(),
        size: size_of(&package.dir),
        enabled: if kind == "mod" { settings.enabled_mods.contains(&id) } else { settings.active_version.as_ref() == Some(&id) },
        shipped: shipped.contains(&id),
        icon: package.icon(),
        settings: m
            .settings
            .iter()
            .map(|s| SettingView {
                id: s.id.clone(),
                kind: s.kind.clone(),
                label: s.label.clone(),
                description: s.description.clone(),
                default: s.default,
                value: package.values.get(&s.id).copied().unwrap_or(s.default),
                choices: s.choices.clone(),
                min: s.min,
                max: s.max,
                step: s.step,
            })
            .collect(),
        changes: Changes { files: package.footprint.files.clone(), memory: package.footprint.memory.len(), partial: package.footprint.partial },
        requires: m.requires.clone(),
        conflicts: m.conflicts.clone(),
        problems: problems.into_iter().filter(|p| p.packages.contains(&id)).collect(),
    })
}

/// Saves the values of a package's settings; none: back to the defaults. They
/// apply from the next launch.
#[tauri::command]
pub async fn package_settings_set(app: AppHandle, id: String, values: BTreeMap<String, i64>) -> Result<Detail, String> {
    let settings = app.state::<Store>().get(&app);
    let (mods, versions, _) = everything(&app, &settings)?;
    let package = mods.iter().chain(&versions).find(|m| m.manifest.id == id).ok_or_else(|| tr!("Ce mod n’est plus installé."))?;
    for (key, value) in &values {
        if !package.manifest.settings.iter().any(|s| &s.id == key && s.allows(*value)) {
            return Err(tr!("La valeur {value} n’est pas permise pour « {setting} ».", value = value, setting = key));
        }
    }
    // (a preset's value comes from the others)
    let values: BTreeMap<String, i64> = values.into_iter().filter(|(key, _)| package.manifest.settings.iter().any(|s| &s.id == key && !s.is_preset())).collect();
    app.state::<Store>().update(&app, |s| {
        if values.is_empty() {
            s.package_settings.remove(&id);
        } else {
            s.package_settings.insert(id.clone(), values);
        }
    })?;
    package_detail(app, id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wp-mods-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("in")).unwrap();
        std::fs::create_dir_all(dir.join("mods")).unwrap();
        dir
    }

    fn manifest(id: &str, extra: &str) -> String {
        format!(
            r#"{{"format":1,"id":"{id}","name":"{id}","version":"1.2.0","games":["SUPP01"],{extra}
            "patches":[{{"type":"riivolution","xml":"riivolution/m.xml","options":[{{"section":"S","option":"O","choice":1}}]}}]}}"#
        )
    }

    fn xml(body: &str) -> String {
        format!(
            r#"<wiidisc version="1"><id game="SUPP"/><options><section name="S"><option name="O"><choice name="On"><patch id="p"/></choice></option></section></options>
            <patch id="p" root="/data">{body}</patch><patch id="other"><file disc="/never.bin" external="x"/></patch></wiidisc>"#
        )
    }

    fn package(path: &Path, entries: &[(&str, &str)]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, contents) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    /// Installs a mod and reads it back.
    fn mod_with(dir: &Path, id: &str, extra: &str, body: &str) -> Installed {
        let path = dir.join("in").join(format!("{id}.wpmod"));
        package(&path, &[("mod.json", &manifest(id, extra)), ("riivolution/m.xml", &xml(body)), ("data/a.bin", "x")]);
        install(&WPMOD, &path, &dir.join("mods")).unwrap();
        installed(&WPMOD, &dir.join("mods"), &Values::new()).into_iter().find(|m| m.manifest.id == id).unwrap()
    }

    fn levels(problems: &[Problem]) -> Vec<&'static str> {
        problems.iter().map(|p| p.level).collect()
    }

    #[test]
    fn installs_and_replaces_a_mod() {
        let dir = scratch("installs");
        let first = mod_with(&dir, "a.b", "", r#"<file disc="/Rel/X.bin" external="a.bin"/>"#);
        assert_eq!(first.footprint.files, ["/rel/x.bin"]);
        assert!(!first.footprint.partial);
        assert!(first.dir.join("data/a.bin").is_file());
        let again = mod_with(&dir, "a.b", "", r#"<file disc="/y.bin" external="a.bin"/>"#);
        assert_eq!(again.footprint.files, ["/y.bin"]);
        assert_eq!(installed(&WPMOD, &dir.join("mods"), &Values::new()).len(), 1);
        assert!(!dir.join("mods/.a.b.part").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refuses_what_is_not_a_good_wpmod() {
        let dir = scratch("refuses");
        let mods = dir.join("mods");
        let at = |name: &str| dir.join("in").join(name);
        let good = xml("");
        let cases: Vec<(&str, Vec<(&str, String)>, &str)> = vec![
            ("patch.zip", vec![("mod.json", manifest("a", ""))], "seuls"),
            ("empty.wpmod", vec![("riivolution/m.xml", good.clone())], "mod.json"),
            ("bad-id.wpmod", vec![("mod.json", manifest("../evil", "")), ("riivolution/m.xml", good.clone())], "identifiant"),
            ("no-xml.wpmod", vec![("mod.json", manifest("a", ""))], "m.xml"),
            ("format.wpmod", vec![("mod.json", manifest("a", "").replace("\"format\":1", "\"format\":2"))], "format 2"),
            ("gecko.wpmod", vec![("mod.json", manifest("a", "").replace("riivolution\",\"xml", "gecko\",\"xml"))], "gecko"),
            ("option.wpmod", vec![("mod.json", manifest("a", "").replace("\"option\":\"O\"", "\"option\":\"?\"")), ("riivolution/m.xml", good.clone())], "option"),
            ("escape.wpmod", vec![("mod.json", manifest("a", "")), ("riivolution/m.xml", xml(r#"<file disc="/x" external="../../secret"/>"#))], "hors du paquet"),
            ("range.wpmod", vec![("mod.json", manifest("a", r#""requires":{"b":"~1"},"#))], "plage"),
        ];
        for (name, entries, expected) in cases {
            let entries: Vec<(&str, &str)> = entries.iter().map(|(n, c)| (*n, c.as_str())).collect();
            package(&at(name), &entries);
            let why = install(&WPMOD, &at(name), &mods).err().unwrap_or_else(|| panic!("{name} accepté"));
            assert!(why.contains(expected), "{name} : {why}");
        }
        std::fs::write(at("noise.wpmod"), "PK nope").unwrap();
        assert!(install(&WPMOD, &at("noise.wpmod"), &mods).is_err());
        assert_eq!(std::fs::read_dir(&mods).unwrap().count(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_zip_cannot_write_outside_the_mod() {
        let dir = scratch("slip");
        let path = dir.join("in/slip.wpmod");
        package(&path, &[("mod.json", &manifest("a", "")), ("riivolution/m.xml", &xml("")), ("../../evil.txt", "x")]);
        assert!(install(&WPMOD, &path, &dir.join("mods")).err().unwrap().contains("dangereux"));
        assert!(!dir.join("evil.txt").exists());
        assert!(!dir.join("mods/.a.part").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn checks_what_is_declared() {
        let dir = scratch("declared");
        let a = mod_with(&dir, "a", "", "");
        assert!(check(&[&a], Some("SUPP01")).is_empty());
        assert_eq!(levels(&check(&[&a], Some("SUPE01"))), ["error"]);
        assert!(check(&[&a], None).is_empty());

        let needs = mod_with(&dir, "needs", r#""requires":{"a":">=1.2"},"#, "");
        assert!(check(&[&needs, &a], None).is_empty());
        assert_eq!(levels(&check(&[&needs], None)), ["error"]);
        let too_new = mod_with(&dir, "new", r#""requires":{"a":">=1.10"},"#, "");
        assert_eq!(levels(&check(&[&too_new, &a], None)), ["error"]);

        let enemy = mod_with(&dir, "enemy", r#""conflicts":{"a":"*"},"#, "");
        assert_eq!(levels(&check(&[&a, &enemy], None)), ["error"]);
        assert_eq!(levels(&check(&[&enemy, &a], None)), ["error"]);
        let old_enemy = mod_with(&dir, "old", r#""conflicts":{"a":">=2"},"#, "");
        assert!(check(&[&a, &old_enemy], None).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn finds_what_mods_share() {
        let dir = scratch("shared");
        let file = r#"<file disc="/same.bin" external="a.bin"/>"#;
        let (a, b) = (mod_with(&dir, "a", "", file), mod_with(&dir, "b", "", r#"<file disc="/SAME.bin" external="a.bin"/>"#));
        let problems = check(&[&a, &b], None);
        assert_eq!(levels(&problems), ["warning"]);
        assert!(problems[0].message.contains("« b » passe en priorité"), "{}", problems[0].message);

        let dol = |id| mod_with(&dir, id, "", r#"<file disc="main.dol" external="a.bin"/>"#);
        let (d1, d2) = (dol("d1"), dol("d2"));
        assert!(!d1.footprint.partial);
        assert_eq!(levels(&check(&[&d1, &d2], None)), ["warning"]);

        let folder = mod_with(&dir, "folder", "", r#"<folder disc="/" external="."/>"#);
        assert!(folder.footprint.files.contains(&"/a.bin".to_string()));
        let same_file = mod_with(&dir, "named", "", r#"<file disc="/a.bin" external="a.bin"/>"#);
        assert_eq!(levels(&check(&[&folder, &same_file], None)), ["warning"]);

        let memory = |value: &str| format!(r#"<memory offset="0x80001000" value="{value}"/>"#);
        let (m1, m2) = (mod_with(&dir, "m1", "", &memory("11223344")), mod_with(&dir, "m2", "", &memory("11229944")));
        assert_eq!(levels(&check(&[&m1, &m2], None)), ["error"]);
        let m3 = mod_with(&dir, "m3", "", &memory("11223344"));
        assert!(check(&[&m1, &m3], None).is_empty());
        let m4 = mod_with(&dir, "m4", "", r#"<memory offset="0x80001004" value="99"/>"#);
        assert!(check(&[&m1, &m4], None).is_empty());

        let vague = mod_with(&dir, "vague", "", r#"<memory value="60000000" search="true"/>"#);
        assert!(vague.footprint.partial);
        assert_eq!(levels(&check(&[&a, &vague], None)), ["warning"]);
        assert!(check(&[&vague], None).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn describes_the_boot_to_dolphin() {
        let dir = scratch("boot");
        let a = mod_with(&dir, "a", "", "");
        let disc = Disc { path: "/jeux/wp.rvz".into(), name: "wp.rvz".into(), game_id: "SUPP01".into(), country: "Europe".into() };
        let d = descriptor(&disc, &[&a], &dir);
        assert_eq!(d["type"], "dolphin-game-mod-descriptor");
        assert_eq!(d["base-file"], "/jeux/wp.rvz");
        let patch = &d["riivolution"]["patches"][0];
        assert_eq!(patch["xml"], a.dir.join("riivolution/m.xml").to_string_lossy().as_ref());
        assert_eq!(patch["root"], a.dir.to_string_lossy().as_ref());
        assert_eq!(patch["options"][0], json!({ "section-name": "S", "option-name": "O", "choice": 1 }));
        let b = mod_with(&dir, "b", "", "");
        let mods = [a, b];
        let order = load_order(Some(&mods[1]), &mods, &["a".to_string()]);
        assert_eq!(order.iter().map(|m| m.manifest.id.as_str()).collect::<Vec<_>>(), ["b", "a"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A mod with an option "Neige" (choices "Peu" and "Beaucoup") next to "O", and these settings.
    fn settings_mod(dir: &Path, id: &str, settings: &str) -> PathBuf {
        let path = dir.join("in").join(format!("{id}.wpmod"));
        let xml = r#"<wiidisc version="1"><options><section name="S"><option name="O"><choice name="On"><patch id="p"/></choice></option>
            <option name="Neige"><choice name="Peu"><patch id="peu"/></choice><choice name="Beaucoup"><patch id="beaucoup"/></choice></option></section></options>
            <patch id="p"/><patch id="peu"><file disc="/peu.bin" external="a.bin"/></patch><patch id="beaucoup"><file disc="/beaucoup.bin" external="a.bin"/></patch></wiidisc>"#;
        package(&path, &[("mod.json", &manifest(id, &format!(r#""settings":{settings},"#))), ("riivolution/m.xml", xml), ("a.bin", "x")]);
        path
    }

    const SNOW: &str = r#"{"id":"neige","type":"choice","label":{"fr":"Neige","en":"Snow"},"default":1,"riivolution":{"xml":"riivolution/m.xml","section":"S","option":"Neige"},
        "choices":[{"value":0,"label":"Aucune"},{"value":1,"label":"Peu"},{"value":2,"label":{"fr":"Beaucoup"}}]}"#;
    const DIE: &str = r#"{"id":"de","type":"number","label":"Dé","min":1,"max":6,"default":3,"memory":{"offset":"0x80001800","size":1}}"#;

    #[test]
    fn settings_pick_patches_and_write_memory() {
        let dir = scratch("settings");
        let path = settings_mod(&dir, "s", &format!("[{SNOW},{DIE}]"));
        install(&WPMOD, &path, &dir.join("mods")).unwrap();
        let read = |saved: &str| {
            let saved: Values = serde_json::from_str(&format!(r#"{{"s":{saved}}}"#)).unwrap();
            installed(&WPMOD, &dir.join("mods"), &saved).pop().unwrap()
        };

        let defaults = read("{}");
        assert_eq!(defaults.footprint.files, ["/peu.bin"]);
        assert_eq!(defaults.footprint.memory, [(0x80001800, vec![3])]);
        let chosen = read(r#"{"neige":2,"de":5}"#);
        assert_eq!(chosen.footprint.files, ["/beaucoup.bin"]);
        assert_eq!(chosen.footprint.memory, [(0x80001800, vec![5])]);
        assert!(read(r#"{"neige":0}"#).footprint.files.is_empty());
        // A value the package doesn't allow (any more) gives way to the default.
        assert_eq!(read(r#"{"de":9}"#).values["de"], 3);

        let disc = Disc { path: "/jeux/wp.rvz".into(), name: "wp.rvz".into(), game_id: "SUPP01".into(), country: "Europe".into() };
        let d = descriptor(&disc, &[&chosen], &dir);
        let patches = &d["riivolution"]["patches"];
        assert_eq!(patches[0]["options"][1], json!({ "section-name": "S", "option-name": "Neige", "choice": 2 }));
        assert_eq!(patches[1]["xml"], dir.join(".settings-s.xml").to_string_lossy().as_ref());
        assert_eq!(patches[1]["options"][0], json!({ "section-name": SETTINGS_SECTION, "option-name": SETTINGS_OPTION, "choice": 1 }));
        assert!(settings_xml(&chosen).unwrap().contains(r#"<memory offset="0x80001800" value="05"/>"#));
        let off = descriptor(&disc, &[&read(r#"{"neige":0}"#)], &dir);
        assert_eq!(off["riivolution"]["patches"][0]["options"].as_array().unwrap().len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// "Difficulty": easy puts the die on 2 and the snow on 1, normal is the defaults.
    fn preset(choices: &str, default: i64) -> String {
        format!(r#"{{"id":"niveau","type":"choice","label":"Niveau","default":{default},"choices":[{choices}]}}"#)
    }
    const PRESETS: &str = r#"{"value":0,"label":"Douce","sets":{"de":2,"neige":1}},{"value":1,"label":"Normale","sets":{"de":3,"neige":1}},{"value":9,"label":"Perso"}"#;

    #[test]
    fn presets_follow_the_other_settings() {
        let dir = scratch("presets");
        let path = settings_mod(&dir, "p", &format!("[{},{SNOW},{DIE}]", preset(PRESETS, 1)));
        install(&WPMOD, &path, &dir.join("mods")).unwrap();
        let read = |saved: &str| {
            let saved: Values = serde_json::from_str(&format!(r#"{{"p":{saved}}}"#)).unwrap();
            installed(&WPMOD, &dir.join("mods"), &saved).pop().unwrap().values
        };
        assert_eq!(read("{}")["niveau"], 1);
        assert_eq!(read(r#"{"de":2}"#)["niveau"], 0);
        assert_eq!(read(r#"{"de":4}"#)["niveau"], 9);
        // (a saved value of the preset itself is not what decides it)
        assert_eq!(read(r#"{"niveau":0}"#)["niveau"], 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refuses_settings_that_cannot_work() {
        let dir = scratch("bad-settings");
        let toggle = |extra: &str| format!(r#"{{"id":"t","type":"toggle","label":"T",{extra}}}"#);
        let cases = [
            (format!("[{}]", toggle(r#""riivolution":{"xml":"riivolution/m.xml","section":"S","option":"Neige"},"type":"slider""#)), "type"),
            (format!("[{}]", DIE.replace(r#""default":3"#, r#""default":9"#)), "défaut"),
            (format!("[{}]", toggle(r#""memory":{"offset":"0x80001800"},"riivolution":{"xml":"riivolution/m.xml","section":"S","option":"Neige"}"#)), "soit"),
            (format!("[{}]", DIE.replace(r#""memory":{"offset":"0x80001800","size":1}"#, r#""riivolution":{"xml":"riivolution/m.xml","section":"S","option":"Neige"}"#)), "soit"),
            (format!("[{}]", toggle(r#""riivolution":{"xml":"riivolution/m.xml","section":"S","option":"Pluie"}"#)), "Pluie"),
            (format!("[{}]", toggle(r#""riivolution":{"xml":"riivolution/m.xml","section":"S","option":"Neige","on":3}"#)), "Neige"),
            (format!("[{}]", toggle(r#""riivolution":{"xml":"riivolution/autre.xml","section":"S","option":"Neige"}"#)), "autre.xml"),
            (format!("[{}]", toggle(r#""riivolution":{"xml":"riivolution/m.xml","section":"S","option":"O"}"#)), "déjà choisie"),
            (format!("[{}]", DIE.replace(r#""max":6"#, r#""max":300"#)), "octet"),
            (format!("[{DIE},{DIE}]"), "double"),
            (format!("[{}]", SNOW.replace(r#""label":{"fr":"Neige","en":"Snow"}"#, r#""label":{"fr":"Neige","en":""}"#)), "texte"),
            (format!("[{},{SNOW},{DIE}]", preset(&PRESETS.replace(r#","label":"Perso""#, r#","label":"Perso","sets":{"de":4}"#), 1)), "personnalisé"),
            (format!("[{},{SNOW},{DIE}]", preset(&PRESETS.replace(r#""de":2"#, r#""de":7"#), 1)), "« de » sur 7"),
            (format!("[{},{SNOW},{DIE}]", preset(&PRESETS.replace(r#""de":2"#, r#""rien":2"#), 1)), "« rien »"),
            (format!("[{},{SNOW},{DIE}]", preset(PRESETS, 0)), "par défaut n’est pas celle"),
            (format!("[{},{DIE}]", preset(PRESETS, 1).replace(r#""default":1"#, r#""default":1,"memory":{"offset":"0x80001804","size":1}"#)), "ne choisit rien"),
        ];
        for (settings, expected) in cases {
            let why = install(&WPMOD, &settings_mod(&dir, "bad", &settings), &dir.join("mods")).err().unwrap_or_else(|| panic!("accepté : {settings}"));
            assert!(why.contains(expected), "{expected} : {why}");
        }
        assert_eq!(Slot { offset: "0x0".into(), size: 2 }.bytes(-1), [0xFF, 0xFF]);
        assert!(!Slot { offset: "0x0".into(), size: 1 }.holds(256));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn disc_paths_read_the_same_on_every_system() {
        assert_eq!(disc_path("/Rel/X.bin"), "/rel/x.bin");
        assert_eq!(disc_path("/mr001/sub\\Y.bin"), "/mr001/sub/y.bin");
    }

    #[test]
    fn compares_versions() {
        assert!(satisfies("1.2.0", ">=1.2"));
        assert!(satisfies("1.10", ">=1.2"));
        assert!(!satisfies("1.1.9", ">=1.2"));
        assert!(satisfies("0.1", "*"));
    }
}
