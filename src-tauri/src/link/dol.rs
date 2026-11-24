//! The mods' code for main.dol, which never changes: it is linked at a fixed address of the free low memory
//! and reached by injections. An injected instruction becomes a `b` to a trampoline that saves every register
//! into a `WpContext` (wp/mod/context.h), calls the handlers with it (they may read and change the registers,
//! or set `resume` to continue elsewhere), restores, runs the instruction and comes back. Several mods at the
//! same place share the trampoline. A replaced function's first instruction becomes a `b` to its replacement.
//! Once main.dol's minigame table is moved (minigames.rs), its original place takes what does not fit in the
//! low memory. Everything becomes Riivolution memory patches.

use super::elf::{self, Object, Symbol};
use super::rel::MainSymbols;
use super::LinkError;
use std::collections::HashMap;

/// The free memory the mods' main.dol code goes into (the exception vectors end at 0x1800, the OS globals
/// start at 0x3000).
pub const BASE: u32 = 0x8000_1800;
pub const END: u32 = 0x8000_3000;

/// One injection: before (or after) the instruction at `symbol + offset`, which must be `expect`.
#[derive(Clone)]
pub struct Inject {
    pub symbol: String,
    pub offset: u32,
    pub expect: u32,
    pub handler: String,
    pub after: bool,
}

/// One function replaced: its first instruction, which must be `expect`, becomes a `b` to `handler`.
/// `original`, when set, is a function the launcher makes that runs the replaced one: `run` (its first
/// instruction, `expect` or what another patch puts there), then the rest in place.
#[derive(Clone)]
pub struct Replace {
    pub symbol: String,
    pub expect: u32,
    pub run: u32,
    pub handler: String,
    pub original: Option<String>,
}

#[derive(Clone, Default)]
pub struct DolPart {
    pub id: String,
    pub objects: Vec<Object>,
    pub injects: Vec<Inject>,
    pub replaces: Vec<Replace>,
    pub writes: Vec<(String, u32, Vec<u8>)>,
}

/// Memory of main.dol that the mods' code may take after the low memory: the original place of the
/// minigame table once the launcher moved it, with its bytes (Riivolution's `original`).
pub struct Spare {
    pub address: u32,
    pub original: Vec<u8>,
}

/// A memory patch: `original`, when set, must be there (else Riivolution leaves it).
#[derive(Clone, Debug, PartialEq)]
pub struct Patch {
    pub address: u32,
    pub value: Vec<u8>,
    pub original: Option<Vec<u8>>,
}

// WpContext (wp/mod/context.h): gpr[32], cr, lr, ctr, xer, resume, pad, fpr[32] (doubles)
const CTX_GPR: i16 = 0;
const CTX_CR: i16 = 0x80;
const CTX_LR: i16 = 0x84;
const CTX_CTR: i16 = 0x88;
const CTX_XER: i16 = 0x8C;
const CTX_RESUME: i16 = 0x90;
const CTX_FPR: i16 = 0x98;
pub const CTX_SIZE: i16 = 0x198;
/// Back chain and LR save word, then the context.
const CTX_AT: i16 = 8;
const FRAME: i16 = (CTX_AT + CTX_SIZE + 15) & !15;

