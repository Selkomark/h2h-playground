//! The launch and flight math, written by hand as pure functions.
//!
//! Units are metres, seconds and radians. `x` points right and `y` points up.
//! Flight positions are measured **from the launch point**, not from the ground.
//! There is no air drag. `docs/PHYSICS.md` explains every formula.

/// Gravity: how much downward speed a falling object gains each second (m/s²).
pub const G: f64 = 9.81;
/// The fastest the slingshot can throw (m/s).
pub const MAX_SPEED: f64 = 25.0;
/// Releases slower than this do not fire (m/s).
pub const MIN_SPEED: f64 = 2.0;
/// Pull distance that gives `MAX_SPEED`; pulling farther adds nothing (m).
pub const MAX_PULL: f64 = 5.0;
/// Launch speed gained per metre of pull.
pub const SPEED_PER_METRE: f64 = MAX_SPEED / MAX_PULL;
/// The slingshot aims from flat (0°) to straight up (90°).
pub const MIN_ANGLE: f64 = 0.0;
pub const MAX_ANGLE: f64 = std::f64::consts::FRAC_PI_2;

/// A throw: how fast, and at what angle above the horizontal (radians).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shot {
    pub speed: f64,
    pub angle: f64,
}

impl Shot {
    pub fn from_degrees(speed: f64, degrees: f64) -> Self {
        Self {
            speed,
            angle: degrees.to_radians(),
        }
    }

    pub fn degrees(&self) -> f64 {
        self.angle.to_degrees()
    }

    /// `(vx, vy)` for this shot.
    pub fn velocity(&self) -> (f64, f64) {
        velocity_components(self.speed, self.angle)
    }

    /// Where the pouch must be, relative to the launch point, to produce this shot.
    pub fn pull_offset(&self) -> (f64, f64) {
        let (vx, vy) = self.velocity();
        (-vx / SPEED_PER_METRE, -vy / SPEED_PER_METRE)
    }
}

/// `vx = v cos(a)`, `vy = v sin(a)`.
pub fn velocity_components(speed: f64, angle: f64) -> (f64, f64) {
    (speed * angle.cos(), speed * angle.sin())
}

/// Turns a pull into a shot. `(pull_x, pull_y)` is the pouch position minus the
/// launch point. The ball flies the opposite way to the pull.
///
/// Speed grows with pull distance up to `MAX_SPEED`. The angle is limited to
/// 0°–90°. Returns `None` for no pull at all, or for a pull up and forward
/// (which would throw the ball down and backward).
pub fn pull_to_shot(pull_x: f64, pull_y: f64) -> Option<Shot> {
    let (dir_x, dir_y) = (-pull_x, -pull_y);
    let distance = dir_x.hypot(dir_y);
    if distance <= f64::EPSILON || (dir_x < 0.0 && dir_y < 0.0) {
        return None;
    }
    Some(Shot {
        speed: (distance * SPEED_PER_METRE).min(MAX_SPEED),
        angle: dir_y.atan2(dir_x).clamp(MIN_ANGLE, MAX_ANGLE),
    })
}

/// `x(t) = vx t`: distance travelled sideways after `t` seconds.
pub fn x_at(vx: f64, t: f64) -> f64 {
    vx * t
}

/// `y(t) = vy t - g t²/2`: height above the launch point after `t` seconds.
pub fn y_at(vy: f64, t: f64) -> f64 {
    vy * t - G * t * t / 2.0
}

/// Position relative to the launch point after `t` seconds of free flight.
pub fn position_at(shot: Shot, t: f64) -> (f64, f64) {
    let (vx, vy) = shot.velocity();
    (x_at(vx, t), y_at(vy, t))
}

/// `T = 2 vy / g`: time to come back down **to launch height**, nothing in the way.
pub fn flight_time(vy: f64) -> f64 {
    2.0 * vy / G
}

/// `H = vy² / (2g)`: highest point **above the launch point**, nothing in the way.
pub fn max_height(vy: f64) -> f64 {
    vy * vy / (2.0 * G)
}

/// `R = v² sin(2a) / g`: sideways distance when the ball is back **at launch
/// height**, nothing in the way. This is not where it hits lower ground.
pub fn level_range(speed: f64, angle: f64) -> f64 {
    speed * speed * (2.0 * angle).sin() / G
}
