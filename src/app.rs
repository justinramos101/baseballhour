use std::{collections::BTreeSet, fs, path::Path};

use chrono::{DateTime, FixedOffset, Local, NaiveDate, Utc};
use chrono_tz::Tz;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};

use crate::{
    model::{Coordinates, Game, League, Place, Query, Snapshot, parse_date, shift_date},
    storage,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    All,
    Following,
    Nearby,
}

#[derive(Clone, Debug, Default)]
pub enum Input {
    #[default]
    Normal,
    Search,
    Date {
        value: String,
        error: Option<String>,
    },
    Place {
        value: String,
        error: Option<String>,
    },
    Favorite {
        index: usize,
    },
    League {
        index: usize,
    },
    Help,
    Details,
}

#[derive(Clone, Debug)]
pub enum DataState {
    Loading,
    Fresh,
    Cached(String),
    Failed(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Action {
    #[default]
    None,
    Request,
    Save,
    Quit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub version: u8,
    pub favorites: BTreeSet<u32>,
    pub place: Option<Place>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            favorites: BTreeSet::new(),
            place: None,
        }
    }
}

pub struct App {
    pub query: Query,
    pub snapshot: Option<Snapshot>,
    pub state: DataState,
    pub selected: Option<u64>,
    pub search: String,
    pub view: View,
    pub input: Input,
    pub preferences: Preferences,
    pub now: DateTime<Utc>,
    pub timezone: Option<Tz>,
    pub demo: bool,
    pub ascii: bool,
    pub message: Option<String>,
}

impl App {
    pub fn new(query: Query, preferences: Preferences, timezone: Option<Tz>) -> Self {
        Self {
            query,
            snapshot: None,
            state: DataState::Loading,
            selected: None,
            search: String::new(),
            view: View::All,
            input: Input::Normal,
            preferences,
            now: Utc::now(),
            timezone,
            demo: false,
            ascii: false,
            message: None,
        }
    }

    pub fn games(&self) -> &[Game] {
        self.snapshot.as_ref().map_or(&[], |s| s.games.as_slice())
    }

    pub fn visible_games(&self) -> Vec<&Game> {
        let mut games: Vec<_> = self
            .games()
            .iter()
            .filter(|g| {
                g.matches(&self.search)
                    && match self.view {
                        View::All => true,
                        View::Following => self.is_favorite(g),
                        View::Nearby => {
                            self.preferences.place.is_some() && g.venue.coordinates.is_some()
                        }
                    }
            })
            .collect();
        games.sort_by(|a, b| {
            if self.view == View::Nearby {
                let da = self.distance(a).unwrap_or(f64::INFINITY);
                let db = self.distance(b).unwrap_or(f64::INFINITY);
                da.total_cmp(&db)
                    .then(a.starts_at.cmp(&b.starts_at))
                    .then(a.id.cmp(&b.id))
            } else {
                a.status
                    .rank()
                    .cmp(&b.status.rank())
                    .then(a.starts_at.is_none().cmp(&b.starts_at.is_none()))
                    .then(a.starts_at.cmp(&b.starts_at))
                    .then(a.id.cmp(&b.id))
            }
        });
        games
    }

    pub fn is_favorite(&self, game: &Game) -> bool {
        self.preferences.favorites.contains(&game.away.id)
            || self.preferences.favorites.contains(&game.home.id)
    }

    pub fn distance(&self, game: &Game) -> Option<f64> {
        Some(
            self.preferences
                .place
                .as_ref()?
                .coordinates
                .miles_to(game.venue.coordinates?),
        )
    }

    pub fn selected_game(&self) -> Option<&Game> {
        self.games().iter().find(|g| Some(g.id) == self.selected)
    }

    pub fn normalize_selection(&mut self) {
        let games = self.visible_games();
        if !games.iter().any(|g| Some(g.id) == self.selected) {
            self.selected = games.first().map(|g| g.id);
        }
    }

    pub fn select(&mut self, delta: isize) {
        let games = self.visible_games();
        if !games.is_empty() {
            let index = games
                .iter()
                .position(|g| Some(g.id) == self.selected)
                .unwrap_or(0);
            let next = index.saturating_add_signed(delta).min(games.len() - 1);
            self.selected = Some(games[next].id);
        }
    }

    pub fn accept(&mut self, snapshot: Snapshot, state: DataState) -> bool {
        if snapshot.query != self.query {
            return false;
        }
        self.snapshot = Some(snapshot);
        self.state = state;
        self.normalize_selection();
        true
    }