mod ppc {
    pub fn d_form(op: u32, rt: u32, ra: u32, d: i16) -> u32 {
        (op << 26) | (rt << 21) | (ra << 16) | (d as u16 as u32)
    }
    pub fn stwu(rs: u32, ra: u32, d: i16) -> u32 { d_form(37, rs, ra, d) }
    pub fn stw(rs: u32, ra: u32, d: i16) -> u32 { d_form(36, rs, ra, d) }
    pub fn lwz(rt: u32, ra: u32, d: i16) -> u32 { d_form(32, rt, ra, d) }
    pub fn stmw(rs: u32, ra: u32, d: i16) -> u32 { d_form(47, rs, ra, d) }
    pub fn lmw(rt: u32, ra: u32, d: i16) -> u32 { d_form(46, rt, ra, d) }
    pub fn stfd(fs: u32, ra: u32, d: i16) -> u32 { d_form(54, fs, ra, d) }
    pub fn lfd(ft: u32, ra: u32, d: i16) -> u32 { d_form(50, ft, ra, d) }
    pub fn addi(rt: u32, ra: u32, d: i16) -> u32 { d_form(14, rt, ra, d) }
    pub fn cmpwi0(ra: u32) -> u32 { d_form(11, 0, ra, 0) }
    fn spr(spr: u32) -> u32 { ((spr & 31) << 16) | ((spr >> 5) << 11) }
    pub fn mfspr(rt: u32, n: u32) -> u32 { (31 << 26) | (rt << 21) | spr(n) | (339 << 1) }
    pub fn mtspr(n: u32, rs: u32) -> u32 { (31 << 26) | (rs << 21) | spr(n) | (467 << 1) }
    pub const XER: u32 = 1;
    pub const LR: u32 = 8;
    pub const CTR: u32 = 9;
    pub fn mfcr(rt: u32) -> u32 { (31 << 26) | (rt << 21) | (19 << 1) }
    pub fn mtcr(rs: u32) -> u32 { (31 << 26) | (rs << 21) | (0xFF << 12) | (144 << 1) }
    pub const BCTR: u32 = 0x4E80_0420;
    pub fn b(from: u32, to: u32, link: bool) -> Option<u32> {
        let d = to.wrapping_sub(from) as i32;
        (-0x200_0000..0x200_0000).contains(&d).then(|| 0x4800_0000 | (d as u32 & 0x03FF_FFFC) | link as u32)
    }
    /// beq cr0, +d
    pub fn beq(d: i16) -> u32 { 0x4182_0000 | (d as u16 as u32 & 0xFFFC) }
}

/// The instruction `ins`, moved from `from` to `to`: relative branches keep their target.
fn moved(ins: u32, from: u32, to: u32) -> Result<Vec<u32>, String> {
    let op = ins >> 26;
    if op == 18 && ins & 2 == 0 {
        let mut d = (ins & 0x03FF_FFFC) as i32;
        if d & 0x0200_0000 != 0 {
            d -= 0x0400_0000;
        }
        let target = from.wrapping_add(d as u32);
        return ppc::b(to, target, ins & 1 != 0).map(|w| vec![w]).ok_or_else(|| "branchement trop loin".into());
    }
    if op == 16 && ins & 2 == 0 {
        let mut d = (ins & 0xFFFC) as i32;
        if d & 0x8000 != 0 {
            d -= 0x1_0000;
        }
        let target = from.wrapping_add(d as u32);
        // bc +8 (to the long branch) ; b over it ; b(l) target
        let short = (ins & !0xFFFC) | 8;
        let long = ppc::b(to + 8, target, ins & 1 != 0).ok_or("branchement trop loin")?;
        return Ok(vec![short, 0x4800_0008, long]);
    }
    Ok(vec![ins])
}

/// A free area, filled from its start: its bytes up to `next` become one memory patch.
struct Region {
    start: u32,
    end: u32,
    next: u32,
    bytes: Vec<u8>,
    original: Option<Vec<u8>>,
}

/// The free areas, in the order they are filled.
struct Memory {
    regions: Vec<Region>,
}

impl Memory {
    /// `size` bytes aligned to `align` in the first area with room.
    fn alloc(&mut self, size: u32, align: u32) -> Option<u32> {
        for r in &mut self.regions {
            let at = (r.next + align - 1) & !(align - 1);
            if at + size <= r.end {
                r.next = at + size;
                return Some(at);
            }
        }
        None
    }

    /// Code that `make` writes for its address, in the first area with room.
    fn alloc_code(&mut self, make: impl Fn(u32) -> Result<Vec<u32>, String>) -> Result<Option<(u32, Vec<u32>)>, String> {
        for r in &mut self.regions {
            let at = (r.next + 3) & !3;
            let code = make(at)?;
            if at + 4 * code.len() as u32 <= r.end {
                r.next = at + 4 * code.len() as u32;
                return Ok(Some((at, code)));
            }
        }
        Ok(None)
    }

    fn free(&self) -> u32 {
        self.regions.iter().map(|r| r.end - r.next).sum()
    }

    /// The bytes at `at` (once the layout is done: `close`).
    fn bytes(&mut self, at: u32, len: usize) -> &mut [u8] {
        let r = self.regions.iter_mut().find(|r| r.start <= at && at < r.next).expect("adresse hors de la mémoire des mods");
        let o = (at - r.start) as usize;
        &mut r.bytes[o..o + len]
    }

