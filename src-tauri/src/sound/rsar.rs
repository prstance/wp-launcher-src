//! NW4R sound archive (RSAR, the game's .brsar): read, edit, write (wp-sound tools/wpsound/rsar.py,
//! which this follows step by step: both must give the same bytes).
//!
//! The archive is kept as it is on disc and only what an edit touches is rebuilt, so writing an
//! unedited archive gives back the same bytes (wp_sound.brsar and McVoice.brsar both do, rebuilt
//! whole too).
//!
//! Layout (all big endian):
//!   header 0x40: "RSAR", BOM, version, file size, header size, 3 sections (offset, size)
//!   SYMB: offsets (string table, then the sound / player / group / bank name trees), the names,
//!         the trees. A tree is a patricia trie over the names' bits; the game looks names up with
//!         it (SoundArchive::ConvertLabelStringToSoundId / ...GroupId), so it is rebuilt for every
//!         name added.
//!   INFO: 6 references (relative to INFO + 8): sounds, banks, players, files, groups tables, then
//!         the sound archive player info. Each table is followed by its entries ("blocks"); a block
//!         holds an entry and what it references (3D params, details, file positions, group items).
//!   FILE: each group's data in group order: the main files (RWSD, RSEQ, RBNK) of its items, then
//!         their wave archives (RWAR). A file in several groups is written in each.
//!
//! New sounds, banks and files go after the game's (its ids don't move); a new group goes before
//! the archive's last one (add_group).

pub const NONE: u32 = 0xFFFF_FFFF;

pub const SOUND_SEQ: u8 = 1;
pub const SOUND_STRM: u8 = 2;
pub const SOUND_WAVE: u8 = 3;

pub fn u32_at(d: &[u8], o: usize) -> u32 {
    u32::from_be_bytes(d[o..o + 4].try_into().unwrap())
}

pub fn put_u32(d: &mut [u8], o: usize, v: u32) {
    d[o..o + 4].copy_from_slice(&v.to_be_bytes());
}

pub fn align(n: usize, a: usize) -> usize {
    (n + a - 1) & !(a - 1)
}

/// A u32 read that may fall outside the data (the disc's archive is checked as it is read).
fn read(d: &[u8], o: usize) -> Result<u32, String> {
    d.get(o..o + 4).map(|b| u32::from_be_bytes(b.try_into().unwrap())).ok_or_else(|| format!("archive tronquée (0x{o:X})"))
}

fn slice(d: &[u8], from: usize, to: usize) -> Result<&[u8], String> {
    d.get(from..to).ok_or_else(|| format!("archive tronquée (0x{from:X}..0x{to:X})"))
}

/// A data reference (type 1 = offset) at `o`: its offset, None when null or 0.
fn ref_at(d: &[u8], o: usize) -> Option<u32> {
    let r = d.get(o..o + 8)?;
    // (a null reference, or one to offset 0, means nothing: as rsar.py's truth tests)
    let (rt, v) = (r[0], u32_at(r, 4));
    (rt == 1 && v != 0).then_some(v)
}

/// A reference written as the game's: type 1, offset.
fn put_ref(d: &mut [u8], o: usize, v: u32) {
    d[o..o + 4].copy_from_slice(&[1, 0, 0, 0]);
    put_u32(d, o + 4, v);
}

/// The name in the strings, for messages.
pub fn latin1_str(s: &[u8]) -> String {
    s.iter().map(|&b| b as char).collect()
}

/// A label as the archive's bytes (latin-1).
pub fn latin1(s: &str) -> Result<Vec<u8>, String> {
    s.chars().map(|c| u8::try_from(c as u32).map_err(|_| format!("nom « {s} » : caractère {c} impossible"))).collect()
}

// ===================================================================================== names

