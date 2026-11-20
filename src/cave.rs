//! When the minimap goes underground, and how it gets there.
//!
//! - Cover: a few columns round the player are looked up through for a roof. Glass of every
//!   kind counts as one (a dome, or a roof of glass between solid ribs), leaves don't (a tree is
//!   no cave), and a column that is a wall at head height is left out, so a ravine's sides or a
//!   tunnel's walls don't count as a roof.
//! - Hysteresis: cave mode starts only once most open columns have been covered for a few ticks
//!   running, and ends only once few are for a few more, so a one-block overhang, a dome's seam
//!   or a step under a ledge never flicks the map back and forth.
//! - Crossfade: the surface and cave pictures blend over about a third of a second, timed by
//!   real frame time and eased with a smoothstep, instead of cutting over in one frame. A cave
//!   column not yet read keeps the surface pixel, so the fade never flashes dark on a staircase.

use crate::colors::Cover;
use std::time::Duration;

/// The columns looked up through, round the player's: a 5 x 5 block area, every other block.
pub const SAMPLES: [(i32, i32); 9] = [
    (-2, -2),
    (0, -2),
    (2, -2),
    (-2, 0),
    (0, 0),
    (2, 0),
    (-2, 2),
    (0, 2),
    (2, 2),
];
/// How far over the player's head a roof is looked for.
pub const ROOF_REACH: i32 = 48;
/// Cave mode starts when at least this share of open columns is covered...
pub const ENTER_SHARE: f32 = 0.5;
/// ...for this many ticks running (200 ms).
pub const ENTER_TICKS: u32 = 4;
/// It ends when at most this share is covered...
pub const LEAVE_SHARE: f32 = 0.25;
/// ...for this many ticks running (350 ms).
pub const LEAVE_TICKS: u32 = 7;
/// How long the crossfade between the surface and the cave picture takes.
pub const FADE: Duration = Duration::from_millis(320);

/// Whether a column has a roof, from the blocks over the player's head, lowest first.
pub fn column_covered(above: impl IntoIterator<Item = Cover>) -> bool {
    above
        .into_iter()
        .any(|c| matches!(c, Cover::Roof | Cover::Glass))
}

/// The share of open columns with a roof: `columns` holds, for each column looked at, None for
/// a wall at head height (left out), else whether it is covered. None when no column is open.
pub fn covered_share(columns: impl IntoIterator<Item = Option<bool>>) -> Option<f32> {
    let (mut open, mut covered) = (0u32, 0u32);
    for c in columns.into_iter().flatten() {
        open += 1;
        covered += u32::from(c);
    }
    (open > 0).then(|| covered as f32 / open as f32)
}

/// Cave mode's on/off with hysteresis, stepped once a tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sense {
    pub active: bool,
    covered_for: u32,
    open_for: u32,
}

impl Sense {
    /// One tick's reading: the covered share (None: nothing to go by, the state holds).
    pub fn update(&mut self, share: Option<f32>) -> bool {
        match share {
            Some(s) if s >= ENTER_SHARE => {
                self.covered_for = self.covered_for.saturating_add(1);
                self.open_for = 0;
            }
            Some(s) if s <= LEAVE_SHARE => {
                self.open_for = self.open_for.saturating_add(1);
                self.covered_for = 0;
            }
            // In between, or no reading: hold, and start both counts again.
            _ => {
                self.covered_for = 0;
                self.open_for = 0;
            }
        }
        if !self.active && self.covered_for >= ENTER_TICKS {
            self.active = true;
        } else if self.active && self.open_for >= LEAVE_TICKS {
            self.active = false;
        }
        self.active
    }

    /// Straight to `active` (the Nether, cave mode turned off, a new world).
    pub fn force(&mut self, active: bool) {
        *self = Self {
            active,
            ..Self::default()
        };
    }
}

/// The crossfade from the surface (0) to the cave picture (1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fade {
    t: f32,
}

