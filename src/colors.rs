//! Each block's map colour, worked out from what it looks like: the average of the pixels of
//! the texture on its top face (from the resource packs in use, so a pack's look carries over
//! to the map), weighted by how opaque each pixel is. Faces the game tints by biome (grass,
//! leaves, water) are tinted per column when a column is read.

use crate::png;
use mod_api::{BlockState, ClientHost, HostBlockStates, Identifier, PackType};
use std::collections::HashMap;

/// How a block shows on the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Drawn with its colour.
    Solid,
    /// Water, or a plant that only grows in it: counted into the water's depth.
    Water,
    /// Something the game's own maps leave out (air, glass, torches, light blocks): looked
    /// through to what lies below.
    Clear,
}

/// What a block over the player's head counts as, for cave mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cover {
    /// Air, water, torches, plants: no roof.
    Open,
    /// Leaves: a tree's canopy is not a roof.
    Foliage,
    /// Glass of every kind (plain, stained, tinted, panes): a roof you can see through.
    Glass,
    /// Any other block: a roof.
    Roof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub rgb: u32,
    /// The tint index of the top face, -1 for none.
    pub tint: i32,
    pub role: Role,
    pub cover: Cover,
}

/// Whether a block id's path (`glass`, `red_stained_glass`, `tinted_glass`, `glass_pane`,
/// `lime_stained_glass_pane`, a mod's `framed_glass`) is glass.
pub fn is_glass(path: &str) -> bool {
    let path = path.rsplit(':').next().unwrap_or(path);
    path == "glass" || path.ends_with("_glass") || path.ends_with("glass_pane")
}

/// How a block with id path `path` and map role `role` counts over the player's head.
pub fn cover_of(path: &str, role: Role) -> Cover {
    let path = path.rsplit(':').next().unwrap_or(path);
    if is_glass(path) {
        Cover::Glass
    } else if role != Role::Solid {
        Cover::Open
    } else if path.ends_with("leaves") {
        Cover::Foliage
    } else {
        Cover::Roof
    }
}

/// Used when a block's texture can't be read.
const FALLBACK: u32 = 0x7F7A72;
/// Water's colour before its biome tint, when its texture can't be read.
const WATER_FALLBACK: u32 = 0xB0B0B0;

#[derive(Default)]
pub struct BlockColors {
    looks: HashMap<BlockState, Look>,
    textures: HashMap<String, Option<u32>>,
    version: u64,
}

impl BlockColors {
    /// Forgets everything when the resource packs changed.
    pub fn check_version(&mut self, host: &dyn ClientHost) {
        let v = host.block_sprites_version();
        if v != self.version {
            self.version = v;
            self.looks.clear();
            self.textures.clear();
        }
    }

    pub fn look(&mut self, host: &dyn ClientHost, state: BlockState) -> Look {
        if let Some(l) = self.looks.get(&state) {
            return *l;
        }
        let mut l = self.work_out(host, state);
        if let Some(id) = state.block().id(host) {
            l.cover = cover_of(id, l.role);
        }
        self.looks.insert(state, l);
        l
    }

    fn work_out(&mut self, host: &dyn ClientHost, state: BlockState) -> Look {
        let id = state.block().id(host).unwrap_or("");
        let path = id.split_once(':').map_or(id, |(_, p)| p);
        let water = matches!(
            path,
            "water" | "bubble_column" | "kelp" | "kelp_plant" | "seagrass" | "tall_seagrass"
        );
        if water {
            let rgb = self
                .texture(host, "minecraft:block/water_still")
                .unwrap_or(WATER_FALLBACK);
            return Look {
                rgb,
                tint: 0,
                role: Role::Water,
                cover: Cover::Open,
            };
        }
        if path == "lava" {
            let rgb = self
                .texture(host, "minecraft:block/lava_still")
                .unwrap_or(0xD45A12);
            return Look {
                rgb,
                tint: -1,
                role: Role::Solid,
                cover: Cover::Roof,
            };
        }
        // What the game's maps leave out, we look through too.
        if host.block_states().map_color(state).unwrap_or(0) == 0 {
            return Look {
                rgb: 0,
                tint: -1,
                role: Role::Clear,
                cover: Cover::Open,
            };
        }
        let sprites = host.block_sprites(&[state]).into_iter().next().flatten();
        // The top side's faces, base first; a block with none there (a flower's crossed
        // planes) uses its own name's texture.
        let top = sprites.as_ref().and_then(|s| s.sides[1].first().cloned());
        let (texture, tint) = match top {
            Some(face) => (face.texture.clone(), face.tint_index),
            None => {
                let (ns, p) = id.split_once(':').unwrap_or(("minecraft", id));
                (format!("{ns}:block/{p}"), 0)
            }
        };
        let rgb = self.texture(host, &texture).unwrap_or(FALLBACK);
        Look {
            rgb,
            tint,
            role: Role::Solid,
            cover: Cover::Roof,
        }
    }

