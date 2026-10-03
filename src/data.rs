use std::{fs, io::Read, path::PathBuf, time::Duration};

use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{
    Coordinates, Game, GameStatus, Inning, League, Linescore, Query, Snapshot, Team, Venue,
};
use crate::storage;

const MAX_BODY: u64 = 8 * 1024 * 1024;

pub struct ScheduleClient {
    client: reqwest::blocking::Client,
    cache_dir: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
struct Cache {
    schema: u8,
    query: Query,
    fetched_at: DateTime<Utc>,
    body: String,
}

impl ScheduleClient {
    pub fn new(cache_dir: Option<PathBuf>) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("baseballhour/0.1")
            .build()
            .map_err(|_| anyhow!("Could not initialize the schedule connection"))?;
        Ok(Self { client, cache_dir })
    }

    fn cache_path(&self, query: Query) -> Option<PathBuf> {
        Some(self.cache_dir.as_ref()?.join(format!(
            "schedule-{}-{}.json",
            query.date,
            query.league.sport_ids().replace(',', "-")
        )))
    }

    pub fn cached(&self, query: Query) -> Option<Snapshot> {
        let file = fs::File::open(self.cache_path(query)?).ok()?;
        let mut bytes = Vec::new();
        file.take(MAX_BODY * 2 + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() as u64 > MAX_BODY * 2 {
            return None;
        }
        let cache: Cache = serde_json::from_slice(&bytes).ok()?;
        if cache.schema != 1 || cache.query != query {
            return None;
        }
        parse_schedule(&cache.body, query, cache.fetched_at).ok()
    }

    pub fn fetch(&self, query: Query) -> Result<Snapshot> {
        let response = self
            .client
            .get("https://statsapi.mlb.com/api/v1/schedule")
            .query(&[
                ("sportIds", query.league.sport_ids()),
                ("date", &query.date.to_string()),
                (
                    "hydrate",
                    "team,venue(location),linescore,probablePitcher,broadcasts",
                ),
            ])
            .send()
            .map_err(|error| {
                if error.is_timeout() {
                    anyhow!("Schedule request timed out. Try refreshing.")
                } else {
                    anyhow!("Could not reach MLB schedules. Check your connection and refresh.")
                }
            })?;
        if !response.status().is_success() {
            bail!(
                "MLB schedules returned HTTP {}. Try refreshing.",
                response.status().as_u16()
            );
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_BODY)
        {
            bail!("Schedule response exceeded the size limit");
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_BODY + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| anyhow!("Could not finish reading MLB schedules. Try refreshing."))?;
        if bytes.len() as u64 > MAX_BODY {
            bail!("Schedule response exceeded the size limit");
        }
        let body =
            String::from_utf8(bytes).map_err(|_| anyhow!("MLB returned an unreadable schedule"))?;
        let fetched_at = Utc::now();
        let mut snapshot = parse_schedule(&body, query, fetched_at)?;
        if self.cache_dir.is_some()
            && self
                .write_cache(Cache {
                    schema: 1,
                    query,
                    fetched_at,
                    body,
                })
                .is_err()
        {
            snapshot
                .warnings
                .push("Schedule loaded, but the local cache could not be saved".into());
        }
        Ok(snapshot)
    }

    fn write_cache(&self, cache: Cache) -> Result<()> {
        let destination = self
            .cache_path(cache.query)
            .ok_or_else(|| anyhow!("Cache disabled"))?;
        let bytes = serde_json::to_vec(&cache)?;
        storage::write_atomic(&destination, &bytes)?;
        Ok(())
    }
}

fn text(value: &Value) -> Option<String> {
    let cleaned: String = value
        .as_str()?
        .chars()
        .filter(|c| !c.is_control())
        .take(256)
        .collect();
    let cleaned = cleaned.trim();
    (!cleaned.is_empty()).then(|| cleaned.to_owned())
}

fn number<T: TryFrom<u64>>(value: &Value) -> Option<T> {
    T::try_from(value.as_u64()?).ok()
}

fn team(value: &Value) -> Option<Team> {
    let info = &value["team"];
    Some(Team {
        id: number(&info["id"])?,
        name: text(&info["name"])?,
        abbreviation: text(&info["abbreviation"])
            .or_else(|| text(&info["teamCode"]).map(|s| s.to_uppercase()))
            .unwrap_or_else(|| "?".into()),
        record: value["leagueRecord"]["wins"]
            .as_u64()
            .zip(value["leagueRecord"]["losses"].as_u64())
            .map(|(wins, losses)| format!("{wins}-{losses}")),
        score: number(&value["score"]),
        probable_pitcher: text(&value["probablePitcher"]["fullName"]),
    })
}

fn status(value: &Value) -> (GameStatus, String) {
    let detail = text(&value["detailedState"]).unwrap_or_else(|| "Unknown".into());
    let lower = detail.to_lowercase();
    let state = if lower.contains("postpon") {
        GameStatus::Postponed
    } else if lower.contains("cancel") {
        GameStatus::Cancelled
    } else if lower.contains("suspend") {
        GameStatus::Suspended
    } else if lower.contains("delay") {
        GameStatus::Delayed
    } else {
        match value["abstractGameState"].as_str() {
            Some("Live") => GameStatus::Live,
            Some("Final") => GameStatus::Final,
            Some("Preview") => GameStatus::Scheduled,
            _ => GameStatus::Other,
        }
    };
    (state, detail)
}

fn linescore(value: &Value) -> Option<Linescore> {
    let object = value.as_object()?;
    if object.is_empty() {
        return None;
    }
    Some(Linescore {
        inning: number(&value["currentInning"]),
        half: text(&value["inningState"])
            .or_else(|| text(&value["inningHalf"]))
            .unwrap_or_default(),
        innings: value["innings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|i| {
                Some(Inning {
                    number: number(&i["num"])?,
                    away: number(&i["away"]["runs"]),
                    home: number(&i["home"]["runs"]),
                })
            })
            .collect(),
        away_hits: number(&value["teams"]["away"]["hits"]),
        home_hits: number(&value["teams"]["home"]["hits"]),
        away_errors: number(&value["teams"]["away"]["errors"]),
        home_errors: number(&value["teams"]["home"]["errors"]),
        balls: number(&value["balls"]),
        strikes: number(&value["strikes"]),
        outs: number(&value["outs"]),
        bases: ["first", "second", "third"]
            .map(|base| value["offense"][base]["id"].as_u64().is_some()),
    })
}

