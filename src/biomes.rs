//! The biome of every explored chunk, remembered so the world map can name a spot far from
//! where you are now (the game only knows the biomes of the chunks it holds). One biome per
//! chunk, read at the chunk's middle on the surface, kept per 256 x 256 block region beside
//! the surface map as `b.<x>.<z>.txt`: a list of the region's biome ids, then one line of
//! 256 numbers (16 x 16 chunks, row by row; 0 for a chunk never read, else 1 + the biome's
//! place in the list).

use crate::store::{Disk, REGION_SHIFT};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Chunks across (and down) a region.
const SIDE: i32 = 1 << (REGION_SHIFT - 4);
const AREA: usize = (SIDE * SIDE) as usize;
const MAGIC: &str = "FRB1";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BiomeRegion {
    pub names: Vec<String>,
    pub chunks: Vec<u16>,
    pub dirty: bool,
}

impl BiomeRegion {
    fn empty() -> Self {
        Self {
            names: Vec::new(),
            chunks: vec![0; AREA],
            dirty: false,
        }
    }

    pub fn encode(&self) -> String {
        let mut out = String::from(MAGIC);
        out.push('\n');
        for n in &self.names {
            out.push_str(n);
            out.push('\n');
        }
        out.push('=');
        for (i, c) in self.chunks.iter().enumerate() {
            out.push(if i == 0 { ' ' } else { ',' });
            out.push_str(&c.to_string());
        }
        out.push('\n');
        out
    }

    pub fn decode(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()? != MAGIC {
            return None;
        }
        let mut r = Self::empty();
        for line in lines {
            if let Some(rest) = line.strip_prefix('=') {
                let chunks: Vec<u16> = rest
                    .trim()
                    .split(',')
                    .map(|n| n.trim().parse().ok())
                    .collect::<Option<_>>()?;
                if chunks.len() != AREA || chunks.iter().any(|&c| usize::from(c) > r.names.len()) {
                    return None;
                }
                r.chunks = chunks;
                return Some(r);
            }
            r.names.push(line.to_owned());
        }
        None
    }

    fn get(&self, i: usize) -> Option<&str> {
        let c = usize::from(*self.chunks.get(i)?);
        (c > 0)
            .then(|| self.names.get(c - 1).map(String::as_str))
            .flatten()
    }

    fn set(&mut self, i: usize, biome: &str) {
        let n = match self.names.iter().position(|n| n == biome) {
            Some(p) => p + 1,
            None => {
                self.names.push(biome.to_owned());
                self.names.len()
            }
        };
        if usize::from(self.chunks[i]) != n {
            self.chunks[i] = n as u16;
            self.dirty = true;
        }
    }
}

/// The remembered biomes of one dimension of one server or world (its surface).
pub struct BiomeMap {
    folder: String,
    regions: HashMap<(i32, i32), BiomeRegion>,
    missing: HashSet<(i32, i32)>,
    last_save: Instant,
}

impl BiomeMap {
    /// Biomes kept in `folder` (the surface map's own folder).
    pub fn new(folder: String) -> Self {
        Self {
            folder,
            regions: HashMap::new(),
            missing: HashSet::new(),
            last_save: Instant::now(),
        }
    }

    fn file(&self, key: (i32, i32)) -> String {
        format!("{}/b.{}.{}.txt", self.folder, key.0, key.1)
    }

    fn region(&mut self, disk: &dyn Disk, key: (i32, i32)) -> Option<&mut BiomeRegion> {
        if !self.regions.contains_key(&key) {
            if self.missing.contains(&key) {
                return None;
            }
            match disk
                .read(&self.file(key))
                .and_then(|b| BiomeRegion::decode(&String::from_utf8_lossy(&b)))
            {
                Some(r) => {
                    self.regions.insert(key, r);
                }
                None => {
                    self.missing.insert(key);
                    return None;
                }
            }
        }
        self.regions.get_mut(&key)
    }

    /// The biome remembered for the chunk holding block `(x, z)`.
    pub fn at(&mut self, disk: &dyn Disk, x: i32, z: i32) -> Option<String> {
        let (key, i) = place(x >> 4, z >> 4);
        self.region(disk, key)?.get(i).map(str::to_owned)
    }

    /// Whether chunk `(cx, cz)` has a biome remembered (read from disk if need be).
    pub fn known(&mut self, disk: &dyn Disk, cx: i32, cz: i32) -> bool {
        let (key, i) = place(cx, cz);
        self.region(disk, key).is_some_and(|r| r.get(i).is_some())
    }

