//! Reading the world the client holds into the map: a few chunks each tick, nearest first,
//! read again when they change or after a while; and, underground, a live picture of the
//! floor around the player (cave mode), kept while it is near and also saved as the world
//! map's cave layer.
//!
//! Both readers stop early when a tick's share of time is spent, so a burst of new chunks or
//! the first moments in a cave never stall a frame.

use crate::cave::{self, Cell, Floor};
use crate::colors::{BlockColors, Cover, Look, Role};
use crate::shade::{self, CAVE_ROCK};
use crate::store::{Column, Disk, MapStore};
use mod_api::{BlockState, ClientHost, Heightmap, LevelRef};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// How deep water is followed down (the map shades no deeper).
const MAX_DEPTH: u32 = 24;
/// Water's tint where the game gives none (the usual biome water colour).
const WATER_TINT: u32 = 0x3F76E4;
/// The most time a tick spends reading surface chunks.
const SURFACE_TIME: Duration = Duration::from_micros(3_000);
/// The most time a tick spends reading cave columns once cave mode is on.
pub const CAVE_TIME: Duration = Duration::from_micros(2_500);
/// A lighter budget used to pre-warm the cave picture under any overhead cover, before
/// cave mode has started, so the fade has something to blend into.
pub const CAVE_WARM_TIME: Duration = Duration::from_micros(1_500);
/// The cave picture is swept again from the player outward when the feet have moved this
/// far since the last such sweep. Existing pixels stay, so a staircase never flashes blank.
pub const CAVE_RESWEEP_DY: i32 = 3;
/// A cave pixel where there is open space but no floor in reach.
pub const CAVE_VOID: u32 = 0xFF_08080A;

/// How a dimension's columns are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// From the highest block down (the Overworld, the End).
    Top,
    /// From below the roof at this height (the Nether): through the roof, then down to the
    /// first floor under it.
    UnderRoof(i32),
}

fn colour_of(host: &dyn ClientHost, look: Look, state: BlockState, pos: [i32; 3]) -> u32 {
    if look.tint < 0 {
        return look.rgb;
    }
    match host.block_tint_color(state, look.tint, pos) {
        Some(t) => shade::tint(look.rgb, t),
        None if look.role == Role::Water => shade::tint(look.rgb, WATER_TINT),
        None => look.rgb,
    }
}

/// The column at `x`, `z` from height `y` down: the first thing drawn (water with its depth, or
/// a block), looking through what the game's maps leave out.
pub fn read_column(
    host: &dyn ClientHost,
    level: LevelRef,
    colors: &mut BlockColors,
    x: i32,
    z: i32,
    start: i32,
    min_y: i32,
    surface: Surface,
) -> Option<Column> {
    let mut y = start;
    if let Surface::UnderRoof(_) = surface {
        // Through the roof: past solid blocks to the first open space.
        let mut through = false;
        while y > min_y {
            let state = level.block_state(host, [x, y, z])?;
            if colors.look(host, state).role != Role::Solid {
                through = true;
                break;
            }
            y -= 1;
        }
        if !through {
            return None;
        }
    }
    let mut water: Option<(u32, i32)> = None;
    let mut depth = 0u32;
    let mut steps = 0;
    while y >= min_y && steps < 320 {
        steps += 1;
        let state = level.block_state(host, [x, y, z])?;
        let look = colors.look(host, state);
        match look.role {
            Role::Clear => {}
            Role::Water => {
                if water.is_none() {
                    water = Some((colour_of(host, look, state, [x, y, z]), y));
                }
                depth += 1;
                if depth >= MAX_DEPTH {
                    break;
                }
            }
            Role::Solid => {
                return Some(match water {
                    Some((rgb, top)) => Column::new(rgb, top, depth),
                    None => Column::new(colour_of(host, look, state, [x, y, z]), y, 0),
                });
            }
        }
        y -= 1;
    }
    water.map(|(rgb, top)| Column::new(rgb, top, depth.max(1)))
}

/// What the cave-layer reader knows of one chunk in one layer: the feet height it was read
/// for, the chunk's change count then, and when.
#[derive(Clone, Copy, Debug)]
struct LayerRead {
    feet: i32,
    generation: u32,
    at: Instant,
}

