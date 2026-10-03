use ratatui::{
    style::Modifier,
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
};

use super::{
    AMBER, App, BACKGROUND, BLUE, BORDER, Block, Borders, Color, DataState, FAINT, Frame, GREEN,
    GameStatus, Line, MUTED, Margin, Paragraph, RED, Rect, SELECTED, STRIPE, Span, Style, TEXT,
    UnicodeWidthStr, View, atlas, block, bold, ellipsize, live_color, mix, pulse, teams, text,
    width,
};
use crate::model::{Coordinates, Game, Team};

fn score(value: Option<u16>) -> String {
    value.map_or("-".into(), |n| n.to_string())
}

fn ordinal(n: u16) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// Inning state such as `▼ 3rd`.
fn inning(game: &Game) -> String {
    let Some(line) = &game.linescore else {
        return game.state_label();
    };
    let Some(number) = line.inning else {
        return game.state_label();
    };
    let half = match line.half.as_str() {
        "Top" => "▲",
        "Bottom" => "▼",
        "Middle" => "Mid",
        "End" => "End",
        other => other,
    };
    format!("{half} {}", ordinal(number))
}

/// Short state for a game row.
fn status(app: &App, game: &Game) -> String {
    match game.status {
        GameStatus::Live => inning(game),
        GameStatus::Scheduled => app.start_label(game),
        GameStatus::Final => match game.linescore.as_ref().map(|l| l.innings.len()) {
            Some(n) if n > 9 => format!("F/{n}"),
            _ => "Final".into(),
        },
        _ => game.status_text.clone(),
    }
}

fn status_color(app: &App, game: &Game) -> Color {
    match game.status {
        GameStatus::Live => live_color(app),
        GameStatus::Scheduled => BLUE,
        GameStatus::Final => MUTED,
        _ => AMBER,
    }
}

fn started(game: &Game) -> bool {
    !matches!(
        game.status,
        GameStatus::Scheduled | GameStatus::Postponed | GameStatus::Cancelled
    )
}

/// Which side leads, for emphasis. None when tied or not started.
fn leader(game: &Game) -> Option<bool> {
    let (away, home) = (game.away.score?, game.home.score?);
    (away != home).then_some(home > away)
}

/// "Braxton Ashcraft" becomes "Ashcraft".
fn surname(name: &str) -> &str {
    name.rsplit(' ').next().unwrap_or(name)
}

/// The club nickname: "Pittsburgh Pirates" becomes "Pirates".
fn nickname(name: &str) -> &str {
    let words: Vec<&str> = name.split_whitespace().collect();
    let keep = match words.as_slice() {
        [.., "Red" | "White" | "Blue", "Sox" | "Jays"] => 2,
        [] => return name,
        _ => 1,
    };
    let start = words.len().saturating_sub(keep);
    name.find(words[start]).map_or(name, |i| &name[i..])
}

fn follow_mark(app: &App, team: &Team) -> &'static str {
    if app.preferences.favorites.contains(&team.id) {
        "★"
    } else {
        " "
    }
}

/// Bases and outs, compact. Order follows the field: third, second, first.
fn base(occupied: bool) -> Span<'static> {
    if occupied {
        bold("◆", AMBER)
    } else {
        text("◇", FAINT)
    }
}

fn dots(filled: Option<u8>, total: u8, color: Color) -> Vec<Span<'static>> {
    let filled = filled.unwrap_or(0);
    (0..total)
        .map(|i| {
            if i < filled {
                text("●", color)
            } else {
                text("○", BORDER)
            }
        })
        .collect()
}

fn group(game: &Game) -> (&'static str, Color) {
    match game.status {
        GameStatus::Live => ("LIVE NOW", GREEN),
        GameStatus::Delayed | GameStatus::Suspended => ("PAUSED", AMBER),
        GameStatus::Scheduled => ("UPCOMING", BLUE),
        GameStatus::Final => ("FINAL", MUTED),
        _ => ("SCHEDULE CHANGES", AMBER),
    }
}

