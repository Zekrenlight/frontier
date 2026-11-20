//! The minimap's frames, drawn in code: a plain rim, or a strip of old paper with a frayed
//! outer edge, mottled fibres and a scorched line where it meets the map. Both work for the
//! square and the round minimap. Each is made once per shape and kept.

use crate::config::{FrameStyle, Shape};

/// The minimap picture's layout. Sizes are in map pixels (one map pixel is a zoom step's
/// blocks); `scale` is how many texels of the texture each map pixel takes, so a sharp picture
/// draws every block in a whole number of texels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geo {
    /// Map pixels across the map.
    pub map: usize,
    /// Room round the map for the frame.
    pub margin: usize,
    /// Texels per map pixel.
    pub scale: usize,
}

/// The first layout: 128 map pixels, one texel each (a 148-texel picture).
pub const CLASSIC: Geo = Geo {
    map: 128,
    margin: 10,
    scale: 1,
};
/// Twice the texels: 108 map pixels at two texels each (a 256-texel picture, the most a
/// medium dynamic texture holds).
pub const SHARP: Geo = Geo {
    map: 108,
    margin: 10,
    scale: 2,
};

impl Geo {
    /// The picture's side in map pixels.
    pub fn side(self) -> usize {
        self.map + 2 * self.margin
    }
    /// The texture's side in texels.
    pub fn texels(self) -> usize {
        self.side() * self.scale
    }
}

/// The layout for the Sharp maps setting.
pub fn geo(sharp: bool) -> Geo {
    if sharp {
        SHARP
    } else {
        CLASSIC
    }
}

/// A repeatable pseudo-random number in 0..1 for integer `i` and stream `seed`.
fn hash(i: i64, seed: u32) -> f32 {
    let mut x = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ u64::from(seed).wrapping_mul(0xD6E8_FEB8_6659_FD93);
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

/// Smooth 1D noise along `t`, period `period` samples (so a round edge closes on itself).
fn noise1(t: f32, seed: u32, period: i64) -> f32 {
    let i = t.floor() as i64;
    let f = t - t.floor();
    let a = hash(i.rem_euclid(period.max(1)), seed);
    let b = hash((i + 1).rem_euclid(period.max(1)), seed);
    let s = f * f * (3.0 - 2.0 * f);
    a + (b - a) * s
}

/// Three octaves of `noise1`, in 0..1.
fn fray(t: f32, seed: u32, period: f32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.55;
    let mut freq = 1.0;
    let mut norm = 0.0;
    for o in 0..3 {
        let p = (period * freq).round() as i64;
        sum += amp * noise1(t * freq, seed + o, p);
        norm += amp;
        amp *= 0.5;
        freq *= 2.3;
    }
    sum / norm
}

/// Smooth 2D value noise for the paper's mottling.
fn noise2(x: f32, y: f32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i64, y.floor() as i64);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let h = |i: i64, j: i64| {
        hash(
            i.wrapping_mul(73_856_093) ^ j.wrapping_mul(19_349_663),
            seed,
        )
    };
    let s = |f: f32| f * f * (3.0 - 2.0 * f);
    let top = h(xi, yi) + (h(xi + 1, yi) - h(xi, yi)) * s(fx);
    let bot = h(xi, yi + 1) + (h(xi + 1, yi + 1) - h(xi, yi + 1)) * s(fx);
    top + (bot - top) * s(fy)
}

