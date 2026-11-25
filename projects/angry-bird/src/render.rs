//! Drawing. The game works in metres; they become terminal cells only here
//! (the mouse uses the same `Viewport` to go from cells back to metres).

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::Line;
use ratatui::widgets::canvas::{Canvas, Context, Line as Stroke, Points};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::input::Aim;
use crate::level::{Role, Shape};
use crate::physics::{self, G, MAX_SPEED, Shot};
use crate::world::{Status, World};

pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 24;

/// Shown over the braille drawing.
pub const BASIC_NOTE: &str = "Basic dotted graphics. For smooth graphics use iTerm2 (see README) ";

/// The part of the world that must always be visible (m).
const VIEW_LEFT: f64 = 0.0;
const VIEW_RIGHT: f64 = 42.0;
/// Low enough to show the pouch pulled `MAX_PULL` straight down.
const VIEW_BOTTOM: f64 = -3.0;
const VIEW_TOP: f64 = 18.0;

const PANEL_HEIGHT: u16 = 11;
/// The dotted preview covers this much flight time (s), in this many dots.
const PREVIEW_SECONDS: f64 = 0.7;
const PREVIEW_DOTS: u32 = 9;

pub struct Areas {
    pub status: Rect,
    pub play: Rect,
    pub panel: Option<Rect>,
    pub help: Rect,
}

pub fn areas(screen: Rect, show_panel: bool) -> Areas {
    let panel_height = if show_panel { PANEL_HEIGHT } else { 0 };
    let [status, play, panel, help] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(panel_height),
        Constraint::Length(1),
    ])
    .areas(screen);
    Areas {
        status,
        play,
        panel: show_panel.then_some(panel),
        help,
    }
}

/// Maps between terminal cells and world metres.
///
/// A braille cell holds 2 × 4 dots. Terminal cells are about twice as tall as
/// wide, so those dots are roughly square. Using the same metres-per-dot both
/// ways keeps a 45° angle looking like 45°.
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub area: Rect,
    pub x_bounds: [f64; 2],
    pub y_bounds: [f64; 2],
}

impl Viewport {
    pub fn new(area: Rect) -> Self {
        let dots_x = f64::from((area.width * 2).max(2) - 1);
        let dots_y = f64::from((area.height * 4).max(2) - 1);
        let metres_per_dot =
            ((VIEW_RIGHT - VIEW_LEFT) / dots_x).max((VIEW_TOP - VIEW_BOTTOM) / dots_y);
        Self {
            area,
            x_bounds: [VIEW_LEFT, VIEW_LEFT + metres_per_dot * dots_x],
            y_bounds: [VIEW_BOTTOM, VIEW_BOTTOM + metres_per_dot * dots_y],
        }
    }

    /// The size of one braille dot (m).
    pub fn metres_per_dot(&self) -> f64 {
        (self.x_bounds[1] - self.x_bounds[0]) / f64::from((self.area.width * 2).max(2) - 1)
    }

    /// Where a world position falls in the play area, in cells (with fractions)
    /// from its top left corner. This is the inverse of `cell_to_world`, where a
    /// cell's middle is at its column + 0.5 and row + 0.5.
    pub fn world_to_cells(&self, (x, y): (f64, f64)) -> (f64, f64) {
        let dots_x = f64::from((self.area.width * 2).max(2) - 1);
        let dots_y = f64::from((self.area.height * 4).max(2) - 1);
        let [left, right] = self.x_bounds;
        let [bottom, top] = self.y_bounds;
        let dot_x = (x - left) / (right - left) * dots_x;
        let dot_y = (top - y) / (top - bottom) * dots_y;
        ((dot_x + 0.5) / 2.0, (dot_y + 0.5) / 4.0)
    }

    pub fn contains(&self, column: u16, row: u16) -> bool {
        self.area.contains((column, row).into())
    }

    /// The world position at the middle of a terminal cell. Cells outside the
    /// play area give positions outside the bounds, which dragging relies on.
    pub fn cell_to_world(&self, column: u16, row: u16) -> (f64, f64) {
        let dot_x = (f64::from(column) - f64::from(self.area.x)) * 2.0 + 0.5;
        let dot_y = (f64::from(row) - f64::from(self.area.y)) * 4.0 + 1.5;
        let dots_x = f64::from((self.area.width * 2).max(2) - 1);
        let dots_y = f64::from((self.area.height * 4).max(2) - 1);
        let [left, right] = self.x_bounds;
        let [bottom, top] = self.y_bounds;
        (
            left + dot_x / dots_x * (right - left),
            top - dot_y / dots_y * (top - bottom),
        )
    }
}