    pub fn change_date(&mut self, date: NaiveDate) {
        self.query.date = date;
        self.snapshot = None;
        self.selected = None;
        self.state = DataState::Loading;
    }

    pub fn local_time(&self, utc: DateTime<Utc>) -> DateTime<FixedOffset> {
        match self.timezone {
            Some(tz) => utc.with_timezone(&tz).fixed_offset(),
            None => utc.with_timezone(&Local).fixed_offset(),
        }
    }

    pub fn zone_label(&self) -> String {
        self.timezone
            .map_or_else(|| "Local time".into(), |tz| tz.name().into())
    }

    pub fn start_label(&self, game: &Game) -> String {
        let Some(start) = game.starts_at else {
            return "Time TBD".into();
        };
        let local = self.local_time(start);
        let delta = local
            .date_naive()
            .signed_duration_since(self.query.date)
            .num_days();
        let time = local.format("%-I:%M %p").to_string();
        if delta == 0 {
            time
        } else {
            format!("{time} {delta:+}d")
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Action::Quit;
        }
        if key.code == KeyCode::Char('q')
            && !matches!(
                self.input,
                Input::Search | Input::Date { .. } | Input::Place { .. }
            )
        {
            return Action::Quit;
        }
        let mode = std::mem::take(&mut self.input);
        match mode {
            Input::Search => {
                match key.code {
                    KeyCode::Esc => self.search.clear(),
                    KeyCode::Enter => {}
                    KeyCode::Backspace => {
                        self.search.pop();
                        self.input = Input::Search;
                    }
                    KeyCode::Char(c) if self.search.chars().count() < 80 && !c.is_control() => {
                        self.search.push(c);
                        self.input = Input::Search;
                    }
                    _ => self.input = Input::Search,
                }
                self.normalize_selection();
                Action::None
            }
            Input::Date {
                mut value,
                mut error,
            } => {
                match key.code {
                    KeyCode::Esc => return Action::None,
                    KeyCode::Enter => match parse_date(&value) {
                        Ok(date) => {
                            self.change_date(date);
                            return Action::Request;
                        }
                        _ => error = Some("Use YYYY-MM-DD, between 1900 and 2200.".into()),
                    },
                    KeyCode::Backspace => {
                        value.pop();
                        error = None;
                    }
                    KeyCode::Char(c) if value.len() < 10 && (c.is_ascii_digit() || c == '-') => {
                        value.push(c);
                        error = None;
                    }
                    _ => {}
                }
                self.input = Input::Date { value, error };
                Action::None
            }
            Input::Place {
                mut value,
                mut error,
            } => {
                match key.code {
                    KeyCode::Esc => return Action::None,
                    KeyCode::Enter => match resolve_place(&value, self.games()) {
                        Ok(place) => {
                            self.preferences.place = Some(place);
                            self.view = View::Nearby;
                            self.selected = None;
                            self.normalize_selection();
                            return Action::Save;
                        }
                        Err(e) => error = Some(e),
                    },
                    KeyCode::Backspace => {
                        value.pop();
                        error = None;
                    }
                    KeyCode::Char(c) if value.chars().count() < 80 && !c.is_control() => {
                        value.push(c);
                        error = None;
                    }
                    _ => {}
                }
                self.input = Input::Place { value, error };
                Action::None
            }
            Input::Favorite { mut index } => {
                match key.code {
                    KeyCode::Esc => return Action::None,
                    KeyCode::Up | KeyCode::Down | KeyCode::Char('j' | 'k') => index = 1 - index,
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        if let Some(game) = self.selected_game() {
                            let team = if index == 0 { &game.away } else { &game.home };
                            let (id, name) = (team.id, team.name.clone());
                            if !self.preferences.favorites.remove(&id) {
                                self.preferences.favorites.insert(id);
                                self.message =
                                    Some(format!("Following {name}. Press 2 for your teams."));
                            } else {
                                self.message = Some(format!("Unfollowed {name}."));
                            }
                            self.normalize_selection();
                        }
                        return Action::Save;
                    }
                    _ => {}
                }
                self.input = Input::Favorite { index };
                Action::None
            }
            Input::League { mut index } => {
                match key.code {
                    KeyCode::Esc => return Action::None,
                    KeyCode::Up | KeyCode::Char('k') => index = index.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => {
                        index = (index + 1).min(League::ALL.len() - 1)
                    }
                    KeyCode::Enter => {
                        self.query.league = League::ALL[index];
                        self.change_date(self.query.date);
                        return Action::Request;
                    }
                    _ => {}
                }
                self.input = Input::League { index };
                Action::None
            }
            Input::Help | Input::Details => {
                match key.code {
                    KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?') => {}
                    _ => self.input = mode,
                }
                Action::None
            }
            Input::Normal => self.normal_key(key.code),
        }
    }

    fn normal_key(&mut self, code: KeyCode) -> Action {
        self.message = None;
        match code {
            KeyCode::Char('q') => return Action::Quit,
            KeyCode::Up | KeyCode::Char('k') => self.select(-1),
            KeyCode::Down | KeyCode::Char('j') => self.select(1),
            KeyCode::PageUp => self.select(-8),
            KeyCode::PageDown => self.select(8),
            KeyCode::Home => self.select(isize::MIN),
            KeyCode::End => self.select(isize::MAX),
            KeyCode::Left | KeyCode::Char('h' | '[') => {
                if let Some(date) = shift_date(self.query.date, -1) {
                    self.change_date(date);
                    return Action::Request;
                }
            }
            KeyCode::Right | KeyCode::Char(']') => {
                if let Some(date) = shift_date(self.query.date, 1) {
                    self.change_date(date);
                    return Action::Request;
                }
            }
            KeyCode::Char('t') => {
                let today = self.local_time(self.now).date_naive();
                if crate::model::parse_date(&today.to_string()).is_ok() {
                    self.change_date(today);
                    return Action::Request;
                }
            }
            KeyCode::Char('r') => {
                self.state = DataState::Loading;
                return Action::Request;
            }
            KeyCode::Char('/') => self.input = Input::Search,
            KeyCode::Esc => {
                self.search.clear();
                self.view = View::All;
                self.normalize_selection();
            }
            KeyCode::Char('g') => {
                self.input = Input::Date {
                    value: String::new(),
                    error: None,
                }
            }
            KeyCode::Char('l') => {
                self.input = Input::League {
                    index: League::ALL
                        .iter()
                        .position(|l| *l == self.query.league)
                        .unwrap_or(0),
                }
            }
            KeyCode::Char('f') if self.selected_game().is_some() => {
                self.input = Input::Favorite { index: 1 }
            }
            KeyCode::Char('p' | 'n') => {
                self.input = Input::Place {
                    value: String::new(),
                    error: None,
                }
            }
            KeyCode::Char('1') => {
                self.view = View::All;
                self.normalize_selection();
            }
            KeyCode::Char('2') => {
                self.view = View::Following;
                self.normalize_selection();
            }
            KeyCode::Char('3') => {
                if self.preferences.place.is_none() {
                    self.input = Input::Place {
                        value: String::new(),
                        error: None,
                    };
                } else {
                    self.view = View::Nearby;
                    self.normalize_selection();
                }
            }
            KeyCode::Char('?') => self.input = Input::Help,
            KeyCode::Enter => self.input = Input::Details,
            _ => {}
        }
        Action::None
    }
}

