//! Turning explored columns into map pixels: relief from the heights around each column, water
//! darkening with depth, a faint lift for high ground, and the light of the time of day.

use crate::store::Column;

/// What an unexplored spot looks like on the maps (ARGB).
pub const UNKNOWN: u32 = 0x00_000000;

pub fn argb(a: u32, r: u32, g: u32, b: u32) -> u32 {
    (a.min(255) << 24) | (r.min(255) << 16) | (g.min(255) << 8) | b.min(255)
}

pub fn channels(rgb: u32) -> [f32; 3] {
    [
        ((rgb >> 16) & 0xFF) as f32,
        ((rgb >> 8) & 0xFF) as f32,
        (rgb & 0xFF) as f32,
    ]
}

/// Multiplies two 0xRRGGBB colours, as a tinted texture is.
pub fn tint(rgb: u32, by: u32) -> u32 {
    let (a, b) = (channels(rgb), channels(by));
    let m = |i: usize| (a[i] * b[i] / 255.0).round() as u32;
    (m(0) << 16) | (m(1) << 8) | m(2)
}

/// How the light falls on the map: the sky's share (1 at noon, about 0.35 at midnight) and
/// whether to use it at all.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub sky: f32,
    pub relief: bool,
}

impl Light {
    pub const FULL: Self = Self {
        sky: 1.0,
        relief: true,
    };
}

/// The sky's brightness for a day time (ticks, 0 is sunrise): a smooth rise and fall with
/// short dusk and dawn, kept above 0.35 so the map is still readable at night.
pub fn sky_light(day_time: i64) -> f32 {
    let t = day_time.rem_euclid(24000) as f32 / 24000.0;
    // Height of the sun: 1 at noon (t = 0.25), -1 at midnight (t = 0.75).
    let sun = (std::f32::consts::TAU * (t - 0.25)).cos();
    let day = ((sun + 0.2) / 0.5).clamp(0.0, 1.0);
    let day = day * day * (3.0 - 2.0 * day);
    0.35 + 0.65 * day
}

/// One map pixel (ARGB) for column `c`, with the heights of the columns `step` blocks west,
/// east, north and south of it (the column's own height where a neighbour is unknown).
pub fn pixel(
    c: Column,
    west: i16,
    east: i16,
    north: i16,
    south: i16,
    step: f32,
    light: Light,
) -> u32 {
    if !c.known() {
        return UNKNOWN;
    }
    let mut rgb = channels(c.rgb());
    let depth = c.water_depth();
    let mut bright = 1.0f32;
    if depth > 0 {
        // Shallow water shows lighter, deep water sinks to about half.
        let d = (depth as f32).min(30.0) / 30.0;
        bright *= 1.12 - 0.55 * d.sqrt();
    } else if light.relief {
        // Light from the north-west, a little above the horizon.
        let dx = (f32::from(east) - f32::from(west)) / (2.0 * step);
        let dz = (f32::from(south) - f32::from(north)) / (2.0 * step);
        let n = [-dx, -dz, 1.0];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        const L: [f32; 3] = [-0.55, -0.55, 0.63];
        let lambert = (n[0] * L[0] + n[1] * L[1] + n[2] * L[2]) / len;
        bright *= (1.0 + (lambert - L[2]) * 1.1).clamp(0.62, 1.32);
        // High ground a touch lighter, low ground a touch darker.
        bright *= 1.0 + ((f32::from(c.height) - 64.0) / 700.0).clamp(-0.07, 0.09);
    }
    let sky = light.sky;
    // Night cools the colours as it darkens them.
    let night = 1.0 - sky;
    let cool = [1.0 - 0.18 * night, 1.0 - 0.08 * night, 1.0 + 0.06 * night];
    for (i, v) in rgb.iter_mut().enumerate() {
        *v = (*v * bright * sky * cool[i]).clamp(0.0, 255.0);
    }
    argb(255, rgb[0] as u32, rgb[1] as u32, rgb[2] as u32)
}

/// A cave-mode pixel: the floor found under the player, darker the further below them it is.
pub fn cave_pixel(rgb: u32, below: i32) -> u32 {
    let k = (1.0 - (below.max(0) as f32) * 0.035).clamp(0.4, 1.0);
    let c = channels(rgb);
    argb(255, (c[0] * k) as u32, (c[1] * k) as u32, (c[2] * k) as u32)
}

/// A cave-mode spot that is solid all the way through.
pub const CAVE_ROCK: u32 = 0xFF_1C1A1E;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_is_brightest_at_noon() {
        assert!((sky_light(6000) - 1.0).abs() < 1e-3);
        assert!((sky_light(18000) - 0.35).abs() < 1e-3);
        assert!(sky_light(0) > 0.35 && sky_light(0) < 1.0);
    }

    #[test]
    fn slopes_facing_the_light_are_brighter() {
        let c = Column::new(0x808080, 64, 0);
        let flat = pixel(c, 64, 64, 64, 64, 1.0, Light::FULL);
        let lit = pixel(c, 62, 66, 62, 66, 1.0, Light::FULL);
        let shaded = pixel(c, 66, 62, 66, 62, 1.0, Light::FULL);
        let g = |p: u32| (p >> 8) & 0xFF;
        assert!(g(lit) > g(flat) && g(flat) > g(shaded));
        assert_eq!(
            pixel(Column::default(), 0, 0, 0, 0, 1.0, Light::FULL),
            UNKNOWN
        );
    }

    #[test]
    fn deep_water_is_darker() {
        let shallow = pixel(Column::new(0x3050C0, 62, 1), 0, 0, 0, 0, 1.0, Light::FULL);
        let deep = pixel(Column::new(0x3050C0, 62, 25), 0, 0, 0, 0, 1.0, Light::FULL);
        assert!((deep & 0xFF) < (shallow & 0xFF));
    }
}
