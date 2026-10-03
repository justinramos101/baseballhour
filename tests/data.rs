use baseballhour::{
    data::{ScheduleClient, demo, parse_schedule},
    model::{GameStatus, League, Query},
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

const MLB: &str = include_str!("fixtures/mlb-2026-07-04.json");
const AAA: &str = include_str!("fixtures/aaa-2026-07-04.json");

fn query(league: League) -> Query {
    Query {
        date: "2026-07-04".parse().unwrap(),
        league,
    }
}
fn fetched() -> DateTime<Utc> {
    "2026-07-05T04:00:00Z".parse().unwrap()
}
fn fixture() -> Value {
    serde_json::from_str(MLB).unwrap()
}

#[test]
fn recorded_mlb_scores_venue_pitchers_and_linescore() {
    let result = parse_schedule(MLB, query(League::Mlb), fetched()).unwrap();
    assert_eq!(result.games.len(), 15);
    assert!(result.warnings.is_empty());
    let game = &result.games[0];
    assert_eq!(game.id, 822_716);
    assert_eq!(game.away.name, "Pittsburgh Pirates");
    assert_eq!((game.away.score, game.home.score), (Some(7), Some(1)));
    assert_eq!(
        game.away.probable_pitcher.as_deref(),
        Some("Braxton Ashcraft")
    );
    assert_eq!(game.venue.name, "Nationals Park");
    assert_eq!(game.venue.city, "Washington");
    #[expect(
        clippy::float_cmp,
        reason = "Parsing must preserve the exact fixture coordinate"
    )]
    {
        assert_eq!(game.venue.coordinates.unwrap().latitude, 38.872_861);
    }
    assert_eq!(game.status, GameStatus::Final);
    let line = game.linescore.as_ref().unwrap();
    assert_eq!(line.innings[1].away, Some(4));
    assert_eq!(line.away_hits, Some(11));
    assert_eq!(game.broadcasts, ["SportsNet Pittsburgh", "Nationals.TV"]);
}

#[test]
fn recorded_aaa_retains_doubleheader_identity() {
    let result = parse_schedule(AAA, query(League::Aaa), fetched()).unwrap();
    assert_eq!(result.games.len(), 17);
    assert_eq!(
        (result.games[0].id, result.games[0].doubleheader),
        (816_106, Some(1))
    );
    assert_eq!(
        (result.games[1].id, result.games[1].doubleheader),
        (816_107, Some(2))
    );
    assert_eq!(result.games[0].league, "Triple-A");
    assert_eq!(result.games[0].venue.name, "First Horizon Park");
    assert_eq!(result.games[0].home.score, Some(4));
}

#[test]
fn missing_scores_tbd_start_and_venue_are_independent() {
    let mut body = fixture();
    let game = &mut body["dates"][0]["games"][0];
    game["status"]["startTimeTBD"] = json!(true);
    game["teams"]["away"]
        .as_object_mut()
        .unwrap()
        .remove("score");
    game["teams"]["home"]["score"] = json!(0);
    game["venue"]["name"] = json!("Neutral\u{1b}\n park");
    game["venue"]["location"]["defaultCoordinates"]["latitude"] = json!(123);
    let result = parse_schedule(&body.to_string(), query(League::Mlb), fetched()).unwrap();
    let game = &result.games[0];
    assert_eq!(game.starts_at, None);
    assert_eq!(game.away.score, None);
    assert_eq!(game.home.score, Some(0));
    assert_eq!(game.venue.name, "Neutral park");
    assert_eq!(game.venue.coordinates, None);
    assert_eq!(game.linescore.as_ref().unwrap().inning, Some(9));
}