pub fn load_preferences(directory: &Path) -> (Preferences, Option<String>) {
    let path = directory.join("preferences.json");
    if !path.exists() {
        return (Preferences::default(), None);
    }
    let result = fs::read(&path)
        .ok()
        .filter(|b| b.len() <= 65_536)
        .and_then(|b| serde_json::from_slice::<Preferences>(&b).ok())
        .filter(|p| {
            p.version == 1
                && p.place.as_ref().is_none_or(|v| {
                    Coordinates::new(v.coordinates.latitude, v.coordinates.longitude).is_some()
                        && v.name.chars().count() <= 80
                        && !v.name.chars().any(char::is_control)
                })
        });
    match result {
        Some(p) => (p, None),
        None => (
            Preferences::default(),
            Some("Saved preferences could not be read. Using defaults.".into()),
        ),
    }
}

pub fn save_preferences(directory: &Path, preferences: &Preferences) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(preferences)?;
    storage::write_atomic(&directory.join("preferences.json"), &bytes)?;
    Ok(())
}

pub fn resolve_place(input: &str, games: &[Game]) -> Result<Place, String> {
    let text = input.trim();
    if let Some((lat, lon)) = text.split_once(',') {
        let coordinates = lat
            .trim()
            .parse::<f64>()
            .ok()
            .zip(lon.trim().parse::<f64>().ok())
            .and_then(|(lat, lon)| Coordinates::new(lat, lon));
        return coordinates
            .map(|coordinates| Place {
                name: format!("{:.2}, {:.2}", coordinates.latitude, coordinates.longitude),
                coordinates,
            })
            .ok_or_else(|| "Use latitude,longitude within -90..90 and -180..180.".into());
    }
    if text.is_empty() {
        return Err("Enter a city, today's ballpark, or latitude,longitude.".into());
    }
    let needle = text.to_lowercase();
    let exact = PLACES
        .iter()
        .find(|(name, _, _)| name.to_lowercase() == needle);
    let matches: Vec<_> = PLACES
        .iter()
        .filter(|(name, _, _)| name.to_lowercase().contains(&needle))
        .collect();
    if let Some((name, latitude, longitude)) =
        exact.or_else(|| (matches.len() == 1).then(|| matches[0]))
    {
        return Ok(Place {
            name: (*name).into(),
            coordinates: Coordinates {
                latitude: *latitude,
                longitude: *longitude,
            },
        });
    }
    let venues: Vec<_> = games
        .iter()
        .filter(|g| {
            g.venue.coordinates.is_some()
                && (g.venue.name.to_lowercase().contains(&needle)
                    || g.venue.city.to_lowercase() == needle)
        })
        .collect();
    if let Some(game) = venues.first()
        && venues
            .iter()
            .all(|candidate| candidate.venue_key() == game.venue_key())
    {
        return Ok(Place {
            name: game.venue.name.clone(),
            coordinates: game.venue.coordinates.unwrap(),
        });
    }
    Err("Place not found or ambiguous. Try a city such as Denver, or 39.75,-104.99.".into())
}

