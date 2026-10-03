use chrono::{DateTime, Datelike, Days, NaiveDate, Utc};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
pub enum League {
    #[default]
    Mlb,
    All,
    Aaa,
    Aa,
    HighA,
    SingleA,
}

impl League {
    pub const ALL: [Self; 6] = [
        Self::Mlb,
        Self::All,
        Self::Aaa,
        Self::Aa,
        Self::HighA,
        Self::SingleA,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Mlb => "MLB",
            Self::All => "MLB + MiLB",
            Self::Aaa => "Triple-A",
            Self::Aa => "Double-A",
            Self::HighA => "High-A",
            Self::SingleA => "Single-A",
        }
    }

    pub fn sport_ids(self) -> &'static str {
        match self {
            Self::Mlb => "1",
            Self::All => "1,11,12,13,14",
            Self::Aaa => "11",
            Self::Aa => "12",
            Self::HighA => "13",
            Self::SingleA => "14",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Query {
    pub date: NaiveDate,
    pub league: League,
}

pub const MIN_DATE: NaiveDate = NaiveDate::from_ymd_opt(1900, 1, 1).unwrap();
pub const MAX_DATE: NaiveDate = NaiveDate::from_ymd_opt(2200, 12, 31).unwrap();

pub fn parse_date(value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .filter(|date| (MIN_DATE.year()..=MAX_DATE.year()).contains(&date.year()))
        .ok_or_else(|| "expected YYYY-MM-DD between 1900 and 2200".into())
}

pub fn shift_date(date: NaiveDate, days: i64) -> Option<NaiveDate> {
    let shifted = if days < 0 {
        date.checked_sub_days(Days::new(days.unsigned_abs()))
    } else {
        date.checked_add_days(Days::new(days as u64))
    }?;
    (MIN_DATE..=MAX_DATE).contains(&shifted).then_some(shifted)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Team {
    pub id: u32,
    pub name: String,
    pub abbreviation: String,
    pub record: Option<String>,
    pub score: Option<u16>,
    pub probable_pitcher: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Coordinates {
    pub latitude: f64,
    pub longitude: f64,
}

impl Coordinates {
    pub fn new(latitude: f64, longitude: f64) -> Option<Self> {
        (latitude.is_finite()
            && longitude.is_finite()
            && (-90.0..=90.0).contains(&latitude)
            && (-180.0..=180.0).contains(&longitude))
        .then_some(Self {
            latitude,
            longitude,
        })
    }

    pub fn miles_to(self, other: Self) -> f64 {
        let a = ((other.latitude - self.latitude).to_radians() / 2.0)
            .sin()
            .powi(2)
            + self.latitude.to_radians().cos()
                * other.latitude.to_radians().cos()
                * ((other.longitude - self.longitude).to_radians() / 2.0)
                    .sin()
                    .powi(2);
        3958.7613 * 2.0 * a.sqrt().min(1.0).asin()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Venue {
    pub id: Option<u32>,
    pub name: String,
    pub city: String,
    pub coordinates: Option<Coordinates>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum GameStatus {
    Scheduled,
    Live,
    Final,
    Delayed,
    Postponed,
    Suspended,
    Cancelled,
    Other,
}

impl GameStatus {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Live | Self::Delayed | Self::Suspended)
    }

    pub fn rank(self) -> u8 {
        match self {
            Self::Live => 0,
            Self::Delayed | Self::Suspended => 1,
            Self::Scheduled => 2,
            Self::Final => 3,
            _ => 4,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Inning {
    pub number: u16,
    pub away: Option<u16>,
    pub home: Option<u16>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Linescore {
    pub inning: Option<u16>,
    pub half: String,
    pub innings: Vec<Inning>,
    pub away_hits: Option<u16>,
    pub home_hits: Option<u16>,
    pub away_errors: Option<u16>,
    pub home_errors: Option<u16>,
    pub balls: Option<u8>,
    pub strikes: Option<u8>,
    pub outs: Option<u8>,
    pub bases: [bool; 3],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub id: u64,
    /// The provider's original official date, which may precede the requested slate date.
    pub official_date: NaiveDate,
    pub starts_at: Option<DateTime<Utc>>,
    pub away: Team,
    pub home: Team,
    pub venue: Venue,
    pub status: GameStatus,
    pub status_text: String,
    pub league: String,
    pub doubleheader: Option<u8>,
    pub linescore: Option<Linescore>,
    pub broadcasts: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum VenueKey {
    Known(u32),
    UnknownGame(u64),
}

impl Game {
    pub(crate) fn venue_key(&self) -> VenueKey {
        self.venue
            .id
            .map_or(VenueKey::UnknownGame(self.id), VenueKey::Known)
    }

    pub fn matches(&self, search: &str) -> bool {
        let needle = search.to_lowercase();
        [
            &self.away.name,
            &self.home.name,
            &self.away.abbreviation,
            &self.home.abbreviation,
            &self.venue.name,
            &self.venue.city,
        ]
        .iter()
        .any(|s| s.to_lowercase().contains(&needle))
    }

    pub fn state_label(&self) -> String {
        if self.status == GameStatus::Live {
            if let Some(line) = &self.linescore
                && let Some(inning) = line.inning
            {
                return format!("{} {}", line.half, inning);
            }
            return "Live".into();
        }
        self.status_text.clone()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub query: Query,
    pub fetched_at: DateTime<Utc>,
    pub games: Vec<Game>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Place {
    pub name: String,
    pub coordinates: Coordinates,
}
