//! Higher resolution drawing of the play area, for terminals that can show
//! images. The scene from `render` is drawn into pixels, then sent with the
//! iTerm2 or kitty image protocol. Other terminals keep the braille drawing.
//! This changes only what you see: sizes, positions and the mouse are the same.

use std::io::{self, Write, stdout};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use crossterm::cursor::{MoveTo, RestorePosition, SavePosition};
use crossterm::style::{Attribute, Color as TermColor, Print, ResetColor};
use crossterm::style::{SetAttribute, SetBackgroundColor, SetForegroundColor};
use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use crossterm::{execute, queue};
use ratatui::DefaultTerminal;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::input::Aim;
use crate::render::{self, Banner, Item, PlayStyle, Viewport};
use crate::world::World;

/// Set to `iterm`, `kitty` or `braille` to override the detection.
pub const OVERRIDE_VARIABLE: &str = "SLINGSHOT_GRAPHICS";

/// Set to a number from 10 to 60 to change how many frames a second are shown.
pub const FPS_VARIABLE: &str = "SLINGSHOT_FPS";

/// Used when the terminal does not say how many pixels a cell has.
const DEFAULT_CELL: (u32, u32) = (16, 32);
const MAX_IMAGE_WIDTH: u32 = 2400;
const KITTY_IMAGE_ID: u32 = 7391;
/// The most base64 text kitty accepts in one escape sequence.
const KITTY_CHUNK: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Iterm,
    Kitty,
}

impl Protocol {
    /// `None` means: draw with braille characters.
    pub fn detect() -> Option<Self> {
        Self::from_env(|name| std::env::var(name).ok())
    }

    /// Only terminals known to support a protocol get images, because others
    /// would show nothing or garbage.
    pub fn from_env(var: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let value = |name| var(name).unwrap_or_default();
        match value(OVERRIDE_VARIABLE).to_ascii_lowercase().as_str() {
            "iterm" => return Some(Self::Iterm),
            "kitty" => return Some(Self::Kitty),
            "braille" => return None,
            _ => {}
        }
        // tmux and screen do not pass images through by default.
        let term = value("TERM");
        if var("TMUX").is_some() || term.starts_with("screen") || term.starts_with("tmux") {
            return None;
        }
        let program = value("TERM_PROGRAM");
        // `LC_TERMINAL` is still set when iTerm2 is used over ssh.
        if matches!(program.as_str(), "iTerm.app" | "WezTerm") || value("LC_TERMINAL") == "iTerm2" {
            Some(Self::Iterm)
        } else if program == "ghostty" || term == "xterm-kitty" || term == "xterm-ghostty" {
            Some(Self::Kitty)
        } else {
            None
        }
    }
}

/// The time between frames. Images are large, and a steady 30 a second looks
/// smoother than a faster rate the terminal cannot keep up with. The physics does
/// not depend on this.
pub fn frame_time(protocol: Option<Protocol>, requested: Option<&str>) -> Duration {
    let fps = requested
        .and_then(|text| text.parse::<u32>().ok())
        .map(|fps| fps.clamp(10, 60))
        .unwrap_or(if protocol.is_some() { 30 } else { 60 });
    Duration::from_secs(1) / fps
}

/// The pixel size to draw each cell at, from what the terminal reports about its
/// whole window. Some report points rather than pixels on high density screens,
/// so small cells are drawn at twice the size.
pub fn cell_pixels(columns: u16, rows: u16, width: u16, height: u16) -> (u32, u32) {
    if columns == 0 || rows == 0 || width < columns || height < rows {
        return DEFAULT_CELL;
    }
    let (w, h) = (u32::from(width / columns), u32::from(height / rows));
    let scale = if w < 12 { 2 } else { 1 };
    (w * scale, h * scale)
}

