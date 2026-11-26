//! PartyLauncher's linker: it puts the mods' code into the game (wp-core docs/modding.md), on top of the
//! mods' core: the game rebuilt from its decompilation (wp-srccore docs/core.md), whose files are made
//! from the player's disc (corepkg.rs) and go in place of the disc's. A code patch's modules: "main"
//! (main.dol: low memory and injections, dol.rs), "resident" (/wp/wpmods.rel, handlers of main.dol's
//! hook lists, resident.rs), a module of the core (mr001, inst, menu: module.rs) or a new REL.

pub mod corepkg;
pub mod dol;
pub mod elf;
pub mod minigames;
pub mod module;
pub mod rel;
pub mod resident;
pub mod xmsg;

/// Why the mods could not be linked, and which mods are concerned.
#[derive(Debug, Clone)]
pub struct LinkError {
    pub message: String,
    pub mods: Vec<String>,
}

impl LinkError {
    pub fn new(message: String, mods: &[&str]) -> LinkError {
        let mut list: Vec<String> = mods.iter().map(|m| m.to_string()).collect();
        list.dedup();
        LinkError { message, mods: list }
    }
}


use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A module part of a "code" patch (mod.json: patches[].modules.<module>).
#[derive(Deserialize, Clone, Default, Debug)]
pub struct CodeModule {
    /// Objects of the package (mwcc .o).
    #[serde(default)]
    pub objects: Vec<String>,
    /// main.dol: injections.
    #[serde(default)]
    pub inject: Vec<InjectSpec>,
    /// Core modules and "resident" (main.dol's lists, main.json): hook -> handler.
    #[serde(default)]
    pub hooks: BTreeMap<String, String>,
    /// Core modules: function of the game -> replacement.
    #[serde(default)]
    pub replace: BTreeMap<String, String>,
    /// Core modules: bytes of the board's save (IslandSaveData's padding) the mod keeps there. The launcher
    /// writes where they start (u32, from gWpSave) into `symbol`.
    #[serde(default)]
    pub save: Option<SaveSpec>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct InjectSpec {
    pub at: String,
    #[serde(default)]
    pub offset: String,
    pub expect: String,
    pub call: String,
    #[serde(default)]
    pub after: bool,
}

/// A "sound" patch (mod.json: patches[] of type "sound"): the disc's archive ("/sound/wp_sound.brsar")
/// and the package's patch.json (sound/mod.rs).
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct SoundSpec {
    pub archive: String,
    pub patch: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct SaveSpec {
    pub symbol: String,
    pub size: u32,
}

fn hex(text: &str) -> Option<u32> {
    let t = text.trim();
    if t.is_empty() {
        return Some(0);
    }
    u32::from_str_radix(t.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
}

/// One active mod's code: its folder, its modules' parts, and the values its settings write.
pub struct CodeMod<'a> {
    pub id: &'a str,
    pub dir: &'a Path,
    pub modules: BTreeMap<String, CodeModule>,
    /// (module, symbol, offset, bytes)
    pub writes: Vec<(String, String, u32, Vec<u8>)>,
    /// Its texts: language folder ("fr_EU") -> message key -> text.
    pub messages: BTreeMap<String, BTreeMap<String, String>>,
    /// Its texts when another mod is also in the list: that mod's id -> language -> key -> text.
    pub messages_with: BTreeMap<String, BTreeMap<String, BTreeMap<String, String>>>,
    /// The minigames it adds (minigames.rs).
    pub minigames: Vec<minigames::Spec>,
    /// Its patches of the disc's sound archives.
    pub sounds: Vec<SoundSpec>,
}

impl CodeMod<'_> {
    fn has_code(&self) -> bool {
        !self.modules.is_empty() || !self.minigames.is_empty()
    }
}

/// The texts mods give for the others of the list, by language.
fn texts_with<'a>(mods: &'a [CodeMod]) -> BTreeMap<&'a str, Vec<(&'a CodeMod<'a>, &'a BTreeMap<String, String>)>> {
    let mut out: BTreeMap<&str, Vec<_>> = BTreeMap::new();
    for m in mods {
        for (other, languages) in &m.messages_with {
            if mods.iter().any(|o| o.id == other) {
                for (language, texts) in languages {
                    out.entry(language).or_default().push((m, texts));
                }
            }
        }
    }
    out
}

/// The module id of the first new REL (a mod's minigame): the game's are 1..106.
pub const NEW_MODULE_ID: u32 = 0x100;

