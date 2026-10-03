use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    app::{App, DataState, Input, View},
    model::{Game, GameStatus, shift_date},
};

mod atlas;
mod games;
mod overlay;
mod teams;

pub const BACKGROUND: Color = Color::Rgb(10, 19, 27);
pub const PANEL: Color = Color::Rgb(15, 27, 37);
pub const TEXT: Color = Color::Rgb(231, 234, 224);
pub const MUTED: Color = Color::Rgb(149, 170, 182);
/// Reserved for live play.
pub const GREEN: Color = Color::Rgb(113, 221, 173);
/// Reserved for the selection and for alerts.
pub const AMBER: Color = Color::Rgb(255, 197, 112);
const BLUE: Color = Color::Rgb(135, 187, 222);
const RED: Color = Color::Rgb(245, 144, 137);
const BORDER: Color = Color::Rgb(48, 73, 86);
/// The background tinted toward the amber selection hue.
const SELECTED: Color = Color::Rgb(37, 39, 37);
const STRIPE: Color = Color::Rgb(17, 30, 40);
const FAINT: Color = Color::Rgb(88, 110, 122);
const KEYCAP: Color = Color::Rgb(30, 47, 60);

fn text(value: impl Into<String>, color: Color) -> Span<'static> {
    Span::styled(value.into(), Style::default().fg(color))
}

fn bold(value: impl Into<String>, color: Color) -> Span<'static> {
    Span::styled(
        value.into(),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}

fn keycap(cap: &str) -> Span<'static> {
    Span::styled(
        cap.to_string(),
        Style::default()
            .fg(TEXT)
            .bg(KEYCAP)
            .add_modifier(Modifier::BOLD),
    )
}

/// A key rendered as a keycap, followed by its label.
fn key(cap: &str, label: &str) -> [Span<'static>; 2] {
    [keycap(cap), text(format!(" {label}  "), MUTED)]
}

fn width(spans: &[Span]) -> usize {
    spans.iter().map(Span::width).sum()
}

fn ellipsize(value: &str, width: u16) -> String {
    if value.width() <= usize::from(width) {
        return value.into();
    }
    if width == 0 {
        return String::new();
    }
    let mut result = String::new();
    let mut used = 0;
    for c in value.chars() {
        let next = used + c.width().unwrap_or(0);
        if next >= usize::from(width) {
            break;
        }
        result.push(c);
        used = next;
    }
    result.truncate(result.trim_end().len());
    result.push('…');
    result
}

fn block(title: impl Into<String>) -> Block<'static> {
    let title = title.into();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BACKGROUND));
    if title.is_empty() {
        return block;
    }
    block.title(Line::from(vec![
        text("─", BORDER),
        bold(format!(" {title} "), TEXT),
    ]))
}

fn rgb(color: Color) -> (f64, f64, f64) {
    match color {
        Color::Rgb(r, g, b) => (f64::from(r), f64::from(g), f64::from(b)),
        _ => (128., 128., 128.),
    }
}

/// Blend two RGB colors. `amount` 0 keeps `from`, 1 reaches `to`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Channels are rounded and clamped to the u8 range"
)]
fn mix(from: Color, to: Color, amount: f64) -> Color {
    let (a, b) = (rgb(from), rgb(to));
    let t = amount.clamp(0.0, 1.0);
    let channel = |x: f64, y: f64| (x + (y - x) * t).round().clamp(0.0, 255.0) as u8;
    Color::Rgb(channel(a.0, b.0), channel(a.1, b.1), channel(a.2, b.2))
}

/// A smooth 0..1 oscillation with a period measured in animation ticks.
#[expect(
    clippy::cast_precision_loss,
    reason = "Only the phase within one period matters"
)]
fn pulse(tick: u64, period: u64) -> f64 {
    let phase = (tick % period) as f64 / period as f64;
    0.5 - 0.5 * (phase * std::f64::consts::TAU).cos()
}

/// Whether the screen has motion that needs animation frames.
#[must_use]
pub fn animates(app: &App) -> bool {
    matches!(app.state, DataState::Loading)
        || matches!(
            app.input,
            Input::Search | Input::Date { .. } | Input::Place { .. }
        )
        || app.selected_game().is_some()
        || app.games().iter().any(|g| g.status == GameStatus::Live)
}