/// An RGBA picture. Pixels nothing is drawn on stay transparent, so the
/// terminal's own background shows.
#[derive(PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Image {
    /// Draws the play area at `cell` pixels per terminal cell.
    pub fn render(
        view: &Viewport,
        world: &World,
        aim: &Aim,
        show_panel: bool,
        cell: (u32, u32),
        blend: f64,
    ) -> Self {
        let columns = u32::from(view.area.width).max(1);
        let rows = u32::from(view.area.height).max(1);
        let shrink = (f64::from(MAX_IMAGE_WIDTH) / f64::from(columns * cell.0)).min(1.0);
        let cell_width = f64::from(cell.0) * shrink;
        let cell_height = f64::from(cell.1) * shrink;
        let width = (f64::from(columns) * cell_width).round().max(1.0) as u32;
        let height = (f64::from(rows) * cell_height).round().max(1.0) as u32;
        let mut image = Self {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
        };

        let pixel = |at| {
            let (column, row) = view.world_to_cells(at);
            (column * cell_width, row * cell_height)
        };
        // Line thickness follows the cell size, so it looks alike on any screen.
        let thickness = (cell_width / 6.0).max(1.5);
        let rim = thickness * 0.9;
        // Solid shapes need no gaps: their darker rims keep them apart.
        for item in render::scene(view, world, aim, show_panel, 0.0, blend) {
            match item {
                Item::Line { from, to, color } => {
                    let (a, b) = (pixel(from), pixel(to));
                    match color {
                        // The ground: solid earth with grass on top.
                        Color::Green => {
                            image.fill_below(a.1, [95, 70, 48]);
                            let grass = thickness * 1.5;
                            image.segment(
                                (a.0, a.1 + grass),
                                (b.0, b.1 + grass),
                                grass,
                                rgb(color),
                            );
                        }
                        // The wooden slingshot is thicker than its bands.
                        Color::Yellow => image.segment(a, b, thickness * 1.6, [176, 122, 62]),
                        _ => image.segment(a, b, thickness * 0.8, rgb(color)),
                    }
                }
                Item::Polygon { corners, color } => {
                    image.polygon(corners.map(pixel), rim, rgb(color));
                }
                Item::Circle {
                    centre,
                    radius,
                    color,
                } => {
                    let (x, y) = pixel(centre);
                    let edge = pixel((centre.0 + radius, centre.1)).0;
                    image.disc((x, y), edge - x, rim, rgb(color));
                }
                Item::Spoke { from, to, color } => {
                    // Stops inside the rim.
                    let (a, b) = (pixel(from), pixel(to));
                    let end = (a.0 + (b.0 - a.0) * 0.7, a.1 + (b.1 - a.1) * 0.7);
                    image.segment(a, end, thickness * 0.7, shade(rgb(color)));
                }
                Item::Dot { at, color } => {
                    image.segment(pixel(at), pixel(at), thickness, rgb(color));
                }
                Item::Cross { at } => {
                    let (x, y) = pixel(at);
                    let arm = cell_width * 0.45;
                    for (dx, dy) in [(arm, 0.0), (0.0, arm)] {
                        image.segment(
                            (x - dx, y - dy),
                            (x + dx, y + dy),
                            thickness / 2.0,
                            rgb(Color::White),
                        );
                    }
                }
            }
        }
        image
    }

    /// Calls `coverage` for each pixel in a rectangle, and paints the pixels it
    /// returns a colour and amount for.
    fn shade_pixels(
        &mut self,
        from: (f64, f64),
        to: (f64, f64),
        coverage: impl Fn(f64, f64) -> ([u8; 3], f64),
    ) {
        let clip = |value: f64, size: u32| value.clamp(0.0, f64::from(size)) as u32;
        let (left, right) = (
            clip(from.0.floor(), self.width),
            clip(to.0.ceil(), self.width),
        );
        let (top, bottom) = (
            clip(from.1.floor(), self.height),
            clip(to.1.ceil(), self.height),
        );
        for y in top..bottom {
            for x in left..right {
                let (color, amount) = coverage(f64::from(x) + 0.5, f64::from(y) + 0.5);
                if amount > 0.0 {
                    self.blend(x, y, color, amount.min(1.0));
                }
            }
        }
    }

    /// Fills everything under the height `top`.
    fn fill_below(&mut self, top: f64, color: [u8; 3]) {
        let size = (f64::from(self.width), f64::from(self.height));
        self.shade_pixels((0.0, top - 1.0), size, |_, y| (color, y - top + 0.5));
    }

    /// A solid circle with a darker rim and smooth edges.
    fn disc(&mut self, centre: (f64, f64), radius: f64, rim: f64, color: [u8; 3]) {
        let reach = radius + 1.0;
        self.shade_pixels(
            (centre.0 - reach, centre.1 - reach),
            (centre.0 + reach, centre.1 + reach),
            |x, y| solid((x - centre.0).hypot(y - centre.1) - radius, rim, color),
        );
    }

    /// A solid convex four-sided shape with a darker rim and smooth edges.
    fn polygon(&mut self, corners: [(f64, f64); 4], rim: f64, color: [u8; 3]) {
        let middle = (
            corners.iter().map(|c| c.0).sum::<f64>() / 4.0,
            corners.iter().map(|c| c.1).sum::<f64>() / 4.0,
        );
        // Each side as a point on it and the direction pointing out of the shape.
        let sides: [_; 4] = std::array::from_fn(|i| {
            let (a, b) = (corners[i], corners[(i + 1) % 4]);
            let length = (b.0 - a.0).hypot(b.1 - a.1).max(f64::EPSILON);
            let mut out = ((b.1 - a.1) / length, (a.0 - b.0) / length);
            if (a.0 - middle.0) * out.0 + (a.1 - middle.1) * out.1 < 0.0 {
                out = (-out.0, -out.1);
            }
            (a, out)
        });
        let low = corners
            .iter()
            .fold((f64::MAX, f64::MAX), |m, c| (m.0.min(c.0), m.1.min(c.1)));
        let high = corners
            .iter()
            .fold((f64::MIN, f64::MIN), |m, c| (m.0.max(c.0), m.1.max(c.1)));
        self.shade_pixels(
            (low.0 - 1.0, low.1 - 1.0),
            (high.0 + 1.0, high.1 + 1.0),
            |x, y| {
                // How far outside the nearest side; negative inside.
                let distance = sides
                    .iter()
                    .map(|&(a, out)| (x - a.0) * out.0 + (y - a.1) * out.1)
                    .fold(f64::MIN, f64::max);
                solid(distance, rim, color)
            },
        );
    }

    /// A smooth-edged line with round ends. Equal ends make a round dot.
    fn segment(&mut self, from: (f64, f64), to: (f64, f64), radius: f64, color: [u8; 3]) {
        let reach = radius + 1.0;
        let clip = |value: f64, size: u32| value.clamp(0.0, f64::from(size)) as u32;
        let left = clip((from.0.min(to.0) - reach).floor(), self.width);
        let right = clip((from.0.max(to.0) + reach).ceil(), self.width);
        let top = clip((from.1.min(to.1) - reach).floor(), self.height);
        let bottom = clip((from.1.max(to.1) + reach).ceil(), self.height);
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length_squared = dx * dx + dy * dy;
        for y in top..bottom {
            for x in left..right {
                let (px, py) = (f64::from(x) + 0.5 - from.0, f64::from(y) + 0.5 - from.1);
                let along = if length_squared > 0.0 {
                    ((px * dx + py * dy) / length_squared).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let distance = (px - along * dx).hypot(py - along * dy);
                let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    self.blend(x, y, color, coverage);
                }
            }
        }
    }

    /// Paints `color` over a pixel, covering the given part of it.
    fn blend(&mut self, x: u32, y: u32, color: [u8; 3], coverage: f64) {
        let start = (y as usize * self.width as usize + x as usize) * 4;
        let pixel = &mut self.pixels[start..start + 4];
        let below = f64::from(pixel[3]) / 255.0 * (1.0 - coverage);
        let alpha = coverage + below;
        for (channel, new) in pixel.iter_mut().zip(color) {
            let mixed = (f64::from(new) * coverage + f64::from(*channel) * below) / alpha;
            *channel = mixed.round() as u8;
        }
        pixel[3] = (alpha * 255.0).round() as u8;
    }

    pub fn to_png(&self) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header().map_err(io::Error::other)?;
        writer
            .write_image_data(&self.pixels)
            .map_err(io::Error::other)?;
        writer.finish().map_err(io::Error::other)?;
        Ok(bytes)
    }
}

