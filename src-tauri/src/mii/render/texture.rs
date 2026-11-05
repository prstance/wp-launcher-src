//! GX textures as RFL stores them: an RFLiTexture header (RFLi_Texture.h) then the
//! image in GX's tiled layout, decoded like Dolphin's TextureDecoder_Generic.cpp.

#[derive(Clone, Copy, PartialEq)]
pub enum Wrap {
    Clamp,
    Repeat,
    Mirror,
}

pub struct Texture {
    width: usize,
    height: usize,
    pub wrap: [Wrap; 2],
    texels: Vec<[u8; 4]>,
}

impl Texture {
    pub fn decode(file: &[u8]) -> Result<Texture, String> {
        let bad = || "bad texture".to_string();
        let header = file.get(..32).ok_or_else(bad)?;
        let be16 = |at: usize| u16::from_be_bytes([header[at], header[at + 1]]) as usize;
        let (format, width, height) = (header[0], be16(2), be16(4));
        let offset = u32::from_be_bytes([header[0x1C], header[0x1D], header[0x1E], header[0x1F]]) as usize;
        let wrap = |w: u8| match w {
            1 => Wrap::Repeat,
            2 => Wrap::Mirror,
            _ => Wrap::Clamp,
        };
        if width == 0 || height == 0 {
            return Err(bad());
        }
        let texels = decode_gx(format, width, height, file.get(offset..).ok_or_else(bad)?)?;
        Ok(Texture { width, height, wrap: [wrap(header[6]), wrap(header[7])], texels })
    }

    /// An 8-bit RGBA image, clamped at its edges.
    pub fn from_rgba(size: usize, texels: Vec<[u8; 4]>) -> Texture {
        Texture { width: size, height: size, wrap: [Wrap::Clamp; 2], texels }
    }

    /// GX_LINEAR filtering at (s, t), with the given wrap modes.
    pub fn sample_wrap(&self, wrap: [Wrap; 2], s: f32, t: f32) -> [f32; 4] {
        let u = s * self.width as f32 - 0.5;
        let v = t * self.height as f32 - 0.5;
        let (x0, y0) = (floor(u), floor(v));
        let (fx, fy) = (u - x0 as f32, v - y0 as f32);
        let xs = [texel_at(x0, self.width, wrap[0]), texel_at(x0 + 1, self.width, wrap[0])];
        let ys = [texel_at(y0, self.height, wrap[1]), texel_at(y0 + 1, self.height, wrap[1])];
        let at = |x: usize, y: usize| self.texels[ys[y] * self.width + xs[x]];
        let (a, b, c, d) = (at(0, 0), at(1, 0), at(0, 1), at(1, 1));
        let mut out = [0.0; 4];
        for k in 0..4 {
            let top = a[k] as f32 + (b[k] as f32 - a[k] as f32) * fx;
            let bottom = c[k] as f32 + (d[k] as f32 - c[k] as f32) * fx;
            out[k] = (top + (bottom - top) * fy) * (1.0 / 255.0);
        }
        out
    }

    pub fn sample(&self, s: f32, t: f32) -> [f32; 4] {
        self.sample_wrap(self.wrap, s, t)
    }

    /// The (s0, t0, s1, t1) box outside which sampling this clamped texture gives
    /// zero alpha, or None if it is fully transparent.
    pub fn visible(&self) -> Option<[f32; 4]> {
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        for (i, t) in self.texels.iter().enumerate() {
            if t[3] != 0 {
                let (x, y) = (i % self.width, i / self.width);
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
        // Bilinear filtering reaches one texel further.
        (x0 != usize::MAX).then(|| {
            let (w, h) = (self.width as f32, self.height as f32);
            [(x0 as f32 - 1.0) / w, (y0 as f32 - 1.0) / h, (x1 as f32 + 1.0) / w, (y1 as f32 + 1.0) / h]
        })
    }
}

/// A GX image (`format`: GXTexFmt, I4 to RGB5A3) in its tiled layout, as RGBA texels
/// row by row. RFL's textures and the game's TPL thumbnails both use it.
pub fn decode_gx(format: u8, width: usize, height: usize, image: &[u8]) -> Result<Vec<[u8; 4]>, String> {
    // Block size and bits per texel of each format.
    let (bw, bh, bits) = match format {
        0 => (8, 8, 4),      // I4
        1 | 2 => (8, 4, 8),  // I8, IA4
        3..=5 => (4, 4, 16), // IA8, RGB565, RGB5A3
        _ => return Err(format!("unsupported texture format {format}")),
    };
    let (bx, by) = (width.div_ceil(bw), height.div_ceil(bh));
    let image = image.get(..bx * by * bw * bh * bits / 8).ok_or("truncated texture")?;
    let mut texels = vec![[0; 4]; width * height];
    let mut n = 0;
    for block_y in 0..by {
        for block_x in 0..bx {
            for y in block_y * bh..block_y * bh + bh {
                for x in block_x * bw..block_x * bw + bw {
                    let texel = match bits {
                        4 => {
                            let b = image[n / 2];
                            let i = (if n % 2 == 0 { b >> 4 } else { b & 15 }) * 17;
                            [i; 4]
                        }
                        8 => {
                            let b = image[n];
                            if format == 1 {
                                [b; 4]
                            } else {
                                let i = (b & 15) * 17;
                                [i, i, i, (b >> 4) * 17]
                            }
                        }
                        _ => {
                            let (hi, lo) = (image[n * 2], image[n * 2 + 1]);
                            let v = u16::from_be_bytes([hi, lo]);
                            match format {
                                3 => [lo, lo, lo, hi],
                                4 => [five(v >> 11), six(v >> 5), five(v), 255],
                                _ if v & 0x8000 != 0 => [five(v >> 10), five(v >> 5), five(v), 255],
                                _ => {
                                    let a = ((v >> 12) & 7) as u8;
                                    let c = |s: u16| ((v >> s) & 15) as u8 * 17;
                                    [c(8), c(4), c(0), (a << 5) | (a << 2) | (a >> 1)]
                                }
                            }
                        }
                    };
                    if x < width && y < height {
                        texels[y * width + x] = texel;
                    }
                    n += 1;
                }
            }
        }
    }
    Ok(texels)
}

/// f32::floor is a libm call on baseline x86-64.
fn floor(v: f32) -> i32 {
    let i = v as i32;
    i - (v < i as f32) as i32
}

fn five(v: u16) -> u8 {
    let v = (v & 31) as u8;
    (v << 3) | (v >> 2)
}

fn six(v: u16) -> u8 {
    let v = (v & 63) as u8;
    (v << 2) | (v >> 4)
}

fn texel_at(i: i32, n: usize, wrap: Wrap) -> usize {
    let n = n as i32;
    (match wrap {
        Wrap::Clamp => i.clamp(0, n - 1),
        Wrap::Repeat => i.rem_euclid(n),
        Wrap::Mirror => {
            let m = i.rem_euclid(2 * n);
            if m < n {
                m
            } else {
                2 * n - 1 - m
            }
        }
    }) as usize
}
