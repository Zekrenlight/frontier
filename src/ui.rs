//! Frontier's own screen: the world map, the waypoint list, the waypoint editor, the delete
//! check and a small menu. Buttons and fields are the game's own widgets, so a resource pack
//! that restyles the game's buttons restyles these too.

use crate::waypoints::{self, Kind, Waypoint, COLORS, COLOR_NAMES};
use mod_api::client::widgets::{Button, Outcome, TextField, WidgetId, Widgets};
use mod_api::{ClientHost, DrawContext, Text};

/// The world map's zoom steps, in blocks per GUI pixel.
pub const MAP_ZOOMS: [f32; 9] = [0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0];

#[derive(Clone, Debug, PartialEq)]
pub struct MapView {
    pub cx: f64,
    pub cz: f64,
    /// An index into `MAP_ZOOMS`.
    pub zoom: usize,
    /// Stays on the player until dragged.
    pub follow: bool,
    pub dragging: bool,
    /// Shows a cave layer rather than the surface.
    pub cave: bool,
    /// The cave layer shown (see [`crate::world::layer_of`]), while `cave`.
    pub layer: i32,
    /// The cave layer moves with the player (until stepped up or down by hand).
    pub follow_layer: bool,
    /// The dimension's lowest and highest cave layers.
    pub bounds: (i32, i32),
}

impl MapView {
    pub fn bpg(&self) -> f64 {
        f64::from(MAP_ZOOMS[self.zoom.min(MAP_ZOOMS.len() - 1)])
    }
    /// The world's x and z under GUI point `(gx, gy)` of a `w` x `h` screen.
    pub fn world_at(&self, gx: f32, gy: f32, w: f32, h: f32) -> (f64, f64) {
        (
            self.cx + f64::from(gx - w / 2.0) * self.bpg(),
            self.cz + f64::from(gy - h / 2.0) * self.bpg(),
        )
    }
    /// What is shown: None for the surface, else the cave layer's number. Pictures of the map
    /// are kept under this, so a change of it is a change of picture.
    pub fn view(&self) -> Option<i32> {
        self.cave.then_some(self.layer)
    }
    /// Opened from play: on the player, in the view that matches where they are (in cave mode,
    /// the cave layer of their feet, following them; else the surface).
    pub fn open(&mut self, in_cave: bool, player_layer: i32) {
        self.follow = true;
        self.dragging = false;
        self.cave = in_cave;
        self.layer = self.clamp(player_layer);
        self.follow_layer = true;
    }
    /// The Surface | Cave switch (and C): into the caves at the player's own layer.
    pub fn set_cave(&mut self, cave: bool, player_layer: i32) {
        if cave && !self.cave {
            self.layer = self.clamp(player_layer);
            self.follow_layer = true;
        }
        self.cave = cave;
    }
    /// One layer up or down (from the surface: into the caves at the player's layer). Back on
    /// the player's own layer, the view follows them again.
    pub fn step_layer(&mut self, up: bool, player_layer: i32) {
        if !self.cave {
            self.set_cave(true, player_layer);
            return;
        }
        self.layer = self.clamp(if up { self.layer + 1 } else { self.layer - 1 });
        self.follow_layer = self.layer == player_layer;
    }
    /// Each frame: the layer goes with the player while it follows them.
    pub fn track(&mut self, player_layer: i32) {
        if self.cave && self.follow_layer {
            self.layer = self.clamp(player_layer);
        }
    }
    fn clamp(&self, layer: i32) -> i32 {
        let (lo, hi) = self.bounds;
        if lo <= hi {
            layer.clamp(lo, hi)
        } else {
            layer
        }
    }
    /// Where the world's `(x, z)` shows on a `w` x `h` screen.
    pub fn screen_at(&self, x: f64, z: f64, w: f32, h: f32) -> (f32, f32) {
        (
            ((x - self.cx) / self.bpg()) as f32 + w / 2.0,
            ((z - self.cz) / self.bpg()) as f32 + h / 2.0,
        )
    }
}

