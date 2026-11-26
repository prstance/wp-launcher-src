//! Links the mods' objects into a module of the core (mr001.plf): their sections go after the core's,
//! their undefined symbols are found in the core, in the other mods or (later, rel.rs) in main.dol,
//! the core's hooks get their lists of handlers, and the replaced functions jump to their replacement.

use super::elf::{self, Object, Reloc, Section, Symbol};
use super::rel::MainSymbols;
use super::LinkError;
use std::collections::HashMap;

/// A hook of the core (wp-core include/wp/mod/hooks_<module>.h).
#[derive(Clone, serde::Deserialize)]
#[allow(dead_code)] // kind, doc: for the mod authors' tools
pub struct HookDef {
    pub name: String,
    pub kind: String,
    pub exclusive: bool,
    #[serde(default)]
    pub doc: String,
    /// main.json: the modules that call it ("main": main.dol itself).
    #[serde(default)]
    pub modules: Vec<String>,
}

/// What one mod puts into one module.
#[derive(Clone, Default)]
pub struct ModPart {
    pub id: String,
    pub objects: Vec<Object>,
    /// (hook, handler symbol), in the mod's order.
    pub hooks: Vec<(String, String)>,
    /// (function of the core, replacement symbol).
    pub replaces: Vec<(String, String)>,
    /// (symbol, offset, bytes): values written into the mod's data (its settings).
    pub writes: Vec<(String, u32, Vec<u8>)>,
}

/// The symbol that holds a hook's list of handlers (wp/mod/hook.h WP_HOOK).
pub fn hook_list_symbol(hook: &str) -> String {
    format!("wph_{hook}")
}

const LIST_SECTIONS: [&str; 2] = [".ctors", ".dtors"];

fn is_list_terminated(s: &Section, index: usize) -> bool {
    let n = s.data.len();
    n >= 4 && s.data[n - 4..] == [0, 0, 0, 0] && !s.relocs.iter().any(|r| r.offset as usize == n - 4) && index > 0
}

/// Who defines a global symbol of the linked module: the core or a mod.
type Owners = HashMap<String, String>;

struct Linker {
    out: Object,
    owners: Owners,
}

