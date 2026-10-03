use ratatui::{
    style::Modifier,
    widgets::{Clear, Padding, Wrap},
};

use crate::model::League;

use super::{
    AMBER, App, BACKGROUND, FAINT, Frame, GREEN, Input, KEYCAP, Line, MUTED, PANEL, Paragraph, RED,
    Rect, SELECTED, Span, Style, TEXT, block, bold, dim, games, keycap, teams, text,
};

const HELP: [(&[&str], &str); 13] = [
    (&["1", "2", "3"], "All games / Following / Nearby"),
    (&["↑", "↓", "j", "k"], "Select a game and its ballpark"),
    (&["←", "→"], "Previous / next schedule day"),
    (&["g"], "Jump to a date, YYYY-MM-DD"),
    (&["t"], "Return to today"),
    (&["/"], "Search teams, cities, ballparks"),
    (&["f"], "Follow or unfollow either team"),
    (&["p"], "Choose a nearby city or coordinates"),
    (&["l"], "MLB, MiLB level, or all leagues"),
    (&["Enter"], "Expand game details"),
    (&["r"], "Refresh the schedule"),
    (&["Esc"], "Close, clear search, return to All"),
    (&["q", "Ctrl-C"], "Quit"),
];

fn caret(app: &App) -> &'static str {
    if app.ascii {
        "_"
    } else if app.tick % 10 < 6 {
        "▏"
    } else {
        " "
    }
}

fn field(app: &App, value: &str) -> Line<'static> {
    let style = Style::default().bg(KEYCAP);
    Line::from(vec![
        Span::styled(" › ", style.fg(AMBER).add_modifier(Modifier::BOLD)),
        Span::styled(
            value.to_string(),
            style.fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(caret(app), style.fg(AMBER)),
        Span::styled(" ".repeat(24usize.saturating_sub(value.len())), style),
    ])
}