enum Row<'a> {
    Header(&'static str, usize, Color),
    Game(&'a Game),
}

fn empty_state(app: &App) -> (&'static str, &'static str) {
    match &app.state {
        DataState::Loading if app.snapshot.is_none() => (
            "Finding today's baseball…",
            "The map is ready. Fetching the schedule.",
        ),
        DataState::Failed(_) if app.snapshot.is_none() => (
            "Could not load the schedule",
            "Press r to retry. Use --demo to explore offline.",
        ),
        _ if !app.search.is_empty() => (
            "No matching games",
            "Esc clears your search. Left / right changes the day.",
        ),
        _ if app.view == View::Following && app.preferences.favorites.is_empty() => (
            "Your teams belong here",
            "Press 1, select a game, then f to follow either team.",
        ),
        _ if app.view == View::Following => (
            "No games for your followed teams in this schedule",
            "Try another day with ←/→, or press l to change leagues.",
        ),
        _ if app.view == View::Nearby => (
            "No mapped games on this day",
            "Try another day with left / right, or l for all leagues.",
        ),
        _ => (
            "A quiet day at the ballpark",
            "No games are scheduled. Left / right changes the day. g jumps to a date.",
        ),
    }
}

fn section(frame: &mut Frame, area: Rect, label: &str, tail: &str, color: Color) {
    let lead = format!(" {label} ");
    let tail = format!(" {tail} ");
    let rule = usize::from(area.width).saturating_sub(lead.width() + tail.width() + 1);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            bold(lead, color),
            text("─".repeat(rule), BORDER),
            text(tail, FAINT),
        ])),
        area,
    );
}

/// Shared column widths so every card in the slate lines up.
struct Columns {
    nickname: usize,
}

