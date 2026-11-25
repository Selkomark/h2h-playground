//! The random layouts, and room for the longest pull.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};

use ratatui::layout::Rect;
use slingshot::level::{self, LAYOUTS, Level, MIN_GAP, Object, Role, Shape, ZONE_LEFT, ZONE_RIGHT};
use slingshot::physics::{MAX_PULL, MAX_SPEED, Shot, pull_to_shot};
use slingshot::render::{self, MIN_HEIGHT, MIN_WIDTH, Viewport};
use slingshot::world::{DT, World};

/// `(left, right, bottom, top)` of an object (m).
fn edges(object: &Object) -> (f64, f64, f64, f64) {
    let (w, h) = match object.shape {
        Shape::Box { width, height } => (width / 2.0, height / 2.0),
        Shape::Ball { radius } => (radius, radius),
    };
    (object.x - w, object.x + w, object.y - h, object.y + h)
}

#[test]
fn every_layout_has_the_same_pieces_inside_the_zone_with_room_between_them() {
    let view = Viewport::new(render::areas(Rect::new(0, 0, MIN_WIDTH, MIN_HEIGHT), false).play);
    for number in 1..=LAYOUTS {
        let level = level::layout(number);
        assert_eq!(level.number, number);
        assert_eq!(level.objects.len(), 8);
        let targets = level.objects.iter().filter(|o| o.role == Role::Target);
        assert_eq!(targets.count(), 7);
        for (i, a) in level.objects.iter().enumerate() {
            let (left, right, bottom, top) = edges(a);
            assert!(
                left >= ZONE_LEFT - 1e-9 && right <= ZONE_RIGHT + 1e-9,
                "{number}"
            );
            assert!(right < view.x_bounds[1] && bottom >= -1e-9 && top < view.y_bounds[1]);
            for b in &level.objects[i + 1..] {
                let other = edges(b);
                let gap = (other.0 - right).max(left - other.1);
                let stacked = (a.x - b.x).abs() < 1e-9;
                if stacked {
                    // Resting on each other: they touch or are apart, no overlap.
                    assert!(
                        other.2 >= top - 1e-9 || bottom >= other.3 - 1e-9,
                        "{number}"
                    );
                } else {
                    assert!(gap >= MIN_GAP - 1e-9, "{number}: gap {gap}");
                }
            }
        }
    }
}

#[test]
fn layouts_vary_and_a_number_always_means_the_same_layout() {
    let distinct: HashSet<String> = (1..=LAYOUTS)
        .map(|n| format!("{:?}", level::layout(n).objects))
        .collect();
    assert_eq!(distinct.len(), LAYOUTS as usize);
    assert_eq!(level::layout(7).objects, level::layout(7).objects);
    // Every left-to-right order of the four groups turns up.
    let orders: HashSet<Vec<&str>> = (1..=LAYOUTS)
        .map(|n| {
            let mut objects = level::layout(n).objects.into_owned();
            objects.sort_by(|a, b| a.x.total_cmp(&b.x));
            let mut names: Vec<&str> = objects
                .iter()
                .map(|o| {
                    if o.name.starts_with("Pizza") {
                        "Pizza"
                    } else {
                        o.name
                    }
                })
                .filter(|name| *name != "Melon pedestal")
                .collect();
            names.dedup();
            names
        })
        .collect();
    assert_eq!(orders.len(), 24);
}

#[test]
fn a_new_game_picks_a_different_layout() {
    let mut seen = HashSet::new();
    let mut current = 0;
    for seed in 0..20_000 {
        let next = level::pick_layout(seed, current);
        assert!((1..=LAYOUTS).contains(&next));
        assert_ne!(next, current);
        seen.insert(next);
        current = next;
    }
    assert_eq!(seen.len(), LAYOUTS as usize);
    for current in [1, LAYOUTS] {
        assert!((0..5000).all(|seed| level::pick_layout(seed, current) != current));
    }
}

#[test]
fn every_layout_stands_still_until_hit() {
    for number in 1..=LAYOUTS {
        let level = level::layout(number);
        let mut world = World::new(level);
        for _ in 0..(5.0 / DT) as u32 {
            world.step();
        }
        for piece in &world.pieces {
            let (x, y, turn) = world.piece_pose(piece);
            let moved = (x - piece.object.x).hypot(y - piece.object.y);
            assert!(
                moved < 0.02 && turn.abs() < 0.01 && !piece.down,
                "layout {number}: {} moved {moved} m, turned {turn}",
                piece.object.name
            );
        }
    }
}