fn game(value: &Value, query: Query) -> Option<Game> {
    let venue = &value["venue"];
    let location = &venue["location"];
    let coords = &location["defaultCoordinates"];
    let (status, status_text) = status(&value["status"]);
    let mut broadcasts = Vec::new();
    for broadcast in value["broadcasts"].as_array().into_iter().flatten() {
        if broadcast["type"].as_str() == Some("TV")
            && let Some(name) = text(&broadcast["name"])
            && !broadcasts.contains(&name)
        {
            broadcasts.push(name);
        }
    }
    Some(Game {
        id: number(&value["gamePk"])?,
        official_date: value["officialDate"].as_str()?.parse().ok()?,
        starts_at: if value["status"]["startTimeTBD"].as_bool() == Some(true) {
            None
        } else {
            value["gameDate"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|t| t.with_timezone(&Utc))
        },
        away: team(&value["teams"]["away"])?,
        home: team(&value["teams"]["home"])?,
        venue: Venue {
            id: number(&venue["id"]).filter(|id| *id != 0),
            name: text(&venue["name"]).unwrap_or_else(|| "Venue TBD".into()),
            city: text(&location["city"]).unwrap_or_default(),
            coordinates: coords["latitude"]
                .as_f64()
                .zip(coords["longitude"].as_f64())
                .and_then(|(lat, lon)| Coordinates::new(lat, lon)),
        },
        status,
        status_text,
        league: match value["teams"]["home"]["team"]["sport"]["id"].as_u64() {
            Some(1) => "MLB".into(),
            Some(11) => "Triple-A".into(),
            Some(12) => "Double-A".into(),
            Some(13) => "High-A".into(),
            Some(14) => "Single-A".into(),
            _ => query.league.label().into(),
        },
        doubleheader: match value["doubleHeader"].as_str() {
            Some("Y" | "S") => number(&value["gameNumber"]),
            _ => None,
        },
        linescore: linescore(&value["linescore"]),
        broadcasts,
    })
}

