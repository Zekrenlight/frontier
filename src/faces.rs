//! Creature faces for the Faces dot style: each face is 8 x 8 texels in one dynamic texture
//! of 16 x 16 faces, made at run time from the creature's model and the texture the game
//! draws it with ([`crate::headshot`]), so it follows its variant and the resource packs in
//! use. Players' heads are drawn by the game itself, as chat heads are. Nothing is shipped: a
//! creature whose face can't be made keeps its dot.
//!
//! Before any of that, a marker image wins: `assets/<namespace>/textures/frontier/markers/
//! <path>.png` for entity type `<namespace>:<path>` (see [`marker_path`]), from a mod's own
//! assets or a resource pack (a pack's wins over a mod's, as for any texture). It is how a
//! mod whose creature is drawn by its own renderer, over a vanilla type it copies, gives the
//! map its real look; a pack can replace a vanilla creature's face the same way.

use crate::png;
use crate::radar::{Dot, Face};
use mod_api::{ClientHost, Identifier, PackType};
use std::collections::{HashMap, HashSet};

pub const FACES_TEX: &str = "frontier:dynamic/faces";
/// Faces across (and down) the texture.
const PER_ROW: usize = 16;
const SIDE: usize = PER_ROW * 8;
/// Whole faces atlas side in texels (16 faces of 8 px).
pub const FACES_SIDE: i32 = SIDE as i32;

#[derive(Default)]
pub struct Faces {
    slots: HashMap<String, usize>,
    /// Faces that couldn't be made, not tried again until the packs change.
    failed: HashSet<String>,
    /// Entity types looked up for a marker image since the packs last changed: whether one
    /// was found.
    markers: HashMap<String, bool>,
    /// Textures read, decoded: width, height and RGBA.
    images: HashMap<String, Option<(u32, u32, Vec<u8>)>>,
    pixels: Vec<u8>,
    dirty: bool,
    made: bool,
    broken: bool,
    version: u64,
}

fn read_pack_png(host: &dyn ClientHost, id: &Identifier) -> Option<crate::png::Image> {
    use std::io::Read;
    let mut reader = host
        .open_resource(mod_api::PackType::ClientResources, id)
        .ok()
        .flatten()?;
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).ok()?;
    crate::png::decode(&bytes)
}


impl Faces {
    /// Where face `key` is in the texture (`[x, y]` in texels), once it has been made.
    pub fn slot(&self, face: &Face) -> Option<[f32; 2]> {
        if !self.made {
            return None;
        }
        if let Face::Marker { entity, then } = face {
            return match self.markers.get(entity) {
                Some(true) => self.slot_of(&face.key()?),
                Some(false) => self.slot(then),
                None => None,
            };
        }
        self.slot_of(&face.key()?)
    }

    fn slot_of(&self, key: &str) -> Option<[f32; 2]> {
        let i = *self.slots.get(key)?;
        Some([((i % PER_ROW) * 8) as f32, ((i / PER_ROW) * 8) as f32])
    }

    /// Makes the faces of `dots` not made yet and uploads the texture when it changed.
    pub fn prepare(&mut self, host: &mut dyn ClientHost, dots: &[Dot]) {
        if self.broken {
            return;
        }
        let v = host.block_sprites_version();
        if v != self.version {
            // The packs changed: every face again.
            self.version = v;
            self.slots.clear();
            self.failed.clear();
            self.images.clear();
            self.markers.clear();
            self.dirty = true;
        }
        if self.pixels.is_empty() {
            self.pixels = vec![0; SIDE * SIDE * 4];
        }
        let mut new = 0;
        for d in dots {
            // A marker image first: looked up once per entity type (and pack change).
            let face = match &d.face {
                Face::Marker { entity, then } => {
                    let has = match self.markers.get(entity) {
                        Some(&has) => has,
                        None => {
                            let has = self.image(host, &marker_path(entity)).is_some();
                            self.markers.insert(entity.clone(), has);
                            has
                        }
                    };
                    if has { &d.face } else { &**then }
                }
                f => f,
            };
            let Some(key) = face.key() else { continue };
            if self.slots.contains_key(&key) || self.failed.contains(&key) {
                continue;
            }
            // A few a frame.
            if new >= 8 {
                break;
            }
            new += 1;
            match self.make(host, face) {
                Some(rgba) => {
                    if self.slots.len() >= PER_ROW * PER_ROW {
                        // Full: start again with the faces on show now.
                        self.slots.clear();
                    }
                    let i = self.slots.len();
                    self.put(i, &rgba);
                    self.slots.insert(key, i);
                    self.dirty = true;
                }
                None => {
                    self.failed.insert(key);
                }
            }
        }
        if self.dirty {
            let id = Identifier::parse(FACES_TEX).expect("valid id");
            let s = SIDE as u32;
            let ok = if self.made {
                host.update_dynamic_texture(&id, s, s, &self.pixels).is_ok()
            } else {
                host.create_dynamic_texture(&id, s, s, &self.pixels)
                    .or_else(|_| host.update_dynamic_texture(&id, s, s, &self.pixels))
                    .is_ok()
            };
            self.made = ok;
            self.broken = !ok;
            self.dirty = false;
        }
    }

