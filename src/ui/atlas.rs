use std::{
    collections::{BTreeMap, HashSet},
    f64::consts::{PI, TAU},
    sync::{Mutex, OnceLock},
};

use ratatui::{buffer::Buffer, style::Modifier};
use serde::Deserialize;

use super::{
    AMBER, App, BACKGROUND, BLUE, Color, FAINT, Frame, Game, GameStatus, Line, MUTED, Rect, Span,
    Style, TEXT, UnicodeWidthStr, View, block, ellipsize, live_color, mix, pulse, state_color,
    state_symbol, text,
};
use crate::model::Coordinates;

const SHALLOW: Color = Color::Rgb(15, 30, 41);
const LAND: Color = Color::Rgb(27, 45, 51);
const COAST: Color = Color::Rgb(98, 142, 148);
const STATE: Color = Color::Rgb(50, 77, 85);
const GRATICULE: Color = Color::Rgb(30, 50, 62);
const RING: Color = Color::Rgb(58, 88, 98);

/// Mean Earth radius. Both projections work on the unit sphere.
const EARTH_MILES: f64 = 3958.8;

type Polyline = Vec<(f64, f64)>;

#[derive(Deserialize)]
struct Geography {
    land: Vec<(f64, f64)>,
    borders: Vec<Polyline>,
}

fn geography() -> &'static Geography {
    static MAP: OnceLock<Geography> = OnceLock::new();
    MAP.get_or_init(|| {
        serde_json::from_str(include_str!("../../assets/north-america.json"))
            .expect("bundled geography")
    })
}

#[derive(Deserialize)]
struct WorldSource {
    step: f64,
    coast: Vec<Polyline>,
    land: Vec<String>,
}

/// World coastlines and a coarse land mask for views beyond the contiguous US.
struct World {
    step: f64,
    columns: usize,
    coast: Vec<Polyline>,
    land: Vec<bool>,
}

fn world() -> &'static World {
    static WORLD: OnceLock<World> = OnceLock::new();
    WORLD.get_or_init(|| {
        let source: WorldSource =
            serde_json::from_str(include_str!("../../assets/world.json")).expect("bundled world");
        let columns = source.land.first().map_or(0, |row| row.len() * 4);
        let land = source
            .land
            .iter()
            .flat_map(|row| {
                row.chars().flat_map(|digit| {
                    let value = digit.to_digit(16).unwrap_or(0);
                    (0..4).rev().map(move |bit| value >> bit & 1 == 1)
                })
            })
            .collect();
        World {
            step: source.step,
            columns,
            coast: source.coast,
            land,
        }
    })
}

/// The North American samples: a 0.4° grid over 132°W–58.4°W and 12°N–57.6°N.
fn north_america() -> &'static Vec<bool> {
    static MASK: OnceLock<Vec<bool>> = OnceLock::new();
    MASK.get_or_init(|| {
        let mut bits = vec![false; 186 * 116];
        for &(lon, lat) in &geography().land {
            if let Some(index) = north_america_index(lon, lat) {
                bits[index] = true;
            }
        }
        bits
    })
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Values are rounded and range-checked before conversion"
)]
fn north_america_index(lon: f64, lat: f64) -> Option<usize> {
    let column = (lon / 0.4).round() + 330.0;
    let row = (lat / 0.4).round() - 30.0;
    ((0.0..186.0).contains(&column) && (0.0..116.0).contains(&row))
        .then(|| row as usize * 186 + column as usize)
}

/// Land test: the finer North American grid where it exists, else the world mask.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Indices are floored from in-range coordinates"
)]
fn is_land(lon: f64, lat: f64) -> bool {
    if let Some(index) = north_america_index(lon, lat) {
        return north_america()[index];
    }
    let world = world();
    let lon = (lon + 180.0).rem_euclid(360.0);
    let lat = (90.0 - lat).clamp(0.0, 179.999);
    let index = (lat / world.step) as usize * world.columns + (lon / world.step) as usize;
    world.land.get(index).copied().unwrap_or(false)
}

#[derive(Clone, Copy, PartialEq)]
enum Projection {
    /// Albers equal-area conic with the USGS parameters for the contiguous US.
    Albers,
    /// Azimuthal equidistant around a center: true distance and bearing from it.
    Azimuthal { lat: f64, lon: f64 },
}

struct AlbersConstants {
    n: f64,
    c: f64,
    rho0: f64,
}

fn albers() -> &'static AlbersConstants {
    static CONSTANTS: OnceLock<AlbersConstants> = OnceLock::new();
    CONSTANTS.get_or_init(|| {
        let (p1, p2, p0) = (
            29.5f64.to_radians(),
            45.5f64.to_radians(),
            37.5f64.to_radians(),
        );
        let n = f64::midpoint(p1.sin(), p2.sin());
        let c = p1.cos().powi(2) + 2.0 * n * p1.sin();
        let rho0 = (c - 2.0 * n * p0.sin()).sqrt() / n;
        AlbersConstants { n, c, rho0 }
    })
}

