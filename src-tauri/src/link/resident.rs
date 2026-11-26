//! The mods' resident code (mod.json: a code patch's module "resident"): one module, /wp/wpmods.rel, with
//! every mod's resident objects, that the core loads once at the start and never unloads (wp-srccore
//! src/wp/core/ModCode.cpp: at the end of GameData::initialize, into MEM2, OSLink against main.dol, then
//! its prolog). Its prolog is made here: it runs the mods' constructors and their own prologs, then adds
//! each handler to main.dol's list of its hook (wpAddHandler, in load order), so that every module
//! calling the hook (main.dol, the modes, the minigames) calls it.
//!
//! The module is in MEM2 and main.dol in MEM1, too far apart for a `bl`: each branch to main.dol goes
//! through a veneer of the module (lis/addi r12, mtctr, bctr). A module the launcher links the mods into
//! (mr001, inst, menu) has its own list of a hook a mod names in its part: when resident handlers are on
//! that hook too, the module's list gets a chain (chain()) that calls main.dol's list in their place.

use super::dol::ppc;
use super::elf::{self, Object, Reloc, Section, Symbol};
use super::module::{self, HookDef, ModPart};
use super::rel::{self, MainSymbols};
use super::LinkError;
use std::collections::HashMap;

/// The module key of mod.json's code patch.
pub const MODULE: &str = "resident";
/// The disc file the core loads (ModCode.cpp kPath), uncompressed.
pub const FILE: &str = "/wp/wpmods.rel";
/// Its module id: none of the game's (1..), none of the mods' new RELs (0x100 + n).
pub const MODULE_ID: u32 = 0xF000;
/// main.dol's lists have room for this many handlers (wp/mod/hook.h WP_HOOK_SLOTS).
pub const SLOTS: usize = 8;

/// What the core says about main.dol (wp-srccore tools/core/export.py, main.json): every hook, each with
/// its list in main.dol, and the modules that call it.
#[derive(serde::Deserialize)]
pub struct MainInfo {
    pub hooks: Vec<HookDef>,
}

/// The resident module, and the hooks its handlers are on: (hook, mod id), in load order.
pub struct Linked {
    pub rel: Vec<u8>,
    pub users: Vec<(String, String)>,
}

/// Generated code: its words and its relocations (offset, kind, symbol, addend).
struct Code {
    words: Vec<u32>,
    relocs: Vec<(u32, u8, usize, i32)>,
}

impl Code {
    fn new() -> Code {
        Code { words: Vec::new(), relocs: Vec::new() }
    }

    fn e(&mut self, w: u32) {
        self.words.push(w);
    }

    fn at(&self) -> u32 {
        4 * self.words.len() as u32
    }

    /// `rt` = the address of `symbol` + `addend` (lis, addi).
    fn address(&mut self, rt: u32, symbol: usize, addend: i32) {
        self.relocs.push((self.at() + 2, elf::R_PPC_ADDR16_HA, symbol, addend));
        self.e(ppc::lis(rt));
        self.relocs.push((self.at() + 2, elf::R_PPC_ADDR16_LO, symbol, addend));
        self.e(ppc::addi(rt, rt, 0));
    }

    /// bl symbol + addend
    fn call(&mut self, symbol: usize, addend: i32) {
        self.relocs.push((self.at(), elf::R_PPC_REL24, symbol, addend));
        self.e(0x4800_0001);
    }

    /// Appends the code to `obj`'s section `section` (4-aligned); returns where it starts.
    fn put(self, obj: &mut Object, section: usize) -> u32 {
        let s = &mut obj.sections[section];
        let start = (s.size + 3) & !3;
        s.data.resize(start as usize, 0);
        s.data.extend(self.words.iter().flat_map(|w| w.to_be_bytes()));
        s.size = s.data.len() as u32;
        for (offset, kind, symbol, addend) in self.relocs {
            s.relocs.push(Reloc { offset: start + offset, kind, symbol, addend });
        }
        start
    }
}

/// The index of the undefined symbol `name` of `obj` (made if needed).
fn undefined(obj: &mut Object, name: &str) -> usize {
    if let Some(i) = obj.symbols.iter().position(|s| s.name == name && s.section == elf::SHN_UNDEF) {
        return i;
    }
    obj.symbols.push(Symbol { name: name.into(), value: 0, size: 0, bind: elf::STB_GLOBAL, kind: 0, section: elf::SHN_UNDEF });
    obj.symbols.len() - 1
}

