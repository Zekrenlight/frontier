//! Creatures, things and marked blocks near the player, for the maps' dots.
//!
//! The mod API can't tell one creature type from another on a client, so the game side
//! (`src/native/glue.rs`) reads its own copies of the creatures once a tick, when asked
//! here, and hands back a short list; the rest of the mod only reads that list.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Mutex;

/// What a dot stands for, and so its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Hostile,
    Passive,
    /// Villagers and wandering traders.
    Villager,
    Player,
    /// Things in the world that aren't creatures or dropped items: item frames, armour
    /// stands, minecarts, boats, paintings, cushions.
    Object,
    /// Dropped items (`minecraft:item`).
    Item,
    /// Blocks that hold things: chests, barrels, shulker boxes, furnaces, hoppers.
    Container,
    /// Blocks you place to look at: signs, banners, beds, heads, lecterns, campfires.
    Decor,
}

impl Kind {
    pub fn argb(self) -> u32 {
        match self {
            Kind::Hostile => 0xFF_E8463A,
            Kind::Passive => 0xFF_5CD65A,
            Kind::Villager => 0xFF_F2D23C,
            Kind::Player => 0xFF_FFFFFF,
            Kind::Object => 0xFF_B868F0,
            Kind::Item => 0xFF_000000,
            Kind::Container => 0xFF_E89A3C,
            Kind::Decor => 0xFF_58C8D0,
        }
    }
    /// Blocks rather than entities: their names are `block.*` keys.
    pub fn is_block(self) -> bool {
        matches!(self, Kind::Container | Kind::Decor)
    }
    /// The colour of its name beside the dot (a black item's name is written light).
    pub fn text_argb(self) -> u32 {
        match self {
            Kind::Item => 0xFF_E8E8E8,
            k => k.argb(),
        }
    }
    /// Creatures, which can have faces.
    pub fn is_creature(self) -> bool {
        matches!(
            self,
            Kind::Hostile | Kind::Passive | Kind::Villager | Kind::Player
        )
    }
}

/// Where a creature's face comes from, for the Faces dot style.
#[derive(Clone, Debug, PartialEq)]
pub enum Face {
    /// No face known: a dot.
    None,
    /// A player's head as the game draws chat heads: their skin's face and hat layer.
    Player { name: String },
    /// Made from the creature's model: `plan` read from texture `path`, kept under `key`; or,
    /// when that paints nothing, `or`'s plan read from its texture (a resource pack's own
    /// model, when its texture is laid out for that model only).
    Model {
        key: String,
        path: String,
        plan: std::sync::Arc<crate::headshot::Plan>,
        or: Option<(String, std::sync::Arc<crate::headshot::Plan>)>,
    },
    /// A marker image another mod or a resource pack gives for entity type `entity` (see
    /// [`crate::faces::marker_path`]), used before anything Frontier makes itself; where there
    /// is none, `then` (the face made from the model, or no face: a dot).
    Marker { entity: String, then: Box<Face> },
}