/// A cave layer's chunk read this long ago is read again (what changed is read sooner).
const LAYER_STALE: Duration = Duration::from_secs(90);
/// A chunk the client didn't hold is asked about again after this.
const LAYER_UNHELD: Duration = Duration::from_millis(1_500);
/// A layer read for feet this far from the feet now is read again.
const LAYER_FEET_SLACK: i32 = 2;

#[derive(Default)]
pub struct Scanner {
    scanned: HashMap<(i32, i32), Instant>,
    /// Bumped each time a chunk changes, so every cave layer knows to read it again.
    generation: HashMap<(i32, i32), u32>,
    /// `(layer, chunk x, chunk z)`: what each cave layer has read.
    layer_read: HashMap<(i32, i32, i32), LayerRead>,
    /// Chunks the client didn't hold when last asked.
    unheld: HashMap<(i32, i32), Instant>,
    dirty: HashSet<(i32, i32)>,
    cave: HashMap<(i32, i32), u32>,
    cave_cursor: usize,
    cave_order: Vec<(i32, i32)>,
    /// The feet height the cave picture was read for.
    cave_feet: Option<i32>,
}

/// Cave mode reads this far round the player, in blocks.
pub const CAVE_RADIUS: i32 = 96;

impl Scanner {
    pub fn reset(&mut self) {
        self.scanned.clear();
        self.dirty.clear();
        self.generation.clear();
        self.layer_read.clear();
        self.unheld.clear();
        self.clear_cave();
    }

    /// A block changed in chunk `(cx, cz)`, or the chunk arrived: read it again soon (the
    /// surface and every cave layer).
    pub fn mark(&mut self, chunk: (i32, i32)) {
        self.dirty.insert(chunk);
        *self.generation.entry(chunk).or_default() += 1;
        self.unheld.remove(&chunk);
    }

    /// Whether cave layer `layer`'s chunk `c` wants reading for feet at `feet`.
    fn layer_wants(&self, layer: i32, c: (i32, i32), feet: i32, now: Instant) -> bool {
        match self.layer_read.get(&(layer, c.0, c.1)) {
            None => true,
            Some(r) => {
                r.generation != self.generation.get(&c).copied().unwrap_or(0)
                    || (r.feet - feet).abs() > LAYER_FEET_SLACK
                    || now.duration_since(r.at) > LAYER_STALE
            }
        }
    }

    /// How many chunks cave layer `layer` has read (for tests and the map's progress).
    pub fn layer_chunks(&self, layer: i32) -> usize {
        self.layer_read.keys().filter(|k| k.0 == layer).count()
    }

