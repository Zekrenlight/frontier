//! Creature faces drawn from the creature's own model: the parts that make its face, seen
//! straight on (or from the side, for fish), each output pixel taking the texel of the
//! nearest face of the model that covers it. Only the texture the game draws the creature
//! with is sampled, at run time, so a face follows its variant and the resource packs; the
//! mod ships no art.
//!
//! [`plan`] works on the model alone and answers, for each of the face's 8 x 8 pixels, where
//! on the texture to look (up to three places, nearest first, for a hat or a see-through
//! front); [`paint`] then reads those from a texture.

/// The face's side in pixels.
pub const SIDE: usize = 8;
/// Places kept per pixel, nearest first: a clear texel shows the next one behind.
const DEPTH: usize = 3;

/// One face of a model box: four corners `[x, y, z, u, v]` (x, y, z in texels in the part's
/// own frame; u, v in fractions of the texture) and its outward normal.
#[derive(Clone, Debug)]
pub struct Quad {
    pub v: [[f32; 5]; 4],
    pub normal: [f32; 3],
}

/// A model part: its pose `[x, y, z, x_rot, y_rot, z_rot, x_scale, y_scale, z_scale]` (as
/// the game's model parts are posed: moved, turned about z, then y, then x, then scaled),
/// its faces and its children.
#[derive(Clone, Debug)]
pub struct Node {
    pub name: String,
    pub pose: [f32; 9],
    pub quads: Vec<Quad>,
    pub children: Vec<Node>,
}

/// Which way the face is seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// From in front (a model's front looks towards -z).
    Front,
    /// From the creature's left side: fish, whose faces are their sides.
    Side,
}

/// What makes a creature's face: the parts drawn (with the parts under them), the parts the
/// picture is framed on (their own faces only), and the view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pick {
    pub parts: &'static [&'static str],
    pub frame: &'static [&'static str],
    pub view: View,
    /// Framed on the nearest of the bigger faces, for a head with bigger parts beside and
    /// behind it (the creaking's branches).
    pub near: bool,
}

const fn side(parts: &'static [&'static str], frame: &'static [&'static str]) -> Pick {
    Pick {
        parts,
        frame,
        view: View::Side,
        near: false,
    }
}

const fn front(parts: &'static [&'static str], frame: &'static [&'static str]) -> Pick {
    Pick {
        parts,
        frame,
        view: View::Front,
        near: false,
    }
}

/// The head and its outer layer, framed on the head: most creatures.
const HEAD: Pick = front(&["head", "hat"], &["head"]);

/// Every creature of the game (26.3) with where its face is. A creature whose model names
/// its head `head` uses [`HEAD`]; the rest say which parts make their face.
pub const MOBS: &[(&str, Pick)] = &[
    ("allay", HEAD),
    (
        "armadillo",
        front(
            &["head_cube", "right_ear_cube", "left_ear_cube"],
            &["head_cube"],
        ),
    ),
    ("axolotl", HEAD),
    ("bat", HEAD),
    (
        "bee",
        front(&["body", "right_antenna", "left_antenna"], &["body"]),
    ),
    ("blaze", HEAD),
    ("bogged", front(&["head", "hat", "mushrooms"], &["head"])),
    ("breeze", front(&["head", "eyes"], &["head"])),
    ("camel", side(&["head"], &["head"])),
    ("camel_husk", side(&["head"], &["head"])),
    ("cat", HEAD),
    ("cave_spider", HEAD),
    ("chicken", front(&["head", "beak", "red_thing"], &["head"])),
    ("cod", front(&["head", "nose", "body"], &["head"])),
    ("copper_golem", HEAD),
    ("cow", HEAD),
    ("creaking", Pick { near: true, ..HEAD }),
    ("creeper", HEAD),
    ("dolphin", HEAD),
    ("donkey", side(&["head_parts"], &["head"])),
    ("drowned", HEAD),
    ("elder_guardian", HEAD),
    ("ender_dragon", Pick { near: true, ..HEAD }),
    ("enderman", HEAD),
    ("endermite", front(&["segment0"], &["segment0"])),
    ("evoker", HEAD),
    ("fox", HEAD),
    ("frog", front(&["head", "eyes"], &["head"])),
    ("ghast", front(&["body"], &["body"])),
    ("giant", HEAD),
    ("glow_squid", front(&["body"], &["body"])),
    ("goat", side(&["head"], &["head"])),
    ("guardian", HEAD),
    ("happy_ghast", front(&["body"], &["body"])),
    ("hoglin", HEAD),
    ("horse", side(&["head_parts"], &["head"])),
    ("husk", HEAD),
    ("illusioner", HEAD),
    ("iron_golem", HEAD),
    ("llama", Pick { near: true, ..HEAD }),
    (
        "magma_cube",
        front(
            &[
                "cube0",
                "cube1",
                "cube2",
                "cube3",
                "cube4",
                "cube5",
                "cube6",
                "cube7",
                "inside_cube",
            ],
            &[
                "cube0", "cube1", "cube2", "cube3", "cube4", "cube5", "cube6", "cube7",
            ],
        ),
    ),
    ("mannequin", HEAD),
    ("mooshroom", HEAD),
    ("mule", side(&["head_parts"], &["head"])),
    (
        "nautilus",
        front(
            &["body", "upper_mouth", "lower_mouth", "inner_mouth"],
            &["body"],
        ),
    ),
    ("ocelot", HEAD),
    ("panda", HEAD),
    ("parched", HEAD),
    ("parrot", HEAD),
    ("phantom", HEAD),
    ("pig", HEAD),
    ("piglin", HEAD),
    ("piglin_brute", HEAD),
    ("pillager", HEAD),
    ("player", HEAD),
    ("polar_bear", HEAD),
    (
        "pufferfish",
        front(&["body", "right_eye", "left_eye"], &["body"]),
    ),
    (
        "rabbit",
        front(&["head", "right_ear", "left_ear"], &["head"]),
    ),
    ("ravager", HEAD),
    ("salmon", side(&["head", "body_front"], &["head"])),
    ("sheep", HEAD),
    ("shulker", HEAD),
    ("silverfish", front(&["segment0"], &["segment0"])),
    ("skeleton", HEAD),
    ("skeleton_horse", side(&["head_parts"], &["head"])),
    (
        "slime",
        front(&["cube", "right_eye", "left_eye", "mouth"], &["cube"]),
    ),
    ("sniffer", HEAD),
    ("snow_golem", HEAD),
    ("spider", HEAD),
    ("squid", front(&["body"], &["body"])),
    ("stray", HEAD),
    ("strider", front(&["body"], &["body"])),
    ("sulfur_cube", front(&["cube"], &["cube"])),
    ("tadpole", front(&["body"], &["body"])),
    ("trader_llama", Pick { near: true, ..HEAD }),
    ("tropical_fish", side(&["body"], &["body"])),
    ("turtle", HEAD),
    ("vex", HEAD),
    ("villager", HEAD),
    ("vindicator", HEAD),
    ("wandering_trader", HEAD),
    ("warden", HEAD),
    ("witch", HEAD),
    ("wither", front(&["center_head"], &["center_head"])),
    ("wither_skeleton", HEAD),
    ("wolf", front(&["real_head"], &["real_head"])),
    ("zoglin", HEAD),
    ("zombie", HEAD),
    ("zombie_horse", side(&["head_parts"], &["head"])),
    (
        "zombie_nautilus",
        front(
            &["body", "upper_mouth", "lower_mouth", "inner_mouth"],
            &["body"],
        ),
    ),
    ("zombie_villager", HEAD),
    ("zombified_piglin", HEAD),
];

