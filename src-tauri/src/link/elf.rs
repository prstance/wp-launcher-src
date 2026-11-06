//! ELF32 big-endian relocatable objects, as mwcc and mwldeppc -r (a .plf) write them: their sections,
//! symbols and relocations (.rela).

pub const SHT_PROGBITS: u32 = 1;
pub const SHT_SYMTAB: u32 = 2;
pub const SHT_RELA: u32 = 4;
pub const SHT_NOBITS: u32 = 8;
pub const SHF_ALLOC: u32 = 2;
pub const SHF_EXECINSTR: u32 = 4;
pub const SHN_UNDEF: u16 = 0;

pub const STB_LOCAL: u8 = 0;
pub const STB_GLOBAL: u8 = 1;
pub const STB_WEAK: u8 = 2;

pub const R_PPC_ADDR32: u8 = 1;
#[allow(dead_code)] // (the linker refuses it)
pub const R_PPC_ADDR24: u8 = 2;
#[allow(dead_code)] // (the linker refuses it)
pub const R_PPC_ADDR16: u8 = 3;
pub const R_PPC_ADDR16_LO: u8 = 4;
pub const R_PPC_ADDR16_HI: u8 = 5;
pub const R_PPC_ADDR16_HA: u8 = 6;
#[allow(dead_code)] // (the linker refuses it)
pub const R_PPC_ADDR14: u8 = 7;
pub const R_PPC_REL24: u8 = 10;
pub const R_PPC_REL14: u8 = 11;

#[derive(Clone, Debug)]
pub struct Section {
    pub name: String,
    pub kind: u32,
    pub flags: u32,
    pub align: u32,
    /// Empty for SHT_NOBITS (.bss): `size` counts.
    pub data: Vec<u8>,
    pub size: u32,
    pub relocs: Vec<Reloc>,
}

impl Section {
    pub fn is_alloc(&self) -> bool {
        self.flags & SHF_ALLOC != 0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Reloc {
    pub offset: u32,
    pub kind: u8,
    /// Index into `Object::symbols`.
    pub symbol: usize,
    pub addend: i32,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Symbol {
    pub name: String,
    pub value: u32,
    pub size: u32,
    pub bind: u8,
    pub kind: u8,
    /// The section's index; SHN_UNDEF: undefined (elsewhere); 0xFFF1: absolute.
    pub section: u16,
}

impl Symbol {
    pub fn is_defined(&self) -> bool {
        self.section != SHN_UNDEF && self.section < 0xFF00
    }
    pub fn is_global(&self) -> bool {
        self.bind == STB_GLOBAL || self.bind == STB_WEAK
    }
}

#[derive(Clone, Debug, Default)]
pub struct Object {
    /// By ELF index (0: the null section).
    pub sections: Vec<Section>,
    /// By ELF symbol index (0: the null symbol).
    pub symbols: Vec<Symbol>,
}

fn u16_at(d: &[u8], o: usize) -> Result<u16, String> {
    d.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]])).ok_or_else(|| "ELF tronqué".to_string())
}

fn u32_at(d: &[u8], o: usize) -> Result<u32, String> {
    d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| "ELF tronqué".to_string())
}

fn c_string(d: &[u8], o: usize) -> Result<String, String> {
    let rest = d.get(o..).ok_or("nom hors du fichier")?;
    let end = rest.iter().position(|&b| b == 0).ok_or("nom sans fin")?;
    Ok(String::from_utf8_lossy(&rest[..end]).into_owned())
}

impl Object {
    pub fn parse(d: &[u8]) -> Result<Object, String> {
        if d.get(..6) != Some(b"\x7fELF\x01\x02") {
            return Err("pas un ELF 32 bits big-endian".into());
        }
        if u16_at(d, 0x10)? != 1 {
            return Err("pas un objet relogeable".into());
        }
        let shoff = u32_at(d, 0x20)? as usize;
        let shentsize = u16_at(d, 0x2E)? as usize;
        let shnum = u16_at(d, 0x30)? as usize;
        let shstrndx = u16_at(d, 0x32)? as usize;
        struct Raw {
            name: u32,
            kind: u32,
            flags: u32,
            offset: usize,
            size: u32,
            link: u32,
            info: u32,
            align: u32,
        }
        let mut raw = Vec::with_capacity(shnum);
        for i in 0..shnum {
            let o = shoff + i * shentsize;
            raw.push(Raw {
                name: u32_at(d, o)?,
                kind: u32_at(d, o + 4)?,
                flags: u32_at(d, o + 8)?,
                offset: u32_at(d, o + 16)? as usize,
                size: u32_at(d, o + 20)?,
                link: u32_at(d, o + 24)?,
                info: u32_at(d, o + 28)?,
                align: u32_at(d, o + 32)?,
            });
        }
        let names = raw.get(shstrndx).ok_or("pas de table des noms de sections")?.offset;
        let mut sections = Vec::with_capacity(shnum);
        for r in &raw {
            let data = if r.kind == SHT_NOBITS || r.kind == 0 {
                Vec::new()
            } else {
                d.get(r.offset..r.offset + r.size as usize).ok_or("section hors du fichier")?.to_vec()
            };
            sections.push(Section {
                name: c_string(d, names + r.name as usize)?,
                kind: r.kind,
                flags: r.flags,
                align: r.align.max(1),
                data,
                size: r.size,
                relocs: Vec::new(),
            });
        }
        let mut symbols = Vec::new();
        if let Some(st) = raw.iter().find(|r| r.kind == SHT_SYMTAB) {
            let strings = raw.get(st.link as usize).ok_or("pas de table des noms de symboles")?.offset;
            for k in 0..st.size as usize / 16 {
                let o = st.offset + 16 * k;
                let info = *d.get(o + 12).ok_or("ELF tronqué")?;
                symbols.push(Symbol {
                    name: c_string(d, strings + u32_at(d, o)? as usize)?,
                    value: u32_at(d, o + 4)?,
                    size: u32_at(d, o + 8)?,
                    bind: info >> 4,
                    kind: info & 15,
                    section: u16_at(d, o + 14)?,
                });
            }
        }
        for r in raw.iter().filter(|r| r.kind == SHT_RELA) {
            let target = r.info as usize;
            if target >= sections.len() {
                return Err("relocations d'une section inexistante".into());
            }
            for k in 0..r.size as usize / 12 {
                let o = r.offset + 12 * k;
                let info = u32_at(d, o + 4)?;
                let symbol = (info >> 8) as usize;
                if symbol >= symbols.len() {
                    return Err("relocation vers un symbole inexistant".into());
                }
                sections[target].relocs.push(Reloc { offset: u32_at(d, o)?, kind: info as u8, symbol, addend: u32_at(d, o + 8)? as i32 });
            }
        }
        Ok(Object { sections, symbols })
    }

    pub fn section_index(&self, name: &str) -> Option<usize> {
        self.sections.iter().position(|s| s.name == name)
    }

    /// The defined global (or weak) symbol of that name.
    pub fn global(&self, name: &str) -> Option<usize> {
        self.symbols.iter().position(|s| s.is_global() && s.is_defined() && s.name == name)
    }
}
