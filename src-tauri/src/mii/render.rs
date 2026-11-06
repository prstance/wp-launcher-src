//! Mii heads, drawn the way the Wii's Mii library (RFL) draws its icons, in software.
//! The decompiled RFL is the specification: RFL_Icon.c for the camera and light,
//! RFL_Model.c for the 3D parts, RFL_MakeTex.c for the face texture. The models and
//! textures come from the player's own RFL_Res.dat.

mod info;
mod mask;
mod model;
mod raster;
pub mod texture;

use info::Info;
use raster::Target;

/// The parsed RFL_Res.dat.
pub struct Resources {
    data: Vec<u8>,
    /// Per archive (RFLiArcID order): its file count and where its offset table starts.
    archives: [(u16, usize); ARCHIVES],
}

const ARCHIVES: usize = 18;

/// RFLiArcID.
#[derive(Clone, Copy)]
enum Arc {
    Beard,
    Eye,
    Eyebrow,
    Faceline,
    FaceTex,
    ForeHead,
    Glass,
    GlassTex,
    Hair,
    Mask,
    Mole,
    Mouth,
    Mustache,
    Nose,
    Nline,
    NlineTex,
    Cap,
    CapTex,
}

impl Resources {
    /// Reads the archive table like RFL_NANDLoader.c (parseOnmemoryRes_): a u32 offset
    /// per archive after a 4-byte header; each archive is a u16 file count, a u16, then
    /// count + 1 file offsets relative to the end of that table.
    pub fn parse(bytes: Vec<u8>) -> Result<Resources, String> {
        let be32 = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
        let mut archives = [(0, 0); ARCHIVES];
        for (i, archive) in archives.iter_mut().enumerate() {
            let offset = be32((i + 1) * 4).ok_or("RFL_Res.dat is truncated")?;
            let count = bytes.get(offset..offset + 2).ok_or("RFL_Res.dat is truncated")?;
            let count = u16::from_be_bytes([count[0], count[1]]);
            *archive = (count, offset + 4);
        }
        let res = Resources { data: bytes, archives };
        for (i, &(count, _)) in res.archives.iter().enumerate() {
            for file in 0..count {
                res.file_at(i, file)?;
            }
        }
        Ok(res)
    }

    fn file_at(&self, archive: usize, file: u16) -> Result<&[u8], String> {
        let (count, table) = self.archives[archive];
        let bad = || format!("RFL_Res.dat: no file {file} in archive {archive}");
        if file >= count {
            return Err(bad());
        }
        let be32 = |at: usize| self.data.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
        let at = table + file as usize * 4;
        let (start, end) = (be32(at).ok_or_else(bad)?, be32(at + 4).ok_or_else(bad)?);
        let base = table + count as usize * 4 + 4;
        if start > end {
            return Err(bad());
        }
        self.data.get(base + start..base + end).ok_or_else(bad)
    }

    fn file(&self, arc: Arc, file: u16) -> Result<&[u8], String> {
        self.file_at(arc as usize, file)
    }
}

/// The Mii's head, facing front like the RFL icon, on a transparent background, as PNG bytes `size`×`size`.
pub fn head_png(res: &Resources, data: &[u8; 74], size: u32) -> Result<Vec<u8>, String> {
    png(size, size, &head_rgba(res, data, size)?)
}

/// Straight 8-bit RGBA, row by row, as PNG bytes.
pub fn png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(png)
}