/// Where `entity`'s face is (`minecraft:cow` or `cow`); a creature of another mod tries a
/// part named `head`.
pub fn pick(entity: &str) -> Pick {
    let id = entity.strip_prefix("minecraft:").unwrap_or(entity);
    MOBS.iter()
        .find(|(m, _)| *m == id)
        .map_or(HEAD, |(_, p)| *p)
}

/// For each of the face's pixels, row by row, the texture places to try, nearest first.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub pixels: Vec<[Option<[f32; 2]>; DEPTH]>,
}

type Mat = [[f32; 3]; 3];

fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut m = [[0.0; 3]; 3];
    for (r, row) in m.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    m
}

fn apply(m: &Mat, v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// Turned about z, then y, then x (the x turn applied to the point first).
fn rotation(x: f32, y: f32, z: f32) -> Mat {
    let (sx, cx) = x.sin_cos();
    let (sy, cy) = y.sin_cos();
    let (sz, cz) = z.sin_cos();
    let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    mul(&mul(&rz, &ry), &rx)
}

/// A face turned into the view: its corners on screen `[x, y, depth]` (depth grows away from
/// the viewer) with their texture places.
struct Seen {
    p: [[f32; 3]; 4],
    uv: [[f32; 2]; 4],
}

/// Every face of `node` and the parts under it that looks at the viewer, in view space;
/// `framed` gathers those of the parts in `frame` (their own faces only).
#[allow(clippy::too_many_arguments)]
fn gather(
    node: &Node,
    m: &Mat,
    t: [f32; 3],
    view: &Mat,
    take: bool,
    pick: &Pick,
    framing: bool,
    out: &mut Vec<Seen>,
    framed: &mut Vec<[[f32; 3]; 4]>,
) {
    let p = node.pose;
    let here = apply(m, [p[0], p[1], p[2]]);
    let t = [t[0] + here[0], t[1] + here[1], t[2] + here[2]];
    let scale = [[p[6], 0.0, 0.0], [0.0, p[7], 0.0], [0.0, 0.0, p[8]]];
    let m = mul(&mul(m, &rotation(p[3], p[4], p[5])), &scale);
    let take = take || pick.parts.contains(&node.name.as_str());
    // A resource pack's model (`.jem`) keeps the vanilla parts by name but moves their boxes
    // into unnamed bones (`cem#<n>`) under them: those are the part's own faces too.
    let frames =
        pick.frame.contains(&node.name.as_str()) || (framing && node.name.starts_with("cem#"));
    if take {
        let full = mul(view, &m);
        for q in &node.quads {
            let n = apply(
                &mul(
                    view,
                    &mul(&m, &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
                ),
                q.normal,
            );
            // Facing the viewer (who looks along +z): its normal points back at them.
            if n[2] > -0.2 {
                continue;
            }
            let tv = apply(view, t);
            let mut s = Seen {
                p: [[0.0; 3]; 4],
                uv: [[0.0; 2]; 4],
            };
            for (i, c) in q.v.iter().enumerate() {
                let w = apply(&full, [c[0], c[1], c[2]]);
                s.p[i] = [w[0] + tv[0], w[1] + tv[1], w[2] + tv[2]];
                s.uv[i] = [c[3], c[4]];
            }
            if frames {
                framed.push(s.p);
            }
            out.push(s);
        }
    }
    for c in &node.children {
        gather(c, &m, t, view, take, pick, frames, out, framed);
    }
}

/// Where `pick`'s face is on `root`'s texture, pixel by pixel; None when the model has no
/// face at all. A model whose named parts draw nothing (a resource pack's model that moved
/// the head's boxes elsewhere, as one goat does under its body) is framed on its
/// [`front_part`] instead.
pub fn plan(root: &Node, pick: &Pick) -> Option<Plan> {
    plan_named(root, pick).or_else(|| {
        let mut named = root.clone();
        let at = front_part(&named)?;
        part_at(&mut named, &at).name = "frontier#face".into();
        plan_named(
            &named,
            &Pick {
                parts: &["frontier#face"],
                frame: &["frontier#face"],
                view: pick.view,
                near: pick.near,
            },
        )
    })
}

/// The part path (child indices from the root) of the face of a model whose parts have no
/// usable names: the nearest part above the model's frontmost corner (its front looks
/// towards -z) that holds at least two boxes and not the whole model.
pub fn front_part(root: &Node) -> Option<Vec<usize>> {
    /// Every corner in model space, with the path of the part it is on.
    fn walk(
        node: &Node,
        m: &Mat,
        t: [f32; 3],
        path: &mut Vec<usize>,
        out: &mut Vec<(Vec<usize>, [f32; 3])>,
    ) {
        let p = node.pose;
        let here = apply(m, [p[0], p[1], p[2]]);
        let t = [t[0] + here[0], t[1] + here[1], t[2] + here[2]];
        let scale = [[p[6], 0.0, 0.0], [0.0, p[7], 0.0], [0.0, 0.0, p[8]]];
        let m = mul(&mul(m, &rotation(p[3], p[4], p[5])), &scale);
        for q in &node.quads {
            for c in &q.v {
                let w = apply(&m, [c[0], c[1], c[2]]);
                out.push((path.clone(), [w[0] + t[0], w[1] + t[1], w[2] + t[2]]));
            }
        }
        for (i, c) in node.children.iter().enumerate() {
            path.push(i);
            walk(c, &m, t, path, out);
            path.pop();
        }
    }
    let id = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let mut corners = Vec::new();
    walk(root, &id, [0.0; 3], &mut Vec::new(), &mut corners);
    let (front, _) = corners.iter().min_by(|a, b| a.1[2].total_cmp(&b.1[2]))?;
    // The parts with faces at or under `at`.
    let parts = |at: &[usize]| {
        let mut seen: Vec<&Vec<usize>> = corners
            .iter()
            .map(|(p, _)| p)
            .filter(|p| p.starts_with(at))
            .collect();
        seen.dedup();
        seen.len()
    };
    let total = parts(&[]);
    // Up from the frontmost part until one holds two parts with faces, short of the whole.
    (0..=front.len())
        .rev()
        .map(|n| front[..n].to_vec())
        .find(|at| {
            let here = parts(at);
            here >= 2 && here < total
        })
}

fn part_at<'a>(node: &'a mut Node, path: &[usize]) -> &'a mut Node {
    path.iter().fold(node, |n, &i| &mut n.children[i])
}