const CENTRAL_MERIDIAN: f64 = -96.0;

impl Projection {
    fn forward(self, lon: f64, lat: f64) -> Option<(f64, f64)> {
        match self {
            Self::Albers => {
                let k = albers();
                let theta = k.n * (lon - CENTRAL_MERIDIAN).to_radians();
                let rho = (k.c - 2.0 * k.n * lat.to_radians().sin()).max(0.0).sqrt() / k.n;
                Some((rho * theta.sin(), k.rho0 - rho * theta.cos()))
            }
            Self::Azimuthal {
                lat: lat0,
                lon: lon0,
            } => {
                let (phi, phi0) = (lat.to_radians(), lat0.to_radians());
                let delta = (lon - lon0).to_radians();
                let cosine = phi0.sin() * phi.sin() + phi0.cos() * phi.cos() * delta.cos();
                let c = cosine.clamp(-1.0, 1.0).acos();
                // Near the antipode every direction converges; leave it off the map.
                if c > PI * 0.94 {
                    return None;
                }
                let k = if c < 1e-9 { 1.0 } else { c / c.sin() };
                Some((
                    k * phi.cos() * delta.sin(),
                    k * (phi0.cos() * phi.sin() - phi0.sin() * phi.cos() * delta.cos()),
                ))
            }
        }
    }

    fn inverse(self, x: f64, y: f64) -> Option<(f64, f64)> {
        match self {
            Self::Albers => {
                let k = albers();
                let dy = k.rho0 - y;
                let rho = x.hypot(dy);
                let lon = CENTRAL_MERIDIAN + (x.atan2(dy) / k.n).to_degrees();
                let sine = ((k.c - (rho * k.n).powi(2)) / (2.0 * k.n)).clamp(-1.0, 1.0);
                Some((lon, sine.asin().to_degrees()))
            }
            Self::Azimuthal {
                lat: lat0,
                lon: lon0,
            } => {
                let c = x.hypot(y);
                if c > PI {
                    return None;
                }
                if c < 1e-9 {
                    return Some((lon0, lat0));
                }
                let phi0 = lat0.to_radians();
                let phi = (c.cos() * phi0.sin() + y * c.sin() * phi0.cos() / c)
                    .clamp(-1.0, 1.0)
                    .asin();
                let lambda =
                    (x * c.sin()).atan2(c * phi0.cos() * c.cos() - y * phi0.sin() * c.sin());
                let lon = (lon0 + lambda.to_degrees() + 540.0).rem_euclid(360.0) - 180.0;
                Some((lon, phi.to_degrees()))
            }
        }
    }
}

/// Maps projected coordinates to braille dots. Dots are square because a
/// terminal cell is about twice as tall as it is wide.
#[derive(Clone, Copy, PartialEq)]
struct Viewport {
    projection: Projection,
    left: f64,
    top: f64,
    units_per_dot: f64,
}

impl Viewport {
    fn fit(
        projection: Projection,
        (min_x, max_x, min_y, max_y): (f64, f64, f64, f64),
        columns: u16,
        rows: u16,
    ) -> Self {
        let (dots_x, dots_y) = (f64::from(columns) * 2.0, f64::from(rows) * 4.0);
        let units_per_dot = ((max_x - min_x) / dots_x).max((max_y - min_y) / dots_y);
        Self {
            projection,
            left: f64::midpoint(min_x, max_x) - units_per_dot * dots_x / 2.0,
            top: f64::midpoint(min_y, max_y) + units_per_dot * dots_y / 2.0,
            units_per_dot,
        }
    }

    /// The contiguous US with a small margin.
    fn conus(columns: u16, rows: u16) -> Self {
        let projection = Projection::Albers;
        let mut bounds = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for step in 0..=40 {
            let t = f64::from(step) / 40.0;
            for (lon, lat) in [
                (-124.6 + t * 57.6, 24.6),
                (-124.6 + t * 57.6, 49.2),
                (-124.6, 24.6 + t * 24.6),
                (-67.0, 24.6 + t * 24.6),
            ] {
                if let Some((x, y)) = projection.forward(lon, lat) {
                    bounds = (
                        bounds.0.min(x),
                        bounds.1.max(x),
                        bounds.2.min(y),
                        bounds.3.max(y),
                    );
                }
            }
        }
        let (pad_x, pad_y) = ((bounds.1 - bounds.0) * 0.02, (bounds.3 - bounds.2) * 0.04);
        Self::fit(
            projection,
            (
                bounds.0 - pad_x,
                bounds.1 + pad_x,
                bounds.2 - pad_y,
                bounds.3 + pad_y,
            ),
            columns,
            rows,
        )
    }