#[expect(clippy::too_many_lines, reason = "Keep this panel layout together")]
pub(super) fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let games = app.visible_games();
    let game_label = if games.len() == 1 { "game" } else { "games" };
    let title = match app.view {
        View::All => format!("THE SLATE  {} {game_label}", games.len()),
        View::Following => format!("FOLLOWING  {} {game_label}", games.len()),
        View::Nearby => format!(
            "NEAR {}",
            app.preferences
                .place
                .as_ref()
                .map_or("YOU", |p| p.name.as_str())
        ),
    };
    let panel = block(title);
    let mut inside = panel.inner(area);
    if inside.width < 4 || inside.height == 0 {
        frame.render_widget(panel, area);
        return;
    }
    let subtitle = (app.view == View::Nearby).then(|| {
        let unmapped = app
            .games()
            .iter()
            .filter(|g| g.venue.coordinates.is_none())
            .count();
        let row = Rect::new(inside.x, inside.y, inside.width, 1);
        inside.y += 1;
        inside.height = inside.height.saturating_sub(1);
        let label = format!(
            " Nearest first · straight-line miles{}",
            if unmapped > 0 {
                format!(" · {unmapped} unmapped")
            } else {
                String::new()
            }
        );
        (row, label)
    });
    if games.is_empty() {
        frame.render_widget(panel, area);
        if let Some((row, label)) = subtitle {
            frame.render_widget(Paragraph::new(text(label, FAINT)), row);
        }
        let (title, body) = empty_state(app);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(bold(title, TEXT)),
                Line::from(""),
                Line::from(text(body, MUTED)),
            ])
            .wrap(Wrap { trim: true }),
            inside.inner(Margin::new(1, 0)),
        );
        return;
    }

    let spacious = inside.height >= 18 && inside.width >= 34;
    let grouped = spacious && app.view != View::Nearby;
    let game_height = if spacious { 2 } else { 1 };
    let mut rows: Vec<(Row, u16)> = Vec::new();
    let mut current = None;
    for game in &games {
        let (label, color) = group(game);
        if grouped && current != Some(label) {
            let count = games.iter().filter(|g| group(g).0 == label).count();
            rows.push((Row::Header(label, count, color), 1));
            current = Some(label);
        }
        rows.push((Row::Game(game), game_height));
    }
    let total: u16 = rows.iter().map(|r| r.1).sum();
    let mut starts = Vec::with_capacity(rows.len());
    let mut y = 0u16;
    for row in &rows {
        starts.push(y);
        y += row.1;
    }
    let focus = rows
        .iter()
        .position(|r| matches!(r.0, Row::Game(g) if Some(g.id) == app.selected))
        .unwrap_or(0);
    // Keep the focused game and, where possible, its section header in view.
    let anchor = if focus > 0 && matches!(rows[focus - 1].0, Row::Header(..)) {
        focus - 1
    } else {
        focus
    };
    let focus_end = starts[focus] + rows[focus].1;
    let mut offset = focus_end.saturating_sub(inside.height);
    if starts[anchor] < offset {
        offset = starts[anchor].min(starts[focus]);
    }
    offset = starts.iter().copied().find(|&s| s >= offset).unwrap_or(0);
    let bottom = offset + inside.height;
    // Rows that start in view; the last card may show only its first line.
    let mut shown: Vec<usize> = (0..rows.len())
        .filter(|&i| starts[i] >= offset && starts[i] < bottom)
        .collect();
    // A section header with none of its games below it is noise.
    if let Some(&last) = shown.last()
        && matches!(rows[last].0, Row::Header(..))
    {
        shown.pop();
    }
    let positions: Vec<usize> = shown
        .iter()
        .filter(|&&i| starts[i] + rows[i].1 <= bottom)
        .filter_map(|&i| match rows[i].0 {
            Row::Game(game) => games.iter().position(|g| g.id == game.id),
            Row::Header(..) => None,
        })
        .collect();

    let live = games
        .iter()
        .filter(|g| g.status == GameStatus::Live)
        .count();
    let scrolled = total > inside.height;
    let summary = if scrolled {
        format!(
            " {}-{} of {} ",
            positions.iter().min().map_or(0, |p| p + 1),
            positions.iter().max().map_or(0, |p| p + 1),
            games.len()
        )
    } else {
        format!(" {live} live · {} {game_label} ", games.len())
    };
    frame.render_widget(
        panel.title_bottom(Line::from(vec![
            text("─", BORDER),
            text(summary, FAINT),
            text("─ ", BORDER),
            bold("/", TEXT),
            text(" search ", FAINT),
        ])),
        area,
    );
    if let Some((row, label)) = subtitle {
        frame.render_widget(Paragraph::new(text(label, FAINT)), row);
    }
    let columns = Columns {
        nickname: if inside.width >= 64 {
            games
                .iter()
                .flat_map(|g| [&g.away.name, &g.home.name])
                .map(|n| nickname(n).width())
                .max()
                .unwrap_or(0)
                .min(12)
        } else {
            0
        },
    };
    for index in shown {
        let (row, height) = &rows[index];
        let y = inside.y + starts[index] - offset;
        let height = (*height).min(inside.bottom() - y);
        match row {
            Row::Header(label, count, color) => {
                section(
                    frame,
                    Rect::new(inside.x, y, inside.width, 1),
                    label,
                    &count.to_string(),
                    *color,
                );
            }
            Row::Game(game) => {
                let rect = Rect::new(inside.x, y, inside.width, height);
                let stripe = games.iter().position(|g| g.id == game.id).unwrap_or(0) % 2 == 1;
                if spacious {
                    card(frame, rect, app, game, stripe, &columns);
                } else {
                    compact_row(frame, rect, app, game, false);
                }
            }
        }
    }
    // Following: fill spare rows with the rest of the slate, unselectable.
    if app.view == View::Following && total + 3 <= inside.height {
        let others: Vec<&Game> = app
            .games()
            .iter()
            .filter(|g| !app.is_favorite(g) && g.matches(&app.search))
            .collect();
        if !others.is_empty() {
            let mut y = inside.y + total + 1;
            section(
                frame,
                Rect::new(inside.x, y, inside.width, 1),
                "ALSO TODAY",
                "1 shows all",
                FAINT,
            );
            for game in others {
                y += 1;
                if y >= inside.bottom() {
                    break;
                }
                compact_row(
                    frame,
                    Rect::new(inside.x, y, inside.width, 1),
                    app,
                    game,
                    true,
                );
            }
        }
    }
    if scrolled {
        let mut state = ScrollbarState::new(usize::from(total - inside.height))
            .position(usize::from(offset))
            .viewport_content_length(usize::from(inside.height));
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(Some("│"))
                .track_style(Style::default().fg(BORDER))
                .thumb_symbol("┃")
                .thumb_style(Style::default().fg(MUTED)),
            Rect::new(area.x, inside.y, area.width, inside.height),
            &mut state,
        );
    }
}

