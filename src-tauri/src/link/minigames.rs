//! The minigames the mods add (mod.json "minigames"). main.dol's minigame table (GameData sMinigameInfo)
//! cannot grow where it is: a copy, the mods' minigames after the original ones, goes at the start of the
//! mods' memory (dol.rs) and main.dol is patched where it uses the table (wp-core tools/wpcore_registry.py
//! found those places: main.json). The index of a minigame in the table is its slot in the save's
//! per-minigame arrays, which have room for a few more only (`max`). Past them, the core's main.dol code
//! (`extra`: wp-core src/wp/mod/MinigameSave.cpp) keeps them in another part of the save, and the table's
//! original place (`reuse`) takes the mods' main.dol code that does not fit in the low memory (dol.rs). The
//! core's modules that list minigames themselves (the rules screens, Free Play's lists) get `wpMinigames`
//! (wp/mod/minigames.h).

use super::LinkError;
use serde::Deserialize;

/// A minigame of a mod.
#[derive(Deserialize, Clone, Debug)]
pub struct Spec {
    /// Its id (the scene "mgNNN" plays /rel/mgNNN.rel), one the game does not have.
    pub id: i32,
    /// "4p" (free-for-all), "2v2" (pairs), "1v3", "1v1" (duel), "solo".
    pub category: String,
    /// The board's kind: "A", "B", "C", or none (" ").
    #[serde(default)]
    pub kind: Option<String>,
    /// Drawn on the boards (in the random pool).
    #[serde(default = "yes")]
    pub pool: bool,
    /// The sounds of this original minigame (its sound group).
    pub sounds_of: i32,
    /// The rules page: layout/inst/mg<rules>.arc (default: its id).
    #[serde(default)]
    pub rules: Option<i32>,
    /// The controls page: layout/inst/cont<controls>.arc (an original one), none if left out; a
    /// 1v3 minigame's: the solo player's.
    #[serde(default)]
    pub controls: Option<i32>,
    /// A 1v3 minigame: the team's controls page (cont<controls_team>.arc), none if left out. Its
    /// rules page has both sides' layouts: inst<rules>_1 (the solo player) and inst<rules>_3 (the team).
    #[serde(default)]
    pub controls_team: Option<i32>,
    /// In Free Play's lists.
    #[serde(default = "yes")]
    pub free_play: bool,
}

fn yes() -> bool {
    true
}