/// For a pixel centre `(x, y)` of the picture: how far outside the map's edge it lies (negative
/// inside) and where along the edge it is, in pixels round the outline from the top left.
fn edge_coords(g: Geo, shape: Shape, x: f32, y: f32) -> (f32, f32, f32) {
    let c = g.side() as f32 / 2.0;
    let half = g.map as f32 / 2.0;
    match shape {
        Shape::Round => {
            let (dx, dy) = (x - c, y - c);
            let r = (dx * dx + dy * dy).sqrt();
            let around = dy.atan2(dx) / std::f32::consts::TAU + 0.5;
            let perimeter = std::f32::consts::TAU * half;
            (r - half, around * perimeter, perimeter)
        }
        Shape::Square => {
            let (dx, dy) = (x - c, y - c);
            let d = dx.abs().max(dy.abs()) - half;
            // Position along the square's outline, clockwise from the top left corner.
            let side = g.map as f32;
            let (u, v) = (x - g.margin as f32, y - g.margin as f32);
            let along = if dy.abs() >= dx.abs() {
                if dy < 0.0 {
                    u
                } else {
                    2.0 * side + (side - u)
                }
            } else if dx > 0.0 {
                side + v
            } else {
                3.0 * side + (side - v)
            };
            (d, along, 4.0 * side)
        }
    }
}

/// The centre of texel `(x, y)` in map pixels.
fn centre(g: Geo, x: usize, y: usize) -> (f32, f32) {
    let k = g.scale as f32;
    ((x as f32 + 0.5) / k, (y as f32 + 0.5) / k)
}

