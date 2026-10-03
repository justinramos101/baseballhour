use std::{collections::BTreeMap, sync::OnceLock};

use ratatui::{
    symbols::Marker,
    widgets::canvas::{Canvas, Circle, Line as MapLine, Map, MapResolution, Points},
};
use serde::Deserialize;

use super::*;

#[derive(Deserialize)]
struct Geography {
    land: Vec<(f64, f64)>,
    borders: Vec<Vec<(f64, f64)>>,
}

fn geography() -> &'static Geography {
    static MAP: OnceLock<Geography> = OnceLock::new();
    MAP.get_or_init(|| {
        serde_json::from_str(include_str!("../../assets/north-america.json"))
            .expect("bundled geography")
    })
}

pub(super) fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let panel = block("BALLPARK ATLAS");
    let inside = panel.inner(area);
    frame.render_widget(panel, area);
    if inside.width < 8 || inside.height < 3 {
        return;
    }
    let plot = Rect::new(
        inside.x + 1,
        inside.y,
        inside.width.saturating_sub(2),
        inside.height.saturating_sub(2),
    );
    let games = app.visible_games();
    let international = games
        .iter()
        .filter_map(|g| g.venue.coordinates)
        .any(|p| !(-128.0..=-65.0).contains(&p.longitude) || !(22.0..=52.0).contains(&p.latitude));
    let (xb, yb) = if international {
        ([-180., 180.], [-90., 90.])
    } else {
        ([-128., -65.], [22., 52.])
    };
    let world = geography();
    let selected = app.selected_game().and_then(|g| g.venue.coordinates);
    let canvas = Canvas::default()
        .background_color(BACKGROUND)
        .x_bounds(xb)
        .y_bounds(yb)
        .marker(if app.ascii {
            Marker::Dot
        } else {
            Marker::Braille
        })
        .paint(|ctx| {
            if international {
                ctx.draw(&Map {
                    color: Color::Rgb(65, 101, 110),
                    resolution: MapResolution::High,
                });
            } else {
                ctx.draw(&Points {
                    coords: &world.land,
                    color: Color::Rgb(23, 42, 49),
                });
                ctx.layer();
                for boundary in &world.borders {
                    for segment in boundary.windows(2) {
                        ctx.draw(&MapLine {
                            x1: segment[0].0,
                            y1: segment[0].1,
                            x2: segment[1].0,
                            y2: segment[1].1,
                            color: Color::Rgb(61, 97, 105),
                        });
                    }
                }
            }
            ctx.layer();
            if let Some(position) = selected {
                ctx.draw(&Circle {
                    x: position.longitude,
                    y: position.latitude,
                    radius: if international { 3.0 } else { 1.1 },
                    color: AMBER,
                });
            }
        });
    frame.render_widget(canvas, plot);
    let mut venues: BTreeMap<crate::model::VenueKey, Vec<&Game>> = BTreeMap::new();
    for game in &games {
        if game.venue.coordinates.is_some() {
            venues.entry(game.venue_key()).or_default().push(game);
        }
    }
    let mut pins: Vec<_> = venues.values().collect();
    pins.sort_by_key(|group| !group.iter().any(|g| Some(g.id) == app.selected));
    let mut occupied = Vec::new();
    let mut labels = Vec::new();
    for group in pins.iter().rev() {
        let game = group
            .iter()
            .find(|g| Some(g.id) == app.selected)
            .copied()
            .unwrap_or(group[0]);
        let p = game.venue.coordinates.unwrap();
        let x = plot.x
            + ((p.longitude - xb[0]) / (xb[1] - xb[0]) * f64::from(plot.width.saturating_sub(1)))
                .round() as u16;
        let y = plot.y
            + ((yb[1] - p.latitude) / (yb[1] - yb[0]) * f64::from(plot.height.saturating_sub(1)))
                .round() as u16;
        if x >= plot.right() || y >= plot.bottom() {
            continue;
        }
        let focused = Some(game.id) == app.selected;
        let color = if focused { AMBER } else { state_color(game) };
        let marker = if focused {
            if app.ascii { "@" } else { "◆" }
        } else {
            state_symbol(game, app.ascii)
        };
        frame.render_widget(Paragraph::new(bold(marker, color)), Rect::new(x, y, 1, 1));
        occupied.push(Rect::new(x, y, 1, 1));
        if plot.width < 50 && !focused {
            continue;
        }
        let label = if group.len() > 1 {
            format!("{} +{}", game.home.abbreviation, group.len() - 1)
        } else {
            game.home.abbreviation.clone()
        };
        labels.push((x, y, label, color));
    }
    for (x, y, label, color) in labels.into_iter().rev() {
        let width = label.width() as u16;
        for (dx, dy) in [(2, 0), (-(i32::from(width)) - 1, 0), (1, -1), (1, 1)] {
            let lx = i32::from(x) + dx;
            let ly = i32::from(y) + dy;
            if lx < i32::from(plot.x) || ly < i32::from(plot.y) {
                continue;
            }
            let rect = Rect::new(lx as u16, ly as u16, width, 1);
            if rect.right() > plot.right()
                || rect.bottom() > plot.bottom()
                || occupied.iter().any(|r: &Rect| r.intersects(rect))
            {
                continue;
            }
            frame.render_widget(
                Paragraph::new(bold(label.clone(), color)).style(Style::default().bg(BACKGROUND)),
                rect,
            );
            occupied.push(Rect::new(
                rect.x.saturating_sub(1),
                rect.y,
                rect.width + 2,
                1,
            ));
            break;
        }
    }
    let exceptional = games.iter().any(|g| {
        !matches!(
            g.status,
            GameStatus::Live | GameStatus::Scheduled | GameStatus::Final
        )
    });
    let mut legend = if inside.width < 50 {
        Line::from(vec![
            text(" ", MUTED),
            text(if app.ascii { "* Live  " } else { "● Live  " }, GREEN),
            text(if app.ascii { "o Next  " } else { "○ Next  " }, BLUE),
            text(
                if app.ascii {
                    "@ Selected"
                } else {
                    "◆ Selected"
                },
                AMBER,
            ),
        ])
    } else {
        Line::from(vec![
            text("  ", MUTED),
            text(
                if app.ascii {
                    "* Live   "
                } else {
                    "● Live   "
                },
                GREEN,
            ),
            text(
                if app.ascii {
                    "o Upcoming   "
                } else {
                    "○ Upcoming   "
                },
                BLUE,
            ),
            text("· Final   ", MUTED),
            text(
                if app.ascii {
                    "@ Selected"
                } else {
                    "◆ Selected"
                },
                AMBER,
            ),
        ])
    };
    if exceptional && inside.width >= 50 {
        legend.spans.push(text("   ! Status alert", AMBER));
    }
    frame.render_widget(
        Paragraph::new(legend),
        Rect::new(inside.x, inside.bottom().saturating_sub(2), inside.width, 1),
    );
    let unmapped = games
        .iter()
        .filter(|g| g.venue.coordinates.is_none())
        .count();
    let caption = if inside.width < 50 {
        if exceptional {
            " ! See game status · ↑↓ Select".into()
        } else if unmapped > 0 {
            format!(" {unmapped} games unmapped")
        } else {
            " ↑↓ Select a ballpark".into()
        }
    } else if unmapped > 0 {
        format!("  {unmapped} games have no map location. Select them in the slate.")
    } else if app.view == View::Nearby {
        "  Distances are straight-line estimates, not driving routes.".into()
    } else {
        "  Select a game to find its ballpark.".into()
    };
    frame.render_widget(
        Paragraph::new(text(caption, MUTED)),
        Rect::new(inside.x, inside.bottom().saturating_sub(1), inside.width, 1),
    );
}
