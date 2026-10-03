use ratatui::widgets::Wrap;

use super::*;

pub(super) fn draw(frame: &mut Frame, area: Rect, app: &App, spacious: bool) {
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
    frame.render_widget(panel, area);
    if inside.width < 4 || inside.height == 0 {
        return;
    }
    if app.view == View::Nearby {
        let unmapped = app
            .games()
            .iter()
            .filter(|g| g.venue.coordinates.is_none())
            .count();
        let label = format!(
            " Nearest first · straight-line mi{}",
            if unmapped > 0 {
                format!(" · {unmapped} unmapped")
            } else {
                String::new()
            }
        );
        frame.render_widget(
            Paragraph::new(text(label, MUTED)),
            Rect::new(inside.x, inside.y, inside.width, 1),
        );
        inside.y += 1;
        inside.height = inside.height.saturating_sub(1);
    }
    if games.is_empty() {
        let (title, body) = match &app.state {
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
            _ if app.view == View::Following => (
                "Your teams belong here",
                "Press 1, select a game, then f to follow either team.",
            ),
            _ if app.view == View::Nearby => (
                "No mapped games on this day",
                "Try another day with left / right, or l for all leagues.",
            ),
            _ => (
                "A quiet day at the ballpark",
                "No games are scheduled. Left / right changes the day. g jumps to a date.",
            ),
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(bold(format!(" {title}"), TEXT)),
                Line::from(""),
                Line::from(text(format!(" {body}"), MUTED)),
            ])
            .wrap(Wrap { trim: false }),
            inside,
        );
        return;
    }
    let row_height = if spacious { 2u16 } else { 1u16 };
    let available = inside.height.saturating_sub(1);
    let capacity = usize::from((available / row_height).max(1));
    let selected = games
        .iter()
        .position(|g| Some(g.id) == app.selected)
        .unwrap_or(0);
    let first = selected.saturating_sub(capacity.saturating_sub(1));
    let end = (first + capacity).min(games.len());
    for (row, game) in games[first..end].iter().enumerate() {
        let y = inside.y + row as u16 * row_height;
        if y >= inside.bottom() {
            break;
        }
        let focused = Some(game.id) == app.selected;
        let background = if focused { SELECTED } else { BACKGROUND };
        let favorite = if app.is_favorite(game) {
            if app.ascii { "*" } else { "★" }
        } else {
            " "
        };
        let mut state = if game.status == GameStatus::Scheduled {
            app.start_label(game)
        } else {
            game.state_label()
        };
        if !spacious && app.view == View::Nearby {
            state = format!("{:.0}mi {state}", app.distance(game).unwrap_or(0.));
        }
        let scores = |value: Option<u16>| value.map_or("-".into(), |s| s.to_string());
        let matchup = if matches!(
            game.status,
            GameStatus::Scheduled | GameStatus::Postponed | GameStatus::Cancelled
        ) {
            format!(
                "{:>3} at {:<3}",
                game.away.abbreviation, game.home.abbreviation
            )
        } else {
            format!(
                "{:>3} {:>2}  {:<3} {:>2}",
                game.away.abbreviation,
                scores(game.away.score),
                game.home.abbreviation,
                scores(game.home.score)
            )
        };
        let prefix = format!(
            "{}{} ",
            if focused {
                if app.ascii { ">" } else { "›" }
            } else {
                " "
            },
            favorite
        );
        let number = game
            .doubleheader
            .map_or(String::new(), |n| format!(" G{n}"));
        let content_width =
            (prefix.width() + matchup.width() + number.width() + state.width() + 2) as u16;
        let gap = inside.width.saturating_sub(content_width).max(1) as usize;
        let line = Line::from(vec![
            text(prefix, AMBER),
            bold(matchup, if focused { TEXT } else { state_color(game) }),
            text(number, MUTED),
            text(" ".repeat(gap), MUTED),
            text(state, state_color(game)),
        ]);
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(background)),
            Rect::new(inside.x, y, inside.width, 1),
        );
        if spacious && y + 1 < inside.bottom() {
            let subtitle = if app.view == View::Nearby {
                format!(
                    "   {:.0} mi · {}",
                    app.distance(game).unwrap_or(0.),
                    game.venue.name
                )
            } else {
                format!("   {} · {}", game.venue.city, game.venue.name)
            };
            frame.render_widget(
                Paragraph::new(text(ellipsize(&subtitle, inside.width), MUTED))
                    .style(Style::default().bg(background)),
                Rect::new(inside.x, y + 1, inside.width, 1),
            );
        }
    }
    let live = games
        .iter()
        .filter(|g| g.status == GameStatus::Live)
        .count();
    let summary = if end < games.len() || first > 0 {
        format!(" {}-{} of {} · ↑↓ scroll", first + 1, end, games.len())
    } else {
        format!(" {live} live · {} {game_label} · / search", games.len())
    };
    frame.render_widget(
        Paragraph::new(text(summary, MUTED)),
        Rect::new(inside.x, inside.bottom().saturating_sub(1), inside.width, 1),
    );
}