/// Whether texel `(x, y)` of the picture shows the map.
pub fn in_map(g: Geo, shape: Shape, x: usize, y: usize) -> bool {
    let (px, py) = centre(g, x, y);
    let (d, _, _) = edge_coords(g, shape, px, py);
    d < 0.0
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn pack(a: f32, c: [f32; 3]) -> u32 {
    let q = |v: f32| v.round().clamp(0.0, 255.0) as u32;
    (q(a) << 24) | (q(c[0]) << 16) | (q(c[1]) << 8) | q(c[2])
}

/// The frame's texels (ARGB, row by row, `g.texels()` square), clear where it draws nothing.
/// Drawn over the map, so a frame may bite a little into it. The same frame at any scale,
/// only finer.
pub fn build(g: Geo, style: FrameStyle, shape: Shape) -> Vec<u32> {
    let n = g.texels();
    let mut out = vec![0u32; n * n];
    match style {
        FrameStyle::None => {}
        FrameStyle::Plain => {
            for y in 0..n {
                for x in 0..n {
                    let (px, py) = centre(g, x, y);
                    let (d, _, _) = edge_coords(g, shape, px, py);
                    out[y * n + x] = if (0.0..1.0).contains(&d) {
                        0xFF_C8C8C8
                    } else if (1.0..3.0).contains(&d) {
                        0xFF_2A2A2A
                    } else if (3.0..4.0).contains(&d) {
                        0x80_000000
                    } else {
                        0
                    };
                }
            }
        }
        FrameStyle::Parchment => {
            // Paper tones: a warm cream, a dusty fold colour and the scorched brown of the
            // edges.
            const CREAM: [f32; 3] = [236.0, 222.0, 186.0];
            const DUST: [f32; 3] = [205.0, 182.0, 138.0];
            const SCORCH: [f32; 3] = [112.0, 78.0, 44.0];
            const INK: [f32; 3] = [70.0, 50.0, 34.0];
            let band = g.margin as f32 - 1.0;
            for y in 0..n {
                for x in 0..n {
                    let (px, py) = centre(g, x, y);
                    let (d, along, perimeter) = edge_coords(g, shape, px, py);
                    // The paper starts a little inside the map, unevenly...
                    let inner = -1.2 + 1.4 * fray(along / 9.0, 11, perimeter / 9.0);
                    // ...and its outer edge is torn: mostly near the band's end, with bites.
                    let torn = fray(along / 5.0, 23, perimeter / 5.0);
                    let bite = fray(along / 2.0, 37, perimeter / 2.0);
                    let outer = band * (0.62 + 0.38 * torn) - 1.6 * (bite - 0.5).max(0.0) * 2.0;
                    if d < inner || d > outer {
                        // A soft shadow just past the torn edge.
                        if d > outer && d < outer + 1.5 {
                            out[y * n + x] =
                                pack(70.0 * (1.0 - (d - outer) / 1.5), [20.0, 14.0, 8.0]);
                        }
                        continue;
                    }
                    let mottle =
                        noise2(px / 6.0, py / 6.0, 5) * 0.65 + noise2(px / 2.0, py / 2.0, 7) * 0.35;
                    let mut c = mix(CREAM, DUST, (mottle - 0.25).clamp(0.0, 1.0));
                    // Fibres: short dark flecks.
                    let cell = (py.floor() as usize) * g.side() + px.floor() as usize;
                    if hash(cell as i64, 91) > 0.965 {
                        c = mix(c, SCORCH, 0.35);
                    }
                    // Browned near both edges, darkest at the torn one.
                    let to_outer = (outer - d).max(0.0);
                    let to_inner = (d - inner).max(0.0);
                    if to_outer < 2.2 {
                        c = mix(c, SCORCH, (1.0 - to_outer / 2.2).powf(1.4) * 0.85);
                    }
                    if to_inner < 1.0 {
                        c = mix(c, INK, 0.8 * (1.0 - to_inner));
                    }
                    out[y * n + x] = pack(255.0, c);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_map_is_round_with_a_plain_rim() {
        let d = crate::config::Config::default();
        assert_eq!((d.shape, d.frame), (Shape::Round, FrameStyle::Plain));
        // The shape and frame keys (the round shape and plain frame are the defaults).
        assert_eq!(
            (crate::config::KEY_SHAPE, crate::config::KEY_FRAME),
            ("minimap_shape", "minimap_frame")
        );
        for g in [CLASSIC, SHARP] {
            let n = g.texels();
            let f = build(g, FrameStyle::Plain, Shape::Round);
            let (c, k) = (n / 2, g.scale);
            // The rim runs round the circle: just outside the map on each axis it is drawn...
            for (x, y) in [
                (c + (g.map / 2 + 1) * k, c),
                (c, c + (g.map / 2 + 1) * k),
                (c - (g.map / 2 + 2) * k, c),
            ] {
                assert_eq!(f[y * n + x] >> 24, 0xFF, "{x},{y}");
                assert!(!in_map(g, Shape::Round, x, y));
            }
            // ...and the square's corners beyond it are clear, the map not shown there either.
            let corner = c + (g.map / 2 - 4) * k;
            assert_eq!(f[corner * n + corner], 0);
            assert!(!in_map(g, Shape::Round, corner, corner));
            assert!(in_map(g, Shape::Square, corner, corner));
        }
    }

    #[test]
    fn frames_leave_the_middle_of_the_map_clear() {
        for g in [CLASSIC, SHARP] {
            let n = g.texels();
            for shape in [Shape::Square, Shape::Round] {
                for style in [FrameStyle::Plain, FrameStyle::Parchment, FrameStyle::None] {
                    let f = build(g, style, shape);
                    assert_eq!(f[(n / 2) * n + n / 2], 0);
                    assert_eq!(f.len(), n * n);
                }
                assert!(in_map(g, shape, n / 2, n / 2));
                assert!(!in_map(g, shape, 0, 0));
            }
            let p = build(g, FrameStyle::Parchment, Shape::Square);
            assert!(p.iter().filter(|&&c| c >> 24 == 255).count() > 4 * g.texels() * 4);
        }
        assert!(SHARP.texels() <= 256);
    }

    /// `cargo test --lib frame_preview -- --ignored` writes the frames to `target/` to look at.
    #[test]
    #[ignore]
    fn frame_preview() {
        let g = SHARP;
        for (shape, name) in [(Shape::Square, "square"), (Shape::Round, "round")] {
            for (style, sname) in [
                (FrameStyle::Plain, "plain"),
                (FrameStyle::Parchment, "parchment"),
            ] {
                let f = build(g, style, shape);
                let mut rgba = Vec::new();
                for c in f {
                    let a = c >> 24;
                    // On a grey checker so the clear parts show.
                    let bg = if a == 0 { 0x60 } else { 0 };
                    let _ = bg;
                    rgba.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8, a as u8]);
                }
                let png = crate::png::encode_stored(g.texels() as u32, g.texels() as u32, &rgba);
                std::fs::write(format!("target/frame_{sname}_{name}.png"), png).unwrap();
            }
        }
    }
}