fn live_color(app: &App) -> Color {
    mix(GREEN, Color::Rgb(46, 104, 84), pulse(app.tick, 14) * 0.8)
}

fn spinner(app: &App) -> &'static str {
    const FRAMES: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
    const ASCII: [&str; 4] = ["|", "/", "-", "\\"];
    if app.ascii {
        ASCII[usize::try_from(app.tick % 4).unwrap_or(0)]
    } else {
        FRAMES[usize::try_from(app.tick % 8).unwrap_or(0)]
    }
}

fn state_color(game: &Game) -> Color {
    match game.status {
        GameStatus::Live => GREEN,
        GameStatus::Scheduled => BLUE,
        GameStatus::Final => MUTED,
        _ => AMBER,
    }
}

fn state_symbol(status: GameStatus, ascii: bool) -> &'static str {
    match (status, ascii) {
        (GameStatus::Live, false) => "●",
        (GameStatus::Live, true) => "*",
        (GameStatus::Scheduled, false) => "○",
        (GameStatus::Scheduled, true) => "o",
        (GameStatus::Final, _) => ".",
        _ => "!",
    }
}

/// Fade every cell in `area` toward the background, for modal scrims.
fn dim(buffer: &mut Buffer, area: Rect, amount: f64) {
    let shade = Color::Rgb(4, 9, 13);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buffer[(x, y)];
            let fg = if matches!(cell.fg, Color::Rgb(..)) {
                cell.fg
            } else {
                TEXT
            };
            let bg = if matches!(cell.bg, Color::Rgb(..)) {
                cell.bg
            } else {
                BACKGROUND
            };
            cell.set_fg(mix(fg, shade, amount));
            cell.set_bg(mix(bg, shade, amount));
        }
    }
}

/// Replace every non-ASCII glyph with an ASCII stand-in for `--ascii`.
fn asciify(buffer: &mut Buffer) {
    for cell in &mut buffer.content {
        let symbol = cell.symbol();
        if symbol.is_ascii() {
            continue;
        }
        let replacement = match symbol.chars().next().unwrap_or(' ') {
            '─' | '━' | '–' | '—' => "-",
            '│' => "|",
            '┃' => "#",
            '╭' | '╮' | '╰' | '╯' | '├' | '┤' | '┬' | '┴' | '┼' => "+",
            '·' | '…' | '◇' | '○' => ".",
            '●' | '★' => "*",
            '◆' => "@",
            '▲' | '↑' => "^",
            '▼' | '▽' | '↓' => "v",
            '‹' | '←' => "<",
            '›' | '→' | '▎' => ">",
            '⌂' => "H",
            '×' => "x",
            '▌' | '▏' | '°' => " ",
            c if ('\u{2800}'..='\u{28ff}').contains(&c) => ".",
            _ => "?",
        };
        cell.set_symbol(replacement);
    }
}

/// Map truecolor cells to the nearest xterm 256-color entry.
pub fn quantize(buffer: &mut Buffer) {
    fn index(color: Color) -> Color {
        const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
        let Color::Rgb(r, g, b) = color else {
            return color;
        };
        let nearest = |v: u8| {
            (0..6)
                .min_by_key(|&i| (i32::from(LEVELS[i]) - i32::from(v)).abs())
                .unwrap_or(0)
        };
        let (ri, gi, bi) = (nearest(r), nearest(g), nearest(b));
        let step = ((u32::from(r) + u32::from(g) + u32::from(b)) / 3)
            .saturating_sub(3)
            .min(238)
            / 10;
        let gray = 8 + step * 10;
        let distance = |x: u32, y: u32, z: u32| {
            [(x, r), (y, g), (z, b)]
                .iter()
                .map(|&(a, b)| (i64::from(a) - i64::from(b)).pow(2))
                .sum::<i64>()
        };
        let cube = distance(
            u32::from(LEVELS[ri]),
            u32::from(LEVELS[gi]),
            u32::from(LEVELS[bi]),
        );
        if distance(gray, gray, gray) < cube {
            Color::Indexed(232 + u8::try_from(step).unwrap_or(23))
        } else {
            Color::Indexed(u8::try_from(16 + 36 * ri + 6 * gi + bi).unwrap_or(16))
        }
    }
    for cell in &mut buffer.content {
        cell.fg = index(cell.fg);
        cell.bg = index(cell.bg);
    }
}

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(BACKGROUND).fg(TEXT)),
        area,
    );
    if area.width < 40 || area.height < 12 {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(bold("BASEBALL HOUR", TEXT)),
                Line::from(text("Resize to at least 40 x 12.", TEXT)),
                Line::from(text("q Quit", MUTED)),
            ]),
            area,
        );
    } else {
        layout(frame, app, area);
        overlay::draw(frame, app);
    }
    if app.ascii {
        asciify(frame.buffer_mut());
    }
}