impl Linker {
    /// Appends the object's sections to the module's sections of the same name.
    fn merge(&mut self, obj: &Object, owner: &str) -> Result<(), LinkError> {
        let fail = |message: String| LinkError::new(message, &[owner]);
        // object section -> (module section, offset where it goes)
        let mut placed: HashMap<usize, (usize, u32)> = HashMap::new();
        for (index, s) in obj.sections.iter().enumerate() {
            if !s.is_alloc() || (s.size == 0 && s.relocs.is_empty()) {
                continue;
            }
            let Some(target) = self.out.section_index(&s.name).filter(|_| super::rel::PERMITTED_SECTIONS.contains(&s.name.as_str())) else {
                return Err(fail(format!("section « {} » non prise en charge (options de compilation du cœur : -sdata 0 -sdata2 0, sans exceptions)", s.name)));
            };
            let t = &mut self.out.sections[target];
            if (t.kind == elf::SHT_NOBITS) != (s.kind == elf::SHT_NOBITS) {
                return Err(fail(format!("section « {} » de type inattendu", s.name)));
            }
            let at = if LIST_SECTIONS.contains(&s.name.as_str()) && is_list_terminated(t, target) {
                // before the core's terminating null
                let at = t.size - 4;
                let n = s.size;
                t.data.splice(at as usize..at as usize, s.data.iter().copied());
                t.size += n;
                for r in t.relocs.iter_mut().filter(|r| r.offset >= at) {
                    r.offset += n;
                }
                for sym in self.out.symbols.iter_mut().filter(|y| y.section as usize == target && y.value >= at) {
                    sym.value += n;
                }
                at
            } else {
                let align = s.align.max(1);
                let at = (t.size + align - 1) & !(align - 1);
                if t.kind != elf::SHT_NOBITS {
                    t.data.resize(at as usize, 0);
                    t.data.extend_from_slice(&s.data);
                }
                t.size = at + s.size;
                t.align = t.align.max(align);
                at
            };
            placed.insert(index, (target, at));
        }

        // symbols
        let mut map = vec![0usize; obj.symbols.len()];
        for (index, sym) in obj.symbols.iter().enumerate().skip(1) {
            let mut new = sym.clone();
            if sym.is_defined() {
                match placed.get(&(sym.section as usize)) {
                    Some(&(target, at)) => {
                        new.section = target as u16;
                        new.value += at;
                    }
                    // a symbol of a section left out (empty, or not loaded)
                    None => {
                        new.section = elf::SHN_UNDEF;
                        new.bind = elf::STB_LOCAL;
                    }
                }
            }
            if new.is_global() && new.is_defined() {
                if let Some(existing) = self.out.global(&new.name) {
                    let old = &self.out.symbols[existing];
                    let first = self.owners.get(&new.name).cloned().unwrap_or_else(|| "cœur".into());
                    if old.bind == elf::STB_WEAK && new.bind == elf::STB_GLOBAL {
                        // a strong definition wins over a weak one: the references follow the index
                        self.out.symbols[existing] = new;
                        self.owners.insert(sym.name.clone(), owner.to_string());
                    } else if new.bind == elf::STB_GLOBAL && old.bind == elf::STB_GLOBAL {
                        return Err(LinkError::new(
                            format!("« {} » est défini deux fois ({first} et {owner}) : pour remplacer une fonction du jeu, la nommer dans la liste « replace » du module (mod.json)", sym.name),
                            &[owner, first.as_str()],
                        ));
                    }
                    map[index] = existing;
                    continue;
                }
                self.owners.insert(sym.name.clone(), owner.to_string());
            }
            map[index] = self.out.symbols.len();
            self.out.symbols.push(new);
        }

        // relocations
        for (index, s) in obj.sections.iter().enumerate() {
            let Some(&(target, at)) = placed.get(&index) else { continue };
            for r in &s.relocs {
                self.out.sections[target].relocs.push(Reloc { offset: r.offset + at, kind: r.kind, symbol: map[r.symbol], addend: r.addend });
            }
        }
        Ok(())
    }

    /// Points the references to undefined symbols at the definitions of the module, if any.
    fn bind(&mut self) {
        let mut defined: HashMap<&str, usize> = HashMap::new();
        for (i, s) in self.out.symbols.iter().enumerate() {
            if s.is_global() && s.is_defined() {
                defined.entry(s.name.as_str()).or_insert(i);
            }
        }
        let remap: Vec<usize> = (0..self.out.symbols.len())
            .map(|i| {
                let s = &self.out.symbols[i];
                if s.is_defined() { i } else { defined.get(s.name.as_str()).copied().unwrap_or(i) }
            })
            .collect();
        for section in &mut self.out.sections {
            for r in &mut section.relocs {
                r.symbol = remap[r.symbol];
            }
        }
    }

    /// Moves the variable `name` of .bss to the end of .data (zeros: a value can then be written into
    /// it). Its references follow the symbol; one through the section itself (a static's, by its
    /// section and an offset) could not, and is refused. Its room in .bss stays, unused.
    fn to_data(&mut self, name: &str) -> Result<(usize, u32), String> {
        let index = self.out.global(name).ok_or_else(|| format!("« {name} » est introuvable dans le code du mod"))?;
        let (bss, from, size) = {
            let s = &self.out.symbols[index];
            (s.section as usize, s.value, s.size)
        };
        let refuse = || format!("« {name} » est en .bss et lu autrement que par son nom : l'initialiser avec une valeur non nulle (« = 1 »)");
        if size == 0 {
            return Err(refuse());
        }
        for section in &self.out.sections {
            for r in &section.relocs {
                let t = &self.out.symbols[r.symbol];
                let place = t.value as i64 + r.addend as i64;
                // Read through another symbol: inside it by the addend, or from an anchor of no size
                // before it (mwcc's "...bss.0" + the displacement in the instruction, not seen here)
                let inside = (from as i64..(from + size) as i64).contains(&place);
                let anchor = t.size == 0 && place <= from as i64;
                if r.symbol != index && t.is_defined() && t.section as usize == bss && (inside || anchor) {
                    return Err(refuse());
                }
            }
        }
        let data = self.out.section_index(".data").ok_or_else(refuse)?;
        let d = &mut self.out.sections[data];
        let at = (d.size + 7) & !7;
        d.data.resize((at + size) as usize, 0);
        d.size = at + size;
        d.align = d.align.max(8);
        let s = &mut self.out.symbols[index];
        s.section = data as u16;
        s.value = at;
        Ok((data, at))
    }

