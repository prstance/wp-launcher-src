//! What the launcher takes from the player's own Wii Party disc for the Miis:
//! the face parts (RFL_Res.dat), the game's Miis (mii_rsd.bin) and their names
//! (mess.bin), and, from the update partition, the Mii Channel. Nothing of it
//! ships with the launcher.

use crate::game;
use crate::i18n::tr;
use crate::settings::Disc;
use crate::tools;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::AppHandle;

const RESOURCES: &str = "RFL/Resource/RFLRes01.arc.lz";
const MIIS: &str = "RFL/RSD/mii_rsd.bin.lz";
pub const RESOURCES_FILE: &str = "RFL_Res.dat";
pub const MIIS_FILE: &str = "mii_rsd.bin";
/// The game's Miis' names, in mii_rsd.bin's order, as JSON.
pub const NAMES_FILE: &str = "mii_names.json";
/// Where the game's texts are, one folder per language and region (fr_EU…).
const LOCALE: &str = "locale";
/// The Mii Channel's WAD in the update partition (RVL-NigaoeNR-v6.wad on a PAL disc).
const CHANNEL_PREFIX: &str = "_sys/RVL-NigaoeNR";

/// Where the extracted files are kept.
pub fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    game::data_dir(app, "rfl")
}

pub use crate::lz::decompress as decompress_lz;

/// The file called `name` in a U8 archive (.arc).
pub fn u8_file(arc: &[u8], name: &str) -> Option<Vec<u8>> {
    let word = |at: usize| arc.get(at..at + 4).map(|b| u32::from_be_bytes(b.try_into().unwrap()) as usize);
    if word(0)? != 0x55AA_382D {
        return None;
    }
    let root = word(4)?;
    let count = word(root + 8)?;
    let names = root + count * 12;
    (1..count).find_map(|i| {
        let node = root + i * 12;
        let is_file = *arc.get(node)? == 0;
        let at = names + (word(node)? & 0xFF_FFFF);
        let end = at + arc.get(at..)?.iter().position(|&b| b == 0)?;
        if !is_file || &arc[at..end] != name.as_bytes() {
            return None;
        }
        let (offset, size) = (word(node + 4)?, word(node + 8)?);
        arc.get(offset..offset + size).map(<[u8]>::to_vec)
    })
}

/// The text of each key in an XMSG file (locale/*/mess.bin): a u32 count at 8, then
/// 16-byte entries whose first two words are the key's and the text's offsets; keys
/// are Latin-1, texts UTF-16 big-endian, both ending with 0 (wp/tools/xmsg.py).
pub fn xmsg_texts(data: &[u8]) -> Option<std::collections::HashMap<String, String>> {
    let word = |at: usize| data.get(at..at + 4).map(|b| u32::from_be_bytes(b.try_into().unwrap()) as usize);
    if data.get(..4)? != b"XMSG" {
        return None;
    }
    (0..word(8)?)
        .map(|k| {
            let (key, text) = (word(12 + k * 16)?, word(16 + k * 16)?);
            let key: String = data.get(key..)?.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect();
            let units: Vec<u16> = data.get(text..)?.as_chunks::<2>().0.iter().map(|&u| u16::from_be_bytes(u)).take_while(|&u| u != 0).collect();
            Some((key, String::from_utf16_lossy(&units)))
        })
        .collect()
}

/// Extracts one file of the disc with dolphin-tool; its bytes.
pub async fn extract(app: &AppHandle, disc: &Disc, partition: &str, path: &str) -> Result<Vec<u8>, String> {
    let tool = tools::dolphin_tool(app).ok_or_else(|| tr!("Dolphin est absent de ce build du launcher."))?;
    // Its own folder: the Miis and the home screen may extract at the same time.
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let scratch = dir(app)?.join(format!("extract-{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&scratch);
    let output = game::command(&tool)
        .args(["extract", "-q", "-i", &disc.path, "-p", partition, "-s", path, "-o"])
        .arg(&scratch)
        .output()
        .await
        .map_err(|e| e.to_string())?;
    let file = scratch.join(partition).join("files").join(path);
    let bytes = std::fs::read(&file).map_err(|_| String::from_utf8_lossy(&output.stderr).trim().to_string());
    let _ = std::fs::remove_dir_all(&scratch);
    bytes
}

/// Lists a partition, or one of its folders.
pub async fn list(app: &AppHandle, disc: &Disc, partition: &str, folder: Option<&str>) -> Result<Vec<String>, String> {
    let tool = tools::dolphin_tool(app).ok_or_else(|| tr!("Dolphin est absent de ce build du launcher."))?;
    let mut cmd = game::command(&tool);
    cmd.args(["extract", "-l", "-i", &disc.path, "-p", partition]);
    if let Some(folder) = folder {
        cmd.args(["-s", folder]);
    }
    let output = cmd.output().await.map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&output.stdout).lines().map(|l| l.trim().to_string()).collect())
}