/// A two-line scoreboard card: away over home, the live state or starting
/// pitchers in the middle, and the ballpark on the right.
#[expect(clippy::too_many_lines, reason = "Keep the card layout together")]
fn card(frame: &mut Frame, area: Rect, app: &App, game: &Game, stripe: bool, columns: &Columns) {
    let focused = Some(game.id) == app.selected;
    let background = if focused {
        SELECTED
    } else if stripe {
        STRIPE
    } else {
        BACKGROUND
    };
    // A fresh run glows green, then fades over about three seconds.
    let background = match app.scored.get(&game.id) {
        Some(&at) if app.tick.wrapping_sub(at) < 30 => {
            #[expect(clippy::cast_precision_loss, reason = "Ages are under 30 ticks")]
            let fade = 1.0 - app.tick.wrapping_sub(at) as f64 / 30.0;
            mix(background, GREEN, fade * (0.18 + 0.12 * pulse(app.tick, 6)))
        }
        _ => background,
    };
    let lead = leader(game);
    let final_game = game.status == GameStatus::Final;
    let side = |team: &Team, home: bool| -> Vec<Span<'static>> {
        let behind = lead == Some(!home);
        let dim = final_game && behind || !started(game) && game.status != GameStatus::Scheduled;
        let strong = if dim { MUTED } else { TEXT };
        let mut spans = vec![
            if focused {
                text("▎", AMBER)
            } else {
                Span::raw(" ")
            },
            Span::raw(" "),
            teams::chip(team),
            Span::raw(" "),
            bold(format!("{:<3}", team.abbreviation), strong),
            text(follow_mark(app, team), AMBER),
        ];
        if columns.nickname > 0 {
            spans.push(text(
                format!(" {:<w$}", nickname(&team.name), w = columns.nickname),
                if dim { FAINT } else { MUTED },
            ));
        }
        if started(game) {
            let style = if behind {
                Style::default().fg(strong)
            } else {
                Style::default().fg(strong).add_modifier(Modifier::BOLD)
            };
            spans.push(Span::styled(format!("{:>3}", score(team.score)), style));
        } else {
            spans.push(text(
                format!("{:>6}", team.record.clone().unwrap_or_default()),
                FAINT,
            ));
        }
        spans
    };
    let mut away = side(&game.away, false);
    let mut home = side(&game.home, true);
    let pad = width(&away).max(width(&home));
    for spans in [&mut away, &mut home] {
        let gap = pad - width(spans);
        spans.push(Span::raw(" ".repeat(gap + 2)));
    }
    // The state column.
    let state = status(app, game);
    away.push(bold(format!("{state:<9}"), status_color(app, game)));
    home.push(text(
        format!(
            "{:<9}",
            game.doubleheader
                .map_or(String::new(), |n| format!("Game {n}"))
        ),
        FAINT,
    ));
    let used = width(&away);
    let room = usize::from(area.width).saturating_sub(used + 1);
    // The middle column: bases, outs, and count live; starters before the game.
    let mut middle = 0;
    if game.status == GameStatus::Live
        && let Some(line) = &game.linescore
        && room >= 12
    {
        away.extend([
            Span::raw("  "),
            Span::raw(" "),
            base(line.bases[1]),
            Span::raw(" "),
        ]);
        away.push(Span::raw("  "));
        away.extend(dots(line.outs, 2, RED));
        home.extend([
            Span::raw("  "),
            base(line.bases[2]),
            Span::raw(" "),
            base(line.bases[0]),
        ]);
        home.push(Span::raw("  "));
        home.push(text(
            format!(
                "{}-{}",
                line.balls.map_or("?".into(), |b| b.to_string()),
                line.strikes.map_or("?".into(), |s| s.to_string())
            ),
            FAINT,
        ));
        middle = 10;
    } else if game.status == GameStatus::Scheduled && room >= 26 {
        let pitcher =
            |team: &Team| ellipsize(team.probable_pitcher.as_deref().map_or("TBD", surname), 11);
        away.push(text(format!("  {:<11}", pitcher(&game.away)), FAINT));
        home.push(text(format!("  {:<11}", pitcher(&game.home)), FAINT));
        middle = 13;
    }
    // The right column: the ballpark, or distance in Nearby.
    let (right_top, right_bottom) = if app.view == View::Nearby {
        (
            format!("{:.0} mi", app.distance(game).unwrap_or(0.)),
            game.venue.name.clone(),
        )
    } else {
        (game.venue.name.clone(), game.venue.city.clone())
    };
    let venue_color = if focused { MUTED } else { FAINT };
    let room = room.saturating_sub(middle + 2);
    // A venue that will not fit gives way to its shorter city, on one line.
    let (right_top, right_bottom) = if app.view != View::Nearby && right_top.width() > room {
        (right_bottom, String::new())
    } else {
        (right_top, right_bottom)
    };
    let lines = [(away, right_top), (home, right_bottom)];
    for (row, (mut spans, right)) in lines.into_iter().enumerate() {
        let Ok(row) = u16::try_from(row) else {
            continue;
        };
        if row >= area.height {
            break;
        }
        let right = if room >= 6 {
            ellipsize(&right, u16::try_from(room).unwrap_or(0))
        } else {
            String::new()
        };
        let gap = usize::from(area.width).saturating_sub(width(&spans) + right.width() + 1);
        spans.push(Span::raw(" ".repeat(gap)));
        spans.push(text(right, venue_color));
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(background)),
            Rect::new(area.x, area.y + row, area.width, 1),
        );
    }
}

