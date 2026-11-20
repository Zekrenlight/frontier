//! Frontier 1.0.0 for Sunrise, by Zekrenlight (MIT).
//!
//! - Minimap: square or round, in any corner, any size, zoomed with keys, north up or turned
//!   with the player, with compass letters, coordinates and biome under it, the time of day's
//!   light, a cave view underground or under any roof (glass domes too) that fades in and out,
//!   and a plain, paper or no frame.
//! - World map (M): everything explored, saved per server or world and per dimension, as a
//!   surface view and cave layers 16 blocks tall, each saved apart (underground it opens on your
//!   own layer, read live across every loaded chunk; the Surface | Cave switch or C, and the
//!   layer buttons or [ and ], change it); drag to pan, scroll to zoom, the coordinates under
//!   the cursor. Dropped items show their icon and real name while Tab is held.
//! - Waypoints (B to add, J for the list): named and coloured, on both maps and as beacons
//!   with labels in the world; a temporary one where you last died.
//!
//! Colours come from the blocks' own textures (the resource packs in use), shading from the
//! heights around each column. Everything here is the mod's own; the only game-side part is
//! `src/native/glue.rs`, which opens the settings page from the settings key.

pub mod cave;
pub mod colors;
pub mod config;
pub mod faces;
pub mod frame;
pub mod headshot;
pub mod png;
pub mod radar;
pub mod scan;
pub mod shade;
pub mod store;
pub mod ui;
pub mod view;
pub mod waypoints;
pub mod world;

pub use config::*;

use colors::BlockColors;
use faces::{Faces, FACES_TEX};
use mod_api::client::widgets::Outcome;
use mod_api::{
    ClientBlockChangedEvent, ClientChunkLoadEvent, ClientHost, ClientLevelUnloadEvent,
    ClientPlayerLoggingOutEvent, ClientStoppingEvent, ClientTickPostEvent, CommandEvent,
    Corner as HudCorner, DrawContext, Flow, Hooks, Identifier, ItemKey, KeyRegistration,
    NameTagOptions, RegisterClientCommandsEvent, Registrar, RegistrationContext,
    RenderFramePreEvent, RenderGuiPostEvent, RenderLevelStage, RenderLevelStageEvent,
    ScreenCharTypedPreEvent, ScreenKeyPressedPreEvent, ScreenMouseButtonPressedPreEvent,
    ScreenMouseButtonReleasedPreEvent, ScreenMouseDraggedPreEvent, ScreenMouseScrolledPreEvent,
    ScreenRenderPostEvent, SettingChangedEvent, SettingSpec, Text, WorldDepth, WorldLine,
};
use scan::{Scanner, Surface};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use store::REGION_SHIFT;
use ui::{Action, Back, Editor, MenuItem, Page, Request, Ui, MENU_HEAD, MENU_ROW};
use view::View;
use waypoints::{Kind, Waypoint};
use world::World;

const MINIMAP_TEX: &str = "frontier:dynamic/minimap";
const WORLD_TEX: &str = "frontier:dynamic/world_map";

/// Set by the settings key, taken by the game half, which opens the settings page.
static OPEN_SETTINGS: AtomicBool = AtomicBool::new(false);

/// Whether the settings page was asked for since the last call.
pub fn take_open_settings() -> bool {
    OPEN_SETTINGS.swap(false, Ordering::Relaxed)
}

#[derive(Default)]
struct Frontier {
    s: Mutex<State>,
}

struct WorldMapTex {
    w: u32,
    h: u32,
    /// The screen size it was made for.
    size: [f32; 2],
    /// What it shows: centre, zoom, size and which view (None the surface, else the cave
    /// layer's number).
    drawn: Option<(f64, f64, usize, [f32; 2], Option<i32>)>,
    /// Whether every region in view was in memory when it was drawn.
    complete: bool,
    at: Instant,
}

#[derive(Default)]
struct State {
    world: Option<World>,
    colors: BlockColors,
    scanner: Scanner,
    ui: Ui,
    mini_tex: bool,
    mini_broken: bool,
    frame: Option<((FrameStyle, Shape, frame::Geo), Vec<u32>, Vec<bool>)>,
    pixels: Vec<u8>,
    /// The minimap picture's view, when it was made, its settings and its cave mix.
    last_view: Option<(View, Instant, Config, f32)>,
    world_tex: Option<WorldMapTex>,
    /// In cave mode now (after the hysteresis of `sense`).
    cave: bool,
    sense: cave::Sense,
    /// The minimap's crossfade between the surface and cave pictures.
    fade: cave::Fade,
    last_frame: Option<Instant>,
    /// The camera this frame (smooth between ticks): x, z and yaw.
    cam: Option<(f64, f64, f32)>,
    player: Option<[f64; 3]>,
    yaw: f32,
    biome: String,
    sky: f32,
    has_sky: bool,
    notice: Option<(String, Instant)>,
    last_death: Option<(String, [i32; 3])>,
    death_known: bool,
    death_armed: bool,
    open: Option<Page>,
    tick: u64,
    faces: Faces,
    /// The minimap texture's side, in texels (it changes with Sharp maps).
    mini_side: usize,
    /// Chat to open with this typed in, once the map has closed (and tries left).
    chat: Option<(String, u8)>,
    /// The cave layer shown on the world map is still being read (redraw it often).
    layer_busy: bool,
}

/// Item icons, by item id: resolved once (per world) and kept.
static ITEM_KEYS: Mutex<Option<std::collections::HashMap<String, Option<ItemKey>>>> =
    Mutex::new(None);

/// Item `id`'s identity for drawing its icon, None for an id the game doesn't know.
fn item_key(host: &dyn ClientHost, id: &str) -> Option<ItemKey> {
    if id.is_empty() {
        return None;
    }
    let mut keys = ITEM_KEYS.lock().unwrap_or_else(|e| e.into_inner());
    let keys = keys.get_or_insert_with(Default::default);
    if let Some(k) = keys.get(id) {
        return *k;
    }
    if keys.len() > 4096 {
        keys.clear();
    }
    let k = Identifier::parse(id).ok().and_then(|i| host.item_key(&i));
    keys.insert(id.to_owned(), k);
    k
}

fn label(key: &str) -> String {
    format!("frontier.setting.{key}")
}
fn toggle(r: &mut dyn Registrar, key: &str, default: bool) {
    let l = label(key);
    let _ = r.register_setting(
        SettingSpec::toggle(key, l.clone(), default).tooltip(format!("{l}.tooltip")),
    );
}
fn range(r: &mut dyn Registrar, key: &str, min: f64, max: f64, step: f64, default: f64) {
    let l = label(key);
    let _ = r.register_setting(
        SettingSpec::range(key, l.clone(), min, max, step, default).tooltip(format!("{l}.tooltip")),
    );
}
fn choice(r: &mut dyn Registrar, key: &str, options: &[&str], default: usize) {
    let l = label(key);
    let _ = r.register_setting(
        SettingSpec::choice(key, l.clone(), options.iter().copied(), default)
            .tooltip(format!("{l}.tooltip")),
    );
}

/// Reads every setting into the copy the rest of the mod uses.
fn sync(host: &dyn ClientHost) {
    let d = Config::default();
    let b = |key: &str, default: bool| host.setting_bool(key).unwrap_or(default);
    let n = |key: &str, default: f32| {
        host.setting_f64(key)
            .filter(|v| v.is_finite())
            .map_or(default, |v| v as f32)
    };
    let c = |key: &str| host.setting_choice(key).unwrap_or(0);
    set_config(Config {
        minimap: b(KEY_MINIMAP, d.minimap),
        shape: host
            .setting_choice(KEY_SHAPE)
            .map_or(d.shape, Shape::from_index),
        corner: Corner::from_index(c(KEY_CORNER)),
        size: n(KEY_SIZE, d.size).clamp(64.0, 192.0),
        zoom: n(KEY_ZOOM, d.zoom as f32).round().clamp(1.0, 5.0) as u32,
        rotate: b(KEY_ROTATE, d.rotate),
        frame: host
            .setting_choice(KEY_FRAME)
            .map_or(d.frame, FrameStyle::from_index),
        compass: b(KEY_COMPASS, d.compass),
        coords: b(KEY_COORDS, d.coords),
        biome: b(KEY_BIOME, d.biome),
        lighting: b(KEY_LIGHTING, d.lighting),
        caves: b(KEY_CAVES, d.caves),
        map_waypoints: b(KEY_MAP_WAYPOINTS, d.map_waypoints),
        beacons: b(KEY_BEACONS, d.beacons),
        labels: b(KEY_LABELS, d.labels),
        label_range: n(KEY_LABEL_RANGE, d.label_range).clamp(0.0, 10_000.0),
        death: b(KEY_DEATH, d.death),
        slopes: b(KEY_SLOPES, d.slopes),
        mobs: b(KEY_MOBS, d.mobs),
        mobs_hostile: b(KEY_MOBS_HOSTILE, d.mobs_hostile),
        mobs_passive: b(KEY_MOBS_PASSIVE, d.mobs_passive),
        mobs_players: b(KEY_MOBS_PLAYERS, d.mobs_players),
        mobs_range: n(KEY_MOBS_RANGE, d.mobs_range).clamp(0.0, 256.0),
        mobs_fade: b(KEY_MOBS_FADE, d.mobs_fade),
        mobs_world_map: b(KEY_MOBS_WORLD_MAP, d.mobs_world_map),
        mobs_villagers: b(KEY_MOBS_VILLAGERS, d.mobs_villagers),
        dot_style: host
            .setting_choice(KEY_DOT_STYLE)
            .map_or(d.dot_style, DotStyle::from_index),
        names: Names::from_index(c(KEY_NAMES)),
        player_names: b(KEY_PLAYER_NAMES, d.player_names),
        items: b(KEY_ITEMS, d.items),
        objects: b(KEY_OBJECTS, d.objects),
        containers: b(KEY_CONTAINERS, d.containers),
        decor: b(KEY_DECOR, d.decor),
        sharp: b(KEY_SHARP, d.sharp),
        hide_with_f3: b(KEY_HIDE_F3, d.hide_with_f3),
        map_compass: b(KEY_MAP_COMPASS, d.map_compass),
        map_menu: b(KEY_MAP_MENU, d.map_menu),
    });
}

fn tr(host: &dyn ClientHost, key: &str) -> String {
    host.translate(key).unwrap_or_else(|| key.to_owned())
}

