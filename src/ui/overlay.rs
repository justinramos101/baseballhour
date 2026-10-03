use ratatui::widgets::{Clear, Wrap};

use crate::model::League;

use super::{
    App, BACKGROUND, Block, Frame, GREEN, Input, Line, MUTED, PANEL, Paragraph, RED, Rect, Span,
    Style, TEXT, block, bold, games, text,
};

#[expect(clippy::too_many_lines, reason = "Keep this panel layout together")]
pub(super) fn draw(frame: &mut Frame, app: &App) {
    if matches!(app.input, Input::Normal | Input::Search) {
        return;
    }
    let compact = frame.area().width < 60 || frame.area().height < 18;
    let (title, mut lines, text_input) = match &app.input {
        Input::Help => (
            "Keyboard guide",
            if compact {
                vec![
                    Line::from("  ↑↓ Game   ←→ Day   1/2/3 Views"),
                    Line::from("  / Search  f Follow p Place"),
                    Line::from("  g Date    l League ? Help"),
                    Line::from(text("  Times use the displayed timezone.", MUTED)),
                ]
            } else {
                vec![
                    Line::from("  1 / 2 / 3       All games / Following / Nearby"),
                    Line::from("  Up / Down, j/k  Select a game and its ballpark"),
                    Line::from("  Left / Right    Previous / next schedule day"),
                    Line::from("  g               Jump to a date, YYYY-MM-DD"),
                    Line::from("  t               Return to today"),
                    Line::from("  /               Search teams, cities, ballparks"),
                    Line::from("  f               Follow or unfollow either team"),
                    Line::from("  p               Choose a nearby city or coordinates"),
                    Line::from("  l               MLB, MiLB level, or all leagues"),
                    Line::from("  Enter           Expand game details"),
                    Line::from("  r               Refresh the schedule"),
                    Line::from("  Esc             Close, clear search, return to All"),
                    Line::from("  q / Ctrl-C      Quit"),
                    Line::from(text(
                        "Times use the displayed timezone. Dates are MLB slate dates.",
                        MUTED,
                    )),
                    Line::from(text(
                        "+1d means the game starts on the next local day.",
                        MUTED,
                    )),
                    Line::from(text(
                        "Nearby: straight-line miles. Use l for minor leagues, too.",
                        MUTED,
                    )),
                ]
            },
            false,
        ),
        Input::Date { value, error } => (
            "Jump to a date",
            vec![
                Line::from("Enter an official schedule date."),
                Line::from(""),
                Line::from(bold(format!(" > {value}_"), GREEN)),
                Line::from(text(" YYYY-MM-DD · for example 2026-07-04", MUTED)),
                Line::from(text(error.clone().unwrap_or_default(), RED)),
            ],
            true,
        ),
        Input::Place { value, error } => (
            "Choose your nearby place",
            vec![
                Line::from("Enter a city, today's ballpark, or latitude,longitude."),
                Line::from(text("Try Denver, Boston, Durham, or 39.75,-104.99.", MUTED)),
                Line::from(""),
                Line::from(bold(format!(" > {value}_"), GREEN)),
                Line::from(""),
                Line::from(text(
                    "Your choice stays on this device. No location account needed.",
                    MUTED,
                )),
                Line::from(text(error.clone().unwrap_or_default(), RED)),
            ],
            true,
        ),
        Input::Favorite { index } => {
            let mut lines = vec![
                Line::from("Choose a team to follow or unfollow."),
                Line::from(""),
            ];
            if let Some(game) = app.selected_game() {
                for (i, team) in [&game.away, &game.home].iter().enumerate() {
                    lines.push(Line::from(Span::styled(
                        format!(
                            " {} {} {}",
                            if i == *index { ">" } else { " " },
                            if app.preferences.favorites.contains(&team.id) {
                                "[following]"
                            } else {
                                "[ ]"
                            },
                            team.name
                        ),
                        Style::default().fg(if i == *index { GREEN } else { TEXT }),
                    )));
                }
            }
            lines.push(Line::from(""));
            lines.push(Line::from(text(
                "↑↓ Choose · Enter toggle · 2 opens Following",
                MUTED,
            )));
            ("Follow a team", lines, false)
        }
        Input::League { index } => (
            "Choose a league",
            League::ALL
                .iter()
                .enumerate()
                .map(|(i, league)| {
                    Line::from(Span::styled(
                        format!(
                            " {} {}",
                            if i == *index { ">" } else { " " },
                            league.label()
                        ),
                        Style::default().fg(if i == *index { GREEN } else { TEXT }),
                    ))
                })
                .collect(),
            false,
        ),
        Input::Details => {
            let screen = frame.area();
            let rect = centered(screen, 94, 17);
            frame.render_widget(Clear, rect);
            frame.render_widget(
                Block::default().style(Style::default().bg(BACKGROUND)),
                rect,
            );
            games::details(frame, rect, app);
            frame.render_widget(
                Paragraph::new(text("Esc Close · q Quit", MUTED))
                    .alignment(ratatui::layout::Alignment::Center),
                Rect::new(
                    rect.x + 1,
                    rect.bottom().saturating_sub(2),
                    rect.width.saturating_sub(2),
                    1,
                ),
            );
            return;
        }
        _ => return,
    };
    lines.push(Line::from(""));
    lines.push(Line::from(text(
        if text_input {
            "Enter confirm · Esc Close"
        } else {
            "Esc Close · q Quit"
        },
        MUTED,
    )));
    let rect = centered(
        frame.area(),
        70,
        u16::try_from(lines.len() + 4).unwrap_or(u16::MAX),
    );
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(TEXT).bg(PANEL))
            .block(
                block(title)
                    .padding(if compact {
                        ratatui::widgets::Padding::new(1, 1, 0, 0)
                    } else {
                        ratatui::widgets::Padding::new(2, 2, 1, 1)
                    })
                    .style(Style::default().bg(PANEL)),
            )
            .wrap(Wrap { trim: false }),
        rect,
    );
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}
