//! Where things fall on the maps: from the minimap's pixels to the world and back (north up or
//! turned with the player), and the pictures themselves, made pixel by pixel.

use crate::config::Shape;
use crate::frame::{self, Geo};

/// The minimap's background where nothing has been explored yet (ARGB).
pub const MAP_BG: u32 = 0xC8_15130F;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// The block the map is centred on (fractional, for smooth movement).
    pub cx: f64,
    pub cz: f64,
    /// The player's yaw in degrees (0 faces south, 90 west), used when `rotate`.
    pub yaw: f32,
    pub rotate: bool,
    /// Blocks per map pixel.
    pub bpp: f32,
}

impl View {
    /// The world's x and z under map pixel offset `(du, dv)` from the map's centre.
    pub fn world_of(&self, du: f32, dv: f32) -> (f64, f64) {
        let (du, dv) = (f64::from(du * self.bpp), f64::from(dv * self.bpp));
        if self.rotate {
            let y = f64::from(self.yaw).to_radians();
            // Up on the map is where the player looks; right is their right hand.
            let (fx, fz) = (-y.sin(), y.cos());
            let (rx, rz) = (-y.cos(), -y.sin());
            (self.cx + du * rx - dv * fx, self.cz + du * rz - dv * fz)
        } else {
            (self.cx + du, self.cz + dv)
        }
    }

    /// The map pixel offset from the centre where the world's `(x, z)` shows.
    pub fn map_of(&self, x: f64, z: f64) -> (f32, f32) {
        let (dx, dz) = (x - self.cx, z - self.cz);
        let (du, dv) = if self.rotate {
            let y = f64::from(self.yaw).to_radians();
            let (fx, fz) = (-y.sin(), y.cos());
            let (rx, rz) = (-y.cos(), -y.sin());
            (dx * rx + dz * rz, -(dx * fx + dz * fz))
        } else {
            (dx, dz)
        };
        (
            (du / f64::from(self.bpp)) as f32,
            (dv / f64::from(self.bpp)) as f32,
        )
    }

    /// Which way north is on the map, as a map direction `(du, dv)` of length 1.
    pub fn north(&self) -> (f32, f32) {
        let (u, v) = self.map_of(self.cx, self.cz - 1000.0);
        let len = (u * u + v * v).sqrt().max(1e-6);
        (u / len, v / len)
    }
}

/// Pulls an offset back inside the map's outline (less `inset` pixels), for markers of things
/// beyond its edge. Answers the offset and whether it was moved.
pub fn clamp_to_map(g: Geo, shape: Shape, du: f32, dv: f32, inset: f32) -> (f32, f32, bool) {
    let r = g.map as f32 / 2.0 - inset;
    match shape {
        Shape::Round => {
            let d = (du * du + dv * dv).sqrt();
            if d > r {
                (du * r / d, dv * r / d, true)
            } else {
                (du, dv, false)
            }
        }
        Shape::Square => {
            let m = du.abs().max(dv.abs());
            if m > r {
                (du * r / m, dv * r / m, true)
            } else {
                (du, dv, false)
            }
        }
    }
}

/// Which texels of the picture show the map, `g.texels()` square.
pub fn mask(g: Geo, shape: Shape) -> Vec<bool> {
    let n = g.texels();
    (0..n * n)
        .map(|i| frame::in_map(g, shape, i % n, i / n))
        .collect()
}

/// Lays `over` (ARGB) on `under` (ARGB) by `over`'s alpha.
pub fn blend(under: u32, over: u32) -> u32 {
    let a = over >> 24;
    if a == 255 {
        return over;
    }
    if a == 0 {
        return under;
    }
    let ua = under >> 24;
    let mix = |s: u32| {
        let (o, u) = ((over >> s) & 0xFF, (under >> s) & 0xFF);
        (o * a + u * (255 - a)) / 255
    };
    let out_a = a + ua * (255 - a) / 255;
    (out_a << 24) | (mix(16) << 16) | (mix(8) << 8) | mix(0)
}

