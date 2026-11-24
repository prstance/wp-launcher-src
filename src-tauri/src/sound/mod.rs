//! The mods' "sound" patches, applied to the disc's sound archives before a launch (link/mod.rs), as
//! wp-sound's tools/wpsound/merge.py does (the same bytes; wp-sound docs/sound.md). Several mods'
//! patches add up: each adds its own groups and sounds, and edits of the same thing by two mods are
//! reported (the later mod wins), like a file two mods replace.
//!
//! A package's patch (mod.json: {"type": "sound", "archive": "/sound/wp_sound.brsar", "patch":
//! "sound/wp_sound/patch.json"}); paths in patch.json are relative to its folder:
//!
//!   {"format": 1,
//!    "waves":  [{"file": 621, "wave": 3, "rwav": "w/621_3.rwav"}],   // a wave of a file's RWAR
//!    "sounds": {"SEQ_SE_X": {"volume": 90, "priority": 70}},         // INFO fields (rsar.rs)
//!    "wsd":    {"WSD_X": {"pitch": 1.2, "note": {"release": 100}}},  // a wave sound's RWSD entry
//!    "groups": [{"label": "GROUP_MOD_X",
//!                "files": [{"main": "g/x.rwsd", "waves": "g/x.rwar"}, {"bank": "BANK_SEBD001"}],
//!                "banks": [{"label": "BANK_MOD_X", "file": 1}],
//!                "sounds": [{"label": "WSD_MOD_X_A", "type": "wave", "file": 0, "player": "PLAYER_SE_GAME",
//!                            "fields": {"volume": 100, "sub": 0}},
//!                           {"label": "SEQ_MOD_X_B", "type": "seq", "file": 2, "player": "PLAYER_SE_GAME",
//!                            "bank": "BANK_MOD_X", "fields": {"data_offset": 0}}]}]}
//!
//! "file" of a group's sound or bank = index in that group's "files". A {"bank": label} file is a file
//! of the archive (the bank's RBNK and its waves) also put in the new group.

pub mod files;
pub mod rsar;

use rsar::Archive;
use serde::de::{Deserializer, MapAccess, Visitor};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path};

/// One mod's patch of an archive.
pub struct SoundPatch<'a> {
    pub id: &'a str,
    /// Its patch.json.
    pub json: &'a str,
    /// Reads a file the patch names (relative to the patch's folder).
    pub file: &'a dyn Fn(&str) -> Result<Vec<u8>, String>,
}

/// Why the patches could not be applied, and the mod whose patch it is (None: the archive).
#[derive(Debug)]
pub struct MergeError {
    pub message: String,
    pub id: Option<String>,
}

