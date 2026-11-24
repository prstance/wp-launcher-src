//! PartyLauncher's linker: it puts the mods' code into the game (wp-core docs/modding.md).

pub mod dol;
pub mod elf;
pub mod minigames;
pub mod module;
pub mod rel;
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
    /// Core modules: hook -> handler.
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

/// What the core says about one of its modules (wp-core tools/wpcore_pack.py, <module>.json).
#[derive(Deserialize)]
struct ModuleInfo {
    #[serde(flatten)]
    params: rel::RelParams,
    #[serde(default)]
    hooks: Vec<module::HookDef>,
    /// Bytes of the board's save the mods share (0: none).
    #[serde(default)]
    save_bytes: u32,
}

/// The files and memory patches that put the mods' code into the game, as a Riivolution XML.
#[allow(dead_code)] // (examples/wplink.rs)
pub struct Linked {
    pub xml: PathBuf,
    /// The module files made (disc path), for the compatibility check.
    pub files: Vec<String>,
}

/// The core's modules (.plf) the launcher can link into: those with a <module>.json.
pub fn core_modules(core: &Path) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(core)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".plf").map(String::from))
        .filter(|m| core.join(format!("{m}.json")).is_file())
        .collect();
    found.sort();
    found
}

fn read_object(dir: &Path, path: &str, id: &str) -> Result<elf::Object, LinkError> {
    let bytes = std::fs::read(dir.join(path)).map_err(|_| LinkError::new(format!("{path} est introuvable"), &[id]))?;
    elf::Object::parse(&bytes).map_err(|e| LinkError::new(format!("{path} : {e}"), &[id]))
}