/// What the core says of main.dol's table (main.json "minigames").
#[derive(Deserialize, Clone, Debug)]
#[allow(dead_code)] // table: the original's address, for the record
pub struct Registry {
    pub table: String,
    pub entry_size: u32,
    pub count: u32,
    pub max: u32,
    pub unroll: u32,
    pub entries: String,
    pub sites: Vec<Site>,
    /// The table's original place, free once the table is moved (none: an older core).
    #[serde(default)]
    pub reuse: Option<Reuse>,
    /// What keeps the minigames past `max` (none: an older core, `max` is the limit).
    #[serde(default)]
    pub extra: Option<Extra>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Reuse {
    pub address: String,
    pub size: u32,
}

/// The core's main.dol code for the minigames past `max`: its object (in the core's folder), how many more
/// it keeps, the symbols the launcher writes (the table's address, its number of entries) and the functions
/// it replaces.
#[derive(Deserialize, Clone, Debug)]
pub struct Extra {
    pub object: String,
    pub room: u32,
    pub table: String,
    pub count: String,
    pub replace: Vec<ExtraReplace>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ExtraReplace {
    pub at: String,
    pub expect: String,
    pub call: String,
    #[serde(default)]
    pub original: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Site {
    pub address: String,
    pub kind: String,
    pub original: String,
    #[serde(default)]
    pub addend: u32,
    #[serde(default)]
    pub offset: u32,
    #[serde(default)]
    pub data: String,
}

pub const CATEGORY_NONE: i32 = 999;

pub fn category(name: &str) -> Option<i32> {
    Some(match name {
        "4p" => 0,
        "2v2" => 1,
        "1v3" => 2,
        "1v1" => 3,
        "solo" => 4,
        _ => return None,
    })
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok()).collect()
}

fn hex32(text: &str) -> Option<u32> {
    u32::from_str_radix(text.trim_start_matches("0x"), 16).ok()
}

/// main.dol's part of the mods' minigames: the data at `at` (the table, then what it needs) and the patches.
pub struct Table {
    pub data: Vec<u8>,
    /// Its entries (without the ones that round it up).
    pub count: u32,
    /// (address, new instruction, original instruction)
    pub patches: Vec<(u32, u32, u32)>,
}

/// The mods' minigames, in load order: (mod id, minigames).
pub type Added<'a> = Vec<(&'a str, &'a [Spec])>;

fn check(registry: &Registry, original: &[u8], added: &Added) -> Result<(), LinkError> {
    let entry = registry.entry_size as usize;
    let ids: Vec<i32> = original.chunks(entry).map(|e| i32::from_be_bytes(e[..4].try_into().unwrap())).collect();
    let mut seen: Vec<(i32, &str)> = Vec::new();
    for (id, list) in added {
        for m in list.iter() {
            let fail = |msg: String| LinkError::new(msg, &[id]);
            if !(100..1000).contains(&m.id) {
                return Err(fail(format!("mini-jeu {} : numéro hors de 100..999", m.id)));
            }
            if ids.contains(&m.id) {
                return Err(fail(format!("le jeu a déjà un mini-jeu {}", m.id)));
            }
            if let Some((_, other)) = seen.iter().find(|(i, _)| *i == m.id) {
                return Err(LinkError::new(format!("le mini-jeu {} est ajouté par deux mods", m.id), &[other, id]));
            }
            if category(&m.category).is_none() {
                return Err(fail(format!("mini-jeu {} : catégorie « {} » inconnue (4p, 2v2, 1v3, 1v1, solo)", m.id, m.category)));
            }
            if !matches!(m.kind.as_deref(), None | Some(" " | "A" | "B" | "C")) {
                return Err(fail(format!("mini-jeu {} : type « {} » inconnu (A, B, C)", m.id, m.kind.as_deref().unwrap_or(""))));
            }
            if !ids.contains(&m.sounds_of) {
                return Err(fail(format!("mini-jeu {} : sounds_of {} n'est pas un mini-jeu du jeu", m.id, m.sounds_of)));
            }
            seen.push((m.id, id));
        }
    }
    let n = seen.len() as u32;
    let max = registry.max + registry.extra.as_ref().map_or(0, |e| e.room);
    if registry.count + n > max {
        let all: Vec<&str> = added.iter().filter(|(_, l)| !l.is_empty()).map(|(id, _)| *id).collect();
        return Err(LinkError::new(format!("{n} mini-jeux ajoutés, la sauvegarde n'a de place que pour {}", max - registry.count), &all));
    }
    Ok(())
}

/// main.dol's table with the mods' minigames, for the address `at`.
pub fn table(registry: &Registry, added: &Added, at: u32) -> Result<Table, LinkError> {
    let bad = |what: &str| LinkError::new(format!("main.json : {what}"), &[]);
    let original = unhex(&registry.entries).ok_or_else(|| bad("table illisible"))?;
    check(registry, &original, added)?;
    let entry = registry.entry_size as usize;
    let sound_group = |of: i32| {
        original.chunks(entry).find(|e| i32::from_be_bytes(e[..4].try_into().unwrap()) == of).map(|e| u32::from_be_bytes(e[0x1C..0x20].try_into().unwrap()))
    };

    // the table: the game's minigames, the mods', then entries no id matches up to a multiple of `unroll`
    let mut data = original.clone();
    for (_, list) in added {
        for m in list.iter() {
            let mut e = vec![0u8; entry];
            e[0..4].copy_from_slice(&m.id.to_be_bytes());
            e[4..8].copy_from_slice(&(-1i32).to_be_bytes()); // mPair
            e[8..12].copy_from_slice(&category(&m.category).unwrap().to_be_bytes()); // mGroup
            e[12..16].copy_from_slice(&(-1i32).to_be_bytes()); // mRecordIndex: none
            e[0x10] = 1; // mHigherIsBetter
            e[0x18] = m.kind.as_deref().unwrap_or(" ").as_bytes()[0]; // mType
            e[0x19] = m.pool as u8; // mInPool
            e[0x1C..0x20].copy_from_slice(&sound_group(m.sounds_of).unwrap().to_be_bytes());
            data.extend_from_slice(&e);
        }
    }
    let count = (data.len() / entry) as u32;
    let unroll = registry.unroll.max(1);
    let rounded = count.div_ceil(unroll) * unroll;
    for _ in count..rounded {
        let mut e = vec![0u8; entry];
        e[0..4].copy_from_slice(&0x7FFF_FFFFi32.to_be_bytes());
        e[4..8].copy_from_slice(&(-1i32).to_be_bytes());
        e[8..12].copy_from_slice(&CATEGORY_NONE.to_be_bytes());
        e[12..16].copy_from_slice(&(-1i32).to_be_bytes());
        e[0x1C..0x20].copy_from_slice(&(-1i32).to_be_bytes());
        data.extend_from_slice(&e);
    }

    let mut patches = Vec::new();
    for s in &registry.sites {
        let address = hex32(&s.address).ok_or_else(|| bad("adresse illisible"))?;
        let original = hex32(&s.original).ok_or_else(|| bad("instruction illisible"))?;
        let value: u32 = match s.kind.as_str() {
            "count" => count,
            "unrolled" => rounded / unroll,
            "ha" => (at.wrapping_add(s.addend).wrapping_add(0x8000)) >> 16,
            "lo" => at.wrapping_add(s.addend) & 0xFFFF,
            "after" => {
                // the data after the original table that this place reaches from the table's address
                let bytes = unhex(&s.data).ok_or_else(|| bad("données illisibles"))?;
                while data.len() % 4 != 0 {
                    data.push(0);
                }
                let off = data.len() as u32 + s.offset;
                data.extend_from_slice(&bytes);
                if off > 0x7FFF {
                    return Err(bad("table trop grande"));
                }
                off
            }
            k => return Err(bad(&format!("sorte d'emplacement « {k} » inconnue (launcher à mettre à jour ?)"))),
        };
        patches.push((address, (original & 0xFFFF_0000) | (value & 0xFFFF), original));
    }
    Ok(Table { data, count, patches })
}

/// The table's original place and bytes, free once the table is moved.
pub fn reuse(registry: &Registry) -> Option<(u32, Vec<u8>)> {
    let r = registry.reuse.as_ref()?;
    let bytes = unhex(&registry.entries)?;
    let size = r.size as usize;
    if bytes.len() < size {
        return None;
    }
    Some((hex32(&r.address)?, bytes[..size].to_vec()))
}

/// The list `wpMinigames` of the core's modules (wp/mod/minigames.h WpMinigame), ended by an id -1.
pub fn module_list(added: &Added) -> Vec<u8> {
    let mut out = Vec::new();
    for (_, list) in added {
        for m in list.iter() {
            let fields = [
                m.id,
                category(&m.category).unwrap_or(CATEGORY_NONE),
                m.rules.unwrap_or(m.id),
                m.controls.unwrap_or(-1),
                (!m.free_play) as i32,
                m.controls_team.unwrap_or(-1),
            ];
            for v in fields {
                out.extend_from_slice(&v.to_be_bytes());
            }
        }
    }
    for v in [-1i32, 0, 0, 0, 0, -1] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> Registry {
        // two original minigames of 0x20 bytes, ids 100 and 406 (sound group 7)
        let mut entries = String::new();
        for (id, group) in [(100u32, 3u32), (406, 7)] {
            let mut e = vec![0u8; 0x20];
            e[0..4].copy_from_slice(&id.to_be_bytes());
            e[0x1C..0x20].copy_from_slice(&group.to_be_bytes());
            entries.extend(e.iter().map(|b| format!("{b:02x}")));
        }
        let site = |address: &str, kind: &str, original: &str, addend: u32, offset: u32, data: &str| Site {
            address: address.into(), kind: kind.into(), original: original.into(), addend, offset, data: data.into(),
        };
        Registry {
            table: "0x80200000".into(),
            entry_size: 0x20,
            count: 2,
            max: 4,
            unroll: 4,
            entries,
            reuse: Some(Reuse { address: "0x80200000".into(), size: 0x40 }),
            extra: None,
            sites: vec![
                site("0x80010000", "count", "38000002", 0, 0, ""),
                site("0x80010004", "unrolled", "38000001", 0, 0, ""),
                site("0x80010008", "ha", "3CA08020", 0, 0, ""),
                site("0x8001000C", "lo", "38A50010", 0x10, 0, ""),
                site("0x80010010", "after", "38C51014", 0, 4, "0102030405060708"),
            ],
        }
    }

    fn spec(id: i32) -> Spec {
        Spec { id, category: "4p".into(), kind: Some("A".into()), pool: true, sounds_of: 406, rules: None, controls: Some(42), controls_team: None, free_play: true }
    }

    #[test]
    fn makes_the_longer_table() {
        let r = registry();
        let mine = [spec(444)];
        let t = table(&r, &vec![("winter", &mine[..])], 0x8000_1800).unwrap();
        // 3 entries, rounded to 4, then the data after the table
        assert_eq!(t.data.len(), 4 * 0x20 + 8);
        assert_eq!(&t.data[0x40..0x44], &444u32.to_be_bytes());
        assert_eq!(&t.data[0x5C..0x60], &7u32.to_be_bytes());
        assert_eq!(t.data[0x58], b'A');
        assert_eq!(&t.data[0x60..0x64], &0x7FFF_FFFFu32.to_be_bytes());
        assert_eq!(&t.data[0x80..], &[1, 2, 3, 4, 5, 6, 7, 8]);
        let p: Vec<(u32, u32)> = t.patches.iter().map(|p| (p.0, p.1)).collect();
        assert_eq!(p, [(0x80010000, 0x38000003), (0x80010004, 0x38000001), (0x80010008, 0x3CA08000), (0x8001000C, 0x38A51810), (0x80010010, 0x38C50084)]);
        assert_eq!(module_list(&vec![("winter", &mine[..])]).len(), 48);
    }

    #[test]
    fn refuses_what_does_not_fit() {
        let r = registry();
        let three = [spec(444), spec(445), spec(446)];
        assert!(table(&r, &vec![("a", &three[..])], 0x8000_1800).is_err());
        let one = [spec(444)];
        let e = table(&r, &vec![("a", &one[..]), ("b", &one[..])], 0x8000_1800).err().unwrap();
        assert_eq!(e.mods, ["a", "b"]);
        let old = [spec(406)];
        assert!(table(&r, &vec![("a", &old[..])], 0x8000_1800).is_err());
    }

    #[test]
    fn extra_room_raises_the_limit() {
        let mut r = registry();
        r.extra = Some(Extra { object: "minigames.o".into(), room: 2, table: "t".into(), count: "c".into(), replace: vec![] });
        let four = [spec(444), spec(445), spec(446), spec(447)];
        let t = table(&r, &vec![("a", &four[..])], 0x8000_1800).unwrap();
        assert_eq!(t.count, 6);
        let five = [spec(444), spec(445), spec(446), spec(447), spec(448)];
        let e = table(&r, &vec![("a", &five[..])], 0x8000_1800).err().unwrap();
        assert!(e.message.contains("que pour 4"), "{}", e.message);
        let (at, bytes) = reuse(&r).unwrap();
        assert_eq!((at, bytes.len()), (0x8020_0000, 0x40));
    }
}