impl std::fmt::Display for MergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.id {
            Some(id) => write!(f, "{id} : {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

/// The archive (the disc's file) with the patches applied in load order, and the warnings (two mods
/// that change the same thing: the later one wins).
pub fn merge(archive: Vec<u8>, patches: &[SoundPatch]) -> Result<(Vec<u8>, Vec<String>), MergeError> {
    let run = || {
        let fail = |id: Option<&str>| {
            let id = id.map(String::from);
            move |message: String| MergeError { message, id }
        };
        let mut m = Merger { a: Archive::parse(archive).map_err(fail(None))?, owner: HashMap::new(), warnings: Vec::new(), rwar: BTreeMap::new(), rwsd: BTreeMap::new() };
        for p in patches {
            m.apply(p).map_err(fail(Some(p.id)))?;
        }
        let bytes = m.to_bytes().map_err(fail(None))?;
        Ok((bytes, m.warnings))
    };
    // (an archive or a patch file not as expected must not take the launcher down)
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).unwrap_or_else(|_| Err(MergeError { message: "archive ou fichier de son illisible".into(), id: None }))
}

/// The voices' archive: the voice player reads it into fixed buffers (merge.py's _check_voice).
pub const VOICE_ARCHIVE: &str = "/sound/McVoice.brsar";
const VOICE_HEADER: usize = 0x4000;
const VOICE_HEAP: usize = 0x10_0000;
const VOICE_MARGIN: usize = 0x1000;

/// Whether the merged archive (`archive`: its disc path) still fits where the game reads it.
pub fn check_archive(archive: &str, data: &[u8]) -> Result<(), String> {
    if archive != VOICE_ARCHIVE {
        return Ok(());
    }
    let info = data.get(0x1C..0x20).map_or(usize::MAX, |b| u32::from_be_bytes(b.try_into().unwrap()) as usize);
    if info > VOICE_HEADER - 0x100 {
        return Err(format!("McVoice.brsar : INFO de {info:#x} octets, le lecteur de voix n'en lit que {VOICE_HEADER:#x}"));
    }
    let a = Archive::parse(data.to_vec())?;
    let size = if a.groups.is_empty() { 0 } else { a.group_bytes(0) };
    if size > VOICE_HEAP - VOICE_MARGIN {
        return Err(format!("McVoice.brsar : GROUP_MCVOICE fait {size} octets, le tas des voix {VOICE_HEAP} (raccourcir les voix ou garder \"fit\": \"same\")"));
    }
    Ok(())
}

/// What a patch changes, to tell the clashes between mods without the disc (mods.rs).
#[allow(dead_code)] // (examples/wplink.rs)
#[derive(Default, Debug, PartialEq)]
pub struct Claims {
    /// What it edits ("onde 43 du fichier 662", "SEQ_SE_X.volume"…): two mods editing the same
    /// thing get a warning, the later one wins.
    pub edits: Vec<String>,
    /// The names it adds (groups, banks, sounds): two mods can't add the same.
    pub names: Vec<String>,
    /// The files it reads, relative to the patch's folder.
    pub files: Vec<String>,
}

#[allow(dead_code)] // (examples/wplink.rs)
pub fn claims(json: &str) -> Result<Claims, String> {
    let p = parse(json)?;
    let mut files: Vec<String> = p.waves.iter().map(|w| w.rwav.clone()).collect();
    let mut names = Vec::new();
    for g in &p.groups {
        names.push(g.label.clone());
        for f in &g.files {
            if let GroupFile::New { main, waves } = f {
                files.push(main.clone());
                files.extend(waves.iter().filter(|w| !w.is_empty()).cloned());
            }
        }
        names.extend(g.banks.iter().map(|b| b.label.clone()));
        names.extend(g.sounds.iter().map(|s| s.label.clone()));
    }
    Ok(Claims { edits: edits(&p), names, files })
}

/// The package's path of a file a patch names (`patch`: the patch's path in the package), None when
/// it would leave the patch's folder.
pub fn package_path(patch: &str, name: &str) -> Option<String> {
    let plain = !name.is_empty() && Path::new(name).components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    let folder = patch.rsplit_once('/').map_or("", |(f, _)| f);
    plain.then(|| if folder.is_empty() { name.to_string() } else { format!("{folder}/{name}") })
}

// ===================================================================================== patch.json

/// A JSON object's members in the file's order (as merge.py sees them: the warnings come in it).
struct Ordered<V>(Vec<(String, V)>);

impl<V> Default for Ordered<V> {
    fn default() -> Self {
        Ordered(Vec::new())
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for Ordered<V> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Members<V>(std::marker::PhantomData<V>);
        impl<'de, V: Deserialize<'de>> Visitor<'de> for Members<V> {
            type Value = Ordered<V>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("un objet")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Ordered<V>, A::Error> {
                let mut out = Vec::new();
                while let Some(member) = map.next_entry()? {
                    out.push(member);
                }
                Ok(Ordered(out))
            }
        }
        d.deserialize_map(Members(std::marker::PhantomData))
    }
}

fn one() -> u32 {
    1
}

#[derive(Deserialize)]
struct Patch {
    #[serde(default = "one")]
    format: u32,
    #[serde(default)]
    waves: Vec<WaveEdit>,
    #[serde(default)]
    sounds: Ordered<Ordered<Value>>,
    #[serde(default)]
    wsd: Ordered<Ordered<WsdValue>>,
    #[serde(default)]
    groups: Vec<Group>,
}

#[derive(Deserialize)]
struct WaveEdit {
    file: u32,
    wave: usize,
    rwav: String,
}