/// One-line row for short terminals, and the dimmed "also today" list.
fn compact_row(frame: &mut Frame, area: Rect, app: &App, game: &Game, quiet: bool) {
    let focused = Some(game.id) == app.selected && !quiet;
    let background = if focused { SELECTED } else { BACKGROUND };
    let lead = leader(game);
    let color = |home: bool| {
        if quiet || game.status == GameStatus::Final && lead == Some(!home) {
            MUTED
        } else {
            TEXT
        }
    };
    let team = |team: &Team, home: bool| -> Vec<Span<'static>> {
        vec![
            teams::chip(team),
            Span::raw(" "),
            bold(format!("{:<3}", team.abbreviation), color(home)),
            text(follow_mark(app, team), AMBER),
            bold(
                format!(
                    "{:>2}",
                    if started(game) {
                        score(team.score)
                    } else {
                        String::new()
                    }
                ),
                color(home),
            ),
        ]
    };
    let mut spans = vec![
        if focused {
            text("▎", AMBER)
        } else {
            Span::raw(" ")
        },
        Span::raw(" "),
    ];
    spans.extend(team(&game.away, false));
    spans.push(text(if started(game) { "   " } else { " at" }, FAINT));
    spans.push(Span::raw(" "));
    spans.extend(team(&game.home, true));
    if let Some(n) = game.doubleheader {
        spans.push(text(format!(" G{n}"), FAINT));
    }
    let mut right = vec![];
    if app.view == View::Nearby && !quiet {
        right.push(text(
            format!(
                "{:>7} ",
                format!("{:.0} mi", app.distance(game).unwrap_or(0.))
            ),
            FAINT,
        ));
    }
    right.push(text(
        format!("{:>9}", status(app, game)),
        if quiet {
            FAINT
        } else {
            status_color(app, game)
        },
    ));
    let gap = usize::from(area.width).saturating_sub(width(&spans) + width(&right) + 1);
    spans.push(Span::raw(" ".repeat(gap.max(1))));
    spans.extend(right);
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(background)),
        area,
    );
}

/// The base diamond and count for a live game, three rows tall.
fn diamond(game: &Game) -> Vec<Line<'static>> {
    let Some(line) = &game.linescore else {
        return vec![];
    };
    let mut rows = vec![
        vec![Span::raw("  "), base(line.bases[1]), Span::raw("     ")],
        vec![
            base(line.bases[2]),
            Span::raw("   "),
            base(line.bases[0]),
            Span::raw("   "),
        ],
        vec![Span::raw("  "), text("▽", BORDER), Span::raw("     ")],
    ];
    rows[0].push(text("B ", FAINT));
    rows[0].extend(dots(line.balls, 3, BLUE));
    rows[1].push(text("S ", FAINT));
    rows[1].extend(dots(line.strikes, 2, AMBER));
    rows[2].push(text("O ", FAINT));
    rows[2].extend(dots(line.outs, 2, RED));
    rows.into_iter().map(Line::from).collect()
}

/// The state shown in a panel's top-right border.
fn state_title(app: &App, game: &Game) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    spans.extend(match game.status {
        GameStatus::Live => vec![
            bold("● ", live_color(app)),
            bold("LIVE  ", GREEN),
            bold(inning(game), TEXT),
        ],
        GameStatus::Scheduled => vec![
            text("First pitch ", MUTED),
            bold(app.start_label(game), BLUE),
        ],
        GameStatus::Final => vec![bold(status(app, game).to_uppercase(), MUTED)],
        _ => vec![bold(game.state_label(), AMBER)],
    });
    spans.push(Span::raw(" "));
    spans.push(text("─", BORDER));
    Line::from(spans).right_aligned()
}

fn pitchers_line(game: &Game) -> Line<'static> {
    Line::from(vec![
        text(
            if started(game) {
                "Starters  "
            } else {
                "Probables  "
            },
            FAINT,
        ),
        text(
            format!(
                "{} / {}",
                game.away.probable_pitcher.as_deref().unwrap_or("TBD"),
                game.home.probable_pitcher.as_deref().unwrap_or("TBD")
            ),
            MUTED,
        ),
    ])
}

fn broadcast_line(app: &App, game: &Game) -> Line<'static> {
    Line::from(vec![
        text(
            if started(game) {
                "Started "
            } else {
                "First pitch "
            },
            FAINT,
        ),
        text(app.start_label(game), MUTED),
        text("  ·  ", FAINT),
        text(
            if game.broadcasts.is_empty() {
                "Broadcast not listed".into()
            } else {
                game.broadcasts.join(" / ")
            },
            MUTED,
        ),
    ])
}