    /// Reads cave layer `layer` across every chunk the client holds within `radius` chunks of
    /// `centre` (nearest first), as seen from feet at `feet` (see [`crate::cave::slice_feet`]),
    /// into `store`, until `time` is spent (always at least one chunk). Answers whether more
    /// is left to read.
    #[allow(clippy::too_many_arguments)]
    pub fn layer_tick(
        &mut self,
        host: &dyn ClientHost,
        level: LevelRef,
        colors: &mut BlockColors,
        store: &mut MapStore,
        disk: &dyn Disk,
        layer: i32,
        feet: i32,
        centre: (i32, i32),
        radius: i32,
        time: Duration,
    ) -> bool {
        let started = Instant::now();
        let mut read = 0usize;
        loop {
            let now = Instant::now();
            let mut wanted: Vec<(i32, i32)> = Vec::new();
            'rings: for ring in 0..=radius {
                for (dx, dz) in ring_cells(ring) {
                    let c = (centre.0 + dx, centre.1 + dz);
                    if self
                        .unheld
                        .get(&c)
                        .is_some_and(|t| now.duration_since(*t) < LAYER_UNHELD)
                    {
                        continue;
                    }
                    if self.layer_wants(layer, c, feet, now) {
                        wanted.push(c);
                        if wanted.len() >= 32 {
                            break 'rings;
                        }
                    }
                }
            }
            if wanted.is_empty() {
                return false;
            }
            let probes: Vec<(i32, i32)> = wanted
                .iter()
                .map(|c| (c.0 * 16 + 8, c.1 * 16 + 8))
                .collect();
            let Ok(held) = host.client_surface_heights(&probes, Heightmap::WorldSurface) else {
                return false;
            };
            for (c, h) in wanted.iter().zip(held) {
                if h.is_none() {
                    self.unheld.insert(*c, now);
                    continue;
                }
                if read > 0 && started.elapsed() > time {
                    return true;
                }
                for i in 0..256 {
                    let (x, z) = (c.0 * 16 + (i & 15), c.1 * 16 + (i >> 4));
                    if let Some((pixel, y)) = cave_column(host, level, colors, x, z, feet) {
                        store.set(disk, x, z, Column::new(pixel, y, 0));
                    }
                }
                let generation = self.generation.get(c).copied().unwrap_or(0);
                self.layer_read.insert(
                    (layer, c.0, c.1),
                    LayerRead {
                        feet,
                        generation,
                        at: Instant::now(),
                    },
                );
                read += 1;
            }
            // Keep the tables small.
            if self.layer_read.len() > 40_000 {
                let r = radius * 2;
                self.layer_read
                    .retain(|k, _| (k.1 - centre.0).abs() <= r && (k.2 - centre.1).abs() <= r);
            }
            if self.unheld.len() > 20_000 {
                self.unheld.clear();
            }
            // Time spent: whatever is left waits for the next call.
            if started.elapsed() > time {
                return true;
            }
        }
    }

    /// Reads up to `budget` chunks within `radius` chunks of `centre` that are new, changed or
    /// stale.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        host: &dyn ClientHost,
        level: LevelRef,
        colors: &mut BlockColors,
        store: &mut MapStore,
        disk: &dyn Disk,
        centre: (i32, i32),
        radius: i32,
        min_y: i32,
        surface: Surface,
        budget: usize,
    ) {
        let now = Instant::now();
        // Candidates, nearest first: never read, changed, or read long enough ago.
        let mut wanted: Vec<(i32, i32)> = Vec::new();
        'rings: for ring in 0..=radius {
            for (dx, dz) in ring_cells(ring) {
                let c = (centre.0 + dx, centre.1 + dz);
                let stale = match self.scanned.get(&c) {
                    None => true,
                    Some(t) => {
                        let age = now.duration_since(*t);
                        self.dirty.contains(&c)
                            || (ring <= 2 && age > Duration::from_secs(15))
                            || age > Duration::from_secs(120)
                    }
                };
                if stale {
                    wanted.push(c);
                    if wanted.len() >= 96 {
                        break 'rings;
                    }
                }
            }
        }
        if wanted.is_empty() {
            return;
        }
        // Which of them the client holds: one column each.
        let probes: Vec<(i32, i32)> = wanted
            .iter()
            .map(|c| (c.0 * 16 + 8, c.1 * 16 + 8))
            .collect();
        let Ok(held) = host.client_surface_heights(&probes, Heightmap::WorldSurface) else {
            return;
        };
        let chosen: Vec<(i32, i32)> = wanted
            .iter()
            .zip(held)
            .filter(|(_, h)| h.is_some())
            .map(|(c, _)| *c)
            .take(budget)
            .collect();
        if chosen.is_empty() {
            return;
        }
        let mut columns = Vec::with_capacity(chosen.len() * 256);
        for c in &chosen {
            for i in 0..256 {
                columns.push((c.0 * 16 + (i & 15), c.1 * 16 + (i >> 4)));
            }
        }
        let Ok(tops) = host.client_surface_heights(&columns, Heightmap::WorldSurface) else {
            return;
        };
        // A chunk at a time, nearest first, until this tick's time is spent (always one).
        for (n, c) in chosen.iter().enumerate() {
            if n > 0 && now.elapsed() > SURFACE_TIME {
                break;
            }
            for i in n * 256..(n + 1) * 256 {
                let ((x, z), Some(top)) = (columns[i], tops[i]) else {
                    continue;
                };
                let start = match surface {
                    Surface::Top => top - 1,
                    Surface::UnderRoof(roof) => (top - 1).min(roof),
                };
                if let Some(col) = read_column(host, level, colors, x, z, start, min_y, surface) {
                    store.set(disk, x, z, col);
                }
            }
            self.scanned.insert(*c, now);
            self.dirty.remove(c);
        }
        // Forget timestamps far away so the table stays small.
        if self.scanned.len() > 20_000 {
            self.scanned.retain(|c, _| {
                (c.0 - centre.0).abs() <= radius * 2 && (c.1 - centre.1).abs() <= radius * 2
            });
        }
    }

    /// If the feet have moved by [`CAVE_RESWEEP_DY`] or more since the last sweep started, the
    /// next read begins at the player again. Pixels already read stay on the map: a staircase
    /// must not flash blank. Answers whether a re-sweep from the centre was started.
    pub fn cave_follow_feet(&mut self, feet: i32) -> bool {
        let resweep = match self.cave_feet {
            Some(f) if (f - feet).abs() >= CAVE_RESWEEP_DY => {
                self.cave_cursor = 0;
                self.cave_feet = Some(feet);
                true
            }
            None => {
                self.cave_feet = Some(feet);
                false
            }
            Some(_) => false,
        };
        if self.cave_order.is_empty() {
            let r = CAVE_RADIUS;
            let mut v: Vec<(i32, i32)> = (-r..=r)
                .flat_map(|z| (-r..=r).map(move |x| (x, z)))
                .filter(|(x, z)| x * x + z * z <= r * r)
                .collect();
            v.sort_by_key(|(x, z)| x * x + z * z);
            self.cave_order = v;
        }
        resweep
    }

    /// The feet height the current cave sweep was started from.
    pub fn cave_feet(&self) -> Option<i32> {
        self.cave_feet
    }

    /// How far through the nearest-first sweep the next read is.
    pub fn cave_cursor(&self) -> usize {
        self.cave_cursor
    }

    /// Reads up to `budget` more cave columns round `(px, pz)`, nearest first, for a player
    /// whose feet are at `feet`, within `time`; each is kept for the minimap and, given
    /// `layer`, saved to the world map's cave layer.
    #[allow(clippy::too_many_arguments)]
    pub fn cave_tick(
        &mut self,
        host: &dyn ClientHost,
        level: LevelRef,
        colors: &mut BlockColors,
        px: i32,
        pz: i32,
        feet: i32,
        budget: usize,
        time: Duration,
        mut layer: Option<(&mut MapStore, &dyn Disk)>,
    ) {
        self.cave_follow_feet(feet);
        let started = Instant::now();
        for n in 0..budget {
            if n % 64 == 63 && started.elapsed() > time {
                break;
            }
            let (dx, dz) = self.cave_order[self.cave_cursor % self.cave_order.len()];
            self.cave_cursor = (self.cave_cursor + 1) % self.cave_order.len();
            let (x, z) = (px + dx, pz + dz);
            if let Some((pixel, y)) = cave_column(host, level, colors, x, z, feet) {
                self.cave.insert((x, z), pixel);
                if let Some((store, disk)) = layer.as_mut() {
                    store.set(*disk, x, z, Column::new(pixel, y, 0));
                }
            }
        }
        if self.cave.len() > 60_000 {
            let r = CAVE_RADIUS + 16;
            self.cave
                .retain(|(x, z), _| (x - px).abs() <= r && (z - pz).abs() <= r);
        }
    }

    /// Cave mode's pixel for a column, None where that column has not been read yet (as
    /// against a scanned rock or void pixel).
    pub fn cave_pixel(&self, x: i32, z: i32) -> Option<u32> {
        self.cave.get(&(x, z)).copied()
    }

    /// How many cave columns are held (for tests and the cache's bound).
    pub fn cave_len(&self) -> usize {
        self.cave.len()
    }

    pub fn clear_cave(&mut self) {
        self.cave.clear();
        self.cave_cursor = 0;
        self.cave_feet = None;
    }
}

