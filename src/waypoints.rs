//! Waypoints: named, coloured spots kept per server or world and per dimension, plus the
//! temporary one where you last died.
//!
//! Saved as plain text, one waypoint a line: `x y z colour shown kind name`, the name last
//! so it may hold spaces (tabs and line breaks are turned into spaces).

/// The colours a waypoint can have (ARGB), in the order the colour button steps through.
pub const COLORS: [u32; 12] = [
    0xFF_E8483A, // red
    0xFF_F28B2C, // orange
    0xFF_F3D23B, // yellow
    0xFF_8BD63F, // lime
    0xFF_2FA84F, // green
    0xFF_3FD0C9, // cyan
    0xFF_3B8CE8, // blue
    0xFF_6B5BE0, // indigo
    0xFF_B45BE0, // purple
    0xFF_EC6FB6, // pink
    0xFF_F4F1E8, // white
    0xFF_8A8F98, // grey
];

pub const COLOR_NAMES: [&str; 12] = [
    "frontier.color.red",
    "frontier.color.orange",
    "frontier.color.yellow",
    "frontier.color.lime",
    "frontier.color.green",
    "frontier.color.cyan",
    "frontier.color.blue",
    "frontier.color.indigo",
    "frontier.color.purple",
    "frontier.color.pink",
    "frontier.color.white",
    "frontier.color.grey",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// One the player made.
    Normal,
    /// Where the player last died; goes away when reached or replaced.
    Death,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Waypoint {
    pub name: String,
    pub pos: [i32; 3],
    /// An index into `COLORS`.
    pub color: usize,
    pub shown: bool,
    pub kind: Kind,
}

impl Waypoint {
    pub fn argb(&self) -> u32 {
        COLORS[self.color % COLORS.len()]
    }
    /// The first letter, drawn on its marker.
    pub fn initial(&self) -> char {
        self.name
            .chars()
            .find(|c| !c.is_whitespace())
            .map_or('?', |c| c.to_uppercase().next().unwrap_or(c))
    }
}

fn clean(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let s = s.trim();
    if s.is_empty() {
        "Waypoint".to_owned()
    } else {
        s.chars().take(32).collect()
    }
}

pub fn encode(list: &[Waypoint]) -> String {
    let mut out = String::from("# Frontier waypoints: x y z colour shown kind name\n");
    for w in list {
        out.push_str(&format!(
            "{} {} {} {} {} {} {}\n",
            w.pos[0],
            w.pos[1],
            w.pos[2],
            w.color % COLORS.len(),
            u8::from(w.shown),
            match w.kind {
                Kind::Normal => "point",
                Kind::Death => "death",
            },
            clean(&w.name)
        ));
    }
    out
}

pub fn decode(text: &str) -> Vec<Waypoint> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut parts = line.splitn(7, ' ');
        let mut num = || parts.next().and_then(|p| p.parse::<i64>().ok());
        let (Some(x), Some(y), Some(z), Some(c), Some(shown)) = (num(), num(), num(), num(), num())
        else {
            continue;
        };
        let kind = match parts.next() {
            Some("death") => Kind::Death,
            Some(_) => Kind::Normal,
            None => continue,
        };
        let name = clean(parts.next().unwrap_or(""));
        let clamp = |v: i64| v.clamp(-30_000_000, 30_000_000) as i32;
        out.push(Waypoint {
            name,
            pos: [clamp(x), clamp(y), clamp(z)],
            color: (c.max(0) as usize) % COLORS.len(),
            shown: shown != 0,
            kind,
        });
    }
    out
}

/// A name for a new waypoint: "Waypoint", then "Waypoint 2" and so on, skipping names in use.
pub fn next_name(list: &[Waypoint], base: &str) -> String {
    let taken = |n: &str| list.iter().any(|w| w.name == n);
    if !taken(base) {
        return base.to_owned();
    }
    (2..)
        .map(|i| format!("{base} {i}"))
        .find(|n| !taken(n))
        .unwrap_or_else(|| base.to_owned())
}

/// The colour for the next waypoint: the first not in use yet, else round again.
pub fn next_color(list: &[Waypoint]) -> usize {
    (0..COLORS.len())
        .find(|c| !list.iter().any(|w| w.color == *c))
        .unwrap_or(list.len() % COLORS.len())
}

/// Distance in blocks, for labels.
pub fn distance(a: [f64; 3], b: [i32; 3]) -> f64 {
    let d = [
        a[0] - (f64::from(b[0]) + 0.5),
        a[1] - f64::from(b[1]),
        a[2] - (f64::from(b[2]) + 0.5),
    ];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// `1234 m` or `12.3 km`.
pub fn distance_text(d: f64) -> String {
    if d < 10_000.0 {
        format!("{} m", d.round() as i64)
    } else {
        format!("{:.1} km", d / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waypoints_round_trip_as_text() {
        let list = vec![
            Waypoint {
                name: "Home base".into(),
                pos: [10, 64, -20],
                color: 3,
                shown: true,
                kind: Kind::Normal,
            },
            Waypoint {
                name: "Death".into(),
                pos: [-5, 12, 7],
                color: 0,
                shown: false,
                kind: Kind::Death,
            },
        ];
        assert_eq!(decode(&encode(&list)), list);
        assert_eq!(decode("garbage\n1 2\n1 2 3 4 1 point  \n").len(), 1);
    }

    #[test]
    fn new_names_and_colours_avoid_ones_in_use() {
        let mut list = Vec::new();
        assert_eq!(next_name(&list, "Waypoint"), "Waypoint");
        list.push(Waypoint {
            name: "Waypoint".into(),
            pos: [0; 3],
            color: 0,
            shown: true,
            kind: Kind::Normal,
        });
        assert_eq!(next_name(&list, "Waypoint"), "Waypoint 2");
        assert_eq!(next_color(&list), 1);
        assert_eq!(list[0].initial(), 'W');
        assert_eq!(distance_text(1234.4), "1234 m");
        assert_eq!(distance_text(12345.0), "12.3 km");
    }
}