/// Where the editor goes back to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Back {
    Game,
    List,
    Map,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Editor {
    /// The waypoint being changed; None for a new one.
    pub index: Option<usize>,
    pub name: String,
    pub pos: [i32; 3],
    pub color: usize,
    pub back: Back,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    Menu,
    Map,
    List,
    Edit(Editor),
    /// Removing waypoint `.0`, asked first; `.1` is where the answer goes back to.
    Confirm(usize, Back),
}

/// A choice of the world map's right-click menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItem {
    EditWaypoint(usize),
    RemoveWaypoint(usize),
    TeleportTo,
    NewWaypointHere,
    CopyCoords,
    ShareInChat,
    OpenSettings,
}

impl MenuItem {
    pub fn label_key(self) -> &'static str {
        match self {
            MenuItem::EditWaypoint(_) => "frontier.map_menu.edit",
            MenuItem::RemoveWaypoint(_) => "frontier.map_menu.remove",
            MenuItem::TeleportTo => "frontier.map_menu.teleport",
            MenuItem::NewWaypointHere => "frontier.map_menu.new_waypoint",
            MenuItem::CopyCoords => "frontier.map_menu.copy",
            MenuItem::ShareInChat => "frontier.map_menu.share",
            MenuItem::OpenSettings => "frontier.map_menu.settings",
        }
    }
}

/// The world map's right-click menu: where it opened, the spot it is about and its choices.
#[derive(Clone, Debug, PartialEq)]
pub struct MapMenu {
    /// Its top left corner, in GUI pixels (moved to fit the screen when drawn).
    pub at: [f32; 2],
    /// The block it is about.
    pub pos: [i32; 3],
    /// The waypoint under the pointer, when it opened on one.
    pub waypoint: Option<usize>,
    pub items: Vec<MenuItem>,
}

pub const MENU_W: f32 = 132.0;
pub const MENU_ROW: f32 = 13.0;
pub const MENU_HEAD: f32 = 26.0;

impl MapMenu {
    /// The menu's rectangle on a `w` x `h` screen.
    pub fn rect(&self, w: f32, h: f32) -> [f32; 4] {
        let mh = MENU_HEAD + self.items.len() as f32 * MENU_ROW + 3.0;
        let x = self.at[0].min(w - MENU_W - 2.0).max(2.0).floor();
        let y = self.at[1].min(h - mh - 2.0).max(2.0).floor();
        [x, y, MENU_W, mh]
    }
    /// Which choice is under `(mx, my)`.
    pub fn item_at(&self, w: f32, h: f32, mx: f32, my: f32) -> Option<MenuItem> {
        let [x, y, mw, _] = self.rect(w, h);
        if mx < x || mx >= x + mw {
            return None;
        }
        let row = ((my - y - MENU_HEAD) / MENU_ROW).floor();
        if row < 0.0 {
            return None;
        }
        self.items.get(row as usize).copied()
    }
}

/// The world map's Surface | Cave switch: its rectangle on a `w`-wide screen (top left, under
/// the title bar), each half `MODE_SEG` wide.
pub const MODE_SEG: f32 = 46.0;
pub fn mode_switch_rect(_w: f32) -> [f32; 4] {
    [4.0, 18.0, MODE_SEG * 2.0, 14.0]
}
/// Which half of the switch `(mx, my)` is on: Some(false) Surface, Some(true) Cave.
pub fn mode_switch_hit(w: f32, mx: f32, my: f32) -> Option<bool> {
    let [x, y, sw, sh] = mode_switch_rect(w);
    if mx < x || mx >= x + sw || my < y || my >= y + sh {
        return None;
    }
    Some(mx >= x + MODE_SEG)
}

/// The cave layer buttons, under the switch while a cave layer shows: down, then up, each
/// `LAYER_BTN` square, and the layer's name beside them.
pub const LAYER_BTN: f32 = 14.0;
pub fn layer_button_rect(w: f32, up: bool) -> [f32; 4] {
    let [x, y, _, h] = mode_switch_rect(w);
    let x = if up { x + LAYER_BTN + 2.0 } else { x };
    [x, y + h + 3.0, LAYER_BTN, LAYER_BTN]
}

/// What a click on the world map's layer controls does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerHit {
    Surface,
    Cave,
    Down,
    Up,
}

