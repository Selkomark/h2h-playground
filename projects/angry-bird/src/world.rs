//! The game world: rapier2d handles collisions, bouncing, spinning, stacking
//! and toppling. The world always advances in fixed steps of `DT` seconds.

use std::f64::consts::{PI, TAU};

use rapier2d::prelude::*;

use crate::level::{Level, Object, Role, Shape};
use crate::physics::{self, Shot};

/// The fixed physics time step (s).
pub const DT: f64 = 1.0 / 120.0;
/// A target is down once its centre has moved this far from its start (m)...
pub const DOWN_DISTANCE: f64 = 0.8;
/// ...or it has tipped this far (radians, 35°). Balls only use distance.
pub const DOWN_TILT: f64 = 35.0 * std::f64::consts::PI / 180.0;

const MAX_SHOT_SECONDS: f64 = 10.0;
const CALM_SPEED: f64 = 0.2;
const CALM_SECONDS: f64 = 0.75;

/// Turns uneven frame times into a whole number of fixed physics steps, so the
/// simulation does not depend on the frame rate.
#[derive(Debug, Default)]
pub struct FixedStep {
    leftover: f64,
}

impl FixedStep {
    /// The most real time one frame may add. After a long stall the game slows
    /// down briefly instead of running a burst of thousands of steps.
    const MAX_FRAME: f64 = 0.25;

    pub fn steps_for(&mut self, frame_seconds: f64) -> u32 {
        self.leftover += frame_seconds.clamp(0.0, Self::MAX_FRAME);
        let steps = (self.leftover / DT).floor();
        self.leftover -= steps * DT;
        steps as u32
    }

    /// How far real time is into the next step, from 0 to 1.
    pub fn fraction(&self) -> f64 {
        (self.leftover / DT).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Playing,
    Won,
    Lost,
}

/// A level object and its physics body.
pub struct Piece {
    pub object: Object,
    pub down: bool,
    /// The pose before the latest step, only used to draw smooth motion.
    previous: (f64, f64, f64),
    handle: RigidBodyHandle,
}

/// The ball that was last thrown.
pub struct Flight {
    pub shot: Shot,
    /// True until the shot is over; no new shot can be fired meanwhile.
    pub active: bool,
    /// Physics steps from launch until the first touch (or until now).
    free_steps: u32,
    steps: u32,
    calm_steps: u32,
    pub has_hit: bool,
    /// The centre before the latest step, only used to draw smooth motion.
    previous: (f64, f64),
    handle: RigidBodyHandle,
    collider: ColliderHandle,
}

impl Flight {
    /// Seconds of untouched flight. The formulas apply only to this part.
    pub fn free_time(&self) -> f64 {
        f64::from(self.free_steps) * DT
    }
}

pub struct World {
    pub level: Level,
    pub pieces: Vec<Piece>,
    pub flight: Option<Flight>,
    /// Ball centre positions of the current or last shot, in world metres.
    pub trail: Vec<(f64, f64)>,
    pub shots_left: u32,
    physics: PhysicsWorld,
}

fn vector(x: f64, y: f64) -> Vector {
    Vector::new(x as f32, y as f32)
}

impl World {
    pub fn new(level: Level) -> Self {
        let mut physics = PhysicsWorld::new();
        physics.gravity = vector(0.0, -physics::G);
        physics.integration_parameters.dt = DT as f32;

        // The ground: a thick fixed slab whose top surface is y = 0.
        physics.insert(
            RigidBodyBuilder::fixed().translation(vector(40.0, -5.0)),
            ColliderBuilder::cuboid(100.0, 5.0).friction(0.8),
        );

        let pieces = level
            .objects
            .iter()
            .map(|&object| {
                let body = match object.role {
                    Role::Target => RigidBodyBuilder::dynamic().angular_damping(0.5),
                    Role::Fixed => RigidBodyBuilder::fixed(),
                }
                .translation(vector(object.x, object.y));
                let collider = match object.shape {
                    Shape::Box { width, height } => {
                        ColliderBuilder::cuboid(width as f32 / 2.0, height as f32 / 2.0)
                    }
                    Shape::Ball { radius } => ColliderBuilder::ball(radius as f32),
                }
                .density(1.0)
                .friction(0.6)
                .restitution(0.1);
                let (handle, _) = physics.insert(body, collider);
                Piece {
                    object,
                    down: false,
                    previous: (object.x, object.y, 0.0),
                    handle,
                }
            })
            .collect();

        let shots_left = level.shots;
        Self {
            level,
            pieces,
            flight: None,
            trail: Vec::new(),
            shots_left,
            physics,
        }
    }

    /// Current centre `(x, y)` and rotation (radians) of a piece.
    pub fn piece_pose(&self, piece: &Piece) -> (f64, f64, f64) {
        let body = &self.physics.bodies[piece.handle];
        let at = body.translation();
        (
            f64::from(at.x),
            f64::from(at.y),
            f64::from(body.rotation().angle()),
        )
    }

    /// Where to draw a piece: `blend` of the way from its pose before the latest
    /// step (0) to its current pose (1). Frames rarely fall exactly on physics
    /// steps, so drawing between them keeps the motion even.
    pub fn piece_pose_between(&self, piece: &Piece, blend: f64) -> (f64, f64, f64) {
        let (x, y, turn) = self.piece_pose(piece);
        let (old_x, old_y, old_turn) = piece.previous;
        let back = 1.0 - blend;
        // The short way round, as angles jump between -180° and 180°.
        let turned = (turn - old_turn + PI).rem_euclid(TAU) - PI;
        (
            x - (x - old_x) * back,
            y - (y - old_y) * back,
            turn - turned * back,
        )
    }