    /// A disc of `radius` (in Earth radii) around `center`.
    fn around(center: Coordinates, radius: f64, columns: u16, rows: u16) -> Self {
        Self::fit(
            Projection::Azimuthal {
                lat: center.latitude,
                lon: center.longitude,
            },
            (-radius, radius, -radius, radius),
            columns,
            rows,
        )
    }

    fn dot(self, lon: f64, lat: f64) -> Option<(f64, f64)> {
        let (x, y) = self.projection.forward(lon, lat)?;
        Some((
            (x - self.left) / self.units_per_dot,
            (self.top - y) / self.units_per_dot,
        ))
    }

    fn unproject(self, dx: f64, dy: f64) -> Option<(f64, f64)> {
        self.projection.inverse(
            self.left + dx * self.units_per_dot,
            self.top - dy * self.units_per_dot,
        )
    }

    fn miles_per_dot(self) -> f64 {
        self.units_per_dot * EARTH_MILES
    }
}

/// A braille dot raster. Each cell keeps its bits and the color of its
/// highest-priority ink.
struct Raster {
    columns: usize,
    rows: usize,
    bits: Vec<u8>,
    ink: Vec<(u8, Color)>,
}

const BRAILLE: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

impl Raster {
    fn new(columns: u16, rows: u16) -> Self {
        let (columns, rows) = (usize::from(columns), usize::from(rows));
        Self {
            columns,
            rows,
            bits: vec![0; columns * rows],
            ink: vec![(0, BACKGROUND); columns * rows],
        }
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Dot positions are floored and range-checked"
    )]
    fn cell(&self, x: f64, y: f64) -> Option<(usize, u8)> {
        if !(x >= 0.0 && y >= 0.0) {
            return None;
        }
        let (x, y) = (x as usize, y as usize);
        if x >= self.columns * 2 || y >= self.rows * 4 {
            return None;
        }
        Some(((y / 4) * self.columns + x / 2, BRAILLE[y % 4][x % 2]))
    }

    fn dot(&mut self, x: f64, y: f64, color: Color, priority: u8) {
        if let Some((cell, bit)) = self.cell(x, y) {
            self.bits[cell] |= bit;
            if priority >= self.ink[cell].0 {
                self.ink[cell] = (priority, color);
            }
        }
    }

    /// Plot a segment. `keep` filters dots by step index and cell.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Step counts are bounded before conversion"
    )]
    fn line(
        &mut self,
        from: (f64, f64),
        to: (f64, f64),
        color: Color,
        priority: u8,
        keep: impl Fn(usize, usize) -> bool,
    ) {
        let steps = (to.0 - from.0).abs().max((to.1 - from.1).abs()).ceil();
        if !steps.is_finite() || steps > 4000.0 {
            return;
        }
        let steps = (steps as usize).max(1);
        for i in 0..=steps {
            #[expect(clippy::cast_precision_loss, reason = "Step counts are small")]
            let progress = i as f64 / steps as f64;
            let x = from.0 + (to.0 - from.0) * progress;
            let y = from.1 + (to.1 - from.1) * progress;
            if let Some((cell, _)) = self.cell(x, y)
                && keep(i, cell)
            {
                self.dot(x, y, color, priority);
            }
        }
    }

    /// Plot a polyline of geographic points, breaking where the projection
    /// cannot place a point or a segment leaps across the map.
    fn path(
        &mut self,
        view: Viewport,
        points: &[(f64, f64)],
        color: Color,
        priority: u8,
        keep: impl Fn(usize, usize) -> bool + Copy,
    ) {
        let mut previous: Option<(f64, f64)> = None;
        for &(lon, lat) in points {
            let next = view.dot(lon, lat);
            if let (Some(a), Some(b)) = (previous, next)
                && (a.0 - b.0).abs() + (a.1 - b.1).abs() < 400.0
            {
                self.line(a, b, color, priority, keep);
            }
            previous = next;
        }
    }

    fn ring(
        &mut self,
        center: (f64, f64),
        radius: f64,
        color: Color,
        priority: u8,
        keep: impl Fn(usize) -> bool,
    ) {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "Ring circumferences are a few hundred dots"
        )]
        let count = (TAU * radius * 1.4).ceil().clamp(8.0, 4000.0) as usize;
        for i in (0..count).filter(|&i| keep(i)) {
            #[expect(clippy::cast_precision_loss, reason = "Counts are small")]
            let angle = i as f64 / count as f64 * TAU;
            self.dot(
                center.0 + radius * angle.cos(),
                center.1 + radius * angle.sin(),
                color,
                priority,
            );
        }
    }

    fn symbol(&self, cell: usize, ascii: bool) -> Option<char> {
        let bits = self.bits[cell];
        if bits == 0 {
            None
        } else if ascii {
            Some(if bits.count_ones() > 2 { ':' } else { '.' })
        } else {
            char::from_u32(0x2800 + u32::from(bits))
        }
    }
}

