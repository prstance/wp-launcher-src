//! Wii Mii data, as the Wii's Mii library (RFL) stores it: the 74-byte
//! RFLiCharData, and RFL_DB.dat, the console's database of 100 Miis that the
//! Mii Channel writes and every game reads.
//! Layout from the decompiled RFL (wp/src/RVLFaceLib, RFLi_Types.h, RFLi_Database.h).

use crate::i18n::tr;

pub const SIZE: usize = 74;
pub type Raw = [u8; SIZE];

/// RFLStoreData: a Mii and its CRC, as in .rsd files and the disc's mii_rsd.bin.
const STORE_SIZE: usize = SIZE + 2;
const NAME_LEN: usize = 10;
const DB_SLOTS: usize = 100;
const DB_MAGIC: &[u8; 4] = b"RNOD";
const HIDDEN_MAGIC: &[u8; 4] = b"RNHD";
const HIDDEN_AT: usize = 0x1D00;
/// sizeof(RFLiDatabase): what the CRC covers, ending with the CRC itself.
const DB_SIZE: usize = 0x1F1E0;
/// RFL_DB.dat as the Mii Channel creates it.
const DB_FILE_SIZE: usize = 0xBE6C0;

const ID_TEMPORARY: u32 = 0x2000_0000;
const ID_NOT_SPECIAL: u32 = 0x8000_0000;

/// RFL's CRC-16 (CCITT, data shifted in): over data followed by its CRC, it gives 0.
pub fn crc16(bytes: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in bytes {
        let mut data = byte;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
            if data & 0x80 != 0 {
                crc ^= 1;
            }
            data <<= 1;
        }
    }
    crc
}

/// `width` bits of the big-endian u16 at `at`, `skip` bits from its top (CodeWarrior bitfields).
fn bits(raw: &Raw, at: usize, skip: u32, width: u32) -> u16 {
    let word = u16::from_be_bytes([raw[at], raw[at + 1]]);
    (word >> (16 - skip - width)) & ((1 << width) - 1)
}

fn utf16(raw: &Raw, at: usize) -> String {
    let units: Vec<u16> = (0..NAME_LEN).map(|i| u16::from_be_bytes([raw[at + i * 2], raw[at + i * 2 + 1]])).take_while(|&u| u != 0).collect();
    String::from_utf16_lossy(&units)
}

fn set_utf16(raw: &mut Raw, at: usize, text: &str) {
    let units: Vec<u16> = text.encode_utf16().take(NAME_LEN).collect();
    for i in 0..NAME_LEN {
        let unit = units.get(i).copied().unwrap_or(0).to_be_bytes();
        raw[at + i * 2..at + i * 2 + 2].copy_from_slice(&unit);
    }
}

pub fn name(raw: &Raw) -> String {
    utf16(raw, 0x02)
}

/// At most 10 characters, as on the Wii.
pub fn set_name(raw: &mut Raw, name: &str) {
    set_utf16(raw, 0x02, name.trim());
}

pub fn creator(raw: &Raw) -> String {
    utf16(raw, 0x36)
}

pub fn set_creator(raw: &mut Raw, name: &str) {
    set_utf16(raw, 0x36, name.trim());
}

pub fn female(raw: &Raw) -> bool {
    bits(raw, 0x00, 1, 1) == 1
}

/// 0 red … 11 black (RFLFavoriteColor).
pub fn favorite_color(raw: &Raw) -> u16 {
    bits(raw, 0x00, 11, 4).min(11)
}

/// (month, day), when set.
pub fn birthday(raw: &Raw) -> Option<(u16, u16)> {
    let (month, day) = (bits(raw, 0x00, 2, 4), bits(raw, 0x00, 6, 5));
    ((1..=12).contains(&month) && day >= 1).then_some((month, day))
}

/// RFLCreateID: the Mii's own 4 bytes, then the console's 4.
pub fn create_id(raw: &Raw) -> [u8; 8] {
    raw[0x18..0x20].try_into().unwrap()
}

pub fn set_create_id(raw: &mut Raw, id: [u8; 8]) {
    raw[0x18..0x20].copy_from_slice(&id);
}

/// The create ID in hex: how the launcher names a Mii.
pub fn id(raw: &Raw) -> String {
    create_id(raw).iter().map(|b| format!("{b:02x}")).collect()
}

/// A new create ID: RFL's layout (not special, not temporary, then the time in
/// 4-second steps since 2006), made unique against `taken`.
pub fn new_create_id(system: [u8; 4], now_secs: u64, taken: &[[u8; 8]]) -> [u8; 8] {
    const WII_EPOCH: u64 = 1_136_073_600; // 2006-01-01
    let mut stamp = (now_secs.saturating_sub(WII_EPOCH) / 4) as u32 & 0x1FFF_FFFF;
    loop {
        let mut id = [0; 8];
        id[..4].copy_from_slice(&(ID_NOT_SPECIAL | stamp).to_be_bytes());
        id[4..].copy_from_slice(&system);
        if !taken.contains(&id) {
            return id;
        }
        stamp = (stamp + 1) & 0x1FFF_FFFF;
    }
}