/// Straight RGBA, drawn supersampled then averaged down for smooth edges.
fn head_rgba(res: &Resources, data: &[u8; 74], size: u32) -> Result<Vec<u8>, String> {
    if size == 0 || size > 2048 {
        return Err("bad size".into());
    }
    let info = Info::parse(data)?;
    let factor = if size <= 256 { 3 } else { 2 };
    let inner = size as usize * factor;
    // RFL picks the face texture's size from the icon's; it covers about half of it.
    let mask = mask::make(res, &info, if inner <= 512 { 256 } else { 512 })?;
    let mut target = Target::new(inner);
    model::draw(res, &info, &mask, &mut target)?;
    Ok(target.downsample(factor))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extracted from the disc, next to the launcher in the mod project.
    const RES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../original/unpacked/RFL/Resource/RFLRes01.arc.d/RFL_Res.dat");
    const RSD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../original/unpacked/RFL/RSD/mii_rsd.bin");

    /// Where the sheets go: $MII_OUT, or the system's temporary folder.
    fn out() -> String {
        std::env::var("MII_OUT").unwrap_or_else(|_| std::env::temp_dir().join("mii-heads").to_string_lossy().into_owned())
    }

    /// RFL_DefaultDatabase.c's six guests: their first two bytes (sex, favorite color)
    /// and their appearance, 0x20..0x36. The rest is the name and ID.
    #[rustfmt::skip]
    const GUESTS: [([u8; 2], [u8; 22]); 6] = [
        ([0x00, 0x08], [0x10, 0x04, 0x88, 0x00, 0x31, 0x80, 0x08, 0xA2, 0x08, 0x8C, 0x08, 0x58, 0x14, 0x4A, 0xB8, 0x8D, 0x00, 0x8A, 0x00, 0x8A, 0x25, 0x04]),
        ([0x00, 0x0A], [0x00, 0x04, 0x6F, 0x80, 0x31, 0x80, 0xC8, 0xA2, 0x08, 0x8C, 0x88, 0x58, 0x14, 0x4A, 0xB8, 0x8D, 0x00, 0x8A, 0x00, 0x8A, 0x25, 0x04]),
        ([0x00, 0x00], [0x04, 0x04, 0x42, 0x40, 0x31, 0x80, 0x28, 0xA2, 0x08, 0x8C, 0x08, 0x58, 0x14, 0x4A, 0xB8, 0x8D, 0x00, 0x8A, 0x00, 0x8A, 0x25, 0x04]),
        ([0x40, 0x04], [0x08, 0x04, 0x30, 0x00, 0x01, 0x80, 0x08, 0xA2, 0x10, 0x6C, 0x08, 0x58, 0x14, 0x4A, 0xB8, 0x8D, 0x00, 0x8A, 0x00, 0x8A, 0x25, 0x04]),
        ([0x40, 0x0C], [0x00, 0x04, 0x1D, 0xC0, 0x01, 0x80, 0xE8, 0xA2, 0x10, 0x6C, 0xA8, 0x58, 0x14, 0x4A, 0xB8, 0x8D, 0x00, 0x8A, 0x00, 0x8A, 0x25, 0x04]),
        ([0x40, 0x0E], [0x00, 0x04, 0x18, 0x40, 0x01, 0x80, 0x28, 0xA2, 0x10, 0x6C, 0x08, 0x58, 0x14, 0x4A, 0xB8, 0x8D, 0x00, 0x8A, 0x00, 0x8A, 0x25, 0x04]),
    ];

    /// The 100 RSD Miis (RFLStoreData: RFLiCharData then a CRC), then the guests.
    fn miis() -> Vec<[u8; 74]> {
        let rsd = std::fs::read(RSD).unwrap();
        let mut miis: Vec<[u8; 74]> = rsd.as_chunks::<76>().0.iter().map(|r| r[..74].try_into().unwrap()).collect();
        for (head, look) in GUESTS {
            let mut m = [0; 74];
            m[..2].copy_from_slice(&head);
            m[0x20..0x36].copy_from_slice(&look);
            miis.push(m);
        }
        miis
    }

    /// Lays heads out in a grid over a light gray checker, to see their transparency.
    fn sheet(path: &str, heads: &[Result<Vec<u8>, String>], cell: usize, cols: usize) {
        let rows = heads.len().div_ceil(cols);
        let width = cols.min(heads.len()) * cell;
        let mut sheet = vec![0u8; width * rows * cell * 4];
        for (i, head) in heads.iter().enumerate() {
            let head = match head {
                Ok(head) => head,
                Err(e) => {
                    println!("head {i}: {e}");
                    continue;
                }
            };
            let (cx, cy) = (i % cols * cell, i / cols * cell);
            for y in 0..cell {
                for x in 0..cell {
                    let src = &head[(y * cell + x) * 4..][..4];
                    let bg = if (x * 16 / cell + y * 16 / cell).is_multiple_of(2) { 200.0 } else { 230.0 };
                    let a = src[3] as f32 / 255.0;
                    let dst = &mut sheet[((cy + y) * width + cx + x) * 4..][..4];
                    for k in 0..3 {
                        dst[k] = (src[k] as f32 * a + bg * (1.0 - a)) as u8;
                    }
                    dst[3] = 255;
                }
            }
        }
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width as u32, (rows * cell) as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&sheet).unwrap();
    }

    /// Renders the 100 RSD Miis and the 6 guests into sheet.png, and the Miis listed
    /// in $MII (comma-separated indices) at $SIZE px (256 by default) into zoom.png.
    #[test]
    #[ignore]
    fn contact_sheet() {
        fn shared<T: Send + Sync>(_: &T) {}
        let res = Resources::parse(std::fs::read(RES).unwrap()).unwrap();
        shared(&res);
        let miis = miis();
        let out = out();
        std::fs::create_dir_all(&out).unwrap();
        let start = std::time::Instant::now();
        let heads: Vec<_> = miis.iter().map(|m| head_rgba(&res, m, 128)).collect();
        let elapsed = start.elapsed();
        println!("{} heads in {:?} ({:?} each)", miis.len(), elapsed, elapsed / miis.len() as u32);
        sheet(&format!("{out}/sheet.png"), &heads, 128, 12);
        let zoom: Vec<usize> = std::env::var("MII").unwrap_or_default().split(',').filter_map(|s| s.parse().ok()).collect();
        if !zoom.is_empty() {
            let size = std::env::var("SIZE").ok().and_then(|s| s.parse().ok()).unwrap_or(256);
            let heads: Vec<_> = zoom.iter().map(|&i| head_rgba(&res, &miis[i], size as u32)).collect();
            sheet(&format!("{out}/zoom.png"), &heads, size, 4);
        }
        assert!(heads.iter().all(|h| h.is_ok()));
        assert!(head_png(&res, &miis[0], 128).unwrap().starts_with(b"\x89PNG"));
    }
}