/// Fraction of each cell covered by land, sampled at the braille dot centers.
fn coverage(view: Viewport, columns: u16, rows: u16) -> Vec<f64> {
    type Cached = Option<((u16, u16), [u64; 5], Vec<f64>)>;
    static CACHE: Mutex<Cached> = Mutex::new(None);
    let (center_lat, center_lon) = match view.projection {
        Projection::Albers => (f64::NAN, f64::NAN),
        Projection::Azimuthal { lat, lon } => (lat, lon),
    };
    let key = [
        view.left.to_bits(),
        view.top.to_bits(),
        view.units_per_dot.to_bits(),
        center_lat.to_bits(),
        center_lon.to_bits(),
    ];
    let mut cache = CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((size, cached, values)) = cache.as_ref()
        && *size == (columns, rows)
        && *cached == key
    {
        return values.clone();
    }
    let mut values = Vec::with_capacity(usize::from(columns) * usize::from(rows));
    for row in 0..rows {
        for column in 0..columns {
            let mut land = 0u8;
            for (sx, sy) in (0..2).flat_map(|x| (0..4).map(move |y| (x, y))) {
                let dx = f64::from(column) * 2.0 + f64::from(sx) + 0.5;
                let dy = f64::from(row) * 4.0 + f64::from(sy) + 0.5;
                if let Some((lon, lat)) = view.unproject(dx, dy)
                    && is_land(lon, lat)
                {
                    land += 1;
                }
            }
            values.push(f64::from(land) / 8.0);
        }
    }
    *cache = Some(((columns, rows), key, values.clone()));
    values
}

/// What the map shows and how much detail the panel can afford.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// The contiguous US in Albers.
    Nation,
    /// Distance rings around the chosen Nearby place.
    Nearby,
    /// A wider disc around the slate's ballparks, for games beyond the US.
    Wide,
    /// A close-up inset around one ballpark.
    Locator,
}

fn legend(app: &App, wide: bool, exceptional: bool, mode: Mode) -> Line<'static> {
    let mut spans = vec![
        Span::raw(" "),
        text(state_symbol(GameStatus::Live, app.ascii), live_color(app)),
        text(" Live  ", MUTED),
        text(state_symbol(GameStatus::Scheduled, app.ascii), BLUE),
        text(" Upcoming  ", MUTED),
    ];
    if wide {
        spans.push(text(state_symbol(GameStatus::Final, app.ascii), MUTED));
        spans.push(text(" Final  ", MUTED));
    }
    spans.push(text(if app.ascii { "@" } else { "◆" }, AMBER));
    spans.push(text(" Selected ", MUTED));
    if wide && mode == Mode::Nearby {
        spans.push(text(if app.ascii { " H" } else { " ⌂" }, TEXT));
        spans.push(text(" You ", MUTED));
    }
    if exceptional && wide {
        spans.push(text(" !", AMBER));
        spans.push(text(" Delayed/PPD ", MUTED));
    }
    Line::from(spans)
}

/// A round number of miles for range rings or a scale bar.
fn nice_miles(target: f64) -> f64 {
    [
        10.0, 25.0, 50.0, 100.0, 200.0, 250.0, 500.0, 1000.0, 2000.0, 2500.0, 5000.0,
    ]
    .into_iter()
    .find(|&m| m >= target)
    .unwrap_or(5000.0)
}

/// Spherical mean of a set of points.
fn centroid(points: &[Coordinates]) -> Option<Coordinates> {
    if points.is_empty() {
        return None;
    }
    let (mut x, mut y, mut z) = (0.0, 0.0, 0.0);
    for p in points {
        let (phi, lambda) = (p.latitude.to_radians(), p.longitude.to_radians());
        x += phi.cos() * lambda.cos();
        y += phi.cos() * lambda.sin();
        z += phi.sin();
    }
    Coordinates::new(z.atan2(x.hypot(y)).to_degrees(), y.atan2(x).to_degrees())
}

struct Label {
    x: u16,
    y: u16,
    text: String,
    fallback: Option<String>,
    color: Color,
    rank: u8,
}