/// A language's message file on the disc.
pub fn messages_file(language: &str) -> String {
    format!("/locale/{language}/mess.bin")
}

/// What the core says about one of its modules (wp-srccore tools/core/export.py, <module>.json).
#[derive(Deserialize)]
struct ModuleInfo {
    #[serde(flatten)]
    params: rel::RelParams,
    #[serde(default)]
    hooks: Vec<module::HookDef>,
    /// Bytes of the board's save the mods share; left out: the size of the module's gWpSave (none: 0).
    #[serde(default)]
    save_bytes: Option<u32>,
}

/// The bytes of the board's save the mods share in `plf`.
fn save_bytes(info: &ModuleInfo, plf: &elf::Object) -> u32 {
    info.save_bytes.unwrap_or_else(|| plf.global("gWpSave").map_or(0, |s| plf.symbols[s].size))
}

/// The files and memory patches that put the mods' code into the game, as a Riivolution XML.
#[allow(dead_code)] // (examples/wplink.rs)
pub struct Linked {
    pub xml: PathBuf,
    /// The module files made (disc path), for the compatibility check.
    pub files: Vec<String>,
    /// What the player should know: two mods changing the same sound (the later one wins).
    pub warnings: Vec<String>,
}

/// Whether link_game has something to do: a mod has code, minigames, texts or sounds.
pub fn links(mods: &[CodeMod]) -> bool {
    mods.iter().any(|m| m.has_code() || !m.messages.is_empty() || !m.sounds.is_empty())
}

fn read_object(dir: &Path, path: &str, id: &str) -> Result<elf::Object, LinkError> {
    let bytes = std::fs::read(dir.join(path)).map_err(|_| LinkError::new(format!("{path} est introuvable"), &[id]))?;
    elf::Object::parse(&bytes).map_err(|e| LinkError::new(format!("{path} : {e}"), &[id]))
}