/// A member of a "wsd" edit: "note": {...} edits some of the note's fields, "pitch": 1.2 the field.
#[derive(Deserialize)]
#[serde(untagged)]
enum WsdValue {
    Fields(Ordered<Value>),
    Value(Value),
}

#[derive(Deserialize)]
struct Group {
    label: String,
    files: Vec<GroupFile>,
    #[serde(default)]
    banks: Vec<BankDef>,
    #[serde(default)]
    sounds: Vec<SoundDef>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum GroupFile {
    /// A bank's file of the archive.
    Bank { bank: String },
    /// Files of the package.
    New {
        main: String,
        #[serde(default)]
        waves: Option<String>,
    },
}

#[derive(Deserialize)]
struct BankDef {
    label: String,
    file: usize,
}

#[derive(Deserialize)]
struct SoundDef {
    label: String,
    #[serde(rename = "type")]
    kind: String,
    file: usize,
    player: String,
    #[serde(default)]
    bank: Option<String>,
    #[serde(default)]
    fields: Ordered<Value>,
}

fn parse(json: &str) -> Result<Patch, String> {
    let p: Patch = serde_json::from_str(json).map_err(|e| format!("patch de son invalide : {e}"))?;
    if p.format != 1 {
        return Err(format!("patch de son au format {}, inconnu", p.format));
    }
    Ok(p)
}

/// What a patch edits, in merge.py's order.
fn edits(p: &Patch) -> Vec<String> {
    let mut out: Vec<String> = p.waves.iter().map(|w| format!("onde {} du fichier {}", w.wave, w.file)).collect();
    for (label, fields) in &p.sounds.0 {
        out.extend(fields.0.iter().map(|(k, _)| format!("{label}.{k}")));
    }
    for (label, edit) in &p.wsd.0 {
        for (k, v) in &edit.0 {
            match v {
                WsdValue::Fields(fields) => out.extend(fields.0.iter().map(|(kk, _)| format!("{label}.{k}.{kk}"))),
                WsdValue::Value(_) => out.push(format!("{label}.{k}")),
            }
        }
    }
    out
}

// ===================================================================================== merging

fn int(k: &str, v: &Value) -> Result<i64, String> {
    match v {
        Value::Bool(b) => Ok(*b as i64),
        _ => v.as_i64().ok_or_else(|| format!("{k} : valeur {v} invalide")),
    }
}

fn byte(k: &str, v: &Value) -> Result<u8, String> {
    u8::try_from(int(k, v)?).map_err(|_| format!("{k} : {v} hors limites"))
}

fn float(k: &str, v: &Value) -> Result<f32, String> {
    v.as_f64().map(|f| f as f32).ok_or_else(|| format!("{k} : valeur {v} invalide"))
}

/// Sets a field of a wave sound's RWSD entry (`kk`: of its "event" or "note").
fn set_wsd(e: &mut files::Wsd, k: &str, kk: Option<&str>, v: &Value) -> Result<(), String> {
    let name = kk.map_or(k.to_string(), |kk| format!("{k}.{kk}"));
    match (k, kk) {
        ("pitch", None) => e.pitch = float(&name, v)?,
        ("pan", None) => e.pan = byte(&name, v)?,
        ("surround_pan", None) => e.surround_pan = byte(&name, v)?,
        ("main_send", None) => e.main_send = byte(&name, v)?,
        ("fx_send", None) => {
            let sends = v.as_array().filter(|a| a.len() == 3).ok_or_else(|| format!("{name} : il faut 3 valeurs"))?;
            for (i, s) in sends.iter().enumerate() {
                e.fx_send[i] = byte(&name, s)?;
            }
        }
        ("event", Some("pos")) => e.event.0 = float(&name, v)?,
        ("event", Some("length")) => e.event.1 = float(&name, v)?,
        ("note", Some("wave")) => e.note.wave = i32::try_from(int(&name, v)?).map_err(|_| format!("{name} : {v} hors limites"))?,
        ("note", Some("tune")) => e.note.tune = float(&name, v)?,
        ("note", Some(kk)) => {
            let i = files::NOTE_BYTES.iter().position(|n| *n == kk).ok_or_else(|| format!("champ {name} inconnu"))?;
            e.note.bytes[i] = byte(&name, v)?;
        }
        _ => return Err(format!("champ {name} inconnu")),
    }
    Ok(())
}

struct Merger {
    a: Archive,
    /// What was edited -> the mod that did it.
    owner: HashMap<String, String>,
    warnings: Vec<String>,
    /// The waves of the files whose waves are replaced.
    rwar: BTreeMap<usize, Vec<Vec<u8>>>,
    /// The wave sounds of the RWSD edited.
    rwsd: BTreeMap<usize, Vec<files::Wsd>>,
}

impl Merger {
    fn claim(&mut self, key: String, id: &str) {
        if let Some(other) = self.owner.get(&key).filter(|o| *o != id) {
            self.warnings.push(format!("{id} et {other} changent tous deux {key} : {id} l'emporte"));
        }
        self.owner.insert(key, id.to_string());
    }

