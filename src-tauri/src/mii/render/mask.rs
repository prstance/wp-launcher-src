//! The face "mask" texture: eyes, eyebrows, mouth, mustache and mole drawn flat into
//! one square image (RFL_MakeTex.c: RFLiSetFaceParts, RFLiCapture, RFLiDrawQuad).

use super::info::{Info, Part};
use super::texture::{Texture, Wrap};
use super::{Arc, Resources};

const SCALE_X: f32 = 0.889_614_64;
const SCALE_Y: f32 = 0.927_667_5;

const EYE_ROT_OFFSET: [u8; 50] = [
    29, 28, 28, 28, 29, 28, 28, 28, 29, 28, 28, 28, 28, 29, 29, 28, 28, 28, 29, 29, 28, 29, 28, 29, 29, 28, 29, 28, 28, 29, 28, 28, 28, 29,
    29, 29, 28, 28, 29, 29, 29, 28, 28, 29, 29, 29, 29, 29, 28, 28,
];
const EYEBROW_ROT_OFFSET: [u8; 24] = [26, 26, 27, 25, 26, 25, 26, 25, 28, 25, 26, 24, 27, 27, 26, 26, 25, 25, 26, 26, 27, 26, 25, 27];

const MOLE_COLOR: [u8; 3] = [18, 15, 15];
const EYE_COLOR: [[u8; 3]; 6] = [[0, 0, 0], [124, 128, 128], [112, 80, 64], [112, 110, 64], [88, 104, 184], [72, 128, 104]];
/// Also RFLi_BEARD_COLOR0, and the same as the hair colors.
pub const HAIR_COLOR: [[u8; 3]; 8] =
    [[30, 26, 24], [56, 32, 21], [85, 38, 23], [112, 64, 36], [114, 114, 120], [73, 54, 26], [122, 89, 40], [193, 159, 100]];
const MOUTH_COLOR: [[[u8; 3]; 2]; 3] = [[[190, 78, 38], [113, 42, 4]], [[216, 48, 40], [120, 21, 16]], [[207, 68, 71], [126, 37, 40]]];

#[derive(Clone, Copy, PartialEq)]
enum Origin {
    Center,
    Left,
    Right,
}

/// How the TEV colors a part (RFLiSetTev4*).
#[derive(Clone, Copy)]
enum Tev {
    /// One color, the texture's alpha.
    Flat([u8; 3]),
    /// The texture's channels pick between colors: red → first, green → second, blue → white.
    Mouth([u8; 3], [u8; 3]),
    /// Same, but blue → second and green → white.
    Eye([u8; 3], [u8; 3]),
}

struct Quad<'a> {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    angle: f32,
    origin: Origin,
    texture: &'a Texture,
    tev: Tev,
}

/// RFLi_TEX_SCALE2DIM.
fn dim(scale: u8) -> f32 {
    (1.0 + 0.4f32 as f64 * scale as f64) as f32
}

/// RFLi_TEX_ROTATE2ANG.
fn angle(rotate: u32) -> f32 {
    (360.0 / 32.0) * (rotate % 32) as f32
}

/// Draws the Normal expression's mask at `size`×`size`.
pub fn make(res: &Resources, info: &Info, size: usize) -> Result<Texture, String> {
    let load = |arc: Arc, file: u8| res.file(arc, file as u16).and_then(Texture::decode);
    let (eye, eyebrow) = (load(Arc::Eye, info.eye.kind)?, load(Arc::Eyebrow, info.eyebrow.kind)?);
    let (mouth, mustache, mole) =
        (load(Arc::Mouth, info.mouth_type)?, load(Arc::Mustache, info.mustache)?, load(Arc::Mole, info.mole_type)?);

    let unit = size as f32 / 64.0;
    let eye_color0 = match info.eye.kind {
        9 => [255, 130, 0],
        20 => [0, 255, 255],
        _ => [0, 0, 0],
    };
    let eyes = pair(
        unit,
        &info.eye,
        18.451525,
        342.0 / 64.0,
        288.0 / 64.0,
        EYE_ROT_OFFSET[info.eye.kind as usize],
        &eye,
        Tev::Eye(eye_color0, EYE_COLOR[info.eye.color as usize]),
    );
    let eyebrows = pair(
        unit,
        &info.eyebrow,
        16.549807,
        324.0 / 64.0,
        288.0 / 64.0,
        EYEBROW_ROT_OFFSET[info.eyebrow.kind as usize],
        &eyebrow,
        Tev::Flat(HAIR_COLOR[info.eyebrow.color as usize]),
    );
    let mustache_y = y(31.763554, info.beard_y) * unit;
    let mustaches = [Origin::Right, Origin::Left].map(|origin| Quad {
        x: 32.0 * unit,
        y: mustache_y,
        width: 288.0 / 64.0 * dim(info.beard_scale) * unit,
        height: 576.0 / 64.0 * dim(info.beard_scale) * unit,
        angle: 0.0,
        origin,
        texture: &mustache,
        tev: Tev::Flat(HAIR_COLOR[info.beard_color as usize]),
    });
    let [m0, m1] = MOUTH_COLOR[info.mouth_color as usize];
    let mouth = Quad {
        x: 32.0 * unit,
        y: y(29.25885, info.mouth_y) * unit,
        width: 396.0 / 64.0 * dim(info.mouth_scale) * unit,
        height: 288.0 / 64.0 * dim(info.mouth_scale) * unit,
        angle: 0.0,
        origin: Origin::Center,
        texture: &mouth,
        tev: Tev::Mouth(m0, m1),
    };
    let mole = Quad {
        x: (17.766165 + 2.0 * SCALE_X * info.mole_x as f32) * unit,
        y: y(17.95986, info.mole_y) * unit,
        width: dim(info.mole_scale) * unit,
        height: dim(info.mole_scale) * unit,
        angle: 0.0,
        origin: Origin::Center,
        texture: &mole,
        tev: Tev::Flat(MOLE_COLOR),
    };

    // RFLiCapture draws the parts twice with "under" blending, so the first one drawn
    // stays on top: once for color (and a by-product alpha), once for the real alpha.
    let mut color = vec![[0f32; 3]; size * size];
    let mut alpha1 = vec![0f32; size * size];
    let mut alpha2 = vec![0f32; size * size];
    let [m0, m1] = mustaches;
    let [er, el] = eyes;
    let [br, bl] = eyebrows;
    for quad in [m0, m1, mouth, br, bl, er, el, mole] {
        draw(&quad, size, |i, c, a| {
            let d = alpha1[i];
            for k in 0..3 {
                color[i][k] = c[k] * (1.0 - d) + color[i][k] * d;
            }
            alpha1[i] = a * (1.0 - d) + d * d;
            alpha2[i] = a * (1.0 - alpha2[i]) + alpha2[i];
        });
    }
    let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let texels = color.iter().zip(&alpha2).map(|(c, &a)| [to8(c[0]), to8(c[1]), to8(c[2]), to8(a)]).collect();
    Ok(Texture::from_rgba(size, texels))
}