fn score(value: Option<u16>) -> String {
    value.map_or("-".into(), |n| n.to_string())
}

pub(super) fn details(frame: &mut Frame, area: Rect, app: &App) {
    let panel = block("AT THE BALLPARK");
    let inside = panel.inner(area).inner(Margin::new(1, 0));
    frame.render_widget(panel, area);
    let Some(game) = app.selected_game() else {
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
    let matchup = format!(
        "{}  {}   @   {}  {}",
        game.away.abbreviation,
        if game.status == GameStatus::Scheduled {
            String::new()
        } else {
            score(game.away.score)
        },
        game.home.abbreviation,
        if game.status == GameStatus::Scheduled {
            String::new()
        } else {
            score(game.home.score)
        }
    );
    let mut lines = vec![
        Line::from(vec![
            bold(matchup, TEXT),
            text(format!("    {}", game.state_label()), state_color(game)),
            text(
                game.doubleheader
                    .map_or(String::new(), |n| format!(" · Game {n}")),
                MUTED,
            ),
        ]),
        Line::from(vec![
            bold(game.venue.name.clone(), AMBER),
            text(format!(" · {}", game.venue.city), MUTED),
        ]),
        Line::from(text(
            format!("{} at {}", game.away.name, game.home.name),
            TEXT,
        )),
    ];
    if let Some(line) = &game.linescore {
        if !line.innings.is_empty() {
            let max_innings = ((inside.width.saturating_sub(17)) / 3).clamp(1, 12) as usize;
            let start = line.innings.len().saturating_sub(max_innings);
            let innings = &line.innings[start..];
            let head = innings
                .iter()
                .map(|i| format!("{:>3}", i.number))
                .collect::<String>();
            lines.push(Line::from(text(format!("     {head}    R  H  E"), MUTED)));
            let away = innings
                .iter()
                .map(|i| format!("{:>3}", score(i.away)))
                .collect::<String>();
            let home = innings
                .iter()
                .map(|i| format!("{:>3}", score(i.home)))
                .collect::<String>();
            lines.push(Line::from(bold(
                format!(
                    "{:<4} {away}   {:>2} {:>2} {:>2}",
                    game.away.abbreviation,
                    score(game.away.score),
                    score(line.away_hits),
                    score(line.away_errors)
                ),
                TEXT,
            )));
            lines.push(Line::from(bold(
                format!(
                    "{:<4} {home}   {:>2} {:>2} {:>2}",
                    game.home.abbreviation,
                    score(game.home.score),
                    score(line.home_hits),
                    score(line.home_errors)
                ),
                TEXT,
            )));
        }
        if game.status == GameStatus::Live {
            let bases = line.bases.map(|occupied| {
                if occupied {
                    if app.ascii { "X" } else { "◆" }
                } else if app.ascii {
                    "."
                } else {
                    "◇"
                }
            });
            let count = |n: Option<u8>| n.map_or("?".into(), |n| n.to_string());
            lines.push(Line::from(vec![
                text(
                    format!("Bases {} {} {}  ", bases[2], bases[1], bases[0]),
                    AMBER,
                ),
                text(
                    format!(
                        "Balls {} · Strikes {} · Outs {}",
                        count(line.balls),
                        count(line.strikes),
                        count(line.outs)
                    ),
                    GREEN,
                ),
            ]));
        }
    }
    if lines.len() < inside.height as usize {
        lines.push(Line::from(text(
            format!(
                "Probables  {} / {}",
                game.away.probable_pitcher.as_deref().unwrap_or("TBD"),
                game.home.probable_pitcher.as_deref().unwrap_or("TBD")
            ),
            MUTED,
        )));
    }
    if lines.len() < inside.height as usize {
        lines.push(Line::from(text(
            format!(
                "First pitch {} · {}",
                app.start_label(game),
                if game.broadcasts.is_empty() {
                    "Broadcast not listed".into()
                } else {
                    game.broadcasts.join(" / ")
                }
            ),
            MUTED,
        )));
    }
    frame.render_widget(Paragraph::new(lines), inside);
}

pub(super) fn compact_detail(frame: &mut Frame, area: Rect, app: &App) {
    let Some(game) = app.selected_game() else {
        return;
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                bold(
                    format!(" {} @ {} ", game.away.abbreviation, game.home.abbreviation),
                    TEXT,
                ),
                text(format!(" {} · {}", game.venue.name, game.venue.city), AMBER),
            ]),
            Line::from(text(
                format!(
                    " First pitch {} · Enter details · f Follow",
                    app.start_label(game)
                ),
                MUTED,
            )),
        ])
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(BORDER)),
        ),
        area,
    );
}