#[expect(clippy::too_many_lines, reason = "Keep the map pipeline together")]
pub(super) fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let games = app.visible_games();
    let wide = area.width >= 52;
    let exceptional = games.iter().any(|g| {
        !matches!(
            g.status,
            GameStatus::Live | GameStatus::Scheduled | GameStatus::Final
        )
    });
    let unmapped = games
        .iter()
        .filter(|g| g.venue.coordinates.is_none())
        .count();
    let mut venues: BTreeMap<crate::model::VenueKey, Vec<&Game>> = BTreeMap::new();
    for game in &games {
        if game.venue.coordinates.is_some() {
            venues.entry(game.venue_key()).or_default().push(game);
        }
    }
    let inner = block("").inner(area);
    if inner.width < 8 || inner.height < 3 {
        frame.render_widget(block("BALLPARK ATLAS"), area);
        return;
    }
    let plot = inner;
    let selected = app.selected_game().and_then(|g| g.venue.coordinates);
    let place = app
        .preferences
        .place
        .as_ref()
        .filter(|_| app.view == View::Nearby)
        .map(|p| p.coordinates);
    let positions: Vec<Coordinates> = games.iter().filter_map(|g| g.venue.coordinates).collect();
    let outside = positions
        .iter()
        .any(|p| !(-128.0..=-65.0).contains(&p.longitude) || !(22.0..=52.0).contains(&p.latitude));
    let (mode, view) = if let Some(center) = place {
        // Frame the nearest handful of ballparks, and always the selected one.
        let mut distances: Vec<f64> = positions.iter().map(|p| center.miles_to(*p)).collect();
        distances.sort_by(f64::total_cmp);
        let near = distances
            .get(distances.len().min(8).saturating_sub(1))
            .copied()
            .unwrap_or(250.0);
        let reach = selected
            .map_or(near, |s| near.max(center.miles_to(s)))
            .max(120.0)
            * 1.12;
        (
            Mode::Nearby,
            Viewport::around(center, reach / EARTH_MILES, plot.width, plot.height),
        )
    } else if outside && let Some(center) = centroid(&positions) {
        let reach = positions
            .iter()
            .map(|p| center.miles_to(*p))
            .fold(300.0, f64::max)
            * 1.12;
        (
            Mode::Wide,
            Viewport::around(
                center,
                (reach / EARTH_MILES).min(PI * 0.85),
                plot.width,
                plot.height,
            ),
        )
    } else {
        (Mode::Nation, Viewport::conus(plot.width, plot.height))
    };

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Checked against the plot before conversion"
    )]
    let cell_of = |p: Coordinates| -> Option<(u16, u16)> {
        let (x, y) = view.dot(p.longitude, p.latitude)?;
        let (column, row) = ((x / 2.0).floor(), (y / 4.0).floor());
        let inside = (0.0..f64::from(plot.width)).contains(&column)
            && (0.0..f64::from(plot.height)).contains(&row);
        inside.then(|| (plot.x + column as u16, plot.y + row as u16))
    };
    let off_map = venues
        .values()
        .filter(|group| group[0].venue.coordinates.and_then(cell_of).is_none())
        .count();
    let caption = if unmapped > 0 {
        format!(" {unmapped} unmapped ")
    } else if off_map > 0 {
        format!(" +{off_map} beyond view ")
    } else if mode == Mode::Nearby {
        " straight-line miles ".into()
    } else {
        format!(
            " {} {} ",
            venues.len(),
            if venues.len() == 1 {
                "ballpark"
            } else {
                "ballparks"
            }
        )
    };
    let legend = legend(app, wide, exceptional, mode);
    let caption = if legend.width() + caption.width() + 4 > usize::from(area.width) {
        String::new()
    } else {
        caption
    };
    frame.render_widget(
        block("BALLPARK ATLAS").title_bottom(legend).title_bottom(
            Line::from(vec![text(caption, FAINT), text("─", super::BORDER)]).right_aligned(),
        ),
        area,
    );

    let buffer = frame.buffer_mut();
    let rings = paint(
        buffer,
        plot,
        view,
        mode,
        app,
        selected.and_then(|s| view.dot(s.longitude, s.latitude)),
    );

    // Markers reserve their neighbors so no label can touch a ballpark.
    let mut occupied: Vec<Rect> = Vec::new();
    let mut markers: HashSet<(u16, u16)> = HashSet::new();
    let mut labels: Vec<Label> = Vec::new();
    let small = plot.width < 50;
    let mut pins: Vec<&Vec<&Game>> = venues.values().collect();
    pins.sort_by_key(|group| group.iter().any(|g| Some(g.id) == app.selected));
    let mut names: BTreeMap<String, usize> = BTreeMap::new();
    for group in &pins {
        *names.entry(group[0].home.abbreviation.clone()).or_default() += 1;
    }
    for group in pins {
        let focused_game = group.iter().find(|g| Some(g.id) == app.selected).copied();
        let game = focused_game.unwrap_or(group[0]);
        let Some((x, y)) = game.venue.coordinates.and_then(cell_of) else {
            continue;
        };
        let focused = focused_game.is_some();
        let live = group.iter().find(|g| g.status == GameStatus::Live).copied();
        let shown = live.unwrap_or(game);
        let color = if focused {
            AMBER
        } else if live.is_some() {
            live_color(app)
        } else {
            state_color(game)
        };
        let marker = if focused {
            if app.ascii { "@" } else { "◆" }
        } else {
            state_symbol(shown.status, app.ascii)
        };
        buffer[(x, y)]
            .set_symbol(marker)
            .set_style(Style::default().fg(color).add_modifier(Modifier::BOLD));
        markers.insert((x, y));
        occupied.push(Rect::new(x.saturating_sub(1), y, 3, 1));
        if small && !focused && live.is_none() {
            continue;
        }
        // Two clubs can share an abbreviation across leagues; name the city instead.
        let name = if names.get(&game.home.abbreviation).copied().unwrap_or(0) > 1 {
            ellipsize(&game.venue.city, 12)
        } else {
            game.home.abbreviation.clone()
        };
        let name = if group.len() > 1 {
            format!("{name} ×{}", group.len())
        } else {
            name
        };
        let (label, fallback) = if focused && !small {
            (format!("{name}  {}", game.venue.name), Some(name))
        } else {
            (name, None)
        };
        let color = if focused {
            AMBER
        } else if game.status == GameStatus::Final && live.is_none() {
            FAINT
        } else {
            mix(color, BACKGROUND, 0.12)
        };
        let rank = match (focused, live.is_some()) {
            (true, _) => 3,
            (false, true) => 1,
            (false, false) => 0,
        };
        labels.push(Label {
            x,
            y,
            text: label,
            fallback,
            color,
            rank,
        });
    }
    // The place marker stays, but its label yields to a ballpark in the same city.
    let beside_selection = place
        .zip(selected)
        .is_some_and(|(p, s)| p.miles_to(s) < 30.0);
    if let Some((x, y)) = place.and_then(cell_of) {
        buffer[(x, y)]
            .set_symbol(if app.ascii { "H" } else { "⌂" })
            .set_style(Style::default().fg(TEXT).add_modifier(Modifier::BOLD));
        markers.insert((x, y));
        occupied.push(Rect::new(x.saturating_sub(1), y, 3, 1));
        let name = app
            .preferences
            .place
            .as_ref()
            .map_or("You".into(), |p| ellipsize(&p.name, 18));
        if !beside_selection {
            labels.push(Label {
                x,
                y,
                text: name,
                fallback: Some("You".into()),
                color: TEXT,
                rank: 2,
            });
        }
    }
    labels.sort_by_key(|label| std::cmp::Reverse(label.rank));
    for (text, x, y) in rings {
        place_fixed(buffer, plot, &mut occupied, &markers, &text, x, y, FAINT);
    }
    for label in labels {
        let candidates = std::iter::once(label.text.clone()).chain(label.fallback.clone());
        'placed: for text in candidates {
            let Ok(width) = u16::try_from(text.width()) else {
                continue;
            };
            let w = i32::from(width);
            for (dx, dy) in [
                (2, 0),
                (-w - 1, 0),
                (1, -1),
                (1, 1),
                (-w, -1),
                (-w, 1),
                (-w / 2, -1),
                (-w / 2, 1),
            ] {
                let (Ok(lx), Ok(ly)) = (
                    u16::try_from(i32::from(label.x) + dx),
                    u16::try_from(i32::from(label.y) + dy),
                ) else {
                    continue;
                };
                let rect = Rect::new(lx, ly, width, 1);
                if lx < plot.x
                    || ly < plot.y
                    || rect.right() > plot.right()
                    || rect.bottom() > plot.bottom()
                    || occupied.iter().any(|r| r.intersects(rect))
                {
                    continue;
                }
                knockout(buffer, plot, &markers, rect);
                let mut style = Style::default().fg(label.color);
                if label.rank > 1 {
                    style = style.add_modifier(Modifier::BOLD);
                }
                buffer.set_string(lx, ly, &text, style);
                // Two columns of clearance keep neighboring labels from reading as one.
                occupied.push(Rect::new(lx.saturating_sub(2), ly, width + 4, 1));
                break 'placed;
            }
        }
    }
}