    /// A field's value: a player or bank by name, or a number.
    fn value(&self, k: &str, v: &Value) -> Result<i64, String> {
        match (k, v) {
            ("player", Value::String(name)) => self.a.player_id(name).map(|i| i as i64).ok_or_else(|| format!("lecteur {name} introuvable dans l'archive")),
            ("bank", Value::String(name)) => self.a.bank_id(name).map(|i| i as i64).ok_or_else(|| format!("banque {name} introuvable dans l'archive")),
            _ => int(k, v),
        }
    }

    fn sound(&self, label: &str) -> Result<usize, String> {
        self.a.sound_id(label).ok_or_else(|| format!("son {label} introuvable dans l'archive"))
    }

    fn apply(&mut self, p: &SoundPatch) -> Result<(), String> {
        let patch = parse(p.json)?;
        for key in edits(&patch) {
            self.claim(key, p.id);
        }
        for w in &patch.waves {
            let fid = w.file as usize;
            if !self.rwar.contains_key(&fid) {
                let wave = self.a.wave.get(fid).and_then(|w| w.as_deref()).ok_or_else(|| format!("fichier {fid} absent de l'archive"))?;
                self.rwar.insert(fid, files::rwar_split(wave).map_err(|e| format!("fichier {fid} : {e}"))?);
            }
            let waves = self.rwar.get_mut(&fid).unwrap();
            if w.wave >= waves.len() {
                return Err(format!("fichier {fid} sans onde {}", w.wave));
            }
            waves[w.wave] = (p.file)(&w.rwav)?;
        }
        for (label, fields) in &patch.sounds.0 {
            let sid = self.sound(label)?;
            let values = fields.0.iter().map(|(k, v)| self.value(k, v).map(|v| (k, v))).collect::<Result<Vec<_>, _>>()?;
            for (k, v) in values {
                self.a.set_sound(sid, k, v).map_err(|e| format!("{label} : {e}"))?;
            }
        }
        for (label, edit) in &patch.wsd.0 {
            let sid = self.sound(label)?;
            let fid = self.a.sound_field(sid, "file")? as usize;
            let sub = self.a.sound_field(sid, "sub").map_err(|e| format!("{label} : {e}"))?;
            if !self.rwsd.contains_key(&fid) {
                let main = self.a.main.get(fid).and_then(|m| m.as_deref()).ok_or_else(|| format!("fichier {fid} absent de l'archive"))?;
                self.rwsd.insert(fid, files::read_rwsd(main).map_err(|e| format!("{label} : {e}"))?);
            }
            let entries = self.rwsd.get_mut(&fid).unwrap();
            let e = usize::try_from(sub).ok().and_then(|s| entries.get_mut(s)).ok_or_else(|| format!("{label} : pas d'entrée {sub} dans son RWSD"))?;
            for (k, v) in &edit.0 {
                match v {
                    WsdValue::Fields(fields) => {
                        for (kk, vv) in &fields.0 {
                            set_wsd(e, k, Some(kk), vv).map_err(|e| format!("{label} : {e}"))?;
                        }
                    }
                    WsdValue::Value(v) => set_wsd(e, k, None, v).map_err(|e| format!("{label} : {e}"))?,
                }
            }
        }
        for g in &patch.groups {
            let mut fids = Vec::new();
            for f in &g.files {
                match f {
                    GroupFile::Bank { bank } => {
                        let b = self.a.bank_id(bank).ok_or_else(|| format!("banque {bank} introuvable dans l'archive"))?;
                        fids.push(self.a.bank_file(b));
                    }
                    GroupFile::New { main, waves } => {
                        let main = (p.file)(main)?;
                        let wave = match waves.as_deref().filter(|w| !w.is_empty()) {
                            Some(w) => (p.file)(w)?,
                            None => Vec::new(),
                        };
                        fids.push(self.a.add_file(main, wave)?);
                    }
                }
            }
            let file = |i: usize, label: &str| fids.get(i).copied().ok_or_else(|| format!("{label} : le groupe n'a pas de fichier {i}"));
            self.a.add_group(&g.label, &fids)?;
            for b in &g.banks {
                self.a.add_bank(&b.label, file(b.file, &b.label)?)?;
            }
            for s in &g.sounds {
                let kind = match s.kind.as_str() {
                    "seq" => rsar::SOUND_SEQ,
                    "strm" => rsar::SOUND_STRM,
                    "wave" => rsar::SOUND_WAVE,
                    other => return Err(format!("{} : type de son « {other} » inconnu", s.label)),
                };
                let mut fields = s.fields.0.iter().map(|(k, v)| self.value(k, v).map(|v| (k.clone(), v))).collect::<Result<Vec<_>, _>>()?;
                if kind == rsar::SOUND_SEQ {
                    let bank = s.bank.as_deref().ok_or_else(|| format!("{} : séquence sans banque", s.label))?;
                    fields.push(("bank".into(), self.value("bank", &Value::String(bank.into()))?));
                }
                let player = self.a.player_id(&s.player).ok_or_else(|| format!("lecteur {} introuvable dans l'archive", s.player))?;
                self.a.add_sound(&s.label, kind, file(s.file, &s.label)?, player, &fields).map_err(|e| format!("{} : {e}", s.label))?;
            }
        }
        Ok(())
    }