/// Which control `(mx, my)` is on: the switch's halves always, the layer buttons while a cave
/// layer shows.
pub fn layer_controls_hit(w: f32, cave: bool, mx: f32, my: f32) -> Option<LayerHit> {
    if let Some(c) = mode_switch_hit(w, mx, my) {
        return Some(if c { LayerHit::Cave } else { LayerHit::Surface });
    }
    if !cave {
        return None;
    }
    for (up, hit) in [(false, LayerHit::Down), (true, LayerHit::Up)] {
        let [x, y, bw, bh] = layer_button_rect(w, up);
        if mx >= x && mx < x + bw && my >= y && my < y + bh {
            return Some(hit);
        }
    }
    None
}

/// The keys that step through the cave layers on the world map: `]` or Page Up a layer up,
/// `[` or Page Down a layer down.
pub fn layer_key(key: &str) -> Option<bool> {
    match key {
        "key.keyboard.right.bracket" | "key.keyboard.page.up" => Some(true),
        "key.keyboard.left.bracket" | "key.keyboard.page.down" => Some(false),
        _ => None,
    }
}

/// The name of what the map shows: `Surface`, or `Cave · Y 48–63`.
pub fn layer_name(surface: &str, cave: &str, view: Option<i32>) -> String {
    match view {
        None => surface.to_owned(),
        Some(l) => {
            let (lo, hi) = crate::world::layer_range(l);
            format!("{cave} \u{b7} Y {lo}\u{2013}{hi}")
        }
    }
}

/// What a button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    OpenMap,
    OpenList,
    NewWaypoint,
    Settings,
    Done,
    Toggle(usize),
    Edit(usize),
    Delete(usize),
    ConfirmDelete(usize, Back),
    /// Leaves the remove check without removing.
    CancelDelete(Back),
    CycleColor,
    Save,
    Cancel,
    Centre,
    PageUp,
    PageDown,
}

/// What the screen asks of the game, done on the next tick (the input hooks only read).
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    Close,
    Settings,
    /// Closes the screen and opens chat with this typed in, not sent.
    Chat(String),
    /// `/tp` to this spot (asked only of a player allowed to).
    Teleport([i32; 3]),
}

pub struct Ui {
    pub page: Page,
    pub map: MapView,
    pub list_scroll: usize,
    widgets: Widgets,
    actions: Vec<(WidgetId, Action)>,
    fields: Vec<WidgetId>,
    built: Option<(String, [f32; 2], usize)>,
    pub size: [f32; 2],
    pub requests: Vec<Request>,
    pub menu: Option<MapMenu>,
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            page: Page::Menu,
            map: MapView {
                cx: 0.0,
                cz: 0.0,
                zoom: 3,
                follow: true,
                dragging: false,
                cave: false,
                layer: 4,
                follow_layer: true,
                bounds: (-4, 19),
            },
            list_scroll: 0,
            widgets: Widgets::new(),
            actions: Vec::new(),
            fields: Vec::new(),
            built: None,
            size: [0.0, 0.0],
            requests: Vec::new(),
            menu: None,
        }
    }
}

fn tr(host: &dyn ClientHost, key: &str) -> String {
    host.translate(key).unwrap_or_else(|| key.to_owned())
}

/// Rows of the waypoint list that fit on a screen `h` high.
fn rows_for(h: f32) -> usize {
    (((h - 76.0) / 24.0).floor() as usize).max(1)
}

impl Ui {
    pub fn show(&mut self, page: Page) {
        self.page = page;
        self.built = None;
        self.menu = None;
    }

    fn signature(&self) -> String {
        match &self.page {
            Page::Menu => "menu".into(),
            Page::Map => "map".into(),
            Page::List => format!("list{}", self.list_scroll),
            Page::Edit(e) => format!("edit{:?}{}", e.index, e.color),
            Page::Confirm(i, b) => format!("confirm{i}{b:?}"),
        }
    }

    fn button(&mut self, rect: [f32; 4], label: String, action: Action) {
        let id = self
            .widgets
            .add_button(Button::new(rect, Text::literal(label)));
        self.actions.push((id, action));
    }

