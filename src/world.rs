//! The server or world being played and the dimension shown: its map, its waypoints and where
//! they are kept in the game's bulk data folder (`sunrise-mod-data/frontier/`):
//! `maps/<world>/<dimension>/r.<x>.<z>.bin` (the surface), `maps/<world>/<dimension>/caves/
//! y_<layer>/r.<x>.<z>.bin` (one cave layer per 16-block band of height, so a base at Y 10, a
//! cave at Y -40 and a dome's inside at Y 70 each keep their own map and never write over one
//! another or the surface) and `waypoints/<world>/<dimension>.txt`.

use crate::biomes::BiomeMap;
use crate::store::{path_part, Column, Disk, MapStore};
use crate::waypoints::{self, Waypoint};
use mod_api::ClientHost;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// The bulk data folder (`<instance>/sunrise-mod-data/frontier/`), as the map's `Disk`.
pub struct DataDisk {
    root: PathBuf,
}

/// Join `path` under `root`, refusing absolute paths and `..`.
fn safe_join(root: &Path, path: &str) -> Option<PathBuf> {
    if path.is_empty() || path.contains('\0') {
        return None;
    }
    let mut out = root.to_path_buf();
    for c in Path::new(path).components() {
        match c {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    out.starts_with(root).then_some(out)
}

impl Disk for DataDisk {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        let p = safe_join(&self.root, path)?;
        match std::fs::read(&p) {
            Ok(b) => Some(b),
            Err(_) => None,
        }
    }
    fn write(&self, path: &str, bytes: &[u8]) -> bool {
        let Some(p) = safe_join(&self.root, path) else {
            return false;
        };
        if let Some(parent) = p.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                return false;
            }
        }
        std::fs::write(&p, bytes).is_ok()
    }
}

pub fn disk(host: &dyn ClientHost) -> DataDisk {
    DataDisk {
        root: host
            .instance_dir()
            .join("sunrise-mod-data")
            .join("frontier"),
    }
}

/// A folder name for the server or world being played: the save's folder for a world of your
/// own, the address for a server.
pub fn world_key(host: &dyn ClientHost) -> Option<String> {
    let server = host.client_session().client_server()?;
    if let Some(save) = server.save_folder {
        return Some(format!("local~{}", path_part(&save)));
    }
    if let Some(addr) = server.address {
        // `play.example.net` and `play.example.net:25565` are the same server.
        let addr = addr.trim().to_ascii_lowercase();
        let addr = addr.strip_suffix(":25565").unwrap_or(&addr).to_owned();
        return Some(format!("server~{}", path_part(&addr)));
    }
    server.name.map(|n| format!("world~{}", path_part(&n)))
}

pub struct World {
    pub key: String,
    pub dimension: String,
    pub map: MapStore,
    /// The cave layers, by band (see [`layer_of`]): the floor read under each band's ceiling.
    pub caves: BTreeMap<i32, MapStore>,
    /// The surface's biome, chunk by chunk (`b.<x>.<z>.txt` beside the surface map).
    pub biomes: BiomeMap,
    pub waypoints: Vec<Waypoint>,
    pub waypoints_dirty: bool,
}

impl World {
    pub fn open(disk: &dyn Disk, key: String, dimension: String) -> Self {
        let map = MapStore::new(map_folder(&key, &dimension));
        let caves = BTreeMap::new();
        let biomes = BiomeMap::new(map_folder(&key, &dimension));
        let waypoints = disk
            .read(&waypoint_file(&key, &dimension))
            .map(|b| waypoints::decode(&String::from_utf8_lossy(&b)))
            .unwrap_or_default();
        Self {
            key,
            dimension,
            map,
            caves,
            biomes,
            waypoints,
            waypoints_dirty: false,
        }
    }

    /// The map shown for `view`: the surface (None) or cave layer `Some(layer)`; None for a
    /// cave layer not read yet.
    pub fn layer(&self, view: Option<i32>) -> Option<&MapStore> {
        match view {
            None => Some(&self.map),
            Some(l) => self.caves.get(&l),
        }
    }
    /// The map for `view`, made (empty, its files read as needed) the first time.
    pub fn layer_mut(&mut self, view: Option<i32>) -> &mut MapStore {
        match view {
            None => &mut self.map,
            Some(l) => {
                let (key, dim) = (&self.key, &self.dimension);
                self.caves
                    .entry(l)
                    .or_insert_with(|| MapStore::new(cave_folder(key, dim, l)))
            }
        }
    }
    /// The column at `(x, z)` in `view`, if read (unknown where nothing is).
    pub fn column(&self, view: Option<i32>, x: i32, z: i32) -> Column {
        self.layer(view)
            .map_or_else(Column::default, |m| m.column(x, z))
    }

    pub fn save(&mut self, disk: &dyn Disk) {
        self.map.save(disk);
        for c in self.caves.values_mut() {
            c.save(disk);
        }
        self.biomes.save(disk);
        self.save_waypoints(disk);
    }

    /// Saves now and then and lets idle regions go, every layer; a cave layer with nothing
    /// left in memory is let go too (its files stay, and are read again when shown).
    pub fn maintain(&mut self, disk: &dyn Disk, keep: Option<i32>) {
        self.map.maintain(disk);
        self.biomes.maintain(disk);
        for c in self.caves.values_mut() {
            c.maintain(disk);
        }
        self.caves
            .retain(|l, c| Some(*l) == keep || !c.regions.is_empty());
    }

    pub fn save_waypoints(&mut self, disk: &dyn Disk) {
        if self.waypoints_dirty
            && disk.write(
                &waypoint_file(&self.key, &self.dimension),
                waypoints::encode(&self.waypoints).as_bytes(),
            )
        {
            self.waypoints_dirty = false;
        }
    }
}