    /// Lets the texture go (the maps stopped showing faces).
    pub fn free(&mut self, host: &mut dyn ClientHost) {
        if self.made {
            let _ = host.release_dynamic_texture(&Identifier::parse(FACES_TEX).expect("valid id"));
        }
        *self = Faces {
            broken: self.broken,
            ..Faces::default()
        };
    }

    fn put(&mut self, i: usize, rgba: &[u8]) {
        let (ox, oy) = ((i % PER_ROW) * 8, (i / PER_ROW) * 8);
        for y in 0..8 {
            for x in 0..8 {
                let s = (y * 8 + x) * 4;
                let d = ((oy + y) * SIDE + ox + x) * 4;
                self.pixels[d..d + 4].copy_from_slice(&rgba[s..s + 4]);
            }
        }
    }

    fn make(&mut self, host: &dyn ClientHost, face: &Face) -> Option<Vec<u8>> {
        match face {
            Face::None | Face::Player { .. } => None,
            Face::Marker { entity, .. } => {
                let (w, h, img) = self.image(host, &marker_path(entity))?;
                Some(shrink_to_8(*w, *h, img))
            }
            Face::Model { path, plan, or, .. } => {
                let first = self
                    .image(host, path)
                    .and_then(|(w, h, img)| crate::headshot::paint(plan, *w, *h, img));
                first.or_else(|| {
                    let (path, plan) = or.as_ref()?;
                    let (w, h, img) = self.image(host, path)?;
                    crate::headshot::paint(plan, *w, *h, img)
                })
            }
        }
    }

    fn image(&mut self, host: &dyn ClientHost, path: &str) -> Option<&(u32, u32, Vec<u8>)> {
        if !self.images.contains_key(path) {
            let (ns, p) = path.split_once(':').unwrap_or(("minecraft", path));
            let p = p.strip_prefix("textures/").unwrap_or(p);
            let p = p.strip_suffix(".png").unwrap_or(p);
            let img = Identifier::parse(&format!("{ns}:textures/{p}.png"))
                .ok()
                .and_then(|id| read_pack_png(host, &id))
                .map(|i| (i.width, i.height, i.rgba));
            self.images.insert(path.to_owned(), img);
        }
        self.images.get(path)?.as_ref()
    }
}

/// Where entity type `entity`'s marker image is looked for: `<namespace>:frontier/markers/
/// <path>` under `textures/`, so `vulture:vulture` is `assets/vulture/textures/frontier/
/// markers/vulture.png` and `minecraft:cow` is `assets/minecraft/textures/frontier/markers/
/// cow.png`.
pub fn marker_path(entity: &str) -> String {
    let (ns, path) = entity.split_once(':').unwrap_or(("minecraft", entity));
    format!("{ns}:frontier/markers/{path}")
}