/// Preview dot positions in world metres, computed with the free-flight
/// formulas from the same launch point the ball is thrown from. Stops at the
/// ground. It ignores obstacles.
pub fn preview_points(launch: (f64, f64), shot: Shot) -> Vec<(f64, f64)> {
    (1..=PREVIEW_DOTS)
        .map(|i| {
            let t = PREVIEW_SECONDS * f64::from(i) / f64::from(PREVIEW_DOTS);
            let (x, y) = physics::position_at(shot, t);
            (launch.0 + x, launch.1 + y)
        })
        .take_while(|&(_, y)| y >= 0.0)
        .collect()
}

/// The shot being flown, or else the one being aimed.
fn shown_shot(world: &World, aim: &Aim) -> Shot {
    match &world.flight {
        Some(flight) if flight.active => flight.shot,
        _ => aim.shot,
    }
}

/// How the play area is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayStyle {
    /// Braille characters. Works in any terminal.
    Braille,
    /// The play area is left blank for an image drawn over it (see `graphics`).
    Pixels,
}

/// One shape in the play area, in world metres. Both ways of drawing use these.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Item {
    Line {
        from: (f64, f64),
        to: (f64, f64),
        color: Color,
    },
    /// A box. Corners go anticlockwise.
    Polygon {
        corners: [(f64, f64); 4],
        color: Color,
    },
    /// A ball.
    Circle {
        centre: (f64, f64),
        radius: f64,
        color: Color,
    },
    /// A line from a ball's centre to its edge, showing it roll.
    Spoke {
        from: (f64, f64),
        to: (f64, f64),
        color: Color,
    },
    Dot {
        at: (f64, f64),
        color: Color,
    },
    /// The `+` marking the launch point. It is drawn on top of everything.
    Cross {
        at: (f64, f64),
    },
}

/// The message shown over the play area when the game is over.
pub struct Banner {
    pub text: &'static str,
    pub color: Color,
    pub area: Rect,
}

pub fn banner(world: &World, play: Rect) -> Option<Banner> {
    let (text, color) = match world.status() {
        Status::Won => (
            " ALL KNOCKED DOWN - YOU WIN!  r: new layout  t: retry  q: quit ",
            Color::Green,
        ),
        Status::Lost => (
            " Out of shots.  r: new layout  t: retry  q: quit ",
            Color::Red,
        ),
        Status::Playing => return None,
    };
    let width = (text.len() as u16).min(play.width);
    let area = Rect::new(play.x + (play.width - width) / 2, play.y + 1, width, 1);
    Some(Banner { text, color, area })
}

pub fn fits(screen: Rect) -> bool {
    screen.width >= MIN_WIDTH && screen.height >= MIN_HEIGHT
}

pub fn draw(frame: &mut Frame, world: &World, aim: &Aim, show_panel: bool) {
    draw_styled(frame, world, aim, show_panel, PlayStyle::Braille, 1.0);
}

pub fn draw_styled(
    frame: &mut Frame,
    world: &World,
    aim: &Aim,
    show_panel: bool,
    style: PlayStyle,
    blend: f64,
) {
    let screen = frame.area();
    if !fits(screen) {
        let text = format!(
            "Please make the terminal at least {MIN_WIDTH}x{MIN_HEIGHT} (now {}x{}). q quits.",
            screen.width, screen.height
        );
        frame.render_widget(Paragraph::new(text).wrap(Default::default()), screen);
        return;
    }
    let areas = areas(screen, show_panel);
    let shot = shown_shot(world, aim);

    let status = format!(
        " Angle {:5.1}°  Speed {:5.2} m/s{}  Shots {}/{}  Targets left {}{}",
        shot.degrees(),
        shot.speed,
        if shot.speed >= MAX_SPEED - 1e-9 {
            " MAX"
        } else {
            "    "
        },
        world.shots_left,
        world.level.shots,
        world.targets_left(),
        match world.level.number {
            0 => String::new(),
            number => format!("  Layout {number}"),
        },
    );
    frame.render_widget(
        Paragraph::new(status).style(Style::new().add_modifier(Modifier::BOLD)),
        areas.status,
    );

    // In pixel style the image and the banner are written after this frame.
    if style == PlayStyle::Braille {
        draw_braille(frame, areas.play, world, aim, show_panel, blend);
    }

    if let Some(area) = areas.panel {
        let lines: Vec<Line> = panel_lines(world, aim)
            .into_iter()
            .map(Line::from)
            .collect();
        let block = Block::new()
            .borders(Borders::TOP)
            .title(" The math of this shot ");
        frame.render_widget(Paragraph::new(lines).block(block), area);
    }

    let help = if aim.dragging() {
        " Release to throw. Right-click cancels."
    } else {
        " Drag back | ↑↓ angle ←→ speed, space fires | m math r new t retry q quit"
    };
    frame.render_widget(
        Paragraph::new(help).style(Style::new().fg(Color::DarkGray)),
        areas.help,
    );
}

