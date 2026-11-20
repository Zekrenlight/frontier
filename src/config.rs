//! The settings: their keys, choices, defaults and the copy every part reads.

use std::sync::Mutex;

pub const ID: &str = "frontier";

/// Key mappings (Options > Controls, under Frontier): id and default key.
pub const KEYS: [(&str, &str); 7] = [
    ("world_map", "key.keyboard.m"),
    ("new_waypoint", "key.keyboard.b"),
    ("waypoints", "key.keyboard.j"),
    ("settings", "key.keyboard.k"),
    ("zoom_in", "key.keyboard.equal"),
    ("zoom_out", "key.keyboard.minus"),
    ("toggle_minimap", "key.keyboard.unknown"),
];

// Minimap
pub const KEY_MINIMAP: &str = "minimap";
// The minimap starts round with a plain frame, as the mod's icon has it.
pub const KEY_SHAPE: &str = "minimap_shape";
pub const KEY_CORNER: &str = "corner";
pub const KEY_SIZE: &str = "size";
pub const KEY_ZOOM: &str = "zoom";
pub const KEY_ROTATE: &str = "rotate";
pub const KEY_FRAME: &str = "minimap_frame";
pub const KEY_COMPASS: &str = "compass";
pub const KEY_COORDS: &str = "coordinates";
pub const KEY_BIOME: &str = "biome";
pub const KEY_LIGHTING: &str = "day_night";
pub const KEY_CAVES: &str = "cave_mode";
pub const KEY_MAP_WAYPOINTS: &str = "minimap_waypoints";
// Waypoints
pub const KEY_BEACONS: &str = "beacons";
pub const KEY_LABELS: &str = "labels";
pub const KEY_LABEL_RANGE: &str = "label_range";
pub const KEY_DEATH: &str = "death_waypoint";
// Map
pub const KEY_SLOPES: &str = "slopes";
// Creatures
pub const KEY_MOBS: &str = "mobs";
pub const KEY_MOBS_HOSTILE: &str = "mobs_hostile";
pub const KEY_MOBS_PASSIVE: &str = "mobs_passive";
pub const KEY_MOBS_PLAYERS: &str = "mobs_players";
pub const KEY_MOBS_RANGE: &str = "mobs_range";
pub const KEY_MOBS_FADE: &str = "mobs_height_fade";
pub const KEY_MOBS_WORLD_MAP: &str = "mobs_world_map";
pub const KEY_MOBS_VILLAGERS: &str = "mobs_villagers";
pub const KEY_DOT_STYLE: &str = "dot_style";
pub const KEY_NAMES: &str = "dot_names";
pub const KEY_PLAYER_NAMES: &str = "player_names";
// Things and blocks
// Dropped items start on (black dots); other things (purple) start off.
pub const KEY_ITEMS: &str = "items";
pub const KEY_OBJECTS: &str = "objects";
// Storage markers start off.
pub const KEY_CONTAINERS: &str = "storage";
pub const KEY_DECOR: &str = "decor";
// Look and behaviour
pub const KEY_SHARP: &str = "sharp_maps";
pub const KEY_HIDE_F3: &str = "hide_with_f3";
pub const KEY_MAP_COMPASS: &str = "map_compass";
pub const KEY_MAP_MENU: &str = "map_menu";
// Teleport here in the world map's menu, for Creative and Spectator (on at first).
pub const KEY_MAP_TELEPORT: &str = "map_teleport";

/// How creatures show on the maps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DotStyle {
    Dots,
    /// Their faces where one can be found, dots for the rest.
    Faces,
}

impl DotStyle {
    pub const OPTIONS: [&'static str; 2] = ["frontier.dot_style.dots", "frontier.dot_style.faces"];
    pub fn from_index(i: u32) -> Self {
        match i {
            0 => Self::Dots,
            _ => Self::Faces,
        }
    }
}

/// When names show beside the dots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Names {
    /// While the player list key (Tab) is held, or the pointer is on a dot of the world map.
    HoldTab,
    Always,
    Never,
}

impl Names {
    pub const OPTIONS: [&'static str; 3] = [
        "frontier.names.hold_tab",
        "frontier.names.always",
        "frontier.names.never",
    ];
    pub fn from_index(i: u32) -> Self {
        match i {
            1 => Self::Always,
            2 => Self::Never,
            _ => Self::HoldTab,
        }
    }
}

/// The minimap's outline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Square,
    Round,
}

impl Shape {
    pub const OPTIONS: [&'static str; 2] = ["frontier.shape.square", "frontier.shape.round"];
    pub fn from_index(i: u32) -> Self {
        match i {
            1 => Self::Round,
            _ => Self::Square,
        }
    }
    pub fn index(self) -> u32 {
        match self {
            Self::Square => 0,
            Self::Round => 1,
        }
    }
}

/// Which screen corner the minimap sits in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft,
}

impl Corner {
    pub const OPTIONS: [&'static str; 4] = [
        "frontier.corner.top_right",
        "frontier.corner.top_left",
        "frontier.corner.bottom_right",
        "frontier.corner.bottom_left",
    ];
    pub fn from_index(i: u32) -> Self {
        match i {
            1 => Self::TopLeft,
            2 => Self::BottomRight,
            3 => Self::BottomLeft,
            _ => Self::TopRight,
        }
    }
    pub fn right(self) -> bool {
        matches!(self, Self::TopRight | Self::BottomRight)
    }
    pub fn bottom(self) -> bool {
        matches!(self, Self::BottomRight | Self::BottomLeft)
    }
}

/// What goes round the minimap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameStyle {
    /// A thin dark rim with a light inner line.
    Plain,
    /// A frayed paper edge.
    Parchment,
    /// Nothing: the map alone.
    None,
}