fn choice(selected: bool, content: Vec<Span<'static>>) -> Line<'static> {
    let mut spans = vec![if selected {
        bold(" › ", AMBER)
    } else {
        Span::raw("   ")
    }];
    spans.extend(content);
    let line = Line::from(spans);
    if selected {
        line.style(Style::default().bg(SELECTED))
    } else {
        line
    }
}

/// Darken everything behind a dialog, then cast a soft shadow.
fn backdrop(frame: &mut Frame, rect: Rect) {
    let screen = frame.area();
    dim(frame.buffer_mut(), screen, 0.55);
    let shadow = Rect::new(rect.x + 2, rect.y + 1, rect.width, rect.height).intersection(screen);
    dim(frame.buffer_mut(), shadow, 0.6);
    frame.render_widget(Clear, rect);
}

#[expect(clippy::too_many_lines, reason = "Keep this panel layout together")]
pub(super) fn draw(frame: &mut Frame, app: &App) {
    if matches!(app.input, Input::Normal | Input::Search) {
        return;
    }
    let compact = frame.area().width < 60 || frame.area().height < 18;
    let (title, mut lines, text_input) = match &app.input {
        Input::Help => (
            "KEYBOARD GUIDE",
            if compact {
                vec![
                    Line::from("  ↑↓ Game   ←→ Day   1/2/3 Views"),
                    Line::from("  / Search  f Follow p Place"),
                    Line::from("  g Date    l League ? Help"),
                    Line::from(text("  Times use the displayed timezone.", MUTED)),
                ]
            } else {
                let mut lines: Vec<Line> = HELP
                    .iter()
                    .map(|(keys, action)| {
                        let mut spans = vec![];
                        for cap in *keys {
                            spans.push(keycap(&format!(" {cap} ")));
                            spans.push(Span::raw(" "));
                        }
                        let used = super::width(&spans);
                        spans.push(Span::raw(" ".repeat(18usize.saturating_sub(used))));
                        spans.push(text(*action, TEXT));
                        Line::from(spans)
                    })
                    .collect();
                lines.push(Line::from(""));
                for note in [
                    "Times use the displayed timezone. Dates are MLB slate dates.",
                    "+1d means the game starts on the next local day.",
                    "Nearby: straight-line miles. Use l for minor leagues, too.",
                ] {
                    lines.push(Line::from(text(note, FAINT)));
                }
                lines
            },
            false,
        ),
        Input::Date { value, error } => (
            "JUMP TO A DATE",
            vec![
                Line::from("Enter an official schedule date."),
                Line::from(""),
                field(app, value),
                Line::from(text(" YYYY-MM-DD · for example 2026-07-04", FAINT)),
                Line::from(text(error.clone().unwrap_or_default(), RED)),
            ],
            true,
        ),
        Input::Place { value, error } => (
            "CHOOSE A NEARBY PLACE",
            vec![
                Line::from("Enter a city, today's ballpark, or latitude,longitude."),
                Line::from(text("Try Denver, Boston, Durham, or 39.75,-104.99.", MUTED)),
                Line::from(""),
                field(app, value),
                Line::from(""),
                Line::from(text(
                    "Your choice stays on this device. No location account needed.",
                    FAINT,
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
                    let following = app.preferences.favorites.contains(&team.id);
                    lines.push(choice(
                        i == *index,
                        vec![
                            text(
                                if following {
                                    if app.ascii { "* " } else { "★ " }
                                } else {
                                    "  "
                                },
                                AMBER,
                            ),
                            teams::chip(team),
                            bold(format!(" {}", team.name), TEXT),
                            text(if following { "  following" } else { "" }, GREEN),
                        ],
                    ));
                }
            }
            lines.push(Line::from(""));
            lines.push(Line::from(text(
                "↑↓ Choose · Enter toggle · 2 opens Following",
                MUTED,
            )));
            ("FOLLOW A TEAM", lines, false)
        }
        Input::League { index } => (
            "CHOOSE A LEAGUE",
            League::ALL
                .iter()
                .enumerate()
                .map(|(i, league)| {
                    let current = *league == app.query.league;
                    choice(
                        i == *index,
                        vec![
                            text(league.label(), if i == *index { TEXT } else { MUTED }),
                            text(if current { "  current" } else { "" }, FAINT),
                        ],
                    )
                })
                .collect(),
            false,
        ),
        Input::Details => {
            let screen = frame.area();
            let rect = centered(screen, 116, 21);
            backdrop(frame, rect);
            frame.render_widget(
                ratatui::widgets::Block::default().style(Style::default().bg(BACKGROUND)),
                rect,
            );
            if rect.width >= 60 && rect.height >= 14 {
                games::gamecast(frame, rect, app);
            } else {
                games::details(frame, rect, app);
            }
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    text(" Esc Close · q Quit ", MUTED),
                    text("─", super::BORDER),
                ]))
                .alignment(ratatui::layout::Alignment::Right),
                Rect::new(
                    rect.x + 2,
                    rect.bottom().saturating_sub(1),
                    rect.width.saturating_sub(3),
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
    // Size the dialog to its content; drop vertical padding when space is tight.
    let screen = frame.area();
    let content = lines.iter().map(Line::width).max().unwrap_or(0);
    let wide = u16::try_from(content + 6)
        .unwrap_or(u16::MAX)
        .clamp(44, 76)
        .min(screen.width.saturating_sub(2));
    // Count rows after wrapping at the dialog's inner width.
    let inner = usize::from(wide.saturating_sub(if compact { 4 } else { 6 })).max(1);
    let rows: usize = lines.iter().map(|l| l.width().div_ceil(inner).max(1)).sum();
    let tall = rows + 4 > usize::from(screen.height.saturating_sub(2));
    let rect = centered(
        screen,
        wide,
        u16::try_from(rows + if compact || tall { 2 } else { 4 }).unwrap_or(u16::MAX),
    );
    backdrop(frame, rect);
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(TEXT).bg(PANEL))
            .block(
                block(title)
                    .padding(if compact {
                        Padding::new(1, 1, 0, 0)
                    } else if tall {
                        Padding::new(2, 2, 0, 0)
                    } else {
                        Padding::new(2, 2, 1, 1)
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