fn hexs(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

/// Links the mods' code (in load order) with the core into `out`: the core's files made from the disc
/// (`base`: disc path, file; corepkg.rs), the core's modules with the mods' code (REL, LZ-compressed as on
/// the disc), main.dol's memory patches, the mods' minigames (/wp/minigames.bin), the message files with
/// the mods' texts (from the disc's: `disc` reads one) and the sound archives with the mods' sound patches
/// (from the disc's too), all in `out/linked.xml` (section "PartyLauncher", option "Code", choice 1). A
/// file of the core that a mod replaces whole (`replaced`: lowercase disc paths) is left to the mod, as
/// the disc's would be, unless the mods' code changes it (mods.rs check tells that clash). None if no mod
/// has code, minigames, texts or sounds.
pub fn link_game(core: &Path, disc: &dyn Fn(&str) -> Result<Vec<u8>, String>, base: &[(String, PathBuf)], replaced: &[String], mods: &[CodeMod], out: &Path) -> Result<Option<Linked>, LinkError> {
    if !links(mods) {
        return Ok(None);
    }
    let kept = |disc: &str| replaced.iter().any(|r| r.eq_ignore_ascii_case(disc) || r.eq_ignore_ascii_case(&format!("/{disc}")));
    // the core's main.dol, where the injections' instructions are checked (unless a mod replaces it whole)
    let dol = base.iter().find(|(name, _)| name.eq_ignore_ascii_case("main.dol")).filter(|_| !kept("main.dol")).and_then(|(_, file)| std::fs::read(file).ok());
    let (built, made, warnings) = build(core, Some(disc), dol.as_deref(), &kept, mods)?;
    let io = |e: std::io::Error| LinkError::new(format!("{} : {e}", out.display()), &[]);
    let _ = std::fs::remove_dir_all(out);
    // the core's files first: the linked modules and the mods' new ones, after them, win
    let mut lines = Vec::new();
    for (name, file) in base.iter().filter(|(name, _)| !kept(name)) {
        let inside = corepkg::inside(name);
        corepkg::put(file, &out.join(inside)).map_err(io)?;
        lines.push(format!(r#"<file disc="{name}" external="{inside}"/>"#));
    }
    lines.extend(built);
    let mut files = Vec::new();
    for (name, bytes) in made {
        let inside = name.trim_start_matches('/');
        std::fs::create_dir_all(out.join(inside).parent().unwrap()).map_err(io)?;
        std::fs::write(out.join(inside), bytes).map_err(io)?;
        files.push(name);
    }
    let xml = format!(
        "<wiidisc version=\"1\">\n<id game=\"SUPP\"/>\n<options><section name=\"PartyLauncher\"><option name=\"Code\"><choice name=\"On\"><patch id=\"code\"/></choice></option></section></options>\n<patch id=\"code\">\n{}\n</patch>\n</wiidisc>\n",
        lines.join("\n")
    );
    let path = out.join("linked.xml");
    std::fs::create_dir_all(out).map_err(io)?;
    std::fs::write(&path, xml).map_err(io)?;
    Ok(Some(Linked { xml: path, files, warnings }))
}

/// Whether the mods' code links (the texts aside): what link_game would say, without writing anything.
#[allow(dead_code)] // (examples/wplink.rs)
pub fn check_code(core: &Path, mods: &[CodeMod]) -> Result<(), LinkError> {
    if mods.iter().all(|m| !m.has_code()) {
        return Ok(());
    }
    build(core, None, None, &|_| false, mods).map(|_| ())
}

/// The XML's patch lines, the files they name (disc path, contents) and the warnings. Without `disc`, no
/// files: only the modules the mods' code goes into are linked, and no texts and no sounds. `dol`: the
/// core's main.dol (the injections are checked against it). `kept`: a disc file a mod replaces whole.
type Built = (Vec<String>, Vec<(String, Vec<u8>)>, Vec<String>);

/// main.json of the core: every hook, with its list in main.dol.
fn main_hooks(core: &Path) -> Result<Vec<module::HookDef>, LinkError> {
    let text = std::fs::read_to_string(core.join("main.json")).map_err(|_| LinkError::new("ce cœur des mods n'a pas main.json (les hooks de main.dol) : launcher à mettre à jour".into(), &[]))?;
    serde_json::from_str::<resident::MainInfo>(&text).map(|i| i.hooks).map_err(|e| LinkError::new(format!("main.json : {e}"), &[]))
}

fn build(core: &Path, disc: Option<&dyn Fn(&str) -> Result<Vec<u8>, String>>, dol: Option<&[u8]>, kept: &dyn Fn(&str) -> bool, mods: &[CodeMod]) -> Result<Built, LinkError> {
    let all: Vec<&str> = mods.iter().map(|m| m.id).collect();
    let io = |e: std::io::Error| LinkError::new(format!("cœur des mods : {e}"), &[]);
    let mut main = rel::MainSymbols::parse(&std::fs::read_to_string(core.join("main.sym")).map_err(io)?).map_err(|e| LinkError::new(e, &[]))?;
    let mut lines = Vec::new();
    let mut made = Vec::new();

    // the mods' minigames: the core reads them from /wp/minigames.bin
    let added: minigames::Added = mods.iter().filter(|m| !m.minigames.is_empty()).map(|m| (m.id, &m.minigames[..])).collect();
    minigames::check(&added)?;
    // (made last: a sound group named by its label is found in the sound archive the mods patch)
    let mut registry = None;
    if !added.is_empty() && disc.is_some() {
        let inside = minigames::FILE.trim_start_matches('/');
        lines.push(format!(r#"<file disc="{}" external="{inside}" create="true"/>"#, minigames::FILE));
        registry = Some(made.len());
        made.push((minigames::FILE.to_string(), Vec::new()));
    }

    // main.dol
    let mut parts = Vec::new();
    for m in mods {
        let Some(part) = m.modules.get("main") else { continue };
        let fail = |msg: String| LinkError::new(msg, &[m.id]);
        if !part.hooks.is_empty() {
            return Err(fail(format!("les « hooks » du code de main.dol vont dans le module « {} » (chargé en {}), pas dans « main »", resident::MODULE, resident::FILE)));
        }
        if !part.replace.is_empty() || part.save.is_some() {
            return Err(fail("« replace » et « save » ne sont permis que dans un module du cœur (mr001, inst, menu), pas dans « main »".into()));
        }
        let objects = part.objects.iter().map(|o| read_object(m.dir, o, m.id)).collect::<Result<_, _>>()?;
        let mut injects = Vec::new();
        for i in &part.inject {
            let offset = hex(&i.offset).ok_or_else(|| fail(format!("décalage « {} » invalide", i.offset)))?;
            let expect = hex(&i.expect).ok_or_else(|| fail(format!("instruction « {} » invalide", i.expect)))?;
            injects.push(dol::Inject { symbol: i.at.clone(), offset, expect, handler: i.call.clone(), after: i.after });
        }
        let writes = m.writes.iter().filter(|w| w.0 == "main").map(|w| (w.1.clone(), w.2, w.3.clone())).collect();
        parts.push(dol::DolPart { id: m.id.to_string(), objects, injects, writes, ..Default::default() });
    }
    let (patches, symbols) = dol::link(&parts, &main, dol)?;
    for p in patches {
        match &p.original {
            Some(o) => lines.push(format!(r#"<memory offset="0x{:08X}" value="{}" original="{}"/>"#, p.address, hexs(&p.value), hexs(o))),
            None => lines.push(format!(r#"<memory offset="0x{:08X}" value="{}"/>"#, p.address, hexs(&p.value))),
        }
    }
    // the RELs may use the mods' main.dol code (a REL part that drives its main.dol part)
    for (name, address) in &symbols {
        main.add(name, *address);
    }

    // the resident code: /wp/wpmods.rel, whose prolog adds its handlers to main.dol's lists
    let mut hooks_of_main = None;
    let mut resident_users: Vec<(String, String)> = Vec::new();
    let residents: Vec<&CodeMod> = mods.iter().filter(|m| m.modules.contains_key(resident::MODULE)).collect();
    if !residents.is_empty() {
        let mut parts = Vec::new();
        for m in &residents {
            let part = &m.modules[resident::MODULE];
            let fail = |msg: String| LinkError::new(msg, &[m.id]);
            if !part.inject.is_empty() {
                return Err(fail(format!("les injections (« inject ») vont dans le module « main », pas dans « {} »", resident::MODULE)));
            }
            if part.save.is_some() {
                return Err(fail(format!("« save » n'est permis que dans mr001, pas dans « {} »", resident::MODULE)));
            }
            let objects = part.objects.iter().map(|o| read_object(m.dir, o, m.id)).collect::<Result<_, _>>()?;
            let writes = m.writes.iter().filter(|w| w.0 == resident::MODULE).map(|w| (w.1.clone(), w.2, w.3.clone())).collect();
            parts.push(module::ModPart {
                id: m.id.to_string(),
                objects,
                hooks: part.hooks.iter().map(|(h, f)| (h.clone(), f.clone())).collect(),
                replaces: part.replace.iter().map(|(t, w)| (t.clone(), w.clone())).collect(),
                writes,
            });
        }
        let defs = main_hooks(core).map_err(|e| LinkError::new(e.message, &residents.iter().map(|m| m.id).collect::<Vec<_>>()))?;
        let linked = resident::link(&parts, &defs, &main)?;
        resident_users = linked.users;
        hooks_of_main = Some(defs);
        if disc.is_some() {
            let inside = resident::FILE.trim_start_matches('/');
            lines.push(format!(r#"<file disc="{}" external="{inside}" create="true"/>"#, resident::FILE));
            made.push((resident::FILE.to_string(), linked.rel));
        }
    }

    // the core's modules: always shipped (the disc's would not fit the core's main.dol), but those a mod
    // replaces whole while no mod's code goes into them
    let package = corepkg::read(core).map_err(|e| LinkError::new(e, &[]))?;
    let cores = &package.modules;
    for module in cores {
        let module = module.clone();
        let users: Vec<&CodeMod> = mods.iter().filter(|m| m.modules.contains_key(&module)).collect();
        let name = format!("rel/{module}.rel.lz");
        if users.is_empty() && (disc.is_none() || kept(&name)) {
            continue;
        }
        let plf = elf::Object::parse(&std::fs::read(core.join(format!("{module}.plf"))).map_err(io)?).map_err(|e| LinkError::new(format!("{module}.plf : {e}"), &[]))?;
        let info: ModuleInfo = serde_json::from_str(&std::fs::read_to_string(core.join(format!("{module}.json"))).map_err(io)?)
            .map_err(|e| LinkError::new(format!("{module}.json : {e}"), &[]))?;
        let room = save_bytes(&info, &plf);
        let mut parts = Vec::new();
        let mut save_at = 0u32;
        let mut layout = 0x811C_9DC5u32; // FNV-1a of the mods' save layout: a save of another layout is ignored
        for m in &users {
            let part = &m.modules[&module];
            let objects = part.objects.iter().map(|o| read_object(m.dir, o, m.id)).collect::<Result<_, _>>()?;
            let mut writes: Vec<(String, u32, Vec<u8>)> = m.writes.iter().filter(|w| w.0 == module).map(|w| (w.1.clone(), w.2, w.3.clone())).collect();
            if let Some(save) = &part.save {
                if save_at + save.size > room {
                    let ids: Vec<&str> = users.iter().filter(|u| u.modules[&module].save.is_some()).map(|u| u.id).collect();
                    return Err(LinkError::new(format!("plus assez de place dans la sauvegarde du plateau ({room} octets en tout)"), &ids));
                }
                writes.push((save.symbol.clone(), 0, save_at.to_be_bytes().to_vec()));
                for b in m.id.bytes().chain(save_at.to_be_bytes()).chain(save.size.to_be_bytes()) {
                    layout = (layout ^ b as u32).wrapping_mul(0x0100_0193);
                }
                save_at += save.size;
            }
            parts.push(module::ModPart {
                id: m.id.to_string(),
                objects,
                hooks: part.hooks.iter().map(|(h, f)| (h.clone(), f.clone())).collect(),
                replaces: part.replace.iter().map(|(t, w)| (t.clone(), w.clone())).collect(),
                writes,
            });
        }
        // a hook this module gets its own list of (a part names it) while resident handlers are on
        // main.dol's: the module's list calls main.dol's (a chain), where the first resident mod comes
        let at = |id: &str| mods.iter().position(|m| m.id == id).unwrap_or(usize::MAX);
        let mut order: Vec<usize> = users.iter().map(|m| at(m.id)).collect();
        let mut chained: Vec<&str> = Vec::new();
        for (hook, first) in &resident_users {
            if chained.contains(&hook.as_str()) || !users.iter().any(|m| m.modules[&module].hooks.contains_key(hook)) {
                continue;
            }
            chained.push(hook);
            let def = hooks_of_main.as_ref().and_then(|d| d.iter().find(|d| &d.name == hook)).expect("resident hooks come from main.json");
            let object = resident::chain(def, &main).map_err(|e| LinkError::new(e, &[first.as_str()]))?;
            let index = at(first);
            let position = order.iter().position(|o| *o > index).unwrap_or(order.len());
            order.insert(position, index);
            parts.insert(position, module::ModPart { id: first.clone(), objects: vec![object], hooks: vec![(hook.clone(), resident::chain_symbol(hook))], ..Default::default() });
        }
        if room > 0 && plf.global("gWpSaveLayout").is_some() {
            parts.push(module::ModPart { id: "cœur".into(), writes: vec![("gWpSaveLayout".into(), 0, layout.to_be_bytes().to_vec())], ..Default::default() });
        }
        let linked = module::link(&plf, &info.hooks, &parts, &main)?;
        let bytes = rel::write(&linked, &info.params, &main).map_err(|e| LinkError::new(format!("{module} : {e}"), &all))?;
        if disc.is_some() {
            let packed = crate::lz::compress(&bytes);
            // (a twin of the disc plays the same module: mg418 = mg103)
            let twins = package.twins.iter().filter(|(_, of)| **of == module).map(|(twin, _)| format!("rel/{twin}.rel.lz"));
            for name in std::iter::once(name.clone()).chain(twins) {
                lines.push(format!(r#"<file disc="/{name}" external="{name}"/>"#));
                made.push((format!("/{name}"), packed.clone()));
            }
        }
    }

    // the mods' new modules (their minigames): each one of one mod, made from its objects alone
    let mut new: Vec<(&String, &CodeMod)> = Vec::new();
    for m in mods {
        for name in m.modules.keys().filter(|n| *n != "main" && *n != resident::MODULE && !cores.contains(n)) {
            if let Some((_, other)) = new.iter().find(|(n, _)| *n == name) {
                return Err(LinkError::new(format!("deux mods ont un module « {name} »"), &[other.id, m.id]));
            }
            new.push((name, m));
        }
    }
    new.sort_by(|a, b| a.0.cmp(b.0));
    for (index, (name, m)) in new.iter().enumerate() {
        let fail = |msg: String| LinkError::new(msg, &[m.id]);
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(fail(format!("nom de module « {name} » invalide")));
        }
        let part = &m.modules[*name];
        if !part.hooks.is_empty() || !part.replace.is_empty() || part.save.is_some() {
            return Err(fail(format!("« {name} » n'est pas un module du cœur : pas de hooks, replace ni save")));
        }
        let objects = part.objects.iter().map(|o| read_object(m.dir, o, m.id)).collect::<Result<_, _>>()?;
        let writes = m.writes.iter().filter(|w| &w.0 == *name).map(|w| (w.1.clone(), w.2, w.3.clone())).collect();
        let linked = module::link(&module::empty_module(), &[], &[module::ModPart { id: m.id.to_string(), objects, writes, ..Default::default() }], &main)?;
        let params = rel::RelParams {
            module_id: NEW_MODULE_ID + index as u32,
            num_sections: linked.sections.len() as u32,
            name_offset: 0,
            name_size: 0,
            version: 3,
            align: 32,
            bss_align: 32,
            exec: vec![],
        };
        let bytes = rel::write(&linked, &params, &main).map_err(|e| fail(format!("{name} : {e}")))?;
        if disc.is_some() {
            let file = format!("rel/{name}.rel.lz");
            lines.push(format!(r#"<file disc="/{file}" external="{file}" create="true"/>"#));
            made.push((format!("/{file}"), crate::lz::compress(&bytes)));
        }
    }

    // the texts: for one key, the last mod wins (mods.rs warns); those given for another mod of the
    // list win over all of them
    let Some(disc) = disc else { return Ok((lines, made, Vec::new())) };
    let with = texts_with(mods);
    let mut languages: BTreeMap<&str, Vec<&CodeMod>> = with.keys().map(|l| (*l, Vec::new())).collect();
    for m in mods {
        for language in m.messages.keys() {
            languages.entry(language).or_default().push(m);
        }
    }
    for (language, users) in languages {
        let ids: Vec<&str> = users.iter().map(|m| m.id).collect();
        let name = messages_file(language);
        let bytes = disc(&name).map_err(|e| LinkError::new(e, &ids))?;
        let mut file = xmsg::Xmsg::parse(bytes).map_err(|e| LinkError::new(format!("{name} : {e}"), &ids))?;
        for m in users {
            for (key, text) in &m.messages[language] {
                file.set(key, text).map_err(|e| LinkError::new(format!("{language} : {e}"), &[m.id]))?;
            }
        }
        for (m, texts) in with.get(language).into_iter().flatten() {
            for (key, text) in *texts {
                file.set(key, text).map_err(|e| LinkError::new(format!("{language} : {e}"), &[m.id]))?;
            }
        }
        lines.push(format!(r#"<file disc="{name}" external="{}"/>"#, name.trim_start_matches('/')));
        made.push((name, file.bytes().to_vec()));
    }
    let warnings = sounds(disc, mods, &mut lines, &mut made)?;
    if let Some(at) = registry {
        // a sound group by its label: in the archive as the mods' sound patches leave it
        let mut archive = None;
        let mut group = |label: &str| -> Result<Option<i32>, String> {
            if archive.is_none() {
                let bytes = match made.iter().find(|(name, _)| name.eq_ignore_ascii_case(minigames::SOUND_ARCHIVE)) {
                    Some((_, bytes)) => bytes.clone(),
                    None => disc(minigames::SOUND_ARCHIVE)?,
                };
                archive = Some(crate::sound::rsar::Archive::parse(bytes).map_err(|e| format!("{} : {e}", minigames::SOUND_ARCHIVE))?);
            }
            Ok(archive.as_ref().unwrap().group_id(label).map(|g| g as i32))
        };
        let bytes = minigames::file(&added, &mut group)?;
        made[at].1 = bytes;
    }
    Ok((lines, made, warnings))
}

/// The sound archives the mods patch: each the disc's with their patches applied in load order
/// (sound/mod.rs), last in the XML. The warnings: the same thing changed by two mods.
fn sounds(disc: &dyn Fn(&str) -> Result<Vec<u8>, String>, mods: &[CodeMod], lines: &mut Vec<String>, made: &mut Vec<(String, Vec<u8>)>) -> Result<Vec<String>, LinkError> {
    let mut archives: Vec<(&str, Vec<(&CodeMod, &SoundSpec)>)> = Vec::new();
    for m in mods {
        for s in &m.sounds {
            match archives.iter_mut().find(|a| a.0 == s.archive) {
                Some(a) => a.1.push((m, s)),
                None => archives.push((&s.archive, vec![(m, s)])),
            }
        }
    }
    let mut warnings = Vec::new();
    for (archive, patches) in archives {
        let ids: Vec<&str> = patches.iter().map(|p| p.0.id).collect();
        let bytes = disc(archive).map_err(|e| LinkError::new(e, &ids))?;
        let texts = patches
            .iter()
            .map(|(m, s)| std::fs::read_to_string(m.dir.join(&s.patch)).map_err(|_| LinkError::new(format!("{} est introuvable", s.patch), &[m.id])))
            .collect::<Result<Vec<String>, _>>()?;
        let loaders: Vec<_> = patches
            .iter()
            .map(|(m, s)| {
                move |name: &str| {
                    let path = crate::sound::package_path(&s.patch, name).ok_or_else(|| format!("{name} : hors du paquet"))?;
                    std::fs::read(m.dir.join(&path)).map_err(|_| format!("{path} est introuvable"))
                }
            })
            .collect();
        let list: Vec<crate::sound::SoundPatch> = patches.iter().zip(&texts).zip(&loaders).map(|(((m, _), json), file)| crate::sound::SoundPatch { id: m.id, json, file }).collect();
        let (bytes, w) = crate::sound::merge(bytes, &list).map_err(|e| {
            let message = format!("{archive} : {}", e.message);
            match &e.id {
                Some(id) => LinkError::new(message, &[id.as_str()]),
                None => LinkError::new(message, &ids),
            }
        })?;
        crate::sound::check_archive(archive, &bytes).map_err(|e| LinkError::new(e, &ids))?;
        lines.push(format!(r#"<file disc="{archive}" external="{}"/>"#, archive.trim_start_matches('/')));
        made.push((archive.to_string(), bytes));
        warnings.extend(w);
    }
    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The core's export (wp-srccore tools/core/export.py): WP_CORE=<…/build/SUPP01/corepkg>.
    fn core_dir() -> Option<std::path::PathBuf> {
        std::env::var("WP_CORE").ok().map(Into::into)
    }

    /// The launcher's core (src-tauri/core), its files made from the disc faked (the files are only put in place).
    #[test]
    fn puts_the_core_in_place_of_the_disc() {
        let core = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("core");
        let dir = std::env::temp_dir().join("wp-link-core-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("made/rel")).unwrap();
        let base: Vec<(String, PathBuf)> = ["main.dol", "/rel/mg100.rel.lz", "/rel/mg101.rel.lz"].iter().map(|d| (d.to_string(), dir.join("made").join(d.trim_start_matches('/')))).collect();
        for (d, f) in &base {
            std::fs::write(f, d).unwrap();
        }
        let spec: minigames::Spec = serde_json::from_str(r#"{"id": 305, "category": "1v3", "sounds_of": 406, "controls": 110, "controls_team": 2}"#).unwrap();
        let winter = CodeMod {
            id: "winter",
            dir: &dir,
            modules: BTreeMap::new(),
            writes: Vec::new(),
            messages: BTreeMap::new(),
            messages_with: BTreeMap::new(),
            minigames: vec![spec],
            sounds: Vec::new(),
        };
        let disc = |path: &str| Err(format!("{path} : pas de disque"));
        let out = dir.join("linked");
        // a mod replaces mg101 and inst whole: theirs are kept
        let replaced = ["/rel/mg101.rel.lz".to_string(), "/rel/inst.rel.lz".to_string()];
        let linked = link_game(&core, &disc, &base, &replaced, &[winter], &out).unwrap().unwrap();
        let xml = std::fs::read_to_string(&linked.xml).unwrap();
        let lines: Vec<&str> = xml.lines().filter(|l| l.starts_with("<file") || l.starts_with("<memory")).collect();
        assert_eq!(
            lines,
            [
                r#"<file disc="main.dol" external="main.dol"/>"#,
                r#"<file disc="/rel/mg100.rel.lz" external="rel/mg100.rel.lz"/>"#,
                r#"<file disc="/wp/minigames.bin" external="wp/minigames.bin" create="true"/>"#,
                r#"<file disc="/rel/mr001.rel.lz" external="rel/mr001.rel.lz"/>"#,
                r#"<file disc="/rel/menu.rel.lz" external="rel/menu.rel.lz"/>"#,
            ]
        );
        assert_eq!(std::fs::read_to_string(out.join("main.dol")).unwrap(), "main.dol");
        assert_eq!(std::fs::read(out.join("wp/minigames.bin")).unwrap().len(), 16 + 0x28);
        assert!(crate::lz::decompress(&std::fs::read(out.join("rel/mr001.rel.lz")).unwrap()).is_ok());
        assert!(!out.join("rel/mg101.rel.lz").exists() && !out.join("rel/inst.rel.lz").exists());
        // no code, no texts, no sounds: nothing
        assert!(link_game(&core, &disc, &base, &[], &[], &out).unwrap().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn code(id: &'static str, dir: &'static Path, module: &str, part: &str) -> CodeMod<'static> {
        CodeMod {
            id,
            dir,
            modules: BTreeMap::from([(module.to_string(), serde_json::from_str(part).unwrap())]),
            writes: Vec::new(),
            messages: BTreeMap::new(),
            messages_with: BTreeMap::new(),
            minigames: Vec::new(),
            sounds: Vec::new(),
        }
    }

    /// main.dol's hooks are the resident module's; it needs the core's main.json.
    #[test]
    fn main_dols_hooks_go_in_the_resident_module() {
        let core = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("core");
        let dir = Path::new("/nowhere");
        let e = check_code(&core, &[code("a", dir, "main", r#"{"objects": [], "hooks": {"Scene_loaded": "A"}}"#)]).err().unwrap();
        assert!(e.message.contains("« resident »") && e.mods == ["a"], "{}", e.message);
        let e = check_code(&core, &[code("a", dir, "main", r#"{"objects": [], "replace": {"f": "g"}}"#)]).err().unwrap();
        assert!(e.message.contains("replace"), "{}", e.message);
        let e = check_code(&core, &[code("a", dir, "resident", r#"{"objects": [], "inject": [{"at": "f", "expect": "0", "call": "h"}]}"#)]).err().unwrap();
        assert!(e.message.contains("« main »"), "{}", e.message);
        if !core.join("main.json").exists() {
            let e = check_code(&core, &[code("a", dir, "resident", r#"{"objects": [], "hooks": {"Scene_loaded": "A"}}"#)]).err().unwrap();
            assert!(e.message.contains("main.json"), "{}", e.message);
        }
    }

    #[test]
    fn texts_for_another_mod_need_it() {
        let dir = Path::new("");
        let code = |id: &'static str, with: &str| CodeMod {
            id,
            dir,
            modules: BTreeMap::new(),
            writes: Vec::new(),
            messages: BTreeMap::new(),
            messages_with: BTreeMap::from([(with.to_string(), BTreeMap::from([("fr_EU".to_string(), BTreeMap::from([("k".to_string(), "t".to_string())]))]))]),
            minigames: Vec::new(),
            sounds: Vec::new(),
        };
        assert!(texts_with(&[code("a", "b")]).is_empty());
        let both = [code("a", "b"), code("b", "c")];
        let with = texts_with(&both);
        assert_eq!(with["fr_EU"].len(), 1);
        assert_eq!(with["fr_EU"][0].0.id, "a");
    }

    #[test]
    fn encodes_instructions() {
        use dol::tests_ppc as p;
        assert_eq!(p::all(), [0x9421FE50, 0x90010008, 0xBC410010, 0x7C0802A6, 0x7C0803A6, 0x7C000026, 0x7C0FF120, 0x7C0902A6, 0x7C0903A6, 0x7C0102A6, 0x7C0103A6, 0xD8010098, 0xC8010098, 0x38000000, 0x2C000000, 0x41820010, 0xB8410010, 0x80010008]);
    }

    /// WP_CORE (the core's export) and WP_CORE_BUILD (the build it was made from, wp-srccore
    /// build/SUPP01): each module of the core, linked with no mod, is the build's REL, byte for byte.
    #[test]
    #[ignore]
    fn makes_the_core_rels_again() {
        let dir = core_dir().expect("WP_CORE");
        let build = std::path::PathBuf::from(std::env::var("WP_CORE_BUILD").expect("WP_CORE_BUILD"));
        let main = rel::MainSymbols::parse(&std::fs::read_to_string(dir.join("main.sym")).unwrap()).unwrap();
        let modules = corepkg::read(&dir).unwrap().modules;
        for module in &modules {
            let plf = elf::Object::parse(&std::fs::read(dir.join(format!("{module}.plf"))).unwrap()).unwrap();
            let info: ModuleInfo = serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{module}.json"))).unwrap()).unwrap();
            let made = rel::write(&module::link(&plf, &info.hooks, &[], &main).unwrap(), &info.params, &main).unwrap();
            let want = std::fs::read(build.join(module).join(format!("{module}.rel"))).unwrap();
            assert_eq!(made.len(), want.len(), "{module}");
            if let Some(at) = made.iter().zip(&want).position(|(a, b)| a != b) {
                panic!("{module}: first difference at {at:#X}");
            }
        }
        assert!(!modules.is_empty());
    }
}