    /// Remembers chunk `(cx, cz)`'s biome.
    pub fn set(&mut self, disk: &dyn Disk, cx: i32, cz: i32, biome: &str) {
        if biome.is_empty() {
            return;
        }
        let (key, i) = place(cx, cz);
        if self.region(disk, key).is_none() {
            self.missing.remove(&key);
            self.regions.insert(key, BiomeRegion::empty());
        }
        if let Some(r) = self.regions.get_mut(&key) {
            r.set(i, biome);
        }
    }

    /// Writes every changed region.
    pub fn save(&mut self, disk: &dyn Disk) {
        let folder = self.folder.clone();
        for (key, r) in self.regions.iter_mut().filter(|(_, r)| r.dirty) {
            let path = format!("{folder}/b.{}.{}.txt", key.0, key.1);
            if disk.write(&path, r.encode().as_bytes()) {
                r.dirty = false;
            }
        }
        self.last_save = Instant::now();
    }

    /// Now and then saves, then lets go of what is saved (it is small, and read again when
    /// asked).
    pub fn maintain(&mut self, disk: &dyn Disk) {
        if self.last_save.elapsed() < Duration::from_secs(45) {
            return;
        }
        self.save(disk);
        if self.regions.len() > 64 {
            self.regions.retain(|_, r| r.dirty);
        }
        self.missing.clear();
    }
}

/// The region of chunk `(cx, cz)` and its place in it.
fn place(cx: i32, cz: i32) -> ((i32, i32), usize) {
    let shift = REGION_SHIFT - 4;
    let key = (cx >> shift, cz >> shift);
    let i = ((cz & (SIDE - 1)) * SIDE + (cx & (SIDE - 1))) as usize;
    (key, i)
}

/// The chunks round `(cx, cz)` within `radius`, nearest first: the order biomes are read in.
pub fn ring_order(radius: i32) -> Vec<(i32, i32)> {
    let mut v: Vec<(i32, i32)> = (-radius..=radius)
        .flat_map(|dz| (-radius..=radius).map(move |dx| (dx, dz)))
        .collect();
    v.sort_by_key(|&(dx, dz)| (dx * dx + dz * dz, dz, dx));
    v
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
    fn biomes_are_remembered_per_chunk_and_survive_a_save() {
        let disk = Mem::default();
        let mut b = BiomeMap::new("maps/w/o".into());
        b.set(&disk, 0, 0, "minecraft:plains");
        b.set(&disk, -1, 3, "minecraft:dark_forest");
        b.set(&disk, 20, -40, "minecraft:desert");
        assert_eq!(b.at(&disk, 7, 15).as_deref(), Some("minecraft:plains"));
        assert_eq!(
            b.at(&disk, -16, 48).as_deref(),
            Some("minecraft:dark_forest")
        );
        assert_eq!(b.at(&disk, 16, 0), None);
        b.save(&disk);
        let mut files: Vec<String> = disk.0.borrow().keys().cloned().collect();
        files.sort();
        assert_eq!(
            files,
            [
                "maps/w/o/b.-1.0.txt",
                "maps/w/o/b.0.0.txt",
                "maps/w/o/b.1.-3.txt"
            ]
        );
        let mut again = BiomeMap::new("maps/w/o".into());
        assert_eq!(
            again.at(&disk, 330, -630).as_deref(),
            Some("minecraft:desert")
        );
        assert_eq!(
            again.at(&disk, -10, 60).as_deref(),
            Some("minecraft:dark_forest")
        );
        assert!(again.known(&disk, 0, 0) && !again.known(&disk, 1, 0));
    }

    #[test]
    fn a_region_file_round_trips_and_bad_ones_are_refused() {
        let mut r = BiomeRegion::empty();
        r.set(5, "minecraft:plains");
        r.set(6, "minecraft:river");
        r.set(7, "minecraft:plains");
        let text = r.encode();
        let back = BiomeRegion::decode(&text).expect("decodes");
        assert_eq!(back.get(5), Some("minecraft:plains"));
        assert_eq!(back.get(6), Some("minecraft:river"));
        assert_eq!(back.get(7), Some("minecraft:plains"));
        assert_eq!(back.get(8), None);
        assert_eq!(back.names.len(), 2);
        assert!(BiomeRegion::decode("nope").is_none());
        assert!(BiomeRegion::decode("FRB1\nminecraft:plains\n= 1,2\n").is_none());
    }

    #[test]
    fn chunks_are_read_nearest_first() {
        let o = ring_order(2);
        assert_eq!(o.len(), 25);
        assert_eq!(o[0], (0, 0));
        assert!(o[1..5].iter().all(|&(x, z)| x * x + z * z == 1));
    }
}