/// The scanned floor height of a cave (or surface) column, None if it has not been read,
/// or if it is solid rock or open void (those store a sentinel height, not a floor).
pub fn cave_floor(col: Column) -> Option<i32> {
    if !col.known() {
        return None;
    }
    let rgb = col.rgb();
    if rgb == (CAVE_ROCK & 0xFF_FFFF) || rgb == (CAVE_VOID & 0xFF_FFFF) {
        return None;
    }
    Some(i32::from(col.height))
}

/// How a block counts in the cave floor search.
fn cell_of(host: &dyn ClientHost, look: Look, state: BlockState, pos: [i32; 3]) -> Cell {
    match (look.cover, look.role) {
        (Cover::Glass, Role::Clear) => Cell::Glass(0),
        (Cover::Glass, _) => Cell::Glass(colour_of(host, look, state, pos)),
        (_, Role::Clear) => Cell::Open,
        _ => Cell::Solid(colour_of(host, look, state, pos)),
    }
}

/// The floor under the player in one column, from just over their head to fourteen blocks
/// under their feet: its cave pixel and height (see [`cave::pick_floor`]).
fn cave_column(
    host: &dyn ClientHost,
    level: LevelRef,
    colors: &mut BlockColors,
    x: i32,
    z: i32,
    feet: i32,
) -> Option<(u32, i32)> {
    let top = feet + 2;
    let mut cells = [Cell::Open; 18];
    for (i, y) in (feet - 14..=top + 1).rev().enumerate() {
        let state = level.block_state(host, [x, y, z])?;
        let look = colors.look(host, state);
        cells[i] = cell_of(host, look, state, [x, y, z]);
    }
    Some(match cave::pick_floor(&cells, top, feet) {
        Floor::At { rgb, y } => (shade::cave_pixel(rgb, feet - 1 - y), y),
        Floor::Void => (CAVE_VOID, feet - 14),
        Floor::Rock => (CAVE_ROCK, feet),
    })
}