    fn word(&mut self, at: u32, w: u32) {
        self.bytes(at, 4).copy_from_slice(&w.to_be_bytes());
    }

    fn close(&mut self) {
        for r in &mut self.regions {
            r.bytes.resize((r.next - r.start) as usize, 0);
        }
    }
}

/// Assembles code at its address.
struct Asm {
    code: Vec<u32>,
    pc: u32,
}

impl Asm {
    fn e(&mut self, w: u32) {
        self.code.push(w);
        self.pc += 4;
    }
}

/// The trampoline of an injection site at `at` (instruction `original`), for the address `start`.
fn trampoline(start: u32, at: u32, original: u32, before: &[u32], after: &[u32]) -> Result<Vec<u32>, String> {
    let mut a = Asm { code: Vec::new(), pc: start };
    fn save(a: &mut Asm) {
        a.e(ppc::stwu(1, 1, -FRAME));
        a.e(ppc::stw(0, 1, CTX_AT + CTX_GPR));
        a.e(ppc::stmw(2, 1, CTX_AT + CTX_GPR + 8));
        a.e(ppc::addi(0, 1, FRAME));
        a.e(ppc::stw(0, 1, CTX_AT + CTX_GPR + 4));
        a.e(ppc::mfcr(0));
        a.e(ppc::stw(0, 1, CTX_AT + CTX_CR));
        a.e(ppc::mfspr(0, ppc::LR));
        a.e(ppc::stw(0, 1, CTX_AT + CTX_LR));
        a.e(ppc::mfspr(0, ppc::CTR));
        a.e(ppc::stw(0, 1, CTX_AT + CTX_CTR));
        a.e(ppc::mfspr(0, ppc::XER));
        a.e(ppc::stw(0, 1, CTX_AT + CTX_XER));
        a.e(ppc::addi(0, 0, 0));
        a.e(ppc::stw(0, 1, CTX_AT + CTX_RESUME));
        for f in 0..32 {
            a.e(ppc::stfd(f, 1, CTX_AT + CTX_FPR + 8 * f as i16));
        }
    }
    // restores everything but CR0..7 and CTR when `resume` (they are needed to leave)
    fn restore(a: &mut Asm, keep_ctr: bool) {
        for f in 0..32 {
            a.e(ppc::lfd(f, 1, CTX_AT + CTX_FPR + 8 * f as i16));
        }
        a.e(ppc::lwz(0, 1, CTX_AT + CTX_XER));
        a.e(ppc::mtspr(ppc::XER, 0));
        if keep_ctr {
            a.e(ppc::lwz(0, 1, CTX_AT + CTX_CTR));
            a.e(ppc::mtspr(ppc::CTR, 0));
        }
        a.e(ppc::lwz(0, 1, CTX_AT + CTX_LR));
        a.e(ppc::mtspr(ppc::LR, 0));
        a.e(ppc::lwz(0, 1, CTX_AT + CTX_CR));
        a.e(ppc::mtcr(0));
        a.e(ppc::lmw(2, 1, CTX_AT + CTX_GPR + 8));
        a.e(ppc::lwz(0, 1, CTX_AT + CTX_GPR));
        a.e(ppc::addi(1, 1, FRAME));
    }
    fn block(a: &mut Asm, handlers: &[u32]) -> Result<(), String> {
        if handlers.is_empty() {
            return Ok(());
        }
        save(a);
        for &target in handlers {
            a.e(ppc::addi(3, 1, CTX_AT));
            let w = ppc::b(a.pc, target, true).ok_or("appel trop loin")?;
            a.e(w);
        }
        // resume set: leave through CTR
        a.e(ppc::lwz(0, 1, CTX_AT + CTX_RESUME));
        a.e(ppc::cmpwi0(0));
        let beq_at = a.code.len();
        a.e(0);
        a.e(ppc::mtspr(ppc::CTR, 0));
        restore(a, false);
        a.e(ppc::BCTR);
        let d = (a.code.len() - beq_at) as i16 * 4;
        a.code[beq_at] = ppc::beq(d);
        restore(a, true);
        Ok(())
    }
    block(&mut a, before)?;
    for w in moved(original, at, a.pc)? {
        a.e(w);
    }
    block(&mut a, after)?;
    let back = ppc::b(a.pc, at + 4, false).ok_or("retour trop loin")?;
    a.e(back);
    Ok(a.code)
}