impl Fade {
    /// Moves toward `cave` by `dt` of real time (a long stall moves at most a tenth of a
    /// second, so the fade is never skipped) and answers the eased mix.
    pub fn advance(&mut self, cave: bool, dt: Duration) -> f32 {
        let step = dt.min(Duration::from_millis(100)).as_secs_f32() / FADE.as_secs_f32();
        let target = if cave { 1.0 } else { 0.0 };
        self.t = if self.t < target {
            (self.t + step).min(target)
        } else {
            (self.t - step).max(target)
        };
        self.mix()
    }
    /// The eased mix now (smoothstep).
    pub fn mix(&self) -> f32 {
        let t = self.t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
    /// Straight to one picture.
    pub fn set(&mut self, cave: bool) {
        self.t = if cave { 1.0 } else { 0.0 };
    }
    pub fn moving(&self) -> bool {
        self.t > 0.0 && self.t < 1.0
    }
}

/// Mixes two ARGB pixels, `t` of the way from `a` to `b`.
pub fn lerp_argb(a: u32, b: u32, t: f32) -> u32 {
    if t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    let ch = |s: u32| {
        let (x, y) = (((a >> s) & 0xFF) as f32, ((b >> s) & 0xFF) as f32);
        ((x + (y - x) * t).round() as u32).min(255) << s
    };
    ch(24) | ch(16) | ch(8) | ch(0)
}

/// The map sample while the surface and cave pictures are mixed by `mix`.
/// `cave` is None where that column has not been read yet: the surface pixel is kept, so
/// the fade never flashes the map's empty background going down a staircase.
pub fn mix_sample(surface: u32, cave: Option<u32>, mix: f32) -> u32 {
    if mix <= 0.0 {
        return surface;
    }
    match cave {
        Some(c) if mix >= 1.0 => c,
        Some(c) => lerp_argb(surface, c, mix),
        None => surface,
    }
}

/// One block of a cave column, as the floor search sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    /// Air and what the map looks through.
    Open,
    /// Glass: looked through over the player's feet (a roof or a wall), a floor under them.
    Glass(u32),
    /// Water, lava and every other block, with its colour.
    Solid(u32),
}

/// What a cave column shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Floor {
    /// The floor found at height `y`, of colour `rgb`.
    At { rgb: u32, y: i32 },
    /// Open space but no floor in reach.
    Void,
    /// Solid all the way.
    Rock,
}

/// The feet height a cave layer is read for: the player's own while they stand in that layer
/// (so the map shows the very room they are in, under whatever roof is over them: a dome's
/// glass or blackstone is cut away above their head, as the minimap does), else the layer's
/// own slice, cut two blocks over the layer's top so every floor in its 16 blocks shows.
pub fn slice_feet(layer: i32, player_feet: Option<i32>) -> i32 {
    match player_feet {
        Some(f) if crate::world::layer_of(f) == layer => f,
        _ => crate::world::layer_range(layer).1 - 2,
    }
}

/// Glass's colour as a floor where it has none of its own (plain glass has no map colour).
pub const GLASS_RGB: u32 = 0xB8D4DA;