impl Face {
    /// What a face is kept under in the face atlas (a player's head isn't kept there).
    pub fn key(&self) -> Option<String> {
        match self {
            Face::None | Face::Player { .. } => None,
            Face::Model { key, .. } => Some(key.clone()),
            Face::Marker { entity, .. } => Some(format!("marker|{entity}")),
        }
    }
    /// Whether there is a face to show instead of a dot.
    pub fn some(&self) -> bool {
        !matches!(self, Face::None)
    }
    /// A marker image to try first for entity `entity`, else `then`. Players keep their heads.
    pub fn marker_or(entity: &str, then: Face) -> Face {
        match then {
            Face::Player { .. } => then,
            _ if entity.is_empty() => then,
            _ => Face::Marker {
                entity: entity.to_owned(),
                then: Box::new(then),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Dot {
    pub pos: [f64; 3],
    pub kind: Kind,
    /// A player's or a custom name when `named`, else the type's id (`minecraft:cow`, or a
    /// block's id for blocks).
    pub label: String,
    pub named: bool,
    pub face: Face,
    /// A dropped item's item id (`minecraft:cobblestone`); empty for everything else.
    pub item: String,
    /// A dropped item's stack size (0 for everything else).
    pub count: u32,
}

impl Dot {
    /// A creature, thing or marked block.
    pub fn new(pos: [f64; 3], kind: Kind, label: String, named: bool, face: Face) -> Self {
        Self {
            pos,
            kind,
            label,
            named,
            face,
            item: String::new(),
            count: 0,
        }
    }
    /// A dropped item: `name` is the stack's name as the game shows it (its custom name when
    /// renamed, else the item's own name in the language in use; empty when unknown), `item`
    /// its id and `count` how many.
    pub fn dropped(pos: [f64; 3], item: &str, name: String, count: u32) -> Self {
        let named = !name.trim().is_empty();
        Self {
            pos,
            kind: Kind::Item,
            label: name,
            named,
            face: Face::None,
            item: item.to_owned(),
            count,
        }
    }
}

/// The lang keys an item id may have its name under: `item.minecraft.diamond`, then
/// `block.minecraft.oak_log` for a block's item.
pub fn item_keys(id: &str) -> [String; 2] {
    let (ns, path) = id.split_once(':').unwrap_or(("minecraft", id));
    let path = path.replace('/', ".");
    [format!("item.{ns}.{path}"), format!("block.{ns}.{path}")]
}

/// `oak_log` as words: "Oak Log".
pub fn title_case(id: &str) -> String {
    let path = id.rsplit(':').next().unwrap_or(id);
    path.split(['_', '/'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or(String::new(), |f| {
                f.to_uppercase().collect::<String>() + c.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A dropped item's name on the maps: the game's name for the stack (a custom name as it was
/// given), else the item's name in the language (`translate`), else its id in words; then
/// "×16" for a stack of more than one. Never just "Item" when anything better is known.
pub fn item_label(
    name: &str,
    id: &str,
    count: u32,
    translate: impl Fn(&str) -> Option<String>,
) -> String {
    let name = name.trim();
    let base = if !name.is_empty() {
        name.to_owned()
    } else if id.is_empty() || id == "minecraft:air" {
        "Item".to_owned()
    } else {
        let [item, block] = item_keys(id);
        translate(&item)
            .or_else(|| translate(&block))
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| title_case(id))
    };
    if count > 1 {
        format!("{base} \u{d7}{count}")
    } else {
        base
    }
}

/// What the next list is wanted for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wanted {
    pub centre: [f64; 3],
    /// Blocks round `centre`, across.
    pub radius: f64,
    pub creatures: bool,
    pub objects: bool,
    /// Dropped items, apart from the other things.
    pub items: bool,
    pub containers: bool,
    pub decor: bool,
    /// Block markers further above or below than this are left out.
    pub block_height: f64,
    /// Work out where each creature's face is.
    pub faces: bool,
}

static WANTED: Mutex<Option<Wanted>> = Mutex::new(None);
static DOTS: Mutex<Vec<Dot>> = Mutex::new(Vec::new());
/// The player list key (Tab) is held: set by the game side each frame.
static TAB: AtomicBool = AtomicBool::new(false);
/// The F3 screen is up over play: set by the game side each frame.
static DEBUG_SHOWN: AtomicBool = AtomicBool::new(false);

/// Asks for a list (the mod's tick), or none (`None` clears the last one).
pub fn want(wanted: Option<Wanted>) {
    if wanted.is_none() {
        DOTS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
    *WANTED.lock().unwrap_or_else(|e| e.into_inner()) = wanted;
}

/// The game side: the request, taken, so the list is made once per ask.
pub fn wanted() -> Option<Wanted> {
    WANTED.lock().unwrap_or_else(|e| e.into_inner()).take()
}

/// The game side: what was found.
pub fn put(dots: Vec<Dot>) {
    *DOTS.lock().unwrap_or_else(|e| e.into_inner()) = dots;
}

/// A copy of the latest list.
pub fn dots() -> Vec<Dot> {
    DOTS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The game side, once a frame: whether Tab is held and whether F3 is showing over play.
pub fn set_inputs(tab: bool, debug_shown: bool) {
    TAB.store(tab, Ordering::Relaxed);
    DEBUG_SHOWN.store(debug_shown, Ordering::Relaxed);
}

/// The game side, once a frame: the local player's permission level as the server told the
/// client (0 to 4; what vanilla's F3 + F4 and `/tp` go by).
pub fn set_permission(level: u8) {
    PERMISSION.store(level.min(4), Ordering::Relaxed);
}

/// The local player's permission level, None until the game side has said.
pub fn permission() -> Option<u8> {
    match PERMISSION.load(Ordering::Relaxed) {
        u8::MAX => None,
        l => Some(l),
    }
}

static PERMISSION: AtomicU8 = AtomicU8::new(u8::MAX);

/// Whether the player list key is held (the game's own key state, so it works while the
/// player list shows, and on the world map).
pub fn tab_held() -> bool {
    TAB.load(Ordering::Relaxed)
}

/// Whether the F3 screen is showing over play (the game's or SunnyF3's).
pub fn debug_shown() -> bool {
    DEBUG_SHOWN.load(Ordering::Relaxed)
}

/// The kind of a creature that isn't a player, from its type and spawn category.
pub fn kind_of(id: &str, category: &str) -> Kind {
    match id {
        "minecraft:villager" | "minecraft:wandering_trader" => Kind::Villager,
        // In the monster category but no enemies: they leave you be (the first three turn
        // only when a hostile rider is on them, as the game's own `Enemy` check has it).
        "minecraft:zombie_horse"
        | "minecraft:camel_husk"
        | "minecraft:zombie_nautilus"
        | "minecraft:sulfur_cube" => Kind::Passive,
        _ if category == "monster" => Kind::Hostile,
        _ => Kind::Passive,
    }
}

/// The kind of a thing that isn't a creature, from its type id: dropped items have their
/// own; boats, minecarts, armour stands, frames and the rest are things.
pub fn kind_of_thing(id: &str) -> Kind {
    match id {
        "minecraft:item" | "item" => Kind::Item,
        _ => Kind::Object,
    }
}

/// The kind of a block entity's block (`minecraft:chest`), None for one not marked.
pub fn kind_of_block(id: &str) -> Option<Kind> {
    let path = id.rsplit(':').next().unwrap_or(id);
    const CONTAINERS: [&str; 10] = [
        "chest",
        "barrel",
        "shulker_box",
        "hopper",
        "dispenser",
        "dropper",
        "furnace",
        "smoker",
        "brewing_stand",
        "crafter",
    ];
    const DECOR: [&str; 15] = [
        "bed",
        "sign",
        "banner",
        "head",
        "skull",
        "lectern",
        "campfire",
        "bell",
        "jukebox",
        "beacon",
        "enchanting_table",
        "shelf",
        "decorated_pot",
        "conduit",
        "statue",
    ];
    if CONTAINERS.iter().any(|c| path.ends_with(c)) {
        Some(Kind::Container)
    } else if DECOR.iter().any(|c| path.ends_with(c)) {
        Some(Kind::Decor)
    } else {
        None
    }
}

/// The lang key of an entity type id: `minecraft:cow` is `entity.minecraft.cow`.
pub fn type_key(id: &str) -> String {
    let (ns, path) = id.split_once(':').unwrap_or(("minecraft", id));
    format!("entity.{ns}.{}", path.replace('/', "."))
}

/// The lang key of a block id: `minecraft:chest` is `block.minecraft.chest`.
pub fn block_key(id: &str) -> String {
    let (ns, path) = id.split_once(':').unwrap_or(("minecraft", id));
    format!("block.{ns}.{}", path.replace('/', "."))
}

/// How far a non-player entity may be above or below the layer being viewed (~5 blocks).
pub const LAYER_REACH: f64 = 5.5;
/// How far below a known surface column's top a non-player entity may still be. A mob
/// standing on a block at height 63 has y = 64; this still hides anything under the ground.
pub const SURFACE_BELOW: f64 = 4.5;

/// Which map picture the vertical filter is applied to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerView {
    /// The minimap in cave mode (and while it is fading toward the cave picture).
    MiniCave,
    /// The minimap on the surface (and while it is fading toward the surface).
    MiniSurface,
    /// The world map's surface view.
    WorldSurface,
    /// The world map's cave layer `layer`, a 16-block band from `lo` to `hi`.
    WorldCave {
        layer: i32,
        lo: i32,
        hi: i32,
        player_layer: i32,
    },
}

/// Whether a dot of `kind` at height `entity_y` belongs on this view.
///
/// Other players are always shown, at any height (a family on a server should still find
/// each other). Everything else must be on the layer being looked at:
/// - minimap, cave mode: within [`LAYER_REACH`] of the player's y;
/// - minimap or world map, surface: on or just under the surface at that column
///   (`entity_y >= height - SURFACE_BELOW`) when the column is known, else within
///   [`LAYER_REACH`] of the player (so unknown ground never reveals a cave full of mobs);
/// - world map, a cave layer: within [`LAYER_REACH`] of that layer's scanned floor at the
///   column when it is a real floor (not solid rock or open void); otherwise, within
///   [`LAYER_REACH`] of the player when looking at their own layer, or within the layer's
///   16-block band plus 5 blocks when looking at another.
pub fn on_layer(
    kind: Kind,
    entity_y: f64,
    player_y: f64,
    view: LayerView,
    column: Option<(i32, bool)>,
) -> bool {
    if kind == Kind::Player {
        return true;
    }
    match view {
        LayerView::MiniCave => near_y(entity_y, player_y, LAYER_REACH),
        LayerView::MiniSurface | LayerView::WorldSurface => match column {
            Some((h, _)) => entity_y >= f64::from(h) - SURFACE_BELOW,
            None => near_y(entity_y, player_y, LAYER_REACH),
        },
        LayerView::WorldCave {
            layer,
            lo,
            hi,
            player_layer,
        } => match column {
            Some((h, true)) => near_y(entity_y, f64::from(h), LAYER_REACH),
            _ if layer == player_layer => near_y(entity_y, player_y, LAYER_REACH),
            _ => entity_y >= f64::from(lo) - 5.0 && entity_y <= f64::from(hi) + 5.0,
        },
    }
}

/// Whether `entity_y` is within `reach` blocks of `origin`.
pub fn near_y(entity_y: f64, origin: f64, reach: f64) -> bool {
    (entity_y - origin).abs() <= reach
}

/// How opaque a dot `dy` blocks above or below the player is: full within 4 blocks, down
/// to 35% at 36 and beyond.
pub fn height_alpha(dy: f64) -> f32 {
    let t = ((dy.abs() - 4.0) / 32.0).clamp(0.0, 1.0) as f32;
    1.0 - 0.65 * t
}

/// `argb` with its alpha multiplied by `a`.
pub fn fade(argb: u32, a: f32) -> u32 {
    let alpha = ((argb >> 24) as f32 * a.clamp(0.0, 1.0)).round() as u32;
    (alpha << 24) | (argb & 0x00FF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_and_keys() {
        assert_eq!(kind_of("minecraft:zombie", "monster"), Kind::Hostile);
        assert_eq!(kind_of("minecraft:cow", "creature"), Kind::Passive);
        assert_eq!(kind_of("minecraft:villager", "misc"), Kind::Villager);
        assert_eq!(
            kind_of("minecraft:wandering_trader", "creature"),
            Kind::Villager
        );
        for id in [
            "minecraft:sulfur_cube",
            "minecraft:camel_husk",
            "minecraft:zombie_nautilus",
            "minecraft:zombie_horse",
        ] {
            assert_eq!(kind_of(id, "monster"), Kind::Passive, "{id}");
        }
        assert_eq!(kind_of("minecraft:creeper", "monster"), Kind::Hostile);
        assert_eq!(
            kind_of_block("minecraft:trapped_chest"),
            Some(Kind::Container)
        );
        assert_eq!(
            kind_of_block("minecraft:red_shulker_box"),
            Some(Kind::Container)
        );
        assert_eq!(
            kind_of_block("minecraft:oak_hanging_sign"),
            Some(Kind::Decor)
        );
        assert_eq!(kind_of_block("minecraft:spawner"), None);
        assert_eq!(
            type_key("minecraft:zombie_villager"),
            "entity.minecraft.zombie_villager"
        );
        assert_eq!(type_key("pig"), "entity.minecraft.pig");
        assert_eq!(block_key("minecraft:barrel"), "block.minecraft.barrel");
    }

    #[test]
    fn dropped_items_are_their_own_black_dots() {
        assert_eq!(kind_of_thing("minecraft:item"), Kind::Item);
        assert_eq!(Kind::Item.argb(), 0xFF_000000);
        assert!(!Kind::Item.is_creature() && !Kind::Item.is_block());
        // Readable beside a black dot.
        assert_ne!(Kind::Item.text_argb() & 0xFF_FFFF, 0);
        for id in [
            "minecraft:boat",
            "minecraft:minecart",
            "minecraft:armor_stand",
            "minecraft:item_frame",
            "minecraft:painting",
            "minecraft:cushion",
        ] {
            assert_eq!(kind_of_thing(id), Kind::Object, "{id}");
        }
        assert_eq!(Kind::Object.argb(), 0xFF_B868F0);
        assert_ne!(Kind::Item, Kind::Object);
    }

    #[test]
    fn far_dots_fade_but_stay_visible() {
        assert_eq!(height_alpha(2.0), 1.0);
        assert!((height_alpha(-100.0) - 0.35).abs() < 1e-6);
        assert!(height_alpha(20.0) < 1.0 && height_alpha(20.0) > 0.35);
        assert_eq!(fade(0xFF_123456, 0.5) >> 24, 0x80);
        assert_eq!(fade(0xFF_123456, 0.5) & 0xFF_FFFF, 0x123456);
    }

    #[test]
    fn a_request_is_taken_once() {
        let w = Wanted {
            centre: [1.0, 2.0, 3.0],
            radius: 50.0,
            creatures: true,
            objects: false,
            items: true,
            containers: false,
            decor: false,
            block_height: 24.0,
            faces: false,
        };
        want(Some(w));
        assert_eq!(wanted(), Some(w));
        assert_eq!(wanted(), None);
        put(vec![Dot::new(
            [0.0; 3],
            Kind::Player,
            "A".into(),
            true,
            Face::None,
        )]);
        assert_eq!(dots().len(), 1);
        want(None);
        assert!(dots().is_empty());
    }

    #[test]
    fn dropped_items_have_their_real_names() {
        let lang = |k: &str| match k {
            "item.minecraft.diamond" => Some("Diamond".to_owned()),
            "block.minecraft.oak_log" => Some("Oak Log".to_owned()),
            "block.minecraft.cobblestone" => Some("Cobblestone".to_owned()),
            _ => None,
        };
        // The game's name for the stack (translated, or a custom name) wins, with the count.
        assert_eq!(
            item_label("Cobblestone", "minecraft:cobblestone", 16, lang),
            "Cobblestone \u{d7}16"
        );
        assert_eq!(
            item_label("Diamond", "minecraft:diamond", 1, lang),
            "Diamond"
        );
        assert_eq!(
            item_label("Sword of Dawn", "minecraft:diamond_sword", 1, lang),
            "Sword of Dawn"
        );
        assert_eq!(
            item_label("  Lucky Pick ", "minecraft:iron_pickaxe", 2, lang),
            "Lucky Pick \u{d7}2"
        );
        // No name from the game: the item's lang key, a block's, then the id in words.
        assert_eq!(
            item_label("", "minecraft:diamond", 3, lang),
            "Diamond \u{d7}3"
        );
        assert_eq!(
            item_label("", "minecraft:oak_log", 64, lang),
            "Oak Log \u{d7}64"
        );
        assert_eq!(item_label("", "mymod:copper_gear", 1, lang), "Copper Gear");
        // Only when nothing at all is known.
        assert_eq!(item_label("", "", 1, lang), "Item");
        assert_eq!(
            item_keys("minecraft:oak_log"),
            [
                "item.minecraft.oak_log".to_owned(),
                "block.minecraft.oak_log".to_owned()
            ]
        );
        let d = Dot::dropped([0.0; 3], "minecraft:diamond", "Diamond".into(), 5);
        assert!(d.named && d.kind == Kind::Item && d.count == 5 && d.item == "minecraft:diamond");
        assert!(!Dot::dropped([0.0; 3], "minecraft:stone", String::new(), 1).named);
        assert!(Dot::new(
            [0.0; 3],
            Kind::Passive,
            "minecraft:cow".into(),
            false,
            Face::None
        )
        .item
        .is_empty());
    }

    #[test]
    fn players_show_on_every_layer_and_other_dots_stay_near_theirs() {
        let player = Kind::Player;
        let zombie = Kind::Hostile;
        let cow = Kind::Passive;
        let item = Kind::Item;
        let boat = Kind::Object;
        let villager = Kind::Villager;
        let chest = Kind::Container;
        let py = 64.0;
        // Other players: any height, both maps, cave or surface.
        for view in [
            LayerView::MiniCave,
            LayerView::MiniSurface,
            LayerView::WorldSurface,
            LayerView::WorldCave {
                layer: 4,
                lo: 64,
                hi: 79,
                player_layer: 4,
            },
            LayerView::WorldCave {
                layer: -3,
                lo: -48,
                hi: -33,
                player_layer: 4,
            },
        ] {
            assert!(on_layer(player, 64.0, py, view, None));
            assert!(on_layer(player, -40.0, py, view, Some((70, true))));
            assert!(on_layer(player, 200.0, py, view, Some((10, false))));
        }
        // Minimap, cave mode: only within ~5 blocks of the player.
        let cave = LayerView::MiniCave;
        assert!(on_layer(zombie, 64.0, py, cave, None));
        assert!(on_layer(cow, 69.5, py, cave, None));
        assert!(on_layer(item, 58.5, py, cave, None));
        assert!(!on_layer(zombie, 70.0, py, cave, None));
        assert!(!on_layer(item, 58.0, py, cave, None));
        assert!(
            !on_layer(boat, 20.0, py, cave, None),
            "a mob far below must not show"
        );
        // A known surface column must not punch a hole in cave mode's player-y rule.
        assert!(!on_layer(zombie, 20.0, py, cave, Some((20, true))));

        // Minimap / world map, surface: on the surface at that column, not under it.
        for view in [LayerView::MiniSurface, LayerView::WorldSurface] {
            // A cow on a hill (col.height 73, standing at 74) and one in a valley both show.
            assert!(on_layer(cow, 74.0, py, view, Some((73, true))));
            assert!(on_layer(cow, 58.0, py, view, Some((57, true))));
            // A mob standing on the top block (height 63, y = 64).
            assert!(on_layer(zombie, 64.0, py, view, Some((63, true))));
            // A cave mob under the ground is hidden, even only a few blocks down.
            assert!(!on_layer(zombie, 58.0, py, view, Some((63, true))));
            assert!(!on_layer(item, 10.0, py, view, Some((63, true))));
            assert!(!on_layer(chest, 40.0, py, view, Some((70, true))));
            // Column not yet known: fall back to ~5 blocks of the player.
            assert!(on_layer(cow, 68.0, py, view, None));
            assert!(!on_layer(zombie, 20.0, py, view, None));
            assert!(!on_layer(villager, 80.0, py, view, None));
        }

        // World map, the player's own cave layer, floor known: within ~5 of that floor.
        let own = LayerView::WorldCave {
            layer: 4,
            lo: 64,
            hi: 79,
            player_layer: 4,
        };
        assert!(on_layer(zombie, 69.0, py, own, Some((70, true))));
        assert!(on_layer(item, 64.5, py, own, Some((70, true))));
        assert!(!on_layer(zombie, 80.0, py, own, Some((70, true))));
        assert!(!on_layer(cow, 10.0, py, own, Some((70, true))));
        // Rock or void (or unknown): fall back to the player's y on their own layer.
        assert!(on_layer(zombie, 68.0, py, own, Some((70, false))));
        assert!(!on_layer(zombie, 20.0, py, own, Some((70, false))));
        assert!(on_layer(cow, 60.0, py, own, None));
        assert!(!on_layer(cow, 50.0, py, own, None));

        // World map, another cave layer, floor known: within ~5 of that floor, not the player.
        let deep = LayerView::WorldCave {
            layer: -3,
            lo: -48,
            hi: -33,
            player_layer: 4,
        };
        assert!(on_layer(zombie, -41.0, py, deep, Some((-40, true))));
        assert!(!on_layer(zombie, 64.0, py, deep, Some((-40, true))));
        // Unknown / rock / void: the layer's band plus 5 blocks.
        assert!(on_layer(cow, -40.0, py, deep, None));
        assert!(on_layer(cow, -53.0, py, deep, Some((0, false))));
        assert!(on_layer(item, -28.0, py, deep, None));
        assert!(!on_layer(zombie, 64.0, py, deep, None));
        assert!(!on_layer(zombie, -60.0, py, deep, None));
    }
}
