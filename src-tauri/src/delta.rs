//! A package's file made from the player's own disc: a delta (bsdiff) against the
//! disc's file, so that the package carries none of the game's files. For an .lz
//! file, the delta is between the decompressed contents, and the result is
//! compressed again (lz.rs): a delta between two compressed files would be as big
//! as the file.
//!
//! A delta file: "WPD1", a flags byte (1: .lz), the SHA-256 of the disc's file and
//! of the result (decompressed for an .lz), then bsdiff 4.x's patch.

use crate::lz;
use sha2::{Digest, Sha256};

const MAGIC: &[u8; 4] = b"WPD1";
const LZ: u8 = 1;
const HEADER: usize = 4 + 1 + 32 + 32;

#[derive(Debug, PartialEq)]
pub enum Error {
    /// Not a delta file, or a damaged one.
    Broken,
    /// The disc's file isn't the one it was made against.
    OtherSource,
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

/// An .lz file that decompresses, as the disc's are.
fn unpacked(data: &[u8]) -> Option<Vec<u8>> {
    matches!(data.first(), Some(0x10 | 0x11)).then(|| lz::decompress(data).ok()).flatten()
}

/// The delta that makes `result` from `source`; `lz` when both are .lz files.
/// (Packaging only: examples/wpdelta.rs.)
#[allow(dead_code)]
pub fn make(source: &[u8], result: &[u8], lz: bool) -> Result<Vec<u8>, String> {
    let (from, to) = match lz {
        true => (unpacked(source).ok_or("source: LZ illisible")?, unpacked(result).ok_or("résultat : LZ illisible")?),
        false => (source.to_vec(), result.to_vec()),
    };
    let mut out = Vec::with_capacity(HEADER);
    out.extend_from_slice(MAGIC);
    out.push(if lz { LZ } else { 0 });
    out.extend_from_slice(&sha256(source));
    out.extend_from_slice(&sha256(&to));
    qbsdiff::Bsdiff::new(&from, &to).compare(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// The SHA-256 of the disc's file it needs.
pub fn source_hash(delta: &[u8]) -> Result<[u8; 32], Error> {
    if delta.len() < HEADER || &delta[..4] != MAGIC || delta[4] & !LZ != 0 {
        return Err(Error::Broken);
    }
    Ok(delta[5..37].try_into().unwrap())
}

/// The file made from the disc's one.
pub fn apply(delta: &[u8], source: &[u8]) -> Result<Vec<u8>, Error> {
    if source_hash(delta)? != sha256(source) {
        return Err(Error::OtherSource);
    }
    let lz = delta[4] & LZ != 0;
    let from = if lz { unpacked(source).ok_or(Error::OtherSource)? } else { source.to_vec() };
    let patch = qbsdiff::Bspatch::new(&delta[HEADER..]).map_err(|_| Error::Broken)?;
    let mut to = Vec::with_capacity(patch.hint_target_size() as usize);
    patch.apply(&from, &mut to).map_err(|_| Error::Broken)?;
    if sha256(&to) != delta[37..69] {
        return Err(Error::Broken);
    }
    Ok(if lz { lz::compress(&to) } else { to })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_the_file_again() {
        let source: Vec<u8> = (0..50_000u32).map(|i| (i * 31 % 253) as u8).collect();
        let mut result = source.clone();
        result[1000..1010].copy_from_slice(b"Winter!!!!");
        result.extend_from_slice(b"new end");
        let delta = make(&source, &result, false).unwrap();
        assert!(delta.len() < 1000);
        assert_eq!(apply(&delta, &source).unwrap(), result);
        assert_eq!(apply(&delta, &result), Err(Error::OtherSource));
        assert_eq!(apply(b"WPD2", &source), Err(Error::Broken));

        let (packed, packed_result) = (lz::compress(&source), lz::compress(&result));
        let delta = make(&packed, &packed_result, true).unwrap();
        assert_eq!(lz::decompress(&apply(&delta, &packed).unwrap()).unwrap(), result);
    }
}