/// Clear map linework under and beside a label so it reads cleanly.
fn knockout(buffer: &mut Buffer, plot: Rect, markers: &HashSet<(u16, u16)>, rect: Rect) {
    let left = rect.x.saturating_sub(1).max(plot.x);
    let right = (rect.right() + 1).min(plot.right());
    for x in left..right {
        if !markers.contains(&(x, rect.y)) {
            buffer[(x, rect.y)].set_char(' ');
        }
    }
}

#[expect(clippy::too_many_arguments, reason = "A small internal drawing helper")]
fn place_fixed(
    buffer: &mut Buffer,
    plot: Rect,
    occupied: &mut Vec<Rect>,
    markers: &HashSet<(u16, u16)>,
    text: &str,
    x: u16,
    y: u16,
    color: Color,
) {
    let Ok(width) = u16::try_from(text.width()) else {
        return;
    };
    let x = x.saturating_sub(width / 2).max(plot.x);
    let rect = Rect::new(x, y, width, 1);
    if y < plot.y
        || rect.right() > plot.right()
        || rect.bottom() > plot.bottom()
        || occupied.iter().any(|r| r.intersects(rect))
    {
        return;
    }
    knockout(buffer, plot, markers, rect);
    buffer.set_string(x, y, text, Style::default().fg(color));
    occupied.push(rect);
}

