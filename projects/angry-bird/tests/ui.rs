//! Mouse and keyboard handling, and drawing into an in-memory terminal.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use slingshot::input::{Aim, Command, handle_key, handle_mouse};
use slingshot::level::LEVEL;
use slingshot::physics::{MAX_SPEED, Shot};
use slingshot::render::{self, Viewport};
use slingshot::world::World;

fn view() -> Viewport {
    Viewport::new(render::areas(Rect::new(0, 0, 100, 30), false).play)
}

/// The terminal cell nearest a world position.
fn cell(view: &Viewport, x: f64, y: f64) -> (u16, u16) {
    let area = view.area;
    (area.y..area.bottom())
        .flat_map(|row| (area.x..area.right()).map(move |column| (column, row)))
        .min_by(|a, b| {
            let distance = |&(c, r): &(u16, u16)| {
                let (wx, wy) = view.cell_to_world(c, r);
                (wx - x).hypot(wy - y)
            };
            distance(a).total_cmp(&distance(b))
        })
        .unwrap()
}

fn mouse(kind: MouseEventKind, (column, row): (u16, u16)) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn press(aim: &mut Aim, code: KeyCode) -> Command {
    handle_key(KeyEvent::new(code, KeyModifiers::NONE), aim, true)
}

#[test]
fn dragging_back_and_releasing_fires_the_opposite_way() {
    let view = view();
    let (lx, ly) = LEVEL.launch;
    let mut aim = Aim::default();
    let start = cell(&view, lx, ly);
    let end = cell(&view, lx - 2.0, ly - 2.0);
    let send =
        |aim: &mut Aim, kind, at| handle_mouse(mouse(kind, at), aim, &view, LEVEL.launch, true);
    send(&mut aim, MouseEventKind::Down(MouseButton::Left), start);
    send(&mut aim, MouseEventKind::Drag(MouseButton::Left), end);
    let Command::Fire(shot) = send(&mut aim, MouseEventKind::Up(MouseButton::Left), end) else {
        panic!("did not fire");
    };
    // One cell is large, so allow for rounding to the nearest cell.
    assert!(
        (shot.degrees() - 45.0).abs() < 12.0,
        "angle {}",
        shot.degrees()
    );
    assert!(
        (shot.speed - 8f64.sqrt() * 5.0).abs() < 3.0,
        "speed {}",
        shot.speed
    );
}

#[test]
fn clicks_far_drags_and_right_click_do_not_fire() {
    let view = view();
    let (lx, ly) = LEVEL.launch;
    let (start, far, back) = (
        cell(&view, lx, ly),
        cell(&view, lx + 20.0, ly),
        cell(&view, lx - 2.0, ly - 1.0),
    );
    use MouseEventKind::{Down, Drag, Up};
    let left = MouseButton::Left;
    for events in [
        vec![(Down(left), start), (Up(left), start)],
        vec![(Down(left), far), (Drag(left), back), (Up(left), back)],
        vec![
            (Down(left), start),
            (Drag(left), back),
            (Down(MouseButton::Right), back),
            (Up(left), back),
        ],
        // Pulled forward and up: that would throw backward.
        vec![
            (Down(left), start),
            (Drag(left), cell(&view, lx + 2.0, ly + 2.0)),
            (Up(left), start),
        ],
    ] {
        let mut aim = Aim::default();
        for (kind, at) in events {
            let command = handle_mouse(mouse(kind, at), &mut aim, &view, LEVEL.launch, true);
            assert_eq!(command, Command::None);
        }
    }
}

#[test]
fn arrow_keys_snap_to_exact_values_after_a_mouse_aim() {
    let mut aim = Aim::default();
    aim.shot = Shot::from_degrees(18.034, 27.23);
    press(&mut aim, KeyCode::Up);
    press(&mut aim, KeyCode::Left);
    assert!((aim.shot.degrees() - 28.0).abs() < 1e-9);
    assert!((aim.shot.speed - 17.75).abs() < 1e-9);
}

#[test]
fn arrows_aim_space_fires_and_command_keys_work() {
    let mut aim = Aim::default();
    let before = aim.shot;
    press(&mut aim, KeyCode::Up);
    press(&mut aim, KeyCode::Right);
    assert!((aim.shot.degrees() - before.degrees() - 1.0).abs() < 1e-9);
    assert!((aim.shot.speed - before.speed - 0.25).abs() < 1e-9);
    assert_eq!(press(&mut aim, KeyCode::Char(' ')), Command::Fire(aim.shot));
    for _ in 0..200 {
        press(&mut aim, KeyCode::Up);
        press(&mut aim, KeyCode::Right);
    }
    assert!((aim.shot.degrees() - 90.0).abs() < 1e-9);
    assert!((aim.shot.speed - MAX_SPEED).abs() < 1e-9);
    assert_eq!(press(&mut aim, KeyCode::Char('r')), Command::Restart);
    assert_eq!(press(&mut aim, KeyCode::Char('m')), Command::TogglePanel);
    assert_eq!(press(&mut aim, KeyCode::Char('q')), Command::Quit);
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(handle_key(ctrl_c, &mut aim, false), Command::Quit);
    let space = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
    assert_eq!(handle_key(space, &mut aim, false), Command::None);
}

fn screen(width: u16, height: u16, world: &World, panel: bool) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| render::draw(f, world, &Aim::default(), panel))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                + "\n"
        })
        .collect()
}

#[test]
fn screen_shows_status_and_the_whole_panel_at_the_smallest_size() {
    let mut world = World::new(LEVEL);
    let plain = screen(80, 24, &world, false);
    assert!(plain.contains("Angle  40.0°"), "{plain}");
    assert!(plain.contains("m math r new t retry q quit"), "{plain}");
    assert!(!plain.contains("math of this shot"));

    // Every panel line must fit, both while aiming and in flight.
    for step in 0..400 {
        if step == 1 {
            world.fire(Aim::default().shot);
        }
        if step % 20 == 0 {
            let text = screen(80, 24, &world, true);
            for line in render::panel_lines(&world, &Aim::default()) {
                assert!(text.contains(&line), "cut off: {line}\n{text}");
            }
        }
        world.step();
    }
    assert!(screen(60, 20, &world, false).contains("at least 80x24"));
}

#[test]
fn r_asks_for_a_new_layout_and_t_for_the_same_one() {
    let mut aim = Aim::default();
    assert_eq!(press(&mut aim, KeyCode::Char('r')), Command::Restart);
    assert_eq!(press(&mut aim, KeyCode::Char('t')), Command::Retry);

    // The layout number and the whole help line fit at the smallest size.
    let world = World::new(slingshot::level::layout(slingshot::level::LAYOUTS));
    let text = screen(80, 24, &world, false);
    assert!(text.contains("Targets left 7  Layout 1000"), "{text}");
    assert!(text.contains("m math r new t retry q quit"), "{text}");
}