    /// The average colour of texture `name` (`block/stone`, `minecraft:block/stone` or a mod's
    /// `mymod:block/x`), None when it can't be read.
    fn texture(&mut self, host: &dyn ClientHost, name: &str) -> Option<u32> {
        if let Some(c) = self.textures.get(name) {
            return *c;
        }
        let (ns, path) = name.split_once(':').unwrap_or(("minecraft", name));
        let c = Identifier::parse(&format!("{ns}:textures/{path}.png"))
            .ok()
            .and_then(|id| read_pack_texture(host, &id))
            .and_then(|img| average(&img.rgba));
        self.textures.insert(name.to_owned(), c);
        c
    }
}

/// Bytes of a client texture from the winning resource pack.
fn read_pack_texture(host: &dyn ClientHost, id: &Identifier) -> Option<crate::png::Image> {
    use std::io::Read;
    let mut reader = host
        .open_resource(PackType::ClientResources, id)
        .ok()
        .flatten()?;
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).ok()?;
    png::decode(&bytes)
}

/// The colour of a picture's pixels, each counted by its opacity; None for a fully clear one.
pub fn average(rgba: &[u8]) -> Option<u32> {
    let (mut r, mut g, mut b, mut w) = (0f64, 0f64, 0f64, 0f64);
    for p in rgba.chunks_exact(4) {
        let a = f64::from(p[3]) / 255.0;
        if a < 0.1 {
            continue;
        }
        r += f64::from(p[0]) * a;
        g += f64::from(p[1]) * a;
        b += f64::from(p[2]) * a;
        w += a;
    }
    if w <= 0.0 {
        return None;
    }
    let q = |v: f64| ((v / w).round() as u32).min(255);
    Some((q(r) << 16) | (q(g) << 8) | q(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn averages_count_only_what_shows() {
        let px = [255, 0, 0, 255, 0, 0, 255, 255, 9, 9, 9, 0];
        assert_eq!(average(&px), Some(0x80_0080));
        assert_eq!(average(&[1, 2, 3, 0]), None);
    }

    #[test]
    fn every_glass_is_a_see_through_roof() {
        for id in [
            "minecraft:glass",
            "minecraft:tinted_glass",
            "minecraft:white_stained_glass",
            "minecraft:light_blue_stained_glass",
            "minecraft:glass_pane",
            "minecraft:black_stained_glass_pane",
        ] {
            assert!(is_glass(id), "{id}");
            // Plain glass has no map colour (Clear); stained and tinted glass have one (Solid).
            assert_eq!(cover_of(id, Role::Clear), Cover::Glass, "{id}");
            assert_eq!(cover_of(id, Role::Solid), Cover::Glass, "{id}");
        }
        for id in [
            "minecraft:glowstone",
            "minecraft:spyglass",
            "minecraft:stone",
            "minecraft:sand",
        ] {
            assert!(!is_glass(id), "{id}");
        }
        assert_eq!(cover_of("minecraft:stone", Role::Solid), Cover::Roof);
        assert_eq!(
            cover_of("minecraft:oak_leaves", Role::Solid),
            Cover::Foliage
        );
        assert_eq!(cover_of("minecraft:air", Role::Clear), Cover::Open);
        assert_eq!(cover_of("minecraft:water", Role::Water), Cover::Open);
    }

    #[test]
    fn decodes_its_own_png() {
        let rgba: Vec<u8> = (0..16 * 16)
            .flat_map(|i| [i as u8, 255 - i as u8, 7, 255])
            .collect();
        let file = crate::png::encode_stored(16, 16, &rgba);
        let img = crate::png::decode(&file).unwrap();
        assert_eq!((img.width, img.height), (16, 16));
        assert_eq!(img.rgba, rgba);
    }
}
