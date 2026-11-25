//! The hand-written formulas and the pull-to-throw conversion.

use slingshot::physics::*;

const TINY: f64 = 1e-9;

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < TINY, "{a} is not {b}");
}

#[test]
fn forty_five_degrees_gives_the_longest_range() {
    let best = level_range(14.0, 45f64.to_radians());
    for tenth in 0..=900 {
        let degrees = f64::from(tenth) / 10.0;
        if tenth != 450 {
            assert!(
                level_range(14.0, degrees.to_radians()) < best,
                "{degrees}° beat 45°"
            );
        }
    }
}

#[test]
fn thirty_and_sixty_degrees_give_equal_range() {
    close(
        level_range(14.0, 30f64.to_radians()),
        level_range(14.0, 60f64.to_radians()),
    );
}

#[test]
fn height_is_zero_again_at_flight_time_and_x_equals_range() {
    for degrees in [10.0, 30.0, 45.0, 60.0, 90.0] {
        let shot = Shot::from_degrees(14.0, degrees);
        let (x, y) = position_at(shot, flight_time(shot.velocity().1));
        close(y, 0.0);
        close(x, level_range(shot.speed, shot.angle));
    }
}

#[test]
fn height_at_half_flight_time_is_the_maximum_height() {
    for degrees in [10.0, 45.0, 80.0] {
        let vy = Shot::from_degrees(14.0, degrees).velocity().1;
        let half = flight_time(vy) / 2.0;
        close(y_at(vy, half), max_height(vy));
        assert!(y_at(vy, half - 0.01) < max_height(vy));
        assert!(y_at(vy, half + 0.01) < max_height(vy));
    }
}

#[test]
fn velocity_splits_into_sideways_and_upward_parts() {
    let (vx, vy) = velocity_components(10.0, 30f64.to_radians());
    close(vx, 75f64.sqrt());
    close(vy, 5.0);
}

#[test]
fn pull_sets_speed_by_distance_and_angle_by_direction() {
    // Pull 2 m down and to the left at 45°: throw up and right at 45°.
    let side = 2.0 / 2f64.sqrt();
    let shot = pull_to_shot(-side, -side).unwrap();
    close(shot.speed, 2.0 * SPEED_PER_METRE);
    close(shot.degrees(), 45.0);
    let (vx, vy) = shot.velocity();
    close(vx, side * SPEED_PER_METRE);
    close(vy, side * SPEED_PER_METRE);
    // And back again, which is where the pouch gets drawn.
    let (px, py) = shot.pull_offset();
    close(px, -side);
    close(py, -side);
}

#[test]
fn speed_is_capped_but_direction_is_kept() {
    let at_cap = pull_to_shot(-MAX_PULL, 0.0).unwrap();
    close(at_cap.speed, MAX_SPEED);
    let beyond = pull_to_shot(-30.0, -40.0).unwrap();
    close(beyond.speed, MAX_SPEED);
    close(beyond.angle, 40f64.atan2(30.0));
}

#[test]
fn angle_is_limited_and_backward_throws_are_rejected() {
    close(pull_to_shot(-2.0, 0.5).unwrap().degrees(), 0.0);
    close(pull_to_shot(0.5, -2.0).unwrap().degrees(), 90.0);
    assert!(pull_to_shot(0.0, 0.0).is_none());
    assert!(pull_to_shot(2.0, 2.0).is_none());
}