/// Linescore rows, header first, sized to `width`. Returns the lines and the
/// table's natural width.
#[expect(clippy::too_many_lines, reason = "Keep the linescore layout together")]
fn linescore(app: &App, game: &Game, available: u16, names: bool) -> (Vec<Line<'static>>, u16) {
    let live = game.status == GameStatus::Live;
    let innings = game
        .linescore
        .as_ref()
        .map(|l| l.innings.as_slice())
        .unwrap_or_default();
    let has_line = !innings.is_empty();
    let sheet = game.linescore.as_ref();
    let has_he = sheet.is_some_and(|l| {
        l.away_hits.is_some()
            || l.home_hits.is_some()
            || l.away_errors.is_some()
            || l.home_errors.is_some()
    });
    let totals = if !has_line {
        0
    } else if has_he {
        10
    } else {
        4
    };
    let max_slots = usize::from(available.saturating_sub(7 + totals + 2) / 3);
    let wanted = innings.len().max(9);
    let slots = if has_line {
        wanted.min(max_slots.max(1))
    } else {
        0
    };
    let first_slot = innings.len().saturating_sub(slots);
    let hidden_after = first_slot + slots < wanted;
    let name_width = if names {
        usize::from(available)
            .saturating_sub(7 + slots * 3 + usize::from(totals) + 1)
            .min(28)
    } else {
        0
    };
    // Both rows share one naming style: full names, then without records, then nicknames.
    let record_width = |t: &Team| t.record.as_ref().map_or(0, |r| r.width() + 1);
    let fits = |width: &dyn Fn(&Team) -> usize| {
        [&game.away, &game.home]
            .iter()
            .all(|t| width(t) < name_width)
    };
    let full_names = fits(&|t| t.name.width());
    let records = if full_names {
        fits(&|t| t.name.width() + record_width(t))
    } else {
        fits(&|t| nickname(&t.name).width() + record_width(t))
    };
    let show_names = name_width >= 8 && (full_names || fits(&|t| nickname(&t.name).width()));
    let name_width = if show_names { name_width } else { 0 };
    let current = sheet.and_then(|l| l.inning);
    let lead = leader(game);
    let mut lines = vec![];
    if has_line {
        let mut head = vec![Span::raw(" ".repeat(7 + name_width))];
        for slot in first_slot..first_slot + slots {
            let number = u16::try_from(slot + 1).unwrap_or(u16::MAX);
            let style = if live && Some(number) == current {
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(FAINT)
            };
            let cell = if slot == first_slot && first_slot > 0 {
                format!("‹{number:>2}")
            } else {
                format!("{number:>3}")
            };
            head.push(Span::styled(cell, style));
        }
        head.push(bold(
            format!(
                "{}  R{}",
                if hidden_after { " ›" } else { "  " },
                if has_he { "  H  E" } else { "" }
            ),
            FAINT,
        ));
        lines.push(Line::from(head));
    }
    for (home, team) in [(false, &game.away), (true, &game.home)] {
        let dim = game.status == GameStatus::Final && lead == Some(!home);
        let strong = if dim { MUTED } else { TEXT };
        let mut row = vec![
            teams::chip(team),
            Span::raw(" "),
            bold(format!("{:<3}", team.abbreviation), strong),
            text(follow_mark(app, team), AMBER),
            Span::raw(" "),
        ];
        if show_names {
            let record = if records {
                team.record
                    .as_ref()
                    .map_or(String::new(), |r| format!(" {r}"))
            } else {
                String::new()
            };
            let name = if full_names {
                team.name.clone()
            } else {
                nickname(&team.name).to_string()
            };
            let pad = name_width.saturating_sub(name.width() + record.width());
            row.push(text(name, if dim { FAINT } else { MUTED }));
            row.push(text(record, FAINT));
            row.push(Span::raw(" ".repeat(pad)));
        }
        if has_line {
            for slot in first_slot..first_slot + slots {
                let value = innings
                    .get(slot)
                    .and_then(|i| if home { i.home } else { i.away });
                row.push(match value {
                    Some(runs) => text(format!("{runs:>3}"), if runs > 0 { strong } else { MUTED }),
                    None => text(format!("{:>3}", "·"), BORDER),
                });
            }
            let (hits, errors) = if home {
                (
                    sheet.and_then(|l| l.home_hits),
                    sheet.and_then(|l| l.home_errors),
                )
            } else {
                (
                    sheet.and_then(|l| l.away_hits),
                    sheet.and_then(|l| l.away_errors),
                )
            };
            row.push(Span::raw(" "));
            let runs = format!("{:>3}", score(team.score));
            row.push(if lead == Some(!home) {
                text(runs, strong)
            } else {
                bold(runs, strong)
            });
            if has_he {
                row.push(text(
                    format!("{:>3}{:>3}", score(hits), score(errors)),
                    MUTED,
                ));
            }
        } else if game.status == GameStatus::Scheduled {
            row.push(text("SP ", FAINT));
            row.push(text(
                team.probable_pitcher
                    .clone()
                    .unwrap_or_else(|| "TBD".into()),
                TEXT,
            ));
        } else if team.score.is_some() {
            row.push(bold(score(team.score), strong));
        }
        lines.push(Line::from(row));
    }
    let natural =
        u16::try_from(7 + name_width + slots * 3 + usize::from(totals) + 3).unwrap_or(u16::MAX);
    (lines, natural)
}