/// The share of open columns round the player with a roof over them (glass counts, leaves
/// don't; a column that is a wall at head height is left out): see [`cave`].
pub fn cover_share(
    host: &dyn ClientHost,
    level: LevelRef,
    colors: &mut BlockColors,
    x: i32,
    z: i32,
    feet: i32,
) -> Option<f32> {
    let cols: Vec<(i32, i32)> = cave::SAMPLES
        .iter()
        .map(|(dx, dz)| (x + dx, z + dz))
        .collect();
    let tops = host
        .client_surface_heights(&cols, Heightmap::WorldSurface)
        .ok()?;
    let head = feet + 1;
    let reading = cols.iter().zip(tops).map(|(&(cx, cz), top)| {
        let top = top?;
        let at_head = level.block_state(host, [cx, head, cz])?;
        if colors.look(host, at_head).cover == Cover::Roof {
            return None;
        }
        // Nothing at all over the head: open sky.
        if top <= head + 1 {
            return Some(false);
        }
        let reach = (top - 1).min(head + cave::ROOF_REACH);
        let mut covers = Vec::new();
        for y in head + 1..=reach {
            let c = level
                .block_state(host, [cx, y, cz])
                .map_or(Cover::Open, |s| colors.look(host, s).cover);
            covers.push(c);
            if matches!(c, Cover::Roof | Cover::Glass) {
                break;
            }
        }
        Some(cave::column_covered(covers))
    });
    cave::covered_share(reading.collect::<Vec<_>>())
}