/// The biome's name as the game shows it: `minecraft:dark_forest` is "Dark Forest".
fn biome_name(host: &dyn ClientHost, id: &str) -> String {
    let (ns, path) = id.split_once(':').unwrap_or(("minecraft", id));
    host.translate(&format!("biome.{ns}.{path}"))
        .unwrap_or_else(|| {
            path.split('_')
                .map(|w| {
                    let mut c = w.chars();
                    c.next().map_or(String::new(), |f| {
                        f.to_uppercase().collect::<String>() + c.as_str()
                    })
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
}

impl State {
    fn notice(&mut self, text: String) {
        self.notice = Some((text, Instant::now()));
    }

    /// Opens the world for the server or world being played and the dimension shown, saving
    /// the one before.
    fn follow_world(&mut self, host: &dyn ClientHost) {
        let disk = world::disk(host);
        let key = world::world_key(host);
        let dim = host.dimension().to_owned();
        let same = match (&self.world, &key) {
            (Some(w), Some(k)) => w.key == *k && w.dimension == dim,
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        let new_world = self.world.as_ref().map(|w| w.key.clone()) != key;
        if let Some(mut w) = self.world.take() {
            w.save(&disk);
        }
        self.scanner.reset();
        self.layer_busy = false;
        *ITEM_KEYS.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.last_view = None;
        self.sense.force(false);
        self.fade.set(false);
        self.cave = false;
        if new_world {
            self.death_known = false;
            self.last_death = None;
        }
        self.world = key.map(|k| World::open(&disk, k, dim));
    }

    fn save(&mut self, host: &dyn ClientHost) {
        let disk = world::disk(host);
        if let Some(w) = self.world.as_mut() {
            w.save(&disk);
        }
    }

    /// The player's spot, rounded down to the block.
    fn block_pos(&self) -> Option<[i32; 3]> {
        self.player.map(|p| {
            [
                p[0].floor() as i32,
                p[1].floor() as i32,
                p[2].floor() as i32,
            ]
        })
    }

    /// The cave layer of the player's feet.
    fn player_layer(&self) -> i32 {
        world::layer_of(self.block_pos().map_or(64, |p| p[1]))
    }

    /// Opens the world map from play: in cave mode straight onto the player's cave layer.
    fn open_world_map(&mut self) {
        let (cave, layer) = (self.cave, self.player_layer());
        self.ui.map.open(cave, layer);
    }

    fn new_waypoint_editor(&self, pos: [i32; 3], back: Back, host: &dyn ClientHost) -> Editor {
        let list = self.world.as_ref().map_or(&[][..], |w| &w.waypoints[..]);
        Editor {
            index: None,
            name: waypoints::next_name(list, &tr(host, "frontier.waypoint.default_name")),
            pos,
            color: waypoints::next_color(list),
            back,
        }
    }

    /// Notices a new death: a death waypoint there, in whichever dimension it was.
    fn check_death(&mut self, host: &dyn ClientHost) {
        let cfg = config();
        let Some(player) = host.local_player() else {
            return;
        };
        let death = player.last_death_location(host).map(|g| {
            (g.dimension().to_string(), {
                let p = g.pos();
                [p.x, p.y, p.z]
            })
        });
        if !self.death_known {
            // What was known on joining is old news.
            self.death_known = true;
            self.last_death = death;
            return;
        }
        if death.is_some() && death != self.last_death {
            self.last_death = death.clone();
            if !cfg.death {
                return;
            }
            let (dim, pos) = death.unwrap_or_default();
            let w = Waypoint {
                name: tr(host, "frontier.waypoint.death"),
                pos,
                color: 0,
                shown: true,
                kind: Kind::Death,
            };
            let disk = world::disk(host);
            if let Some(world) = self.world.as_mut() {
                if world.dimension == dim {
                    world.waypoints.retain(|x| x.kind != Kind::Death);
                    world.waypoints.push(w);
                    world.waypoints_dirty = true;
                } else {
                    world::add_elsewhere(&disk, &world.key, &dim, w, true);
                }
            }
            self.death_armed = false;
        }
        // Reaching the death spot clears it (once you have been away from it).
        if let (Some(p), Some(world)) = (self.player, self.world.as_mut()) {
            if let Some(i) = world.waypoints.iter().position(|x| x.kind == Kind::Death) {
                let d = waypoints::distance(p, world.waypoints[i].pos);
                if d > 12.0 {
                    self.death_armed = true;
                } else if self.death_armed && d < 4.0 {
                    world.waypoints.remove(i);
                    world.waypoints_dirty = true;
                    self.death_armed = false;
                }
            }
        }
    }

    fn act(&mut self, host: &dyn ClientHost, action: Action) {
        match action {
            Action::OpenMap => {
                self.open_world_map();
                self.ui.show(Page::Map);
            }
            Action::OpenList => self.ui.show(Page::List),
            Action::NewWaypoint => {
                let back = if matches!(self.ui.page, Page::Map) {
                    Back::Map
                } else {
                    Back::List
                };
                let pos = self.block_pos().unwrap_or([0; 3]);
                let e = self.new_waypoint_editor(pos, back, host);
                self.ui.show(Page::Edit(e));
            }
            Action::Settings => self.ui.requests.push(Request::Settings),
            Action::Done => self.ui.requests.push(Request::Close),
            Action::Toggle(i) => {
                if let Some(w) = self.world.as_mut() {
                    if let Some(wp) = w.waypoints.get_mut(i) {
                        wp.shown = !wp.shown;
                        w.waypoints_dirty = true;
                    }
                }
                self.ui.show(Page::List);
            }
            Action::Edit(i) => self.edit_waypoint(i, Back::List),
            Action::Delete(i) => self.ui.show(Page::Confirm(i, Back::List)),
            Action::ConfirmDelete(i, back) => {
                let mut removed = None;
                if let Some(w) = self.world.as_mut() {
                    if i < w.waypoints.len() {
                        removed = Some(w.waypoints.remove(i).name);
                        w.waypoints_dirty = true;
                    }
                }
                if let Some(name) = removed {
                    self.notice(tr(host, "frontier.notice.removed").replace("%s", &name));
                }
                self.leave_editor(back);
            }
            Action::CancelDelete(back) => self.leave_editor(back),
            Action::CycleColor => {
                let values = self.ui.editor_values();
                if let Page::Edit(e) = &mut self.ui.page {
                    if let Some((name, pos)) = values {
                        e.name = name;
                        e.pos = pos;
                    }
                    e.color = (e.color + 1) % waypoints::COLORS.len();
                }
                let page = self.ui.page.clone();
                self.ui.show(page);
            }
            Action::Save => {
                let values = self.ui.editor_values();
                if let (Page::Edit(e), Some((name, pos))) = (self.ui.page.clone(), values) {
                    let back = e.back;
                    if let Some(w) = self.world.as_mut() {
                        let name = if name.trim().is_empty() {
                            e.name.clone()
                        } else {
                            name
                        };
                        match e.index.and_then(|i| w.waypoints.get_mut(i)) {
                            Some(wp) => {
                                wp.name = name.clone();
                                wp.pos = pos;
                                wp.color = e.color;
                            }
                            None => w.waypoints.push(Waypoint {
                                name: name.clone(),
                                pos,
                                color: e.color,
                                shown: true,
                                kind: Kind::Normal,
                            }),
                        }
                        w.waypoints_dirty = true;
                        if e.index.is_none() {
                            self.notice(tr(host, "frontier.notice.added").replace("%s", &name));
                        }
                    }
                    self.leave_editor(back);
                }
            }
            Action::Cancel => {
                let back = if let Page::Edit(e) = &self.ui.page {
                    e.back
                } else {
                    Back::List
                };
                self.leave_editor(back);
            }
            Action::Centre => {
                self.ui.map.follow = true;
                if let Some(p) = self.player {
                    self.ui.map.cx = p[0];
                    self.ui.map.cz = p[2];
                }
            }
            Action::PageUp => {
                let rows = self.ui.list_rows();
                self.ui.list_scroll = self.ui.list_scroll.saturating_sub(rows);
                self.ui.show(Page::List);
            }
            Action::PageDown => {
                let rows = self.ui.list_rows();
                self.ui.list_scroll += rows;
                self.ui.show(Page::List);
            }
        }
    }

    fn edit_waypoint(&mut self, i: usize, back: Back) {
        if let Some(wp) = self.world.as_ref().and_then(|w| w.waypoints.get(i)) {
            let e = Editor {
                index: Some(i),
                name: wp.name.clone(),
                pos: wp.pos,
                color: wp.color,
                back,
            };
            self.ui.show(Page::Edit(e));
        }
    }

    /// The ground's height at `(x, z)` on the map, plus one: where a waypoint there stands.
    fn ground_at(&self, x: i32, z: i32) -> i32 {
        self.world
            .as_ref()
            .map(|wd| wd.column(self.ui.map.view(), x, z))
            .filter(|c| c.known())
            .map(|c| i32::from(c.height) + 1)
            .or(self.block_pos().map(|p| p[1]))
            .unwrap_or(64)
    }

    /// Opens the world map's right-click menu at `at`: about the waypoint there, or the spot.
    fn open_menu(&mut self, host: &dyn ClientHost, at: [f32; 2]) {
        let [w, h] = self.ui.size;
        let m = &self.ui.map;
        let waypoint = self.world.as_ref().and_then(|world| {
            world
                .waypoints
                .iter()
                .enumerate()
                .filter(|(_, wp)| wp.shown)
                .map(|(i, wp)| {
                    let (sx, sy) =
                        m.screen_at(f64::from(wp.pos[0]) + 0.5, f64::from(wp.pos[2]) + 0.5, w, h);
                    (i, (at[0] - sx).abs().max((at[1] - sy).abs()))
                })
                .filter(|(_, d)| *d < 6.0)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        });
        let can_teleport = host
            .local_player()
            .and_then(|p| p.permission_level(host))
            .is_some_and(|l| l >= 2);
        let (pos, mut items) = match waypoint {
            Some(i) => {
                let pos = self.world.as_ref().map_or([0; 3], |wd| wd.waypoints[i].pos);
                (
                    pos,
                    vec![MenuItem::EditWaypoint(i), MenuItem::RemoveWaypoint(i)],
                )
            }
            None => {
                let (wx, wz) = m.world_at(at[0], at[1], w, h);
                let (bx, bz) = (wx.floor() as i32, wz.floor() as i32);
                (
                    [bx, self.ground_at(bx, bz), bz],
                    vec![MenuItem::NewWaypointHere],
                )
            }
        };
        if can_teleport {
            items.push(MenuItem::TeleportTo);
        }
        items.extend([
            MenuItem::CopyCoords,
            MenuItem::ShareInChat,
            MenuItem::OpenSettings,
        ]);
        self.ui.menu = Some(ui::MapMenu {
            at,
            pos,
            waypoint,
            items,
        });
    }

    /// Does a choice of the right-click menu (inside the click, so copying may use the
    /// clipboard).
    fn menu_choice(&mut self, host: &dyn ClientHost, item: MenuItem, pos: [i32; 3]) {
        self.ui.menu = None;
        let coords = format!("{} {} {}", pos[0], pos[1], pos[2]);
        match item {
            MenuItem::EditWaypoint(i) => self.edit_waypoint(i, Back::Map),
            MenuItem::RemoveWaypoint(i) => self.ui.show(Page::Confirm(i, Back::Map)),
            MenuItem::TeleportTo => self.ui.requests.push(Request::Teleport(pos)),
            MenuItem::NewWaypointHere => {
                let e = self.new_waypoint_editor(pos, Back::Map, host);
                self.ui.show(Page::Edit(e));
            }
            MenuItem::CopyCoords => {
                let msg = if host.set_clipboard_text(&coords) {
                    "frontier.notice.copied"
                } else {
                    "frontier.notice.copy_failed"
                };
                self.notice(tr(host, msg).replace("%s", &coords));
            }
            MenuItem::ShareInChat => {
                let dim = short_dim(&self.world);
                let text = tr(host, "frontier.share.text")
                    .replace("%1$s", &coords)
                    .replace("%2$s", &dim);
                self.ui.requests.push(Request::Chat(text));
            }
            MenuItem::OpenSettings => self.ui.requests.push(Request::Settings),
        }
    }

    fn leave_editor(&mut self, back: Back) {
        match back {
            Back::Game => self.ui.requests.push(Request::Close),
            Back::List => self.ui.show(Page::List),
            Back::Map => self.ui.show(Page::Map),
        }
    }

    fn frame(&mut self, cfg: &Config) -> (&[u32], &[bool]) {
        let g = frame::geo(cfg.sharp);
        let key = (cfg.frame, cfg.shape, g);
        if self.frame.as_ref().map(|f| f.0) != Some(key) {
            self.frame = Some((
                key,
                frame::build(g, cfg.frame, cfg.shape),
                view::mask(g, cfg.shape),
            ));
        }
        let f = self
            .frame
            .as_ref()
            .map(|f| (&f.1[..], &f.2[..]))
            .unwrap_or((&[], &[]));
        f
    }
}

/// A map pixel for column `(x, z)` with the light given.
fn map_pixel(map: &mut store::Sampler<'_>, x: i32, z: i32, step: i32, light: shade::Light) -> u32 {
    let c = map.column(x, z);
    if !c.known() {
        return shade::UNKNOWN;
    }
    let mut h = |dx: i32, dz: i32| {
        let n = map.column(x + dx, z + dz);
        if n.known() {
            n.height
        } else {
            c.height
        }
    };
    let (w, e, n, s) = (h(-step, 0), h(step, 0), h(0, -step), h(0, step));
    shade::pixel(c, w, e, n, s, step as f32, light)
}

/// Whether dots of `kind` are on.
fn kind_shown(cfg: &Config, kind: radar::Kind) -> bool {
    use radar::Kind as K;
    match kind {
        K::Hostile => cfg.mobs && cfg.mobs_hostile,
        K::Passive => cfg.mobs && cfg.mobs_passive,
        K::Villager => cfg.mobs && cfg.mobs_villagers,
        K::Player => cfg.mobs && cfg.mobs_players,
        K::Object => cfg.objects,
        K::Item => cfg.items,
        K::Container => cfg.containers,
        K::Decor => cfg.decor,
    }
}

/// The vertical filter's view: the world map's surface or cave layer, or the minimap's
/// cave or surface picture (the target of the fade, so dots don't flicker mid-crossfade).
fn layer_view(st: &State, world_map: bool) -> radar::LayerView {
    if world_map {
        match st.ui.map.view() {
            None => radar::LayerView::WorldSurface,
            Some(layer) => {
                let (lo, hi) = world::layer_range(layer);
                radar::LayerView::WorldCave {
                    layer,
                    lo,
                    hi,
                    player_layer: st.player_layer(),
                }
            }
        }
    } else if st.cave {
        radar::LayerView::MiniCave
    } else {
        radar::LayerView::MiniSurface
    }
}

/// The scanned column at a dot's (x, z) for the vertical filter: the world map's viewed
/// layer, or the surface (the minimap's cave picture has no per-column height of its own).
fn layer_column(st: &State, d: &radar::Dot, world_map: bool) -> Option<(i32, bool)> {
    let world = st.world.as_ref()?;
    let (x, z) = (d.pos[0].floor() as i32, d.pos[2].floor() as i32);
    let view = if world_map { st.ui.map.view() } else { None };
    let col = world.column(view, x, z);
    match scan::cave_floor(col) {
        Some(h) => Some((h, true)),
        None if col.known() => Some((i32::from(col.height), false)),
        None => None,
    }
}

/// Whether this dot belongs on the map being drawn: other players always, everything else
/// only within about 5 blocks of the layer in view (see [`radar::on_layer`]).
fn dot_on_layer(st: &State, d: &radar::Dot, world_map: bool) -> bool {
    let Some(p) = st.player else { return true };
    radar::on_layer(
        d.kind,
        d.pos[1],
        p[1],
        layer_view(st, world_map),
        layer_column(st, d, world_map),
    )
}

/// Whether faces, and names beside the dots without one, show now (the world map also names
/// the dot under the pointer).
fn names_now(cfg: &Config) -> bool {
    match cfg.names {
        Names::HoldTab => radar::tab_held(),
        Names::Always => true,
        Names::Never => false,
    }
}

/// A dot's name: a dropped item's stack name (custom or the item's own, with the count);
/// the player's or the creature's own; else its type's (or block's) name.
fn dot_label(host: &dyn ClientHost, d: &radar::Dot) -> String {
    if d.kind == radar::Kind::Item {
        return radar::item_label(if d.named { &d.label } else { "" }, &d.item, d.count, |k| {
            host.translate(k)
        });
    }
    if d.named {
        return d.label.clone();
    }
    let key = if d.kind.is_block() {
        radar::block_key(&d.label)
    } else {
        radar::type_key(&d.label)
    };
    host.translate(&key).unwrap_or_else(|| {
        d.label
            .rsplit(':')
            .next()
            .unwrap_or(&d.label)
            .replace('_', " ")
    })
}

/// A creature's dot: a small square with a dark edge, faded by `alpha`; `dy` blocks above
/// (or below) beyond 4 adds a tick on top (or underneath).
fn creature_dot(
    draw: &mut dyn DrawContext,
    x: f32,
    y: f32,
    kind: radar::Kind,
    alpha: f32,
    dy: f64,
) {
    let edge = radar::fade(0xD0_101010, alpha);
    let fill = radar::fade(kind.argb(), alpha);
    draw.fill([x - 2.0, y - 2.0, 4.0, 4.0], edge);
    draw.fill([x - 1.0, y - 1.0, 2.0, 2.0], fill);
    if dy > 4.0 {
        draw.fill([x - 1.0, y - 4.0, 2.0, 1.0], fill);
    } else if dy < -4.0 {
        draw.fill([x - 1.0, y + 3.0, 2.0, 1.0], fill);
    }
}

/// A marked block: a hollow square in its colour, so it doesn't read as a creature.
fn block_marker(draw: &mut dyn DrawContext, x: f32, y: f32, kind: radar::Kind, alpha: f32) {
    let edge = radar::fade(0xD0_101010, alpha);
    let c = radar::fade(kind.argb(), alpha);
    draw.fill([x - 2.5, y - 2.5, 5.0, 5.0], edge);
    draw.fill([x - 1.5, y - 1.5, 3.0, 3.0], c);
    draw.fill([x - 0.5, y - 0.5, 1.0, 1.0], edge);
}

/// A thing that isn't a creature: a small diamond in its colour.
fn object_marker(draw: &mut dyn DrawContext, x: f32, y: f32, kind: radar::Kind, alpha: f32) {
    let edge = radar::fade(0xD0_101010, alpha);
    let c = radar::fade(kind.argb(), alpha);
    draw.quad(
        [[x, y - 3.0], [x + 3.0, y], [x, y + 3.0], [x - 3.0, y]],
        edge,
    );
    draw.quad([[x, y - 2.0], [x + 2.0, y], [x, y + 2.0], [x - 2.0, y]], c);
}

/// A dropped item: a crisp black dot on a light rim, with a faint dark halo, so it reads on
/// dark rock as on snow.
fn item_marker(draw: &mut dyn DrawContext, x: f32, y: f32, alpha: f32) {
    let (x, y) = (x.round(), y.round());
    draw.fill(
        [x - 2.0, y - 2.0, 4.0, 4.0],
        radar::fade(0x70_000000, alpha),
    );
    draw.fill(
        [x - 1.5, y - 1.5, 3.0, 3.0],
        radar::fade(0xE6_D8D8D8, alpha),
    );
    draw.fill(
        [x - 1.0, y - 1.0, 2.0, 2.0],
        radar::fade(radar::Kind::Item.argb(), alpha),
    );
}

/// A dropped item's icon, `size` GUI pixels square centred on `(x, y)`, drawn by the game
/// as it draws one in a slot (the resource packs' model and texture), on a small dark card so
/// it reads over any ground; false (nothing drawn) for an item the game doesn't know.
fn item_icon(
    draw: &mut dyn DrawContext,
    key: ItemKey,
    x: f32,
    y: f32,
    size: f32,
    alpha: f32,
) -> bool {
    let (x0, y0) = ((x - size / 2.0).round(), (y - size / 2.0).round());
    draw.fill(
        [x0 - 1.0, y0 - 1.0, size + 2.0, size + 2.0],
        radar::fade(0xB0_0F1118, alpha),
    );
    draw.item(key, [x0, y0], size)
}

/// How big a dropped item's icon is drawn: on the minimap, and on the world map.
pub const ITEM_ICON_MINI: f32 = 10.0;
pub const ITEM_ICON_WORLD: f32 = 12.0;

/// Whether a dropped item shows as its icon rather than a dot: while names are revealed
/// (Tab held, or names always on), for an item the game knows, not faded far out of reach.
pub fn item_icon_shown(kind: radar::Kind, reveal: bool, known: bool, alpha: f32) -> bool {
    kind == radar::Kind::Item && reveal && known && alpha >= 0.3
}

/// Draws one dot, face or marker at `(x, y)`; answers its half size, for its label, and
/// whether a face (or an item's icon) was drawn. Faces and item icons show only while
/// `reveal` (as names do); `icon` is an item icon's size.
#[allow(clippy::too_many_arguments)]
fn draw_dot(
    draw: &mut dyn DrawContext,
    host: &dyn ClientHost,
    icon: f32,
    faces: &Faces,
    cfg: &Config,
    d: &radar::Dot,
    x: f32,
    y: f32,
    alpha: f32,
    dy: f64,
    reveal: bool,
) -> (f32, bool) {
    use radar::Kind as K;
    match d.kind {
        K::Container | K::Decor => {
            block_marker(draw, x, y, d.kind, alpha);
            (3.0, false)
        }
        K::Object => {
            object_marker(draw, x, y, d.kind, alpha);
            (3.0, false)
        }
        K::Item => {
            let key = if reveal {
                item_key(host, &d.item)
            } else {
                None
            };
            if item_icon_shown(d.kind, reveal, key.is_some(), alpha) {
                if let Some(key) = key {
                    if item_icon(draw, key, x, y, icon, alpha) {
                        return (icon / 2.0 + 1.0, true);
                    }
                }
            }
            item_marker(draw, x, y, alpha);
            (2.0, false)
        }
        K::Hostile | K::Passive | K::Villager | K::Player => {
            let faced = reveal && cfg.dot_style == DotStyle::Faces;
            let (x, y) = (x.round(), y.round());
            let drawn = faced
                && match &d.face {
                    radar::Face::Player { name } if valid_player_name(name) => {
                        face_rim(draw, x, y, d.kind, alpha);
                        player_head(draw, name, x, y, alpha)
                    }
                    radar::Face::Model { .. } => match faces.slot(&d.face) {
                        Some([u, v]) => {
                            // The face on a rim of its kind's colour, so a cow's face still
                            // says "passive" at a glance.
                            face_rim(draw, x, y, d.kind, alpha);
                            let tint = radar::fade(0xFF_FFFFFF, alpha);
                            draw.sprite(
                                FACES_TEX,
                                [u, v, 8.0, 8.0],
                                [x - 4.0, y - 4.0, 8.0, 8.0],
                                Some(tint),
                            );
                            true
                        }
                        None => false,
                    },
                    _ => false,
                };
            if drawn {
                if dy > 4.0 {
                    draw.fill(
                        [x - 1.0, y - 7.0, 2.0, 1.5],
                        radar::fade(d.kind.argb(), alpha),
                    );
                } else if dy < -4.0 {
                    draw.fill(
                        [x - 1.0, y + 5.5, 2.0, 1.5],
                        radar::fade(d.kind.argb(), alpha),
                    );
                }
                (5.0, true)
            } else {
                creature_dot(draw, x, y, d.kind, alpha, dy);
                (2.0, false)
            }
        }
    }
}

/// The dark-edged rim of a face, in the kind's colour.
fn face_rim(draw: &mut dyn DrawContext, x: f32, y: f32, kind: radar::Kind, alpha: f32) {
    draw.fill(
        [x - 5.0, y - 5.0, 10.0, 10.0],
        radar::fade(0xE0_101010, alpha),
    );
    draw.fill(
        [x - 4.5, y - 4.5, 9.0, 9.0],
        radar::fade(kind.argb(), alpha),
    );
}

/// A name the game takes for a player head (`ExtraCodecs.PLAYER_NAME`).
fn valid_player_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 16 && name.bytes().all(|b| (0x21..=0x7E).contains(&b))
}

/// Player `name`'s head centred on `(x, y)`, drawn by the game as it draws a head in chat
/// (the face of their skin with its hat layer): the same source and look as chat heads.
fn player_head(draw: &mut dyn DrawContext, name: &str, x: f32, y: f32, alpha: f32) -> bool {
    use mod_api::client::hud_layout::PoseOp;
    use mod_api::text::{Text, TextContent, TextObject, TextProfile};
    let head = Text::new(TextContent::Object(TextObject::Player {
        profile: {
            let mut p = TextProfile::default();
            p.name = Some(name.to_owned());
            p
        },
        hat: true,
        fallback: None,
    }));
    draw.gui_paint(&[
        mod_api::GuiDraw::Push(vec![PoseOp::Translate(x - 4.0, y - 4.0)]),
        mod_api::GuiDraw::Text {
            text: head,
            x: 0,
            y: 0,
            color: radar::fade(0xFF_FFFFFF, alpha),
            shadow: false,
        },
        mod_api::GuiDraw::Pop,
    ])
    .is_ok()
}

/// Whether a dot is named beside it while names show: anything without a face, players
/// (with their head) when the setting says so, and dropped items (beside their icon).
fn named_now(cfg: &Config, d: &radar::Dot, faced: bool) -> bool {
    !faced || (d.kind == radar::Kind::Player && cfg.player_names) || d.kind == radar::Kind::Item
}

/// A dot's name beside it, on a faint backing so it reads over any ground.
fn dot_name(
    draw: &mut dyn DrawContext,
    host: &dyn ClientHost,
    d: &radar::Dot,
    x: f32,
    y: f32,
    half: f32,
    alpha: f32,
) {
    let label = dot_label(host, d);
    let tw = draw.text_width(&label);
    let (lx, ly) = ((x + half + 2.0).round(), (y - 4.0).round());
    draw.fill(
        [lx - 1.0, ly - 1.0, tw + 2.0, 10.0],
        radar::fade(0x80_000000, alpha),
    );
    draw.text(
        &label,
        [lx, ly],
        radar::fade(d.kind.text_argb(), alpha),
        true,
    );
}

/// Draws a waypoint marker: a diamond in its colour with a dark rim.
fn diamond(draw: &mut dyn DrawContext, x: f32, y: f32, r: f32, argb: u32) {
    let rr = r + 1.0;
    draw.quad(
        [[x, y - rr], [x + rr, y], [x, y + rr], [x - rr, y]],
        0xFF_141414,
    );
    draw.quad([[x, y - r], [x + r, y], [x, y + r], [x - r, y]], argb);
}

/// The player's arrow at `(x, y)` pointing along `angle` (radians, 0 up, clockwise).
fn arrow(draw: &mut dyn DrawContext, x: f32, y: f32, angle: f32, size: f32) {
    let (s, c) = angle.sin_cos();
    let pt = |u: f32, v: f32| [x + u * c - v * s, y + u * s + v * c];
    let shape = |k: f32| {
        [
            pt(0.0, -5.0 * k),
            pt(3.6 * k, 4.0 * k),
            pt(0.0, 1.8 * k),
            pt(-3.6 * k, 4.0 * k),
        ]
    };
    draw.quad(shape(size * 1.35), 0xFF_101010);
    draw.quad(shape(size), 0xFF_FFFFFF);
    let tip = [
        pt(0.0, -5.0 * size),
        pt(1.8 * size, -0.5 * size),
        pt(0.0, 0.6 * size),
        pt(-1.8 * size, -0.5 * size),
    ];
    draw.quad(tip, 0xFF_E04030);
}

/// The world map's compass: a dark disc with a ring, a needle (north half red) and the four
/// letters, fixed with north up as the map is.
fn compass(draw: &mut dyn DrawContext, cx: f32, cy: f32) {
    let r = 17.0f32;
    // The disc and ring, as a fan of thin quads.
    let ring = |draw: &mut dyn DrawContext, radius: f32, argb: u32| {
        let steps = 24;
        for i in 0..steps {
            let a0 = i as f32 / steps as f32 * std::f32::consts::TAU;
            let a1 = (i + 1) as f32 / steps as f32 * std::f32::consts::TAU;
            draw.quad(
                [
                    [cx, cy],
                    [cx + radius * a0.sin(), cy - radius * a0.cos()],
                    [cx + radius * a1.sin(), cy - radius * a1.cos()],
                    [cx, cy],
                ],
                argb,
            );
        }
    };
    ring(draw, r + 1.0, 0xE0_D8CCA8);
    ring(draw, r, 0xD0_1A1814);
    // Ticks between the letters.
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::FRAC_PI_4;
        if i % 2 == 1 {
            let (s, c) = a.sin_cos();
            draw.fill(
                [cx + (r - 4.0) * s - 0.5, cy - (r - 4.0) * c - 0.5, 1.0, 1.0],
                0xFF_8A8070,
            );
        }
    }
    // The needle: north red, south pale.
    draw.quad(
        [[cx, cy - 9.0], [cx + 2.5, cy], [cx, cy], [cx - 2.5, cy]],
        0xFF_E04838,
    );
    draw.quad(
        [[cx, cy + 9.0], [cx - 2.5, cy], [cx, cy], [cx + 2.5, cy]],
        0xFF_E8E0C8,
    );
    draw.fill([cx - 0.5, cy - 0.5, 1.0, 1.0], 0xFF_101010);
    for (letter, dx, dy, c) in [
        ("N", 0.0, -(r - 1.0), 0xFF_FF6B5A),
        ("E", r - 3.0, 0.0, 0xFF_E8E0C8),
        ("S", 0.0, r - 1.0, 0xFF_E8E0C8),
        ("W", -(r - 3.0), 0.0, 0xFF_E8E0C8),
    ] {
        let tw = draw.text_width(letter);
        draw.text(
            letter,
            [(cx + dx - tw / 2.0 + 0.5).round(), (cy + dy - 3.5).round()],
            c,
            true,
        );
    }
}

impl Frontier {
    fn hud(&self, host: &dyn ClientHost, event: &mut RenderGuiPostEvent<'_>) {
        let cfg = config();
        let st = self.s.lock().unwrap_or_else(|e| e.into_inner());
        if !cfg.minimap || !st.mini_tex || st.world.is_none() || host.mod_screen() == Some(ID) {
            return;
        }
        // Out of the F3 screen's way.
        if cfg.hide_with_f3 && radar::debug_shown() {
            return;
        }
        let Some(p) = st.player else { return };
        let g = frame::geo(cfg.sharp);
        let k = cfg.size / g.map as f32;
        let pic = g.side() as f32 * k;
        let mut lines: Vec<(String, u32)> = Vec::new();
        if cfg.coords {
            lines.push((
                format!("{}, {}, {}", p[0].floor(), p[1].floor(), p[2].floor()),
                0xFF_FFFFFF,
            ));
        }
        if cfg.biome && !st.biome.is_empty() {
            lines.push((st.biome.clone(), 0xFF_D8D0B8));
        }
        if let Some((n, at)) = &st.notice {
            if at.elapsed() < Duration::from_millis(2500) {
                lines.push((n.clone(), 0xFF_9FE08A));
            }
        }
        let text_h = lines.len() as f32 * 10.0;
        let corner = match cfg.corner {
            Corner::TopRight => HudCorner::TopRight,
            Corner::TopLeft => HudCorner::TopLeft,
            Corner::BottomRight => HudCorner::BottomRight,
            Corner::BottomLeft => HudCorner::BottomLeft,
        };
        let (x, y) = event.corners.place(corner, pic, pic + text_h);
        let (x, y) = (x.round(), y.round());
        let draw = &mut *event.draw;
        // With text above the map in the bottom corners, below it in the top ones.
        let (map_y, text_y) = if cfg.corner.bottom() {
            (y + text_h, y)
        } else {
            (y, y + pic)
        };
        let t = st.mini_side as f32;
        draw.sprite(MINIMAP_TEX, [0.0, 0.0, t, t], [x, map_y, pic, pic], None);
        let (cx, cy) = (x + pic / 2.0, map_y + pic / 2.0);
        // Dots, waypoints, the arrow and the compass are placed by the very view the picture
        // under them was made for (the camera's smooth place that frame), so they move with the
        // map instead of jumping once a tick.
        let v = match &st.last_view {
            Some((v, ..)) if v.rotate == cfg.rotate && v.bpp == cfg.blocks_per_pixel() => *v,
            _ => View {
                cx: p[0],
                cz: p[2],
                yaw: st.yaw,
                rotate: cfg.rotate,
                bpp: cfg.blocks_per_pixel(),
            },
        };
        // Marked blocks and things first, creatures over them, waypoints over those, then
        // the player over all.
        let names = names_now(&cfg);
        let mut dots = radar::dots();
        dots.sort_by_key(|d| u8::from(d.kind.is_creature()));
        for d in &dots {
            if !kind_shown(&cfg, d.kind) || !dot_on_layer(&st, d, false) {
                continue;
            }
            let (du, dv) = v.map_of(d.pos[0], d.pos[2]);
            // Only where the whole icon is inside the outline (a circle when round), so none
            // spills over the frame: a face and its rim are 5 GUI pixels across from the centre.
            let half = if d.kind.is_creature() && cfg.dot_style == DotStyle::Faces {
                5.0
            } else if d.kind == radar::Kind::Item && names {
                ITEM_ICON_MINI / 2.0 + 1.0
            } else {
                3.0
            };
            let (_, _, edge) = view::clamp_to_map(g, cfg.shape, du, dv, (half / k).max(2.0));
            if edge {
                continue;
            }
            let dy = d.pos[1] - p[1];
            let a = if cfg.mobs_fade {
                radar::height_alpha(dy)
            } else {
                1.0
            };
            let (sx, sy) = (cx + du * k, cy + dv * k);
            let dy_tick = if cfg.mobs_fade { dy } else { 0.0 };
            let (half, faced) = draw_dot(
                draw,
                host,
                ITEM_ICON_MINI,
                &st.faces,
                &cfg,
                d,
                sx,
                sy,
                a,
                dy_tick,
                names,
            );
            if names && named_now(&cfg, d, faced) {
                dot_name(draw, host, d, sx, sy, half, a);
            }
        }
        if cfg.map_waypoints {
            if let Some(w) = &st.world {
                for wp in w.waypoints.iter().filter(|w| w.shown) {
                    let (du, dv) = v.map_of(f64::from(wp.pos[0]) + 0.5, f64::from(wp.pos[2]) + 0.5);
                    let (du, dv, edge) = view::clamp_to_map(g, cfg.shape, du, dv, 3.0);
                    let r = if edge { 2.0 } else { 3.0 };
                    diamond(draw, cx + du * k, cy + dv * k, r, wp.argb());
                }
            }
        }
        let angle = if cfg.rotate {
            0.0
        } else {
            (v.yaw + 180.0).to_radians()
        };
        arrow(draw, cx, cy, angle, (k * 0.9).clamp(0.7, 1.3));
        if cfg.compass {
            let (nu, nv) = v.north();
            for (i, letter) in ["N", "E", "S", "W"].iter().enumerate() {
                // Each letter a quarter turn clockwise from the last.
                let a = i as f32 * std::f32::consts::FRAC_PI_2;
                let (du, dv) = (nu * a.cos() - nv * a.sin(), nu * a.sin() + nv * a.cos());
                let (du, dv, _) = view::clamp_to_map(g, cfg.shape, du * 400.0, dv * 400.0, 5.0);
                let tw = draw.text_width(letter);
                let colour = if i == 0 { 0xFF_FF6B5A } else { 0xFF_FFFFFF };
                draw.text(
                    letter,
                    [
                        (cx + du * k - tw / 2.0 + 0.5).round(),
                        (cy + dv * k - 3.5).round(),
                    ],
                    colour,
                    true,
                );
            }
        }
        for (i, (s, c)) in lines.iter().enumerate() {
            let s = ui::fit(draw, s, pic.max(90.0));
            let tw = draw.text_width(&s);
            draw.text(
                &s,
                [(cx - tw / 2.0).round(), text_y + 1.0 + i as f32 * 10.0],
                *c,
                true,
            );
        }
    }

    /// The world map page: the picture, marked blocks, things, creatures, waypoints, the
    /// player, the cursor's coordinates, the compass and the right-click menu.
    fn draw_world_map(
        &self,
        st: &State,
        host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        mouse: [f32; 2],
    ) {
        let [w, h] = st.ui.size;
        draw.fill([0.0, 0.0, w, h], 0xFF_14120F);
        let m = &st.ui.map;
        if let Some(t) = &st.world_tex {
            // Only a picture of the view shown (none for the frame a switch takes to redraw).
            if let Some((cx, cz, zoom, size, _)) = t.drawn.filter(|d| d.4 == m.view()) {
                // The picture as drawn, moved to where the view is now (it follows a frame
                // behind while dragging), by whole texels so it stays sharp.
                let bpg = f64::from(ui::MAP_ZOOMS[zoom]);
                let tpg = t.w as f32 / w.max(1.0);
                let snap = |v: f32| (v * tpg).round() / tpg;
                let (ox, oy) = (
                    snap(((cx - m.cx) / bpg) as f32),
                    snap(((cz - m.cz) / bpg) as f32),
                );
                if zoom == m.zoom && size == st.ui.size {
                    draw.sprite(
                        WORLD_TEX,
                        [0.0, 0.0, t.w as f32, t.h as f32],
                        [ox, oy, w, h],
                        None,
                    );
                }
            }
        }
        let cfg = config();
        // What the minimap shows, the same toggles.
        if cfg.mobs_world_map {
            let names = names_now(&cfg);
            let mut dots = radar::dots();
            dots.sort_by_key(|d| u8::from(d.kind.is_creature()));
            let mut hover: Option<(f32, usize, f32, f32, f32, bool)> = None;
            for (i, d) in dots.iter().enumerate() {
                if !kind_shown(&cfg, d.kind) || !dot_on_layer(st, d, true) {
                    continue;
                }
                let (sx, sy) = m.screen_at(d.pos[0], d.pos[2], w, h);
                if sx < 0.0 || sy < 0.0 || sx > w || sy > h {
                    continue;
                }
                let (half, faced) = draw_dot(
                    draw,
                    host,
                    ITEM_ICON_WORLD,
                    &st.faces,
                    &cfg,
                    d,
                    sx,
                    sy,
                    1.0,
                    0.0,
                    names,
                );
                let labelled = names && named_now(&cfg, d, faced);
                let dist = (mouse[0] - sx).abs().max((mouse[1] - sy).abs());
                if dist <= half + 1.5 && hover.is_none_or(|hv| dist < hv.0) {
                    hover = Some((dist, i, sx, sy, half, labelled));
                }
                if labelled {
                    dot_name(draw, host, d, sx, sy, half, 1.0);
                }
            }
            // The dot under the pointer is named whatever the setting (unless names are off),
            // a face too.
            if let Some((_, i, sx, sy, half, labelled)) = hover {
                if !labelled && cfg.names != Names::Never && st.ui.menu.is_none() {
                    dot_name(draw, host, &dots[i], sx, sy, half, 1.0);
                }
            }
        }
        if let Some(world) = &st.world {
            for wp in world.waypoints.iter().filter(|w| w.shown) {
                let (sx, sy) =
                    m.screen_at(f64::from(wp.pos[0]) + 0.5, f64::from(wp.pos[2]) + 0.5, w, h);
                if sx < -8.0 || sy < -8.0 || sx > w + 8.0 || sy > h + 8.0 {
                    continue;
                }
                diamond(draw, sx, sy, 4.0, wp.argb());
                let near = (mouse[0] - sx).abs() < 6.0 && (mouse[1] - sy).abs() < 6.0;
                if near || m.zoom <= 3 {
                    let tw = draw.text_width(&wp.name);
                    draw.fill([sx - tw / 2.0 - 2.0, sy + 6.0, tw + 4.0, 10.0], 0x90_000000);
                    draw.text(
                        &wp.name,
                        [(sx - tw / 2.0).round(), (sy + 7.0).round()],
                        0xFF_FFFFFF,
                        false,
                    );
                }
            }
        }
        if let Some(p) = st.player {
            // The camera's smooth place and turn this frame, else the last tick's.
            let (px, pz, yaw) = st.cam.unwrap_or((p[0], p[2], st.yaw));
            let (sx, sy) = m.screen_at(px, pz, w, h);
            arrow(draw, sx, sy, (yaw + 180.0).to_radians(), 1.2);
        }
        // The cursor's spot.
        let (wx, wz) = m.world_at(mouse[0], mouse[1], w, h);
        let (bx, bz) = (wx.floor() as i32, wz.floor() as i32);
        let mut line = format!("X {bx}   Z {bz}");
        if let Some(world) = &st.world {
            let c = world.column(m.view(), bx, bz);
            if c.known() {
                line = format!("X {bx}   Y {}   Z {bz}", c.height);
            }
        }
        let title = format!(
            "{} \u{2022} {}",
            tr(host, "frontier.menu.world_map"),
            short_dim(&st.world)
        );
        let tw = draw.text_width(&title);
        draw.fill([0.0, 0.0, w, 14.0], 0x90_000000);
        draw.text(&title, [((w - tw) / 2.0).round(), 3.0], 0xFF_FFFFFF, true);
        let lw = draw.text_width(&line);
        draw.fill(
            [((w - lw) / 2.0 - 4.0).round(), h - 40.0, lw + 8.0, 12.0],
            0x90_000000,
        );
        draw.text(
            &line,
            [((w - lw) / 2.0).round(), h - 38.0],
            0xFF_E8E0C8,
            false,
        );
        let hint = tr(
            host,
            if cfg.map_menu {
                "frontier.map.hint_menu"
            } else {
                "frontier.map.hint"
            },
        );
        let hw = draw.text_width(&hint);
        draw.text(&hint, [(w - hw - 4.0).round(), 17.0], 0xFF_A09880, false);
        if cfg.map_compass {
            compass(draw, (w - 26.0).round(), (h - 50.0).round());
        }
        draw_layer_controls(draw, host, m.view(), st.layer_busy, w, mouse);
        if let Some(menu) = &st.ui.menu {
            draw_menu(draw, host, menu, st, w, h, mouse);
        }
    }
}

/// Whether the world map's picture, drawn for `drawn`, is to be drawn again for `want` now.
/// A change of view (the surface, or another cave layer) redraws at once, so no pixel of the
/// view before ever shows; a move or zoom at most every 33 ms; the same view every half
/// second (every 33 ms while regions are still loading, every 100 ms while its cave layer is
/// still being read, so what is found shows as it comes).
fn redraw_due(
    drawn: Option<(f64, f64, usize, [f32; 2], Option<i32>)>,
    want: (f64, f64, usize, [f32; 2], Option<i32>),
    complete: bool,
    busy: bool,
    age: Duration,
) -> bool {
    let Some(d) = drawn else { return true };
    if d.4 != want.4 {
        return true;
    }
    if d != want {
        return age >= Duration::from_millis(33);
    }
    let every = if !complete {
        33
    } else if busy {
        100
    } else {
        500
    };
    age > Duration::from_millis(every)
}

/// The world map's layer controls, in the shared dark card: the Surface | Cave switch (the
/// view shown filled with the accent, the other lit under the pointer) and, in a cave view,
/// a layer down and a layer up button with the layer's name (`Cave · Y 48–63`) beside them.
fn draw_layer_controls(
    draw: &mut dyn DrawContext,
    host: &dyn ClientHost,
    view: Option<i32>,
    busy: bool,
    w: f32,
    mouse: [f32; 2],
) {
    let accent = 0xFF00_0000 | (host.setting_color("accent").unwrap_or(0xF9801D) & 0xFF_FFFF);
    let card = |draw: &mut dyn DrawContext, [x, y, sw, sh]: [f32; 4]| {
        draw.fill([x + 1.0, y + 1.0, sw, sh], 0x60_000000);
        draw.fill([x, y, sw, sh], 0xF4_0F1118);
        draw.fill([x, y, sw, 1.0], 0xFF_1E2330);
        draw.fill([x, y + sh - 1.0, sw, 1.0], 0xFF_1E2330);
        draw.fill([x, y, 1.0, sh], 0xFF_1E2330);
        draw.fill([x + sw - 1.0, y, 1.0, sh], 0xFF_1E2330);
    };
    let cave = view.is_some();
    let rect = ui::mode_switch_rect(w);
    let [x, y, _, sh] = rect;
    card(draw, rect);
    let hover = ui::layer_controls_hit(w, cave, mouse[0], mouse[1]);
    for (i, (key, hit)) in [
        ("frontier.map.surface", ui::LayerHit::Surface),
        ("frontier.map.cave", ui::LayerHit::Cave),
    ]
    .into_iter()
    .enumerate()
    {
        let sx = x + i as f32 * ui::MODE_SEG;
        let on = (hit == ui::LayerHit::Cave) == cave;
        if on {
            draw.fill([sx + 1.0, y + 1.0, ui::MODE_SEG - 2.0, sh - 2.0], accent);
        } else if hover == Some(hit) {
            draw.fill(
                [sx + 1.0, y + 1.0, ui::MODE_SEG - 2.0, sh - 2.0],
                0xFF_2A3142,
            );
        }
        let label = ui::fit(draw, &tr(host, key), ui::MODE_SEG - 4.0);
        let tw = draw.text_width(&label);
        let c = if on { 0xFF_0F1118 } else { 0xFF_E8DCC8 };
        draw.text(
            &label,
            [(sx + (ui::MODE_SEG - tw) / 2.0).round(), y + 3.0],
            c,
            false,
        );
    }
    let Some(layer) = view else { return };
    for (up, hit) in [(false, ui::LayerHit::Down), (true, ui::LayerHit::Up)] {
        let r = ui::layer_button_rect(w, up);
        card(draw, r);
        let lit = hover == Some(hit);
        if lit {
            draw.fill(
                [r[0] + 1.0, r[1] + 1.0, r[2] - 2.0, r[3] - 2.0],
                0xFF_2A3142,
            );
        }
        // A small triangle, pointing up or down, in the accent.
        let (cx, cy) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        let c = if lit { 0xFF_FFFFFF } else { accent };
        if up {
            draw.quad(
                [
                    [cx, cy - 3.0],
                    [cx + 4.0, cy + 2.0],
                    [cx - 4.0, cy + 2.0],
                    [cx, cy - 3.0],
                ],
                c,
            );
        } else {
            draw.quad(
                [
                    [cx - 4.0, cy - 2.0],
                    [cx + 4.0, cy - 2.0],
                    [cx, cy + 3.0],
                    [cx, cy + 3.0],
                ],
                c,
            );
        }
    }
    let name = ui::layer_name(
        &tr(host, "frontier.map.surface"),
        &tr(host, "frontier.map.cave"),
        Some(layer),
    );
    let name = if busy {
        format!("{name} \u{2022} {}", tr(host, "frontier.map.reading"))
    } else {
        name
    };
    let [bx, by, bw, bh] = ui::layer_button_rect(w, true);
    let tw = draw.text_width(&name);
    let lr = [bx + bw + 3.0, by, tw + 8.0, bh];
    card(draw, lr);
    draw.fill([lr[0] + 1.0, lr[1] + 1.0, 2.0, lr[3] - 2.0], accent);
    draw.text(&name, [lr[0] + 5.0, by + 3.0], 0xFF_E8DCC8, false);
}

/// The right-click menu: a dark card with a thin accent line, the spot's coordinates (and the
/// waypoint's name) on top, then the choices; the one under the pointer lights up.
fn draw_menu(
    draw: &mut dyn DrawContext,
    host: &dyn ClientHost,
    menu: &ui::MapMenu,
    st: &State,
    w: f32,
    h: f32,
    mouse: [f32; 2],
) {
    // The shared Sunrise dark card: #0F1118 with an accent top edge and charcoal lines.
    let accent = 0xFF00_0000 | (host.setting_color("accent").unwrap_or(0xF9801D) & 0xFF_FFFF);
    let [x, y, mw, mh] = menu.rect(w, h);
    draw.fill([x + 2.0, y + 2.0, mw, mh], 0x60_000000);
    draw.fill([x, y, mw, mh], 0xF4_0F1118);
    draw.fill([x, y, mw, 2.0], accent);
    draw.fill([x, y + mh - 1.0, mw, 1.0], 0xFF_1E2330);
    draw.fill([x, y, 1.0, mh], 0xFF_1E2330);
    draw.fill([x + mw - 1.0, y, 1.0, mh], 0xFF_1E2330);
    let head = match menu
        .waypoint
        .and_then(|i| st.world.as_ref()?.waypoints.get(i))
    {
        Some(wp) => ui::fit(draw, &wp.name, mw - 8.0),
        None => tr(host, "frontier.map_menu.here"),
    };
    draw.text(&head, [x + 5.0, y + 4.0], 0xFF_E8DCC8, true);
    let p = menu.pos;
    let coords = format!("{}  {}  {}", p[0], p[1], p[2]);
    draw.text(&coords, [x + 5.0, y + 14.0], 0xFF_8B93A7, false);
    draw.fill([x + 4.0, y + MENU_HEAD - 2.0, mw - 8.0, 1.0], 0xFF_1E2330);
    let hovered = menu.item_at(w, h, mouse[0], mouse[1]);
    for (i, item) in menu.items.iter().enumerate() {
        let ry = y + MENU_HEAD + i as f32 * MENU_ROW;
        let lit = hovered == Some(*item);
        if lit {
            draw.fill([x + 2.0, ry, mw - 4.0, MENU_ROW], 0xFF_1E2330);
            draw.fill([x + 2.0, ry, 2.0, MENU_ROW], accent);
        }
        let danger = matches!(item, ui::MenuItem::RemoveWaypoint(_));
        let c = match (danger, lit) {
            (true, _) => 0xFF_FF8A7A,
            (false, true) => 0xFF_FFFFFF,
            (false, false) => 0xFF_E8DCC8,
        };
        draw.text(&tr(host, item.label_key()), [x + 8.0, ry + 3.0], c, false);
    }
}

fn short_dim(world: &Option<World>) -> String {
    let d = world.as_ref().map_or("", |w| w.dimension.as_str());
    let p = d.split_once(':').map_or(d, |(_, p)| p);
    p.replace('_', " ")
}

/// Which of the mod's keys this is, by its current binding.
fn bound(host: &dyn ClientHost, name: &str, key: &str) -> bool {
    host.mod_key(ID, name)
        .is_some_and(|(k, _)| k == key && k != "key.keyboard.unknown")
}

#[mod_api::sunrise_mod]
impl Hooks for Frontier {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        for (id, default) in KEYS {
            r.register_key_mapping(
                KeyRegistration::new(id, default)
                    .label(&format!("frontier.key.{id}"))
                    .category("frontier"),
            );
        }
        let d = Config::default();
        toggle(r, KEY_MINIMAP, d.minimap);
        choice(r, KEY_SHAPE, &Shape::OPTIONS, d.shape.index() as usize);
        choice(r, KEY_CORNER, &Corner::OPTIONS, 0);
        range(r, KEY_SIZE, 64.0, 192.0, 8.0, f64::from(d.size));
        range(r, KEY_ZOOM, 1.0, 5.0, 1.0, f64::from(d.zoom));
        toggle(r, KEY_ROTATE, d.rotate);
        choice(r, KEY_FRAME, &FrameStyle::OPTIONS, d.frame.index() as usize);
        toggle(r, KEY_COMPASS, d.compass);
        toggle(r, KEY_COORDS, d.coords);
        toggle(r, KEY_BIOME, d.biome);
        toggle(r, KEY_LIGHTING, d.lighting);
        toggle(r, KEY_SLOPES, d.slopes);
        toggle(r, KEY_CAVES, d.caves);
        toggle(r, KEY_MAP_WAYPOINTS, d.map_waypoints);
        toggle(r, KEY_BEACONS, d.beacons);
        toggle(r, KEY_LABELS, d.labels);
        range(
            r,
            KEY_LABEL_RANGE,
            0.0,
            2000.0,
            50.0,
            f64::from(d.label_range),
        );
        toggle(r, KEY_DEATH, d.death);
        toggle(r, KEY_MOBS, d.mobs);
        toggle(r, KEY_MOBS_HOSTILE, d.mobs_hostile);
        toggle(r, KEY_MOBS_PASSIVE, d.mobs_passive);
        toggle(r, KEY_MOBS_PLAYERS, d.mobs_players);
        range(r, KEY_MOBS_RANGE, 0.0, 256.0, 8.0, f64::from(d.mobs_range));
        toggle(r, KEY_MOBS_FADE, d.mobs_fade);
        toggle(r, KEY_MOBS_VILLAGERS, d.mobs_villagers);
        toggle(r, KEY_MOBS_WORLD_MAP, d.mobs_world_map);
        choice(r, KEY_DOT_STYLE, &DotStyle::OPTIONS, 1);
        choice(r, KEY_NAMES, &Names::OPTIONS, 0);
        toggle(r, KEY_PLAYER_NAMES, d.player_names);
        toggle(r, KEY_ITEMS, d.items);
        toggle(r, KEY_OBJECTS, d.objects);
        toggle(r, KEY_CONTAINERS, d.containers);
        toggle(r, KEY_DECOR, d.decor);
        toggle(r, KEY_SHARP, d.sharp);
        toggle(r, KEY_HIDE_F3, d.hide_with_f3);
        toggle(r, KEY_MAP_COMPASS, d.map_compass);
        toggle(r, KEY_MAP_MENU, d.map_menu);
        // The accent colour of frontier's cards (Sunrise orange by default).
        let _ = r.register_setting(
            mod_api::SettingSpec::color("accent", "frontier.setting.accent", 0xF9801D)
                .tooltip("frontier.setting.accent.tooltip"),
        );
    }

    fn setting_changed(&self, host: &mut dyn ClientHost, event: &mut SettingChangedEvent<'_>) {
        if event.mod_id == ID {
            sync(host);
        }
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        if !host.on(ID) {
            return;
        }
        if !config_loaded() {
            sync(host);
        }
        let mut st = self.s.lock().unwrap_or_else(|e| e.into_inner());
        st.tick += 1;
        // What the screen asked for.
        for req in std::mem::take(&mut st.ui.requests) {
            match req {
                Request::Close => host.mod_screen_close(ID),
                Request::Settings => {
                    host.mod_screen_close(ID);
                    OPEN_SETTINGS.store(true, Ordering::Relaxed);
                }
                Request::Chat(text) => {
                    host.mod_screen_close(ID);
                    st.chat = Some((text, 10));
                }
                Request::Teleport(p) => {
                    host.mod_screen_close(ID);
                    let line = format!("/tp @s {} {} {}", p[0], p[1], p[2]);
                    if let Err(why) = host.send_chat(&line) {
                        let msg = tr(host, "frontier.notice.teleport_failed").replace("%s", &why);
                        st.notice(msg);
                    }
                }
            }
        }
        // Chat with the location typed in, once the map is closed (a frame or two later).
        if let Some((text, tries)) = st.chat.take() {
            if host.mod_screen().is_none() {
                if host.open_chat(&text).is_err() && tries > 0 {
                    st.chat = Some((text, tries - 1));
                }
            } else if tries > 0 {
                st.chat = Some((text, tries - 1));
            }
        }
        if let Some(page) = st.open.take() {
            if host.mod_screen_open(ID) {
                st.ui.show(page);
            }
        }
        st.follow_world(host);
        if st.world.is_none() {
            radar::want(None);
            return;
        }
        let Some(player) = host.local_player() else {
            return;
        };
        let entity = player.entity();
        st.player = entity.position(host);
        if let Some([yaw, _]) = entity.rotation(host) {
            st.yaw = yaw;
        }
        // Dots: the game side makes the list on the next frame. The world map, open, asks
        // for all it can show (as far as the game keeps things loaded round the player).
        {
            let c = config();
            let map_open = c.mobs_world_map
                && host.mod_screen() == Some(ID)
                && matches!(st.ui.page, Page::Map);
            let shown = c.minimap || map_open;
            let creatures =
                c.mobs && (c.mobs_hostile || c.mobs_passive || c.mobs_players || c.mobs_villagers);
            let any = creatures || c.items || c.objects || c.containers || c.decor;
            radar::want(match st.player {
                Some(p) if shown && any => Some(radar::Wanted {
                    centre: p,
                    radius: if map_open { 256.0 } else { c.mobs_radius() },
                    creatures,
                    objects: c.objects,
                    items: c.items,
                    containers: c.containers,
                    decor: c.decor,
                    block_height: if map_open { 64.0 } else { 24.0 },
                    faces: c.dot_style == DotStyle::Faces && creatures,
                }),
                _ => None,
            });
        }
        let Some(pos) = st.block_pos() else { return };
        let mut cfg = config();
        // The keys.
        let click = |host: &mut dyn ClientHost, k: &str| host.consume_click(ID, k);
        if click(host, "world_map") {
            st.open = Some(Page::Map);
            st.open_world_map();
        }
        if click(host, "new_waypoint") {
            let e = st.new_waypoint_editor(pos, Back::Game, host);
            st.open = Some(Page::Edit(e));
        }
        if click(host, "waypoints") {
            st.open = Some(Page::List);
        }
        if click(host, "settings") {
            OPEN_SETTINGS.store(true, Ordering::Relaxed);
        }
        let mut zoom = cfg.zoom;
        while click(host, "zoom_in") {
            zoom = zoom.saturating_sub(1).max(1);
        }
        while click(host, "zoom_out") {
            zoom = (zoom + 1).min(5);
        }
        if zoom != cfg.zoom {
            host.set_setting(ID, KEY_ZOOM, &zoom.to_string());
            cfg.zoom = zoom;
            set_config(cfg);
        }
        if click(host, "toggle_minimap") {
            cfg.minimap = !cfg.minimap;
            host.set_setting(ID, KEY_MINIMAP, if cfg.minimap { "true" } else { "false" });
            set_config(cfg);
        }
        // Reading the world.
        let dim = host.dimension();
        let Some(level) = host.level(dim) else { return };
        let dtype = level.dimension_type(host);
        let (min_y, has_ceiling, has_sky, logical) =
            dtype.as_ref().map_or((-64, false, true, 384), |t| {
                (t.min_y, t.has_ceiling, t.has_skylight, t.logical_height)
            });
        st.has_sky = has_sky;
        st.ui.map.bounds = world::layer_bounds(min_y, logical);
        st.sky = if cfg.lighting && has_sky {
            level.day_time(host).map_or(1.0, shade::sky_light)
        } else {
            1.0
        };
        let surface = if has_ceiling {
            Surface::UnderRoof(min_y + logical - 3)
        } else {
            Surface::Top
        };
        let disk = world::disk(host);
        let State {
            world,
            colors,
            scanner,
            ..
        } = &mut *st;
        let world = world.as_mut().expect("checked above");
        colors.check_version(host);
        // The regions round the player, from disk when explored before.
        for dz in -1..=1 {
            for dx in -1..=1 {
                let _ = world.map.region(
                    &disk,
                    ((pos[0] >> REGION_SHIFT) + dx, (pos[2] >> REGION_SHIFT) + dz),
                );
            }
        }
        let radius = 12;
        scanner.tick(
            host,
            level,
            colors,
            &mut world.map,
            &disk,
            (pos[0] >> 4, pos[2] >> 4),
            radius,
            min_y,
            surface,
            4,
        );
        // Cave mode: under a roof (rock, any block, or glass: a dome counts), steadily for a
        // few ticks each way, or always under the Nether's. Any overhead cover also pre-warms
        // the cave picture, so the fade already has the floor around you when it starts.
        let (cave, warming) = if !cfg.caves {
            st.sense.force(false);
            (false, false)
        } else if has_ceiling {
            st.sense.force(true);
            (true, false)
        } else {
            let share = scan::cover_share(host, level, &mut st.colors, pos[0], pos[2], pos[1]);
            let warming = share.is_some_and(|s| s > 0.0);
            (st.sense.update(share), warming)
        };
        let map_open = host.mod_screen() == Some(ID) && matches!(st.ui.page, Page::Map);
        if cave || warming {
            // The minimap's live picture, and (once cave mode is on) the world map's cave
            // layer of the player's feet with it; then (while the world map isn't open, which
            // reads its own layer each frame) the rest of that layer across every chunk the
            // client holds, so the map opens on the whole base, not a patch.
            let State {
                world,
                colors,
                scanner,
                ..
            } = &mut *st;
            let world = world.as_mut().expect("checked above");
            let layer = world::layer_of(pos[1]);
            let (budget, time) = if cave {
                (900, scan::CAVE_TIME)
            } else {
                (480, scan::CAVE_WARM_TIME)
            };
            if cave {
                let store = world.layer_mut(Some(layer));
                scanner.cave_tick(
                    host,
                    level,
                    colors,
                    pos[0],
                    pos[2],
                    pos[1],
                    budget,
                    time,
                    Some((&mut *store, &disk)),
                );
                if !map_open {
                    let centre = (pos[0] >> 4, pos[2] >> 4);
                    scanner.layer_tick(
                        host,
                        level,
                        colors,
                        store,
                        &disk,
                        layer,
                        pos[1],
                        centre,
                        12,
                        Duration::from_micros(1_500),
                    );
                }
            } else {
                scanner.cave_tick(
                    host, level, colors, pos[0], pos[2], pos[1], budget, time, None,
                );
            }
        }
        // What was read is kept on leaving (it fades out, and is there at once on coming back).
        st.cave = cave;
        if st.tick % 10 == 0 {
            st.biome = level
                .client_biome(host, pos)
                .map(|b| biome_name(host, &b))
                .unwrap_or_default();
        }
        st.check_death(host);
        let keep = st.ui.map.view().or(cave.then(|| world::layer_of(pos[1])));
        if let Some(w) = st.world.as_mut() {
            w.maintain(&disk, keep);
            w.save_waypoints(&disk);
        }
    }

    fn render_frame_pre(&self, host: &mut dyn ClientHost, _event: &mut RenderFramePreEvent) {
        if !host.on(ID) {
            return;
        }
        let cfg = config();
        let mut st = self.s.lock().unwrap_or_else(|e| e.into_inner());
        if st.world.is_none() || st.player.is_none() {
            return;
        }
        // Smooth movement: the camera's place this frame.
        let (px, pz, yaw) = match host.camera() {
            Some(c)
                if host
                    .camera_entity()
                    .is_none_or(|e| host.local_player().is_some_and(|p| p.entity() == e)) =>
            {
                (c.position[0], c.position[2], c.yaw)
            }
            _ => {
                let p = st.player.unwrap_or_default();
                (p[0], p[2], st.yaw)
            }
        };
        st.cam = Some((px, pz, yaw));
        if st.ui.map.follow {
            st.ui.map.cx = px;
            st.ui.map.cz = pz;
        }
        // The cave crossfade, by real frame time.
        let now = Instant::now();
        let dt = st
            .last_frame
            .map_or(Duration::ZERO, |t| now.duration_since(t));
        st.last_frame = Some(now);
        let target = st.cave && cfg.minimap;
        let mix = st.fade.advance(target, dt);
        // The minimap's picture, when anything about it changed.
        let g = frame::geo(cfg.sharp);
        if st.mini_tex && st.mini_side != g.texels() {
            // Sharp maps changed: a texture of the new size.
            let _ = host.free_dynamic_texture(&Identifier::parse(MINIMAP_TEX).expect("valid id"));
            st.mini_tex = false;
            st.last_view = None;
        }
        if cfg.minimap && !st.mini_broken {
            let v = View {
                cx: px,
                cz: pz,
                yaw,
                rotate: cfg.rotate,
                bpp: cfg.blocks_per_pixel(),
            };
            // Drawn again whenever the view moved by a fifth of a block's pixel or turned (at
            // most every 7 ms), every frame of a crossfade, and every 400 ms for what the
            // scanners found.
            let redo = match &st.last_view {
                None => true,
                Some((old, at, oc, old_mix)) => {
                    let moved = ((old.cx - v.cx).abs() + (old.cz - v.cz).abs()) as f32
                        > v.bpp * 0.2
                        || (old.yaw - v.yaw).abs() > 0.25 && cfg.rotate;
                    *oc != cfg
                        || *old_mix != mix
                        || (moved && at.elapsed() >= Duration::from_millis(7))
                        || at.elapsed() > Duration::from_millis(400)
                }
            };
            if redo {
                let light = shade::Light {
                    sky: st.sky,
                    relief: cfg.slopes,
                };
                let step = (cfg.blocks_per_pixel().round() as i32).max(1);
                let mut pixels = std::mem::take(&mut st.pixels);
                {
                    let (frame_px, inside) = {
                        let (f, m) = st.frame(&cfg);
                        (f.to_vec(), m.to_vec())
                    };
                    let State { world, scanner, .. } = &*st;
                    let mut map = world.as_ref().expect("checked").map.sampler();
                    view::compose(
                        &v,
                        g,
                        &inside,
                        &frame_px,
                        |x, z| {
                            let surface = map_pixel(&mut map, x, z, step, light);
                            let cave_px = scanner.cave_pixel(x, z);
                            if mix <= 0.0 {
                                surface
                            } else if mix >= 1.0 {
                                // Fully in cave: an unread column is clear (the scanners fill it in).
                                cave_px.unwrap_or(0)
                            } else {
                                // Mid-fade: both pictures, each over the map's background. A
                                // column not yet read keeps the surface, so a staircase fade
                                // never flashes dark.
                                match cave_px {
                                    None => surface,
                                    Some(below) => cave::lerp_argb(
                                        view::blend(view::MAP_BG, surface),
                                        view::blend(view::MAP_BG, below),
                                        mix,
                                    ),
                                }
                            }
                        },
                        &mut pixels,
                    );
                }
                let id = Identifier::parse(MINIMAP_TEX).expect("valid id");
                let side = g.texels() as u32;
                let ok = if st.mini_tex {
                    host.update_dynamic_texture(&id, side, side, &pixels)
                        .is_ok()
                } else {
                    host.create_dynamic_texture(&id, side, side, &pixels)
                        .or_else(|_| host.update_dynamic_texture(&id, side, side, &pixels))
                        .is_ok()
                };
                st.mini_tex = ok;
                st.mini_side = g.texels();
                if !ok {
                    st.mini_broken = true;
                }
                st.pixels = pixels;
                st.last_view = Some((v, Instant::now(), cfg, mix));
            }
        }
        // The world map's picture while its page is open.
        let open = host.mod_screen() == Some(ID) && matches!(st.ui.page, Page::Map);
        // Faces, while a map that shows them is up.
        let faces_wanted = cfg.dot_style == DotStyle::Faces
            && cfg.mobs
            && cfg.names != Names::Never
            && (cfg.minimap || (open && cfg.mobs_world_map));
        if faces_wanted {
            let dots = radar::dots();
            st.faces.prepare(host, &dots);
        } else {
            st.faces.free(host);
        }
        if !open {
            if st.world_tex.take().is_some() {
                let _ = host.free_dynamic_texture(&Identifier::parse(WORLD_TEX).expect("valid id"));
            }
            return;
        }
        let size = st.ui.size;
        if size[0] < 16.0 || size[1] < 16.0 {
            return;
        }
        // A cave layer shown is read live across every chunk the client holds round the
        // map's centre, a few milliseconds a frame, nearest first: standing in a base (or a
        // dome) and opening the map fills in the whole of it at that layer.
        let player_layer = st.player_layer();
        st.ui.map.track(player_layer);
        st.layer_busy = false;
        if let Some(layer) = st.ui.map.view() {
            let feet = cave::slice_feet(layer, st.block_pos().map(|p| p[1]));
            let centre = (
                (st.ui.map.cx.floor() as i32) >> 4,
                (st.ui.map.cz.floor() as i32) >> 4,
            );
            let dim = host.dimension().to_owned();
            if let Some(level) = host.level(&dim) {
                let disk = world::disk(host);
                let State {
                    world,
                    colors,
                    scanner,
                    layer_busy,
                    ..
                } = &mut *st;
                if let Some(world) = world.as_mut() {
                    let store = world.layer_mut(Some(layer));
                    *layer_busy = scanner.layer_tick(
                        &*host,
                        level,
                        colors,
                        store,
                        &disk,
                        layer,
                        feet,
                        centre,
                        24,
                        Duration::from_micros(4_000),
                    );
                }
            }
        }
        // Texels per GUI pixel: two with Sharp maps (crisp at GUI scale 2 and up), else one.
        let density = if cfg.sharp { 2.0f32 } else { 1.0 };
        let fits = |t: &WorldMapTex| {
            let want = (size[0] * density).min(1024.0).floor() as u32;
            t.w == want || (t.w < want && t.w as f32 >= size[0] * density * 0.99)
        };
        if st
            .world_tex
            .as_ref()
            .is_some_and(|t| !fits(t) || t.size != size)
        {
            // The window or the setting changed: a texture to match.
            let _ = host.free_dynamic_texture(&Identifier::parse(WORLD_TEX).expect("valid id"));
            st.world_tex = None;
        }
        if st.world_tex.is_none() {
            // As sharp as a big image allows, else a medium one.
            let id = Identifier::parse(WORLD_TEX).expect("valid id");
            let mut made = None;
            for side in [1024.0f32, 512.0, 256.0] {
                let tw = side.min(size[0] * density).floor().max(16.0) as u32;
                let th = ((tw as f32) * size[1] / size[0])
                    .floor()
                    .clamp(16.0, 1024.0) as u32;
                let blank = vec![0u8; (tw * th * 4) as usize];
                if host.create_dynamic_texture(&id, tw, th, &blank).is_ok() {
                    made = Some((tw, th));
                    break;
                }
            }
            match made {
                Some((w, h)) => {
                    st.world_tex = Some(WorldMapTex {
                        w,
                        h,
                        size,
                        drawn: None,
                        complete: false,
                        at: Instant::now(),
                    })
                }
                None => return,
            }
        }
        let m = st.ui.map.clone();
        let want = (m.cx, m.cz, m.zoom, size, m.view());
        let disk = world::disk(host);
        let busy = st.layer_busy;
        let State {
            world, world_tex, ..
        } = &mut *st;
        let (Some(world), Some(tex)) = (world.as_mut(), world_tex.as_mut()) else {
            return;
        };
        if !redraw_due(tex.drawn, want, tex.complete, busy, tex.at.elapsed()) {
            return;
        }
        let layer = world.layer_mut(m.view());
        // Regions in view, a few a frame from disk.
        let bpg = m.bpg();
        let (x0, z0) = m.world_at(0.0, 0.0, size[0], size[1]);
        let (x1, z1) = m.world_at(size[0], size[1], size[0], size[1]);
        let mut loads = 0;
        let mut pending = false;
        for rz in ((z0.floor() as i32) >> REGION_SHIFT)..=((z1.floor() as i32) >> REGION_SHIFT) {
            for rx in ((x0.floor() as i32) >> REGION_SHIFT)..=((x1.floor() as i32) >> REGION_SHIFT)
            {
                if layer.loaded((rx, rz)).is_some() || layer.is_missing((rx, rz)) {
                    continue;
                }
                if loads < 6 {
                    let _ = layer.region(&disk, (rx, rz));
                    loads += 1;
                } else {
                    pending = true;
                }
            }
        }
        let light = shade::Light {
            sky: 1.0,
            relief: cfg.slopes,
        };
        let (tw, th) = (tex.w, tex.h);
        let per = f64::from(size[0]) / f64::from(tw);
        let step = ((bpg * per).round() as i32).max(1);
        let mut rgba: Vec<u8> = Vec::with_capacity((tw * th * 4) as usize);
        let mut sampler = layer.sampler();
        // Every texel shows exactly one block (nearest, never a blend of two): the block
        // under its centre. Columns of texels share their block's x, rows their z, so a block
        // covering several texels is worked out once and its row copied.
        let bxs: Vec<i32> = (0..tw)
            .map(|tx| {
                m.world_at(((f64::from(tx) + 0.5) * per) as f32, 0.0, size[0], size[1])
                    .0
                    .floor() as i32
            })
            .collect();
        let row_len = (tw * 4) as usize;
        let mut last_bz = None;
        for ty in 0..th {
            let gy = (f64::from(ty) + 0.5) * f64::from(size[1]) / f64::from(th);
            let bz = m.world_at(0.0, gy as f32, size[0], size[1]).1.floor() as i32;
            if last_bz == Some(bz) {
                let start = rgba.len() - row_len;
                rgba.extend_from_within(start..);
                continue;
            }
            last_bz = Some(bz);
            let mut last: Option<(i32, u32)> = None;
            for &bx in &bxs {
                let c = match last {
                    Some((x, c)) if x == bx => c,
                    _ => {
                        let c =
                            view::blend(0xFF_14120F, map_pixel(&mut sampler, bx, bz, step, light));
                        last = Some((bx, c));
                        c
                    }
                };
                rgba.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8, 255]);
            }
        }
        let id = Identifier::parse(WORLD_TEX).expect("valid id");
        if host.update_dynamic_texture(&id, tw, th, &rgba).is_ok() {
            tex.drawn = Some(want);
            // Not complete: drawn again soon, until every region in view is in.
            tex.complete = !pending;
            tex.at = Instant::now();
        }
    }

    fn render_gui_post(&self, host: &dyn ClientHost, event: &mut RenderGuiPostEvent<'_>) {
        if host.on(ID) {
            self.hud(host, event);
        }
    }

    fn screen_render_post(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        if host.mod_screen() != Some(ID) {
            return;
        }
        let size = event.draw.size();
        let mouse = event.screen.mouse;
        let mut st = self.s.lock().unwrap_or_else(|e| e.into_inner());
        let list = st
            .world
            .as_ref()
            .map(|w| w.waypoints.clone())
            .unwrap_or_default();
        st.ui.build(host, &list, size);
        let draw = &mut *event.draw;
        if matches!(st.ui.page, Page::Map) {
            self.draw_world_map(&st, host, draw, mouse);
        } else {
            draw.draw_menu_background([0.0, 0.0, size[0], size[1]], true);
        }
        let player = st.player.unwrap_or_default();
        st.ui.draw_page(draw, host, &list, player);
        st.ui.draw_widgets(draw, host, mouse);
    }

    fn screen_mouse_button_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseButtonPressedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(ID) {
            return Flow::Continue;
        }
        let (at, button, modifiers) = (event.screen.mouse, event.button, event.modifiers);
        let mut st = self.s.lock().unwrap_or_else(|e| e.into_inner());
        // The right-click menu takes the click first: a choice, or a click away closes it.
        if matches!(st.ui.page, Page::Map) {
            if let Some(menu) = st.ui.menu.clone() {
                let [w, h] = st.ui.size;
                let [x, y, mw, mh] = menu.rect(w, h);
                let inside = at[0] >= x && at[0] < x + mw && at[1] >= y && at[1] < y + mh;
                if button == 0 && inside {
                    if let Some(item) = menu.item_at(w, h, at[0], at[1]) {
                        st.menu_choice(host, item, menu.pos);
                    }
                    return Flow::Cancel;
                }
                st.ui.menu = None;
                if inside || button != 1 {
                    return Flow::Cancel;
                }
                // A right-click elsewhere opens it again there (below).
            }
        }
        // The Surface | Cave switch and the layer buttons.
        if matches!(st.ui.page, Page::Map) && button == 0 {
            if let Some(hit) = ui::layer_controls_hit(st.ui.size[0], st.ui.map.cave, at[0], at[1]) {
                let layer = st.player_layer();
                match hit {
                    ui::LayerHit::Surface => st.ui.map.set_cave(false, layer),
                    ui::LayerHit::Cave => st.ui.map.set_cave(true, layer),
                    ui::LayerHit::Up => st.ui.map.step_layer(true, layer),
                    ui::LayerHit::Down => st.ui.map.step_layer(false, layer),
                }
                return Flow::Cancel;
            }
        }
        let (handled, action) = st
            .ui
            .input(|w| w.mouse_pressed(host, at, button, modifiers));
        if let Some(a) = action {
            st.act(host, a);
            return Flow::Cancel;
        }
        if !handled && matches!(st.ui.page, Page::Map) {
            let [w, h] = st.ui.size;
            if button == 0 {
                st.ui.map.dragging = true;
            } else if button == 1 {
                if config().map_menu {
                    st.open_menu(host, at);
                } else {
                    // A waypoint where you right-click, at the ground's height there.
                    let (wx, wz) = st.ui.map.world_at(at[0], at[1], w, h);
                    let (bx, bz) = (wx.floor() as i32, wz.floor() as i32);
                    let y = st.ground_at(bx, bz);
                    let e = st.new_waypoint_editor([bx, y, bz], Back::Map, host);
                    st.ui.show(Page::Edit(e));
                }
            }
            return Flow::Cancel;
        }
        if handled {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn screen_mouse_button_released_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseButtonReleasedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(ID) {
            return Flow::Continue;
        }
        let button = event.button;
        let mut st = self.s.lock().unwrap_or_else(|e| e.into_inner());
        st.ui.map.dragging = false;
        let (handled, _) = st.ui.input(|w| w.mouse_released(button));
        if handled {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn screen_mouse_dragged_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseDraggedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(ID) {
            return Flow::Continue;
        }
        let (at, button, delta) = (event.screen.mouse, event.button, event.delta);
        let mut st = self.s.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(st.ui.page, Page::Map) && st.ui.map.dragging && button == 0 {
            st.ui.menu = None;
            let bpg = st.ui.map.bpg();
            st.ui.map.follow = false;
            st.ui.map.cx -= f64::from(delta[0]) * bpg;
            st.ui.map.cz -= f64::from(delta[1]) * bpg;
            return Flow::Cancel;
        }
        let (handled, _) = st.ui.input(|w| w.mouse_dragged(host, at, button));
        if handled {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn screen_mouse_scrolled_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseScrolledPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(ID) {
            return Flow::Continue;
        }
        let (at, dy) = (event.screen.mouse, event.scroll[1]);
        let mut st = self.s.lock().unwrap_or_else(|e| e.into_inner());
        match st.ui.page {
            Page::Map => {
                st.ui.menu = None;
                let [w, h] = st.ui.size;
                let before = st.ui.map.world_at(at[0], at[1], w, h);
                let z = st.ui.map.zoom;
                st.ui.map.zoom = if dy > 0.0 {
                    z.saturating_sub(1)
                } else if dy < 0.0 {
                    (z + 1).min(ui::MAP_ZOOMS.len() - 1)
                } else {
                    z
                };
                // Zoom about the cursor.
                let after = st.ui.map.world_at(at[0], at[1], w, h);
                if st.ui.map.zoom != z {
                    st.ui.map.follow = false;
                    st.ui.map.cx += before.0 - after.0;
                    st.ui.map.cz += before.1 - after.1;
                }
                Flow::Cancel
            }
            Page::List => {
                st.ui.list_scroll = if dy > 0.0 {
                    st.ui.list_scroll.saturating_sub(1)
                } else {
                    st.ui.list_scroll + 1
                };
                st.ui.show(Page::List);
                Flow::Cancel
            }
            Page::Menu | Page::Edit(_) | Page::Confirm(..) => Flow::Continue,
        }
    }

    fn screen_key_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenKeyPressedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(ID) {
            return Flow::Continue;
        }
        let (key, modifiers) = (event.key, event.modifiers);
        let mut st = self.s.lock().unwrap_or_else(|e| e.into_inner());
        // Escape closes the right-click menu before the map.
        if st.ui.menu.is_some() && key == "key.keyboard.escape" {
            st.ui.menu = None;
            return Flow::Cancel;
        }
        // On the map, Tab is held to show faces and names (read from the key itself), so it
        // never moves the focus between the buttons, which held it would do over and over.
        if matches!(st.ui.page, Page::Map) && st.ui.menu.is_none() && key == "key.keyboard.tab" {
            return Flow::Cancel;
        }
        let (handled, action) = st.ui.input(|w| w.key_pressed(host, key, modifiers));
        if let Some(a) = action {
            st.act(host, a);
            return Flow::Cancel;
        }
        if handled {
            return Flow::Cancel;
        }
        // The map key closes the map; the list key the list.
        let closes = match st.ui.page {
            Page::Map => bound(host, "world_map", key),
            Page::List => bound(host, "waypoints", key),
            Page::Menu | Page::Edit(_) | Page::Confirm(..) => false,
        };
        if closes {
            st.ui.requests.push(Request::Close);
            return Flow::Cancel;
        }
        if matches!(st.ui.page, Page::Map) && key == "key.keyboard.c" {
            let (cave, layer) = (!st.ui.map.cave, st.player_layer());
            st.ui.map.set_cave(cave, layer);
            return Flow::Cancel;
        }
        if matches!(st.ui.page, Page::Map) {
            if let Some(up) = ui::layer_key(key) {
                let layer = st.player_layer();
                st.ui.map.step_layer(up, layer);
                return Flow::Cancel;
            }
        }
        if matches!(st.ui.page, Page::Map) {
            let step = st.ui.map.bpg() * 24.0;
            let (dx, dz) = match key {
                "key.keyboard.left" | "key.keyboard.a" => (-step, 0.0),
                "key.keyboard.right" | "key.keyboard.d" => (step, 0.0),
                "key.keyboard.up" | "key.keyboard.w" => (0.0, -step),
                "key.keyboard.down" | "key.keyboard.s" => (0.0, step),
                _ => return Flow::Continue,
            };
            st.ui.map.follow = false;
            st.ui.map.cx += dx;
            st.ui.map.cz += dz;
            return Flow::Cancel;
        }
        Flow::Continue
    }

    fn screen_char_typed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenCharTypedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(ID) {
            return Flow::Continue;
        }
        let c = event.c;
        let mut st = self.s.lock().unwrap_or_else(|e| e.into_inner());
        let (handled, _) = st.ui.input(|w| w.char_typed(host, c));
        if handled {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn register_client_commands(
        &self,
        _host: &mut dyn ClientHost,
        event: &mut RegisterClientCommandsEvent<'_>,
    ) {
        for usage in [
            "frontier",
            "frontier map",
            "frontier waypoints",
            "frontier settings",
            "frontier add [name:text]",
        ] {
            event.commands.push(mod_api::Command::new(usage));
        }
    }

    fn command(&self, host: &mut dyn ClientHost, event: &mut CommandEvent<'_>) -> Flow {
        let mut words = event.command.trim().splitn(3, ' ');
        if words.next() != Some("frontier") {
            return Flow::Continue;
        }
        let mut st = self.s.lock().unwrap_or_else(|e| e.into_inner());
        match words.next() {
            None | Some("") => st.open = Some(Page::Menu),
            Some("map") => {
                st.open_world_map();
                st.open = Some(Page::Map);
            }
            Some("waypoints") => st.open = Some(Page::List),
            Some("settings") => OPEN_SETTINGS.store(true, Ordering::Relaxed),
            Some("add") => {
                let pos = st.block_pos().unwrap_or([0; 3]);
                let mut e = st.new_waypoint_editor(pos, Back::Game, host);
                if let Some(name) = words.next().map(str::trim).filter(|n| !n.is_empty()) {
                    // Named on the command line: added at once.
                    let w = Waypoint {
                        name: name.chars().take(32).collect(),
                        pos,
                        color: e.color,
                        shown: true,
                        kind: Kind::Normal,
                    };
                    let msg = tr(host, "frontier.notice.added").replace("%s", &w.name);
                    if let Some(world) = st.world.as_mut() {
                        world.waypoints.push(w);
                        world.waypoints_dirty = true;
                        st.notice(msg);
                    }
                } else {
                    e.back = Back::Game;
                    st.open = Some(Page::Edit(e));
                }
            }
            Some(_) => st.open = Some(Page::Menu),
        }
        Flow::Cancel
    }

    fn render_level_stage(&self, host: &dyn ClientHost, event: &mut RenderLevelStageEvent<'_>) {
        if event.stage != RenderLevelStage::AfterLevel || !host.on(ID) {
            return;
        }
        let cfg = config();
        if !cfg.beacons && !cfg.labels {
            return;
        }
        let st = self.s.lock().unwrap_or_else(|e| e.into_inner());
        let Some(world) = &st.world else { return };
        let cam = event.view.camera;
        let mut shown: Vec<(&Waypoint, f64)> = world
            .waypoints
            .iter()
            .filter(|w| w.shown)
            .map(|w| (w, waypoints::distance(cam, w.pos)))
            .filter(|(_, d)| cfg.label_range <= 0.0 || *d <= f64::from(cfg.label_range))
            .collect();
        shown.sort_by(|a, b| a.1.total_cmp(&b.1));
        shown.truncate(24);
        let mut lines = Vec::new();
        for (w, d) in &shown {
            let (x, y, z) = (
                f64::from(w.pos[0]) + 0.5,
                f64::from(w.pos[1]),
                f64::from(w.pos[2]) + 0.5,
            );
            if cfg.beacons && *d > 2.0 {
                let c = (w.argb() & 0x00FF_FFFF) | 0xB0_000000;
                lines.push(WorldLine::new(
                    [x, y, z],
                    [x, y + 96.0, z],
                    c,
                    3.0,
                    WorldDepth::ThroughWalls,
                ));
            }
            if cfg.labels {
                // Drawn a fixed way from the camera toward the spot, so it reads the same
                // size near or far.
                let to = [x - cam[0], y + 1.2 - cam[1], z - cam[2]];
                let len = (to[0] * to[0] + to[1] * to[1] + to[2] * to[2])
                    .sqrt()
                    .max(1e-3);
                let k = len.min(12.0) / len;
                let at = [cam[0] + to[0] * k, cam[1] + to[1] * k, cam[2] + to[2] * k];
                let text = Text::literal(format!(
                    "{} \u{2022} {}",
                    w.name,
                    waypoints::distance_text(*d)
                ));
                let _ = event
                    .draw
                    .name_tag(at, &text, NameTagOptions::new(true, true, 15));
            }
        }
        if !lines.is_empty() {
            let _ = event.draw.lines(&lines);
        }
    }

    fn client_block_changed(
        &self,
        _host: &dyn ClientHost,
        event: &mut ClientBlockChangedEvent<'_>,
    ) {
        let p = event.pos;
        if let Ok(mut st) = self.s.try_lock() {
            st.scanner.mark((p.x >> 4, p.z >> 4));
        }
    }

    fn client_chunk_load(&self, _host: &mut dyn ClientHost, event: &mut ClientChunkLoadEvent<'_>) {
        let mut st = self.s.lock().unwrap_or_else(|e| e.into_inner());
        st.scanner.mark((event.chunk[0], event.chunk[1]));
    }

    fn client_level_unload(
        &self,
        host: &mut dyn ClientHost,
        _event: &mut ClientLevelUnloadEvent<'_>,
    ) {
        self.s.lock().unwrap_or_else(|e| e.into_inner()).save(host);
    }

    fn client_player_logging_out(
        &self,
        host: &mut dyn ClientHost,
        _event: &mut ClientPlayerLoggingOutEvent,
    ) {
        let mut st = self.s.lock().unwrap_or_else(|e| e.into_inner());
        st.save(host);
        st.world = None;
        st.player = None;
    }

    fn client_stopping(&self, host: &mut dyn ClientHost, _event: &mut ClientStoppingEvent) {
        self.s.lock().unwrap_or_else(|e| e.into_inner()).save(host);
    }
}

#[allow(dead_code)]
fn _outcome_is_used(o: Outcome) -> bool {
    o.handled()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_of_layer_redraws_the_world_map_at_once() {
        let at = |v: Option<i32>| (10.0, 20.0, 3usize, [400.0f32, 300.0], v);
        let just = Duration::from_millis(1);
        // Nothing drawn yet: draw.
        assert!(redraw_due(None, at(None), true, false, just));
        // Surface to cave, cave to surface, and one cave layer to the next: at once, even a
        // millisecond after the last picture, so nothing of the view before shows.
        assert!(redraw_due(Some(at(None)), at(Some(4)), true, false, just));
        assert!(redraw_due(Some(at(Some(4))), at(None), true, false, just));
        assert!(redraw_due(
            Some(at(Some(4))),
            at(Some(5)),
            true,
            false,
            just
        ));
        assert!(redraw_due(
            Some(at(Some(4))),
            at(Some(3)),
            true,
            false,
            just
        ));
        // The same view, finished and not being read: only every half second.
        assert!(!redraw_due(
            Some(at(Some(4))),
            at(Some(4)),
            true,
            false,
            Duration::from_millis(200)
        ));
        assert!(redraw_due(
            Some(at(Some(4))),
            at(Some(4)),
            true,
            false,
            Duration::from_millis(501)
        ));
        // While its cave layer is still being read: every 100 ms, so it fills in as it comes.
        assert!(redraw_due(
            Some(at(Some(4))),
            at(Some(4)),
            true,
            true,
            Duration::from_millis(101)
        ));
        assert!(!redraw_due(
            Some(at(Some(4))),
            at(Some(4)),
            true,
            true,
            Duration::from_millis(60)
        ));
        // Regions still loading: every 33 ms.
        assert!(redraw_due(
            Some(at(None)),
            at(None),
            false,
            false,
            Duration::from_millis(34)
        ));
        // A pan in the same view: at most every 33 ms.
        let moved = (11.0, 20.0, 3usize, [400.0f32, 300.0], Some(4));
        assert!(!redraw_due(
            Some(at(Some(4))),
            moved,
            true,
            false,
            Duration::from_millis(10)
        ));
        assert!(redraw_due(
            Some(at(Some(4))),
            moved,
            true,
            false,
            Duration::from_millis(40)
        ));
    }

    #[test]
    fn dropped_items_show_their_icon_only_while_revealed() {
        use radar::Kind;
        // Tab held (or names always on), a known item: its icon.
        assert!(item_icon_shown(Kind::Item, true, true, 1.0));
        // Tab not held: the black dot.
        assert!(!item_icon_shown(Kind::Item, false, true, 1.0));
        // An item the game doesn't know: the dot (and its name).
        assert!(!item_icon_shown(Kind::Item, true, false, 1.0));
        // Faded far above or below: the dot.
        assert!(!item_icon_shown(Kind::Item, true, true, 0.2));
        // Only items.
        assert!(!item_icon_shown(Kind::Object, true, true, 1.0));
        // Named beside the icon, as players are beside their heads.
        let cfg = Config::default();
        let d = radar::Dot::dropped([0.0; 3], "minecraft:diamond", "Diamond".into(), 3);
        assert!(named_now(&cfg, &d, true));
        assert!(named_now(&cfg, &d, false));
    }
}
