//! The core's game files (wp-srccore docs/core.md, "Distribution"): the whole game rebuilt from its
//! decompilation, shipped as deltas (delta.rs) against the player's own disc. core.json lists them; they
//! are made once into a folder kept for the next launches (`cache`), made again for another core or
//! another disc, and linked.xml puts them in place of the disc's (link_game). The modules the mods link
//! into (`modules`: mr001, inst, menu) come as .plf instead, always linked.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const FORMAT: u32 = 2;
/// In the cache: what its files were made from.
const STAMP: &str = "core.txt";

#[derive(Deserialize)]
pub struct Package {
    format: u32,
    /// The modules the launcher links the mods into, each a <module>.plf with its <module>.json.
    pub modules: Vec<String>,
    pub deltas: Vec<Delta>,
    /// A disc REL that plays one of `modules` (its name -> the module): it gets the linked module too.
    #[serde(default)]
    pub twins: BTreeMap<String, String>,
}

/// A file of the core: the disc's `disc` ("main.dol", "/rel/<name>.rel.lz") changed by `delta`.
#[derive(Deserialize)]
pub struct Delta {
    pub disc: String,
    pub delta: String,
    #[allow(dead_code)] // (the delta says it too)
    #[serde(default)]
    pub lz: bool,
}

/// Why the core's files could not be made.
#[derive(Debug)]
pub enum MakeError {
    /// The disc's file (disc path) is not the one the core was made against.
    OtherSource(String),
    /// The core's delta (its file name) is damaged.
    Broken(String),
    /// Reading the disc or writing the cache.
    Io(String),
}

impl std::fmt::Display for MakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MakeError::OtherSource(file) => write!(f, "{file} n'est pas celui du disque d'origine"),
            MakeError::Broken(file) => write!(f, "{file} est abîmé"),
            MakeError::Io(e) => f.write_str(e),
        }
    }
}

/// The core's core.json.
pub fn read(core: &Path) -> Result<Package, String> {
    let text = std::fs::read_to_string(core.join("core.json")).map_err(|_| "cœur des mods : core.json est introuvable".to_string())?;
    let package: Package = serde_json::from_str(&text).map_err(|e| format!("core.json : {e}"))?;
    if package.format != FORMAT {
        return Err(format!("core.json : format {} inconnu de ce launcher", package.format));
    }
    Ok(package)
}

/// Where a disc file goes in a folder: main.dol at its root, the others at their disc path.
pub fn inside(disc: &str) -> &str {
    disc.trim_start_matches('/')
}

impl Package {
    /// The files made in `cache`: (disc path, file).
    pub fn files(&self, cache: &Path) -> Vec<(String, PathBuf)> {
        self.deltas.iter().map(|d| (d.disc.clone(), cache.join(inside(&d.disc)))).collect()
    }

    /// What the files are made from: `disc` (the player's disc, as the caller tells it) and the deltas'
    /// headers (the SHA-256 of their source and of their result).
    fn stamp(&self, core: &Path, disc: &str) -> Result<String, MakeError> {
        use std::io::Read;
        let mut all = format!("{disc}\n").into_bytes();
        for d in &self.deltas {
            let mut head = [0u8; 69];
            std::fs::File::open(core.join(&d.delta)).and_then(|mut f| f.read_exact(&mut head)).map_err(|_| MakeError::Broken(d.delta.clone()))?;
            all.extend_from_slice(&head);
        }
        Ok(crate::delta::sha256(&all).iter().map(|b| format!("{b:02x}")).collect())
    }

    /// Whether `cache` has the files of this core made from `disc`.
    pub fn is_made(&self, core: &Path, cache: &Path, disc: &str) -> bool {
        let Ok(stamp) = self.stamp(core, disc) else { return false };
        std::fs::read_to_string(cache.join(STAMP)).ok().as_deref() == Some(stamp.as_str()) && self.files(cache).iter().all(|(_, f)| f.is_file())
    }

