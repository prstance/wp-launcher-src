//! The minigames the mods add (mod.json "minigames"). The core's GameData reads them from the disc file
//! /wp/minigames.bin at the start and appends them to the game's minigame table (wp-srccore
//! include/wp/mod/minigames.h, src/wp/GameData.cpp loadModMinigames): every list of the game (the boards'
//! pools, Free Play, the rules screens, the records, the sounds) has them, with no limit but memory.

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
    /// The sounds of this original minigame (its sound group), unless `sound_group` is given; one of
    /// the two is needed.
    #[serde(default)]
    pub sounds_of: Option<i32>,
    /// Its own sound group in /sound/wp_sound.brsar: its index, or its label (a group a mod's sound
    /// patch adds, "GROUP_MOD_X", found once the sound patches are applied). It wins over `sounds_of`.
    #[serde(default)]
    pub sound_group: Option<SoundGroup>,
    /// Another minigame played with it (MinigameInfo::mPair: a duel's twin), of the game or of a mod.
    #[serde(default)]
    pub pair: Option<i32>,
    /// 103, 417: the disc has /rel/mg<id>.rel without a minigame of the table; the mod's REL replaces
    /// it, only when it says so.
    #[serde(default)]
    pub replace_disc_rel: bool,
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

/// A sound group: its index in the archive, or its label.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum SoundGroup {
    Id(i32),
    Label(String),
}

/// The disc file the core reads them from (WP_MINIGAME_FILE_PATH).
pub const FILE: &str = "/wp/minigames.bin";
const MAGIC: u32 = 0x5750_4D47; // 'WPMG'
const VERSION: u32 = 1;
/// sizeof(WpMinigame)
const ENTRY_SIZE: u32 = 0x28;
// WpMinigame mFlags
const HIDDEN: u32 = 1;
const NO_POOL: u32 = 2;

/// The archive a sound group's label is looked up in.
pub const SOUND_ARCHIVE: &str = "/sound/wp_sound.brsar";

/// Ids with a REL on the disc but no minigame of the table (mg103 is mg418's twin, mg417 mg111's).
const DISC_RELS: [i32; 2] = [103, 417];

/// The ids of the game's minigame table (GameData sGameMinigameInfo).
const GAME: [i32; 88] = [
    100, 101, 102, 104, 106, 107, 108, 109, 110, 111, 112, 200, 201, 202, 203, 204, 205, 206, 208, 209, 210, 211, 212, 213, 214, 215, 216, 218,
    219, 221, 222, 223, 224, 300, 301, 302, 303, 304, 400, 401, 402, 403, 404, 405, 406, 407, 408, 410, 411, 412, 413, 414, 415, 416, 418, 419,
    420, 421, 422, 423, 424, 426, 427, 428, 430, 431, 432, 433, 434, 436, 437, 438, 439, 440, 441, 442, 443, 445, 446, 501, 502, 503, 504, 505,
    506, 507, 508, 509,
];

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

/// The mods' minigames, in load order: (mod id, minigames).
pub type Added<'a> = Vec<(&'a str, &'a [Spec])>;