pub(super) fn details(frame: &mut Frame, area: Rect, app: &App) {
    let panel = block("AT THE BALLPARK");
    let inside = panel.inner(area).inner(Margin::new(1, 0));
    let Some(game) = app.selected_game() else {
        frame.render_widget(panel, area);
        frame.render_widget(
            Paragraph::new(text(
                "Select a game to see its ballpark, pitchers, and broadcast.",
                MUTED,
            ))
            .wrap(Wrap { trim: true }),
            inside,
        );
        return;
    };
    frame.render_widget(panel.title(state_title(app, game)), area);
    if inside.width < 20 || inside.height == 0 {
        return;
    }
    let mut lines: Vec<Line> = vec![Line::from(vec![
        bold(game.venue.name.clone(), AMBER),
        text(format!("  {}", game.venue.city), MUTED),
        text(
            game.doubleheader
                .map_or(String::new(), |n| format!("  ·  Game {n}")),
            MUTED,
        ),
        text(
            app.distance(game)
                .map_or(String::new(), |d| format!("  ·  {d:.0} mi away")),
            FAINT,
        ),
    ])];
    let roomy = inside.height >= 8;
    if roomy {
        lines.push(Line::from(""));
    }
    let live = game.status == GameStatus::Live && game.linescore.is_some();
    let side_width: u16 = if live && inside.width >= 56 { 18 } else { 0 };
    let (table, natural) = linescore(app, game, inside.width - side_width, true);
    let board_top = u16::try_from(lines.len()).unwrap_or(0);
    lines.extend(table);
    if roomy && lines.len() < usize::from(inside.height) {
        lines.push(Line::from(""));
    }
    if game.status != GameStatus::Scheduled {
        lines.push(pitchers_line(game));
    }
    lines.push(broadcast_line(app, game));
    frame.render_widget(Paragraph::new(lines), inside);
    if side_width > 0 {
        let rows = diamond(game);
        let height = u16::try_from(rows.len()).unwrap_or(0);
        let top = inside.y + board_top;
        if top + height <= inside.bottom() {
            // Sit just right of the linescore rather than at the far edge.
            let x = (inside.x + natural + 2).min(inside.right() - side_width + 4);
            frame.render_widget(
                Paragraph::new(rows),
                Rect::new(x, top, side_width - 4, height),
            );
        }
    }
}

/// Two lines for short terminals: the score and live state, then the ballpark.
pub(super) fn compact_detail(frame: &mut Frame, area: Rect, app: &App) {
    let Some(game) = app.selected_game() else {
        return;
    };
    let mut first = vec![Span::raw(" ")];
    for (i, team) in [&game.away, &game.home].into_iter().enumerate() {
        if i == 1 {
            first.push(text(if started(game) { "  –  " } else { "  at  " }, FAINT));
        }
        first.push(teams::chip(team));
        first.push(bold(format!(" {}", team.abbreviation), TEXT));
        if started(game) {
            first.push(bold(format!(" {}", score(team.score)), TEXT));
        }
    }
    first.push(Span::raw("   "));
    first.push(bold(status(app, game), status_color(app, game)));
    if game.status == GameStatus::Live
        && let Some(line) = &game.linescore
    {
        first.push(Span::raw("   "));
        first.extend([
            base(line.bases[2]),
            base(line.bases[1]),
            base(line.bases[0]),
        ]);
        first.push(Span::raw("  "));
        first.extend(dots(line.outs, 2, RED));
        first.push(text(
            format!(
                "  {}-{}",
                line.balls.unwrap_or(0),
                line.strikes.unwrap_or(0)
            ),
            FAINT,
        ));
    } else if game.status == GameStatus::Scheduled {
        first.push(text(
            format!(
                "   {} vs {}",
                game.away.probable_pitcher.as_deref().map_or("TBD", surname),
                game.home.probable_pitcher.as_deref().map_or("TBD", surname)
            ),
            FAINT,
        ));
    }
    let second = format!(
        " {} · {} · {}",
        game.venue.name,
        game.venue.city,
        if game.broadcasts.is_empty() {
            "Broadcast not listed".into()
        } else {
            game.broadcasts.join(" / ")
        }
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(first),
            Line::from(text(ellipsize(&second, area.width), MUTED)),
        ])
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(BORDER)),
        ),
        area,
    );
}