    fn symbol_at(&self, name: &str) -> Option<(usize, u32)> {
        self.out.global(name).map(|i| (self.out.symbols[i].section as usize, self.out.symbols[i].value))
    }
}

/// A module of the game's sections, empty: what a new REL of a mod (a minigame) is linked into.
pub fn empty_module() -> Object {
    let mut obj = Object::default();
    let section = |name: &str, kind: u32, flags: u32| Section { name: name.into(), kind, flags, align: if name.is_empty() { 1 } else { 4 }, data: vec![], size: 0, relocs: vec![] };
    obj.sections.push(section("", 0, 0));
    let alloc = elf::SHF_ALLOC;
    let exec = elf::SHF_ALLOC | elf::SHF_EXECINSTR;
    for (name, kind, flags) in [
        (".init", elf::SHT_PROGBITS, exec),
        (".text", elf::SHT_PROGBITS, exec),
        (".ctors", elf::SHT_PROGBITS, alloc | 1),
        (".dtors", elf::SHT_PROGBITS, alloc | 1),
        (".rodata", elf::SHT_PROGBITS, alloc),
        (".data", elf::SHT_PROGBITS, alloc | 1),
        (".bss", elf::SHT_NOBITS, alloc | 1),
    ] {
        obj.sections.push(section(name, kind, flags));
    }
    obj.symbols.push(Symbol { name: String::new(), value: 0, size: 0, bind: 0, kind: 0, section: 0 });
    obj
}

/// An object defining `symbol`: `bytes`, in .data (tests).
#[cfg(test)]
pub fn data_object(symbol: &str, bytes: Vec<u8>) -> Object {
    let mut obj = Object::default();
    obj.sections.push(Section { name: String::new(), kind: 0, flags: 0, align: 1, data: vec![], size: 0, relocs: vec![] });
    obj.symbols.push(Symbol { name: String::new(), value: 0, size: 0, bind: 0, kind: 0, section: 0 });
    let size = bytes.len() as u32;
    obj.sections.push(Section { name: ".data".into(), kind: elf::SHT_PROGBITS, flags: elf::SHF_ALLOC | 1, align: 4, data: bytes, size, relocs: vec![] });
    obj.symbols.push(Symbol { name: symbol.into(), value: 0, size, bind: elf::STB_GLOBAL, kind: 1, section: 1 });
    obj
}

/// An object holding each hook's list: the handlers' addresses, then a null.
fn hook_lists(lists: &[(String, Vec<String>)]) -> Object {
    let mut obj = Object::default();
    obj.sections.push(Section { name: String::new(), kind: 0, flags: 0, align: 1, data: vec![], size: 0, relocs: vec![] });
    let mut data = Section { name: ".data".into(), kind: elf::SHT_PROGBITS, flags: elf::SHF_ALLOC | 1, align: 4, data: vec![], size: 0, relocs: vec![] };
    obj.symbols.push(Symbol { name: String::new(), value: 0, size: 0, bind: 0, kind: 0, section: 0 });
    for (hook, handlers) in lists {
        let at = data.data.len() as u32;
        obj.symbols.push(Symbol { name: hook_list_symbol(hook), value: at, size: 4 * (handlers.len() as u32 + 1), bind: elf::STB_GLOBAL, kind: 1, section: 1 });
        for (k, handler) in handlers.iter().enumerate() {
            obj.symbols.push(Symbol { name: handler.clone(), value: 0, size: 0, bind: elf::STB_GLOBAL, kind: 0, section: elf::SHN_UNDEF });
            data.relocs.push(Reloc { offset: at + 4 * k as u32, kind: elf::R_PPC_ADDR32, symbol: obj.symbols.len() - 1, addend: 0 });
        }
        data.data.resize(data.data.len() + 4 * (handlers.len() + 1), 0);
    }
    data.size = data.data.len() as u32;
    obj.sections.push(data);
    obj
}