fn layout(frame: &mut Frame, app: &App, area: Rect) {
    let outer = area.inner(Margin::new(1, 0));
    let footer_rows = footer_layout(app, outer.width).1;
    let sections = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(footer_rows),
    ])
    .split(outer);
    header(frame, sections[0], app);
    navigation(frame, sections[1], app);
    let body = sections[2];
    if area.width >= 110 && area.height >= 28 {
        let side = (area.width * 36 / 100).clamp(44, 72);
        let columns =
            Layout::horizontal([Constraint::Min(40), Constraint::Length(side)]).split(body);
        let left = columns[0];
        // The US is wider than tall; rows the map cannot use go to the scorebug.
        let ideal_map = left.width.saturating_sub(2) * 10 / 33 + 2;
        let detail = left
            .height
            .saturating_sub(ideal_map)
            .clamp(if left.height >= 32 { 11 } else { 10 }, 14);
        let rows = Layout::vertical([Constraint::Min(8), Constraint::Length(detail)]).split(left);
        atlas::draw(frame, rows[0], app);
        games::details(frame, rows[1], app);
        games::draw(frame, columns[1], app);
    } else if area.width >= 72 && area.height >= 22 {
        let full_detail = body.height >= 26;
        let rows = Layout::vertical([
            Constraint::Min(6),
            Constraint::Length(if full_detail { 10 } else { 3 }),
        ])
        .split(body);
        let split = Layout::horizontal([Constraint::Percentage(43), Constraint::Percentage(57)])
            .split(rows[0]);
        atlas::draw(frame, split[0], app);
        games::draw(frame, split[1], app);
        if full_detail {
            games::details(frame, rows[1], app);
        } else {
            games::compact_detail(frame, rows[1], app);
        }
    } else {
        games::draw(frame, body, app);
    }
    footer(frame, sections[3], app);
}

/// `America/New_York` reads as "New York"; the system zone reads as "Local".
fn zone_city(app: &App) -> String {
    app.timezone.map_or_else(
        || "Local".into(),
        |tz| {
            tz.name()
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .replace('_', " ")
        },
    )
}

fn neighbor(app: &App, days: i64) -> Vec<Span<'static>> {
    shift_date(app.query.date, days).map_or_else(Vec::new, |d| {
        let label = d.format("%a %-d").to_string();
        if days < 0 {
            vec![text(format!("‹ {label}"), FAINT), Span::raw("    ")]
        } else {
            vec![Span::raw("    "), text(format!("{label} ›"), FAINT)]
        }
    })
}