/// Degrees with hemispheres: "38.9°N 77.0°W".
fn coordinates(position: Coordinates) -> String {
    format!(
        "{:.1}°{} {:.1}°{}",
        position.latitude.abs(),
        if position.latitude >= 0.0 { 'N' } else { 'S' },
        position.longitude.abs(),
        if position.longitude >= 0.0 { 'E' } else { 'W' }
    )
}

/// The expanded game view: a large scoreboard, every inning, the facts of
/// the day, and a locator map of the ballpark.
#[expect(clippy::too_many_lines, reason = "Keep the game view layout together")]
pub(super) fn gamecast(frame: &mut Frame, area: Rect, app: &App) {
    let Some(game) = app.selected_game() else {
        return;
    };
    let panel = block("GAME DETAILS").title(state_title(app, game));
    let inside = panel.inner(area).inner(Margin::new(2, 1));
    frame.render_widget(panel, area);
    if inside.width < 30 || inside.height < 6 {
        return;
    }
    let map_width = if inside.width >= 90 {
        inside.width * 2 / 5
    } else {
        0
    };
    let left = Rect::new(
        inside.x,
        inside.y,
        inside.width - map_width.saturating_add(if map_width > 0 { 2 } else { 0 }),
        inside.height,
    );
    let mut lines = vec![];
    for team in [&game.away, &game.home] {
        lines.push(Line::from(vec![
            teams::chip(team),
            teams::chip(team),
            Span::raw(" "),
            bold(team.name.to_uppercase(), TEXT),
            text(
                team.record
                    .as_ref()
                    .map_or(String::new(), |r| format!("  {r}")),
                FAINT,
            ),
            text(follow_mark(app, team), AMBER),
        ]));
    }
    lines.push(Line::from(""));
    let (table, _) = linescore(app, game, left.width, false);
    lines.extend(table);
    if game.status == GameStatus::Live
        && let Some(line) = &game.linescore
    {
        lines.push(Line::from(""));
        let mut count = vec![text("Bases ", FAINT)];
        count.extend([
            base(line.bases[2]),
            base(line.bases[1]),
            base(line.bases[0]),
        ]);
        count.push(text("   Count ", FAINT));
        count.push(text(
            format!("{}-{}", line.balls.unwrap_or(0), line.strikes.unwrap_or(0)),
            TEXT,
        ));
        count.push(text("   Outs ", FAINT));
        count.extend(dots(line.outs, 2, RED));
        lines.push(Line::from(count));
    }
    lines.push(Line::from(""));
    let mut facts = vec![
        (
            "Ballpark",
            format!("{} · {}", game.venue.name, game.venue.city),
        ),
        (
            if started(game) {
                "Started"
            } else {
                "First pitch"
            },
            format!("{} {}", app.start_label(game), super::zone_city(app)),
        ),
        (
            if started(game) {
                "Starters"
            } else {
                "Probables"
            },
            format!(
                "{} / {}",
                game.away.probable_pitcher.as_deref().unwrap_or("TBD"),
                game.home.probable_pitcher.as_deref().unwrap_or("TBD")
            ),
        ),
        (
            "Broadcast",
            if game.broadcasts.is_empty() {
                "Not listed".into()
            } else {
                game.broadcasts.join(" / ")
            },
        ),
    ];
    if let Some(position) = game.venue.coordinates {
        let mut value = coordinates(position);
        if let (Some(miles), Some(place)) = (app.distance(game), &app.preferences.place) {
            value = format!("{value} · {miles:.0} mi from {}", place.name);
        }
        facts.push(("Location", value));
    }
    facts.push(("League", game.league.clone()));
    for (label, value) in facts {
        lines.push(Line::from(vec![
            text(format!("{label:<12}"), FAINT),
            text(ellipsize(&value, left.width.saturating_sub(12)), TEXT),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), left);
    if map_width > 0
        && let Some(position) = game.venue.coordinates
    {
        let map = Rect::new(
            inside.right() - map_width,
            inside.y,
            map_width,
            inside.height,
        );
        atlas::locator(frame, map, app, position, &game.venue.city);
    }
}
