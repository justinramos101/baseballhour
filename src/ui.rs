use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    app::{App, DataState, Input, View},
    model::{Game, GameStatus},
};

mod atlas;
mod games;
mod overlay;

pub const BACKGROUND: Color = Color::Rgb(10, 19, 27);
pub const PANEL: Color = Color::Rgb(15, 27, 37);
pub const TEXT: Color = Color::Rgb(231, 234, 224);
pub const MUTED: Color = Color::Rgb(149, 170, 182);
pub const GREEN: Color = Color::Rgb(113, 221, 173);
pub const AMBER: Color = Color::Rgb(255, 197, 112);
const BLUE: Color = Color::Rgb(135, 187, 222);
const RED: Color = Color::Rgb(245, 144, 137);
const BORDER: Color = Color::Rgb(48, 73, 86);
const SELECTED: Color = Color::Rgb(36, 50, 55);

fn text(value: impl Into<String>, color: Color) -> Span<'static> {
    Span::styled(value.into(), Style::default().fg(color))
}

fn bold(value: impl Into<String>, color: Color) -> Span<'static> {
    Span::styled(
        value.into(),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
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
    result.push('…');
    result
}

fn block(title: impl Into<String>) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BACKGROUND))
        .title(Line::from(bold(format!(" {} ", title.into()), TEXT)))
}

fn state_color(game: &Game) -> Color {
    match game.status {
        GameStatus::Live => GREEN,
        GameStatus::Scheduled => BLUE,
        GameStatus::Final => MUTED,
        _ => AMBER,
    }
}

fn state_symbol(game: &Game, ascii: bool) -> &'static str {
    if ascii {
        return if game.status == GameStatus::Live {
            "*"
        } else {
            "o"
        };
    }
    match game.status {
        GameStatus::Live => "●",
        GameStatus::Scheduled => "○",
        GameStatus::Final => "·",
        _ => "!",
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
                Line::from(bold("BASEBALL HOUR", GREEN)),
                Line::from(text("Resize to at least 40 x 12.", TEXT)),
                Line::from(text("q Quit", MUTED)),
            ]),
            area,
        );
        return;
    }
    let outer = area.inner(Margin::new(1, 0));
    let compact = area.height < 30;
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(if compact { 2 } else { 3 }),
            Constraint::Min(3),
            Constraint::Length(if compact { 2 } else { 3 }),
        ])
        .split(outer);
    header(frame, sections[0], app);
    navigation(frame, sections[1], app);
    if area.width >= 110 && area.height >= 28 {
        let body = Layout::horizontal([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(sections[2]);
        let left = Layout::vertical([Constraint::Min(10), Constraint::Length(11)]).split(body[0]);
        atlas::draw(frame, left[0], app);
        games::details(frame, left[1], app);
        games::draw(frame, body[1], app, true);
    } else if area.width >= 72 && area.height >= 24 {
        let body = Layout::vertical([Constraint::Min(4), Constraint::Length(3)]).split(sections[2]);
        let split = Layout::horizontal([Constraint::Percentage(43), Constraint::Percentage(57)])
            .split(body[0]);
        atlas::draw(frame, split[0], app);
        games::draw(frame, split[1], app, false);
        games::compact_detail(frame, body[1], app);
    } else {
        games::draw(frame, sections[2], app, false);
    }
    footer(frame, sections[3], app);
    overlay::draw(frame, app);
}

fn header(frame: &mut Frame, area: Rect, app: &App) {
    if area.width < 70 {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    bold(" BASEBALL HOUR", GREEN),
                    text(if app.demo { "  DEMO" } else { "" }, AMBER),
                ]),
                Line::from(vec![
                    bold(format!(" {}", app.query.date), TEXT),
                    text(format!(" · {}", app.zone_label()), MUTED),
                ]),
            ]),
            area,
        );
        return;
    }
    let halves = Layout::horizontal([Constraint::Length(31), Constraint::Min(1)]).split(area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                bold(if app.ascii { " <> " } else { " ◇ " }, AMBER),
                bold("BASEBALL", TEXT),
                bold(" HOUR", GREEN),
                text(if app.demo { "  DEMO" } else { "" }, AMBER),
            ]),
            Line::from(text("    Every game has a place.", MUTED)),
        ]),
        halves[0],
    );
    let date = app.query.date.format("%a, %b %-d, %Y").to_string();
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![bold(date, TEXT), text("   [g] date", MUTED)]),
            Line::from(text(app.zone_label(), MUTED)),
        ])
        .alignment(ratatui::layout::Alignment::Right),
        halves[1],
    );
}