    fn field(&mut self, rect: [f32; 4], value: &str, max: usize) -> WidgetId {
        let mut f = TextField::new(rect, max);
        f.set_value(value);
        let id = self.widgets.add_text_field(f);
        self.fields.push(id);
        id
    }

    /// The text in the editor's fields, so a rebuild keeps what was typed.
    fn field_values(&self) -> Vec<String> {
        self.fields
            .iter()
            .filter_map(|id| self.widgets.text_field(*id).map(|f| f.value().to_owned()))
            .collect()
    }

    /// Lays the widgets out again when the page, the list or the screen size changed.
    pub fn build(&mut self, host: &dyn ClientHost, list: &[Waypoint], size: [f32; 2]) {
        self.size = size;
        let sig = (self.signature(), size, list.len());
        if self.built.as_ref() == Some(&sig) {
            return;
        }
        // Keep what was typed into the editor across a rebuild.
        if let (Page::Edit(e), Some(_)) = (&mut self.page, &self.built) {
            let v: Vec<String> = self
                .fields
                .iter()
                .filter_map(|id| self.widgets.text_field(*id).map(|f| f.value().to_owned()))
                .collect();
            if v.len() == 4 {
                e.name = v[0].clone();
                for (i, s) in v[1..].iter().enumerate() {
                    if let Ok(n) = s.trim().parse::<i32>() {
                        e.pos[i] = n;
                    }
                }
            }
        }
        self.widgets = Widgets::new();
        self.actions.clear();
        self.fields.clear();
        let [w, h] = size;
        let cx = (w / 2.0).floor();
        match self.page.clone() {
            Page::Menu => {
                let x = cx - 100.0;
                let mut y = (h / 2.0 - 60.0).floor();
                for (label, action) in [
                    ("frontier.menu.world_map", Action::OpenMap),
                    ("frontier.menu.waypoints", Action::OpenList),
                    ("frontier.menu.new_waypoint", Action::NewWaypoint),
                    ("frontier.menu.settings", Action::Settings),
                    ("gui.done", Action::Done),
                ] {
                    self.button([x, y, 200.0, 20.0], tr(host, label), action);
                    y += 24.0;
                }
            }
            Page::Map => {
                let y = h - 24.0;
                let bw = 76.0;
                let x0 = cx - (bw * 4.0 + 12.0) / 2.0;
                for (i, (label, action)) in [
                    ("frontier.menu.waypoints", Action::OpenList),
                    ("frontier.map.centre", Action::Centre),
                    ("frontier.menu.settings", Action::Settings),
                    ("gui.done", Action::Done),
                ]
                .into_iter()
                .enumerate()
                {
                    self.button(
                        [x0 + i as f32 * (bw + 4.0), y, bw, 20.0],
                        tr(host, label),
                        action,
                    );
                }
            }
            Page::List => {
                let rows = rows_for(h);
                let max_scroll = list.len().saturating_sub(rows);
                self.list_scroll = self.list_scroll.min(max_scroll);
                let left = cx - 160.0;
                for (row, i) in
                    (self.list_scroll..list.len().min(self.list_scroll + rows)).enumerate()
                {
                    let y = 32.0 + row as f32 * 24.0;
                    let shown = if list[i].shown {
                        "frontier.list.shown"
                    } else {
                        "frontier.list.hidden"
                    };
                    self.button(
                        [left + 170.0, y, 50.0, 20.0],
                        tr(host, shown),
                        Action::Toggle(i),
                    );
                    self.button(
                        [left + 224.0, y, 46.0, 20.0],
                        tr(host, "frontier.list.edit"),
                        Action::Edit(i),
                    );
                    self.button(
                        [left + 274.0, y, 46.0, 20.0],
                        tr(host, "frontier.list.delete"),
                        Action::Delete(i),
                    );
                }
                let y = h - 28.0;
                self.button(
                    [cx - 160.0, y, 20.0, 20.0],
                    "\u{25B2}".into(),
                    Action::PageUp,
                );
                self.button(
                    [cx - 136.0, y, 20.0, 20.0],
                    "\u{25BC}".into(),
                    Action::PageDown,
                );
                self.button(
                    [cx - 110.0, y, 70.0, 20.0],
                    tr(host, "frontier.list.new"),
                    Action::NewWaypoint,
                );
                self.button(
                    [cx - 36.0, y, 70.0, 20.0],
                    tr(host, "frontier.menu.world_map"),
                    Action::OpenMap,
                );
                self.button(
                    [cx + 38.0, y, 122.0, 20.0],
                    tr(host, "gui.done"),
                    Action::Done,
                );
            }
            Page::Edit(e) => {
                let x = cx - 100.0;
                let top = (h / 2.0 - 70.0).floor().max(30.0);
                self.field([x, top, 200.0, 20.0], &e.name, 32);
                for i in 0..3 {
                    self.field(
                        [x + i as f32 * 68.0, top + 36.0, 64.0, 20.0],
                        &e.pos[i].to_string(),
                        9,
                    );
                }
                let colour = format!(
                    "{}: {}",
                    tr(host, "frontier.edit.color"),
                    tr(host, COLOR_NAMES[e.color % COLORS.len()])
                );
                self.button(
                    [x + 24.0, top + 66.0, 176.0, 20.0],
                    colour,
                    Action::CycleColor,
                );
                self.button(
                    [x, top + 96.0, 98.0, 20.0],
                    tr(host, "frontier.edit.save"),
                    Action::Save,
                );
                self.button(
                    [x + 102.0, top + 96.0, 98.0, 20.0],
                    tr(host, "gui.cancel"),
                    Action::Cancel,
                );
                let _ = self.widgets.set_focus(host, self.fields.first().copied());
            }
            Page::Confirm(i, back) => {
                let y = (h / 2.0).floor();
                self.button(
                    [cx - 102.0, y, 100.0, 20.0],
                    tr(host, "frontier.list.delete"),
                    Action::ConfirmDelete(i, back),
                );
                self.button(
                    [cx + 2.0, y, 100.0, 20.0],
                    tr(host, "gui.cancel"),
                    Action::CancelDelete(back),
                );
            }
        }
        self.built = Some((self.signature(), size, list.len()));
    }