fn draw_braille(
    frame: &mut Frame,
    play: Rect,
    world: &World,
    aim: &Aim,
    show_panel: bool,
    blend: f64,
) {
    let view = Viewport::new(play);
    let dot = view.metres_per_dot();
    // Boxes are inset by one dot so touching boxes stay visibly separate.
    let items = scene(&view, world, aim, show_panel, dot, blend);
    let canvas = Canvas::default()
        .marker(Marker::Braille)
        .x_bounds(view.x_bounds)
        .y_bounds(view.y_bounds)
        .paint(|ctx| paint(ctx, &items));
    frame.render_widget(canvas, play);

    // Say why it looks dotted, if the terminal is wide enough (it always is).
    let note = Rect::new(play.x, play.y, play.width, 1);
    frame.render_widget(
        Paragraph::new(BASIC_NOTE)
            .alignment(Alignment::Right)
            .style(Style::new().fg(Color::DarkGray)),
        note,
    );

    if let Some(banner) = banner(world, play) {
        let style = Style::new()
            .fg(Color::Black)
            .bg(banner.color)
            .add_modifier(Modifier::BOLD);
        frame.render_widget(Paragraph::new(banner.text).style(style), banner.area);
    }
}

/// The math panel text, with this shot's numbers substituted in. While the ball
/// is in free flight, `t` is the live flight time; while aiming it is `T/2`.
pub fn panel_lines(world: &World, aim: &Aim) -> Vec<String> {
    let shot = shown_shot(world, aim);
    let (launch_x, launch_y) = world.level.launch;
    let (v, degrees) = (shot.speed, shot.degrees());
    let (vx, vy) = shot.velocity();
    let big_t = physics::flight_time(vy);
    let big_h = physics::max_height(vy);
    let big_r = physics::level_range(v, shot.angle);

    let active = world.flight.as_ref().filter(|f| f.active);
    let t = active.map_or(big_t / 2.0, |f| f.free_time());
    let (x, y) = physics::position_at(shot, t);

    let when = match active {
        None => format!("Aiming: t = T/2 = {t:.2} s. x, y, H are measured from the launch point +"),
        Some(f) => {
            let state = if f.has_hit {
                "first hit, formulas stop at"
            } else {
                "flying, t ="
            };
            match world.ball_position().filter(|_| !f.has_hit) {
                Some((bx, by)) => format!(
                    "{state} {t:.2} s  game's ball: x = {:.2}  y = {:.2}",
                    bx - launch_x,
                    by - launch_y
                ),
                None => format!("{state} t = {t:.2} s (bounces are left to the physics engine)"),
            }
        }
    };

    vec![
        format!("a = {degrees:.1}°   v = {v:.2} m/s   g = {G} m/s²"),
        format!("vx = v cos(a) = {v:.2} × cos({degrees:.1}°) = {vx:.2} m/s"),
        format!("vy = v sin(a) = {v:.2} × sin({degrees:.1}°) = {vy:.2} m/s"),
        format!("x(t) = vx t = {vx:.2} × {t:.2} = {x:.2} m"),
        format!("y(t) = vy t - g t²/2 = {vy:.2}×{t:.2} - {G}×{t:.2}²/2 = {y:.2} m"),
        format!("T = 2 vy/g = 2×{vy:.2}/{G} = {big_t:.2} s   (*)"),
        format!(
            "H = vy²/(2g) = {vy:.2}²/(2×{G}) = {big_h:.2} m ({:.2} m above ground)",
            launch_y + big_h
        ),
        format!(
            "R = v² sin(2a)/g = {v:.2}² × sin({:.1}°)/{G} = {big_r:.2} m (*)",
            2.0 * degrees
        ),
        format!(
            "(*) T, R assume landing at launch height ({launch_y:.1} m up), nothing in the way."
        ),
        when,
    ]
}