const PLACES: &[(&str, f64, f64)] = &[
    ("Atlanta", 33.749, -84.388),
    ("Baltimore", 39.29, -76.612),
    ("Boston", 42.36, -71.059),
    ("Chicago", 41.878, -87.63),
    ("Cincinnati", 39.103, -84.512),
    ("Cleveland", 41.499, -81.694),
    ("Dallas", 32.777, -96.797),
    ("Denver", 39.739, -104.99),
    ("Detroit", 42.331, -83.046),
    ("Houston", 29.76, -95.37),
    ("Kansas City", 39.1, -94.579),
    ("Los Angeles", 34.052, -118.244),
    ("Miami", 25.762, -80.192),
    ("Milwaukee", 43.039, -87.906),
    ("Minneapolis", 44.978, -93.265),
    ("New York", 40.713, -74.006),
    ("NYC", 40.713, -74.006),
    ("Oakland", 37.804, -122.271),
    ("Philadelphia", 39.953, -75.164),
    ("Phoenix", 33.449, -112.074),
    ("Pittsburgh", 40.44, -79.996),
    ("Sacramento", 38.582, -121.494),
    ("San Diego", 32.716, -117.161),
    ("San Francisco", 37.775, -122.419),
    ("Seattle", 47.606, -122.332),
    ("St. Louis", 38.627, -90.199),
    ("St Louis", 38.627, -90.199),
    ("Tampa", 27.951, -82.457),
    ("Toronto", 43.653, -79.383),
    ("Washington", 38.907, -77.037),
    ("Albuquerque", 35.084, -106.65),
    ("Austin", 30.267, -97.743),
    ("Boise", 43.616, -116.202),
    ("Buffalo", 42.886, -78.878),
    ("Charlotte", 35.227, -80.843),
    ("Columbus", 39.961, -82.999),
    ("Durham", 35.994, -78.899),
    ("El Paso", 31.761, -106.485),
    ("Indianapolis", 39.768, -86.158),
    ("Jacksonville", 30.332, -81.656),
    ("Las Vegas", 36.17, -115.14),
    ("Louisville", 38.253, -85.759),
    ("Memphis", 35.15, -90.049),
    ("Nashville", 36.163, -86.782),
    ("Norfolk", 36.851, -76.286),
    ("Oklahoma City", 35.468, -97.516),
    ("Omaha", 41.256, -95.934),
    ("Portland", 45.515, -122.678),
    ("Reno", 39.53, -119.814),
    ("Richmond", 37.541, -77.436),
    ("Salt Lake City", 40.761, -111.891),
    ("San Antonio", 29.424, -98.494),
    ("Spokane", 47.658, -117.426),
    ("Vancouver", 49.283, -123.121),
    ("Worcester", 42.263, -71.803),
];
