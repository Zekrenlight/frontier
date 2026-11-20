// Frontier in the game (not in the Cargo library): the settings page for the key and
// `/frontier settings`, the creatures, things, dropped items and marked blocks the maps show as dots, their
// faces, and two things the mod API doesn't tell a client mod: whether the player list key
// is held and whether the F3 screen is up.
use super::{Game, Screen};
use mod_frontier::radar::{self, Dot, Face, Kind, Wanted};
use sunrise_client::actions::Action;
use sunrise_client::options_ui::Page;
use winit::keyboard::KeyCode;

/// Once a frame, before it is drawn.
pub(super) fn frame(game: &mut Game<'_>) {
    inputs(game);
    nearby(game);
    if mod_frontier::take_open_settings() {
        open_settings(game);
    }
}

/// Tab and F3. The player list key is read from the game's own held keys (and the physical
/// Tab key while a screen such as the world map is open): the mod API's mapping query only
/// answers for movement and the mouse buttons, so it never saw Tab held.
fn inputs(game: &Game<'_>) {
    let tab = game.keys.held(Action::Playerlist) || game.physical_keys.contains(&KeyCode::Tab);
    let playing = game.playing() && game.container.is_none() && game.mod_screen.is_none();
    radar::set_inputs(tab, game.debug && playing);
}

/// Opens Frontier's page of Options > Mods....
fn open_settings(game: &mut Game<'_>) {
    game.list_native_mods();
    let index = game
        .settings
        .mods
        .installed
        .iter()
        .position(|entry| entry.id == mod_frontier::ID && entry.native.is_some());
    let Some(index) = index else { return };
    let from_play = game.screen == Screen::Playing;
    game.open_options_at(false, Page::NativeMod(index));
    // Opened from play (the key, or a command once chat has closed), the pointer is still
    // the game's: changing the screen doesn't let it go, so it is let go here, as every
    // screen opened from play does. Leaving the page goes back to the pause menu, as
    // Options does in a world, so that menu is the full one.
    if from_play || game.screen == Screen::Options {
        game.mining = None;
        game.pause_menu = true;
        game.capture(false);
    }
}

/// Everything near the player the mod asked for this tick: the game's own copies of what
/// the server shows (other players are among the creatures, with their names), skipping the
/// dead, the removed and the invisible.
fn nearby(game: &Game<'_>) {
    let Some(w) = radar::wanted() else { return };
    let mut dots = Vec::new();
    let c = w.centre;
    let r2 = w.radius * w.radius;
    let near = |x: f64, z: f64| {
        let (dx, dz) = (x - c[0], z - c[2]);
        dx * dx + dz * dz <= r2
    };
    if w.creatures {
        let players: std::collections::HashMap<u32, String> = game.remote_player_names().into_iter().collect();
        for m in game.mobs.iter() {
            if m.removed || !m.alive() || m.is_invisible() {
                continue;
            }
            let p = m.position;
            if !near(p.x, p.z) {
                continue;
            }
            let (kind, label, named) = match players.get(&m.id) {
                Some(name) => (Kind::Player, name.clone(), true),
                None => {
                    let kind = radar::kind_of(m.kind.id.as_str(), m.kind.category.as_str());
                    match &m.custom_name {
                        Some(name) if !name.is_empty() => (kind, name.clone(), true),
                        _ => (kind, m.kind.id.clone(), false),
                    }
                }
            };
            let face = match (&kind, w.faces) {
                (_, false) => Face::None,
                (Kind::Player, true) => Face::Player { name: label.clone() },
                _ => face_of(game, m),
            };
            dots.push(Dot::new([p.x, p.y, p.z], kind, label, named, face));
            if dots.len() >= 384 {
                break;
            }
        }
    }
    if w.objects {
        let mut add = |pos: [f64; 3], id: &str| {
            if near(pos[0], pos[2]) && dots.len() < 640 {
                dots.push(Dot::new(pos, radar::kind_of_thing(id), id.to_owned(), false, Face::None));
            }
        };
        let block = |p: [i32; 3]| [f64::from(p[0]) + 0.5, f64::from(p[1]), f64::from(p[2]) + 0.5];
        for f in game.item_frames.iter() {
            add(block(f.pos), if f.glow { "minecraft:glow_item_frame" } else { "minecraft:item_frame" });
        }
        for p in game.paintings.iter() {
            add(block(p.pos), "minecraft:painting");
        }
        for s in game.armor_stands.iter() {
            add([s.position.x, s.position.y, s.position.z], "minecraft:armor_stand");
        }
        for s in game.cushions.iter() {
            add([s.position.x, s.position.y, s.position.z], "minecraft:cushion");
        }
        for b in game.boats.iter() {
            add([b.position.x, b.position.y, b.position.z], "minecraft:boat");
        }
        for m in game.carts.iter() {
            add([m.position.x, m.position.y, m.position.z], "minecraft:minecart");
        }
    }
    // Dropped items: black dots of their own, apart from the other things, each with its
    // stack's item, count and name as the game shows it (`ItemStack.getHoverName`: a custom
    // name when renamed, else the item's name in the language in use).
    if w.items {
        for i in game.dropped.iter() {
            let pos = [i.position.x, i.position.y, i.position.z];
            if near(pos[0], pos[2]) && dots.len() < 896 {
                let name = sunrise_client::stations::hover_name(&game.pack.names, &i.stack);
                dots.push(Dot::dropped(pos, &i.stack.item, name, i.stack.count));
            }
        }
    }
    if w.containers || w.decor {
        let mut found: Vec<(f64, Dot)> = Vec::new();
        for (&(x, y, z), _) in game.world.block_entities.iter() {
            let pos = [f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5];
            if !near(pos[0], pos[2]) || (pos[1] - c[1]).abs() > w.block_height {
                continue;
            }
            let id = game.world.block(x, y, z);
            let Some(state) = game.pack.assets.registry.state(id) else { continue };
            let Some(kind) = radar::kind_of_block(&state.name) else { continue };
            if (kind == Kind::Container && !w.containers) || (kind == Kind::Decor && !w.decor) {
                continue;
            }
            let (dx, dy, dz) = (pos[0] - c[0], pos[1] - c[1], pos[2] - c[2]);
            let label = state.name.to_string();
            found.push((dx * dx + dy * dy + dz * dz, Dot::new(pos, kind, label, false, Face::None)));
        }
        // The nearest ones, when a base is full of chests.
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        dots.extend(found.into_iter().take(320).map(|(_, d)| d));
    }
    radar::put(dots);
}