    pub fn draw_widgets(
        &mut self,
        draw: &mut dyn DrawContext,
        host: &dyn ClientHost,
        mouse: [f32; 2],
    ) {
        let _ = self.widgets.draw(draw, host, mouse);
    }

    /// Draws the list, editor and confirm pages' own text (the map page is drawn by the mod).
    pub fn draw_page(
        &self,
        draw: &mut dyn DrawContext,
        host: &dyn ClientHost,
        list: &[Waypoint],
        player: [f64; 3],
    ) {
        let [w, h] = self.size;
        let cx = (w / 2.0).floor();
        let centred = |draw: &mut dyn DrawContext, s: &str, y: f32, c: u32| {
            let tw = draw.text_width(s);
            draw.text(s, [cx - tw / 2.0, y], c, true);
        };
        match &self.page {
            Page::Menu => centred(draw, "Frontier", (h / 2.0 - 80.0).floor(), 0xFF_FFFFFF),
            Page::Map => {}
            Page::List => {
                centred(draw, &tr(host, "frontier.list.title"), 12.0, 0xFF_FFFFFF);
                let left = cx - 160.0;
                let rows = rows_for(h);
                if list.is_empty() {
                    centred(draw, &tr(host, "frontier.list.empty"), 50.0, 0xFF_A0A0A0);
                }
                for (row, i) in
                    (self.list_scroll..list.len().min(self.list_scroll + rows)).enumerate()
                {
                    let wp = &list[i];
                    let y = 32.0 + row as f32 * 24.0;
                    draw.fill([left, y + 4.0, 12.0, 12.0], 0xFF_101010);
                    draw.fill([left + 1.0, y + 5.0, 10.0, 10.0], wp.argb());
                    let name = if wp.kind == Kind::Death {
                        format!("\u{2620} {}", wp.name)
                    } else {
                        wp.name.clone()
                    };
                    let name = fit(draw, &name, 150.0);
                    draw.text(
                        &name,
                        [left + 16.0, y + 1.0],
                        if wp.shown { 0xFF_FFFFFF } else { 0xFF_909090 },
                        true,
                    );
                    let d = waypoints::distance_text(waypoints::distance(player, wp.pos));
                    let sub = format!("{} {} {}  \u{2022} {d}", wp.pos[0], wp.pos[1], wp.pos[2]);
                    let sub = fit(draw, &sub, 150.0);
                    draw.text(&sub, [left + 16.0, y + 11.0], 0xFF_A0A0A0, false);
                }
            }
            Page::Edit(e) => {
                let x = cx - 100.0;
                let top = (h / 2.0 - 70.0).floor().max(30.0);
                let title = if e.index.is_some() {
                    "frontier.edit.title_edit"
                } else {
                    "frontier.edit.title_new"
                };
                centred(draw, &tr(host, title), top - 22.0, 0xFF_FFFFFF);
                draw.text(
                    &tr(host, "frontier.edit.name"),
                    [x, top - 10.0],
                    0xFF_A0A0A0,
                    false,
                );
                for (i, l) in ["X", "Y", "Z"].iter().enumerate() {
                    draw.text(l, [x + i as f32 * 68.0, top + 26.0], 0xFF_A0A0A0, false);
                }
                draw.fill([x, top + 66.0, 20.0, 20.0], 0xFF_101010);
                draw.fill(
                    [x + 2.0, top + 68.0, 16.0, 16.0],
                    COLORS[e.color % COLORS.len()],
                );
            }
            Page::Confirm(i, _) => {
                let name = list.get(*i).map_or(String::new(), |w| w.name.clone());
                let q = tr(host, "frontier.confirm.question").replace("%s", &name);
                centred(draw, &q, (h / 2.0 - 30.0).floor(), 0xFF_FFFFFF);
                centred(
                    draw,
                    &tr(host, "frontier.confirm.note"),
                    (h / 2.0 - 16.0).floor(),
                    0xFF_A0A0A0,
                );
            }
        }
    }