    fn to_bytes(&mut self) -> Result<Vec<u8>, String> {
        for (fid, waves) in std::mem::take(&mut self.rwar) {
            self.a.replace_file(fid, None, Some(files::build_rwar(&waves)))?;
        }
        for (fid, entries) in std::mem::take(&mut self.rwsd) {
            self.a.replace_file(fid, Some(files::build_rwsd(&entries)), None)?;
        }
        self.a.to_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The disc's sound folder, extracted next to the launcher in the mod project; the tests that need
    /// it say so and pass when it isn't there.
    fn disc_sound(name: &str) -> Option<Vec<u8>> {
        // (WP_SOUND_DISC: another disc root, e.g. one whose archive a version replaces whole)
        let root = std::env::var("WP_SOUND_DISC").map(std::path::PathBuf::from).unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../original/fst"));
        let path = root.join("files/sound").join(name);
        let found = std::fs::read(&path).ok();
        if found.is_none() {
            eprintln!("{} absent : test passé", path.display());
        }
        found
    }

    #[test]
    fn rebuilds_the_disc_archives() {
        for name in ["wp_sound.brsar", "McVoice.brsar"] {
            let Some(d) = disc_sound(name) else { return };
            let mut a = Archive::parse(d.clone()).unwrap();
            assert!(a.to_bytes().unwrap() == d, "{name} non modifié");
            // the name trees, as Nintendo's tool made them
            for (k, t) in a.name_trees().unwrap().iter().enumerate() {
                assert_eq!(t.nodes, a.trees[k], "{name} : arbre {k}");
            }
            // every section rebuilt
            a.dirty_symb = true;
            a.dirty_info = true;
            a.dirty_file = true;
            let again = a.to_bytes().unwrap();
            assert_eq!(again.len(), d.len(), "{name}");
            assert!(again == d, "{name} : première différence à 0x{:X}", again.iter().zip(&d).position(|(x, y)| x != y).unwrap());
            // the RWAR and RWSD inside
            let (mut rwar, mut rwsd) = (0, 0);
            for (m, w) in a.main.iter().zip(&a.wave) {
                let (Some(m), Some(w)) = (m, w) else { continue };
                if !w.is_empty() {
                    assert!(files::build_rwar(&files::rwar_split(w).unwrap()) == *w, "{name} : RWAR");
                    rwar += 1;
                }
                if m.starts_with(b"RWSD") {
                    assert!(files::build_rwsd(&files::read_rwsd(m).unwrap()) == *m, "{name} : RWSD");
                    rwsd += 1;
                }
            }
            println!("{name} : identique, {rwar} RWAR, {rwsd} RWSD refaits");
        }
    }

    /// Two mods on the disc's archive: a group each (new files, a bank of the game), the same wave
    /// replaced and the same fields edited (warnings), then the result read again.
    #[test]
    fn adds_two_mods_groups() {
        let Some(d) = disc_sound("wp_sound.brsar") else { return };
        let a = Archive::parse(d.clone()).unwrap();
        // a wave sound of the game and its files, as a mod's new files
        let sid = (0..a.sounds.len()).find(|&i| a.sounds[i].data[0x16] == rsar::SOUND_WAVE).unwrap();
        let fid = a.sound_field(sid, "file").unwrap() as usize;
        let (rwsd, rwar) = (a.main[fid].clone().unwrap(), a.wave[fid].clone().unwrap());
        let wave_label = rsar::latin1_str(&a.strings[a.sound_field(sid, "string").unwrap() as usize]);
        let bank = rsar::latin1_str(&a.strings[u32_at(&a.banks[0].data, 0) as usize]);
        let player = rsar::latin1_str(&a.strings[u32_at(&a.players[0].data, 0) as usize]);
        let rwav = files::rwar_split(&rwar).unwrap()[0].clone();
        let groups = a.groups.len();
        let patch = |x: &str| {
            format!(
                r#"{{"format": 1, "waves": [{{"file": {fid}, "wave": 0, "rwav": "w.rwav"}}],
                    "sounds": {{"{wave_label}": {{"volume": 77, "player": "{player}"}}}},
                    "wsd": {{"{wave_label}": {{"pitch": 1.1, "note": {{"release": 100}}}}}},
                    "groups": [{{"label": "GROUP_MOD_{x}", "files": [{{"main": "g.rwsd", "waves": "g.rwar"}}, {{"bank": "{bank}"}}],
                                 "banks": [{{"label": "BANK_MOD_{x}", "file": 1}}],
                                 "sounds": [{{"label": "WSD_MOD_{x}", "type": "wave", "file": 0, "player": "{player}", "fields": {{"volume": 90, "sub": 0}}}}]}}]}}"#
            )
        };
        let load = |name: &str| match name {
            "w.rwav" => Ok(rwav.clone()),
            "g.rwsd" => Ok(rwsd.clone()),
            "g.rwar" => Ok(rwar.clone()),
            _ => Err(format!("{name} ?")),
        };
        let (pa, pb) = (patch("A"), patch("B"));
        let patches = [SoundPatch { id: "a", json: &pa, file: &load }, SoundPatch { id: "b", json: &pb, file: &load }];
        let (out, warnings) = merge(d.clone(), &patches).unwrap();
        assert_eq!(warnings.len(), 5, "{warnings:?}");
        assert_eq!(warnings[0], format!("b et a changent tous deux onde 0 du fichier {fid} : b l'emporte"));
        assert_eq!(warnings[2], format!("b et a changent tous deux {wave_label}.player : b l'emporte"));

