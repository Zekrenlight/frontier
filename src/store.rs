//! What has been seen: the top block's colour and height for every explored column, kept in
//! 256 x 256 block regions and saved per server or world and per dimension.
//!
//! A column is one `u32` and one `i16`. The `u32` is 0 for a column never seen, else
//! `(1 + water depth) << 24 | 0xRRGGBB`: the colour of what is on top (with the biome's tint
//! already in it) and, over water, how deep the water is (capped at 200).

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

pub const REGION_SHIFT: i32 = 8;
pub const REGION: i32 = 1 << REGION_SHIFT;
const AREA: usize = (REGION * REGION) as usize;
const MAGIC: &[u8; 4] = b"FRM1";

/// One explored column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Column {
    pub packed: u32,
    pub height: i16,
}

impl Column {
    pub fn new(rgb: u32, height: i32, water_depth: u32) -> Self {
        let depth = water_depth.min(200);
        Self {
            packed: ((1 + depth) << 24) | (rgb & 0xFF_FFFF),
            height: height.clamp(i32::from(i16::MIN) + 1, i32::from(i16::MAX)) as i16,
        }
    }
    pub fn known(self) -> bool {
        self.packed != 0
    }
    pub fn rgb(self) -> u32 {
        self.packed & 0xFF_FFFF
    }
    pub fn water_depth(self) -> u32 {
        (self.packed >> 24).saturating_sub(1)
    }
}

pub struct Region {
    pub columns: Vec<u32>,
    pub heights: Vec<i16>,
    pub dirty: bool,
    pub used: Instant,
    /// Bumped on every change, so pictures made from it know to redraw.
    pub revision: u64,
}

impl Region {
    fn empty() -> Self {
        Self {
            columns: vec![0; AREA],
            heights: vec![0; AREA],
            dirty: false,
            used: Instant::now(),
            revision: 0,
        }
    }

    /// The file's bytes: runs of equal columns, `[count u16][packed u32][height i16]`, little
    /// endian, after a four-byte tag.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        let mut i = 0;
        while i < AREA {
            let (c, h) = (self.columns[i], self.heights[i]);
            let mut n = 1;
            while i + n < AREA && n < 0xFFFF && self.columns[i + n] == c && self.heights[i + n] == h
            {
                n += 1;
            }
            out.extend_from_slice(&(n as u16).to_le_bytes());
            out.extend_from_slice(&c.to_le_bytes());
            out.extend_from_slice(&h.to_le_bytes());
            i += n;
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 4 || &bytes[..4] != MAGIC {
            return None;
        }
        let mut r = Self::empty();
        let mut i = 0usize;
        for run in bytes[4..].chunks_exact(8) {
            let n = usize::from(u16::from_le_bytes([run[0], run[1]]));
            let c = u32::from_le_bytes([run[2], run[3], run[4], run[5]]);
            let h = i16::from_le_bytes([run[6], run[7]]);
            if n == 0 || i + n > AREA {
                return None;
            }
            r.columns[i..i + n].fill(c);
            r.heights[i..i + n].fill(h);
            i += n;
        }
        (i == AREA).then_some(r)
    }
}

/// Where region files are read and written: the game's bulk data folder in play, a map in tests.
pub trait Disk {
    fn read(&self, path: &str) -> Option<Vec<u8>>;
    fn write(&self, path: &str, bytes: &[u8]) -> bool;
}

/// Makes one path part of any text: letters, digits, `-`, `_` and `.` stay, everything else
/// becomes `~` and its hex code, so different names never share a folder.
pub fn path_part(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') || (c == '.' && !out.is_empty()) {
            out.push(c);
        } else {
            out.push_str(&format!("~{:x}", c as u32));
        }
    }
    if out.is_empty() {
        out.push('~');
    }
    if out.ends_with('.') {
        out.pop();
        out.push_str("~2e");
    }
    out
}

/// The map of one dimension of one server or world.
/// Reads columns, keeping the last region at hand (most neighbours share it).
pub struct Sampler<'a> {
    store: &'a MapStore,
    key: Option<(i32, i32)>,
    region: Option<&'a Region>,
}

impl<'a> Sampler<'a> {
    pub fn column(&mut self, x: i32, z: i32) -> Column {
        let key = (x >> REGION_SHIFT, z >> REGION_SHIFT);
        if self.key != Some(key) {
            self.key = Some(key);
            self.region = self.store.regions.get(&key);
        }
        match self.region {
            Some(r) => {
                let i = index(x, z);
                Column {
                    packed: r.columns[i],
                    height: r.heights[i],
                }
            }
            None => Column::default(),
        }
    }
}

pub struct MapStore {
    /// `maps/<world>/<dimension>`.
    pub folder: String,
    pub regions: HashMap<(i32, i32), Region>,
    missing: HashSet<(i32, i32)>,
    last_save: Instant,
}

impl MapStore {
    pub fn new(folder: String) -> Self {
        Self {
            folder,
            regions: HashMap::new(),
            missing: HashSet::new(),
            last_save: Instant::now(),
        }
    }