/// The minimap's picture as RGBA bytes: the map inside the outline, the frame over it.
/// `sample(x, z)` answers the map pixel (ARGB) of a block column, clear for one never seen.
/// Each block is sampled once however many texels it covers (neighbouring texels of one
/// block reuse it), and every texel shows exactly one block: no blending between blocks.
pub fn compose(
    view: &View,
    g: Geo,
    inside: &[bool],
    frame_px: &[u32],
    mut sample: impl FnMut(i32, i32) -> u32,
    out: &mut Vec<u8>,
) {
    let n = g.texels();
    out.clear();
    out.reserve(n * n * 4);
    let half = g.side() as f32 / 2.0;
    let k = g.scale as f32;
    let mut last: Option<((i32, i32), u32)> = None;
    for y in 0..n {
        for x in 0..n {
            let i = y * n + x;
            let mut c = 0u32;
            if inside[i] {
                let (wx, wz) =
                    view.world_of((x as f32 + 0.5) / k - half, (y as f32 + 0.5) / k - half);
                let at = (wx.floor() as i32, wz.floor() as i32);
                let px = match last {
                    Some((l, c)) if l == at => c,
                    _ => {
                        let c = sample(at.0, at.1);
                        last = Some((at, c));
                        c
                    }
                };
                c = blend(MAP_BG, px);
            }
            c = blend(c, frame_px[i]);
            out.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_and_map_offsets_invert_each_other() {
        for rotate in [false, true] {
            for yaw in [0.0f32, 37.0, 90.0, -135.0, 271.0] {
                let v = View {
                    cx: 100.5,
                    cz: -40.25,
                    yaw,
                    rotate,
                    bpp: 2.0,
                };
                let (x, z) = v.world_of(13.0, -7.0);
                let (u, w) = v.map_of(x, z);
                assert!((u - 13.0).abs() < 1e-3 && (w + 7.0).abs() < 1e-3);
            }
        }
    }

    #[test]
    fn turned_map_points_the_way_the_player_looks() {
        // Facing south (yaw 0): up on the map is +z.
        let v = View {
            cx: 0.0,
            cz: 0.0,
            yaw: 0.0,
            rotate: true,
            bpp: 1.0,
        };
        let (x, z) = v.world_of(0.0, -10.0);
        assert!(x.abs() < 1e-6 && (z - 10.0).abs() < 1e-6);
        // Facing west (yaw 90): up is -x, and north is then to the right.
        let v = View { yaw: 90.0, ..v };
        let (x, _) = v.world_of(0.0, -10.0);
        assert!((x + 10.0).abs() < 1e-6);
        let (nu, nv) = v.north();
        assert!(nu > 0.99 && nv.abs() < 1e-3);
        // North up: north is straight up.
        let v = View { rotate: false, ..v };
        assert_eq!(v.north(), (0.0, -1.0));
    }

    #[test]
    fn far_markers_stay_on_the_rim() {
        let g = frame::CLASSIC;
        let (u, v, moved) = clamp_to_map(g, Shape::Round, 300.0, 0.0, 4.0);
        assert!(moved && (u - 60.0).abs() < 1e-3 && v == 0.0);
        let (u, v, moved) = clamp_to_map(g, Shape::Square, 100.0, 200.0, 4.0);
        assert!(moved && (v - 60.0).abs() < 1e-3 && (u - 30.0).abs() < 1e-3);
        assert!(!clamp_to_map(g, Shape::Square, 5.0, 5.0, 4.0).2);
    }


    #[test]
    fn compose_writes_non_transparent_rgba() {
        use crate::frame::CLASSIC;
        let g = CLASSIC;
        let n = g.texels();
        let inside = vec![true; n * n];
        let frame_px = vec![0u32; n * n];
        let v = View {
            cx: 0.0,
            cz: 0.0,
            yaw: 0.0,
            rotate: false,
            bpp: 1.0,
        };
        let mut out = Vec::new();
        compose(&v, g, &inside, &frame_px, |_x, _z| 0xFF_44_88_CC, &mut out);
        assert_eq!(out.len(), n * n * 4);
        let mut opaque = 0usize;
        for px in out.chunks_exact(4) {
            if px[3] != 0 {
                opaque += 1;
                assert_eq!(px, [0x44, 0x88, 0xCC, 0xFF]);
            }
        }
        assert_eq!(opaque, n * n, "map texels should be opaque RGBA, not clear");
    }

}
