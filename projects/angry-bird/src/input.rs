//! Turns keyboard and mouse events into aim changes and game commands.

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::physics::{self, MAX_ANGLE, MAX_SPEED, MIN_ANGLE, MIN_SPEED, Shot};
use crate::render::Viewport;

/// A drag must start this close to the launch point (m).
pub const GRAB_RADIUS: f64 = 3.0;
const ANGLE_STEP_DEGREES: f64 = 1.0;
const SPEED_STEP: f64 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    None,
    Fire(Shot),
    /// Start again with a new layout.
    Restart,
    /// Start again with the same layout.
    Retry,
    TogglePanel,
    Quit,
}

#[derive(Debug, Clone, Copy)]
pub struct Aim {
    pub shot: Shot,
    drag: Option<Drag>,
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    /// Aim to go back to if the drag is cancelled.
    before: Shot,
    moved: bool,
    /// False while the pull would not make a throw.
    valid: bool,
}

impl Default for Aim {
    fn default() -> Self {
        Self {
            shot: Shot::from_degrees(14.0, 40.0),
            drag: None,
        }
    }
}

impl Aim {
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    fn cancel_drag(&mut self) {
        if let Some(drag) = self.drag.take() {
            self.shot = drag.before;
        }
    }

    /// Moves the aim by whole key steps. The result lands on a multiple of the
    /// step, so exact values such as 30° stay reachable after a mouse drag.
    fn nudge(&mut self, angle_steps: f64, speed_steps: f64) {
        let stepped = |value: f64, step: f64, steps: f64| ((value / step).round() + steps) * step;
        if angle_steps != 0.0 {
            let degrees = stepped(self.shot.degrees(), ANGLE_STEP_DEGREES, angle_steps);
            self.shot.angle = degrees.to_radians().clamp(MIN_ANGLE, MAX_ANGLE);
        }
        if speed_steps != 0.0 {
            let speed = stepped(self.shot.speed, SPEED_STEP, speed_steps);
            self.shot.speed = speed.clamp(MIN_SPEED, MAX_SPEED);
        }
    }
}

/// `can_aim` is false while a ball is flying or the game is over.
pub fn handle_key(key: KeyEvent, aim: &mut Aim, can_aim: bool) -> Command {
    if key.kind == KeyEventKind::Release {
        return Command::None;
    }
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Command::Quit,
        KeyCode::Char('q' | 'Q') | KeyCode::Esc => Command::Quit,
        KeyCode::Char('r' | 'R') => {
            aim.cancel_drag();
            Command::Restart
        }
        KeyCode::Char('t' | 'T') => {
            aim.cancel_drag();
            Command::Retry
        }
        KeyCode::Char('m' | 'M') => {
            // The panel changes the screen layout, so a drag cannot survive it.
            aim.cancel_drag();
            Command::TogglePanel
        }
        KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right | KeyCode::Char(' ')
            if can_aim && !aim.dragging() =>
        {
            match key.code {
                KeyCode::Up => aim.nudge(1.0, 0.0),
                KeyCode::Down => aim.nudge(-1.0, 0.0),
                KeyCode::Right => aim.nudge(0.0, 1.0),
                KeyCode::Left => aim.nudge(0.0, -1.0),
                _ => return Command::Fire(aim.shot),
            }
            Command::None
        }
        _ => Command::None,
    }
}

pub fn handle_mouse(
    event: MouseEvent,
    aim: &mut Aim,
    view: &Viewport,
    launch: (f64, f64),
    can_aim: bool,
) -> Command {
    let (x, y) = view.cell_to_world(event.column, event.row);
    let (pull_x, pull_y) = (x - launch.0, y - launch.1);
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            aim.cancel_drag();
            if can_aim
                && view.contains(event.column, event.row)
                && pull_x.hypot(pull_y) <= GRAB_RADIUS
            {
                aim.drag = Some(Drag {
                    before: aim.shot,
                    moved: false,
                    valid: false,
                });
            }
        }
        // Dragging and releasing may happen anywhere, even outside the play area.
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(drag) = aim.drag.as_mut() {
                drag.moved = true;
                let shot = physics::pull_to_shot(pull_x, pull_y);
                drag.valid = shot.is_some_and(|s| s.speed >= MIN_SPEED);
                if let Some(shot) = shot {
                    aim.shot = shot;
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if let Some(drag) = aim.drag {
                // A click without a drag, or a tiny or backward pull, does not fire.
                if drag.moved && drag.valid && can_aim {
                    aim.drag = None;
                    return Command::Fire(aim.shot);
                }
                aim.cancel_drag();
            }
        }
        // A right click lets go without throwing.
        MouseEventKind::Down(MouseButton::Right) => aim.cancel_drag(),
        _ => {}
    }
    Command::None
}