/// The cells of the square ring `ring` chunks out (just the centre for 0).
fn ring_cells(ring: i32) -> Vec<(i32, i32)> {
    if ring == 0 {
        return vec![(0, 0)];
    }
    let mut v = Vec::with_capacity((ring * 8) as usize);
    for i in -ring..ring {
        v.push((i, -ring));
        v.push((ring, i));
        v.push((-i, ring));
        v.push((-ring, -i));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_cave_layer_keeps_its_own_record_of_what_it_read() {
        let mut s = Scanner::default();
        let now = Instant::now();
        let c = (3, -2);
        assert!(s.layer_wants(0, c, 10, now));
        s.layer_read.insert(
            (0, c.0, c.1),
            LayerRead {
                feet: 10,
                generation: 0,
                at: now,
            },
        );
        // Read for layer 0: not again for that layer (feet a step or two off is the same)...
        assert!(!s.layer_wants(0, c, 10, now));
        assert!(!s.layer_wants(0, c, 12, now));
        // ...but every other layer, and feet far off, still want it.
        assert!(s.layer_wants(-3, c, -40, now));
        assert!(s.layer_wants(0, c, 4, now));
        assert_eq!(s.layer_chunks(0), 1);
        assert_eq!(s.layer_chunks(-3), 0);
        // A block changed in the chunk: read again, in every layer.
        s.mark(c);
        assert!(s.layer_wants(0, c, 10, now));
        // Long ago: read again.
        s.layer_read.insert(
            (0, c.0, c.1),
            LayerRead {
                feet: 10,
                generation: 1,
                at: now,
            },
        );
        assert!(!s.layer_wants(0, c, 10, now));
        assert!(s.layer_wants(0, c, 10, now + LAYER_STALE + Duration::from_secs(1)));
        // A new world forgets it all.
        s.reset();
        assert_eq!(s.layer_chunks(0), 0);
    }

    #[test]
    fn rings_cover_each_cell_once() {
        let mut seen = HashSet::new();
        for r in 0..=4 {
            for c in ring_cells(r) {
                assert!(seen.insert(c));
                assert_eq!(c.0.abs().max(c.1.abs()), r);
            }
        }
        assert_eq!(seen.len(), 81);
    }

    #[test]
    fn walking_down_stairs_resweeps_without_clearing_the_picture() {
        let mut s = Scanner::default();
        // As if the columns around the player had already been read at Y 80.
        s.cave_follow_feet(80);
        s.cave.insert((10, 10), 0xFF_7FB238);
        s.cave.insert((11, 10), 0xFF_1C1A1E);
        s.cave_cursor = 400;
        assert_eq!(s.cave_len(), 2);
        assert_eq!(s.cave_pixel(10, 10), Some(0xFF_7FB238));
        assert_eq!(s.cave_pixel(0, 0), None);

        // One or two steps: the sweep keeps going, pixels stay, the start height is held
        // so the next steps can accumulate.
        assert!(!s.cave_follow_feet(79));
        assert!(!s.cave_follow_feet(78));
        assert_eq!(s.cave_feet(), Some(80));
        assert_eq!(s.cave_cursor(), 400);
        assert_eq!(s.cave_len(), 2);

        // The third block down: sweep again from the centre, but never wipe what is on screen.
        assert!(s.cave_follow_feet(77));
        assert_eq!(s.cave_feet(), Some(77));
        assert_eq!(s.cave_cursor(), 0);
        assert_eq!(s.cave_len(), 2);
        assert_eq!(s.cave_pixel(10, 10), Some(0xFF_7FB238));
        assert_eq!(s.cave_pixel(11, 10), Some(0xFF_1C1A1E));

        // A long drop is the same: still no wipe, just a fresh sweep.
        s.cave_cursor = 800;
        assert!(s.cave_follow_feet(40));
        assert_eq!(s.cave_cursor(), 0);
        assert_eq!(s.cave_len(), 2);

        // The order is nearest first, so the visible minimap updates before distant corners.
        assert!(!s.cave_order.is_empty());
        assert_eq!(s.cave_order[0], (0, 0));
        let dist = |c: (i32, i32)| c.0 * c.0 + c.1 * c.1;
        for w in s.cave_order.windows(2) {
            assert!(dist(w[0]) <= dist(w[1]), "{w:?}");
        }
    }

    #[test]
    fn a_cave_floor_column_is_told_apart_from_rock_and_void() {
        let floor = Column::new(0x7FB238, 69, 0);
        let rock = Column::new(CAVE_ROCK, 70, 0);
        let void = Column::new(CAVE_VOID, 56, 0);
        assert_eq!(cave_floor(floor), Some(69));
        assert_eq!(cave_floor(rock), None);
        assert_eq!(cave_floor(void), None);
        assert_eq!(cave_floor(Column::default()), None);
        // Known surface columns are real floors too (used by the vertical entity filter).
        assert_eq!(cave_floor(Column::new(0x55AA33, 63, 0)), Some(63));
    }
}