fn define(obj: &mut Object, name: &str, bind: u8, kind: u8, section: usize, value: u32, size: u32) -> usize {
    obj.symbols.push(Symbol { name: name.into(), value, size, bind, kind, section: section as u16 });
    obj.symbols.len() - 1
}

fn section(obj: &Object, name: &str) -> usize {
    obj.section_index(name).expect("section of the empty module")
}

/// Links the mods' resident parts (in load order) into /wp/wpmods.rel. `hooks`: main.json's.
pub fn link(parts: &[ModPart], hooks: &[HookDef], main: &MainSymbols) -> Result<Linked, LinkError> {
    // a mod's own _prolog (_epilog, _unresolved) is renamed: the module's prolog calls it
    let mut prologs = Vec::new();
    let mut renamed = Vec::new();
    for (k, part) in parts.iter().enumerate() {
        let mut objects = part.objects.clone();
        let mut own = None;
        for s in objects.iter_mut().flat_map(|o| o.symbols.iter_mut()).filter(|s| s.is_global() && s.is_defined()) {
            if ["_prolog", "_epilog", "_unresolved"].contains(&s.name.as_str()) {
                s.name = format!("__wpmods{}_{k}", s.name);
                if s.name.starts_with("__wpmods_prolog") {
                    own = Some(s.name.clone());
                }
            }
        }
        prologs.push(own);
        renamed.push(ModPart { id: part.id.clone(), objects, writes: part.writes.clone(), ..Default::default() });
    }
    let mut out = module::link(&module::empty_module(), &[], &renamed, main)?;

    // the handlers: hooks of main.json, the exclusive ones of one mod, SLOTS at most on a list
    let mut users: Vec<(String, String)> = Vec::new();
    for part in parts {
        let fail = |m: String| LinkError::new(m, &[&part.id]);
        if !part.replaces.is_empty() {
            return Err(fail("« replace » n'est permis que dans un module du cœur (mr001, inst, menu), pas dans « resident »".into()));
        }
        for (hook, handler) in &part.hooks {
            let Some(def) = hooks.iter().find(|d| &d.name == hook) else {
                return Err(fail(format!("le point d'accroche « {hook} » n'existe pas dans cette version du cœur (launcher à mettre à jour ?)")));
            };
            if def.modules.is_empty() {
                return Err(fail(format!("le point d'accroche « {hook} » n'est appelé nulle part")));
            }
            if main.get(&module::hook_list_symbol(hook)).is_none() {
                return Err(fail(format!("main.dol n'a pas de liste pour le point d'accroche « {hook} »")));
            }
            let Some(index) = out.global(handler) else {
                return Err(fail(format!("« {handler} » (point d'accroche {hook}) est introuvable dans le code du mod")));
            };
            if out.sections[out.symbols[index].section as usize].flags & elf::SHF_EXECINSTR == 0 {
                return Err(fail(format!("« {handler} » (point d'accroche {hook}) n'est pas une fonction")));
            }
            let mut on: Vec<&str> = users.iter().filter(|(h, _)| h == hook).map(|(_, id)| id.as_str()).collect();
            if def.exclusive && on.iter().any(|id| *id != part.id) {
                on.push(&part.id);
                return Err(LinkError::new(format!("« {hook} » ne peut être modifié que par un mod à la fois ({})", def.doc), &on));
            }
            if on.len() >= SLOTS {
                on.push(&part.id);
                return Err(LinkError::new(format!("« {hook} » : plus de {SLOTS} gestionnaires résidents, la liste de main.dol n'a que {SLOTS} places"), &on));
            }
            users.push((hook.clone(), part.id.clone()));
        }
    }

    // the prolog: constructors, then each mod's prolog and handlers
    let text = section(&out, ".text");
    let rodata = section(&out, ".rodata");
    let mut code = Code::new();
    code.e(ppc::stwu(1, 1, -16));
    code.e(ppc::mfspr(0, ppc::LR));
    code.e(ppc::stw(0, 1, 20));
    let ctors = section(&out, ".ctors");
    let mut entries: Vec<Reloc> = out.sections[ctors].relocs.clone();
    entries.sort_by_key(|r| r.offset);
    for r in entries.iter().filter(|r| r.kind == elf::R_PPC_ADDR32) {
        code.call(r.symbol, r.addend);
    }
    let add = undefined(&mut out, "wpAddHandler");
    let report = main.get("OSReport").map(|_| undefined(&mut out, "OSReport"));
    for (k, part) in parts.iter().enumerate() {
        if let Some(name) = &prologs[k] {
            let own = out.global(name).expect("renamed prolog");
            code.call(own, 0);
        }
        for (hook, handler) in &part.hooks {
            let list = undefined(&mut out, &module::hook_list_symbol(hook));
            let function = out.global(handler).expect("checked above");
            code.address(3, list, 0);
            code.address(4, function, 0);
            code.call(add, 0);
            if let Some(report) = report {
                // wpAddHandler false: the list is full (handlers added at run time) or has it
                let message = format!("[wpmods] {}: {hook} handler {handler} not added (list full, or added already)\n\0", part.id);
                let r = &mut out.sections[rodata];
                let at = r.size;
                r.data.extend_from_slice(message.as_bytes());
                r.size = r.data.len() as u32;
                let text_symbol = define(&mut out, &format!("__wpmods_message_{k}_{hook}"), elf::STB_LOCAL, 1, rodata, at, message.len() as u32);
                code.e(ppc::TEST_BOOL);
                code.e(ppc::bne(20));
                code.address(3, text_symbol, 0);
                code.e(ppc::CRCLR6);
                code.call(report, 0);
            }
        }
    }
    code.e(ppc::lwz(0, 1, 20));
    code.e(ppc::mtspr(ppc::LR, 0));
    code.e(ppc::addi(1, 1, 16));
    code.e(ppc::BLR);
    let size = code.at();
    let start = code.put(&mut out, text);
    define(&mut out, "_prolog", elf::STB_GLOBAL, elf::STT_FUNC, text, start, size);

    veneers(&mut out).map_err(|m| LinkError::new(m, &parts.iter().map(|p| p.id.as_str()).collect::<Vec<_>>()))?;
    let params = rel::RelParams {
        module_id: MODULE_ID,
        num_sections: out.sections.len() as u32,
        name_offset: 0,
        name_size: 0,
        version: 3,
        align: 32,
        bss_align: 32,
        exec: vec![],
    };
    let bytes = rel::write(&out, &params, main).map_err(|e| LinkError::new(format!("{FILE} : {e}"), &parts.iter().map(|p| p.id.as_str()).collect::<Vec<_>>()))?;
    Ok(Linked { rel: bytes, users })
}