/// A creature's face: its model, as the game draws it now (its variant's model and
/// texture), seen as [`mod_frontier::headshot`] sees it; and, where the resource packs give
/// it a custom model, that model with its texture to fall back on. Most packs' models keep
/// the head's boxes and texture where the game's model has them (and pose them only as they
/// move), so the game's model reads the face first; a pack that lays the texture out anew for
/// its own model (one goat) has it read through that model. The plan is made once a model.
fn face_of(game: &Game<'_>, m: &sunrise_client::mobs::Mob) -> Face {
    use mod_frontier::headshot;
    use std::sync::Arc;
    thread_local! {
        static PLANS: std::cell::RefCell<std::collections::HashMap<String, Option<Arc<headshot::Plan>>>> =
            Default::default();
    }
    let (model_key, texture) = sunrise_client::mobs::render::body(m);
    let id = m.kind.id.as_str();
    // A model's plan, under its address (a new pack's model is a new model).
    let planned = |model: &sunrise_client::mobs::Part| {
        let key = format!("{id}|{model_key}|{:p}", model as *const sunrise_client::mobs::Part);
        let plan = PLANS.with(|plans| {
            let mut plans = plans.borrow_mut();
            if plans.len() > 512 {
                plans.clear();
            }
            plans
                .entry(key.clone())
                .or_insert_with(|| headshot::plan(&node(model), &headshot::pick(id)).map(Arc::new))
                .clone()
        });
        plan.map(|p| (key, p))
    };
    let own = planned(m.kind.layer(&model_key).unwrap_or(&m.kind.model)).map(|(k, p)| (k, texture.to_string(), p));
    // The packs' model for this layer, as the body draws it (the player's own key aside).
    let cem = game.res.mob_data().cem.read().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
    let custom = cem.layer(if model_key == "main" { "player#main" } else { &model_key }).and_then(|c| {
        let (k, p) = planned(&c.model)?;
        Some((k, c.texture_for(&texture).map_or(texture.to_string(), str::to_owned), p))
    });
    match (own, custom) {
        (Some((key, path, plan)), custom) => {
            let or = custom.map(|(_, path, plan)| (path, plan));
            let key = match &or {
                Some((p, _)) => format!("{key}|{texture}|{p}"),
                None => format!("{key}|{texture}"),
            };
            Face::Model { key, path, plan, or }
        }
        (None, Some((key, path, plan))) => Face::Model { key: format!("{key}|{path}"), path, plan, or: None },
        (None, None) => Face::None,
    }
}

/// The game's model part as the face maker takes it.
fn node(part: &sunrise_client::mobs::Part) -> mod_frontier::headshot::Node {
    mod_frontier::headshot::Node {
        name: part.name.clone(),
        pose: part.pose,
        quads: part
            .polygons
            .iter()
            .map(|p| mod_frontier::headshot::Quad { v: p.vertices, normal: p.normal })
            .collect(),
        children: part.children.iter().map(node).collect(),
    }
}