impl FrameStyle {
    pub const OPTIONS: [&'static str; 3] = [
        "frontier.frame.plain",
        "frontier.frame.parchment",
        "frontier.frame.none",
    ];
    pub fn from_index(i: u32) -> Self {
        match i {
            1 => Self::Parchment,
            2 => Self::None,
            _ => Self::Plain,
        }
    }
    pub fn index(self) -> u32 {
        match self {
            Self::Plain => 0,
            Self::Parchment => 1,
            Self::None => 2,
        }
    }
}

/// Blocks per map pixel at each minimap zoom step (1 is the closest).
pub const ZOOM_STEPS: [f32; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub minimap: bool,
    pub shape: Shape,
    pub corner: Corner,
    /// The map's side in GUI pixels.
    pub size: f32,
    /// 1 to 5, an index into `ZOOM_STEPS` plus one.
    pub zoom: u32,
    pub rotate: bool,
    pub frame: FrameStyle,
    pub compass: bool,
    pub coords: bool,
    pub biome: bool,
    pub lighting: bool,
    pub caves: bool,
    pub map_waypoints: bool,
    pub beacons: bool,
    pub labels: bool,
    /// Labels and beacons further than this many blocks are hidden (0: no limit).
    pub label_range: f32,
    pub death: bool,
    pub slopes: bool,
    /// Creature dots on the minimap, by kind.
    pub mobs: bool,
    pub mobs_hostile: bool,
    pub mobs_passive: bool,
    pub mobs_players: bool,
    /// How far dots reach, in blocks (0: as far as the minimap shows).
    pub mobs_range: f32,
    /// Dots far above or below fade, with a tick showing which way.
    pub mobs_fade: bool,
    pub mobs_world_map: bool,
    pub mobs_villagers: bool,
    pub dot_style: DotStyle,
    pub names: Names,
    /// Players' names show with their heads while names do, though they have a face.
    pub player_names: bool,
    /// Dropped items, as black dots.
    pub items: bool,
    /// Things that aren't creatures: item frames, armour stands, minecarts, boats, paintings,
    /// cushions, as purple diamonds (off at first).
    pub objects: bool,
    /// Chests, barrels, shulker boxes, furnaces and the like.
    pub containers: bool,
    /// Signs, banners, beds, heads, lecterns, campfires and the like.
    pub decor: bool,
    /// Pictures with more pixels per block: twice the minimap's, and two per GUI pixel on the
    /// world map.
    pub sharp: bool,
    /// The minimap steps aside while the F3 screen shows over play.
    pub hide_with_f3: bool,
    /// A compass in the world map's corner.
    pub map_compass: bool,
    /// Right-click on the world map opens a menu (off: right-click adds a waypoint there).
    pub map_menu: bool,
    /// *Teleport here* in the world map's menu, in Creative or Spectator with `/tp` allowed.
    pub map_teleport: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            minimap: true,
            shape: Shape::Round,
            corner: Corner::TopRight,
            size: 64.0,
            zoom: 3,
            rotate: false,
            frame: FrameStyle::Plain,
            compass: true,
            coords: true,
            biome: true,
            lighting: true,
            caves: true,
            map_waypoints: true,
            beacons: true,
            labels: true,
            label_range: 0.0,
            death: true,
            slopes: true,
            mobs: true,
            mobs_hostile: true,
            mobs_passive: true,
            mobs_players: true,
            mobs_range: 0.0,
            mobs_fade: true,
            mobs_world_map: true,
            mobs_villagers: true,
            dot_style: DotStyle::Faces,
            names: Names::HoldTab,
            player_names: true,
            items: true,
            objects: false,
            containers: false,
            decor: false,
            sharp: true,
            hide_with_f3: true,
            map_compass: true,
            map_menu: true,
            map_teleport: true,
        }
    }
}

impl Config {
    /// Blocks per map pixel for the minimap's zoom.
    pub fn blocks_per_pixel(&self) -> f32 {
        ZOOM_STEPS[(self.zoom.clamp(1, 5) - 1) as usize]
    }
}

impl Config {
    /// How far from the player creature dots are looked for: the range set, or what the
    /// minimap reaches (to its corners), never past 256 blocks.
    pub fn mobs_radius(&self) -> f64 {
        let shown = f64::from(self.blocks_per_pixel()) * 64.0 * std::f64::consts::SQRT_2;
        let r = if self.mobs_range > 0.0 {
            f64::from(self.mobs_range)
        } else {
            shown
        };
        r.clamp(8.0, 256.0)
    }
}

static CONFIG: Mutex<Option<Config>> = Mutex::new(None);

pub fn set_config(config: Config) {
    *CONFIG.lock().unwrap_or_else(|e| e.into_inner()) = Some(config);
}
pub fn config() -> Config {
    CONFIG
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .unwrap_or_default()
}
pub fn config_loaded() -> bool {
    CONFIG.lock().unwrap_or_else(|e| e.into_inner()).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_items_start_on_and_other_things_off() {
        let d = Config::default();
        assert!(d.items, "dropped items (black dots) are on at first");
        assert!(
            !d.objects,
            "boats, minecarts, armour stands and the like are off at first"
        );
        assert!(!d.containers && !d.decor, "block markers are off at first");
        assert!(d.mobs && d.mobs_hostile && d.mobs_passive && d.mobs_players && d.mobs_villagers);
        assert!(d.caves);
        assert!(d.map_teleport, "Teleport here is offered at first (in Creative)");
        assert_eq!(d.size, 64.0, "the minimap starts at its smallest size");
        assert_eq!(d.zoom, 3, "one block a map pixel");
        assert_ne!(KEY_ITEMS, KEY_OBJECTS);
    }
}
