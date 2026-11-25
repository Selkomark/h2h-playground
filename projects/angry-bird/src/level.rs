//! The level, as plain data, and the randomizer that lays out the targets. To
//! add or remove objects, edit `layout` (what the game plays) below.
//!
//! Positions are the **centre** of each object in metres. `x` grows to the
//! right, `y` grows upward, and the top of the ground is `y = 0`. An object of
//! height `h` standing on the ground has `y = h / 2`. Objects that are meant to
//! rest on each other should exactly touch: no gap and no overlap.

use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Box { width: f64, height: f64 },
    Ball { radius: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Role {
    /// Moves when hit. All targets must be knocked down to win.
    Target,
    /// Never moves, such as a pedestal.
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Object {
    pub name: &'static str,
    pub role: Role,
    pub shape: Shape,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub struct Level {
    /// The layout number, from 1 to `LAYOUTS`. 0 is the fixed `LEVEL`.
    pub number: u32,
    pub shots: u32,
    /// Where the ball starts flying (the fork of the slingshot).
    pub launch: (f64, f64),
    pub ball_radius: f64,
    pub objects: Cow<'static, [Object]>,
}

const fn crate_at(name: &'static str, x: f64, y: f64, size: f64) -> Object {
    Object {
        name,
        role: Role::Target,
        shape: Shape::Box {
            width: size,
            height: size,
        },
        x,
        y,
    }
}

const fn pedestal_at(x: f64, height: f64) -> Object {
    Object {
        name: "Melon pedestal",
        role: Role::Fixed,
        shape: Shape::Box {
            width: PEDESTAL_WIDTH,
            height,
        },
        x,
        y: height / 2.0,
    }
}

/// The melon, sitting on a pedestal of the given height.
const fn melon_at(x: f64, pedestal_height: f64) -> Object {
    Object {
        name: "Smug Melon",
        role: Role::Target,
        shape: Shape::Ball {
            radius: MELON_RADIUS,
        },
        x,
        y: pedestal_height + MELON_RADIUS,
    }
}

const BIG_CRATE: f64 = 1.4;
const PIZZA_BOX: f64 = 1.0;
const PEDESTAL_WIDTH: f64 = 1.6;
const MELON_RADIUS: f64 = 0.7;

/// A fixed arrangement, used by tests. The game itself plays `layout`s.
pub const LEVEL: Level = Level {
    number: 0,
    shots: 5,
    launch: (8.0, 3.0),
    ball_radius: 0.45,
    objects: Cow::Borrowed(&[
        crate_at("Sir Crate-a-lot", 21.0, 0.7, BIG_CRATE),
        pedestal_at(27.0, 2.0),
        melon_at(27.0, 2.0),
        // The Leaning Tower of Pizza Boxes (it does not lean until you help).
        crate_at("Pizza box 1", 34.0, 0.5, PIZZA_BOX),
        crate_at("Pizza box 2", 34.0, 1.5, PIZZA_BOX),
        crate_at("Pizza box 3", 34.0, 2.5, PIZZA_BOX),
        crate_at("Pizza box 4", 34.0, 3.5, PIZZA_BOX),
        crate_at("Crate Expectations", 39.0, 0.7, BIG_CRATE),
    ]),
};

/// How many different layouts there are. A test checks that every one stands
/// still until hit, and another (run by hand) that every one can be won.
pub const LAYOUTS: u32 = 1000;
/// Everything in a layout stays between these `x` positions (m): clear of the
/// slingshot, and on screen.
pub const ZONE_LEFT: f64 = 18.0;
pub const ZONE_RIGHT: f64 = 41.0;
/// The least space between neighbouring groups of objects (m).
pub const MIN_GAP: f64 = 2.0;
/// Pedestals are this tall, in steps of 0.2 m.
pub const PEDESTAL_HEIGHTS: (f64, f64) = (1.2, 3.2);

/// Things that are placed together.
#[derive(Clone, Copy)]
enum Group {
    Crate(&'static str),
    Melon,
    Tower,
}

impl Group {
    fn width(self) -> f64 {
        match self {
            Group::Crate(_) => BIG_CRATE,
            Group::Melon => PEDESTAL_WIDTH,
            Group::Tower => PIZZA_BOX,
        }
    }
}

/// The layout with this number: the same number always gives the same layout.
/// The order of the groups, the space between them and the pedestal height vary.
pub fn layout(number: u32) -> Level {
    let mut random = Random(u64::from(number));
    let mut groups = [
        Group::Crate("Sir Crate-a-lot"),
        Group::Melon,
        Group::Tower,
        Group::Crate("Crate Expectations"),
    ];
    for i in (1..groups.len()).rev() {
        groups.swap(i, random.below(i as u64 + 1) as usize);
    }
    let (low, high) = PEDESTAL_HEIGHTS;
    let steps = ((high - low) / 0.2).round() as u64;
    let pedestal = low + random.below(steps + 1) as f64 * 0.2;

    // Share the space left over between the gaps. Sorted amounts mean each
    // group is shifted at least as far as the one before, so gaps never shrink.
    let taken: f64 = groups.iter().map(|g| g.width()).sum::<f64>() + 3.0 * MIN_GAP;
    let spare = ZONE_RIGHT - ZONE_LEFT - taken;
    let mut shifts = groups.map(|_| random.fraction() * spare);
    shifts.sort_by(f64::total_cmp);

    let mut objects = Vec::new();
    let mut left = ZONE_LEFT;
    for (group, shift) in groups.into_iter().zip(shifts) {
        let x = left + shift + group.width() / 2.0;
        match group {
            Group::Crate(name) => objects.push(crate_at(name, x, BIG_CRATE / 2.0, BIG_CRATE)),
            Group::Melon => objects.extend([pedestal_at(x, pedestal), melon_at(x, pedestal)]),
            Group::Tower => {
                const NAMES: [&str; 4] =
                    ["Pizza box 1", "Pizza box 2", "Pizza box 3", "Pizza box 4"];
                for (i, name) in NAMES.into_iter().enumerate() {
                    let y = (i as f64 + 0.5) * PIZZA_BOX;
                    objects.push(crate_at(name, x, y, PIZZA_BOX));
                }
            }
        }
        left += group.width() + MIN_GAP;
    }
    Level {
        number,
        objects: Cow::Owned(objects),
        ..LEVEL
    }
}

/// A layout number picked by `seed`, never the same as `current`.
pub fn pick_layout(seed: u64, current: u32) -> u32 {
    let mut random = Random(seed);
    if (1..=LAYOUTS).contains(&current) {
        // Choose among the others, skipping over the current one.
        let other = random.below(u64::from(LAYOUTS) - 1) as u32 + 1;
        other + u32::from(other >= current)
    } else {
        random.below(u64::from(LAYOUTS)) as u32 + 1
    }
}

/// A small random number generator (SplitMix64). It is written here so that a
/// layout number keeps meaning the same layout.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A whole number from 0 up to, not including, `limit`.
    fn below(&mut self, limit: u64) -> u64 {
        self.next() % limit
    }

    /// A number from 0 up to, not including, 1.
    fn fraction(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}