/// The colour and coverage of a pixel `distance` outside a solid shape's edge.
fn solid(distance: f64, rim: f64, color: [u8; 3]) -> ([u8; 3], f64) {
    let in_rim = (distance + rim + 0.5).clamp(0.0, 1.0);
    let dark = shade(color);
    let mixed = std::array::from_fn(|i| {
        (f64::from(color[i]) * (1.0 - in_rim) + f64::from(dark[i]) * in_rim).round() as u8
    });
    (mixed, (0.5 - distance).clamp(0.0, 1.0))
}

fn shade(color: [u8; 3]) -> [u8; 3] {
    color.map(|channel| (u16::from(channel) * 55 / 100) as u8)
}

/// Fixed colours close to the usual terminal ones.
fn rgb(color: Color) -> [u8; 3] {
    match color {
        Color::Green => [80, 200, 90],
        Color::Yellow => [215, 175, 50],
        Color::LightRed => [255, 105, 97],
        Color::LightYellow => [255, 240, 130],
        Color::Cyan => [70, 200, 215],
        Color::Magenta => [210, 100, 210],
        Color::Gray => [180, 180, 180],
        Color::DarkGray => [105, 105, 105],
        _ => [240, 240, 240],
    }
}

/// The escape sequence that shows a PNG at the cursor, stretched over exactly
/// `columns` × `rows` cells.
pub fn image_escape(protocol: Protocol, png: &[u8], columns: u16, rows: u16) -> String {
    let data = STANDARD.encode(png);
    match protocol {
        Protocol::Iterm => format!(
            "\x1b]1337;File=inline=1;size={};width={columns};height={rows};\
             preserveAspectRatio=0:{data}\x07",
            png.len()
        ),
        Protocol::Kitty => {
            let chunks: Vec<&[u8]> = data.as_bytes().chunks(KITTY_CHUNK).collect();
            let mut out = String::new();
            for (i, chunk) in chunks.iter().enumerate() {
                let more = u8::from(i + 1 < chunks.len());
                out.push_str("\x1b_G");
                if i == 0 {
                    // Reusing the ids replaces the last image. `q=2` stops
                    // replies, and `C=1` leaves the cursor where it is.
                    out.push_str(&format!(
                        "a=T,f=100,i={KITTY_IMAGE_ID},p=1,c={columns},r={rows},C=1,q=2,"
                    ));
                }
                out.push_str(&format!("m={more};"));
                // Base64 is plain ASCII.
                out.extend(chunk.iter().map(|&byte| char::from(byte)));
                out.push_str("\x1b\\");
            }
            out
        }
    }
}

