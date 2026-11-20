//! Creature faces for the Faces dot style: each face is 8 x 8 texels in one dynamic texture
//! of 16 x 16 faces, made at run time from the creature's model and the texture the game
//! draws it with ([`crate::headshot`]), so it follows its variant and the resource packs in
//! use. Players' heads are drawn by the game itself, as chat heads are. Nothing is shipped: a
//! creature whose face can't be made keeps its dot.

use crate::png;
use crate::radar::{Dot, Face};
use mod_api::{ClientHost, Identifier, PackType};
use std::collections::{HashMap, HashSet};

pub const FACES_TEX: &str = "frontier:dynamic/faces";
/// Faces across (and down) the texture.
const PER_ROW: usize = 16;
const SIDE: usize = PER_ROW * 8;

#[derive(Default)]
pub struct Faces {
    slots: HashMap<String, usize>,
    /// Faces that couldn't be made, not tried again until the packs change.
    failed: HashSet<String>,
    /// Textures read, decoded: width, height and RGBA.
    images: HashMap<String, Option<(u32, u32, Vec<u8>)>>,
    pixels: Vec<u8>,
    dirty: bool,
    made: bool,
    broken: bool,
    version: u64,
}

impl Faces {
    /// Where face `key` is in the texture (`[x, y]` in texels), once it has been made.
    pub fn slot(&self, face: &Face) -> Option<[f32; 2]> {
        if !self.made {
            return None;
        }
        let i = *self.slots.get(&face.key()?)?;
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
            self.dirty = true;
        }
        if self.pixels.is_empty() {
            self.pixels = vec![0; SIDE * SIDE * 4];
        }
        let mut new = 0;
        for d in dots {
            let Some(key) = d.face.key() else { continue };
            if self.slots.contains_key(&key) || self.failed.contains(&key) {
                continue;
            }
            // A few a frame.
            if new >= 8 {
                break;
            }
            new += 1;
            match self.make(host, &d.face) {
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
            let _ = host.free_dynamic_texture(&Identifier::parse(FACES_TEX).expect("valid id"));
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
                .and_then(|id| host.resource(PackType::ClientResources, &id))
                .and_then(|bytes| png::decode(&bytes))
                .map(|i| (i.width, i.height, i.rgba));
            self.images.insert(path.to_owned(), img);
        }
        self.images.get(path)?.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