    /// Feeds an input to the widgets; a pressed button becomes its action.
    pub fn input(&mut self, f: impl FnOnce(&mut Widgets) -> Outcome) -> (bool, Option<Action>) {
        let outcome = f(&mut self.widgets);
        let action = match outcome {
            Outcome::Pressed(id) => self.actions.iter().find(|(w, _)| *w == id).map(|(_, a)| *a),
            Outcome::Submitted(_) if matches!(self.page, Page::Edit(_)) => Some(Action::Save),
            _ => None,
        };
        (outcome.handled(), action)
    }

    /// The editor's fields as typed: name and the three coordinates (unparsable ones kept).
    pub fn editor_values(&self) -> Option<(String, [i32; 3])> {
        let Page::Edit(e) = &self.page else {
            return None;
        };
        let v = self.field_values();
        if v.len() != 4 {
            return None;
        }
        let mut pos = e.pos;
        for (i, s) in v[1..].iter().enumerate() {
            if let Ok(n) = s.trim().parse::<i32>() {
                pos[i] = n.clamp(-30_000_000, 30_000_000);
            }
        }
        Some((v[0].clone(), pos))
    }

    pub fn list_rows(&self) -> usize {
        rows_for(self.size[1])
    }
}

/// Cuts text to fit `width` GUI pixels, ending with an ellipsis.
pub fn fit(draw: &dyn DrawContext, s: &str, width: f32) -> String {
    if draw.text_width(s) <= width {
        return s.to_owned();
    }
    let mut out: String = s.to_owned();
    while !out.is_empty() && draw.text_width(&format!("{out}\u{2026}")) > width {
        out.pop();
    }
    format!("{out}\u{2026}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_opens_in_the_players_view_and_switches() {
        let mut m = Ui::default().map;
        assert!(!m.cave);
        m.follow = false;
        m.open(true, 0);
        assert!(m.cave && m.follow);
        m.open(false, 0);
        assert!(!m.cave);
        // The switch: left half Surface, right half Cave, nothing outside it.
        let [x, y, w, h] = mode_switch_rect(400.0);
        assert_eq!(mode_switch_hit(400.0, x + 2.0, y + h / 2.0), Some(false));
        assert_eq!(mode_switch_hit(400.0, x + w - 2.0, y + h / 2.0), Some(true));
        assert_eq!(mode_switch_hit(400.0, x + w + 1.0, y + 1.0), None);
        assert_eq!(mode_switch_hit(400.0, x + 1.0, y + h + 1.0), None);
        // Below the title bar.
        assert!(y >= 14.0);
    }

    #[test]
    fn opening_underground_shows_the_players_own_cave_layer() {
        let mut m = Ui::default().map;
        // In a base at Y 10 (layer 0): the map opens on that layer, not the surface.
        m.open(true, crate::world::layer_of(10));
        assert_eq!(m.view(), Some(0));
        // It follows the player down to Y -40.
        m.track(crate::world::layer_of(-40));
        assert_eq!(m.view(), Some(-3));
        // Out in the open: the surface.
        m.open(false, crate::world::layer_of(70));
        assert_eq!(m.view(), None);
        m.track(2);
        assert_eq!(m.view(), None);
    }

    #[test]
    fn switching_and_stepping_layers_changes_the_view() {
        let mut m = Ui::default().map;
        m.open(false, 4);
        // C (or the switch) into the caves: the player's layer.
        m.set_cave(true, 4);
        assert_eq!(m.view(), Some(4));
        // Up and down a layer by hand: a new view each time, no longer following.
        m.step_layer(true, 4);
        assert_eq!(m.view(), Some(5));
        assert!(!m.follow_layer);
        m.track(2);
        assert_eq!(m.view(), Some(5));
        m.step_layer(false, 4);
        m.step_layer(false, 4);
        assert_eq!(m.view(), Some(3));
        // Back on the player's own layer it follows them again.
        m.step_layer(true, 4);
        assert!(m.follow_layer);
        // Back to the surface and into the caves again: the player's layer.
        m.set_cave(false, 4);
        assert_eq!(m.view(), None);
        m.step_layer(true, 7);
        assert_eq!(m.view(), Some(7));
        // Never past the dimension's lowest or highest layer.
        m.bounds = (-4, 19);
        for _ in 0..40 {
            m.step_layer(false, 7);
        }
        assert_eq!(m.view(), Some(-4));
        for _ in 0..40 {
            m.step_layer(true, 7);
        }
        assert_eq!(m.view(), Some(19));
    }

    #[test]
    fn layer_controls_and_keys() {
        let w = 400.0;
        // The layer buttons sit under the switch and answer only in a cave view.
        let [dx, dy, bw, bh] = layer_button_rect(w, false);
        let [ux, uy, ..] = layer_button_rect(w, true);
        let [_, sy, _, sh] = mode_switch_rect(w);
        assert!(dy >= sy + sh && uy == dy && ux > dx + bw - 1.0);
        assert_eq!(
            layer_controls_hit(w, true, dx + 2.0, dy + bh / 2.0),
            Some(LayerHit::Down)
        );
        assert_eq!(
            layer_controls_hit(w, true, ux + 2.0, uy + 2.0),
            Some(LayerHit::Up)
        );
        assert_eq!(layer_controls_hit(w, false, ux + 2.0, uy + 2.0), None);
        let [x, y, sw, _] = mode_switch_rect(w);
        assert_eq!(
            layer_controls_hit(w, false, x + 1.0, y + 1.0),
            Some(LayerHit::Surface)
        );
        assert_eq!(
            layer_controls_hit(w, true, x + sw - 1.0, y + 1.0),
            Some(LayerHit::Cave)
        );
        assert_eq!(layer_key("key.keyboard.right.bracket"), Some(true));
        assert_eq!(layer_key("key.keyboard.page.up"), Some(true));
        assert_eq!(layer_key("key.keyboard.left.bracket"), Some(false));
        assert_eq!(layer_key("key.keyboard.page.down"), Some(false));
        assert_eq!(layer_key("key.keyboard.c"), None);
        assert_eq!(layer_name("Surface", "Cave", None), "Surface");
        assert_eq!(
            layer_name("Surface", "Cave", Some(3)),
            "Cave \u{b7} Y 48\u{2013}63"
        );
        assert_eq!(
            layer_name("Surface", "Cave", Some(-3)),
            "Cave \u{b7} Y -48\u{2013}-33"
        );
    }
}