/// Each minigame once, of a known category and kind, with sounds (one of the game's, or its own group),
/// its pair one of the game's or of the mods'.
pub fn check(added: &Added) -> Result<(), LinkError> {
    let all: Vec<i32> = added.iter().flat_map(|(_, l)| l.iter().map(|m| m.id)).collect();
    let mut seen: Vec<(i32, &str)> = Vec::new();
    for (id, list) in added {
        for m in list.iter() {
            let fail = |msg: String| LinkError::new(msg, &[id]);
            if !(100..1000).contains(&m.id) {
                return Err(fail(format!("mini-jeu {} : numéro hors de 100..999", m.id)));
            }
            if GAME.contains(&m.id) {
                return Err(fail(format!("le jeu a déjà un mini-jeu {}", m.id)));
            }
            if DISC_RELS.contains(&m.id) && !m.replace_disc_rel {
                return Err(fail(format!(
                    "mini-jeu {0} : le disque a déjà /rel/mg{0}.rel (sans mini-jeu dans la table du jeu), que le REL du mod remplacerait ; « replace_disc_rel »: true pour le remplacer exprès",
                    m.id
                )));
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
            match (&m.sounds_of, &m.sound_group) {
                (None, None) => return Err(fail(format!("mini-jeu {} : il lui faut « sounds_of » (un mini-jeu du jeu) ou « sound_group » (son groupe de sons)", m.id))),
                (Some(of), _) if !GAME.contains(of) => return Err(fail(format!("mini-jeu {} : sounds_of {of} n'est pas un mini-jeu du jeu", m.id))),
                (_, Some(SoundGroup::Id(g))) if *g < 0 => return Err(fail(format!("mini-jeu {} : sound_group {g} n'est pas un groupe", m.id))),
                (_, Some(SoundGroup::Label(l))) if l.trim().is_empty() => return Err(fail(format!("mini-jeu {} : sound_group vide", m.id))),
                _ => {}
            }
            if let Some(pair) = m.pair {
                if pair == m.id || !(GAME.contains(&pair) || all.contains(&pair)) {
                    return Err(fail(format!("mini-jeu {} : pair {pair} n'est ni un autre mini-jeu du jeu ni un mini-jeu des mods", m.id)));
                }
            }
            seen.push((m.id, id));
        }
    }
    Ok(())
}

/// /wp/minigames.bin (WpMinigameFile, then a WpMinigame per minigame, big-endian), once `check`ed. `group`
/// finds a sound group by its label (None: no such group).
pub fn file(added: &Added, group: &mut dyn FnMut(&str) -> Result<Option<i32>, String>) -> Result<Vec<u8>, LinkError> {
    let count: usize = added.iter().map(|(_, l)| l.len()).sum();
    let mut out = Vec::with_capacity(16 + count * ENTRY_SIZE as usize);
    for v in [MAGIC, VERSION, count as u32, ENTRY_SIZE] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for (id, list) in added {
        for m in list.iter() {
            let sound_group = match &m.sound_group {
                None => -1,
                Some(SoundGroup::Id(g)) => *g,
                Some(SoundGroup::Label(label)) => group(label)
                    .map_err(|e| LinkError::new(e, &[id]))?
                    .ok_or_else(|| LinkError::new(format!("mini-jeu {} : pas de groupe de sons « {label} » dans {SOUND_ARCHIVE}", m.id), &[id]))?,
            };
            let flags = if m.free_play { 0 } else { HIDDEN } | if m.pool { 0 } else { NO_POOL };
            let fields = [
                m.id,
                category(&m.category).unwrap_or(-1),
                m.rules.unwrap_or(m.id),
                m.controls.unwrap_or(-1),
                flags as i32,
                m.controls_team.unwrap_or(-1),
                m.pair.unwrap_or(-1),
                sound_group, // < 0: sounds_of's
                m.sounds_of.unwrap_or(-1),
            ];
            for v in fields {
                out.extend_from_slice(&v.to_be_bytes());
            }
            out.push(m.kind.as_deref().unwrap_or(" ").as_bytes()[0]); // mType
            out.extend_from_slice(&[0; 3]);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(id: i32) -> Spec {
        Spec {
            id,
            category: "1v3".into(),
            kind: None,
            pool: true,
            sounds_of: Some(406),
            sound_group: None,
            pair: None,
            replace_disc_rel: false,
            rules: None,
            controls: Some(110),
            controls_team: Some(2),
            free_play: true,
        }
    }

    fn no_groups(label: &str) -> Result<Option<i32>, String> {
        Ok((label == "GROUP_MOD_X").then_some(311))
    }

    #[test]
    fn writes_the_file_the_core_reads() {
        let hidden = Spec { id: 444, category: "4p".into(), kind: Some("A".into()), pool: false, free_play: false, rules: Some(400), controls: None, controls_team: None, ..spec(0) };
        let (a, b) = ([spec(305)], [hidden]);
        let added: Added = vec![("winter", &a[..]), ("other", &b[..])];
        check(&added).unwrap();
        let f = file(&added, &mut no_groups).unwrap();
        let words = |at: usize, n: usize| -> Vec<u32> { (0..n).map(|k| u32::from_be_bytes(f[at + 4 * k..at + 4 * k + 4].try_into().unwrap())).collect() };
        assert_eq!(f.len(), 16 + 2 * 0x28);
        assert_eq!(words(0, 4), [0x5750_4D47, 1, 2, 0x28]);
        let m = |v: i32| v as u32;
        assert_eq!(words(16, 9), [305, 2, 305, 110, 0, 2, m(-1), m(-1), 406]);
        assert_eq!(&f[16 + 0x24..16 + 0x28], b" \0\0\0");
        assert_eq!(words(16 + 0x28, 9), [444, 0, 400, m(-1), 3, m(-1), m(-1), m(-1), 406]);
        assert_eq!(f[16 + 0x28 + 0x24], b'A');
    }

    #[test]
    fn refuses_what_the_core_would_leave_out() {
        let one = [spec(444)];
        let e = check(&vec![("a", &one[..]), ("b", &one[..])]).err().unwrap();
        assert_eq!(e.mods, ["a", "b"]);
        let game = [spec(406)];
        assert!(check(&vec![("a", &game[..])]).err().unwrap().message.contains("déjà"));
        let silent = [Spec { sounds_of: Some(444), ..spec(450) }];
        assert!(check(&vec![("a", &silent[..])]).is_err());
        let mute = [Spec { sounds_of: None, ..spec(450) }];
        assert!(check(&vec![("a", &mute[..])]).err().unwrap().message.contains("sound_group"));
        let odd = [Spec { category: "3v1".into(), ..spec(445) }];
        assert!(check(&vec![("a", &odd[..])]).is_err());
        // as many as the mods want
        let many: Vec<Spec> = (600..700).map(spec).collect();
        assert!(check(&vec![("a", &many[..])]).is_ok());
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;

    fn read(text: &str) -> Spec {
        serde_json::from_str(text).unwrap()
    }

    /// mod.json's pair and sound_group: written into mPair and mSoundGroup; sounds_of not needed then.
    #[test]
    fn writes_the_pair_and_the_sound_group() {
        let a = [read(r#"{"id": 113, "category": "1v1", "sound_group": "GROUP_MOD_X", "pair": 114}"#), read(r#"{"id": 114, "category": "1v1", "sound_group": 42, "sounds_of": 110, "pair": 113}"#)];
        let b = [read(r#"{"id": 115, "category": "1v1", "sounds_of": 110, "pair": 100}"#)];
        assert_eq!(a[0].sounds_of, None);
        let added: Added = vec![("winter", &a[..]), ("other", &b[..])];
        check(&added).unwrap();
        let f = file(&added, &mut |l: &str| Ok((l == "GROUP_MOD_X").then_some(311))).unwrap();
        let word = |entry: usize, field: usize| i32::from_be_bytes(f[16 + 0x28 * entry + 4 * field..][..4].try_into().unwrap());
        // mPair, mSoundGroup, mSoundsOf
        assert_eq!([word(0, 6), word(0, 7), word(0, 8)], [114, 311, -1]);
        assert_eq!([word(1, 6), word(1, 7), word(1, 8)], [113, 42, 110]);
        assert_eq!([word(2, 6), word(2, 7), word(2, 8)], [100, -1, 110]);
        // a label no patch adds
        let c = [read(r#"{"id": 116, "category": "1v1", "sound_group": "GROUP_MOD_Y"}"#)];
        let e = file(&vec![("c", &c[..])], &mut |_: &str| Ok(None)).err().unwrap();
        assert!(e.message.contains("GROUP_MOD_Y") && e.mods == ["c"], "{}", e.message);
        // a pair that is no minigame, or itself
        for pair in [999, 116] {
            let d = [Spec { pair: Some(pair), ..read(r#"{"id": 116, "category": "1v1", "sounds_of": 110}"#) }];
            assert!(check(&vec![("d", &d[..])]).err().unwrap().message.contains("pair"));
        }
    }

    /// 103 and 417: the disc's leftover RELs, replaced only on purpose.
    #[test]
    fn keeps_the_discs_leftover_rels_unless_told() {
        for id in [103, 417] {
            let one = [read(&format!(r#"{{"id": {id}, "category": "1v1", "sounds_of": 110}}"#))];
            let e = check(&vec![("a", &one[..])]).err().unwrap();
            assert!(e.message.contains(&format!("/rel/mg{id}.rel")) && e.message.contains("replace_disc_rel"), "{}", e.message);
            let sure = [read(&format!(r#"{{"id": {id}, "category": "1v1", "sounds_of": 110, "replace_disc_rel": true}}"#))];
            assert!(check(&vec![("a", &sure[..])]).is_ok());
        }
    }
}