/// The core's module with the mods' parts linked in. The undefined symbols left are main.dol's: among
/// them the lists of the hooks no mod uses here, when main.dol has them (`main`; wp-srccore
/// src/wp/core/Hooks.cpp), so that with no mod the module is the core's own build.
pub fn link(core: &Object, hooks: &[HookDef], mods: &[ModPart], main: &MainSymbols) -> Result<Object, LinkError> {
    let mut linker = Linker { out: core.clone(), owners: Owners::new() };
    for part in mods {
        for obj in &part.objects {
            linker.merge(obj, &part.id)?;
        }
    }

    // each hook the core calls gets its list, empty if no mod uses it (and so does each hook of the core's
    // shared code that a new module has: no mod hooks into those), unless main.dol has one
    let mut called: Vec<String> = linker
        .out
        .symbols
        .iter()
        .filter(|s| !s.is_defined() && s.name.starts_with("wph_") && linker.out.global(&s.name).is_none())
        .map(|s| s.name["wph_".len()..].to_string())
        .collect();
    called.sort();
    called.dedup();
    let mut lists: Vec<(String, Vec<String>)> = called.iter().map(|h| (h.clone(), vec![])).collect();
    let mut users: HashMap<String, Vec<String>> = HashMap::new();
    for part in mods {
        for (hook, handler) in &part.hooks {
            let Some(def) = hooks.iter().find(|d| &d.name == hook) else {
                return Err(LinkError::new(format!("le point d'accroche « {hook} » n'existe pas dans cette version du cœur (launcher à mettre à jour ?)"), &[&part.id]));
            };
            let Some(list) = lists.iter_mut().find(|(h, _)| h == hook) else {
                return Err(LinkError::new(format!("le point d'accroche « {hook} » n'est appelé nulle part"), &[&part.id]));
            };
            if linker.out.global(handler).is_none() {
                return Err(LinkError::new(format!("« {handler} » (point d'accroche {hook}) est introuvable dans le code du mod"), &[&part.id]));
            }
            let who = users.entry(hook.clone()).or_default();
            if def.exclusive && !who.is_empty() && !who.contains(&part.id) {
                let mut both: Vec<&str> = who.iter().map(String::as_str).collect();
                both.push(&part.id);
                return Err(LinkError::new(format!("« {hook} » ne peut être modifié que par un mod à la fois ({})", def.doc), &both));
            }
            who.push(part.id.clone());
            list.1.push(handler.clone());
        }
    }
    lists.retain(|(hook, handlers)| !handlers.is_empty() || main.get(&hook_list_symbol(hook)).is_none());
    if !lists.is_empty() {
        linker.merge(&hook_lists(&lists), "cœur")?;
    }
    linker.bind();

    // replaced functions: their first instruction jumps to the replacement
    let mut replaced: HashMap<&str, &str> = HashMap::new();
    for part in mods {
        for (target, with) in &part.replaces {
            if let Some(first) = replaced.insert(target, &part.id) {
                return Err(LinkError::new(format!("« {target} » est remplacée par deux mods"), &[first, &part.id]));
            }
            let fail = |m: String| LinkError::new(m, &[&part.id]);
            let (section, at) = linker.symbol_at(target).filter(|(s, _)| *s < core.sections.len()).ok_or_else(|| fail(format!("« {target} » n'est pas une fonction du jeu")))?;
            let handler = linker.out.global(with).ok_or_else(|| fail(format!("« {with} » est introuvable dans le code du mod")))?;
            let s = &mut linker.out.sections[section];
            if s.flags & elf::SHF_EXECINSTR == 0 {
                return Err(fail(format!("« {target} » n'est pas une fonction")));
            }
            s.relocs.retain(|r| r.offset != at);
            s.data[at as usize..at as usize + 4].copy_from_slice(&0x4800_0000u32.to_be_bytes());
            s.relocs.push(Reloc { offset: at, kind: elf::R_PPC_REL24, symbol: handler, addend: 0 });
        }
    }

    // values written into the mods' data (a variable of .bss moves to .data first)
    for part in mods {
        for (name, offset, bytes) in &part.writes {
            let fail = |m: String| LinkError::new(m, &[&part.id]);
            let (mut section, mut at) = linker.symbol_at(name).ok_or_else(|| fail(format!("« {name} » est introuvable dans le code du mod")))?;
            if linker.out.sections[section].kind == elf::SHT_NOBITS {
                (section, at) = linker.to_data(name).map_err(fail)?;
            }
            let s = &mut linker.out.sections[section];
            let at = (at + offset) as usize;
            if at + bytes.len() > s.data.len() {
                return Err(fail(format!("« {name} » est trop petite pour la valeur écrite à {offset}")));
            }
            s.data[at..at + bytes.len()].copy_from_slice(bytes);
        }
    }
    Ok(linker.out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An object with `gZero` (4 bytes of .bss) read by `f` through its name, and `sStatic` (4 bytes after
    /// it) through the section.
    fn with_bss() -> Object {
        let mut obj = super::super::resident::tests::function("f", &[]);
        obj.sections.push(Section { name: ".bss".into(), kind: elf::SHT_NOBITS, flags: elf::SHF_ALLOC | 1, align: 4, data: vec![], size: 8, relocs: vec![] });
        let bss = obj.sections.len() as u16 - 1;
        obj.symbols.push(Symbol { name: ".bss".into(), value: 0, size: 0, bind: elf::STB_LOCAL, kind: 3, section: bss });
        let section = obj.symbols.len() - 1;
        obj.symbols.push(Symbol { name: "gZero".into(), value: 0, size: 4, bind: elf::STB_GLOBAL, kind: 1, section: bss });
        let zero = obj.symbols.len() - 1;
        obj.symbols.push(Symbol { name: "gAlso".into(), value: 4, size: 4, bind: elf::STB_GLOBAL, kind: 1, section: bss });
        obj.sections[1].relocs.push(Reloc { offset: 2, kind: elf::R_PPC_ADDR16_HA, symbol: zero, addend: 0 });
        obj.sections[1].relocs.push(Reloc { offset: 2, kind: elf::R_PPC_ADDR16_LO, symbol: section, addend: 4 });
        obj
    }

    /// A setting's variable left at 0 (in .bss) moves to .data, where its value is written.
    #[test]
    fn writes_a_bss_variable_from_data() {
        let main = MainSymbols::default();
        let part = |name: &str| ModPart { id: "m".into(), objects: vec![with_bss()], writes: vec![(name.into(), 0, vec![0, 0, 0, 14])], ..Default::default() };
        let out = link(&empty_module(), &[], &[part("gZero")], &main).unwrap();
        let s = &out.symbols[out.global("gZero").unwrap()];
        let data = &out.sections[s.section as usize];
        assert_eq!(data.name, ".data");
        assert_eq!(&data.data[s.value as usize..s.value as usize + 4], &[0, 0, 0, 14]);
        // its reference follows it
        let text = out.section_index(".text").unwrap();
        assert!(out.sections[text].relocs.iter().any(|r| r.symbol == out.global("gZero").unwrap()));
        // a variable also read through its section stays where it is: refused
        let e = link(&empty_module(), &[], &[part("gAlso")], &main).err().unwrap();
        assert!(e.message.contains("valeur non nulle"), "{}", e.message);
    }

    /// mwcc reaches a file's statics from the section's start ("...bss.0" + 0) and puts their offset in
    /// the load instruction: a variable after such an anchor is refused, it would be read in .bss.
    #[test]
    fn refuses_a_bss_variable_behind_an_anchor() {
        let main = MainSymbols::default();
        let mut obj = with_bss();
        obj.sections[1].relocs[1].addend = 0;
        let part = ModPart { id: "m".into(), objects: vec![obj], writes: vec![("gAlso".into(), 0, vec![0, 0, 0, 14])], ..Default::default() };
        let e = link(&empty_module(), &[], &[part], &main).err().unwrap();
        assert!(e.message.contains("valeur non nulle"), "{}", e.message);
    }
}