/// Everything in the play area, back to front. `inset` is how much smaller than
/// their real size boxes are drawn (m). `blend` places moving things between the
/// last two physics steps (see `World::piece_pose_between`).
pub fn scene(
    view: &Viewport,
    world: &World,
    aim: &Aim,
    show_panel: bool,
    inset: f64,
    blend: f64,
) -> Vec<Item> {
    let mut items = Vec::new();
    let launch = world.level.launch;
    let (launch_x, launch_y) = launch;
    let dot = view.metres_per_dot();

    line(
        &mut items,
        (view.x_bounds[0], 0.0),
        (view.x_bounds[1], 0.0),
        Color::Green,
    );

    if show_panel {
        // Dashed line at launch height: where T and R say the ball "lands".
        let dashes = (0..)
            .map(|i| (launch_x + f64::from(i) * dot * 5.0, launch_y))
            .take_while(|&(x, _)| x <= view.x_bounds[1]);
        dots(&mut items, dashes, Color::DarkGray);
    }

    // The slingshot: a post with a two-pronged fork around the launch point.
    let fork = (launch_x, launch_y - 1.0);
    let prongs = [
        (launch_x - 0.6, launch_y + 0.2),
        (launch_x + 0.6, launch_y + 0.2),
    ];
    line(&mut items, (launch_x, 0.0), fork, Color::Yellow);
    for prong in prongs {
        line(&mut items, fork, prong, Color::Yellow);
    }

    for piece in &world.pieces {
        let (x, y, turn) = world.piece_pose_between(piece, blend);
        let color = match (piece.object.role, piece.down, piece.object.shape) {
            (Role::Fixed, ..) => Color::Gray,
            (_, true, _) => Color::DarkGray,
            (_, _, Shape::Ball { .. }) => Color::Green,
            _ => Color::LightRed,
        };
        match piece.object.shape {
            Shape::Box { width, height } => {
                let (sin, cos) = turn.sin_cos();
                let (w, h) = ((width - inset) / 2.0, (height - inset) / 2.0);
                let corners = [(-w, -h), (w, -h), (w, h), (-w, h)]
                    .map(|(cx, cy)| (x + cx * cos - cy * sin, y + cx * sin + cy * cos));
                items.push(Item::Polygon { corners, color });
            }
            Shape::Ball { radius } => {
                circle(&mut items, (x, y), radius, color);
                items.push(Item::Spoke {
                    from: (x, y),
                    to: (x + radius * turn.cos(), y + radius * turn.sin()),
                    color,
                });
            }
        }
    }

    dots(&mut items, world.trail.iter().copied(), Color::Cyan);
    if let Some(at) = world.ball_position_between(blend) {
        circle(&mut items, at, world.level.ball_radius, Color::White);
    }

    if world.ready_to_fire() {
        // The pouch is drawn pulled back; the throw itself starts at the +.
        let (dx, dy) = aim.shot.pull_offset();
        let pouch = (launch_x + dx, launch_y + dy);
        for prong in prongs {
            line(&mut items, prong, pouch, Color::Magenta);
        }
        circle(&mut items, pouch, world.level.ball_radius, Color::White);
        let preview = preview_points(launch, aim.shot);
        dots(&mut items, preview, Color::LightYellow);
    }

    items.push(Item::Cross { at: launch });
    items
}

fn line(items: &mut Vec<Item>, from: (f64, f64), to: (f64, f64), color: Color) {
    items.push(Item::Line { from, to, color });
}

fn circle(items: &mut Vec<Item>, centre: (f64, f64), radius: f64, color: Color) {
    items.push(Item::Circle {
        centre,
        radius,
        color,
    });
}

fn dots(items: &mut Vec<Item>, at: impl IntoIterator<Item = (f64, f64)>, color: Color) {
    items.extend(at.into_iter().map(|at| Item::Dot { at, color }));
}

fn paint(ctx: &mut Context, items: &[Item]) {
    for &item in items {
        match item {
            Item::Line { from, to, color } | Item::Spoke { from, to, color } => {
                stroke(ctx, from, to, color);
            }
            Item::Polygon { corners, color } => {
                for i in 0..4 {
                    stroke(ctx, corners[i], corners[(i + 1) % 4], color);
                }
            }
            Item::Circle {
                centre,
                radius,
                color,
            } => polygon(ctx, centre, radius, color),
            Item::Dot { at, color } => ctx.draw(&Points {
                coords: &[at],
                color,
            }),
            Item::Cross { at } => {
                ctx.layer();
                ctx.print(at.0, at.1, Line::styled("+", Style::new().fg(Color::White)));
            }
        }
    }
}

fn stroke(ctx: &mut Context, from: (f64, f64), to: (f64, f64), color: Color) {
    ctx.draw(&Stroke::new(from.0, from.1, to.0, to.1, color));
}

fn polygon(ctx: &mut Context, centre: (f64, f64), radius: f64, color: Color) {
    const SIDES: u32 = 12;
    let point = |i: u32| {
        let a = std::f64::consts::TAU * f64::from(i) / f64::from(SIDES);
        (centre.0 + radius * a.cos(), centre.1 + radius * a.sin())
    };
    for i in 0..SIDES {
        stroke(ctx, point(i), point(i + 1), color);
    }
}