    /// Like `piece_pose_between`, for the thrown ball.
    pub fn ball_position_between(&self, blend: f64) -> Option<(f64, f64)> {
        let (x, y) = self.ball_position()?;
        let (old_x, old_y) = self.flight.as_ref()?.previous;
        let back = 1.0 - blend;
        Some((x - (x - old_x) * back, y - (y - old_y) * back))
    }

    /// Centre of the thrown ball, if there is one in the world.
    pub fn ball_position(&self) -> Option<(f64, f64)> {
        let flight = self.flight.as_ref()?;
        let at = self.physics.bodies[flight.handle].translation();
        Some((f64::from(at.x), f64::from(at.y)))
    }

    pub fn targets_left(&self) -> usize {
        self.pieces
            .iter()
            .filter(|p| p.object.role == Role::Target && !p.down)
            .count()
    }

    pub fn ready_to_fire(&self) -> bool {
        self.status() == Status::Playing
            && self.shots_left > 0
            && !self.flight.as_ref().is_some_and(|f| f.active)
    }

    pub fn status(&self) -> Status {
        if self.targets_left() == 0 {
            Status::Won
        } else if self.shots_left == 0 && !self.flight.as_ref().is_some_and(|f| f.active) {
            Status::Lost
        } else {
            Status::Playing
        }
    }

    /// Throws a ball from the launch point. Returns false if that is not allowed now.
    pub fn fire(&mut self, shot: Shot) -> bool {
        if !self.ready_to_fire() {
            return false;
        }
        // The previous ball leaves the world when the next one is thrown.
        if let Some(old) = self.flight.take() {
            self.physics.remove_body(old.handle);
        }
        let (x, y) = self.level.launch;
        let (vx, vy) = shot.velocity();
        // No linear damping: that would act like air drag. Angular damping only
        // slows spinning, so a rolling ball eventually stops.
        let body = RigidBodyBuilder::dynamic()
            .translation(vector(x, y))
            .linvel(vector(vx, vy))
            .angular_damping(1.5)
            .ccd_enabled(true);
        let collider = ColliderBuilder::ball(self.level.ball_radius as f32)
            .density(3.0)
            .friction(0.6)
            .restitution(0.3);
        let (handle, collider) = self.physics.insert(body, collider);
        self.flight = Some(Flight {
            shot,
            active: true,
            free_steps: 0,
            steps: 0,
            calm_steps: 0,
            has_hit: false,
            previous: (x, y),
            handle,
            collider,
        });
        self.shots_left -= 1;
        self.trail = vec![(x, y)];
        true
    }

    /// Advances the world by exactly `DT` seconds.
    pub fn step(&mut self) {
        // Extra forces such as air drag or wind would be applied here, before
        // the physics step. See README.md.
        self.remember_poses();
        self.physics.step();
        self.mark_fallen();
        self.update_flight();
    }

    fn remember_poses(&mut self) {
        for i in 0..self.pieces.len() {
            self.pieces[i].previous = self.piece_pose(&self.pieces[i]);
        }
        if let Some(at) = self.ball_position()
            && let Some(flight) = self.flight.as_mut()
        {
            flight.previous = at;
        }
    }

    fn mark_fallen(&mut self) {
        for piece in &mut self.pieces {
            if piece.object.role != Role::Target || piece.down {
                continue;
            }
            let body = &self.physics.bodies[piece.handle];
            let at = body.translation();
            let moved = (f64::from(at.x) - piece.object.x).hypot(f64::from(at.y) - piece.object.y);
            let tipped = matches!(piece.object.shape, Shape::Box { .. })
                && f64::from(body.rotation().angle()).abs() > DOWN_TILT;
            // Once down, a target stays down.
            piece.down = moved > DOWN_DISTANCE || tipped;
        }
    }

    fn update_flight(&mut self) {
        let Some(flight) = self.flight.as_mut() else {
            return;
        };
        if !flight.active {
            return;
        }
        flight.steps += 1;
        if !flight.has_hit {
            flight.free_steps += 1;
            flight.has_hit = self
                .physics
                .narrow_phase
                .contact_pairs_with(flight.collider)
                .any(|pair| pair.has_any_active_contact());
        }

        let body = &self.physics.bodies[flight.handle];
        let at = body.translation();
        let (x, y) = (f64::from(at.x), f64::from(at.y));
        if flight.steps % 3 == 0 {
            self.trail.push((x, y));
        }

        let everything_calm = self
            .physics
            .bodies
            .iter()
            .filter(|(_, b)| b.is_dynamic())
            .all(|(_, b)| f64::from(b.linvel().length()) < CALM_SPEED);
        flight.calm_steps = if everything_calm {
            flight.calm_steps + 1
        } else {
            0
        };

        let gone = !(-10.0..=70.0).contains(&x) || y < -10.0;
        if gone
            || f64::from(flight.calm_steps) * DT >= CALM_SECONDS
            || f64::from(flight.steps) * DT >= MAX_SHOT_SECONDS
        {
            flight.active = false;
        }
    }
}