/// Each branch to main.dol (an undefined symbol: OSLink resolves it) goes through a veneer at the end of
/// .text, one per target: `bl` reaches ±32 MB, and the module is in MEM2.
fn veneers(out: &mut Object) -> Result<(), String> {
    let text = section(out, ".text");
    let mut made: HashMap<(String, i32), usize> = HashMap::new();
    let mut todo = Vec::new();
    for (s, sec) in out.sections.iter().enumerate() {
        for (r, rel) in sec.relocs.iter().enumerate() {
            let target = &out.symbols[rel.symbol];
            if target.is_defined() || target.section == elf::SHN_ABS {
                continue;
            }
            match rel.kind {
                elf::R_PPC_REL24 => todo.push((s, r, target.name.clone(), rel.symbol, rel.addend)),
                elf::R_PPC_REL14 => return Err(format!("branchement conditionnel vers main.dol ({}) : trop loin pour le code résident", target.name)),
                _ => {}
            }
        }
    }
    for (s, r, name, symbol, addend) in todo {
        let veneer = match made.get(&(name.clone(), addend)) {
            Some(&v) => v,
            None => {
                let mut code = Code::new();
                code.address(12, symbol, addend);
                code.e(ppc::mtspr(ppc::CTR, 12));
                code.e(ppc::BCTR);
                let start = code.put(out, text);
                let v = define(out, &format!("__wpmods_veneer_{name}_{addend}"), elf::STB_LOCAL, elf::STT_FUNC, text, start, 16);
                made.insert((name, addend), v);
                v
            }
        };
        let rel = &mut out.sections[s].relocs[r];
        rel.symbol = veneer;
        rel.addend = 0;
    }
    Ok(())
}

/// The symbol of a hook's chain in a module (chain()).
pub fn chain_symbol(hook: &str) -> String {
    format!("wp_chain_{hook}")
}

