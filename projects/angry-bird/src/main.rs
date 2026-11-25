//! Terminal setup and the game loop.

use std::io::{self, IsTerminal, stdout};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;
use ratatui::DefaultTerminal;
use ratatui::layout::Rect;

use slingshot::graphics::{self, Protocol, Screen};
use slingshot::input::{self, Aim, Command};
use slingshot::level;
use slingshot::render::{self, Viewport};
use slingshot::world::{FixedStep, World};

/// Set to a layout number to start with that layout.
const LAYOUT_VARIABLE: &str = "SLINGSHOT_LAYOUT";

/// A different number each time, to pick layouts with.
fn seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos() as u64)
}

/// Restores the terminal when dropped: on quit, on an error and on a panic.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Removes any image and ends a frame that may have been cut short.
        let _ = graphics::cleanup(&mut stdout());
        let _ = execute!(stdout(), DisableMouseCapture);
        ratatui::restore();
    }
}

fn main() -> io::Result<()> {
    if !stdout().is_terminal() || !io::stdin().is_terminal() {
        return Err(io::Error::other(
            "slingshot must be run in an interactive terminal",
        ));
    }
    // `init` also installs a panic hook that restores the screen first.
    let mut terminal = ratatui::init();
    let _guard = TerminalGuard;
    execute!(stdout(), EnableMouseCapture)?;
    let protocol = Protocol::detect();
    let requested = std::env::var(graphics::FPS_VARIABLE).ok();
    let frame_time = graphics::frame_time(protocol, requested.as_deref());
    run(&mut terminal, Screen::new(protocol), frame_time)
}

fn run(terminal: &mut DefaultTerminal, mut screen: Screen, frame_time: Duration) -> io::Result<()> {
    // A new layout for each run, unless one is asked for by number.
    let number = std::env::var(LAYOUT_VARIABLE)
        .ok()
        .and_then(|text| text.parse().ok())
        .filter(|number| (1..=level::LAYOUTS).contains(number))
        .unwrap_or_else(|| level::pick_layout(seed(), 0));
    let mut world = World::new(level::layout(number));
    let mut aim = Aim::default();
    let mut show_panel = false;
    let mut clock = FixedStep::default();
    let mut last = Instant::now();
    let mut next_frame = last;

    loop {
        screen.draw(terminal, &world, &aim, show_panel, clock.fraction())?;

        // Frames start at even intervals; input is handled while waiting. After
        // a slow frame the schedule restarts instead of rushing to catch up.
        next_frame = (next_frame + frame_time).max(Instant::now());
        while event::poll(next_frame.saturating_duration_since(Instant::now()))? {
            let size = terminal.size()?;
            let whole = Rect::new(0, 0, size.width, size.height);
            let usable = render::fits(whole);
            let can_aim = usable && world.ready_to_fire();
            let command = match event::read()? {
                Event::Key(key) => input::handle_key(key, &mut aim, can_aim),
                Event::Mouse(mouse) => {
                    let view = Viewport::new(render::areas(whole, show_panel).play);
                    input::handle_mouse(mouse, &mut aim, &view, world.level.launch, can_aim)
                }
                _ => Command::None,
            };
            match command {
                Command::Quit => return Ok(()),
                Command::Restart => {
                    let next = level::pick_layout(seed(), world.level.number);
                    world = World::new(level::layout(next));
                }
                Command::Retry => world = World::new(world.level.clone()),
                Command::TogglePanel => show_panel = !show_panel,
                Command::Fire(shot) => {
                    world.fire(shot);
                }
                Command::None => {}
            }
        }

        let now = Instant::now();
        let steps = clock.steps_for((now - last).as_secs_f64());
        last = now;
        for _ in 0..steps {
            world.step();
        }
    }
}