    fn file(&self, key: (i32, i32)) -> String {
        format!("{}/r.{}.{}.bin", self.folder, key.0, key.1)
    }

    /// Region `key`, read from disk the first time; None when it was never explored.
    pub fn region(&mut self, disk: &dyn Disk, key: (i32, i32)) -> Option<&mut Region> {
        if !self.regions.contains_key(&key) {
            if self.missing.contains(&key) {
                return None;
            }
            match disk.read(&self.file(key)).and_then(|b| Region::decode(&b)) {
                Some(r) => {
                    self.regions.insert(key, r);
                }
                None => {
                    self.missing.insert(key);
                    return None;
                }
            }
        }
        let r = self.regions.get_mut(&key)?;
        r.used = Instant::now();
        Some(r)
    }

    /// Only what is already in memory, without touching the disk.
    pub fn loaded(&self, key: (i32, i32)) -> Option<&Region> {
        self.regions.get(&key)
    }

    /// Whether region `key` is known to have no file.
    pub fn is_missing(&self, key: (i32, i32)) -> bool {
        self.missing.contains(&key)
    }

    /// The column at block `x`, `z`, if loaded.
    pub fn column(&self, x: i32, z: i32) -> Column {
        let key = (x >> REGION_SHIFT, z >> REGION_SHIFT);
        match self.regions.get(&key) {
            Some(r) => {
                let i = index(x, z);
                Column {
                    packed: r.columns[i],
                    height: r.heights[i],
                }
            }
            None => Column::default(),
        }
    }

    /// A reader that remembers the last region it looked in, for drawing many nearby columns.
    pub fn sampler(&self) -> Sampler<'_> {
        Sampler {
            store: self,
            key: None,
            region: None,
        }
    }

    /// Records a column, making its region (from disk first) when needed.
    pub fn set(&mut self, disk: &dyn Disk, x: i32, z: i32, col: Column) {
        let key = (x >> REGION_SHIFT, z >> REGION_SHIFT);
        if self.region(disk, key).is_none() {
            self.missing.remove(&key);
            self.regions.insert(key, Region::empty());
        }
        if let Some(r) = self.regions.get_mut(&key) {
            let i = index(x, z);
            if r.columns[i] != col.packed || r.heights[i] != col.height {
                r.columns[i] = col.packed;
                r.heights[i] = col.height;
                r.dirty = true;
                r.revision += 1;
            }
        }
    }

    /// Writes every changed region.
    pub fn save(&mut self, disk: &dyn Disk) {
        let keys: Vec<_> = self
            .regions
            .iter()
            .filter(|(_, r)| r.dirty)
            .map(|(k, _)| *k)
            .collect();
        for key in keys {
            let path = self.file(key);
            if let Some(r) = self.regions.get_mut(&key) {
                if disk.write(&path, &r.encode()) {
                    r.dirty = false;
                }
            }
        }
        self.last_save = Instant::now();
    }

    /// Saves now and then, and lets go of regions nobody looked at for a while.
    pub fn maintain(&mut self, disk: &dyn Disk) {
        if self.last_save.elapsed() < Duration::from_secs(45) {
            return;
        }
        self.save(disk);
        let idle = Duration::from_secs(120);
        self.regions
            .retain(|_, r| r.dirty || r.used.elapsed() < idle);
        // Regions written since are found again.
        self.missing.clear();
    }
}

fn index(x: i32, z: i32) -> usize {
    ((z & (REGION - 1)) * REGION + (x & (REGION - 1))) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Mem(RefCell<HashMap<String, Vec<u8>>>);
    impl Disk for Mem {
        fn read(&self, path: &str) -> Option<Vec<u8>> {
            self.0.borrow().get(path).cloned()
        }
        fn write(&self, path: &str, bytes: &[u8]) -> bool {
            self.0.borrow_mut().insert(path.into(), bytes.to_vec());
            true
        }
    }

    #[test]
    fn regions_round_trip_through_disk() {
        let disk = Mem::default();
        let mut s = MapStore::new("maps/w/o".into());
        s.set(&disk, -1, -1, Column::new(0x336699, 70, 3));
        s.set(&disk, 300, 5, Column::new(0x112233, -20, 0));
        s.save(&disk);
        assert_eq!(disk.0.borrow().len(), 2);
        let mut t = MapStore::new("maps/w/o".into());
        assert!(t.region(&disk, (-1, -1)).is_some());
        assert!(t.region(&disk, (1, 0)).is_some());
        let c = t.column(-1, -1);
        assert_eq!((c.rgb(), c.height, c.water_depth()), (0x336699, 70, 3));
        assert_eq!(t.column(300, 5).height, -20);
        assert!(!t.column(0, 0).known());
        assert!(t.region(&disk, (5, 5)).is_none());
    }

    #[test]
    fn names_make_safe_distinct_folders() {
        assert_eq!(
            path_part("play.example.net:25565"),
            "play.example.net~3a25565"
        );
        assert_ne!(path_part("a/b"), path_part("a_b"));
        assert_eq!(path_part(".."), "~2e~2e");
        assert_eq!(path_part(""), "~");
    }
}