/// The texture y of a part: its base, then its `y` parameter.
fn y(base: f32, y: u8) -> f32 {
    base + 1.1600001 * SCALE_Y * y as f32
}

/// The two eyes or eyebrows, mirrored: each quad's origin is its inner end.
#[allow(clippy::too_many_arguments)]
fn pair<'a>(unit: f32, p: &Part, base_y: f32, w: f32, h: f32, rot: u8, texture: &'a Texture, tev: Tev) -> [Quad<'a>; 2] {
    let x = SCALE_X * p.x as f32;
    let a = angle(p.rotate as u32 + rot as u32);
    let q = |x: f32, angle: f32, origin| Quad {
        x: unit * x,
        y: y(base_y, p.y) * unit,
        width: w * dim(p.scale) * unit,
        height: h * dim(p.scale) * unit,
        angle,
        origin,
        texture,
        tev,
    };
    [q(32.0 - x, a, Origin::Right), q(32.0 + x, 360.0 - a, Origin::Left)]
}

/// Rasterizes a part's quad, handing each covered pixel's TEV output to `blend`.
fn draw(q: &Quad, size: usize, mut blend: impl FnMut(usize, [f32; 3], f32)) {
    let (sin, cos) = q.angle.to_radians().sin_cos();
    let base_x = match q.origin {
        Origin::Center => -0.5,
        Origin::Right => -1.0,
        Origin::Left => 0.0,
    };
    // pos = translate · scale(SCALE) · rotate · scale(width, height)
    let forward = |lx: f32, ly: f32| {
        let (x, y) = (lx * q.width, ly * q.height);
        let (x, y) = (cos * x - sin * y, sin * x + cos * y);
        (q.x + SCALE_X * x, q.y + SCALE_Y * y)
    };
    let corners = [forward(base_x, -0.5), forward(base_x + 1.0, -0.5), forward(base_x, 0.5), forward(base_x + 1.0, 0.5)];
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (x, y) in corners {
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
    }
    let span = |a: f32, b: f32| (a.floor().max(0.0) as usize, (b.ceil().max(0.0) as usize).min(size));
    let ((px0, px1), (py0, py1)) = (span(x0, x1), span(y0, y1));
    for py in py0..py1 {
        for px in px0..px1 {
            let (dx, dy) = ((px as f32 + 0.5 - q.x) / SCALE_X, (py as f32 + 0.5 - q.y) / SCALE_Y);
            let lx = (cos * dx + sin * dy) / q.width;
            let ly = (-sin * dx + cos * dy) / q.height;
            if lx < base_x || lx >= base_x + 1.0 || !(-0.5..0.5).contains(&ly) {
                continue;
            }
            let s = if q.origin == Origin::Left { 1.0 - lx } else { lx - base_x };
            // RFLiInitRFLTexture forces GX_CLAMP.
            let [r, g, b, a] = q.texture.sample_wrap([Wrap::Clamp; 2], s, ly + 0.5);
            // GXSetAlphaCompare(GX_GREATER, 0): 8-bit alpha above zero.
            if a * 255.0 < 0.5 {
                continue;
            }
            let f = |c: [u8; 3]| c.map(|v| v as f32 / 255.0);
            let c = match q.tev {
                Tev::Flat(c) => f(c),
                Tev::Mouth(c0, c1) | Tev::Eye(c0, c1) => {
                    let (k1, k2) = if matches!(q.tev, Tev::Mouth(..)) { (g, b) } else { (b, g) };
                    let (c0, c1) = (f(c0), f(c1));
                    // Three TEV stages, each clamped.
                    [0, 1, 2].map(|k| (((c0[k] * r).min(1.0) + c1[k] * k1).min(1.0) + k2).min(1.0))
                }
            };
            blend(py * size + px, c, a);
        }
    }
}