        let mut b = Archive::parse(out.clone()).unwrap();
        // the trees written are the ones of the names, which find the new ones
        let trees = b.name_trees().unwrap();
        for (k, t) in trees.iter().enumerate() {
            assert_eq!(t.nodes, b.trees[k]);
        }
        let new_sound = trees[0].lookup(b"WSD_MOD_B").unwrap() as usize;
        assert_eq!(new_sound, a.sounds.len() + 1);
        assert_eq!(b.sound_field(new_sound, "volume").unwrap(), 90);
        assert_eq!(trees[2].lookup(b"GROUP_MOD_A"), Some(groups as u32 - 1));
        assert_eq!(trees[2].lookup(b"GROUP_MOD_B"), Some(groups as u32));
        assert_eq!(trees[3].lookup(b"BANK_MOD_B"), Some(a.banks.len() as u32 + 1));
        // the game's last, empty group stays last; the bank's file is in both new groups
        assert_eq!(b.groups.len(), groups + 2);
        assert_eq!(u32_at(&b.groups[groups + 1].data, 0), rsar::NONE);
        let bank_file = b.bank_file(0) as usize;
        let positions = b.file_groups(bank_file);
        assert_eq!(positions[positions.len() - 2..], [(groups as u32 - 1, 1), (groups as u32, 1)]);
        // the edits
        assert_eq!(b.sound_field(sid, "volume").unwrap(), 77);
        let edited = files::read_rwsd(b.main[fid].as_ref().unwrap()).unwrap()[b.sound_field(sid, "sub").unwrap() as usize].clone();
        assert_eq!((edited.pitch, edited.note.bytes[3]), (1.1f32, 100));
        assert!(files::rwar_split(b.wave[fid].as_ref().unwrap()).unwrap()[0] == rwav);
        // read again, it writes itself again
        b.dirty_symb = true;
        b.dirty_info = true;
        b.dirty_file = true;
        assert!(b.to_bytes().unwrap() == out);