/// What RFLiCheckValidInfo checks, plus an ID a database can hold.
pub fn is_valid(raw: &Raw) -> bool {
    let id = u32::from_be_bytes(raw[0x18..0x1C].try_into().unwrap());
    if create_id(raw) == [0; 8] || id & ID_TEMPORARY != 0 {
        return false;
    }
    // (offset, bits skipped, width, max)
    const LIMITS: &[(usize, u32, u32, u16)] = &[
        (0x20, 0, 3, 7),   // faceline type
        (0x20, 3, 3, 5),   // faceline color
        (0x20, 6, 4, 11),  // faceline texture
        (0x22, 0, 7, 71),  // hair type
        (0x22, 7, 3, 7),   // hair color
        (0x24, 0, 5, 23),  // eyebrow type
        (0x24, 5, 5, 11),  // eyebrow rotate
        (0x26, 0, 3, 7),   // eyebrow color
        (0x26, 3, 4, 8),   // eyebrow scale
        (0x26, 7, 5, 18),  // eyebrow y
        (0x26, 12, 4, 12), // eyebrow x
        (0x28, 0, 6, 47),  // eye type
        (0x28, 6, 5, 7),   // eye rotate
        (0x28, 11, 5, 18), // eye y
        (0x2A, 0, 3, 5),   // eye color
        (0x2A, 3, 4, 7),   // eye scale
        (0x2A, 7, 4, 12),  // eye x
        (0x2C, 0, 4, 11),  // nose type
        (0x2C, 4, 4, 8),   // nose scale
        (0x2C, 8, 5, 18),  // nose y
        (0x2E, 0, 5, 23),  // mouth type
        (0x2E, 5, 2, 2),   // mouth color
        (0x2E, 7, 4, 8),   // mouth scale
        (0x2E, 11, 5, 18), // mouth y
        (0x30, 0, 4, 8),   // glasses type
        (0x30, 4, 3, 5),   // glasses color
        (0x30, 7, 4, 7),   // glasses scale
        (0x30, 11, 5, 20), // glasses y
        (0x32, 7, 4, 8),   // beard scale
        (0x32, 11, 5, 16), // beard y
        (0x34, 1, 4, 8),   // mole scale
        (0x34, 5, 5, 30),  // mole y
        (0x34, 10, 5, 16), // mole x
    ];
    LIMITS.iter().all(|&(at, skip, width, max)| bits(raw, at, skip, width) <= max) && bits(raw, 0x26, 7, 5) >= 3
}

fn slots(bytes: &[u8], size: usize) -> Vec<Raw> {
    bytes.chunks_exact(size).map(|c| c[..SIZE].try_into().unwrap()).filter(is_valid).collect()
}

/// Whether RFL_DB.dat is written in full: its CRC checks out.
pub fn is_whole(bytes: &[u8]) -> bool {
    bytes.len() >= DB_SIZE && &bytes[..4] == DB_MAGIC && crc16(&bytes[..DB_SIZE]) == 0
}

/// The Miis of RFL_DB.dat.
pub fn read_database(bytes: &[u8]) -> Vec<Raw> {
    if bytes.len() < DB_SIZE || &bytes[..4] != DB_MAGIC {
        return Vec::new();
    }
    slots(&bytes[4..4 + DB_SLOTS * SIZE], SIZE)
}

/// RFL_DB.dat holding `miis` (the first 100) and nothing else; what else
/// `existing` holds (the Mii Parade) is kept.
pub fn write_database(existing: Option<Vec<u8>>, miis: &[Raw]) -> Vec<u8> {
    let mut db = existing.filter(|b| b.len() >= DB_SIZE && &b[..4] == DB_MAGIC).unwrap_or_else(|| {
        let mut db = vec![0; DB_FILE_SIZE];
        db[..4].copy_from_slice(DB_MAGIC);
        db[0x1CEC] = 0x80;
        db[HIDDEN_AT..HIDDEN_AT + 4].copy_from_slice(HIDDEN_MAGIC);
        db[HIDDEN_AT + 4..HIDDEN_AT + 8].fill(0xFF);
        // RFLiClearTableData: each of the 10000 entries links to none (next and prev: -1).
        for entry in db[HIDDEN_AT + 8..].as_chunks_mut::<12>().0.iter_mut().take(10_000) {
            entry[8..].copy_from_slice(&[0x7F, 0xFF, 0x7F, 0xFF]);
        }
        db
    });
    for slot in 0..DB_SLOTS {
        let at = 4 + slot * SIZE;
        match miis.get(slot) {
            Some(mii) => db[at..at + SIZE].copy_from_slice(mii),
            None => db[at..at + SIZE].fill(0),
        }
    }
    db[DB_SIZE - 2..DB_SIZE].fill(0);
    let crc = crc16(&db[..DB_SIZE]);
    db[DB_SIZE - 2..DB_SIZE].copy_from_slice(&crc.to_be_bytes());
    db
}