/// The memory patches, the code first, and the parts' global symbols (name, address): a mod's code in a REL
/// may use them (its main.dol part's variables and functions).
pub type Linked = (Vec<Patch>, Vec<(String, u32)>);

/// Links the mods' main.dol parts after `data` (at BASE: the minigame table, minigames.rs), then in `spare`
/// what does not fit. An injected instruction and a replaced function's first one are given (`expect`,
/// Riivolution checks them: `original`), so main.dol itself is not needed.
pub fn link(parts: &[DolPart], main: &MainSymbols, data: &[u8], spare: Option<&Spare>) -> Result<Linked, LinkError> {
    if data.is_empty() && parts.iter().all(|p| p.objects.is_empty() && p.injects.is_empty() && p.replaces.is_empty()) {
        return Ok((vec![], vec![]));
    }
    let all: Vec<&str> = parts.iter().map(|p| p.id.as_str()).collect();
    if BASE + data.len() as u32 > END {
        return Err(LinkError::new(format!("la table des mini-jeux dépasse la place libre ({} octets de trop)", BASE + data.len() as u32 - END), &[]));
    }
    let mut memory = Memory {
        regions: vec![Region { start: BASE, end: END, next: (BASE + data.len() as u32 + 31) & !31, bytes: Vec::new(), original: None }],
    };
    if let Some(s) = spare {
        let end = s.address + s.original.len() as u32;
        memory.regions.push(Region { start: s.address, end, next: s.address, bytes: Vec::new(), original: Some(s.original.clone()) });
    }
    let full = |memory: &Memory, size: u32| {
        LinkError::new(format!("le code des mods pour main.dol dépasse la place libre ({size} octets à placer, {} libres)", memory.free()), &all)
    };

    // layout: every allocated section of every object, code then data then bss
    let mut placed: HashMap<(usize, usize, usize), u32> = HashMap::new();
    for kind in [0, 1, 2] {
        for (p, part) in parts.iter().enumerate() {
            for (o, obj) in part.objects.iter().enumerate() {
                for (s, sec) in obj.sections.iter().enumerate() {
                    if !sec.is_alloc() || sec.size == 0 {
                        continue;
                    }
                    let this = if sec.flags & elf::SHF_EXECINSTR != 0 { 0 } else if sec.kind == elf::SHT_NOBITS { 2 } else { 1 };
                    if this != kind {
                        continue;
                    }
                    if sec.name == ".ctors" || sec.name == ".dtors" {
                        if sec.data.iter().any(|&b| b != 0) || !sec.relocs.is_empty() {
                            return Err(LinkError::new("le code pour main.dol ne peut pas avoir de constructeurs globaux".into(), &[&part.id]));
                        }
                        continue;
                    }
                    let at = memory.alloc(sec.size, sec.align.max(4)).ok_or_else(|| full(&memory, sec.size))?;
                    placed.insert((p, o, s), at);
                }
            }
        }
    }

    // global symbols of the parts
    let mut globals: HashMap<String, (u32, String)> = HashMap::new();
    let address_of = |p: usize, o: usize, sym: &Symbol| -> Option<u32> {
        if sym.section == 0xFFF1 {
            return Some(sym.value);
        }
        placed.get(&(p, o, sym.section as usize)).map(|a| a + sym.value)
    };
    for (p, part) in parts.iter().enumerate() {
        for (o, obj) in part.objects.iter().enumerate() {
            for sym in obj.symbols.iter().filter(|s| s.is_global() && s.is_defined()) {
                let Some(at) = address_of(p, o, sym) else { continue };
                match globals.get(&sym.name) {
                    Some((_, other)) if sym.bind == elf::STB_GLOBAL && other != &part.id => {
                        return Err(LinkError::new(format!("« {} » est défini deux fois", sym.name), &[other, &part.id]));
                    }
                    Some(_) => {}
                    None => {
                        globals.insert(sym.name.clone(), (at, part.id.clone()));
                    }
                }
            }
        }
    }

    // replaced functions: the jumps to the replacements, and the originals the parts call
    let mut replaced: Vec<(u32, &Replace, &str)> = Vec::new();
    let mut code: Vec<(u32, Vec<u32>)> = Vec::new();
    for part in parts {
        for r in &part.replaces {
            let fail = |m: String| LinkError::new(m, &[&part.id]);
            let at = main.get(&r.symbol).ok_or_else(|| fail(format!("« {} » n'est pas dans main.dol", r.symbol)))?.0;
            if let Some((_, _, other)) = replaced.iter().find(|x| x.0 == at) {
                return Err(LinkError::new(format!("« {} » est remplacée deux fois", r.symbol), &[other, &part.id]));
            }
            if let Some(name) = &r.original {
                let make = |pc: u32| -> Result<Vec<u32>, String> {
                    let mut words = moved(r.run, at, pc)?;
                    let back = pc + 4 * words.len() as u32;
                    words.push(ppc::b(back, at + 4, false).ok_or("retour trop loin")?);
                    Ok(words)
                };
                let (thunk, words) = memory.alloc_code(make).map_err(fail)?.ok_or_else(|| full(&memory, 8))?;
                if let Some((_, other)) = globals.get(name) {
                    return Err(LinkError::new(format!("« {name} » est défini deux fois"), &[other, &part.id]));
                }
                globals.insert(name.clone(), (thunk, part.id.clone()));
                code.push((thunk, words));
            }
            replaced.push((at, r, &part.id));
        }
    }
    let resolve = |name: &str| globals.get(name).map(|g| g.0).or_else(|| main.get(name).map(|m| m.0));

    // trampolines, after the code
    let mut sites: Vec<(u32, u32, Vec<(&Inject, &str)>)> = Vec::new();
    for part in parts {
        for inj in &part.injects {
            let fail = |m: String| LinkError::new(m, &[&part.id]);
            let at = main.get(&inj.symbol).ok_or_else(|| fail(format!("« {} » n'est pas dans main.dol", inj.symbol)))?.0 + inj.offset;
            let found = inj.expect;
            if resolve(&inj.handler).is_none() {
                return Err(fail(format!("« {} » est introuvable dans le code du mod", inj.handler)));
            }
            if let Some((_, r, other)) = replaced.iter().find(|x| x.1.symbol == inj.symbol) {
                return Err(LinkError::new(format!("« {} » est remplacée : plus d'injection possible dedans", r.symbol), &[other, &part.id]));
            }
            match sites.iter_mut().find(|s| s.0 == at) {
                Some(site) if site.1 != found => {
                    return Err(LinkError::new(format!("{}+{:#X} : deux instructions d'origine différentes attendues", inj.symbol, inj.offset), &[&part.id]));
                }
                Some(site) => site.2.push((inj, &part.id)),
                None => sites.push((at, found, vec![(inj, &part.id)])),
            }
        }
    }
    let mut site_patches = Vec::new();
    for (at, original, injs) in &sites {
        let before: Vec<u32> = injs.iter().filter(|(i, _)| !i.after).map(|(i, _)| resolve(&i.handler).unwrap()).collect();
        let after: Vec<u32> = injs.iter().filter(|(i, _)| i.after).map(|(i, _)| resolve(&i.handler).unwrap()).collect();
        let ids: Vec<&str> = injs.iter().map(|(_, id)| *id).collect();
        let fail = |m: String| LinkError::new(m, &ids);
        let make = |pc: u32| trampoline(pc, *at, *original, &before, &after);
        let (start, words) = memory.alloc_code(make).map_err(fail)?.ok_or_else(|| full(&memory, 4 * trampoline(*at, *at, *original, &before, &after).map_or(0, |w| w.len() as u32)))?;
        code.push((start, words));
        let jump = ppc::b(*at, start, false).ok_or_else(|| fail("tremplin trop loin".into()))?;
        site_patches.push(Patch { address: *at, value: jump.to_be_bytes().to_vec(), original: Some(original.to_be_bytes().to_vec()) });
    }
    for (at, r, id) in &replaced {
        let fail = |m: String| LinkError::new(m, &[id]);
        let target = resolve(&r.handler).ok_or_else(|| fail(format!("« {} » est introuvable dans le code du mod", r.handler)))?;
        let jump = ppc::b(*at, target, false).ok_or_else(|| fail("remplacement trop loin".into()))?;
        site_patches.push(Patch { address: *at, value: jump.to_be_bytes().to_vec(), original: Some(r.expect.to_be_bytes().to_vec()) });
    }

    // the bytes: the data, the sections, relocations, then the trampolines
    memory.close();
    if !data.is_empty() {
        memory.bytes(BASE, data.len()).copy_from_slice(data);
    }
    for (p, part) in parts.iter().enumerate() {
        for (o, obj) in part.objects.iter().enumerate() {
            for (s, sec) in obj.sections.iter().enumerate() {
                let Some(&at) = placed.get(&(p, o, s)) else { continue };
                if sec.kind != elf::SHT_NOBITS {
                    memory.bytes(at, sec.data.len()).copy_from_slice(&sec.data);
                }
                for r in &sec.relocs {
                    let sym = &obj.symbols[r.symbol];
                    let fail = |m: String| LinkError::new(m, &[&part.id]);
                    let target = if sym.is_defined() { address_of(p, o, sym) } else { resolve(&sym.name) }
                        .ok_or_else(|| fail(format!("symbole introuvable : {}", sym.name)))?;
                    let value = target.wrapping_add(r.addend as u32);
                    let place = at + r.offset;
                    // the 16-bit relocations point at the field itself (the instruction + 2)
                    let old = if matches!(r.kind, elf::R_PPC_REL24 | elf::R_PPC_REL14 | elf::R_PPC_ADDR32) {
                        u32::from_be_bytes(memory.bytes(place, 4).try_into().unwrap())
                    } else {
                        0
                    };
                    let half = |v: u16| v.to_be_bytes().to_vec();
                    let new: Vec<u8> = match r.kind {
                        elf::R_PPC_ADDR32 => value.to_be_bytes().to_vec(),
                        elf::R_PPC_ADDR16_LO => half(value as u16),
                        elf::R_PPC_ADDR16_HI => half((value >> 16) as u16),
                        elf::R_PPC_ADDR16_HA => half(((value + 0x8000) >> 16) as u16),
                        elf::R_PPC_REL24 => {
                            let d = value.wrapping_sub(place) as i32;
                            if !(-0x200_0000..0x200_0000).contains(&d) {
                                return Err(fail("branchement trop loin".into()));
                            }
                            ((old & !0x03FF_FFFC) | (d as u32 & 0x03FF_FFFC)).to_be_bytes().to_vec()
                        }
                        elf::R_PPC_REL14 => {
                            let d = value.wrapping_sub(place) as i32;
                            if !(-0x8000..0x8000).contains(&d) {
                                return Err(fail("branchement trop loin".into()));
                            }
                            ((old & !0xFFFC) | (d as u32 & 0xFFFC)).to_be_bytes().to_vec()
                        }
                        k => return Err(fail(format!("type de relocation {k} non pris en charge (compiler avec -sdata 0 -sdata2 0)"))),
                    };
                    memory.bytes(place, new.len()).copy_from_slice(&new);
                }
            }
        }
    }
    for (start, words) in &code {
        for (k, w) in words.iter().enumerate() {
            memory.word(start + 4 * k as u32, *w);
        }
    }
    for part in parts {
        for (name, offset, bytes) in &part.writes {
            let fail = |m: String| LinkError::new(m, &[&part.id]);
            let at = globals.get(name).filter(|g| g.1 == part.id).ok_or_else(|| fail(format!("« {name} » est introuvable dans le code du mod")))?.0 + offset;
            memory.bytes(at, bytes.len()).copy_from_slice(bytes);
        }
    }
    let mut patches = Vec::new();
    for r in memory.regions {
        if r.next > r.start || r.start == BASE {
            let original = r.original.map(|o| o[..r.bytes.len()].to_vec());
            patches.push(Patch { address: r.start, value: r.bytes, original });
        }
    }
    patches.extend(site_patches);
    let mut symbols: Vec<(String, u32)> = globals.into_iter().map(|(name, (at, _))| (name, at)).collect();
    symbols.sort();
    Ok((patches, symbols))
}

