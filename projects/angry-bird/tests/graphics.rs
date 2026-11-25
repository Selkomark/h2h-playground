//! The higher resolution drawing: when it is used, and that it lines up with the
//! cells the mouse works in.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ratatui::layout::Rect;
use slingshot::graphics::{Image, Protocol, cell_pixels, image_escape};
use slingshot::input::Aim;
use slingshot::level::{LEVEL, Shape};
use slingshot::physics::Shot;
use slingshot::render::{self, Viewport};
use slingshot::world::{FixedStep, World};

fn detect(variables: &[(&str, &str)]) -> Option<Protocol> {
    Protocol::from_env(|name| {
        variables
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.to_string())
    })
}

#[test]
fn images_are_used_only_in_terminals_known_to_show_them() {
    let program = "TERM_PROGRAM";
    assert_eq!(detect(&[(program, "iTerm.app")]), Some(Protocol::Iterm));
    assert_eq!(detect(&[(program, "WezTerm")]), Some(Protocol::Iterm));
    assert_eq!(detect(&[("LC_TERMINAL", "iTerm2")]), Some(Protocol::Iterm));
    assert_eq!(detect(&[(program, "ghostty")]), Some(Protocol::Kitty));
    assert_eq!(detect(&[("TERM", "xterm-kitty")]), Some(Protocol::Kitty));
    assert_eq!(detect(&[(program, "Apple_Terminal")]), None);
    assert_eq!(detect(&[("TERM", "xterm-256color")]), None);
    assert_eq!(detect(&[(program, "iTerm.app"), ("TMUX", "x")]), None);
    let forced = "SLINGSHOT_GRAPHICS";
    assert_eq!(detect(&[(program, "iTerm.app"), (forced, "braille")]), None);
    assert_eq!(detect(&[(forced, "kitty")]), Some(Protocol::Kitty));
}

#[test]
fn image_positions_match_the_cells_the_mouse_uses() {
    for (width, height, panel) in [(80, 24, false), (80, 24, true), (213, 61, false)] {
        let view = Viewport::new(render::areas(Rect::new(0, 0, width, height), panel).play);
        let area = view.area;
        for row in area.y..area.bottom() {
            for column in area.x..area.right() {
                let (x, y) = view.world_to_cells(view.cell_to_world(column, row));
                assert!((x - f64::from(column - area.x) - 0.5).abs() < 1e-9);
                assert!((y - f64::from(row - area.y) - 0.5).abs() < 1e-9);
            }
        }
    }
}

#[test]
fn image_has_many_more_pixels_than_braille_has_dots_and_shows_the_launch_point() {
    let view = Viewport::new(render::areas(Rect::new(0, 0, 100, 30), false).play);
    let world = World::new(LEVEL);
    let image = Image::render(&view, &world, &Aim::default(), false, (16, 32), 1.0);
    let (columns, rows) = (u32::from(view.area.width), u32::from(view.area.height));
    assert_eq!((image.width, image.height), (columns * 16, rows * 32));
    let (column, row) = view.world_to_cells(LEVEL.launch);
    let (x, y) = ((column * 16.0) as u32, (row * 32.0) as u32);
    let at = ((y * image.width + x) * 4) as usize;
    assert_eq!(image.pixels[at..at + 4], [240, 240, 240, 255]);
    // The sky stays transparent.
    assert_eq!(image.pixels[3], 0);

    // Very large terminals are limited in size.
    let wide = Viewport::new(Rect::new(0, 0, 500, 100));
    assert!(Image::render(&wide, &world, &Aim::default(), true, (16, 32), 1.0).width <= 2400);

    let png_bytes = image.to_png().unwrap();
    let mut reader = png::Decoder::new(std::io::Cursor::new(png_bytes))
        .read_info()
        .unwrap();
    let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut decoded).unwrap();
    assert_eq!(decoded, image.pixels);
}

#[test]
fn escape_sequences_carry_the_whole_image_and_its_size_in_cells() {
    let data: Vec<u8> = (0..10_000).map(|i| (i % 251) as u8).collect();

    let iterm = image_escape(Protocol::Iterm, &data, 100, 28);
    assert!(iterm.starts_with("\x1b]1337;File=inline=1;size=10000;width=100;height=28;"));
    let payload = iterm.split_once(':').unwrap().1.strip_suffix('\x07');
    assert_eq!(STANDARD.decode(payload.unwrap()).unwrap(), data);

    let kitty = image_escape(Protocol::Kitty, &data, 100, 28);
    let (mut joined, mut flags) = (String::new(), Vec::new());
    for part in kitty.split_terminator("\x1b\\") {
        let (keys, chunk) = part
            .strip_prefix("\x1b_G")
            .unwrap()
            .split_once(';')
            .unwrap();
        assert!(chunk.len() <= 4096);
        flags.push(keys.ends_with("m=1"));
        joined.push_str(chunk);
    }
    assert!(kitty.contains("c=100,r=28,C=1,q=2"));
    assert_eq!(flags, [true, true, true, false]);
    assert_eq!(STANDARD.decode(joined).unwrap(), data);
}

#[test]
fn cell_size_comes_from_the_terminal_with_a_fallback() {
    assert_eq!(cell_pixels(100, 30, 1600, 960), (16, 32));
    // Reported in points on a high density screen: draw at twice the size.
    assert_eq!(cell_pixels(100, 30, 700, 450), (14, 30));
    assert_eq!(cell_pixels(100, 30, 0, 0), (16, 32));
}

#[test]
fn drawing_between_physics_steps_keeps_motion_even() {
    let mut world = World::new(LEVEL);
    let mut clock = FixedStep::default();
    world.fire(Shot::from_degrees(15.0, 45.0));
    // 33 ms frames hold 3.96 steps: some get 3 new steps and some get 4.
    let mut shown = Vec::new();
    for _ in 0..20 {
        for _ in 0..clock.steps_for(0.033) {
            world.step();
        }
        shown.push(world.ball_position_between(clock.fraction()).unwrap().0);
    }
    let moves: Vec<f64> = shown.windows(2).map(|w| w[1] - w[0]).collect();
    // Sideways speed is constant in free flight, so every move should match.
    for step in &moves {
        assert!((step - moves[0]).abs() < 0.002, "{moves:?}");
    }
    let now = world.ball_position().unwrap();
    assert_eq!(world.ball_position_between(1.0), Some(now));
}

#[test]
fn targets_are_drawn_solid() {
    let view = Viewport::new(render::areas(Rect::new(0, 0, 100, 30), false).play);
    let world = World::new(LEVEL);
    let image = Image::render(&view, &world, &Aim::default(), false, (16, 32), 1.0);
    for piece in &world.pieces {
        // Every pixel of a column through the middle half of the shape is opaque.
        let (x, y) = (piece.object.x, piece.object.y);
        let half = match piece.object.shape {
            Shape::Box { height, .. } => height / 4.0,
            Shape::Ball { radius } => radius / 2.0,
        };
        let (column, top) = view.world_to_cells((x - 0.2, y + half));
        let (_, bottom) = view.world_to_cells((x - 0.2, y - half));
        for py in (top * 32.0) as u32..(bottom * 32.0) as u32 {
            let at = ((py * image.width + (column * 16.0) as u32) * 4 + 3) as usize;
            assert_eq!(image.pixels[at], 255, "{} is hollow", piece.object.name);
        }
    }
}