fn delete_kitty_image(out: &mut impl Write) -> io::Result<()> {
    write!(out, "\x1b_Ga=d,d=I,i={KITTY_IMAGE_ID},q=2\x1b\\")
}

/// Leaves the terminal tidy when the game ends.
pub fn cleanup(out: &mut impl Write) -> io::Result<()> {
    if Protocol::detect() == Some(Protocol::Kitty) {
        delete_kitty_image(out)?;
    }
    execute!(out, EndSynchronizedUpdate)
}

/// Draws whole frames and remembers what the terminal is showing.
pub struct Screen {
    protocol: Option<Protocol>,
    /// Where the image goes and what covers it. A change needs a full redraw.
    layout: Option<(Rect, Option<Rect>)>,
    shown: Option<Image>,
}

impl Screen {
    pub fn new(protocol: Option<Protocol>) -> Self {
        Self {
            protocol,
            layout: None,
            shown: None,
        }
    }

    pub fn draw(
        &mut self,
        terminal: &mut DefaultTerminal,
        world: &World,
        aim: &Aim,
        show_panel: bool,
        blend: f64,
    ) -> io::Result<()> {
        let Some(protocol) = self.protocol else {
            terminal.draw(|frame| {
                render::draw_styled(frame, world, aim, show_panel, PlayStyle::Braille, blend);
            })?;
            return Ok(());
        };
        let size = terminal.size()?;
        let whole = Rect::new(0, 0, size.width, size.height);
        let play = render::fits(whole).then(|| render::areas(whole, show_panel).play);
        let banner = play.and_then(|play| render::banner(world, play));
        let layout = play.map(|play| (play, banner.as_ref().map(|b| b.area)));

        execute!(stdout(), BeginSynchronizedUpdate)?;
        if layout != self.layout {
            // ratatui only rewrites cells that changed, so it would leave parts
            // of an old image behind. Start from an empty screen instead.
            if protocol == Protocol::Kitty {
                delete_kitty_image(&mut stdout())?;
            }
            terminal.clear()?;
            self.layout = layout;
            self.shown = None;
        }
        terminal.draw(|frame| {
            render::draw_styled(frame, world, aim, show_panel, PlayStyle::Pixels, blend);
        })?;

        if let Some(play) = play {
            let cell = crossterm::terminal::window_size()
                .map(|w| cell_pixels(w.columns, w.rows, w.width, w.height))
                .unwrap_or(DEFAULT_CELL);
            let view = Viewport::new(play);
            let image = Image::render(&view, world, aim, show_panel, cell, blend);
            // A still picture is not sent again.
            if self.shown.as_ref() != Some(&image) {
                send(&mut stdout(), protocol, &image, play, banner.as_ref())?;
                self.shown = Some(image);
            }
        }
        execute!(stdout(), EndSynchronizedUpdate)
    }
}

/// Writes the image over the play area, then the banner over the image.
fn send(
    out: &mut impl Write,
    protocol: Protocol,
    image: &Image,
    play: Rect,
    banner: Option<&Banner>,
) -> io::Result<()> {
    let escape = image_escape(protocol, &image.to_png()?, play.width, play.height);
    queue!(out, SavePosition, MoveTo(play.x, play.y), Print(escape))?;
    if let Some(banner) = banner {
        let background = if banner.color == Color::Green {
            TermColor::DarkGreen
        } else {
            TermColor::DarkRed
        };
        let text: String = banner
            .text
            .chars()
            .take(usize::from(banner.area.width))
            .collect();
        queue!(
            out,
            MoveTo(banner.area.x, banner.area.y),
            SetForegroundColor(TermColor::Black),
            SetBackgroundColor(background),
            SetAttribute(Attribute::Bold),
            Print(text),
            SetAttribute(Attribute::Reset),
            ResetColor,
        )?;
    }
    queue!(out, RestorePosition)?;
    out.flush()
}