/// [`plan`] by the pick's part names only.
fn plan_named(root: &Node, pick: &Pick) -> Option<Plan> {
    let id = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    // Side: seen from the creature's left (+x), so its front is to the right.
    let view = match pick.view {
        View::Front => id,
        View::Side => [[0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
    };
    let mut seen = Vec::new();
    let mut framed = Vec::new();
    gather(
        root,
        &id,
        [0.0; 3],
        &view,
        false,
        pick,
        false,
        &mut seen,
        &mut framed,
    );
    if seen.is_empty() || framed.is_empty() {
        return None;
    }
    // Framed on the framing parts' main faces (at least half the biggest one's size, so an
    // antenna, an ear or a nose doesn't shrink the face); with `near`, on the nearest of
    // them (so branches beside and behind the head don't take its place).
    let area = |q: &[[f32; 3]; 4]| {
        let (ex, ey) = (q[1][0] - q[0][0], q[1][1] - q[0][1]);
        let (fx, fy) = (q[3][0] - q[0][0], q[3][1] - q[0][1]);
        (ex * fy - ey * fx).abs()
    };
    let biggest = framed.iter().map(area).fold(0.0, f32::max);
    let depth = |q: &[[f32; 3]; 4]| q.iter().map(|c| c[2]).fold(f32::MAX, f32::min);
    let share = if pick.near { 0.3 } else { 0.5 };
    let big: Vec<&[[f32; 3]; 4]> = framed
        .iter()
        .filter(|q| area(q) >= biggest * share)
        .collect();
    let nearest = big.iter().map(|q| depth(q)).fold(f32::MAX, f32::min);
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for q in big
        .into_iter()
        .filter(|q| !pick.near || depth(q) <= nearest + 0.5)
    {
        for c in q {
            x0 = x0.min(c[0]);
            y0 = y0.min(c[1]);
            x1 = x1.max(c[0]);
            y1 = y1.max(c[1]);
        }
    }
    // A square around the framing parts, centred.
    let side = (x1 - x0).max(y1 - y0);
    if side <= 0.01 {
        return None;
    }
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (left, top) = (cx - side / 2.0, cy - side / 2.0);
    let cell = side / SIDE as f32;
    let mut pixels = Vec::with_capacity(SIDE * SIDE);
    for py in 0..SIDE {
        for px in 0..SIDE {
            let (x, y) = (
                left + (px as f32 + 0.5) * cell,
                top + (py as f32 + 0.5) * cell,
            );
            let mut hits: Vec<(f32, [f32; 2])> = Vec::new();
            for s in &seen {
                if let Some(hit) = hit(s, x, y) {
                    hits.push(hit);
                }
            }
            hits.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut at = [None; DEPTH];
            for (slot, (_, uv)) in at.iter_mut().zip(hits) {
                *slot = Some(uv);
            }
            pixels.push(at);
        }
    }
    Some(Plan { pixels })
}

/// Where screen point `(x, y)` falls on face `s`: its depth and texture place, if inside.
/// A box face is a rectangle, so its picture is a parallelogram spanned from corner 0 by the
/// edges to corners 1 and 3.
fn hit(s: &Seen, x: f32, y: f32) -> Option<(f32, [f32; 2])> {
    let [a, b, _, d] = s.p;
    let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
    let (fx, fy) = (d[0] - a[0], d[1] - a[1]);
    let det = ex * fy - ey * fx;
    if det.abs() < 1e-6 {
        return None;
    }
    let (qx, qy) = (x - a[0], y - a[1]);
    let s1 = (qx * fy - qy * fx) / det;
    let t1 = (ex * qy - ey * qx) / det;
    if !(0.0..=1.0).contains(&s1) || !(0.0..=1.0).contains(&t1) {
        return None;
    }
    let lerp = |i: usize| -> f32 {
        let (o, e, f) = (s.uv[0][i], s.uv[1][i], s.uv[3][i]);
        o + s1 * (e - o) + t1 * (f - o)
    };
    let depth = a[2] + s1 * (b[2] - a[2]) + t1 * (d[2] - a[2]);
    Some((depth, [lerp(0), lerp(1)]))
}

/// The face's 8 x 8 RGBA pixels from texture `rgba` (`w` x `h`), or None when too little of
/// it is there to read as a face. What stays clear is shown dark, so the face is a tile.
pub fn paint(plan: &Plan, w: u32, h: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    if w == 0 || h == 0 || rgba.len() < (w * h * 4) as usize {
        return None;
    }
    let mut out = Vec::with_capacity(SIDE * SIDE * 4);
    let mut shown = 0;
    for places in &plan.pixels {
        let mut px = [24, 22, 20, 255];
        for uv in places.iter().flatten() {
            let tx = ((uv[0] * w as f32).floor() as i64).clamp(0, i64::from(w) - 1) as usize;
            let ty = ((uv[1] * h as f32).floor() as i64).clamp(0, i64::from(h) - 1) as usize;
            let at = (ty * w as usize + tx) * 4;
            if rgba[at + 3] > 40 {
                px = [rgba[at], rgba[at + 1], rgba[at + 2], 255];
                shown += 1;
                break;
            }
        }
        out.extend_from_slice(&px);
    }
    (shown >= SIDE * SIDE / 4).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A box `w` x `h` x `d` texels at `at`, faces laid out on the texture as the game lays a
    /// box out from texel `(u, v)` of a `tw` x `th` texture.
    fn cube(name: &str, at: [f32; 3], size: [f32; 3], uv: [f32; 2], tex: [f32; 2]) -> Node {
        let [x0, y0, z0] = at;
        let [w, h, d] = size;
        let (x1, y1) = (x0 + w, y0 + h);
        let [u, v] = uv;
        let f = |u: f32, v: f32| [u / tex[0], v / tex[1]];
        let q = |c: [[f32; 3]; 4], uv0: [f32; 2], uv1: [f32; 2], n: [f32; 3]| {
            let a = f(uv0[0], uv0[1]);
            let b = f(uv1[0], uv1[1]);
            Quad {
                v: [
                    [c[0][0], c[0][1], c[0][2], b[0], a[1]],
                    [c[1][0], c[1][1], c[1][2], a[0], a[1]],
                    [c[2][0], c[2][1], c[2][2], a[0], b[1]],
                    [c[3][0], c[3][1], c[3][2], b[0], b[1]],
                ],
                normal: n,
            }
        };
        // Only the front, as that is what the tests look at: u from d, v from d.
        let front = q(
            [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
            [u + d, v + d],
            [u + d + w, v + d + h],
            [0.0, 0.0, -1.0],
        );
        Node {
            name: name.into(),
            pose: [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            quads: vec![front],
            children: vec![],
        }
    }

    fn root(children: Vec<Node>) -> Node {
        Node {
            name: "root".into(),
            pose: [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            quads: vec![],
            children,
        }
    }

    fn bone(name: &str, children: Vec<Node>) -> Node {
        Node {
            name: name.into(),
            pose: [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            quads: vec![],
            children,
        }
    }

    #[test]
    fn a_pack_model_frames_on_the_boxes_under_its_named_parts() {
        // A resource pack's head (`.jem`): the vanilla `head` keeps its name, drawn by an
        // unnamed bone under it, as the game builds it.
        let model = root(vec![bone(
            "head",
            vec![bone(
                "cem#0",
                vec![cube(
                    "cem#1",
                    [-4.0, -8.0, -4.0],
                    [8.0, 8.0, 8.0],
                    [0.0, 0.0],
                    [64.0, 64.0],
                )],
            )],
        )]);
        let plan = plan(&model, &HEAD).expect("a face");
        let uv = plan.pixels[0][0].unwrap();
        assert_eq!(((uv[0] * 64.0) as i32, (uv[1] * 64.0) as i32), (8, 8));
    }

    #[test]
    fn a_pack_model_with_its_head_moved_under_the_body_still_has_a_face() {
        // As one pack's goat: `head` draws nothing; the head's boxes (a snout with an eye) hang
        // in front of the body, under it, and a leg stands behind.
        let snout = {
            let mut s = cube(
                "cem#3",
                [-3.0, -6.0, -14.0],
                [6.0, 6.0, 6.0],
                [16.0, 32.0],
                [64.0, 64.0],
            );
            s.children.push(cube(
                "cem#4",
                [-1.0, -5.0, -14.5],
                [2.0, 2.0, 1.0],
                [48.0, 48.0],
                [64.0, 64.0],
            ));
            s
        };
        let model = root(vec![
            bone("head", vec![bone("cem#0", vec![])]),
            bone(
                "body",
                vec![bone(
                    "cem#1",
                    vec![
                        cube(
                            "cem#2",
                            [-4.0, -4.0, -8.0],
                            [8.0, 8.0, 16.0],
                            [0.0, 0.0],
                            [64.0, 64.0],
                        ),
                        snout,
                    ],
                )],
            ),
            bone(
                "leg",
                vec![cube(
                    "cem#5",
                    [-1.0, 4.0, 6.0],
                    [2.0, 8.0, 2.0],
                    [0.0, 48.0],
                    [64.0, 64.0],
                )],
            ),
        ]);
        let goat = pick("goat");
        // The head's own name gives nothing (it is empty)...
        assert!(plan_named(&model, &goat).is_none());
        // ...so the part in front holding the snout and its eye is the face.
        assert_eq!(front_part(&model), Some(vec![1, 0, 1]));
        let plan = plan(&model, &front(&["head"], &["head"])).expect("a face");
        // Seen from the front, the snout's front (at u 22..28, v 38..44) fills the picture.
        for at in &plan.pixels {
            let uv = at[0].expect("every pixel on the snout");
            let (u, v) = ((uv[0] * 64.0) as i32, (uv[1] * 64.0) as i32);
            assert!(
                (22..28).contains(&u) && (38..44).contains(&v) || (49..51).contains(&u),
                "{u},{v}"
            );
        }
    }

    /// Each creature: its model layer and default texture in the game's data (26.3).
    const GAME: &[(&str, &str, &str)] = &[
        ("pig", "pig#main", "entity/pig/pig_temperate"),
        ("cow", "cow#main", "entity/cow/cow_temperate"),
        ("mooshroom", "mooshroom#main", "entity/cow/mooshroom_red"),
        ("sheep", "sheep#main", "entity/sheep/sheep"),
        (
            "chicken",
            "chicken#main",
            "entity/chicken/chicken_temperate",
        ),
        ("rabbit", "rabbit#main", "entity/rabbit/rabbit_brown"),
        (
            "zombie_horse",
            "zombie_horse#main",
            "entity/horse/horse_zombie",
        ),
        (
            "skeleton_horse",
            "skeleton_horse#main",
            "entity/horse/horse_skeleton",
        ),
        ("giant", "giant#main", "entity/zombie/zombie"),
        ("horse", "horse#main", "entity/horse/horse_brown"),
        ("donkey", "donkey#main", "entity/horse/donkey"),
        ("mule", "mule#main", "entity/horse/mule"),
        ("llama", "llama#main", "entity/llama/llama_creamy"),
        (
            "trader_llama",
            "trader_llama#main",
            "entity/llama/llama_creamy",
        ),
        ("goat", "goat#main", "entity/goat/goat"),
        ("panda", "panda#main", "entity/panda/panda"),
        ("polar_bear", "polar_bear#main", "entity/bear/polarbear"),
        ("fox", "fox#main", "entity/fox/fox"),
        ("wolf", "wolf#main", "entity/wolf/wolf"),
        ("cat", "cat#main", "entity/cat/cat_tabby"),
        ("ocelot", "ocelot#main", "entity/cat/ocelot"),
        ("parrot", "parrot#main", "entity/parrot/parrot_red_blue"),
        ("bee", "bee#main", "entity/bee/bee"),
        ("turtle", "turtle#main", "entity/turtle/turtle"),
        ("armadillo", "armadillo#main", "entity/armadillo/armadillo"),
        ("frog", "frog#main", "entity/frog/frog_temperate"),
        ("axolotl", "axolotl#main", "entity/axolotl/axolotl_lucy"),
        ("squid", "squid#main", "entity/squid/squid"),
        ("glow_squid", "glow_squid#main", "entity/squid/glow_squid"),
        ("tadpole", "tadpole#main", "entity/tadpole/tadpole"),
        ("cod", "cod#main", "entity/fish/cod"),
        ("salmon", "salmon#main", "entity/fish/salmon"),
        (
            "tropical_fish",
            "tropical_fish_small#main",
            "entity/fish/tropical_a",
        ),
        (
            "pufferfish",
            "pufferfish_big#main",
            "entity/fish/pufferfish",
        ),
        ("dolphin", "dolphin#main", "entity/dolphin/dolphin"),
        ("bat", "bat#main", "entity/bat/bat"),
        ("villager", "villager#main", "entity/villager/villager"),
        (
            "wandering_trader",
            "wandering_trader#main",
            "entity/wandering_trader/wandering_trader",
        ),
        (
            "iron_golem",
            "iron_golem#main",
            "entity/iron_golem/iron_golem",
        ),
        (
            "snow_golem",
            "snow_golem#main",
            "entity/snow_golem/snow_golem",
        ),
        (
            "copper_golem",
            "copper_golem#main",
            "entity/copper_golem/copper_golem",
        ),
        ("zombie", "zombie#main", "entity/zombie/zombie"),
        ("husk", "husk#main", "entity/zombie/husk"),
        ("drowned", "drowned#main", "entity/zombie/drowned"),
        (
            "zombie_villager",
            "zombie_villager#main",
            "entity/zombie_villager/zombie_villager",
        ),
        ("skeleton", "skeleton#main", "entity/skeleton/skeleton"),
        ("stray", "stray#main", "entity/skeleton/stray"),
        ("parched", "parched#main", "entity/skeleton/parched"),
        ("bogged", "bogged#main", "entity/skeleton/bogged"),
        (
            "wither_skeleton",
            "wither_skeleton#main",
            "entity/skeleton/wither_skeleton",
        ),
        ("creeper", "creeper#main", "entity/creeper/creeper"),
        ("spider", "spider#main", "entity/spider/spider"),
        (
            "cave_spider",
            "cave_spider#main",
            "entity/spider/cave_spider",
        ),
        ("enderman", "enderman#main", "entity/enderman/enderman"),
        ("endermite", "endermite#main", "entity/endermite/endermite"),
        (
            "silverfish",
            "silverfish#main",
            "entity/silverfish/silverfish",
        ),
        ("slime", "slime#main", "entity/slime/slime"),
        ("magma_cube", "magma_cube#main", "entity/slime/magmacube"),
        ("witch", "witch#main", "entity/witch/witch"),
        ("phantom", "phantom#main", "entity/phantom/phantom"),
        ("evoker", "evoker#main", "entity/illager/evoker"),
        ("illusioner", "illusioner#main", "entity/illager/illusioner"),
        ("allay", "allay#main", "entity/allay/allay"),
        ("creaking", "creaking#main", "entity/creaking/creaking"),
        ("camel", "camel#main", "entity/camel/camel"),
        (
            "happy_ghast",
            "happy_ghast#main",
            "entity/ghast/happy_ghast",
        ),
        ("sniffer", "sniffer#main", "entity/sniffer/sniffer"),
        ("breeze", "breeze#main", "entity/breeze/breeze"),
        ("wither", "wither#main", "entity/wither/wither"),
        ("ravager", "ravager#main", "entity/illager/ravager"),
        ("vex", "vex#main", "entity/illager/vex"),
        ("shulker", "shulker#main", "entity/shulker/shulker"),
        ("guardian", "guardian#main", "entity/guardian/guardian"),
        (
            "elder_guardian",
            "elder_guardian#main",
            "entity/guardian/guardian_elder",
        ),
        ("pillager", "pillager#main", "entity/illager/pillager"),
        ("vindicator", "vindicator#main", "entity/illager/vindicator"),
        ("piglin", "piglin#main", "entity/piglin/piglin"),
        (
            "piglin_brute",
            "piglin_brute#main",
            "entity/piglin/piglin_brute",
        ),
        (
            "zombified_piglin",
            "zombified_piglin#main",
            "entity/piglin/zombified_piglin",
        ),
        ("hoglin", "hoglin#main", "entity/hoglin/hoglin"),
        ("zoglin", "zoglin#main", "entity/hoglin/zoglin"),
        ("ghast", "ghast#main", "entity/ghast/ghast"),
        ("blaze", "blaze#main", "entity/blaze/blaze"),
        ("strider", "strider#main", "entity/strider/strider"),
        ("warden", "warden#main", "entity/warden/warden"),
        ("camel_husk", "camel#main", "entity/camel/camel_husk"),
        ("nautilus", "nautilus#main", "entity/nautilus/nautilus"),
        (
            "zombie_nautilus",
            "zombie_nautilus#main",
            "entity/nautilus/zombie_nautilus",
        ),
        (
            "sulfur_cube",
            "sulfur_cube#main",
            "entity/sulfur_cube/sulfur_cube_outer",
        ),
        ("mannequin", "player#main", "entity/player/wide/steve"),
        ("player", "player#main", "entity/player/wide/steve"),
        (
            "ender_dragon",
            "ender_dragon#main",
            "entity/enderdragon/dragon",
        ),
    ];

    fn node(name: &str, v: &serde_json::Value) -> Node {
        let f = |x: &serde_json::Value| x.as_f64().unwrap_or(0.0) as f32;
        let mut pose = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        for (i, x) in v["pose"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .take(9)
        {
            pose[i] = f(x);
        }
        let normals: Vec<[f32; 3]> = v["normals"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|n| [f(&n[0]), f(&n[1]), f(&n[2])])
            .collect();
        let quads = v["polygons"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(i, p)| {
                let c: Vec<[f32; 5]> = p
                    .as_array()?
                    .iter()
                    .map(|c| [f(&c[0]), f(&c[1]), f(&c[2]), f(&c[3]), f(&c[4])])
                    .collect();
                (c.len() == 4).then(|| Quad {
                    v: [c[0], c[1], c[2], c[3]],
                    normal: normals.get(i).copied().unwrap_or([0.0; 3]),
                })
            })
            .collect();
        let children = v["children"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, c)| node(k, c))
            .collect();
        Node {
            name: name.into(),
            pose,
            quads,
            children,
        }
    }

    /// Makes every creature's face from the game's own model data and textures (the extracted
    /// client in `vanilla-26.3/` and `FRONTIER_TEXTURES`, default `/tmp/vtex`, with a resource
    /// pack `FRONTIER_PACK` over them if one is given), and writes them
    /// side by side to `target/faces.png` to look over. Skipped where the data isn't there.
    #[test]
    fn every_creature_gets_a_face_from_the_game_data() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let models = root.join("../vanilla-26.3/entity-models.json");
        let tex = std::env::var("FRONTIER_TEXTURES").unwrap_or_else(|_| "/tmp/vtex".into());
        let tex = std::path::Path::new(&tex).join("assets/minecraft/textures");
        // A resource pack over them (`FRONTIER_PACK`), as the game stacks packs: its textures
        // win where it has them.
        let pack = std::env::var("FRONTIER_PACK")
            .ok()
            .map(|p| std::path::Path::new(&p).join("assets/minecraft/textures"));
        let Ok(text) = std::fs::read_to_string(&models) else {
            eprintln!("no {}: skipped", models.display());
            return;
        };
        if !tex.is_dir() {
            eprintln!("no {}: skipped", tex.display());
            return;
        }
        let all: serde_json::Value = serde_json::from_str(&text).unwrap();
        let (cols, cell) = (12usize, 10usize);
        let rows = GAME.len().div_ceil(cols);
        let (sw, sh) = (cols * cell, rows * cell);
        let mut sheet = vec![0u8; sw * sh * 4];
        let mut missing = Vec::new();
        for (i, (id, model, texture)) in GAME.iter().enumerate() {
            assert!(
                MOBS.iter().any(|(m, _)| m == id),
                "{id} has no face mapping"
            );
            let Some(m) = all.get(format!("minecraft:{model}")) else {
                missing.push(format!("{id}: no model {model}"));
                continue;
            };
            let file = format!("{texture}.png");
            let png = pack
                .as_ref()
                .and_then(|p| std::fs::read(p.join(&file)).ok())
                .unwrap_or_else(|| std::fs::read(tex.join(&file)).unwrap());
            let img =
                crate::png::decode(&png).unwrap_or_else(|| panic!("{texture}.png didn't decode"));
            let face = plan(&node("root", m), &pick(id))
                .and_then(|p| paint(&p, img.width, img.height, &img.rgba));
            let Some(face) = face else {
                missing.push(format!("{id}: no face"));
                continue;
            };
            let (ox, oy) = ((i % cols) * cell + 1, (i / cols) * cell + 1);
            for y in 0..SIDE {
                for x in 0..SIDE {
                    let s = (y * SIDE + x) * 4;
                    let d = ((oy + y) * sw + ox + x) * 4;
                    sheet[d..d + 4].copy_from_slice(&face[s..s + 4]);
                }
            }
        }
        let out = root.join("target/faces.png");
        let _ = std::fs::create_dir_all(root.join("target"));
        std::fs::write(
            &out,
            crate::png::encode_stored(sw as u32, sh as u32, &sheet),
        )
        .unwrap();
        assert!(missing.is_empty(), "creatures without a face: {missing:?}");
    }

    #[test]
    fn every_creature_has_a_face_mapping_once() {
        let mut seen = std::collections::HashSet::new();
        for (id, p) in MOBS {
            assert!(seen.insert(*id), "{id} twice");
            assert!(!p.parts.is_empty() && !p.frame.is_empty(), "{id}");
        }
        // The game's creatures (26.3) as its species list has them, the dragon and players.
        for id in [
            "pig",
            "cow",
            "mooshroom",
            "sheep",
            "chicken",
            "rabbit",
            "zombie_horse",
            "skeleton_horse",
            "giant",
            "horse",
            "donkey",
            "mule",
            "llama",
            "trader_llama",
            "goat",
            "panda",
            "polar_bear",
            "fox",
            "wolf",
            "cat",
            "ocelot",
            "parrot",
            "bee",
            "turtle",
            "armadillo",
            "frog",
            "axolotl",
            "squid",
            "glow_squid",
            "tadpole",
            "cod",
            "salmon",
            "tropical_fish",
            "pufferfish",
            "dolphin",
            "bat",
            "villager",
            "wandering_trader",
            "iron_golem",
            "snow_golem",
            "copper_golem",
            "zombie",
            "husk",
            "drowned",
            "zombie_villager",
            "skeleton",
            "stray",
            "parched",
            "bogged",
            "wither_skeleton",
            "creeper",
            "spider",
            "cave_spider",
            "enderman",
            "endermite",
            "silverfish",
            "slime",
            "magma_cube",
            "witch",
            "phantom",
            "evoker",
            "illusioner",
            "allay",
            "creaking",
            "camel",
            "happy_ghast",
            "sniffer",
            "breeze",
            "wither",
            "ravager",
            "vex",
            "shulker",
            "guardian",
            "elder_guardian",
            "pillager",
            "vindicator",
            "piglin",
            "piglin_brute",
            "zombified_piglin",
            "hoglin",
            "zoglin",
            "ghast",
            "blaze",
            "strider",
            "warden",
            "camel_husk",
            "nautilus",
            "zombie_nautilus",
            "sulfur_cube",
            "mannequin",
            "ender_dragon",
            "player",
        ] {
            assert!(
                MOBS.iter().any(|(m, _)| *m == id),
                "{id} has no face mapping"
            );
        }
        assert_eq!(pick("minecraft:wolf").parts, &["real_head"]);
        assert_eq!(pick("othermod:golem"), HEAD);
    }

    #[test]
    fn a_plain_head_reads_its_front_texel_for_texel() {
        // An 8-texel head whose front is at (8, 8) on a 64 x 64 texture, as a zombie's.
        let model = root(vec![cube(
            "head",
            [-4.0, -8.0, -4.0],
            [8.0, 8.0, 8.0],
            [0.0, 0.0],
            [64.0, 64.0],
        )]);
        let plan = plan(&model, &HEAD).expect("a face");
        assert_eq!(plan.pixels.len(), 64);
        // Each pixel lands in the middle of its own texel of the front.
        let uv = plan.pixels[0][0].unwrap();
        assert_eq!(((uv[0] * 64.0) as i32, (uv[1] * 64.0) as i32), (8, 8));
        let uv = plan.pixels[63][0].unwrap();
        assert_eq!(((uv[0] * 64.0) as i32, (uv[1] * 64.0) as i32), (15, 15));
    }

    #[test]
    fn a_hat_in_front_is_tried_first_and_shows_the_head_where_clear() {
        let head = cube(
            "head",
            [-4.0, -8.0, -4.0],
            [8.0, 8.0, 8.0],
            [0.0, 0.0],
            [64.0, 64.0],
        );
        let hat = cube(
            "hat",
            [-4.5, -8.5, -4.5],
            [9.0, 9.0, 9.0],
            [32.0, 0.0],
            [64.0, 64.0],
        );
        let plan = plan(&root(vec![head, hat]), &HEAD).unwrap();
        let p = plan.pixels[27];
        assert!(p[0].unwrap()[0] >= 32.0 / 64.0, "hat first");
        assert!(p[1].unwrap()[0] < 32.0 / 64.0, "head behind");
        // A texture whose hat is clear: the head shows.
        let mut tex = vec![0u8; 64 * 64 * 4];
        for y in 8..16 {
            for x in 8..16 {
                let i = (y * 64 + x) * 4;
                tex[i..i + 4].copy_from_slice(&[200, 100, 50, 255]);
            }
        }
        let face = paint(&plan, 64, 64, &tex).unwrap();
        assert_eq!(&face[27 * 4..27 * 4 + 4], &[200, 100, 50, 255]);
    }

    #[test]
    fn parts_not_picked_are_left_out_and_no_part_is_no_face() {
        let body = cube(
            "body",
            [-4.0, -8.0, -10.0],
            [8.0, 8.0, 2.0],
            [0.0, 32.0],
            [64.0, 64.0],
        );
        let model = root(vec![body]);
        assert!(plan(&model, &HEAD).is_none());
        assert!(plan(&model, &front(&["body"], &["body"])).is_some());
    }

    #[test]
    fn a_turned_part_is_seen_turned() {
        // A head turned half round about y shows its back to the viewer: nothing faces them.
        let mut head = cube(
            "head",
            [-4.0, -8.0, -4.0],
            [8.0, 8.0, 8.0],
            [0.0, 0.0],
            [64.0, 64.0],
        );
        head.pose[4] = std::f32::consts::PI;
        assert!(plan(&root(vec![head]), &HEAD).is_none());
    }
}