    /// Makes the files into `cache` (emptied first) from the disc's, which `read` gives by disc path, a
    /// few at a time. Nothing to do when they are made already.
    pub fn make(&self, core: &Path, cache: &Path, disc: &str, read: &(dyn Fn(&str) -> Result<Vec<u8>, String> + Sync)) -> Result<(), MakeError> {
        if self.is_made(core, cache, disc) {
            return Ok(());
        }
        let stamp = self.stamp(core, disc)?;
        let io = |e: std::io::Error| MakeError::Io(format!("{} : {e}", cache.display()));
        let _ = std::fs::remove_dir_all(cache);
        std::fs::create_dir_all(cache).map_err(io)?;
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).min(8);
        let chunk = self.deltas.len().div_ceil(threads).max(1);
        std::thread::scope(|scope| {
            let workers: Vec<_> = self.deltas.chunks(chunk).map(|part| scope.spawn(move || part.iter().try_for_each(|d| make_one(core, cache, d, read)))).collect();
            workers.into_iter().try_for_each(|w| w.join().unwrap_or_else(|_| Err(MakeError::Io("delta".into()))))
        })?;
        // (last: a cache with its stamp is a finished one)
        std::fs::write(cache.join(STAMP), stamp).map_err(io)
    }
}

fn make_one(core: &Path, cache: &Path, d: &Delta, read: &(dyn Fn(&str) -> Result<Vec<u8>, String> + Sync)) -> Result<(), MakeError> {
    let patch = std::fs::read(core.join(&d.delta)).map_err(|_| MakeError::Broken(d.delta.clone()))?;
    let source = read(&d.disc).map_err(MakeError::Io)?;
    let made = crate::delta::apply(&patch, &source).map_err(|e| match e {
        crate::delta::Error::OtherSource => MakeError::OtherSource(d.disc.clone()),
        crate::delta::Error::Broken => MakeError::Broken(d.delta.clone()),
    })?;
    let to = cache.join(inside(&d.disc));
    let io = |e: std::io::Error| MakeError::Io(format!("{} : {e}", to.display()));
    std::fs::create_dir_all(to.parent().unwrap()).map_err(io)?;
    std::fs::write(&to, made).map_err(io)
}

/// `from` at `to` too: a hard link when it can (the cache and linked.xml's folder are side by side), a
/// copy otherwise.
pub fn put(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(to);
    std::fs::hard_link(from, to).or_else(|_| std::fs::copy(from, to).map(|_| ()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_the_files_once() {
        let dir = std::env::temp_dir().join("wp-corepkg-test");
        let _ = std::fs::remove_dir_all(&dir);
        let (core, cache) = (dir.join("core"), dir.join("cache"));
        std::fs::create_dir_all(&core).unwrap();
        let dol: Vec<u8> = (0..4000u32).map(|i| (i * 7 % 251) as u8).collect();
        let rel: Vec<u8> = (0..3000u32).map(|i| (i * 13 % 241) as u8).collect();
        let (mut dol2, mut rel2) = (dol.clone(), rel.clone());
        dol2[100] ^= 0xFF;
        rel2.extend_from_slice(b"core");
        std::fs::write(core.join("main.dol.wpd"), crate::delta::make(&dol, &dol2, false).unwrap()).unwrap();
        let packed = crate::lz::compress(&rel);
        std::fs::write(core.join("mg100.rel.wpd"), crate::delta::make(&packed, &crate::lz::compress(&rel2), true).unwrap()).unwrap();
        std::fs::write(
            core.join("core.json"),
            r#"{"format": 2, "modules": ["mr001"], "deltas": [{"disc": "main.dol", "delta": "main.dol.wpd", "lz": false},
            {"disc": "/rel/mg100.rel.lz", "delta": "mg100.rel.wpd", "lz": true}], "twins": {}}"#,
        )
        .unwrap();
        let package = read(&core).unwrap();
        let reads = std::sync::atomic::AtomicUsize::new(0);
        let disc = |path: &str| {
            reads.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(if path == "main.dol" { dol.clone() } else { packed.clone() })
        };
        assert!(!package.is_made(&core, &cache, "disc"));
        package.make(&core, &cache, "disc", &disc).unwrap();
        assert_eq!(std::fs::read(cache.join("main.dol")).unwrap(), dol2);
        assert_eq!(crate::lz::decompress(&std::fs::read(cache.join("rel/mg100.rel.lz")).unwrap()).unwrap(), rel2);
        // once: the same core and disc again read nothing
        package.make(&core, &cache, "disc", &disc).unwrap();
        assert_eq!(reads.load(std::sync::atomic::Ordering::Relaxed), 2);
        assert!(!package.is_made(&core, &cache, "another disc"));
        // a disc that isn't the one the core was made against
        let other = |path: &str| Ok(if path == "main.dol" { dol2.clone() } else { packed.clone() });
        assert!(matches!(package.make(&core, &cache, "another disc", &other), Err(MakeError::OtherSource(f)) if f == "main.dol"));
        assert!(!cache.join(STAMP).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