#[test]
fn exceptional_detail_overrides_abstract_state_and_unknown_text_survives() {
    for (detail, expected) in [
        ("Postponed", GameStatus::Postponed),
        ("Delayed Start", GameStatus::Delayed),
        ("Suspended", GameStatus::Suspended),
        ("Cancelled", GameStatus::Cancelled),
    ] {
        let mut body = fixture();
        body["dates"][0]["games"][0]["status"] =
            json!({"abstractGameState":"Live", "detailedState":detail});
        let result = parse_schedule(&body.to_string(), query(League::Mlb), fetched()).unwrap();
        assert_eq!(result.games[0].status, expected);
        assert_eq!(result.games[0].status_text, detail);
    }
    let mut body = fixture();
    body["dates"][0]["games"][0]["status"] =
        json!({"abstractGameState":"FutureCode", "detailedState":"Under review"});
    let result = parse_schedule(&body.to_string(), query(League::Mlb), fetched()).unwrap();
    assert_eq!(result.games[0].status, GameStatus::Other);
    assert_eq!(result.games[0].status_text, "Under review");
}

#[test]
fn invalid_envelopes_are_errors_and_partial_results_warn() {
    for body in [
        "no json",
        "{}",
        r#"{"dates":[]}"#,
        r#"{"dates":[],"totalGames":2}"#,
        r#"{"dates":[{"games":[{}]}],"totalGames":1}"#,
    ] {
        assert!(parse_schedule(body, query(League::Mlb), fetched()).is_err());
    }
    let empty = parse_schedule(
        r#"{"dates":[],"totalGames":0}"#,
        query(League::Mlb),
        fetched(),
    )
    .unwrap();
    assert!(empty.games.is_empty());
    assert!(empty.warnings.is_empty());
    let mut body = fixture();
    body["dates"][0]["games"][0] = json!({});
    let result = parse_schedule(&body.to_string(), query(League::Mlb), fetched()).unwrap();
    assert_eq!(result.games.len(), 14);
    assert_eq!(result.warnings, ["Skipped 1 malformed schedule entries"]);
}

#[test]
fn outer_schedule_date_must_parse_and_match_the_query() {
    for date in [json!("2026-07-03"), json!("not-a-date"), Value::Null] {
        let mut body = fixture();
        body["dates"][0]["date"] = date;
        assert!(parse_schedule(&body.to_string(), query(League::Mlb), fetched()).is_err());
    }
}

#[test]
fn malformed_games_are_skipped_but_resumed_official_dates_are_preserved() {
    let mut body = fixture();
    body["dates"][0]["games"][0]["officialDate"] = json!("2026-07-03");
    body["dates"][0]["games"][1]["officialDate"] = json!("not-a-date");
    body["dates"][0]["games"][2]
        .as_object_mut()
        .unwrap()
        .remove("officialDate");
    let result = parse_schedule(&body.to_string(), query(League::Mlb), fetched()).unwrap();
    assert_eq!(result.games.len(), 13);
    assert_eq!(result.games[0].official_date, "2026-07-03".parse().unwrap());
    assert_eq!(result.warnings, ["Skipped 2 malformed schedule entries"]);
}

#[test]
fn cache_accepts_only_matching_valid_envelopes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schedule-2026-07-04-1.json");
    let client = ScheduleClient::new(Some(directory.path().to_owned())).unwrap();
    let valid = json!({"schema":1,"query":query(League::Mlb),"fetched_at":fetched(),"body":MLB});
    std::fs::write(&path, valid.to_string()).unwrap();
    let cached = client.cached(query(League::Mlb)).unwrap();
    assert_eq!(cached.games[0].home.score, Some(1));
    assert_eq!(cached.fetched_at, fetched());
    for bad in [
        json!({}),
        json!({"schema":2,"query":query(League::Mlb),"fetched_at":fetched(),"body":MLB}),
        json!({"schema":1,"query":query(League::Aaa),"fetched_at":fetched(),"body":MLB}),
        json!({"schema":1,"query":query(League::Mlb),"fetched_at":fetched(),"body":"broken"}),
    ] {
        std::fs::write(&path, bad.to_string()).unwrap();
        assert!(client.cached(query(League::Mlb)).is_none());
    }
    std::fs::write(path, "not json").unwrap();
    assert!(client.cached(query(League::Mlb)).is_none());
}