fn header(frame: &mut Frame, area: Rect, app: &App) {
    let date = app.query.date;
    let today = app.local_time(app.now).date_naive();
    let mut brand = vec![bold(" ◆ ", AMBER), bold("BASEBALL HOUR", TEXT)];
    if app.demo {
        brand.push(Span::raw(" "));
        brand.push(Span::styled(
            " DEMO ",
            Style::default()
                .fg(BACKGROUND)
                .bg(AMBER)
                .add_modifier(Modifier::BOLD),
        ));
    }
    let day = bold(date.format("%a, %b %-d").to_string().to_uppercase(), TEXT);
    let relative = match (date - today).num_days() {
        0 => vec![Span::raw("  "), keycap(" TODAY ")],
        1 => vec![Span::raw("  "), keycap(" TOMORROW ")],
        -1 => vec![Span::raw("  "), keycap(" YESTERDAY ")],
        _ => vec![],
    };
    let live = app
        .games()
        .iter()
        .filter(|g| g.status == GameStatus::Live)
        .count();
    let badge = if matches!(app.state, DataState::Loading) {
        vec![text(format!("{} Updating", spinner(app)), MUTED)]
    } else if live > 0 {
        vec![
            bold(state_symbol(GameStatus::Live, app.ascii), live_color(app)),
            bold(format!(" {live} LIVE"), GREEN),
        ]
    } else {
        vec![]
    };
    let clock = vec![
        text("   ", FAINT),
        bold(
            app.local_time(app.now).format("%-I:%M %p").to_string(),
            TEXT,
        ),
        text(format!(" {}", zone_city(app)), FAINT),
    ];

    // Drop the least useful pieces, in order, until the row fits.
    let available = usize::from(area.width);
    let mut chosen = None;
    for level in 0..5 {
        let mut left = brand.clone();
        if level == 0 {
            left.push(text("   Every game has a place.", FAINT));
        }
        let mut center = vec![];
        if level <= 1 {
            center.extend(neighbor(app, -1));
        }
        center.push(day.clone());
        if level <= 3 {
            center.push(text(date.format(" %Y").to_string(), MUTED));
            center.extend(relative.clone());
        }
        if level <= 1 {
            center.extend(neighbor(app, 1));
        }
        let mut right = badge.clone();
        if level <= 2 {
            right.extend(clock.clone());
        }
        let side = width(&left).max(width(&right));
        if side * 2 + width(&center) + 2 <= available {
            chosen = Some((left, center, right, side));
            break;
        }
    }
    let Some((left, center, right, side)) = chosen else {
        // Narrow: brand and date only.
        let mut narrow = brand;
        narrow.push(Span::raw("  "));
        narrow.push(day);
        frame.render_widget(Paragraph::new(Line::from(narrow)), area);
        return;
    };
    let side = u16::try_from(side).unwrap_or(0);
    let columns = Layout::horizontal([
        Constraint::Length(side),
        Constraint::Min(1),
        Constraint::Length(side),
    ])
    .split(area);
    frame.render_widget(Paragraph::new(Line::from(left)), columns[0]);
    frame.render_widget(
        Paragraph::new(Line::from(center)).alignment(Alignment::Center),
        columns[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(right)).alignment(Alignment::Right),
        columns[2],
    );
}

fn view_counts(app: &App) -> [usize; 3] {
    let games = app.games().iter().filter(|g| g.matches(&app.search));
    let mut counts = [0; 3];
    for game in games {
        counts[0] += 1;
        if app.is_favorite(game) {
            counts[1] += 1;
        }
        if app.preferences.place.is_some() && game.venue.coordinates.is_some() {
            counts[2] += 1;
        }
    }
    counts
}

fn navigation(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(Block::default().style(Style::default().bg(PANEL)), area);
    let row = area.inner(Margin::new(1, 0));
    let mut spans = Vec::new();
    let short = row.width < 64;
    let counts = view_counts(app);
    let views = [
        ("1", if short { "All" } else { "All games" }, View::All),
        ("2", "Following", View::Following),
        ("3", "Nearby", View::Nearby),
    ];
    for (i, (key, label, view)) in views.into_iter().enumerate() {
        let selected = app.view == view;
        spans.push(Span::styled(
            format!(" {key} "),
            if selected {
                Style::default()
                    .fg(BACKGROUND)
                    .bg(AMBER)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(MUTED).bg(KEYCAP)
            },
        ));
        let style = if selected {
            Style::default()
                .fg(TEXT)
                .bg(SELECTED)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(MUTED).bg(PANEL)
        };
        spans.push(Span::styled(format!("{label} "), style));
        if !short && (view != View::Nearby || app.preferences.place.is_some()) {
            spans.push(Span::styled(
                format!("{} ", counts[i]),
                Style::default()
                    .fg(if selected { MUTED } else { FAINT })
                    .bg(if selected { SELECTED } else { PANEL }),
            ));
        }
        spans.push(Span::raw("  "));
    }
    let mut right = vec![];
    if !app.search.is_empty() && matches!(app.input, Input::Normal) {
        right.push(keycap(" / "));
        right.push(Span::styled(
            format!(" {} ", app.search),
            Style::default().fg(TEXT).bg(SELECTED),
        ));
        right.push(Span::raw("  "));
    }
    right.push(keycap(" l "));
    right.push(bold(format!(" {} ", app.query.league.label()), TEXT));
    let used = width(&spans);
    let right_width = width(&right);
    if usize::from(row.width) > used + right_width {
        spans.push(Span::raw(
            " ".repeat(usize::from(row.width) - used - right_width),
        ));
        spans.extend(right);
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), row);
}

