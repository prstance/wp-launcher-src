//! Writes a REL from a relocatable module and main.dol's symbols, as `dtk rel make` does (decomp-toolkit
//! 1.8.3, src/util/rel.rs `write_rel`): with the decomp's mr001.plf, the result is the disc's mr001.rel,
//! byte for byte.

use super::elf::{self, Object};
use std::collections::HashMap;

/// main.dol's global symbols: name -> (address, ELF section index in main.elf).
#[derive(Default)]
pub struct MainSymbols {
    by_name: HashMap<String, (u32, u16)>,
}

impl MainSymbols {
    /// "address section name" per line (wp-core tools/wpcore_pack.py, main.sym); the first of a name wins.
    pub fn parse(text: &str) -> Result<MainSymbols, String> {
        let mut by_name = HashMap::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let mut it = line.splitn(3, ' ');
            let (Some(a), Some(s), Some(n)) = (it.next(), it.next(), it.next()) else {
                return Err(format!("main.sym : ligne illisible « {line} »"));
            };
            let address = u32::from_str_radix(a, 16).map_err(|_| format!("main.sym : adresse « {a} »"))?;
            let section = s.parse().map_err(|_| format!("main.sym : section « {s} »"))?;
            by_name.entry(n.to_string()).or_insert((address, section));
        }
        Ok(MainSymbols { by_name })
    }

    pub fn get(&self, name: &str) -> Option<(u32, u16)> {
        self.by_name.get(name).copied()
    }

    /// A symbol of the mods' main.dol code (dol.rs), for the RELs; main.dol's own of that name wins. The
    /// section does not matter: a REL's relocations against main.dol (module 0) are absolute.
    pub fn add(&mut self, name: &str, address: u32) {
        self.by_name.entry(name.to_string()).or_insert((address, 0));
    }
}

/// What `dtk rel make` keeps of the disc's REL header (wp-core mr001.json).
#[derive(Clone, serde::Deserialize)]
pub struct RelParams {
    pub module_id: u32,
    pub num_sections: u32,
    pub name_offset: u32,
    pub name_size: u32,
    pub version: u32,
    pub align: u32,
    pub bss_align: u32,
    /// By section index: the REL section's executable flag.
    pub exec: Vec<bool>,
}

pub const PERMITTED_SECTIONS: [&str; 7] = [".init", ".text", ".ctors", ".dtors", ".rodata", ".data", ".bss"];

const R_DOLPHIN_NOP: u8 = 201;
const R_DOLPHIN_SECTION: u8 = 202;
const R_DOLPHIN_END: u8 = 203;

fn is_permitted(s: &elf::Section) -> bool {
    PERMITTED_SECTIONS.contains(&s.name.as_str())
}

#[derive(Clone, Debug)]
struct RelReloc {
    kind: u8,
    section: u8,
    address: u32,
    module_id: u32,
    target_section: u8,
    addend: u32,
}

/// The relocations of the module's sections, with the undefined symbols found in main.dol.
fn resolve(obj: &Object, module_id: u32, main: &MainSymbols) -> Result<Vec<RelReloc>, String> {
    let mut out = Vec::new();
    let mut missing = Vec::new();
    for (index, section) in obj.sections.iter().enumerate().filter(|(_, s)| is_permitted(s)) {
        for r in &section.relocs {
            let target = &obj.symbols[r.symbol];
            let (module, target_section, value) = if target.is_defined() {
                (module_id, target.section as u8, target.value)
            } else if target.section == elf::SHN_ABS {
                // an address of main.dol given as is (resident.rs: main.dol's own list of a hook that
                // the module has a list of too)
                (0, 0, target.value)
            } else if let Some((address, section)) = main.get(&target.name) {
                (0, section as u8, address)
            } else {
                missing.push(target.name.clone());
                continue;
            };
            out.push(RelReloc {
                kind: r.kind,
                section: index as u8,
                address: r.offset,
                module_id: module,
                target_section,
                addend: (value as i64 + r.addend as i64) as u32,
            });
        }
    }
    if !missing.is_empty() {
        missing.sort();
        missing.dedup();
        let more = if missing.len() > 8 { format!(" (et {} autres)", missing.len() - 8) } else { String::new() };
        return Err(format!("symboles introuvables : {}{more}", missing[..missing.len().min(8)].join(", ")));
    }
    Ok(out)
}