pub fn parse_schedule(body: &str, query: Query, fetched_at: DateTime<Utc>) -> Result<Snapshot> {
    if body.len() as u64 > MAX_BODY {
        bail!("Schedule response exceeded the size limit");
    }
    let value: Value =
        serde_json::from_str(body).map_err(|_| anyhow!("MLB returned an invalid schedule"))?;
    let dates = value["dates"]
        .as_array()
        .ok_or_else(|| anyhow!("MLB returned an invalid schedule envelope"))?;
    let total = value["totalGames"]
        .as_u64()
        .ok_or_else(|| anyhow!("MLB returned an invalid schedule count"))?;
    let mut games = Vec::new();
    let mut skipped = 0;
    let mut supplied = 0;
    for date in dates {
        let slate_date: chrono::NaiveDate = date["date"]
            .as_str()
            .and_then(|date| date.parse().ok())
            .ok_or_else(|| anyhow!("MLB returned an invalid schedule date"))?;
        if slate_date != query.date {
            bail!("MLB returned a schedule for a different date");
        }
        let entries = date["games"]
            .as_array()
            .ok_or_else(|| anyhow!("MLB returned an invalid schedule day"))?;
        supplied += entries.len() as u64;
        for entry in entries {
            match game(entry, query) {
                Some(game) => games.push(game),
                None => skipped += 1,
            }
        }
    }
    if total > 0 && supplied == 0 {
        bail!("MLB returned an incomplete schedule");
    }
    if supplied > 0 && games.is_empty() {
        bail!("MLB returned no readable games");
    }
    let mut warnings = Vec::new();
    if skipped > 0 {
        warnings.push(format!("Skipped {skipped} malformed schedule entries"));
    }
    if supplied != total {
        warnings.push("The schedule count did not match its entries".into());
    }
    Ok(Snapshot {
        query,
        fetched_at,
        games,
        warnings,
    })
}

pub fn demo(query: Query) -> Snapshot {
    let fixtures: Value =
        serde_json::from_str(include_str!("../assets/demo.json")).expect("embedded demo is valid");
    let keys: &[&str] = match query.league {
        League::Mlb => &["mlb"],
        League::All => &["mlb", "aaa", "aa", "high_a", "single_a"],
        League::Aaa => &["aaa"],
        League::Aa => &["aa"],
        League::HighA => &["high_a"],
        League::SingleA => &["single_a"],
    };
    let fetched_at = query
        .date
        .and_hms_opt(20, 0, 0)
        .expect("valid time")
        .and_utc();
    let source_query = Query {
        date: "2026-07-04".parse().unwrap(),
        league: query.league,
    };
    let mut snapshot = parse_schedule(&fixtures[keys[0]].to_string(), source_query, fetched_at)
        .expect("embedded demo schedule is valid");
    for key in &keys[1..] {
        snapshot.games.extend(
            parse_schedule(&fixtures[key].to_string(), source_query, fetched_at)
                .expect("embedded demo schedule is valid")
                .games,
        );
    }
    for game in &mut snapshot.games {
        let shift = query.date.signed_duration_since(game.official_date);
        game.starts_at = game
            .starts_at
            .and_then(|start| start.checked_add_signed(shift));
        game.official_date = query.date;
    }
    snapshot.warnings =
        vec!["Synthetic demonstration. Scores and game states are illustrative.".into()];
    snapshot.query = query;
    snapshot
}