#[test]
fn demo_is_explicit_synthetic_and_honors_query() {
    let query = Query {
        date: "2030-02-01".parse().unwrap(),
        league: League::Mlb,
    };
    let result = demo(query);
    assert_eq!(result.query, query);
    assert_eq!(result.games.len(), 15);
    assert!(result.warnings[0].contains("Synthetic"));
    for state in [
        GameStatus::Live,
        GameStatus::Scheduled,
        GameStatus::Final,
        GameStatus::Postponed,
    ] {
        assert!(result.games.iter().any(|game| game.status == state));
    }
    assert!(
        result
            .games
            .iter()
            .all(|game| { game.official_date == query.date })
    );
    let aaa = demo(Query {
        league: League::Aaa,
        ..query
    });
    assert_eq!(aaa.games[0].home.name, "Nashville Sounds");
    assert!(aaa.games.iter().all(|game| game.league == "Triple-A"));
}

#[test]
fn all_demo_combines_every_affiliated_level_on_the_requested_day() {
    let query = Query {
        date: "2031-08-19".parse().unwrap(),
        league: League::All,
    };
    let result = demo(query);
    assert_eq!(result.games.len(), 33);
    let leagues: std::collections::BTreeSet<_> = result
        .games
        .iter()
        .map(|game| game.league.as_str())
        .collect();
    assert_eq!(
        leagues,
        ["Double-A", "High-A", "MLB", "Single-A", "Triple-A"]
            .into_iter()
            .collect()
    );
    assert!(
        result
            .games
            .iter()
            .all(|game| game.official_date == query.date)
    );
}

#[test]
fn demo_preserves_overnight_utc_offsets_when_shifting_dates() {
    let original = demo(query(League::All));
    let shifted_query = Query {
        date: "2030-07-04".parse().unwrap(),
        league: League::All,
    };
    let shifted = demo(shifted_query);
    let colorado = original
        .games
        .iter()
        .find(|game| game.id == 824_334)
        .unwrap();
    let start = colorado.starts_at.unwrap();
    assert_eq!(start.to_rfc3339(), "2026-07-05T00:10:00+00:00");
    assert_eq!(
        start
            .with_timezone(&chrono_tz::America::New_York)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        "2026-07-04 20:10"
    );
    let colorado = shifted
        .games
        .iter()
        .find(|game| game.id == 824_334)
        .unwrap();
    assert_eq!(
        colorado
            .starts_at
            .unwrap()
            .with_timezone(&chrono_tz::America::New_York)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        "2030-07-04 20:10"
    );
    assert_eq!(shifted.games.len(), original.games.len());
    for (original, shifted) in original.games.iter().zip(&shifted.games) {
        assert_eq!(original.id, shifted.id);
        assert_eq!(
            original.starts_at.map(|start| start.signed_duration_since(
                original
                    .official_date
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc()
            )),
            shifted.starts_at.map(|start| start.signed_duration_since(
                shifted
                    .official_date
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc()
            ))
        );
    }
}

#[test]
fn inning_break_state_takes_priority_over_half() {
    for state in ["Middle", "End"] {
        let mut body = fixture();
        let game = &mut body["dates"][0]["games"][0];
        game["status"] = json!({"abstractGameState":"Live", "detailedState":"In Progress"});
        game["linescore"]["inningState"] = json!(state);
        game["linescore"]["inningHalf"] = json!("Top");
        game["linescore"]["currentInning"] = json!(5);
        let result = parse_schedule(&body.to_string(), query(League::Mlb), fetched()).unwrap();
        assert_eq!(result.games[0].linescore.as_ref().unwrap().half, state);
        assert_eq!(result.games[0].state_label(), format!("{state} 5"));
    }
}