fn bit(s: &[u8], b: usize) -> bool {
    s.get(b >> 3).is_some_and(|c| (c >> (7 - (b & 7))) & 1 != 0)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Node {
    pub flags: u16,
    pub bit: u16,
    pub left: u32,
    pub right: u32,
    pub string: u32,
    pub id: u32,
}

/// Patricia trie of a SYMB name list, built the way Nintendo's tool does it (the same nodes in the
/// same order: each name inserted in id order adds its leaf, then one inner node).
#[derive(Default)]
pub struct Trie {
    root: u32,
    pub nodes: Vec<Node>,
    /// Name of each leaf node (empty for inner nodes).
    keys: Vec<Vec<u8>>,
}

impl Trie {
    const LEAF: u16 = 1;

    fn leaf_for(&self, key: &[u8]) -> usize {
        let mut n = self.root as usize;
        while self.nodes[n].flags & Self::LEAF == 0 {
            let node = &self.nodes[n];
            n = if bit(key, node.bit as usize) { node.right } else { node.left } as usize;
        }
        n
    }

    pub fn insert(&mut self, key: &[u8], string: u32, id: u32) -> Result<(), String> {
        let leaf = Node { flags: Self::LEAF, bit: 0xFFFF, left: NONE, right: NONE, string, id };
        if self.nodes.is_empty() {
            self.nodes.push(leaf);
            self.keys.push(key.to_vec());
            self.root = 0;
            return Ok(());
        }
        let other = &self.keys[self.leaf_for(key)];
        if other == key {
            return Err(format!("nom en double : {}", latin1_str(key)));
        }
        let mut crit = 0;
        while bit(key, crit) == bit(other, crit) {
            crit += 1;
        }
        self.nodes.push(leaf);
        self.keys.push(key.to_vec());
        let leaf_index = (self.nodes.len() - 1) as u32;
        let (mut parent, mut right, mut cur) = (None, false, self.root as usize);
        while self.nodes[cur].flags & Self::LEAF == 0 && (self.nodes[cur].bit as usize) < crit {
            parent = Some(cur);
            right = bit(key, self.nodes[cur].bit as usize);
            cur = if right { self.nodes[cur].right } else { self.nodes[cur].left } as usize;
        }
        let (left, right_child) = if bit(key, crit) { (cur as u32, leaf_index) } else { (leaf_index, cur as u32) };
        self.nodes.push(Node { flags: 0, bit: crit as u16, left, right: right_child, string: NONE, id: NONE });
        self.keys.push(Vec::new());
        let inner = (self.nodes.len() - 1) as u32;
        match parent {
            None => self.root = inner,
            Some(p) if right => self.nodes[p].right = inner,
            Some(p) => self.nodes[p].left = inner,
        }
        Ok(())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + 0x14 * self.nodes.len());
        out.extend_from_slice(&(if self.nodes.is_empty() { NONE } else { self.root }).to_be_bytes());
        out.extend_from_slice(&(self.nodes.len() as u32).to_be_bytes());
        for n in &self.nodes {
            out.extend_from_slice(&n.flags.to_be_bytes());
            out.extend_from_slice(&n.bit.to_be_bytes());
            for v in [n.left, n.right, n.string, n.id] {
                out.extend_from_slice(&v.to_be_bytes());
            }
        }
        out
    }

    /// entries: (name, string index, id) in id order.
    pub fn build<'a>(entries: impl IntoIterator<Item = (&'a [u8], u32, u32)>) -> Result<Trie, String> {
        let mut t = Trie::default();
        for (key, si, id) in entries {
            t.insert(key, si, id)?;
        }
        Ok(t)
    }
}

#[cfg(test)]
impl Trie {
    /// The id of a name, as the game looks it up.
    pub fn lookup(&self, key: &[u8]) -> Option<u32> {
        if self.nodes.is_empty() {
            return None;
        }
        let n = self.leaf_for(key);
        (self.keys[n] == key).then_some(self.nodes[n].id)
    }
}

// ===================================================================================== INFO blocks

/// An INFO entry and what it owns, as raw bytes, with the places of its references (offsets in the
/// block, relative to its start once read) so it can be moved: a reference inside the block follows it.
#[derive(Clone)]
pub struct Block {
    pub data: Vec<u8>,
    refs: Vec<usize>,
}