/// A marker image of any size as the 8 x 8 a face slot holds: each texel the average of the
/// texels it covers, weighted by how opaque they are (so a transparent edge doesn't darken
/// it); an 8 x 8 image is kept as it is.
pub fn shrink_to_8(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let (w, h) = (w.max(1) as usize, h.max(1) as usize);
    let mut out = vec![0u8; 8 * 8 * 4];
    for oy in 0..8 {
        for ox in 0..8 {
            let (x0, x1) = (ox * w / 8, ((ox + 1) * w / 8).max(ox * w / 8 + 1).min(w));
            let (y0, y1) = (oy * h / 8, ((oy + 1) * h / 8).max(oy * h / 8 + 1).min(h));
            let (mut r, mut g, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * w + x) * 4;
                    let Some(p) = rgba.get(i..i + 4) else { continue };
                    let pa = u64::from(p[3]);
                    r += u64::from(p[0]) * pa;
                    g += u64::from(p[1]) * pa;
                    b += u64::from(p[2]) * pa;
                    a += pa;
                    n += 1;
                }
            }
            let o = (oy * 8 + ox) * 4;
            if a > 0 {
                out[o] = (r / a) as u8;
                out[o + 1] = (g / a) as u8;
                out[o + 2] = (b / a) as u8;
                out[o + 3] = (a / n) as u8;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_images_are_found_by_entity_type_and_shrunk_to_a_face() {
        assert_eq!(marker_path("vulture:vulture"), "vulture:frontier/markers/vulture");
        assert_eq!(marker_path("cow"), "minecraft:frontier/markers/cow");
        assert_eq!(
            marker_path("mymod:beasts/wolf_king"),
            "mymod:frontier/markers/beasts/wolf_king"
        );
        // A 16 x 16: each 2 x 2 becomes one texel; a half transparent block keeps its colour.
        let mut img = vec![0u8; 16 * 16 * 4];
        for y in 0..16 {
            for x in 0..16 {
                let i = (y * 16 + x) * 4;
                img[i..i + 4].copy_from_slice(&[200, 100, 50, if x % 2 == 0 { 255 } else { 0 }]);
            }
        }
        let s = shrink_to_8(16, 16, &img);
        assert_eq!(s.len(), 8 * 8 * 4);
        assert_eq!(&s[0..4], &[200, 100, 50, 127]);
        // An 8 x 8 is kept texel for texel.
        let small: Vec<u8> = (0..8 * 8 * 4).map(|i| if i % 4 == 3 { 255 } else { (i % 251) as u8 }).collect();
        assert_eq!(shrink_to_8(8, 8, &small), small);
    }

    #[test]
    fn a_marker_falls_back_to_what_it_wraps() {
        let mut f = Faces::default();
        f.made = true;
        f.slots.insert("cow".into(), 3);
        let model = Face::Model {
            key: "cow".into(),
            path: "entity/cow/cow_temperate".into(),
            plan: std::sync::Arc::new(crate::headshot::Plan { pixels: Vec::new() }),
            or: None,
        };
        let m = Face::marker_or("minecraft:cow", model);
        assert_eq!(m.key().as_deref(), Some("marker|minecraft:cow"));
        // Not looked up yet: nothing; no marker: the model's face; a marker: its own slot.
        assert_eq!(f.slot(&m), None);
        f.markers.insert("minecraft:cow".into(), false);
        assert_eq!(f.slot(&m), Some([24.0, 0.0]));
        f.markers.insert("minecraft:cow".into(), true);
        f.slots.insert("marker|minecraft:cow".into(), 17);
        assert_eq!(f.slot(&m), Some([8.0, 8.0]));
        // A mod's creature drawn by its own renderer, with no marker: a dot, never the face of
        // the vanilla type it copies.
        let modded = Face::marker_or("vulture:vulture", Face::None);
        f.markers.insert("vulture:vulture".into(), false);
        assert_eq!(f.slot(&modded), None);
        // Players keep their heads.
        let p = Face::marker_or("minecraft:player", Face::Player { name: "Alex".into() });
        assert!(matches!(p, Face::Player { .. }));
    }

    #[test]
    fn faces_have_keys_and_players_and_none_have_none() {
        assert_eq!(Face::None.key(), None);
        assert_eq!(
            Face::Player {
                name: "Alex".into()
            }
            .key(),
            None
        );
        let plan = std::sync::Arc::new(crate::headshot::Plan { pixels: Vec::new() });
        let a = Face::Model {
            key: "cow".into(),
            path: "entity/cow/cow_temperate".into(),
            plan,
            or: None,
        };
        assert_eq!(a.key().as_deref(), Some("cow"));
        assert!(a.some() && !Face::None.some());
    }
}