/// The game's Miis' names (store000…), from the English texts if the disc has them.
async fn names(app: &AppHandle, disc: &Disc) -> Result<Vec<String>, String> {
    let files = list(app, disc, "DATA", Some(LOCALE)).await?;
    let mut texts: Vec<&String> = files.iter().filter(|f| f.ends_with("/mess.bin")).collect();
    texts.sort_by_key(|f| !f.starts_with("locale/en"));
    let Some(path) = texts.first() else { return Ok(Vec::new()) };
    let texts = xmsg_texts(&extract(app, disc, "DATA", path).await?).ok_or("mess.bin illisible")?;
    Ok((0..).map_while(|i| texts.get(&format!("store{i:03}")).cloned()).collect())
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Takes the face parts and the game's Miis from the disc, once.
pub async fn extract_faces(app: &AppHandle, disc: &Disc) -> Result<(), String> {
    let dir = dir(app)?;
    let (resources, miis, names_file) = (dir.join(RESOURCES_FILE), dir.join(MIIS_FILE), dir.join(NAMES_FILE));
    if !resources.is_file() {
        let arc = decompress_lz(&extract(app, disc, "DATA", RESOURCES).await?)?;
        write(&resources, &u8_file(&arc, RESOURCES_FILE).ok_or("RFLRes01.arc: RFL_Res.dat introuvable")?)?;
    }
    if !miis.is_file() {
        write(&miis, &decompress_lz(&extract(app, disc, "DATA", MIIS).await?)?)?;
    }
    if !names_file.is_file() {
        write(&names_file, &serde_json::to_vec(&names(app, disc).await?).map_err(|e| e.to_string())?)?;
    }
    Ok(())
}

/// The Mii Channel's WAD, taken from the disc's update partition the first time.
pub async fn channel_wad(app: &AppHandle, disc: &Disc) -> Result<PathBuf, String> {
    let wad = dir(app)?.join(format!("mii-channel-{}.wad", disc.game_id));
    if wad.is_file() {
        return Ok(wad);
    }
    let files = list(app, disc, "UPDATE", None).await?;
    let Some(path) = files.iter().find(|f| f.starts_with(CHANNEL_PREFIX) && f.ends_with(".wad")) else {
        return Err(tr!("Ton image du jeu n’a pas de partition de mise à jour (c’est souvent le cas des .wbfs) : la Chaîne Mii n’y est pas. Utilise un .iso ou un .rvz complet."));
    };
    write(&wad, &extract(app, disc, "UPDATE", path).await?)?;
    Ok(wad)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decompresses_lz11() {
        // "abcabcabcabc!": 3 literals, a copy of 9 from 3 back, a literal.
        let data = [0x11, 13, 0, 0, 0b0001_0000, b'a', b'b', b'c', 0x80, 0x02, b'!'];
        assert_eq!(decompress_lz(&data).unwrap(), b"abcabcabcabc!");
    }

    /// Against the files extracted from the real disc, when they are there.
    #[test]
    #[ignore]
    fn reads_the_disc_resources() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../original");
        let arc = decompress_lz(&std::fs::read(root.join("fst/files").join(RESOURCES)).unwrap()).unwrap();
        let res = u8_file(&arc, RESOURCES_FILE).unwrap();
        assert_eq!(res, std::fs::read(root.join("unpacked/RFL/Resource/RFLRes01.arc.d/RFL_Res.dat")).unwrap());
        let miis = decompress_lz(&std::fs::read(root.join("fst/files").join(MIIS)).unwrap()).unwrap();
        assert_eq!(crate::mii::data::read_store(&miis).len(), 100);
        let texts = xmsg_texts(&std::fs::read(root.join("fst/files/locale/en_EU/mess.bin")).unwrap()).unwrap();
        assert_eq!((texts["store003"].as_str(), texts["store024"].as_str()), ("Shinnosuke", "Matt"));
    }
}