/// Land, water, linework, and selection effects. Returns range-ring labels
/// as (text, center x, row) for the label pass.
#[expect(clippy::too_many_lines, reason = "Keep the map layers together")]
fn paint(
    buffer: &mut Buffer,
    plot: Rect,
    view: Viewport,
    mode: Mode,
    app: &App,
    selected: Option<(f64, f64)>,
) -> Vec<(String, u16, u16)> {
    let (columns, rows) = (usize::from(plot.width), usize::from(plot.height));
    let land = coverage(view, plot.width, plot.height);
    let near_land = |cell: usize| {
        let (column, row) = (cell % columns, cell / columns);
        (row.saturating_sub(1)..=(row + 1).min(rows - 1)).any(|r| {
            (column.saturating_sub(1)..=(column + 1).min(columns - 1))
                .any(|c| land[r * columns + c] > 0.0)
        })
    };
    let mut raster = Raster::new(plot.width, plot.height);
    let water = |cell: usize| land[cell] == 0.0;
    // Detail scales with the panel: small maps keep only coasts and markers.
    let show_graticule =
        matches!(mode, Mode::Nation | Mode::Wide) && plot.width >= 90 && !app.ascii;
    let show_states =
        plot.width >= 70 || matches!(mode, Mode::Nearby | Mode::Locator) && plot.width >= 30;

    if show_graticule {
        let (lats, lons) = match mode {
            Mode::Nation => ((10..=75).step_by(5), (-170..=-40).step_by(10)),
            _ => ((-80..=80).step_by(10), (-180..=170).step_by(10)),
        };
        for lat in lats {
            let line: Polyline = (0..=720)
                .map(|i| (-180.0 + f64::from(i) * 0.5, f64::from(lat)))
                .collect();
            raster.path(view, &line, GRATICULE, 1, |i, cell| {
                i % 3 == 0 && water(cell)
            });
        }
        for lon in lons {
            let line: Polyline = (-160..=160)
                .map(|i| (f64::from(lon), f64::from(i) * 0.5))
                .collect();
            raster.path(view, &line, GRATICULE, 1, |i, cell| {
                i % 3 == 0 && water(cell)
            });
        }
    }

    // Range rings: true distance from the Nearby place.
    let mut ring_labels = Vec::new();
    if mode == Mode::Nearby {
        let radius_dots = f64::from(plot.width.min(plot.height * 2));
        let span = radius_dots * view.miles_per_dot();
        let step = nice_miles(span / 3.2);
        // The place sits at the center of the plot.
        let center = (f64::from(plot.width), f64::from(plot.height) * 2.0);
        let mut miles = step;
        while miles < span * 1.6 {
            let radius = miles / view.miles_per_dot();
            raster.ring(center, radius, RING, 2, |i| i % 2 == 0);
            let top = (center.1 - radius) / 4.0;
            if top >= 0.0 {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "The ring top is inside the plot"
                )]
                let row = top as u16;
                ring_labels.push((
                    format!("{miles:.0} mi"),
                    plot.x + plot.width / 2,
                    plot.y + row,
                ));
            }
            miles += step;
        }
    }

    let dotted = |i: usize, _: usize| i.is_multiple_of(2);
    let solid = |_: usize, _: usize| true;
    match mode {
        Mode::Nation => {
            for line in &geography().borders {
                let closed = line.len() > 2 && line.first() == line.last();
                if closed {
                    raster.path(view, line, COAST, 3, solid);
                } else if show_states {
                    raster.path(view, line, STATE, 2, dotted);
                }
            }
        }
        Mode::Nearby | Mode::Wide | Mode::Locator => {
            for ring in &world().coast {
                raster.path(view, ring, COAST, 3, solid);
            }
            if show_states && view.miles_per_dot() < 40.0 {
                for line in &geography().borders {
                    if line.first() != line.last() {
                        raster.path(view, line, STATE, 2, dotted);
                    }
                }
            }
        }
    }

    // Nearby: a marching dashed route from the place, which is the map center.
    if mode == Mode::Nearby
        && let Some(to) = selected
    {
        let from = (f64::from(plot.width), f64::from(plot.height) * 2.0);
        let phase = usize::try_from(app.tick % 4).unwrap_or(0);
        let color = mix(AMBER, LAND, 0.25);
        raster.line(from, to, color, 4, |i, _| (i + 4 - phase) % 4 < 2);
    }

    // The selection glow and sonar ping scale with the map.
    let reach = (f64::from(plot.width) * 0.25).clamp(6.0, 18.0);
    if let Some(center) = selected
        && !app.ascii
    {
        for offset in [0, 11] {
            #[expect(clippy::cast_precision_loss, reason = "Ticks are small modulo 22")]
            let phase = ((app.tick + offset) % 22) as f64 / 22.0;
            let radius = 1.5 + phase * (reach * 0.7 - 1.5);
            let color = mix(AMBER, LAND, phase.powf(0.7));
            raster.ring(center, radius, color, 5, |_| true);
        }
    }
    let glow = |column: usize, row: usize| -> f64 {
        let Some((sx, sy)) = selected else {
            return 0.0;
        };
        #[expect(clippy::cast_precision_loss, reason = "Cell indices are small")]
        let (cx, cy) = (column as f64 * 2.0 + 1.0, row as f64 * 4.0 + 2.0);
        let distance = (cx - sx).hypot(cy - sy);
        let breathe = 0.85 + 0.15 * pulse(app.tick, 22);
        ((1.0 - distance / reach).max(0.0)).powi(2) * 0.24 * breathe
    };

    for row in 0..rows {
        for column in 0..columns {
            let cell = row * columns + column;
            let base = if land[cell] > 0.0 {
                mix(BACKGROUND, LAND, 0.35 + 0.65 * land[cell])
            } else if near_land(cell) {
                SHALLOW
            } else {
                BACKGROUND
            };
            let (Ok(x), Ok(y)) = (u16::try_from(column), u16::try_from(row)) else {
                continue;
            };
            let target = &mut buffer[(plot.x + x, plot.y + y)];
            target.set_bg(mix(base, AMBER, glow(column, row)));
            if let Some(symbol) = raster.symbol(cell, app.ascii) {
                target.set_char(symbol).set_fg(raster.ink[cell].1);
            } else {
                target.set_char(' ');
            }
        }
    }
    if mode != Mode::Nearby {
        scale_bar(buffer, plot, view);
    }
    ring_labels
}

