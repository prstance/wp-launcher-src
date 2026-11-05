//! The 3D head: parts loaded and placed like RFLiInitCharModelRes / RFLiInitShapeRes,
//! drawn like RFLDrawOpaCore / RFLDrawXluCore (RFL_Model.c) under RFLiMakeIcon's
//! camera and light (RFL_Icon.c).

use super::info::Info;
use super::mask::HAIR_COLOR;
use super::raster::{Cull, Pass, Target, Vertex};
use super::texture::Texture;
use super::{Arc, Resources};

const FACELINE_COLOR: [[u8; 3]; 6] = [[240, 216, 196], [255, 188, 128], [216, 136, 80], [255, 176, 144], [152, 80, 48], [82, 46, 28]];
const GLASS_COLOR: [[u8; 3]; 6] = [[16, 16, 16], [96, 56, 16], [152, 24, 16], [32, 48, 96], [144, 88, 0], [96, 88, 80]];
const FAVORITE_COLOR: [[u8; 3]; 12] = [
    [184, 64, 48],
    [240, 120, 40],
    [248, 216, 32],
    [128, 200, 40],
    [0, 116, 40],
    [32, 72, 152],
    [64, 160, 216],
    [232, 96, 120],
    [112, 44, 168],
    [72, 56, 24],
    [224, 224, 224],
    [24, 24, 20],
];

/// RFLiMakeIcon's camera: at (0, 34.5, 600) looking down -z, and a field of view of
/// 2·atan(43.2 / 500).
const EYE: [f32; 3] = [0.0, 34.5, 600.0];
const FOCAL: f32 = 500.0 / 43.2;
/// Its near and far planes, as distances from the camera.
const DEPTH: std::ops::RangeInclusive<f32> = 500.0..=700.0;
/// Its light, (1600, 1500, 6000) in the world, here in view space.
const LIGHT: [f32; 3] = [1600.0, 1500.0 - 34.5, 6000.0 - 600.0];
const AMBIENT: f32 = 160.0 / 255.0;

/// A shape file's vertices and triangles (each corner: position, normal, texcoord).
struct Mesh {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    txc: Vec<[f32; 2]>,
    tris: Vec<[[u8; 3]; 3]>,
}

/// The faceline's anchors for the other parts.
#[derive(Clone, Copy, Default)]
struct Anchors {
    nose: [f32; 3],
    beard: [f32; 3],
    hair: [f32; 3],
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let s = self.data.get(self.at..self.at + n).ok_or("truncated shape")?;
        self.at += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    fn i16(&mut self) -> Result<i16, String> {
        Ok(self.u16()? as i16)
    }
    fn vec(&mut self) -> Result<[f32; 3], String> {
        let mut v = [0.0; 3];
        for c in &mut v {
            let b = self.take(4)?;
            *c = f32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        }
        Ok(v)
    }
}

/// Where a part goes: RFLiShapeRes's posScale / posTrans, applied in 8.8 fixed point
/// like RFL does, and its flipX.
#[derive(Clone, Copy)]
struct Place {
    scale: f32,
    trans: [f32; 3],
    flip: bool,
}

/// Reads a shape file (RFLiInitShapeRes). Empty parts give None.
fn shape(res: &Resources, arc: Arc, file: u16, place: Option<Place>, anchors: Option<&mut Anchors>) -> Result<Option<Mesh>, String> {
    let has_txc = !matches!(arc, Arc::ForeHead | Arc::Hair | Arc::Beard | Arc::Nose);
    let mut r = Reader { data: res.file(arc, file)?, at: 4 };
    if let Some(a) = anchors {
        *a = Anchors { nose: r.vec()?, beard: r.vec()?, hair: r.vec()? };
    }
    let n = r.u16()? as usize;
    if n == 0 {
        return Ok(None);
    }
    let mut pos = Vec::with_capacity(n);
    for _ in 0..n {
        let p = [r.i16()?, r.i16()?, r.i16()?];
        let p = match place {
            Some(Place { scale, trans, flip }) => {
                let s = (256.0 * scale) as i32;
                let t = trans.map(|t| (256.0 * t) as i32);
                let x = if flip { -(p[0] as i32) } else { p[0] as i32 };
                [t[0] + ((x * s) >> 8), t[1] + ((p[1] as i32 * s) >> 8), t[2] + ((p[2] as i32 * s) >> 8)].map(|v| v as i16)
            }
            None => p,
        };
        pos.push(p.map(|v| v as f32 / 256.0));
    }
    let flip = place.is_some_and(|p| p.flip);
    let n = r.u16()? as usize;
    let mut nrm = Vec::with_capacity(n);
    for _ in 0..n {
        let x = r.i16()?;
        let x = if flip { x.wrapping_neg() } else { x };
        nrm.push([x, r.i16()?, r.i16()?].map(|v| v as f32 / 16384.0));
    }
    let mut txc = Vec::new();
    if has_txc {
        let n = r.u16()? as usize;
        for _ in 0..n {
            txc.push([r.i16()?, r.i16()?].map(|v| v as f32 / 8192.0));
        }
    }

    let mut tris = Vec::new();
    for _ in 0..r.u8()? {
        let count = r.u8()? as usize;
        let kind = r.u8()?;
        let mut v = Vec::with_capacity(count);
        for _ in 0..count {
            let c = [r.u8()?, r.u8()?, if has_txc { r.u8()? } else { 0 }];
            if c[0] as usize >= pos.len() || c[1] as usize >= nrm.len() || (has_txc && c[2] as usize >= txc.len()) {
                return Err("bad shape index".into());
            }
            v.push(c);
        }
        match kind & 0xF8 {
            0x80 => v.as_chunks::<4>().0.iter().for_each(|q| tris.extend([[q[0], q[1], q[2]], [q[0], q[2], q[3]]])),
            0x90 => v.as_chunks::<3>().0.iter().for_each(|t| tris.push([t[0], t[1], t[2]])),
            0x98 => (2..count).for_each(|i| tris.push(if i % 2 == 0 { [v[i - 2], v[i - 1], v[i]] } else { [v[i - 1], v[i - 2], v[i]] })),
            0xA0 => (2..count).for_each(|i| tris.push([v[0], v[i - 1], v[i]])),
            _ => return Err(format!("unsupported primitive {kind:#x}")),
        }
    }
    if !has_txc {
        txc.push([0.0; 2]);
    }
    Ok(Some(Mesh { pos, nrm, txc, tris }))
}