#[cfg(test)]
pub mod tests_ppc {
    use super::ppc::*;
    pub fn all() -> Vec<u32> {
        vec![stwu(1, 1, -0x1B0), stw(0, 1, 8), stmw(2, 1, 0x10), mfspr(0, LR), mtspr(LR, 0), mfcr(0), mtcr(0), mfspr(0, CTR), mtspr(CTR, 0),
             mfspr(0, XER), mtspr(XER, 0), stfd(0, 1, 0x98), lfd(0, 1, 0x98), addi(0, 0, 0), cmpwi0(0), beq(0x10), lmw(2, 1, 0x10), lwz(0, 1, 8)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b_at(at: u32, w: u32) -> u32 {
        let d = (w & 0x03FF_FFFC) as i32;
        at.wrapping_add(if d & 0x0200_0000 != 0 { d - 0x0400_0000 } else { d } as u32)
    }

    #[test]
    fn replaces_and_spills_into_the_spare_place() {
        let main = MainSymbols::parse("8000A3C0 1 f\n").unwrap();
        // the low memory keeps 0x100 bytes: the handler takes them, the original's code goes to the spare place
        let data = vec![0xAA; 0x1700];
        let part = DolPart {
            id: "cœur".into(),
            objects: vec![super::super::module::data_object("handler", vec![0; 0x100])],
            replaces: vec![Replace { symbol: "f".into(), expect: 0x3C80_801E, run: 0x3C80_8000, handler: "handler".into(), original: Some("f_orig".into()) }],
            writes: vec![("handler".into(), 0, vec![1, 2, 3, 4])],
            ..Default::default()
        };
        let spare = Spare { address: 0x801D_D0E0, original: vec![0x55; 0x40] };
        let (p, symbols) = link(&[part], &main, &data, Some(&spare)).unwrap();
        assert_eq!(symbols, [("f_orig".to_string(), 0x801D_D0E0), ("handler".to_string(), BASE + 0x1700)]);
        assert_eq!(p.len(), 3);
        assert_eq!((p[0].address, p[0].value.len()), (BASE, 0x1800));
        assert_eq!(&p[0].value[0x1700..0x1704], &[1, 2, 3, 4]);
        let thunk: Vec<u32> = p[1].value.chunks(4).map(|w| u32::from_be_bytes(w.try_into().unwrap())).collect();
        assert_eq!((p[1].address, thunk.len(), p[1].original.as_deref()), (0x801D_D0E0, 2, Some(&[0x55u8; 8][..])));
        assert_eq!(thunk[0], 0x3C80_8000);
        assert_eq!(b_at(0x801D_D0E4, thunk[1]), 0x8000_A3C4);
        let jump = u32::from_be_bytes(p[2].value[..].try_into().unwrap());
        assert_eq!((p[2].address, b_at(0x8000_A3C0, jump)), (0x8000_A3C0, BASE + 0x1700));
        assert_eq!(p[2].original.as_deref(), Some(&[0x3C, 0x80, 0x80, 0x1E][..]));
        // no room left anywhere
        let small = Spare { address: 0x801D_D0E0, original: vec![0x55; 4] };
        let part = DolPart { replaces: vec![Replace { original: Some("f_orig".into()), ..replace_f() }], ..core_part() };
        assert!(link(&[part], &main, &data, Some(&small)).is_err());
    }

    fn replace_f() -> Replace {
        Replace { symbol: "f".into(), expect: 0x3C80_801E, run: 0x3C80_801E, handler: "handler".into(), original: None }
    }

    fn core_part() -> DolPart {
        DolPart { id: "cœur".into(), objects: vec![super::super::module::data_object("handler", vec![0; 0x100])], ..Default::default() }
    }

    #[test]
    fn refuses_two_replacements_of_a_function() {
        let main = MainSymbols::parse("8000A3C0 1 f\n").unwrap();
        let a = DolPart { replaces: vec![replace_f()], ..core_part() };
        let b = DolPart { id: "autre".into(), objects: vec![], replaces: vec![replace_f()], ..Default::default() };
        let e = link(&[a, b], &main, &[], None).err().unwrap();
        assert_eq!(e.mods, ["cœur", "autre"]);
    }
}