#[test]
fn the_longest_pull_is_on_screen_and_reachable_with_the_mouse() {
    let level = level::layout(1);
    let (launch_x, launch_y) = level.launch;
    let radius = level.ball_radius;
    for (width, height) in [
        (MIN_WIDTH, MIN_HEIGHT),
        (100, 30),
        (213, 61),
        (80, 70),
        (300, 24),
    ] {
        for panel in [false, true] {
            let view = Viewport::new(render::areas(Rect::new(0, 0, width, height), panel).play);
            for degrees in 0..=90 {
                let (dx, dy) = Shot::from_degrees(MAX_SPEED, f64::from(degrees)).pull_offset();
                assert!((dx.hypot(dy) - MAX_PULL).abs() < 1e-9);
                // The whole pouch is inside the picture.
                assert!(launch_x + dx - radius >= view.x_bounds[0]);
                assert!(launch_y + dy - radius >= view.y_bounds[0]);
            }
            // Some cell inside the play area gives full power, near each angle.
            let area = view.area;
            for wanted in [0.0, 30.0, 45.0, 60.0, 90.0] {
                let found = (area.y..area.bottom()).any(|row| {
                    (area.x..area.right()).any(|column| {
                        let (x, y) = view.cell_to_world(column, row);
                        pull_to_shot(x - launch_x, y - launch_y).is_some_and(|shot| {
                            shot.speed >= MAX_SPEED && (shot.degrees() - wanted).abs() <= 10.0
                        })
                    })
                });
                assert!(
                    found,
                    "{width}x{height} panel {panel}: no full pull at {wanted}°"
                );
            }
        }
    }
}

/// The targets still standing after these shots, by index into `pieces`.
fn standing_after(level: &Level, shots: &[(f64, f64)]) -> Vec<usize> {
    let mut world = World::new(level.clone());
    for &(speed, degrees) in shots {
        if !world.fire(Shot::from_degrees(speed, degrees)) {
            break;
        }
        while world.flight.as_ref().is_some_and(|f| f.active) {
            world.step();
        }
    }
    (0..world.pieces.len())
        .filter(|&i| world.pieces[i].object.role == Role::Target && !world.pieces[i].down)
        .collect()
}

/// Plays a layout by trying many shots the arrow keys can make, and keeping the
/// one that leaves the fewest targets. Returns the shots if that wins.
fn solve(level: &Level) -> Option<Vec<(f64, f64)>> {
    let mut shots = Vec::new();
    for _ in 0..level.shots {
        let mut best = (usize::MAX, (0.0, 0.0));
        'search: for degrees in (0..=70).step_by(2) {
            for half in 18..=50 {
                let shot = (f64::from(half) / 2.0, f64::from(degrees));
                shots.push(shot);
                let left = standing_after(level, &shots).len();
                shots.pop();
                if left < best.0 {
                    best = (left, shot);
                    if left == 0 {
                        break 'search;
                    }
                }
            }
        }
        shots.push(best.1);
        if best.0 == 0 {
            return Some(shots);
        }
    }
    None
}

/// Slow: `cargo test --release --test layouts -- --ignored --nocapture`.
/// Run it after changing the level, the randomizer, the ball or the physics.
#[test]
#[ignore = "takes several minutes"]
fn every_layout_can_be_won() {
    let next = AtomicU32::new(1);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let results: Vec<(u32, Option<usize>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let number = next.fetch_add(1, Ordering::Relaxed);
                        if number > LAYOUTS {
                            return done;
                        }
                        let solved = solve(&level::layout(number));
                        done.push((number, solved.map(|shots| shots.len())));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().unwrap())
            .collect()
    });
    for shots in 1..=5 {
        let count = results.iter().filter(|r| r.1 == Some(shots)).count();
        println!("won in {shots} shots: {count} layouts");
    }
    let lost: Vec<u32> = results
        .iter()
        .filter(|r| r.1.is_none())
        .map(|r| r.0)
        .collect();
    assert!(lost.is_empty(), "not won: {lost:?}");
}