/// An object for a module that has its own list of `hook` (a mod names it in its part) while resident
/// handlers are on main.dol's: `wp_chain_<hook>`, a handler of the module's list that calls main.dol's
/// list with the same arguments (r3..r10, f1..f8): every handler for a void hook, for a bool one until
/// the first that returns true (what it returns).
pub fn chain(hook: &HookDef, main: &MainSymbols) -> Result<Object, String> {
    let list = main.get(&module::hook_list_symbol(&hook.name)).ok_or_else(|| format!("main.dol n'a pas de liste pour le point d'accroche « {} »", hook.name))?.0;
    let mut obj = Object::default();
    obj.sections.push(Section { name: String::new(), kind: 0, flags: 0, align: 1, data: vec![], size: 0, relocs: vec![] });
    obj.sections.push(Section { name: ".text".into(), kind: elf::SHT_PROGBITS, flags: elf::SHF_ALLOC | elf::SHF_EXECINSTR, align: 4, data: vec![], size: 0, relocs: vec![] });
    obj.symbols.push(Symbol { name: String::new(), value: 0, size: 0, bind: 0, kind: 0, section: 0 });
    // main.dol's list, by its address: the module's own list has its name
    let target = define(&mut obj, &format!("wp_main_{}", module::hook_list_symbol(&hook.name)), elf::STB_LOCAL, 1, 0, list, 0);
    obj.symbols[target].section = elf::SHN_ABS;
    let boolean = hook.kind == "bool";
    const FRAME: i16 = 0x70;
    let mut c = Code::new();
    c.e(ppc::stwu(1, 1, -FRAME));
    c.e(ppc::mfspr(0, ppc::LR));
    c.e(ppc::stw(0, 1, FRAME + 4));
    c.e(ppc::stw(31, 1, FRAME - 4));
    let args = |c: &mut Code, load: bool| {
        for i in 0..8u32 {
            let d = 8 + 4 * i as i16;
            c.e(if load { ppc::lwz(3 + i, 1, d) } else { ppc::stw(3 + i, 1, d) });
        }
        for i in 0..8u32 {
            let d = 0x28 + 8 * i as i16;
            c.e(if load { ppc::lfd(1 + i, 1, d) } else { ppc::stfd(1 + i, 1, d) });
        }
    };
    args(&mut c, false);
    c.address(31, target, 0);
    let top = c.words.len();
    c.e(ppc::lwz(12, 31, 0));
    c.e(ppc::cmpwi0(12));
    let to_end = c.words.len();
    c.e(0); // beq end
    c.e(ppc::mtspr(ppc::CTR, 12));
    args(&mut c, true);
    c.e(ppc::BCTRL);
    let to_out = c.words.len();
    if boolean {
        c.e(ppc::TEST_BOOL);
        c.e(0); // bne out (r3: what it returned)
    }
    c.e(ppc::addi(31, 31, 4));
    let back = (top as i32 - c.words.len() as i32) * 4;
    c.e(ppc::b_rel(back));
    let end = c.words.len();
    if boolean {
        c.e(ppc::addi(3, 0, 0));
    }
    let out = c.words.len();
    c.e(ppc::lwz(31, 1, FRAME - 4));
    c.e(ppc::lwz(0, 1, FRAME + 4));
    c.e(ppc::mtspr(ppc::LR, 0));
    c.e(ppc::addi(1, 1, FRAME));
    c.e(ppc::BLR);
    c.words[to_end] = ppc::beq(((end - to_end) * 4) as i16);
    if boolean {
        c.words[to_out + 1] = ppc::bne(((out - to_out - 1) * 4) as i16);
    }
    let size = c.at();
    c.put(&mut obj, 1);
    define(&mut obj, &chain_symbol(&hook.name), elf::STB_GLOBAL, elf::STT_FUNC, 1, 0, size);
    Ok(obj)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// An object with a function `name` in .text (blr), calling `calls` (bl, R_PPC_REL24) first.
    pub fn function(name: &str, calls: &[&str]) -> Object {
        let mut obj = Object::default();
        obj.sections.push(Section { name: String::new(), kind: 0, flags: 0, align: 1, data: vec![], size: 0, relocs: vec![] });
        obj.symbols.push(Symbol { name: String::new(), value: 0, size: 0, bind: 0, kind: 0, section: 0 });
        let mut text = Section { name: ".text".into(), kind: elf::SHT_PROGBITS, flags: elf::SHF_ALLOC | elf::SHF_EXECINSTR, align: 4, data: vec![], size: 0, relocs: vec![] };
        for (k, callee) in calls.iter().enumerate() {
            obj.symbols.push(Symbol { name: callee.to_string(), value: 0, size: 0, bind: elf::STB_GLOBAL, kind: 0, section: 0 });
            text.relocs.push(Reloc { offset: 4 * k as u32, kind: elf::R_PPC_REL24, symbol: obj.symbols.len() - 1, addend: 0 });
            text.data.extend(0x4800_0001u32.to_be_bytes());
        }
        text.data.extend(ppc::BLR.to_be_bytes());
        text.size = text.data.len() as u32;
        obj.symbols.push(Symbol { name: name.into(), value: 0, size: text.size, bind: elf::STB_GLOBAL, kind: elf::STT_FUNC, section: 1 });
        obj.sections.push(text);
        obj
    }

    /// A REL's relocations: (module, section, offset, type, target section, addend), in its order.
    pub fn relocations(rel: &[u8]) -> Vec<(u32, u8, u32, u8, u8, u32)> {
        let word = |o: usize| u32::from_be_bytes(rel[o..o + 4].try_into().unwrap());
        let (imp, size) = (word(0x28) as usize, word(0x2C) as usize);
        let mut out = Vec::new();
        for k in 0..size / 8 {
            let (module, mut at) = (word(imp + 8 * k), word(imp + 8 * k + 4) as usize);
            let (mut section, mut offset) = (0u8, 0u32);
            loop {
                let delta = u16::from_be_bytes([rel[at], rel[at + 1]]) as u32;
                let (kind, target, addend) = (rel[at + 2], rel[at + 3], word(at + 4));
                at += 8;
                offset += delta;
                match kind {
                    203 => break,
                    202 => (section, offset) = (target, 0),
                    201 => {}
                    _ => out.push((module, section, offset, kind, target, addend)),
                }
            }
        }
        out
    }

    pub fn main_symbols() -> MainSymbols {
        MainSymbols::parse("801DE610 4 wpAddHandler\n80139610 4 OSReport\n802F6CDC 9 wph_Scene_loaded\n802F6DB4 9 wph_Minigame_start\n802F7000 9 wph_Game_start\n802F7100 9 wph_Island_x\n").unwrap()
    }

    pub fn defs() -> Vec<HookDef> {
        let def = |name: &str, kind: &str, exclusive: bool, modules: &[&str]| HookDef { name: name.into(), kind: kind.into(), exclusive, doc: "doc".into(), modules: modules.iter().map(|m| m.to_string()).collect() };
        vec![def("Scene_loaded", "void", false, &["main"]), def("Minigame_start", "void", false, &["mg100"]), def("Game_start", "void", false, &["mr001"]), def("Island_x", "bool", true, &["mr001"])]
    }

    fn part(id: &str, hooks: &[(&str, &str)], calls: &[&str]) -> ModPart {
        ModPart {
            id: id.into(),
            objects: hooks.iter().map(|(_, h)| function(h, calls)).collect(),
            hooks: hooks.iter().map(|(k, h)| (k.to_string(), h.to_string())).collect(),
            ..Default::default()
        }
    }

    /// Two mods on Scene_loaded: the prolog adds the first one's handler, then the second's, to main.dol's
    /// list; every branch to main.dol goes through a veneer.
    #[test]
    fn the_prolog_adds_the_handlers_in_load_order() {
        let main = main_symbols();
        let a = part("a", &[("Scene_loaded", "A_loaded"), ("Minigame_start", "A_start")], &["OSReport"]);
        let b = part("b", &[("Scene_loaded", "B_loaded")], &[]);
        let linked = link(&[a, b], &defs(), &main).unwrap();
        assert_eq!(linked.users, [("Scene_loaded".into(), "a".into()), ("Minigame_start".into(), "a".into()), ("Scene_loaded".into(), "b".into())]);
        let rel = &linked.rel;
        let word = |o: usize| u32::from_be_bytes(rel[o..o + 4].try_into().unwrap());
        assert_eq!(word(0), MODULE_ID);
        let text = 2u8; // .init, .text…
        assert_eq!((rel[0x30], word(0x34)), (text, word(0x34)));
        let relocs = relocations(rel);
        // no REL24 to main.dol left: veneers
        assert!(!relocs.iter().any(|r| r.0 == 0 && r.3 == elf::R_PPC_REL24), "{relocs:?}");
        let to_main = |addend: u32| relocs.iter().filter(|r| r.0 == 0 && r.5 == addend && r.3 == elf::R_PPC_ADDR16_HA).count();
        assert_eq!(to_main(0x802F_6CDC), 2); // wph_Scene_loaded, for a and b
        assert_eq!(to_main(0x802F_6DB4), 1);
        assert_eq!(to_main(0x801D_E610), 1); // one veneer for every call of wpAddHandler
        assert_eq!(to_main(0x8013_9610), 1); // and one for OSReport
        // the handlers, in the order the prolog loads them (r4): A_loaded (at 0 of .text: a's first
        // object), A_start (after a's bl OSReport and blr), B_loaded
        let prolog = word(0x34);
        let handlers: Vec<u32> = relocs.iter().filter(|r| r.0 == MODULE_ID && r.1 == text && r.2 > prolog && r.3 == elf::R_PPC_ADDR16_LO && r.4 == text && r.5 < 0x20).map(|r| r.5).collect();
        assert_eq!(handlers, [0, 8, 16]);
    }

    #[test]
    fn checks_the_hooks_of_main() {
        let main = main_symbols();
        let one = |id: &str, hook: &str| part(id, &[(hook, &format!("{id}_{hook}"))], &[]);
        let e = link(&[one("a", "Island_x"), one("b", "Island_x")], &defs(), &main).err().unwrap();
        assert_eq!(e.mods, ["a", "b"]);
        assert!(e.message.contains("un mod à la fois"), "{}", e.message);
        assert!(link(&[one("a", "Nope")], &defs(), &main).err().unwrap().message.contains("n'existe pas"));
        let missing = ModPart { objects: vec![], ..one("a", "Scene_loaded") };
        assert!(link(&[missing], &defs(), &main).err().unwrap().message.contains("introuvable"));
        // main.dol's list has SLOTS places
        let ids: Vec<String> = (0..=SLOTS).map(|k| format!("m{k}")).collect();
        let full: Vec<ModPart> = ids.iter().map(|id| one(id, "Scene_loaded")).collect();
        assert!(link(&full[..SLOTS], &defs(), &main).is_ok());
        let e = link(&full, &defs(), &main).err().unwrap();
        assert!(e.message.contains("8 places") && e.mods.len() == SLOTS + 1, "{}", e.message);
        let replace = ModPart { replaces: vec![("f".into(), "g".into())], ..one("a", "Scene_loaded") };
        assert!(link(&[replace], &defs(), &main).err().unwrap().message.contains("replace"));
    }

    /// A module with its own list of Game_start (a mod's part names it) calls main.dol's through the
    /// chain: an entry of its list, its relocations to main.dol's list by address.
    #[test]
    fn a_modules_own_list_calls_main_dols() {
        let main = main_symbols();
        // the core's caller of the hook: lis of wph_Game_start
        let caller = {
            let mut o = function("Board_go", &[]);
            o.symbols.push(Symbol { name: "wph_Game_start".into(), value: 0, size: 0, bind: elf::STB_GLOBAL, kind: 0, section: 0 });
            let s = o.symbols.len() - 1;
            o.sections[1].relocs.push(Reloc { offset: 2, kind: elf::R_PPC_ADDR16_HA, symbol: s, addend: 0 });
            o
        };
        let core = module::link(&module::empty_module(), &[], &[ModPart { id: "cœur".into(), objects: vec![caller], ..Default::default() }], &main).unwrap();
        let defs = defs();
        let game_start = defs.iter().find(|d| d.name == "Game_start").unwrap();
        let parts = [part("lava", &[("Game_start", "Lava_start")], &[]), ModPart { id: "r".into(), objects: vec![chain(game_start, &main).unwrap()], hooks: vec![("Game_start".into(), chain_symbol("Game_start"))], ..Default::default() }];
        let linked = module::link(&core, &defs, &parts, &main).unwrap();
        let list = linked.global("wph_Game_start").expect("the module's own list");
        assert_eq!(linked.symbols[list].size, 12); // Lava_start, the chain, 0
        let params = rel::RelParams { module_id: 1, num_sections: linked.sections.len() as u32, name_offset: 0, name_size: 0, version: 3, align: 32, bss_align: 32, exec: vec![] };
        let relocs = relocations(&rel::write(&linked, &params, &main).unwrap());
        // the chain reads main.dol's list; the core's caller reads the module's
        assert_eq!(relocs.iter().filter(|r| r.0 == 0 && r.5 == 0x802F_7000).count(), 2);
        assert!(relocs.iter().any(|r| r.0 == 1 && r.3 == elf::R_PPC_ADDR16_HA && r.2 == 2));
        // a bool hook's chain stops at the first true
        let bool_chain = chain(defs.iter().find(|d| d.name == "Island_x").unwrap(), &main).unwrap();
        assert!(bool_chain.sections[1].data.chunks(4).any(|w| w == ppc::TEST_BOOL.to_be_bytes()));
    }
}