/// The Miis in a file someone gives the launcher: a Mii (.mii, .miigx, .rcd),
/// a Mii with its CRC (.rsd), several of those, or a whole RFL_DB.dat.
pub fn read_file(bytes: &[u8]) -> Result<Vec<Raw>, String> {
    let miis = if bytes.len() >= DB_SIZE && &bytes[..4] == DB_MAGIC {
        read_database(bytes)
    } else if bytes.len() == SIZE {
        slots(bytes, SIZE)
    } else if bytes.len().is_multiple_of(STORE_SIZE) && !bytes.is_empty() {
        slots(bytes, STORE_SIZE)
    } else if matches!(bytes.len(), 0x5C | 0x60 | 0x88 | 0x90) {
        return Err(tr!("c’est un Mii de 3DS, de Wii U ou de Switch : seuls les Mii de Wii sont acceptés"));
    } else {
        return Err(tr!("ce n’est pas un fichier Mii de Wii"));
    };
    if miis.is_empty() {
        return Err(tr!("ce fichier ne contient aucun Mii valable"));
    }
    Ok(miis)
}

/// The disc's Miis (RFL/RSD/mii_rsd.bin): store data, one after the other.
pub fn read_store(bytes: &[u8]) -> Vec<Raw> {
    bytes.as_chunks::<STORE_SIZE>().0.iter().map(|c| c[..SIZE].try_into().unwrap()).filter(|r: &Raw| create_id(r) != [0; 8]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "Guest A" of RFL_DefaultDatabase.c, with an ID.
    fn guest() -> Raw {
        let mut raw: Raw = [
            0x00, 0x08, 0x00, 0x6E, 0x00, 0x6F, 0x00, 0x20, 0x00, 0x6E, 0x00, 0x61, 0x00, 0x6D, 0x00, 0x65, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40,
            0x40, 0x80, 0x00, 0x00, 0x00, 0xEC, 0xFF, 0x82, 0xD2, 0x10, 0x04, 0x88, 0x00, 0x31, 0x80, 0x08, 0xA2, 0x08, 0x8C, 0x08, 0x58, 0x14, 0x4A,
            0xB8, 0x8D, 0x00, 0x8A, 0x00, 0x8A, 0x25, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        set_create_id(&mut raw, [0x80, 0, 0, 1, 1, 2, 3, 4]);
        raw
    }

    #[test]
    fn reads_and_writes_fields() {
        let mut raw = guest();
        assert_eq!(name(&raw), "no name");
        assert!(!female(&raw));
        assert_eq!(favorite_color(&raw), 4);
        assert!(is_valid(&raw));
        set_name(&mut raw, "Mathéo, le roi du jeu");
        assert_eq!(name(&raw), "Mathéo, le");
        assert_eq!(id(&raw), "8000000101020304");
    }

    #[test]
    fn database_round_trip_has_a_valid_crc() {
        let db = write_database(None, &[guest()]);
        assert_eq!(db.len(), DB_FILE_SIZE);
        assert_eq!(crc16(&db[..DB_SIZE]), 0);
        assert_eq!(read_database(&db), vec![guest()]);
        // Rewritten without it: empty again, CRC still right.
        let db = write_database(Some(db), &[]);
        assert!(read_database(&db).is_empty());
        assert_eq!(crc16(&db[..DB_SIZE]), 0);
    }

    /// RFL_DB.dat as the Mii Channel made it in a fresh NAND (path in RFL_DB): ours must be the same.
    #[test]
    #[ignore]
    fn an_empty_database_is_the_channels() {
        let Ok(path) = std::env::var("RFL_DB") else { return };
        assert_eq!(write_database(None, &[]), std::fs::read(path).unwrap());
    }

    #[test]
    fn reads_files() {
        assert_eq!(read_file(&guest()).unwrap().len(), 1);
        let mut store = guest().to_vec();
        store.extend(crc16(&guest()).to_be_bytes());
        assert_eq!(read_file(&store).unwrap().len(), 1);
        assert!(read_file(&[0; 96]).is_err());
        assert!(read_file(&[0; 74]).is_err());
    }

    #[test]
    fn new_ids_are_unique_and_not_special() {
        let first = new_create_id([1, 2, 3, 4], 1_700_000_000, &[]);
        let second = new_create_id([1, 2, 3, 4], 1_700_000_000, &[first]);
        assert_ne!(first, second);
        assert!(first[0] & 0x80 != 0 && first[0] & 0x20 == 0);
    }
}