/// The floor in one column for a player whose feet are at `feet`: `cells` from `top` down,
/// `cells[0]` the block just over the column's top (only whether it is open matters). The floor
/// is the first block with open space over it. Glass at or over the feet is see-through, so a
/// glass roof or dome never hides the room under it; glass under the feet is a floor.
pub fn pick_floor(cells: &[Cell], top: i32, feet: i32) -> Floor {
    let see_through = |c: Cell, y: i32| match c {
        Cell::Open => true,
        Cell::Glass(_) => y >= feet,
        Cell::Solid(_) => false,
    };
    let Some((first, rest)) = cells.split_first() else {
        return Floor::Rock;
    };
    let mut open_above = see_through(*first, top + 1);
    let mut any_open = false;
    for (i, c) in rest.iter().enumerate() {
        let y = top - i as i32;
        let clear = see_through(*c, y);
        if !clear && open_above {
            let rgb = match *c {
                Cell::Solid(rgb) => rgb,
                Cell::Glass(rgb) if rgb & 0xFF_FFFF != 0 => rgb,
                _ => GLASS_RGB,
            };
            return Floor::At { rgb, y };
        }
        any_open |= clear;
        open_above = clear;
    }
    if any_open {
        Floor::Void
    } else {
        Floor::Rock
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Cover::*;

    #[test]
    fn glass_roofs_count_as_cover_and_leaves_do_not() {
        // Under a glass dome: air, then the glass.
        assert!(column_covered([Open, Open, Open, Glass]));
        // Stained and tinted glass are Glass too (see colors::cover_of).
        assert!(column_covered([Glass]));
        // Rock overhead.
        assert!(column_covered([Roof]));
        // A tree's canopy over open sky is no roof.
        assert!(!column_covered([Open, Foliage, Foliage, Open]));
        assert!(!column_covered([]));
    }

    #[test]
    fn a_dome_of_glass_and_solid_ribs_is_fully_covered() {
        // Every open column under the dome is covered, whether by a rib or a pane; the dome's
        // own wall at head height is left out.
        let cols = [
            Some(true),
            Some(true),
            Some(true),
            None,
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            None,
        ];
        assert_eq!(covered_share(cols), Some(1.0));
        // A one-block overhang over open ground: one of nine.
        let cols = [
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(true),
            Some(false),
            Some(false),
            Some(false),
            Some(false),
        ];
        assert!(covered_share(cols).unwrap() < LEAVE_SHARE);
        // A ravine: its sides are walls, the open middle is under sky.
        assert_eq!(
            covered_share([
                None,
                Some(false),
                None,
                None,
                Some(false),
                None,
                None,
                Some(false),
                None
            ]),
            Some(0.0)
        );
        assert_eq!(covered_share([None, None]), None);
    }

    #[test]
    fn hysteresis_needs_a_steady_reading_each_way() {
        let mut s = Sense::default();
        // A single covered tick (a dome seam, an overhang) changes nothing.
        assert!(!s.update(Some(1.0)));
        assert!(!s.update(Some(0.0)));
        assert!(!s.update(Some(1.0)));
        assert!(!s.update(Some(1.0)));
        assert!(!s.update(Some(1.0)));
        // The fourth covered tick running: cave mode.
        assert!(s.update(Some(1.0)));
        // A flicker of open sky (or an in-between reading) doesn't end it.
        for _ in 0..LEAVE_TICKS - 1 {
            assert!(s.update(Some(0.0)));
        }
        assert!(s.update(Some(0.4)));
        assert!(s.update(None));
        for _ in 0..LEAVE_TICKS - 1 {
            assert!(s.update(Some(0.1)));
        }
        // Steadily open: back to the surface.
        assert!(!s.update(Some(0.1)));
        s.force(true);
        assert!(s.active);
    }

    #[test]
    fn crossfade_moves_with_frame_time() {
        let mut f = Fade::default();
        assert_eq!(f.mix(), 0.0);
        // 60 fps: about 13 frames to get there, each a little further.
        let frame = Duration::from_micros(16_667);
        let mut last = 0.0;
        let mut frames = 0;
        while f.mix() < 1.0 {
            let m = f.advance(true, frame);
            assert!(m > last && m <= 1.0);
            last = m;
            frames += 1;
            assert!(frames < 100);
        }
        assert!((17..=24).contains(&frames), "{frames}");
        // Eased: a slow start.
        let mut g = Fade::default();
        assert!(g.advance(true, Duration::from_millis(22)) < 0.1);
        assert!(g.moving());
        // A long stall moves at most 100 ms of the way.
        let mut h = Fade::default();
        let m = h.advance(true, Duration::from_secs(3));
        assert!(m > 0.0 && m < 1.0);
        // And back, half the fade (two steps: a long stall is capped at 100 ms).
        // smoothstep(0.5) is 0.5.
        f.advance(false, Duration::from_millis(80));
        f.advance(false, Duration::from_millis(80));
        assert!((f.mix() - 0.5).abs() < 0.01);
        f.set(false);
        assert_eq!(f.mix(), 0.0);
        assert_eq!(lerp_argb(0xFF_000000, 0xFF_FFFFFF, 0.5), 0xFF_808080);
        assert_eq!(lerp_argb(0x00_102030, 0xFF_102030, 1.0), 0xFF_102030);
    }

    #[test]
    fn a_layer_is_read_from_the_players_feet_when_they_are_in_it() {
        // Standing at Y 70 under a dome: layer 4 (64 to 79) is read from those feet.
        assert_eq!(slice_feet(4, Some(70)), 70);
        // Any other layer has its own slice: its top (cut at feet + 2) is the layer's top.
        assert_eq!(slice_feet(3, Some(70)), 61);
        assert_eq!(slice_feet(3, Some(70)) + 2, crate::world::layer_range(3).1);
        assert_eq!(slice_feet(-3, None), -35);
        assert_eq!(slice_feet(0, Some(-1)), 13);
    }

    #[test]
    fn a_solid_dome_roof_inside_the_slice_is_cut_through_to_the_floor() {
        // Read from the feet of a player standing inside a blackstone dome (feet 70): the roof
        // at 72 is right over the head, so the column starts in it and goes through.
        let (feet, top) = (70, 72);
        let roof = 0x2A2530;
        let floor = 0x7FB238;
        let cells = [
            Cell::Solid(roof),
            Cell::Solid(roof),
            Cell::Open,
            Cell::Open,
            Cell::Solid(floor),
        ];
        assert_eq!(
            pick_floor(&cells, top, feet),
            Floor::At { rgb: floor, y: 69 }
        );
    }

    #[test]
    fn the_floor_under_a_glass_dome_shows() {
        let (feet, top) = (64, 66);
        let stone = 0x7F7F7F;
        let grass = 0x5C9E3A;
        let stained = 0xC04040;
        // From top + 1 (67) down: a stained glass roof right over the head (66), air (65, 64),
        // the grass floor (63) and dirt (62).
        let cells = [
            Cell::Open,
            Cell::Glass(stained),
            Cell::Open,
            Cell::Open,
            Cell::Solid(grass),
            Cell::Solid(stone),
        ];
        assert_eq!(
            pick_floor(&cells, top, feet),
            Floor::At { rgb: grass, y: 63 }
        );
        // Plain glass (no colour) the same.
        let cells = [
            Cell::Glass(0),
            Cell::Glass(0),
            Cell::Open,
            Cell::Open,
            Cell::Solid(grass),
        ];
        assert_eq!(
            pick_floor(&cells, top, feet),
            Floor::At { rgb: grass, y: 63 }
        );
        // Under a rock roof the floor is found the same way.
        let cells = [
            Cell::Solid(stone),
            Cell::Open,
            Cell::Open,
            Cell::Open,
            Cell::Solid(stone),
        ];
        assert_eq!(
            pick_floor(&cells, top, feet),
            Floor::At { rgb: stone, y: 63 }
        );
        // A glass floor under the feet is the floor, in glass's colour.
        let cells = [
            Cell::Open,
            Cell::Open,
            Cell::Open,
            Cell::Open,
            Cell::Glass(0),
            Cell::Solid(stone),
        ];
        assert_eq!(
            pick_floor(&cells, top, feet),
            Floor::At {
                rgb: GLASS_RGB,
                y: 63
            }
        );
        // Solid through and through, or open with nothing under.
        assert_eq!(pick_floor(&[Cell::Solid(stone); 6], top, feet), Floor::Rock);
        assert_eq!(pick_floor(&[Cell::Open; 6], top, feet), Floor::Void);
    }

    #[test]
    fn a_staircase_does_not_flick_cave_mode_on_and_off() {
        let mut s = Sense::default();
        for _ in 0..ENTER_TICKS {
            s.update(Some(0.8));
        }
        assert!(s.active);
        // Walking down a long stair: cover hovers near the thresholds as sky opens behind
        // and walls close in, never a long steady open or covered run.
        for share in [
            0.55, 0.4, 0.3, 0.6, 0.35, 0.45, 0.7, 0.2, 0.55, 0.3, 0.4, 0.65,
        ] {
            assert!(s.update(Some(share)), "{share}");
        }
        // A few open ticks are not enough to leave (glancing back up the stairs).
        for _ in 0..LEAVE_TICKS - 1 {
            assert!(s.update(Some(0.1)));
        }
        assert!(s.active);
        assert!(!s.update(Some(0.1)));
    }

    #[test]
    fn an_unscanned_cave_column_keeps_the_surface_pixel_during_the_fade() {
        let surface = 0xFF_55AA33;
        let cave = 0xFF_404040;
        assert_eq!(mix_sample(surface, None, 0.0), surface);
        assert_eq!(mix_sample(surface, None, 0.4), surface);
        assert_eq!(mix_sample(surface, None, 1.0), surface);
        assert_eq!(mix_sample(surface, Some(cave), 0.0), surface);
        assert_eq!(mix_sample(surface, Some(cave), 1.0), cave);
        let mid = mix_sample(surface, Some(cave), 0.5);
        assert_ne!(mid, surface);
        assert_ne!(mid, cave);
        assert_eq!(mid, lerp_argb(surface, cave, 0.5));
    }
}