        // the same name twice: refused, the mod named
        let err = merge(d, &[SoundPatch { id: "a", json: &pa, file: &load }, SoundPatch { id: "c", json: &pa, file: &load }]).unwrap_err();
        assert_eq!((err.id.as_deref(), err.message.as_str()), (Some("c"), "GROUP_MOD_A existe déjà dans l'archive"));
    }

    #[test]
    fn checks_the_voices_buffers() {
        let Some(d) = disc_sound("McVoice.brsar") else { return };
        assert_eq!(check_archive(VOICE_ARCHIVE, &d), Ok(()));
        let a = Archive::parse(d.clone()).unwrap();
        println!("McVoice : INFO 0x{:X}, GROUP_MCVOICE {} octets", u32_at(&d, 0x1C), a.group_bytes(0));
        let mut big = d.clone();
        big[0x1C..0x20].copy_from_slice(&0x3F01u32.to_be_bytes());
        assert!(check_archive(VOICE_ARCHIVE, &big).unwrap_err().contains("INFO de 0x3f01"));
        assert_eq!(check_archive("/sound/wp_sound.brsar", &big), Ok(()));
        // a voice group past the heap
        let mut a = a;
        let fid = a.group_items(0)[0] as usize;
        a.replace_file(fid, None, Some(vec![0u8; 0x10_0000])).unwrap();
        let grown = a.to_bytes().unwrap();
        assert!(check_archive(VOICE_ARCHIVE, &grown).unwrap_err().contains("GROUP_MCVOICE fait"));
    }