/// A cartographic scale bar in the lower-left corner, on a cleared strip.
fn scale_bar(buffer: &mut Buffer, plot: Rect, view: Viewport) {
    let miles_per_column = view.miles_per_dot() * 2.0;
    let budget = f64::from(plot.width) / 7.0;
    let miles = nice_miles(miles_per_column * budget * 0.6);
    if plot.height < 6 || plot.width < 40 {
        return;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "The bar is a few cells long"
    )]
    let length = (miles / miles_per_column).round() as usize;
    if !(4..=usize::from(plot.width) / 4).contains(&length) {
        return;
    }
    let bar = format!(" ├{}┤ {miles:.0} mi ", "─".repeat(length.saturating_sub(2)));
    let y = plot.bottom() - 1;
    buffer.set_string(plot.x + 1, y, bar, Style::default().fg(FAINT));
}

/// A close-up map of one ballpark for the game view.
pub(super) fn locator(frame: &mut Frame, area: Rect, app: &App, position: Coordinates, city: &str) {
    let panel = block("WHERE");
    let plot = panel.inner(area);
    frame.render_widget(panel, area);
    if plot.width < 8 || plot.height < 3 {
        return;
    }
    let view = Viewport::around(position, 280.0 / EARTH_MILES, plot.width, plot.height);
    let center = view.dot(position.longitude, position.latitude);
    let buffer = frame.buffer_mut();
    paint(buffer, plot, view, Mode::Locator, app, center);
    let (x, y) = (plot.x + plot.width / 2, plot.y + plot.height / 2);
    buffer[(x, y)]
        .set_symbol(if app.ascii { "@" } else { "◆" })
        .set_style(Style::default().fg(AMBER).add_modifier(Modifier::BOLD));
    let markers = HashSet::from([(x, y)]);
    let label = ellipsize(city, plot.width / 2);
    let Ok(width) = u16::try_from(label.width()) else {
        return;
    };
    if x + 2 + width <= plot.right() {
        let rect = Rect::new(x + 2, y, width, 1);
        knockout(buffer, plot, &markers, rect);
        buffer.set_string(
            rect.x,
            y,
            label,
            Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
        );
    }
}