fn status(app: &App) -> (String, Color) {
    if app.demo {
        return ("DEMO · Illustrative schedules and scores".into(), AMBER);
    }
    let age = app
        .snapshot
        .as_ref()
        .map_or(0, |s| (app.now - s.fetched_at).num_seconds().max(0));
    let (status, color) = match &app.state {
        DataState::Loading if app.snapshot.is_some() => {
            (format!("SAVED · {age}s old · Updating schedule…"), MUTED)
        }
        DataState::Loading => ("Updating schedule…".into(), MUTED),
        DataState::Fresh => (
            format!("MLB StatsAPI · updated {age}s ago · auto-refresh 30s"),
            MUTED,
        ),
        DataState::Cached(reason) => (format!("CACHED · {}m old · {reason}", age / 60), AMBER),
        DataState::Failed(reason) => (format!("OFFLINE · {reason} · r Retry"), RED),
    };
    let message = app.message.clone().or_else(|| {
        app.snapshot
            .as_ref()
            .and_then(|s| s.warnings.first().cloned())
    });
    (
        message.map_or(status.clone(), |m| format!("{status} · {m}")),
        color,
    )
}

type Keys = &'static [(&'static str, &'static str)];

const FULL_KEYS: Keys = &[
    ("↑↓", "Game"),
    ("←→", "Day"),
    ("t", "Today"),
    ("g", "Date"),
    ("/", "Search"),
    ("f", "Follow"),
    ("p", "Place"),
    ("l", "League"),
    ("Enter", "Details"),
    ("?", "Help"),
    ("q", "Quit"),
];
const MEDIUM_KEYS: Keys = &[
    ("↑↓", "Game"),
    ("←→", "Day"),
    ("/", "Search"),
    ("f", "Follow"),
    ("l", "League"),
    ("Enter", "Details"),
    ("?", "Help"),
    ("q", "Quit"),
];
const SMALL_KEYS: Keys = &[("↑↓", "Game"), ("←→", "Day"), ("?", "Help"), ("q", "Quit")];

/// The footer's left side: key hints, or the search field while searching.
fn footer_controls(app: &App, keys: Keys) -> Vec<Span<'static>> {
    if matches!(app.input, Input::Search) || !app.search.is_empty() {
        let mut line = vec![
            keycap(" / "),
            text(" Search teams / ballparks › ", MUTED),
            bold(app.search.clone(), TEXT),
        ];
        if matches!(app.input, Input::Search) {
            line.push(text(if app.tick % 10 < 6 { "▏" } else { " " }, AMBER));
            line.push(Span::raw("   "));
            line.extend(key("Enter", "apply"));
        } else {
            line.push(Span::raw("   "));
            line.extend(key("/", "edit"));
        }
        line.extend(key("Esc", "clear"));
        return line;
    }
    let mut spans = vec![Span::raw(" ")];
    for (cap, label) in keys {
        spans.extend(key(cap, label));
    }
    spans
}

/// Choose the key set, and whether status and keys share one row.
fn footer_layout(app: &App, available: u16) -> (Keys, u16) {
    let status = status(app).0.width() + 3;
    let available = usize::from(available);
    for keys in [FULL_KEYS, MEDIUM_KEYS, SMALL_KEYS] {
        if width(&footer_controls(app, keys)) + status + 2 <= available {
            return (keys, 1);
        }
    }
    for keys in [FULL_KEYS, MEDIUM_KEYS] {
        if width(&footer_controls(app, keys)) <= available {
            return (keys, 2);
        }
    }
    (SMALL_KEYS, 2)
}

fn footer(frame: &mut Frame, area: Rect, app: &App) {
    let (status, color) = status(app);
    let (keys, rows) = footer_layout(app, area.width);
    let controls = Line::from(footer_controls(app, keys));
    let dot = text(" ● ", color);
    if rows == 1 {
        frame.render_widget(Paragraph::new(controls), area);
        frame.render_widget(
            Paragraph::new(Line::from(vec![dot, text(status, color)])).alignment(Alignment::Right),
            area,
        );
        return;
    }
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                dot,
                text(ellipsize(&status, area.width.saturating_sub(3)), color),
            ]),
            controls,
        ]),
        area,
    );
}