    #[test]
    fn tells_what_a_patch_claims() {
        let json = r#"{"format": 1, "waves": [{"file": 3, "wave": 1, "rwav": "w/3_1.rwav"}],
            "wsd": {"WSD_X": {"note": {"release": 1, "attack": 2}, "pitch": 1}},
            "groups": [{"label": "G", "files": [{"main": "g.rwsd", "waves": ""}, {"bank": "B"}], "banks": [{"label": "BK", "file": 1}],
                        "sounds": [{"label": "S", "type": "wave", "file": 0, "player": "P"}]}]}"#;
        let c = claims(json).unwrap();
        assert_eq!(c.edits, ["onde 1 du fichier 3", "WSD_X.note.release", "WSD_X.note.attack", "WSD_X.pitch"]);
        assert_eq!(c.names, ["G", "BK", "S"]);
        assert_eq!(c.files, ["w/3_1.rwav", "g.rwsd"]);
        assert!(claims(r#"{"format": 2}"#).is_err());
        assert_eq!(package_path("sound/wp_sound/patch.json", "w/1.rwav").as_deref(), Some("sound/wp_sound/w/1.rwav"));
        assert_eq!(package_path("patch.json", "g.rwsd").as_deref(), Some("g.rwsd"));
        assert_eq!(package_path("sound/patch.json", "../mod.json"), None);
        assert_eq!(package_path("sound/patch.json", "/etc/passwd"), None);
    }

    /// The packages merged as merge.py does, the same bytes and warnings: WP_SOUND_CASES, a folder of
    /// cases made by scripts/sound-cases.py (each: mods/<n>/ unpacked packages in load order, out/ and
    /// result.json from merge.py). Passes without it.
    #[test]
    fn merges_as_merge_py() {
        let Ok(cases) = std::env::var("WP_SOUND_CASES") else {
            eprintln!("WP_SOUND_CASES absent : test passé");
            return;
        };
        let mut done = 0;
        let mut list: Vec<PathBuf> = std::fs::read_dir(&cases).unwrap().flatten().map(|e| e.path()).filter(|p| p.join("result.json").is_file()).collect();
        list.sort();
        for case in list {
            let mut mods: Vec<PathBuf> = std::fs::read_dir(case.join("mods")).unwrap().flatten().map(|e| e.path()).collect();
            mods.sort();
            let manifests: Vec<Value> = mods
                .iter()
                .map(|m| serde_json::from_str(&std::fs::read_to_string(m.join("mod.json")).or_else(|_| std::fs::read_to_string(m.join("version.json"))).unwrap()).unwrap())
                .collect();
            let mut by_archive: Vec<(String, Vec<(usize, String)>)> = Vec::new();
            for (i, m) in manifests.iter().enumerate() {
                for p in m["patches"].as_array().unwrap().iter().filter(|p| p["type"] == "sound") {
                    let archive = p["archive"].as_str().unwrap().to_string();
                    let at = by_archive.iter().position(|a| a.0 == archive).unwrap_or_else(|| {
                        by_archive.push((archive.clone(), Vec::new()));
                        by_archive.len() - 1
                    });
                    by_archive[at].1.push((i, p["patch"].as_str().unwrap().to_string()));
                }
            }
            let result: Value = serde_json::from_str(&std::fs::read_to_string(case.join("result.json")).unwrap()).unwrap();
            let mut warnings = Vec::new();
            for (archive, patches) in &by_archive {
                let Some(disc) = disc_sound(archive.trim_start_matches("/sound/")) else { return };
                let texts: Vec<String> = patches.iter().map(|(i, p)| std::fs::read_to_string(mods[*i].join(p)).unwrap()).collect();
                let loaders: Vec<Box<dyn Fn(&str) -> Result<Vec<u8>, String>>> = patches
                    .iter()
                    .map(|(i, p)| {
                        let (dir, patch) = (mods[*i].clone(), p.clone());
                        Box::new(move |name: &str| std::fs::read(dir.join(package_path(&patch, name).unwrap())).map_err(|e| e.to_string())) as Box<dyn Fn(&str) -> Result<Vec<u8>, String>>
                    })
                    .collect();
                let ids: Vec<String> = patches.iter().map(|(i, _)| manifests[*i]["id"].as_str().unwrap().to_string()).collect();
                let list: Vec<SoundPatch> = (0..patches.len()).map(|k| SoundPatch { id: &ids[k], json: &texts[k], file: loaders[k].as_ref() }).collect();
                let (made, w) = merge(disc, &list).unwrap_or_else(|e| panic!("{} : {e}", case.display()));
                check_archive(archive, &made).unwrap();
                let want = std::fs::read(case.join("out").join(archive.trim_start_matches('/'))).unwrap();
                assert_eq!(made.len(), want.len(), "{} {archive}", case.display());
                assert!(made == want, "{} {archive} : première différence à 0x{:X}", case.display(), made.iter().zip(&want).position(|(x, y)| x != y).unwrap_or(0));
                println!("{} {archive} : {} octets identiques à merge.py, {} avertissement(s)", case.display(), made.len(), w.len());
                warnings.extend(w);
            }
            assert_eq!(serde_json::to_value(&warnings).unwrap(), result["warnings"], "{}", case.display());
            done += 1;
        }
        assert!(done > 0, "aucun cas dans {cases}");
    }

    use rsar::u32_at;
}