fn hexs(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

/// Links the mods' code (in load order) with the core into `out`: the core's modules they change (REL,
/// LZ-compressed as on the disc), main.dol's memory patches and the message files with the mods' texts
/// (from the disc's: `disc` reads one), all in `out/linked.xml` (section "PartyLauncher", option "Code",
/// choice 1). None if no mod has code or texts.
pub fn link_game(core: &Path, disc: &dyn Fn(&str) -> Result<Vec<u8>, String>, mods: &[CodeMod], out: &Path) -> Result<Option<Linked>, LinkError> {
    if mods.iter().all(|m| !m.has_code() && m.messages.is_empty()) {
        return Ok(None);
    }
    let (lines, made) = build(core, Some(disc), mods)?;
    let io = |e: std::io::Error| LinkError::new(format!("{} : {e}", out.display()), &[]);
    let _ = std::fs::remove_dir_all(out);
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
    Ok(Some(Linked { xml: path, files }))
}

/// Whether the mods' code links (the texts aside): what link_game would say, without writing anything.
pub fn check_code(core: &Path, mods: &[CodeMod]) -> Result<(), LinkError> {
    if mods.iter().all(|m| !m.has_code()) {
        return Ok(());
    }
    build(core, None, mods).map(|_| ())
}

/// The XML's patch lines and the files they name (disc path, contents). Without `disc`, no texts.
type Built = (Vec<String>, Vec<(String, Vec<u8>)>);

fn build(core: &Path, disc: Option<&dyn Fn(&str) -> Result<Vec<u8>, String>>, mods: &[CodeMod]) -> Result<Built, LinkError> {
    let all: Vec<&str> = mods.iter().map(|m| m.id).collect();
    let io = |e: std::io::Error| LinkError::new(format!("cœur des mods : {e}"), &[]);
    let mut main = rel::MainSymbols::parse(&std::fs::read_to_string(core.join("main.sym")).map_err(io)?).map_err(|e| LinkError::new(e, &[]))?;
    let mut lines = Vec::new();
    let mut made = Vec::new();

    // the mods' minigames: main.dol's longer table, at the start of the mods' memory
    let added: minigames::Added = mods.iter().filter(|m| !m.minigames.is_empty()).map(|m| (m.id, &m.minigames[..])).collect();
    let mut table = minigames::Table { data: vec![], count: 0, patches: vec![] };
    let mut registry = None;
    if !added.is_empty() {
        let ids: Vec<&str> = added.iter().map(|a| a.0).collect();
        let text = std::fs::read_to_string(core.join("main.json")).map_err(|_| LinkError::new("ce launcher ne sait pas encore ajouter de mini-jeux (cœur sans main.json)".into(), &ids))?;
        #[derive(Deserialize)]
        struct MainInfo {
            minigames: minigames::Registry,
        }
        let info: MainInfo = serde_json::from_str(&text).map_err(|e| LinkError::new(format!("main.json : {e}"), &[]))?;
        table = minigames::table(&info.minigames, &added, dol::BASE)?;
        if dol::BASE + table.data.len() as u32 > dol::END {
            let over = dol::BASE + table.data.len() as u32 - dol::END;
            return Err(LinkError::new(format!("trop de mini-jeux ajoutés : leur table dépasse la place libre ({over} octets de trop)"), &ids));
        }
        registry = Some(info.minigames);
    }

    // main.dol
    let mut parts = Vec::new();
    for m in mods {
        let Some(part) = m.modules.get("main") else { continue };
        let fail = |msg: String| LinkError::new(msg, &[m.id]);
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
    // more minigames than the save's arrays hold: the core's code keeps the others (minigames.rs)
    if let Some(extra) = registry.as_ref().filter(|r| table.count > r.max).and_then(|r| r.extra.as_ref()) {
        let fail = |msg: String| LinkError::new(format!("cœur des mods : {msg}"), &[]);
        let object = elf::Object::parse(&std::fs::read(core.join(&extra.object)).map_err(io)?).map_err(|e| fail(format!("{} : {e}", extra.object)))?;
        let mut replaces = Vec::new();
        for r in &extra.replace {
            let expect = hex(&r.expect).ok_or_else(|| fail(format!("instruction « {} » invalide", r.expect)))?;
            let at = main.get(&r.at).ok_or_else(|| fail(format!("« {} » n'est pas dans main.dol", r.at)))?.0;
            // a function that starts with the table's address: the original runs the patched instruction
            let run = match table.patches.iter().position(|p| p.0 == at) {
                Some(i) => table.patches.remove(i).1,
                None => expect,
            };
            replaces.push(dol::Replace { symbol: r.at.clone(), expect, run, handler: r.call.clone(), original: r.original.clone() });
        }
        let writes = vec![(extra.table.clone(), 0, dol::BASE.to_be_bytes().to_vec()), (extra.count.clone(), 0, table.count.to_be_bytes().to_vec())];
        parts.push(dol::DolPart { id: "cœur".into(), objects: vec![object], replaces, writes, ..Default::default() });
    }
    let spare = registry.as_ref().and_then(minigames::reuse).map(|(address, original)| dol::Spare { address, original });
    let (patches, symbols) = dol::link(&parts, &main, &table.data, spare.as_ref())?;
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

    for (address, value, original) in &table.patches {
        lines.push(format!(r#"<memory offset="0x{address:08X}" value="{value:08X}" original="{original:08X}"/>"#));
    }

    // the core's modules
    let cores = core_modules(core);
    for module in &cores {
        let module = module.clone();
        let users: Vec<&CodeMod> = mods.iter().filter(|m| m.modules.contains_key(&module)).collect();
        if users.is_empty() && added.is_empty() {
            continue;
        }
        let plf = elf::Object::parse(&std::fs::read(core.join(format!("{module}.plf"))).map_err(io)?).map_err(|e| LinkError::new(format!("{module}.plf : {e}"), &[]))?;
        // a module that lists the minigames itself (wp/mod/minigames.h) gets the mods' ones
        let lists_minigames = plf.symbols.iter().any(|s| !s.is_defined() && s.name == "wpMinigames");
        if users.is_empty() && !lists_minigames {
            continue;
        }
        let info: ModuleInfo = serde_json::from_str(&std::fs::read_to_string(core.join(format!("{module}.json"))).map_err(io)?)
            .map_err(|e| LinkError::new(format!("{module}.json : {e}"), &[]))?;
        let mut parts = Vec::new();
        if lists_minigames {
            parts.push(module::ModPart { id: "cœur".into(), objects: vec![module::data_object("wpMinigames", minigames::module_list(&added))], ..Default::default() });
        }
        let mut save_at = 0u32;
        let mut layout = 0x811C_9DC5u32; // FNV-1a of the mods' save layout: a save of another layout is ignored
        for m in &users {
            let part = &m.modules[&module];
            let objects = part.objects.iter().map(|o| read_object(m.dir, o, m.id)).collect::<Result<_, _>>()?;
            let mut writes: Vec<(String, u32, Vec<u8>)> = m.writes.iter().filter(|w| w.0 == module).map(|w| (w.1.clone(), w.2, w.3.clone())).collect();
            if let Some(save) = &part.save {
                if save_at + save.size > info.save_bytes {
                    let ids: Vec<&str> = users.iter().filter(|u| u.modules[&module].save.is_some()).map(|u| u.id).collect();
                    return Err(LinkError::new(format!("plus assez de place dans la sauvegarde du plateau ({} octets en tout)", info.save_bytes), &ids));
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
        if info.save_bytes > 0 && plf.global("gWpSaveLayout").is_some() {
            parts.push(module::ModPart { id: "cœur".into(), writes: vec![("gWpSaveLayout".into(), 0, layout.to_be_bytes().to_vec())], ..Default::default() });
        }
        let linked = module::link(&plf, &info.hooks, &parts)?;
        let bytes = rel::write(&linked, &info.params, &main).map_err(|e| LinkError::new(format!("{module} : {e}"), &all))?;
        if disc.is_some() {
            let name = format!("rel/{module}.rel.lz");
            lines.push(format!(r#"<file disc="/{name}" external="{name}"/>"#));
            made.push((format!("/{name}"), crate::lz::compress(&bytes)));
        }
    }

    // the mods' new modules (their minigames): each one of one mod, made from its objects alone
    let mut new: Vec<(&String, &CodeMod)> = Vec::new();
    for m in mods {
        for name in m.modules.keys().filter(|n| *n != "main" && !cores.contains(n)) {
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
        let linked = module::link(&module::empty_module(), &[], &[module::ModPart { id: m.id.to_string(), objects, writes, ..Default::default() }])?;
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
    let Some(disc) = disc else { return Ok((lines, made)) };
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
    Ok((lines, made))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// wp-core's export (tools/wpcore_pack.py): WP_CORE=<…/build/SUPP01/wpcore>.
    fn core_dir() -> Option<std::path::PathBuf> {
        std::env::var("WP_CORE").ok().map(Into::into)
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

    /// WP_CORE (the core's export, for the REL header values), WP_PLAIN (the .plf of the matching build,
    /// without the hooks) and WP_DISC_RELS (the disc's RELs, decompressed): each <module>.plf of WP_PLAIN
    /// makes its disc REL again, byte for byte.
    #[test]
    #[ignore]
    fn makes_the_disc_rels_again() {
        let dir = core_dir().expect("WP_CORE");
        let plain = std::path::PathBuf::from(std::env::var("WP_PLAIN").expect("WP_PLAIN"));
        let discs = std::path::PathBuf::from(std::env::var("WP_DISC_RELS").expect("WP_DISC_RELS"));
        let main = rel::MainSymbols::parse(&std::fs::read_to_string(dir.join("main.sym")).unwrap()).unwrap();
        let mut done = 0;
        for module in core_modules(&dir) {
            let Ok(bytes) = std::fs::read(plain.join(format!("{module}.plf"))) else { continue };
            let plf = elf::Object::parse(&bytes).unwrap();
            let params: rel::RelParams = serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{module}.json"))).unwrap()).unwrap();
            let made = rel::write(&plf, &params, &main).unwrap();
            let want = std::fs::read(discs.join(format!("{module}.rel"))).unwrap();
            assert_eq!(made.len(), want.len(), "{module}");
            if let Some(at) = made.iter().zip(&want).position(|(a, b)| a != b) {
                panic!("{module}: first difference at {at:#X}");
            }
            done += 1;
        }
        assert!(done > 0);
    }
}