fn put32(w: &mut Vec<u8>, v: u32) {
    w.extend_from_slice(&v.to_be_bytes());
}

fn align_up(v: u32, a: u32) -> u32 {
    (v + a - 1) & !(a - 1)
}

/// The REL of `obj`, its undefined symbols taken from main.dol.
pub fn write(obj: &Object, params: &RelParams, main: &MainSymbols) -> Result<Vec<u8>, String> {
    let module_id = params.module_id;
    let mut relocations = resolve(obj, module_id, main)?;
    let rank = |r: &RelReloc| {
        if r.module_id == 0 {
            2
        } else if r.module_id == module_id {
            1
        } else {
            0
        }
    };
    relocations.sort_by(|a, b| {
        rank(a)
            .cmp(&rank(b))
            .then(if rank(a) == 0 { a.module_id.cmp(&b.module_id) } else { std::cmp::Ordering::Equal })
            .then(a.section.cmp(&b.section))
            .then(a.address.cmp(&b.address))
    });
    let relative = |r: &RelReloc| r.kind == elf::R_PPC_REL24 || r.kind == elf::R_PPC_REL14;
    let applied: Vec<RelReloc> = relocations.iter().filter(|r| relative(r)).cloned().collect();
    relocations.retain(|r| !(relative(r) && r.module_id == module_id && r.section == r.target_section));

    let permitted: Vec<(usize, &elf::Section)> = obj.sections.iter().enumerate().filter(|(_, s)| is_permitted(s)).collect();
    let written = |s: &elf::Section| s.kind != elf::SHT_NOBITS;
    let bss = permitted.iter().find(|(_, s)| s.name == ".bss");
    let num_sections = params.num_sections;
    let align = params.align;
    let bss_align = params.bss_align;

    let mut prolog = (0u8, 0u32);
    let mut epilog = (0u8, 0u32);
    let mut unresolved = (0u8, 0u32);
    for s in obj.symbols.iter().filter(|s| s.is_defined()) {
        match s.name.as_str() {
            "_prolog" => prolog = (s.section as u8, s.value),
            "_epilog" => epilog = (s.section as u8, s.value),
            "_unresolved" => unresolved = (s.section as u8, s.value),
            _ => {}
        }
    }

    let header_size = if params.version >= 3 { 0x4C } else if params.version >= 2 { 0x48 } else { 0x40 };
    let section_info_offset = header_size;
    let mut offset = section_info_offset + num_sections * 8;
    let section_data_offset = offset;
    for (_, s) in permitted.iter().filter(|(_, s)| written(s)) {
        offset = align_up(offset, s.align.max(1));
        offset += s.size;
    }
    if params.version >= 3 {
        offset = align_up(offset, 4);
    }

    // Relocation layout
    let mut imp: Vec<(u32, u32)> = Vec::new();
    let mut raw: Vec<(u16, u8, u8, u32)> = Vec::new();
    let mut fix_size = None;
    let (imp_offset, rel_offset);
    if relocations.is_empty() {
        imp_offset = 0;
        rel_offset = 0;
        fix_size = Some(offset);
    } else {
        let mut modules: Vec<u32> = relocations.iter().map(|r| r.module_id).collect();
        modules.dedup();
        imp_offset = offset;
        offset += modules.len() as u32 * 8;
        rel_offset = offset;
        let mut address = 0u32;
        let mut section = u8::MAX;
        let mut last_module = u32::MAX;
        for r in &relocations {
            if r.module_id != last_module {
                if last_module != u32::MAX {
                    raw.push((0, R_DOLPHIN_END, 0, 0));
                    offset += 8;
                }
                imp.push((r.module_id, offset));
                section = u8::MAX;
                last_module = r.module_id;
            }
            if params.version >= 3 && fix_size.is_none() && (r.module_id == 0 || r.module_id == module_id) {
                fix_size = Some(offset);
            }
            if r.section != section {
                raw.push((0, R_DOLPHIN_SECTION, r.section, 0));
                offset += 8;
                address = 0;
                section = r.section;
            }
            let mut delta = r.address - address;
            while delta > 0xFFFF {
                raw.push((0xFFFF, R_DOLPHIN_NOP, 0, 0));
                offset += 8;
                delta -= 0xFFFF;
            }
            match r.kind {
                elf::R_PPC_ADDR32 | elf::R_PPC_ADDR16_LO | elf::R_PPC_ADDR16_HI | elf::R_PPC_ADDR16_HA | elf::R_PPC_REL24 | elf::R_PPC_REL14 => {}
                k => return Err(format!("type de relocation {k} non pris en charge")),
            }
            raw.push((delta as u16, r.kind, r.target_section, r.addend));
            address = r.address;
            offset += 8;
        }
        raw.push((0, R_DOLPHIN_END, 0, 0));
        offset += 8;
    }

    let mut w = Vec::with_capacity(offset as usize);
    put32(&mut w, module_id);
    put32(&mut w, 0);
    put32(&mut w, 0);
    put32(&mut w, num_sections);
    put32(&mut w, section_info_offset);
    put32(&mut w, params.name_offset);
    put32(&mut w, params.name_size);
    put32(&mut w, params.version);
    put32(&mut w, bss.map_or(0, |(_, s)| s.size));
    put32(&mut w, rel_offset);
    put32(&mut w, imp_offset);
    put32(&mut w, imp.len() as u32 * 8);
    w.extend_from_slice(&[prolog.0, epilog.0, unresolved.0, 0]);
    put32(&mut w, prolog.1);
    put32(&mut w, epilog.1);
    put32(&mut w, unresolved.1);
    if params.version >= 2 {
        put32(&mut w, align);
        put32(&mut w, bss_align);
    }
    if params.version >= 3 {
        put32(&mut w, fix_size.unwrap_or(0));
    }
    let mut data_offset = section_data_offset;
    for index in 0..num_sections as usize {
        match obj.sections.get(index).filter(|s| is_permitted(s)) {
            Some(s) => {
                let mut at = 0;
                if written(s) {
                    data_offset = align_up(data_offset, s.align.max(1));
                    at = data_offset;
                    data_offset += s.size;
                }
                let exec = params.exec.get(index).copied().unwrap_or(s.flags & elf::SHF_EXECINSTR != 0);
                put32(&mut w, at | exec as u32);
                put32(&mut w, s.size);
            }
            None => {
                put32(&mut w, 0);
                put32(&mut w, 0);
            }
        }
    }
    for (index, s) in permitted.iter().filter(|(_, s)| written(s)) {
        let pad = align_up(w.len() as u32, s.align.max(1)) as usize - w.len();
        w.resize(w.len() + pad, 0);
        let mut data = s.data.clone();
        for r in applied.iter().filter(|r| r.section as usize == *index) {
            let diff = if r.module_id == module_id && r.section == r.target_section {
                r.addend as i32 - r.address as i32
            } else if unresolved.0 == r.section {
                unresolved.1 as i32 - r.address as i32
            } else {
                continue;
            };
            let at = r.address as usize;
            let mut ins = u32::from_be_bytes(data[at..at + 4].try_into().unwrap());
            if r.kind == elf::R_PPC_REL24 {
                if !(-0x2000000..0x2000000).contains(&diff) {
                    return Err("branchement trop loin (R_PPC_REL24)".into());
                }
                ins = (ins & !0x3FFFFFC) | (diff as u32 & 0x3FFFFFC);
            } else {
                if !(-0x8000..0x8000).contains(&diff) {
                    return Err("branchement trop loin (R_PPC_REL14)".into());
                }
                ins = (ins & !0xFFFC) | (diff as u32 & 0xFFFC);
            }
            data[at..at + 4].copy_from_slice(&ins.to_be_bytes());
        }
        w.extend_from_slice(&data);
    }
    if params.version >= 3 {
        let pad = align_up(w.len() as u32, 4) as usize - w.len();
        w.resize(w.len() + pad, 0);
    }
    if !relocations.is_empty() {
        for (m, o) in &imp {
            put32(&mut w, *m);
            put32(&mut w, *o);
        }
        for (delta, kind, section, addend) in &raw {
            w.extend_from_slice(&delta.to_be_bytes());
            w.push(*kind);
            w.push(*section);
            put32(&mut w, *addend);
        }
    }
    if w.len() as u32 != offset {
        return Err("taille du REL incohérente".into());
    }
    Ok(w)
}
