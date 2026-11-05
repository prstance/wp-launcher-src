//! A small triangle rasterizer standing in for GX: depth buffer, culling, perspective
//! correct attributes, top-left fill rule (so translucent meshes don't blend twice
//! along shared edges).

/// A vertex after projection: screen position, distance from the camera, and the
/// attributes the shading reads.
#[derive(Clone, Copy)]
pub struct Vertex {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub lit: f32,
    pub uv: [f32; 2],
}

#[derive(Clone, Copy, PartialEq)]
pub enum Cull {
    Back,
    Front,
    None,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Pass {
    /// Writes color and depth.
    Opaque,
    /// Depth tested only, blended with the source alpha.
    Translucent,
}

/// Premultiplied RGBA and depth.
pub struct Target {
    pub size: usize,
    color: Vec<[f32; 4]>,
    depth: Vec<f32>,
}

/// Subpixel steps per pixel.
const SUB: f32 = 16.0;

impl Target {
    pub fn new(size: usize) -> Target {
        Target { size, color: vec![[0.0; 4]; size * size], depth: vec![f32::INFINITY; size * size] }
    }

    /// Draws one triangle; `shade` gets the lighting and texture coordinates and
    /// returns straight RGBA.
    pub fn triangle(&mut self, v: [Vertex; 3], cull: Cull, pass: Pass, shade: &mut impl FnMut(f32, [f32; 2]) -> [f32; 4]) {
        if v.iter().any(|v| v.w <= 0.0 || !v.x.is_finite() || !v.y.is_finite()) {
            return;
        }
        let p = v.map(|v| ((v.x * SUB).round() as i64, (v.y * SUB).round() as i64));
        let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
        // GX's front faces wind clockwise on screen (y down): area > 0.
        let culled = match cull {
            Cull::Back => area <= 0,
            Cull::Front => area >= 0,
            Cull::None => area == 0,
        };
        if culled {
            return;
        }
        let (v, p, area) = if area < 0 { ([v[0], v[2], v[1]], [p[0], p[2], p[1]], -area) } else { (v, p, area) };

        let size = self.size as i64;
        let min = |a: i64, b: i64, c: i64| a.min(b).min(c);
        let max = |a: i64, b: i64, c: i64| a.max(b).max(c);
        let sub = SUB as i64;
        let x0 = (min(p[0].0, p[1].0, p[2].0) / sub).max(0);
        let x1 = (max(p[0].0, p[1].0, p[2].0) / sub + 1).min(size);
        let y0 = (min(p[0].1, p[1].1, p[2].1) / sub).max(0);
        let y1 = (max(p[0].1, p[1].1, p[2].1) / sub + 1).min(size);
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        // Edge i is opposite vertex i; its function is that vertex's weight × area.
        struct Edge {
            dx: i64,
            dy: i64,
            ax: i64,
            ay: i64,
            bias: i64,
        }
        let edge = |a: (i64, i64), b: (i64, i64)| {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let top_left = (dy == 0 && dx > 0) || dy < 0;
            Edge { dx, dy, ax: a.0, ay: a.1, bias: if top_left { 0 } else { -1 } }
        };
        let edges = [edge(p[1], p[2]), edge(p[2], p[0]), edge(p[0], p[1])];
        let inv_area = 1.0 / area as f32;
        let iw = v.map(|v| 1.0 / v.w);

        for py in y0..y1 {
            let cy = py * sub + sub / 2;
            let cx = x0 * sub + sub / 2;
            let mut e = edges.each_ref().map(|e| e.dx * (cy - e.ay) - e.dy * (cx - e.ax));
            let step = edges.each_ref().map(|e| -e.dy * sub);
            for px in x0..x1 {
                if e[0] + edges[0].bias >= 0 && e[1] + edges[1].bias >= 0 && e[2] + edges[2].bias >= 0 {
                    let l = [e[0] as f32 * inv_area, e[1] as f32 * inv_area, e[2] as f32 * inv_area];
                    let q = [l[0] * iw[0], l[1] * iw[1], l[2] * iw[2]];
                    let sum = q[0] + q[1] + q[2];
                    let depth = 1.0 / sum;
                    let i = (py * size + px) as usize;
                    // GX_LEQUAL.
                    if depth <= self.depth[i] {
                        let lerp = |a: f32, b: f32, c: f32| (q[0] * a + q[1] * b + q[2] * c) * depth;
                        let lit = lerp(v[0].lit, v[1].lit, v[2].lit);
                        let uv = [lerp(v[0].uv[0], v[1].uv[0], v[2].uv[0]), lerp(v[0].uv[1], v[1].uv[1], v[2].uv[1])];
                        let [r, g, b, a] = shade(lit, uv);
                        let dst = &mut self.color[i];
                        match pass {
                            Pass::Opaque => {
                                *dst = [r, g, b, 1.0];
                                self.depth[i] = depth;
                            }
                            // GXSetAlphaCompare(GX_GREATER, 0) then SRCALPHA / INVSRCALPHA.
                            Pass::Translucent if a * 255.0 >= 0.5 => {
                                *dst = [
                                    r * a + dst[0] * (1.0 - a),
                                    g * a + dst[1] * (1.0 - a),
                                    b * a + dst[2] * (1.0 - a),
                                    a + dst[3] * (1.0 - a),
                                ];
                            }
                            Pass::Translucent => {}
                        }
                    }
                }
                for k in 0..3 {
                    e[k] += step[k];
                }
            }
        }
    }

    /// Averages `factor`×`factor` blocks into straight 8-bit RGBA.
    pub fn downsample(&self, factor: usize) -> Vec<u8> {
        let out = self.size / factor;
        let mut rgba = Vec::with_capacity(out * out * 4);
        let mut sums = vec![[0.0f32; 4]; out];
        let n = (factor * factor) as f32;
        let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        for band in self.color.chunks_exact(self.size * factor) {
            sums.fill([0.0; 4]);
            for row in band.chunks_exact(self.size) {
                for (sum, block) in sums.iter_mut().zip(row.chunks_exact(factor)) {
                    for c in block {
                        for k in 0..4 {
                            sum[k] += c[k];
                        }
                    }
                }
            }
            for sum in &sums {
                if sum[3] <= 0.0 {
                    rgba.extend_from_slice(&[0; 4]);
                } else {
                    let a = sum[3] / n;
                    rgba.extend_from_slice(&[to8(sum[0] / sum[3]), to8(sum[1] / sum[3]), to8(sum[2] / sum[3]), to8(a)]);
                }
            }
        }
        rgba
    }
}