fn navigation(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(Block::default().style(Style::default().bg(PANEL)), area);
    let row = Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), 1);
    let mut spans = Vec::new();
    let labels = if row.width < 55 {
        [
            ("1", "All", View::All),
            ("2", "Following", View::Following),
            ("3", "Nearby", View::Nearby),
        ]
    } else {
        [
            ("1", "All games", View::All),
            ("2", "Following", View::Following),
            ("3", "Nearby", View::Nearby),
        ]
    };
    for (key, label, view) in labels {
        let selected = app.view == view;
        spans.push(Span::styled(
            format!(" {key} {label} "),
            Style::default()
                .fg(if selected { BACKGROUND } else { MUTED })
                .bg(if selected { GREEN } else { PANEL })
                .add_modifier(if selected {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ));
        spans.push(text(" ", MUTED));
    }
    let width = spans.iter().map(|s| s.width()).sum::<usize>();
    let league = format!("l {} ", app.query.league.label());
    if row.width as usize > width + league.len() {
        spans.push(text(
            " ".repeat(row.width as usize - width - league.len()),
            MUTED,
        ));
        spans.push(bold(league, AMBER));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), row);
}

fn footer(frame: &mut Frame, area: Rect, app: &App) {
    let status = if app.demo {
        "DEMO · Illustrative schedules and scores".into()
    } else {
        let age = app
            .snapshot
            .as_ref()
            .map(|s| (app.now - s.fetched_at).num_seconds().max(0));
        match &app.state {
            DataState::Loading if app.snapshot.is_some() => {
                format!("SAVED · {}s old · Updating schedule…", age.unwrap_or(0))
            }
            DataState::Loading => "Updating schedule…".into(),
            DataState::Fresh => format!(
                "MLB StatsAPI · updated {}s ago · auto-refresh 30s",
                age.unwrap_or(0)
            ),
            DataState::Cached(reason) => {
                format!("CACHED · {}m old · {reason}", age.unwrap_or(0) / 60)
            }
            DataState::Failed(reason) => format!("OFFLINE · {reason} · r Retry"),
        }
    };
    let state_color =
        if matches!(app.state, DataState::Cached(_) | DataState::Failed(_)) || app.demo {
            AMBER
        } else {
            MUTED
        };
    if matches!(app.input, Input::Search) || !app.search.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(text(ellipsize(&status, area.width), state_color)),
                Line::from(vec![
                    bold(" Search teams / ballparks > ", GREEN),
                    text(&app.search, TEXT),
                    text(
                        if matches!(app.input, Input::Search) {
                            "_   Enter apply · Esc clear"
                        } else {
                            "   / edit · Esc clear"
                        },
                        MUTED,
                    ),
                ]),
            ]),
            area,
        );
    } else {
        let message = app.message.clone().or_else(|| {
            app.snapshot
                .as_ref()
                .filter(|_| !app.demo)
                .and_then(|s| s.warnings.first().cloned())
        });
        let controls = if area.width >= 108 {
            " ↑↓ Game  ←→ Day  t Today  g Date  / Search  f Follow  p Place  l League  Enter Details  ? Help  q Quit"
        } else if area.width >= 72 {
            " ↑↓ Game  ←→ Day  / Search  f Follow  l League  ? Help  q Quit"
        } else {
            " ↑↓ Game  ←→ Day  ? Help  q Quit"
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(text(
                    ellipsize(
                        &message.map_or_else(
                            || status.clone(),
                            |message| format!("{status} · {message}"),
                        ),
                        area.width,
                    ),
                    state_color,
                )),
                Line::from(text(controls, TEXT)),
            ]),
            area,
        );
    }
}