impl Block {
    fn placed(&self, old_pos: u32, new_pos: u32) -> Vec<u8> {
        let mut out = self.data.clone();
        for &r in &self.refs {
            let v = u32_at(&out, r + 4);
            if v != 0 {
                put_u32(&mut out, r + 4, v.wrapping_sub(old_pos).wrapping_add(new_pos));
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
enum Fmt {
    U8,
    U16,
    U32,
    I32,
}

/// A sound's INFO fields: name, offset in the sound info.
const SOUND_FIELDS: &[(&str, usize, Fmt)] = &[
    ("string", 0x00, Fmt::U32),
    ("file", 0x04, Fmt::U32),
    ("player", 0x08, Fmt::U32),
    ("volume", 0x14, Fmt::U8),
    ("priority", 0x15, Fmt::U8),
    ("type", 0x16, Fmt::U8),
    ("remote_filter", 0x17, Fmt::U8),
    ("user0", 0x20, Fmt::U32),
    ("user1", 0x24, Fmt::U32),
    ("pan_mode", 0x28, Fmt::U8),
    ("pan_curve", 0x29, Fmt::U8),
    ("actor_player", 0x2A, Fmt::U8),
];
/// Its 3D parameters (offset in them).
const SOUND3D_FIELDS: &[(&str, usize, Fmt)] = &[("flags3d", 0, Fmt::U32), ("decay_curve", 4, Fmt::U8), ("decay_ratio", 5, Fmt::U8), ("doppler", 6, Fmt::U8)];
/// Its details, by sound type (offset in them).
const SEQ_FIELDS: &[(&str, usize, Fmt)] =
    &[("data_offset", 0, Fmt::U32), ("bank", 4, Fmt::U32), ("alloc_track", 8, Fmt::U32), ("channel_priority", 12, Fmt::U8), ("release_priority_fix", 13, Fmt::U8)];
const STRM_FIELDS: &[(&str, usize, Fmt)] = &[("start", 0, Fmt::U32), ("alloc_channel", 4, Fmt::U16), ("alloc_track", 6, Fmt::U16)];
const WAVE_FIELDS: &[(&str, usize, Fmt)] = &[("sub", 0, Fmt::I32), ("alloc_track", 4, Fmt::U32), ("channel_priority", 8, Fmt::U8), ("release_priority_fix", 9, Fmt::U8)];

fn detail_fields(kind: u8) -> &'static [(&'static str, usize, Fmt)] {
    match kind {
        SOUND_SEQ => SEQ_FIELDS,
        SOUND_STRM => STRM_FIELDS,
        SOUND_WAVE => WAVE_FIELDS,
        _ => &[],
    }
}

fn find(fields: &[(&str, usize, Fmt)], name: &str) -> Option<(usize, Fmt)> {
    fields.iter().find(|f| f.0 == name).map(|f| (f.1, f.2))
}

pub struct Archive {
    d: Vec<u8>,
    header: Vec<u8>,
    symb_raw: std::ops::Range<usize>,
    pub strings: Vec<Vec<u8>>,
    /// The name trees as read (sounds, players, groups, banks).
    pub trees: Vec<Vec<Node>>,
    pub sounds: Vec<Block>,
    pub banks: Vec<Block>,
    pub players: Vec<Block>,
    pub files: Vec<Block>,
    pub groups: Vec<Block>,
    player_info: Vec<u8>,
    file_head: Vec<u8>,
    /// Each file's main data and wave archive (None: in no group, an external file).
    pub main: Vec<Option<Vec<u8>>>,
    pub wave: Vec<Option<Vec<u8>>>,
    pub dirty_symb: bool,
    pub dirty_info: bool,
    pub dirty_file: bool,
}

impl Archive {
    pub fn parse(data: Vec<u8>) -> Result<Archive, String> {
        if data.get(..4) != Some(b"RSAR") {
            return Err("pas un RSAR".into());
        }
        let symb = read(&data, 0x10)? as usize;
        let header = slice(&data, 0, symb)?.to_vec();
        let (info, file) = (read(&data, 0x18)? as usize, read(&data, 0x20)? as usize);
        let mut a = Archive {
            d: Vec::new(),
            header,
            symb_raw: 0..0,
            strings: Vec::new(),
            trees: Vec::new(),
            sounds: Vec::new(),
            banks: Vec::new(),
            players: Vec::new(),
            files: Vec::new(),
            groups: Vec::new(),
            player_info: Vec::new(),
            file_head: Vec::new(),
            main: Vec::new(),
            wave: Vec::new(),
            dirty_symb: false,
            dirty_info: false,
            dirty_file: false,
        };
        a.read_symb(&data, symb)?;
        a.read_info(&data, info)?;
        a.read_file(&data, file)?;
        a.d = data;
        Ok(a)
    }

    // ------------------------------------------------------------------------- SYMB
    fn read_symb(&mut self, d: &[u8], symb: usize) -> Result<(), String> {
        self.symb_raw = symb..symb + read(d, symb + 4)? as usize;
        slice(d, self.symb_raw.start, self.symb_raw.end)?;
        let sb = symb + 8;
        let st = sb + read(d, sb)? as usize;
        for i in 0..read(d, st)? as usize {
            let o = sb + read(d, st + 4 + 4 * i)? as usize;
            let end = d.get(o..).and_then(|s| s.iter().position(|&c| c == 0)).ok_or("nom sans fin")?;
            self.strings.push(d[o..o + end].to_vec());
        }
        for k in 0..4 {
            let t = sb + read(d, sb + 4 + 4 * k)? as usize;
            let n = read(d, t + 4)? as usize;
            let raw = slice(d, t + 8, t + 8 + 0x14 * n)?;
            let nodes = raw
                .chunks(0x14)
                .map(|c| Node {
                    flags: u16::from_be_bytes([c[0], c[1]]),
                    bit: u16::from_be_bytes([c[2], c[3]]),
                    left: u32_at(c, 4),
                    right: u32_at(c, 8),
                    string: u32_at(c, 12),
                    id: u32_at(c, 16),
                })
                .collect();
            self.trees.push(nodes);
        }
        Ok(())
    }

    fn symb_bytes(&self) -> Result<Vec<u8>, String> {
        let n = self.strings.len();
        let table = 0x14usize;
        let first = table + 4 + 4 * n;
        let mut names = Vec::new();
        let mut offs = Vec::with_capacity(n);
        for s in &self.strings {
            offs.push((first + names.len()) as u32);
            names.extend_from_slice(s);
            names.push(0);
        }
        let mut body = (table as u32).to_be_bytes().to_vec();
        body.extend_from_slice(&[0; 16]);
        body.extend_from_slice(&(n as u32).to_be_bytes());
        for o in offs {
            body.extend_from_slice(&o.to_be_bytes());
        }
        body.extend_from_slice(&names);
        body.resize(align(body.len(), 4), 0);
        for (k, tree) in self.name_trees()?.iter().enumerate() {
            let at = body.len() as u32;
            put_u32(&mut body, 4 + 4 * k, at);
            body.extend_from_slice(&tree.to_bytes());
        }
        let size = align(8 + body.len(), 0x20);
        let mut out = b"SYMB".to_vec();
        out.extend_from_slice(&(size as u32).to_be_bytes());
        out.extend_from_slice(&body);
        out.resize(size, 0);
        Ok(out)
    }

    /// The name trees of the archive as it is now (sounds, players, groups, banks).
    pub fn name_trees(&self) -> Result<[Trie; 4], String> {
        let names = |ids: Vec<u32>| Trie::build(ids.into_iter().enumerate().filter(|(_, si)| *si != NONE).map(|(i, si)| (self.strings[si as usize].as_slice(), si, i as u32)));
        let first = |blocks: &[Block]| blocks.iter().map(|b| u32_at(&b.data, 0)).collect::<Vec<u32>>();
        let sounds = (0..self.sounds.len()).map(|i| self.sound_field(i, "string").map(|v| v as u32)).collect::<Result<Vec<_>, _>>()?;
        if let Some(bad) = sounds.iter().chain(&first(&self.players)).chain(&first(&self.groups)).chain(&first(&self.banks)).find(|&&si| si != NONE && si as usize >= self.strings.len()) {
            return Err(format!("nom {bad} hors de la table des noms"));
        }
        Ok([names(sounds)?, names(first(&self.players))?, names(first(&self.groups))?, names(first(&self.banks))?])
    }

    fn add_string(&mut self, s: Vec<u8>) -> u32 {
        self.strings.push(s);
        self.dirty_symb = true;
        (self.strings.len() - 1) as u32
    }

    // ------------------------------------------------------------------------- INFO
    fn read_info(&mut self, d: &[u8], info: usize) -> Result<(), String> {
        let ib = info + 8;
        let info_size = read(d, info + 4)? as usize;
        let tabs: Vec<usize> = (0..6).map(|k| read(d, ib + 8 * k + 4).map(|v| v as usize)).collect::<Result<_, _>>()?;
        type Refs = fn(&[u8], usize) -> Vec<usize>;
        // (references of a table at `at` in raw, the table's own entries)
        fn table_refs(raw: &[u8], at: Option<usize>) -> Vec<usize> {
            match at {
                Some(at) if at + 4 <= raw.len() => (0..u32_at(raw, at) as usize).map(|i| at + 4 + 8 * i).collect(),
                _ => vec![],
            }
        }
        let sound_refs: Refs = |raw, _| [0x0C, 0x18].into_iter().filter(|&r| ref_at(raw, r).is_some()).collect();
        let none: Refs = |_, _| vec![];
        let file_refs: Refs = |raw, o| {
            let mut out: Vec<usize> = [0x0C, 0x14].into_iter().filter(|&r| ref_at(raw, r).is_some()).collect();
            out.extend(table_refs(raw, ref_at(raw, 0x14).map(|pt| (pt as usize).wrapping_sub(o))));
            out
        };
        let group_refs: Refs = |raw, o| {
            let mut out: Vec<usize> = [0x08, 0x20].into_iter().filter(|&r| ref_at(raw, r).is_some()).collect();
            out.extend(table_refs(raw, ref_at(raw, 0x20).map(|it| (it as usize).wrapping_sub(o))));
            out
        };
        let entries = |k: usize| -> Result<Vec<usize>, String> {
            let t = tabs[k];
            (0..read(d, ib + t)? as usize).map(|i| read(d, ib + t + 8 + 8 * i).map(|v| v as usize)).collect()
        };
        // Each entry ends at the next thing of the block (an entry or a table), not at the next entry of
        // its table: an edited archive may have a table moved to the end (Winter Mode's, from the studio).
        let mut bounds: Vec<usize> = tabs.clone();
        for k in 0..5 {
            bounds.extend(entries(k)?);
        }
        bounds.push(info_size.saturating_sub(8));
        bounds.sort_unstable();
        bounds.dedup();
        let blocks = |k: usize, refs_of: Refs| -> Result<Vec<Block>, String> {
            let offs = entries(k)?;
            let mut out = Vec::new();
            for (i, &o) in offs.iter().enumerate() {
                let end = bounds.iter().copied().find(|&b| b > o).unwrap_or(o);
                let raw = slice(d, ib + o, ib + end)?;
                let refs = refs_of(raw, o);
                if refs.iter().any(|&r| r + 8 > raw.len()) {
                    return Err(format!("entrée INFO {k}/{i} invalide"));
                }
                // references made relative to the block's start
                let mut block = Block { data: raw.to_vec(), refs };
                for &r in &block.refs {
                    let v = u32_at(&block.data, r + 4);
                    if v != 0 {
                        put_u32(&mut block.data, r + 4, v.wrapping_sub(o as u32));
                    }
                }
                out.push(block);
            }
            Ok(out)
        };
        let sizes_ok = |blocks: &[Block], min: usize| blocks.iter().all(|b| b.data.len() >= min);
        self.sounds = blocks(0, sound_refs)?;
        self.banks = blocks(1, none)?;
        self.players = blocks(2, none)?;
        self.files = blocks(3, file_refs)?;
        self.groups = blocks(4, group_refs)?;
        if !sizes_ok(&self.sounds, 0x2C) || !sizes_ok(&self.banks, 0x0C) || !sizes_ok(&self.players, 4) || !sizes_ok(&self.files, 0x1C) || !sizes_ok(&self.groups, 0x28) {
            return Err("entrée INFO trop courte".into());
        }
        self.player_info = slice(d, ib + tabs[5], info + info_size)?.to_vec();
        Ok(())
    }

    fn info_bytes(&self) -> Vec<u8> {
        let mut body = vec![0u8; 0x30];
        for (k, blocks) in [&self.sounds, &self.banks, &self.players, &self.files, &self.groups].into_iter().enumerate() {
            let at = body.len() as u32;
            put_ref(&mut body, 8 * k, at);
            let table = body.len();
            body.extend_from_slice(&(blocks.len() as u32).to_be_bytes());
            body.resize(body.len() + 8 * blocks.len(), 0);
            for (i, b) in blocks.iter().enumerate() {
                let at = body.len() as u32;
                put_ref(&mut body, table + 4 + 8 * i, at);
                body.extend_from_slice(&b.placed(0, at));
            }
        }
        let at = body.len() as u32;
        put_ref(&mut body, 0x28, at);
        body.extend_from_slice(&self.player_info);
        let size = align(8 + body.len(), 0x20);
        let mut out = b"INFO".to_vec();
        out.extend_from_slice(&(size as u32).to_be_bytes());
        out.extend_from_slice(&body);
        out.resize(size, 0);
        out
    }

    // sound fields -------------------------------------------------------------
    fn sound_part(&self, i: usize, name: &str) -> Result<(usize, Fmt), String> {
        let data = &self.sounds[i].data;
        if let Some(f) = find(SOUND_FIELDS, name) {
            return Ok(f);
        }
        let (part, (off, fmt)) = if let Some(f) = find(SOUND3D_FIELDS, name) {
            (0x0C, f)
        } else if let Some(f) = find(detail_fields(data[0x16]), name) {
            (0x18, f)
        } else {
            return Err(format!("champ {name} inconnu pour ce type de son"));
        };
        let at = u32_at(data, part + 4) as usize;
        if ref_at(data, part).is_none() || at + off + 4 > data.len() {
            return Err(format!("champ {name} absent de ce son"));
        }
        Ok((at + off, fmt))
    }

    pub fn sound_field(&self, i: usize, name: &str) -> Result<i64, String> {
        let (off, fmt) = self.sound_part(i, name)?;
        let d = &self.sounds[i].data;
        Ok(match fmt {
            Fmt::U8 => d[off] as i64,
            Fmt::U16 => u16::from_be_bytes([d[off], d[off + 1]]) as i64,
            Fmt::U32 => u32_at(d, off) as i64,
            Fmt::I32 => u32_at(d, off) as i32 as i64,
        })
    }

    pub fn set_sound(&mut self, i: usize, name: &str, v: i64) -> Result<(), String> {
        let (off, fmt) = self.sound_part(i, name)?;
        let d = &mut self.sounds[i].data;
        let bad = || format!("{name} : {v} hors limites");
        match fmt {
            Fmt::U8 => d[off] = u8::try_from(v).map_err(|_| bad())?,
            Fmt::U16 => d[off..off + 2].copy_from_slice(&u16::try_from(v).map_err(|_| bad())?.to_be_bytes()),
            Fmt::U32 => put_u32(d, off, u32::try_from(v).map_err(|_| bad())?),
            Fmt::I32 => put_u32(d, off, i32::try_from(v).map_err(|_| bad())? as u32),
        }
        self.dirty_info = true;
        Ok(())
    }

    // names -----------------------------------------------------------------------
    fn is_named(&self, si: u32, label: &[u8]) -> bool {
        si != NONE && self.strings.get(si as usize).is_some_and(|s| s == label)
    }

    pub fn sound_id(&self, label: &str) -> Option<usize> {
        let key = latin1(label).ok()?;
        (0..self.sounds.len()).find(|&i| self.sound_field(i, "string").is_ok_and(|si| self.is_named(si as u32, &key)))
    }

    fn index_of(&self, blocks: &[Block], label: &str) -> Option<usize> {
        let key = latin1(label).ok()?;
        blocks.iter().position(|b| self.is_named(u32_at(&b.data, 0), &key))
    }

    pub fn group_id(&self, label: &str) -> Option<usize> {
        self.index_of(&self.groups, label)
    }

    pub fn bank_id(&self, label: &str) -> Option<usize> {
        self.index_of(&self.banks, label)
    }

    pub fn player_id(&self, label: &str) -> Option<usize> {
        self.index_of(&self.players, label)
    }

    pub fn bank_file(&self, i: usize) -> u32 {
        u32_at(&self.banks[i].data, 4)
    }

    /// (group, item index) of a file.
    pub fn file_groups(&self, fid: usize) -> Vec<(u32, u32)> {
        let b = &self.files[fid].data;
        let Some(pt) = ref_at(b, 0x14) else { return vec![] };
        let pt = pt as usize;
        (0..u32_at(b, pt) as usize)
            .map(|i| {
                let e = u32_at(b, pt + 4 + 8 * i + 4) as usize;
                (u32_at(b, e), u32_at(b, e + 4))
            })
            .collect()
    }

    /// The file id of each of a group's items.
    pub fn group_items(&self, g: usize) -> Vec<u32> {
        let b = &self.groups[g].data;
        let it = u32_at(b, 0x24) as usize;
        (0..u32_at(b, it) as usize).map(|i| u32_at(b, u32_at(b, it + 4 + 8 * i + 4) as usize)).collect()
    }

    // ------------------------------------------------------------------------- adding
    fn check_new(&self, label: &str, there: Option<usize>) -> Result<Vec<u8>, String> {
        if there.is_some() {
            return Err(format!("{label} existe déjà dans l'archive"));
        }
        latin1(label)
    }

    /// A new file (not yet in a group: add_group puts it in one).
    pub fn add_file(&mut self, main: Vec<u8>, wave: Vec<u8>) -> Result<u32, String> {
        let fid = self.files.len();
        let mut data = vec![0u8; 0x1C];
        put_u32(&mut data, 8, NONE);
        self.files.push(Block { data, refs: vec![0x0C, 0x14] });
        self.set_positions(fid, &[])?;
        self.main.push(Some(Vec::new()));
        self.wave.push(Some(Vec::new()));
        self.replace_file(fid, Some(main), Some(wave))?;
        self.dirty_info = true;
        Ok(fid as u32)
    }

    /// Rewrites a file's block with its (group, item index) list. Internal files only.
    fn set_positions(&mut self, fid: usize, positions: &[(u32, u32)]) -> Result<(), String> {
        let b = &self.files[fid];
        if ref_at(&b.data, 0x0C).is_some() {
            return Err(format!("fichier {fid} externe"));
        }
        let mut data = b.data[..0x1C].to_vec();
        let table = data.len();
        data.extend_from_slice(&(positions.len() as u32).to_be_bytes());
        data.resize(data.len() + 8 * positions.len(), 0);
        put_ref(&mut data, 0x14, table as u32);
        let mut refs = vec![0x14];
        for (i, &(g, item)) in positions.iter().enumerate() {
            let at = data.len() as u32;
            put_ref(&mut data, table + 4 + 8 * i, at);
            refs.push(table + 4 + 8 * i);
            data.extend_from_slice(&g.to_be_bytes());
            data.extend_from_slice(&item.to_be_bytes());
        }
        self.files[fid] = Block { data, refs };
        Ok(())
    }

    pub fn add_bank(&mut self, label: &str, fid: u32) -> Result<usize, String> {
        let key = self.check_new(label, self.bank_id(label))?;
        let si = self.add_string(key);
        let mut data = vec![0u8; 0x0C];
        put_u32(&mut data, 0, si);
        put_u32(&mut data, 4, fid);
        self.banks.push(Block { data, refs: vec![] });
        self.dirty_info = true;
        Ok(self.banks.len() - 1)
    }

    /// A new sound: a copy of the first sound of the same type (its 3D params and details
    /// included), with these fields (SOUND_FIELDS, SOUND3D_FIELDS, the type's details).
    pub fn add_sound(&mut self, label: &str, kind: u8, fid: u32, player: usize, fields: &[(String, i64)]) -> Result<usize, String> {
        let key = self.check_new(label, self.sound_id(label))?;
        let template = self.sounds.iter().find(|b| b.data[0x16] == kind).ok_or_else(|| format!("aucun son de type {kind} à copier"))?.clone();
        self.sounds.push(template);
        let sid = self.sounds.len() - 1;
        let si = self.add_string(key);
        for (name, v) in [("string", si as i64), ("file", fid as i64), ("player", player as i64), ("user0", 0), ("user1", 0)] {
            self.set_sound(sid, name, v)?;
        }
        for (name, v) in fields {
            self.set_sound(sid, name, *v)?;
        }
        Ok(sid)
    }

    /// A new group of these files (new ones or files of the archive: a file of several groups is
    /// written in each).
    ///
    /// It goes before the archive's last group: that one is a nameless, empty group the runtime
    /// doesn't count (SoundArchiveFileReader::GetGroupCount returns the table's count - 1), so a
    /// group after it can't be loaded. No file position or name refers to that last group (it has no
    /// item and no name), so moving it is safe.
    pub fn add_group(&mut self, label: &str, fids: &[u32]) -> Result<usize, String> {
        let key = self.check_new(label, self.group_id(label))?;
        let last = self.groups.len().checked_sub(1).ok_or("archive sans groupe")?;
        if u32_at(&self.groups[last].data, 0) != NONE || !self.group_items(last).is_empty() {
            return Err("le dernier groupe de l'archive n'est pas le groupe vide attendu".into());
        }
        for &fid in fids {
            if self.main.get(fid as usize).is_none_or(|m| m.is_none()) {
                return Err(format!("fichier {fid} sans données dans l'archive"));
            }
        }
        let gid = last;
        let si = self.add_string(key);
        let mut data = vec![0u8; 0x28];
        put_u32(&mut data, 0, si);
        put_u32(&mut data, 4, NONE);
        let table = data.len();
        data.extend_from_slice(&(fids.len() as u32).to_be_bytes());
        data.resize(data.len() + 8 * fids.len(), 0);
        put_ref(&mut data, 0x20, table as u32);
        let mut refs = vec![0x20];
        for (i, &fid) in fids.iter().enumerate() {
            let at = data.len() as u32;
            put_ref(&mut data, table + 4 + 8 * i, at);
            refs.push(table + 4 + 8 * i);
            data.extend_from_slice(&fid.to_be_bytes());
            data.extend_from_slice(&[0; 20]);
            let mut positions = self.file_groups(fid as usize);
            positions.push((gid as u32, i as u32));
            self.set_positions(fid as usize, &positions)?;
        }
        self.groups.insert(gid, Block { data, refs });
        self.dirty_info = true;
        self.dirty_file = true;
        Ok(gid)
    }

    /// Bytes the group takes in the sound heap (its files and waves).
    pub fn group_bytes(&self, g: usize) -> usize {
        let len = |x: &Option<Vec<u8>>| x.as_ref().map_or(0, Vec::len);
        self.group_items(g).iter().map(|&f| len(&self.main[f as usize]) + len(&self.wave[f as usize])).sum()
    }

    // ------------------------------------------------------------------------- FILE
    fn read_file(&mut self, d: &[u8], file: usize) -> Result<(), String> {
        self.file_head = slice(d, file, file + 0x20)?.to_vec();
        self.main = vec![None; self.files.len()];
        self.wave = vec![None; self.files.len()];
        for g in 0..self.groups.len() {
            let b = &self.groups[g].data;
            let (go, gwo) = (u32_at(b, 0x10) as usize, u32_at(b, 0x18) as usize);
            let it = u32_at(b, 0x24) as usize;
            for i in 0..read(b, it)? as usize {
                let q = read(b, it + 4 + 8 * i + 4)? as usize;
                let v: Vec<usize> = (0..5).map(|k| read(b, q + 4 * k).map(|x| x as usize)).collect::<Result<_, _>>()?;
                let (fid, off, size, woff, wsize) = (v[0], v[1], v[2], v[3], v[4]);
                if fid >= self.files.len() {
                    return Err(format!("groupe {g} : fichier {fid} inconnu"));
                }
                if self.main[fid].is_none() {
                    self.main[fid] = Some(slice(d, go + off, go + off + size)?.to_vec());
                    self.wave[fid] = Some(slice(d, gwo + woff, gwo + woff + wsize)?.to_vec());
                }
            }
        }
        Ok(())
    }

    pub fn replace_file(&mut self, fid: usize, main: Option<Vec<u8>>, wave: Option<Vec<u8>>) -> Result<(), String> {
        if [&main, &wave].into_iter().flatten().any(|x| x.len() % 0x20 != 0) {
            return Err(format!("fichier {fid} : taille non alignée sur 0x20"));
        }
        if let Some(m) = main {
            self.main[fid] = Some(m);
        }
        if let Some(w) = wave {
            self.wave[fid] = Some(w);
        }
        self.dirty_file = true;
        Ok(())
    }

    fn file_bytes(&mut self, file_sec: usize) -> Vec<u8> {
        let mut body = Vec::new();
        let base = file_sec + 0x20;
        for g in 0..self.groups.len() {
            let b = &self.groups[g].data;
            let it = u32_at(b, 0x24) as usize;
            let items: Vec<usize> = (0..u32_at(b, it) as usize).map(|i| u32_at(b, it + 4 + 8 * i + 4) as usize).collect();
            let g_off = base + body.len();
            for &q in &items {
                let fid = u32_at(&self.groups[g].data, q) as usize;
                let m = self.main[fid].as_deref().unwrap_or_default();
                let b = &mut self.groups[g].data;
                put_u32(b, q + 4, (base + body.len() - g_off) as u32);
                put_u32(b, q + 8, m.len() as u32);
                body.extend_from_slice(m);
            }
            let g_wave = base + body.len();
            for &q in &items {
                let fid = u32_at(&self.groups[g].data, q) as usize;
                // (an item without waves has offset 0, as in the game)
                let w = self.wave[fid].as_deref().unwrap_or_default();
                let b = &mut self.groups[g].data;
                put_u32(b, q + 12, if w.is_empty() { 0 } else { (base + body.len() - g_wave) as u32 });
                put_u32(b, q + 16, w.len() as u32);
                body.extend_from_slice(w);
            }
            let b = &mut self.groups[g].data;
            put_u32(b, 0x10, g_off as u32);
            put_u32(b, 0x14, (g_wave - g_off) as u32);
            put_u32(b, 0x18, g_wave as u32);
            put_u32(b, 0x1C, (base + body.len() - g_wave) as u32);
        }
        for (fid, f) in self.files.iter_mut().enumerate() {
            if let (Some(m), Some(w)) = (&self.main[fid], &self.wave[fid]) {
                put_u32(&mut f.data, 0, m.len() as u32);
                put_u32(&mut f.data, 4, w.len() as u32);
            }
        }
        let mut out = self.file_head.clone();
        put_u32(&mut out, 4, (0x20 + body.len()) as u32);
        out.extend_from_slice(&body);
        out
    }

    // ------------------------------------------------------------------------- whole archive
    pub fn to_bytes(&mut self) -> Result<Vec<u8>, String> {
        if !(self.dirty_info || self.dirty_symb || self.dirty_file) {
            return Ok(self.d.clone());
        }
        let symb = if self.dirty_symb { self.symb_bytes()? } else { self.d[self.symb_raw.clone()].to_vec() };
        // (its size only: FILE's offsets depend on it)
        let info_size = self.info_bytes().len();
        let file_sec = self.header.len() + symb.len() + info_size;
        let file = self.file_bytes(file_sec);
        // (with the group offsets file_bytes wrote)
        let info = self.info_bytes();
        let mut out = self.header.clone();
        out.reserve(symb.len() + info.len() + file.len());
        let o = out.len();
        for (k, v) in [o, symb.len(), o + symb.len(), info.len(), o + symb.len() + info.len(), file.len()].into_iter().enumerate() {
            put_u32(&mut out, 0x10 + 4 * k, v as u32);
        }
        out.extend_from_slice(&symb);
        out.extend_from_slice(&info);
        out.extend_from_slice(&file);
        let size = out.len() as u32;
        put_u32(&mut out, 8, size);
        Ok(out)
    }
}
