use std::io::{self, Write};

use clap::ValueEnum;
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    style::{Color, Modifier},
};
use unicode_width::UnicodeWidthStr;

use crate::{app::App, ui};

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum Format {
    Plain,
    #[default]
    Ansi,
    Svg,
}

/// Capture a terminal frame in an in-memory buffer.
///
/// # Panics
/// Panics if the in-memory terminal cannot be initialized or drawn.
#[must_use]
pub fn capture(app: &App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui::draw(frame, app)).unwrap();
    terminal.backend().buffer().clone()
}

/// Write a captured frame in the requested output format.
///
/// # Errors
/// Returns an error if the output writer fails.
pub fn write(buffer: &Buffer, format: Format, out: &mut impl Write) -> io::Result<()> {
    if matches!(format, Format::Svg) {
        return svg(buffer, out);
    }
    for y in buffer.area.y..buffer.area.bottom() {
        let mut x = buffer.area.x;
        while x < buffer.area.right() {
            let cell = &buffer[(x, y)];
            if matches!(format, Format::Ansi) {
                write!(
                    out,
                    "\x1b[0;{};{}m",
                    sgr(cell.fg, true),
                    sgr(cell.bg, false)
                )?;
                if cell.modifier.contains(Modifier::BOLD) {
                    write!(out, "\x1b[1m")?;
                }
            }
            write!(out, "{}", cell.symbol())?;
            x = x.saturating_add(u16::try_from(cell.symbol().width().max(1)).unwrap_or(u16::MAX));
        }
        if matches!(format, Format::Ansi) {
            write!(out, "\x1b[0m")?;
        }
        writeln!(out)?;
    }
    Ok(())
}

/// The SGR color parameters for a cell: palette indexes stay indexed, and
/// everything else is written as 24-bit color.
fn sgr(color: Color, foreground: bool) -> String {
    let layer = if foreground { 38 } else { 48 };
    if let Color::Indexed(index) = color {
        return format!("{layer};5;{index}");
    }
    let (r, g, b) = rgb(color, foreground);
    format!("{layer};2;{r};{g};{b}")
}

fn rgb(color: Color, foreground: bool) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black => (0, 0, 0),
        Color::White => (255, 255, 255),
        Color::Red | Color::LightRed => (245, 144, 137),
        Color::Green | Color::LightGreen => (113, 221, 173),
        Color::Yellow | Color::LightYellow => (255, 197, 112),
        Color::Blue | Color::LightBlue => (135, 187, 222),
        Color::Gray | Color::DarkGray => (149, 170, 182),
        _ if foreground => (231, 234, 224),
        _ => (10, 19, 27),
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn svg(buffer: &Buffer, out: &mut impl Write) -> io::Result<()> {
    let width = u32::from(buffer.area.width) * 9;
    let height = u32::from(buffer.area.height) * 19;
    writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\" role=\"img\" aria-label=\"Baseball Hour terminal capture\">"
    )?;
    writeln!(
        out,
        "<title>Baseball Hour. Actual terminal cell capture.</title><rect width=\"100%\" height=\"100%\" fill=\"#0a131b\"/>"
    )?;
    for y in buffer.area.y..buffer.area.bottom() {
        let mut x = buffer.area.x;
        while x < buffer.area.right() {
            let cell = &buffer[(x, y)];
            let start = x;
            let mut content = String::new();
            while x < buffer.area.right() {
                let current = &buffer[(x, y)];
                if current.fg != cell.fg
                    || current.bg != cell.bg
                    || current.modifier != cell.modifier
                {
                    break;
                }
                content.push_str(current.symbol());
                x = x.saturating_add(
                    u16::try_from(current.symbol().width().max(1)).unwrap_or(u16::MAX),
                );
            }
            let (r, g, b) = rgb(cell.fg, true);
            let (br, bg, bb) = rgb(cell.bg, false);
            let px = u32::from(start) * 9;
            let py = u32::from(y) * 19;
            let length = u32::from(x - start) * 9;
            if (br, bg, bb) != (10, 19, 27) {
                writeln!(
                    out,
                    "<rect x=\"{px}\" y=\"{py}\" width=\"{length}\" height=\"19\" fill=\"#{br:02x}{bg:02x}{bb:02x}\"/>"
                )?;
            }
            if !content.trim().is_empty() {
                let weight = if cell.modifier.contains(Modifier::BOLD) {
                    700
                } else {
                    400
                };
                writeln!(
                    out,
                    "<text x=\"{px}\" y=\"{}\" fill=\"#{r:02x}{g:02x}{b:02x}\" font-family=\"DejaVu Sans Mono,monospace\" font-size=\"15\" font-weight=\"{weight}\" xml:space=\"preserve\" textLength=\"{length}\" lengthAdjust=\"spacingAndGlyphs\">{}</text>",
                    py + 15,
                    escape(&content)
                )?;
            }
        }
    }
    writeln!(out, "</svg>")
}