/// How tall a cave layer is, in blocks.
pub const LAYER_HEIGHT: i32 = 16;

/// The cave layer of height `y`: its 16-block band (`0` is Y 0 to 15, `-3` Y -48 to -33).
pub fn layer_of(y: i32) -> i32 {
    y.div_euclid(LAYER_HEIGHT)
}

/// The lowest and highest height of cave layer `layer`.
pub fn layer_range(layer: i32) -> (i32, i32) {
    let lo = layer * LAYER_HEIGHT;
    (lo, lo + LAYER_HEIGHT - 1)
}

/// The cave layers a dimension has, lowest and highest, from its lowest height and height.
pub fn layer_bounds(min_y: i32, height: i32) -> (i32, i32) {
    (layer_of(min_y), layer_of(min_y + height.max(1) - 1))
}

/// The folder of a dimension's surface map.
pub fn map_folder(key: &str, dimension: &str) -> String {
    format!("maps/{key}/{}", path_part(dimension))
}

/// The folder of one cave layer: `caves/y_<layer>` under the dimension's map.
pub fn cave_folder(key: &str, dimension: &str, layer: i32) -> String {
    format!("{}/caves/y_{layer}", map_folder(key, dimension))
}

pub fn waypoint_file(key: &str, dimension: &str) -> String {
    format!("waypoints/{key}/{}.txt", path_part(dimension))
}

/// Adds a waypoint to another dimension's list on disk (a death in a dimension not shown).
pub fn add_elsewhere(
    disk: &dyn Disk,
    key: &str,
    dimension: &str,
    w: Waypoint,
    replace_death: bool,
) {
    let path = waypoint_file(key, dimension);
    let mut list = disk
        .read(&path)
        .map(|b| waypoints::decode(&String::from_utf8_lossy(&b)))
        .unwrap_or_default();
    if replace_death {
        list.retain(|x| x.kind != waypoints::Kind::Death);
    }
    list.push(w);
    disk.write(&path, waypoints::encode(&list).as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Column;
    use std::cell::RefCell;
    use std::collections::HashMap;

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
    fn each_cave_layer_is_kept_apart_from_the_others_and_the_surface() {
        let disk = Mem::default();
        let mut w = World::open(&disk, "local~w".into(), "minecraft:overworld".into());
        // The surface, a base at Y 10 (layer 0), a deep cave at Y -40 (layer -3) and a dome's
        // inside at Y 70 (layer 4), all over the very same column.
        w.map.set(&disk, 10, 10, Column::new(0x55AA33, 70, 0));
        w.layer_mut(Some(layer_of(10)))
            .set(&disk, 10, 10, Column::new(0x1C1A1E, 9, 0));
        w.layer_mut(Some(layer_of(-40)))
            .set(&disk, 10, 10, Column::new(0x404040, -41, 0));
        w.layer_mut(Some(layer_of(70)))
            .set(&disk, 10, 10, Column::new(0x7FB238, 69, 0));
        w.layer_mut(Some(4))
            .set(&disk, 11, 10, Column::new(0x808080, 68, 0));
        w.save(&disk);
        let files: Vec<String> = disk.0.borrow().keys().cloned().collect();
        for f in [
            "maps/local~w/minecraft~3aoverworld/r.0.0.bin",
            "maps/local~w/minecraft~3aoverworld/caves/y_0/r.0.0.bin",
            "maps/local~w/minecraft~3aoverworld/caves/y_-3/r.0.0.bin",
            "maps/local~w/minecraft~3aoverworld/caves/y_4/r.0.0.bin",
        ] {
            assert!(files.contains(&f.to_owned()), "{f} in {files:?}");
        }
        assert_eq!(files.len(), 4, "{files:?}");
        // Opened again: each layer has its own columns, none written over by another.
        let mut again = World::open(&disk, "local~w".into(), "minecraft:overworld".into());
        let _ = again.map.region(&disk, (0, 0));
        for l in [0, -3, 4] {
            let _ = again.layer_mut(Some(l)).region(&disk, (0, 0));
        }
        let at = |w: &World, v: Option<i32>, x: i32| {
            let c = w.column(v, x, 10);
            (c.rgb(), c.height)
        };
        assert_eq!(at(&again, None, 10), (0x55AA33, 70));
        assert_eq!(at(&again, Some(0), 10), (0x1C1A1E, 9));
        assert_eq!(at(&again, Some(-3), 10), (0x404040, -41));
        assert_eq!(at(&again, Some(4), 10), (0x7FB238, 69));
        // A column read only in one layer is known only there.
        assert!(again.column(Some(4), 11, 10).known());
        assert!(!again.column(Some(0), 11, 10).known());
        assert!(!again.column(None, 11, 10).known());
        // A layer never read is simply empty, and isn't made by looking.
        assert!(!again.column(Some(9), 10, 10).known());
        assert!(again.layer(Some(9)).is_none());
    }

    #[test]
    fn layers_are_16_block_bands() {
        assert_eq!((layer_of(0), layer_of(15), layer_of(16)), (0, 0, 1));
        assert_eq!((layer_of(-1), layer_of(-16), layer_of(-17)), (-1, -1, -2));
        assert_eq!((layer_of(10), layer_of(-40), layer_of(70)), (0, -3, 4));
        assert_eq!(layer_range(3), (48, 63));
        assert_eq!(layer_range(-4), (-64, -49));
        // The Overworld (-64, 384 tall) and the Nether (0, 256).
        assert_eq!(layer_bounds(-64, 384), (-4, 19));
        assert_eq!(layer_bounds(0, 256), (0, 15));
        assert_eq!(
            cave_folder("local~w", "minecraft:the_nether", -2),
            "maps/local~w/minecraft~3athe_nether/caves/y_-2"
        );
    }
}
