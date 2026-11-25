//! The simulation: rapier2d against the formulas, and the level itself.

use std::borrow::Cow;

use slingshot::level::{LEVEL, Level, Object, Role, Shape};
use slingshot::physics::{Shot, position_at};
use slingshot::render::preview_points;
use slingshot::world::{DT, FixedStep, Status, World};

/// Rapier moves in small steps, so it drifts a little from the exact formulas:
/// about 1 cm per second of flight at this time step.
const FLIGHT_TOLERANCE: f64 = 0.05;

/// Nothing to bump into. It needs one far-off target, or it counts as won.
fn open_level() -> Level {
    const FAR: &[Object] = &[Object {
        name: "far away",
        role: Role::Target,
        shape: Shape::Ball { radius: 0.5 },
        x: -40.0,
        y: 0.5,
    }];
    Level {
        objects: Cow::Borrowed(FAR),
        ..LEVEL
    }
}

#[test]
fn simulated_free_flight_matches_the_formulas() {
    for (speed, degrees) in [
        (14.0, 45.0),
        (20.0, 70.0),
        (10.0, 60.0),
        (20.0, 30.0),
        (12.0, 0.0),
    ] {
        let shot = Shot::from_degrees(speed, degrees);
        let mut world = World::new(open_level());
        let (launch_x, launch_y) = world.level.launch;
        assert!(world.fire(shot));
        // The ball starts exactly where the formulas and preview start.
        assert_eq!(world.ball_position(), Some((launch_x, launch_y)));

        let mut compared = 0;
        loop {
            world.step();
            let flight = world.flight.as_ref().unwrap();
            if flight.has_hit || !flight.active {
                break;
            }
            let (x, y) = position_at(shot, flight.free_time());
            let (ball_x, ball_y) = world.ball_position().unwrap();
            let error = (ball_x - launch_x - x).hypot(ball_y - launch_y - y);
            assert!(
                error < FLIGHT_TOLERANCE,
                "{speed} m/s {degrees}°: off by {error} m"
            );
            compared += 1;
        }
        assert!(compared > 60, "only {compared} steps of free flight");
    }
}

#[test]
fn preview_follows_the_path_the_ball_really_takes() {
    let shot = Shot::from_degrees(15.0, 50.0);
    let mut world = World::new(open_level());
    let dots = preview_points(world.level.launch, shot);
    world.fire(shot);
    let mut path = vec![world.ball_position().unwrap()];
    for _ in 0..120 {
        world.step();
        path.push(world.ball_position().unwrap());
    }
    assert!(!dots.is_empty());
    for (x, y) in dots {
        let nearest = path
            .iter()
            .map(|(a, b)| (a - x).hypot(b - y))
            .fold(f64::MAX, f64::min);
        // Within half the distance the ball covers in one step, plus drift.
        assert!(
            nearest < 15.0 * DT / 2.0 + FLIGHT_TOLERANCE,
            "dot ({x}, {y}) is {nearest} m off"
        );
    }
}

#[test]
fn stack_and_other_targets_stay_still_until_hit() {
    let mut world = World::new(LEVEL);
    for _ in 0..(20.0 / DT) as u32 {
        world.step();
    }
    for piece in &world.pieces {
        let (x, y, turn) = world.piece_pose(piece);
        let moved = (x - piece.object.x).hypot(y - piece.object.y);
        assert!(moved < 0.02, "{} drifted {moved} m", piece.object.name);
        assert!(turn.abs() < 0.01, "{} turned {turn} rad", piece.object.name);
        assert!(!piece.down);
    }
}

/// Fires and runs until the shot is over.
fn play(world: &mut World, speed: f64, degrees: f64) {
    assert!(world.fire(Shot::from_degrees(speed, degrees)));
    assert!(!world.ready_to_fire(), "no second shot while one is flying");
    while world.flight.as_ref().unwrap().active {
        world.step();
    }
}

/// Also shows the level can be won. If you change the level or the physics,
/// this sequence will probably need new numbers.
#[test]
fn hitting_things_knocks_them_down_and_the_level_can_be_won() {
    let mut world = World::new(LEVEL);
    let targets = world.targets_left();
    play(&mut world, 16.25, 24.0);
    assert!(world.targets_left() < targets);
    let stack_fell = world
        .pieces
        .iter()
        .any(|p| p.object.name.starts_with("Pizza") && p.down);
    assert!(stack_fell, "the stack should topple when hit");
    play(&mut world, 11.5, 0.0);
    assert_eq!(world.targets_left(), 0);
    assert_eq!(world.status(), Status::Won);
    assert!(!world.fire(Shot::from_degrees(10.0, 45.0)));
}

#[test]
fn running_out_of_shots_loses_and_a_new_world_restarts() {
    let mut world = World::new(LEVEL);
    for _ in 0..LEVEL.shots {
        play(&mut world, 4.0, 90.0);
    }
    assert_eq!(world.shots_left, 0);
    assert_eq!(world.status(), Status::Lost);
    assert!(!world.fire(Shot::from_degrees(10.0, 45.0)));
    let fresh = World::new(LEVEL);
    assert_eq!(fresh.status(), Status::Playing);
    assert_eq!(fresh.shots_left, LEVEL.shots);
}

#[test]
fn physics_steps_do_not_depend_on_frame_rate() {
    for fps in [20.0, 30.0, 60.0, 144.0, 1000.0] {
        let mut clock = FixedStep::default();
        let frames = (fps * 3.0) as u32;
        let steps: u32 = (0..frames).map(|_| clock.steps_for(1.0 / fps)).sum();
        let expected = (3.0 / DT).round() as i64;
        assert!(
            (i64::from(steps) - expected).abs() <= 1,
            "{fps} fps gave {steps} steps"
        );
    }
    // A long stall is limited instead of causing a burst of steps.
    assert!(FixedStep::default().steps_for(60.0) <= 30);
}