/// How a part is colored, from its TEV setup in RFLDrawOpaCore / RFLDrawXluCore.
/// Every part is then multiplied by the lighting (TEV stage 1: CPREV × RASC).
enum Material<'a> {
    Konst([u8; 3]),
    /// Hats: (favorite color + favorite color × texture) / 2.
    Cap([u8; 3], &'a Texture),
    /// The faceline color under the face texture (makeup, wrinkles), read through the
    /// R-A-B-G swap table: its alpha sits in green.
    Face([u8; 3], &'a Texture),
    /// The mask texture and the box of it that isn't transparent.
    Mask(&'a Texture, [f32; 4]),
    /// Black, the texture's alpha.
    Noseline(&'a Texture),
    Glasses([u8; 3], &'a Texture),
}

fn color(c: [u8; 3]) -> [f32; 3] {
    c.map(|v| v as f32 / 255.0)
}

impl Material<'_> {
    fn shade(&self, lit: f32, [s, t]: [f32; 2]) -> [f32; 4] {
        let (rgb, a) = match self {
            Material::Konst(c) => (color(*c), 1.0),
            Material::Cap(c, tex) => {
                let (c, i) = (color(*c), tex.sample(s, t)[0]);
                (c.map(|c| (c + c * i) / 2.0), 1.0)
            }
            Material::Face(c, tex) => {
                let [r, g, b, a] = tex.sample(s, t);
                let (c, tex) = (color(*c), [r, a, b]);
                ([0, 1, 2].map(|k| c[k] * (1.0 - g) + tex[k] * g), 1.0)
            }
            Material::Mask(tex, [s0, t0, s1, t1]) => {
                if s < *s0 || s > *s1 || t < *t0 || t > *t1 {
                    return [0.0; 4];
                }
                let [r, g, b, a] = tex.sample(s, t);
                ([r, g, b], a)
            }
            Material::Noseline(tex) => ([0.0; 3], tex.sample(s, t)[3]),
            Material::Glasses(c, tex) => {
                let [i, _, _, a] = tex.sample(s, t);
                (color(*c).map(|c| c * i), a)
            }
        };
        let lit = lit.min(1.0);
        [rgb[0] * lit, rgb[1] * lit, rgb[2] * lit, a]
    }
}

/// Draws the head into `target`, framed like an RFL icon.
pub fn draw(res: &Resources, info: &Info, mask: &Texture, target: &mut Target) -> Result<(), String> {
    let mut anchors = Anchors::default();
    let face = shape(res, Arc::Faceline, info.faceline_type as u16, None, Some(&mut anchors))?;
    let hair_place = Some(Place { scale: 1.0, trans: anchors.hair, flip: info.hair_flip });
    let hair_type = info.hair_type as u16;
    let cap = shape(res, Arc::Cap, hair_type, hair_place, None)?;
    let hair = shape(res, Arc::Hair, hair_type, hair_place, None)?;
    let forehead = shape(res, Arc::ForeHead, hair_type, hair_place, None)?;
    let beard_place = Place { scale: 1.0, trans: anchors.beard, flip: false };
    let beard = shape(res, Arc::Beard, info.beard_type as u16, Some(beard_place), None)?;
    let nose_place = Place {
        scale: 0.4 + 0.175 * info.nose_scale as f32,
        trans: [anchors.nose[0], anchors.nose[1] + -1.5 * (info.nose_y as f32 - 8.0), anchors.nose[2]],
        flip: false,
    };
    let nose = shape(res, Arc::Nose, info.nose_type as u16, Some(nose_place), None)?;
    let noseline = shape(res, Arc::Nline, info.nose_type as u16, Some(nose_place), None)?;
    let mask_mesh = shape(res, Arc::Mask, info.faceline_type as u16, None, None)?;
    let glass_place = Place {
        scale: 0.15 * info.glass_scale as f32 + 0.4,
        trans: [anchors.nose[0], 5.0 + anchors.nose[1] + -1.5 * (info.glass_y as f32 - 11.0), 2.0 + anchors.nose[2]],
        flip: false,
    };
    let glasses = shape(res, Arc::Glass, 0, Some(glass_place), None)?;

    let face_tex = Texture::decode(res.file(Arc::FaceTex, info.faceline_texture as u16)?)?;
    let cap_tex = match cap {
        Some(_) => Some(Texture::decode(res.file(Arc::CapTex, hair_type)?)?),
        None => None,
    };
    let noseline_tex = match noseline {
        Some(_) => Some(Texture::decode(res.file(Arc::NlineTex, info.nose_type as u16)?)?),
        None => None,
    };
    let glass_tex = Texture::decode(res.file(Arc::GlassTex, info.glass_type as u16)?)?;

    let skin = FACELINE_COLOR[info.faceline_color as usize];
    // A flipped hair is mirrored, which turns its triangles inside out.
    let hair_cull = if info.hair_flip { Cull::Front } else { Cull::Back };
    let favorite = FAVORITE_COLOR[info.favorite_color as usize];
    let parts = [
        (&beard, Material::Konst(HAIR_COLOR[info.beard_color as usize]), Cull::Back, Pass::Opaque),
        (&nose, Material::Konst(skin), Cull::Back, Pass::Opaque),
        (&forehead, Material::Konst(skin), hair_cull, Pass::Opaque),
        (&hair, Material::Konst(HAIR_COLOR[info.hair_color as usize]), hair_cull, Pass::Opaque),
    ];
    for (mesh, material, cull, pass) in parts {
        render(target, mesh.as_ref(), &material, cull, pass);
    }
    if let (Some(mesh), Some(tex)) = (&cap, &cap_tex) {
        render(target, Some(mesh), &Material::Cap(favorite, tex), hair_cull, Pass::Opaque);
    }
    render(target, face.as_ref(), &Material::Face(skin, &face_tex), Cull::Back, Pass::Opaque);
    // Fully transparent textures draw nothing (the alpha test drops every pixel).
    if let Some(visible) = mask.visible() {
        render(target, mask_mesh.as_ref(), &Material::Mask(mask, visible), Cull::Back, Pass::Translucent);
    }
    if let (Some(mesh), Some(tex)) = (&noseline, &noseline_tex) {
        render(target, Some(mesh), &Material::Noseline(tex), Cull::Back, Pass::Translucent);
    }
    if glass_tex.visible().is_some() {
        let glass_color = GLASS_COLOR[info.glass_color as usize];
        render(target, glasses.as_ref(), &Material::Glasses(glass_color, &glass_tex), Cull::None, Pass::Translucent);
    }
    Ok(())
}

fn render(target: &mut Target, mesh: Option<&Mesh>, material: &Material, cull: Cull, pass: Pass) {
    let Some(mesh) = mesh else { return };
    let size = target.size as f32;
    let vertex = |c: [u8; 3]| {
        let p = mesh.pos[c[0] as usize];
        let n = mesh.nrm[c[1] as usize];
        let v = [p[0] - EYE[0], p[1] - EYE[1], p[2] - EYE[2]];
        // GX_DF_CLAMP diffuse from a point light, no attenuation, white material.
        let l = [LIGHT[0] - v[0], LIGHT[1] - v[1], LIGHT[2] - v[2]];
        let len = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
        let diffuse = ((n[0] * l[0] + n[1] * l[1] + n[2] * l[2]) / len).max(0.0);
        let w = -v[2];
        Vertex {
            x: (0.5 + 0.5 * FOCAL * v[0] / w) * size,
            y: (0.5 - 0.5 * FOCAL * v[1] / w) * size,
            w,
            lit: (AMBIENT + diffuse).min(1.0),
            uv: mesh.txc[c[2] as usize],
        }
    };
    let mut shade = |lit: f32, uv: [f32; 2]| material.shade(lit, uv);
    for t in &mesh.tris {
        let v = t.map(vertex);
        // GX clips at the planes; the head lies well within them.
        if v.iter().all(|v| DEPTH.contains(&v.w)) {
            target.triangle(v, cull, pass, &mut shade);
        }
    }
}
