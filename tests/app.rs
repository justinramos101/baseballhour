use baseballhour::{
    app::{
        Action, App, DataState, Input, Preferences, View, load_preferences, resolve_place,
        save_preferences,
    },
    data::demo,
    model::{Coordinates, League, MAX_DATE, MIN_DATE, Query},
};
use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::America::New_York;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn date(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

fn instant(value: &str) -> DateTime<Utc> {
    value.parse().unwrap()
}

fn loaded_app() -> App {
    let query = Query {
        date: date("2026-07-04"),
        league: League::Mlb,
    };
    let mut app = App::new(query, Preferences::default(), Some(New_York));
    assert!(app.accept(demo(query), DataState::Fresh));
    app
}

fn press(app: &mut App, code: KeyCode) -> Action {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn type_text(app: &mut App, value: &str) {
    for character in value.chars() {
        assert_eq!(press(app, KeyCode::Char(character)), Action::None);
    }
}

#[test]
fn date_navigation_removes_old_games_before_requesting_new_ones() {
    let mut app = loaded_app();
    assert!(!app.games().is_empty());
    assert_eq!(press(&mut app, KeyCode::Right), Action::Request);
    assert_eq!(app.query.date, date("2026-07-05"));
    assert!(app.games().is_empty());
    assert!(app.selected.is_none());
    assert!(matches!(app.state, DataState::Loading));
    assert_eq!(press(&mut app, KeyCode::Left), Action::Request);
    assert_eq!(app.query.date, date("2026-07-04"));
}

#[test]
fn date_arrows_stop_at_the_supported_boundaries() {
    let mut app = loaded_app();
    app.change_date(MIN_DATE);
    assert_eq!(press(&mut app, KeyCode::Left), Action::None);
    assert_eq!(app.query.date, MIN_DATE);
    app.change_date(MAX_DATE);
    assert_eq!(press(&mut app, KeyCode::Right), Action::None);
    assert_eq!(app.query.date, MAX_DATE);
}

#[test]
fn an_outdated_response_cannot_replace_the_current_schedule() {
    let mut app = loaded_app();
    let previous = demo(app.query);
    press(&mut app, KeyCode::Right);
    assert!(!app.accept(previous, DataState::Fresh));
    assert!(app.snapshot.is_none());
    let mut wrong_league = app.query;
    wrong_league.league = League::Aaa;
    assert!(!app.accept(demo(wrong_league), DataState::Fresh));
    assert!(app.accept(demo(app.query), DataState::Fresh));
    assert_eq!(
        app.snapshot.as_ref().unwrap().query.date,
        date("2026-07-05")
    );
}

#[test]
fn selected_game_survives_a_reordered_refresh() {
    let mut app = loaded_app();
    app.selected = Some(823_526);
    let mut refresh = demo(app.query);
    refresh.games.reverse();
    let selected = refresh
        .games
        .iter_mut()
        .find(|game| game.id == 823_526)
        .unwrap();
    selected.home.score = Some(9);
    assert!(app.accept(refresh, DataState::Fresh));
    assert_eq!(app.selected, Some(823_526));
    assert_eq!(app.selected_game().unwrap().home.score, Some(9));
    assert_eq!(app.selected_game().unwrap().home.abbreviation, "NYY");
}

#[test]
fn search_accepts_q_as_text_and_escape_restores_the_schedule() {
    let mut app = loaded_app();
    press(&mut app, KeyCode::Char('/'));
    assert_eq!(press(&mut app, KeyCode::Char('q')), Action::None);
    assert_eq!(app.search, "q");
    assert!(matches!(app.input, Input::Search));
    press(&mut app, KeyCode::Backspace);
    type_text(&mut app, "nyy");
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.input, Input::Normal));
    assert_eq!(app.visible_games().len(), 1);
    assert_eq!(app.selected_game().unwrap().home.abbreviation, "NYY");
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.search, "");
    assert!(app.visible_games().len() > 1);
}

#[test]
fn invalid_date_stays_open_without_losing_the_current_games() {
    let mut app = loaded_app();
    press(&mut app, KeyCode::Char('g'));
    type_text(&mut app, "2026-02-30");
    assert_eq!(press(&mut app, KeyCode::Enter), Action::None);
    assert!(matches!(&app.input, Input::Date { value, error: Some(_) } if value == "2026-02-30"));
    assert_eq!(app.query.date, date("2026-07-04"));
    assert!(!app.games().is_empty());
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('g'));
    type_text(&mut app, "2026-02-28");
    assert_eq!(press(&mut app, KeyCode::Enter), Action::Request);
    assert_eq!(app.query.date, date("2026-02-28"));
    assert!(matches!(app.input, Input::Normal));
}

#[test]
fn following_starts_empty_and_f_can_follow_each_side() {
    let mut app = loaded_app();
    press(&mut app, KeyCode::Char('2'));
    assert_eq!(app.view, View::Following);
    assert!(app.visible_games().is_empty());
    assert!(app.selected.is_none());
    press(&mut app, KeyCode::Char('1'));
    app.selected = Some(823_526);
    press(&mut app, KeyCode::Char('f'));
    assert!(matches!(app.input, Input::Favorite { index: 1 }));
    assert_eq!(press(&mut app, KeyCode::Enter), Action::Save);
    assert_eq!(
        app.preferences
            .favorites
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![147]
    );
    press(&mut app, KeyCode::Char('2'));
    assert_eq!(app.visible_games().len(), 1);
    assert_eq!(app.selected, Some(823_526));
    press(&mut app, KeyCode::Char('f'));
    press(&mut app, KeyCode::Up);
    assert_eq!(press(&mut app, KeyCode::Enter), Action::Save);
    assert_eq!(
        app.preferences
            .favorites
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![142, 147]
    );
    press(&mut app, KeyCode::Char('f'));
    assert_eq!(press(&mut app, KeyCode::Enter), Action::Save);
    assert_eq!(
        app.preferences
            .favorites
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![142]
    );
    assert_eq!(app.visible_games().len(), 1);
}

#[test]
fn nearby_resolves_denver_and_sorts_ballparks_by_distance() {
    let mut app = loaded_app();
    press(&mut app, KeyCode::Char('3'));
    assert!(matches!(app.input, Input::Place { .. }));
    type_text(&mut app, "Denver");
    assert_eq!(press(&mut app, KeyCode::Enter), Action::Save);
    assert_eq!(app.view, View::Nearby);
    assert_eq!(app.preferences.place.as_ref().unwrap().name, "Denver");
    let games = app.visible_games();
    assert_eq!(games[0].venue.name, "Coors Field");
    assert!(app.distance(games[0]).unwrap() < 2.0);
    assert!(
        games
            .windows(2)
            .all(|pair| app.distance(pair[0]).unwrap() <= app.distance(pair[1]).unwrap())
    );
}

#[test]
fn preferences_recover_from_corruption_and_round_trip_after_save() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("preferences.json"), b"{broken").unwrap();
    let (preferences, warning) = load_preferences(directory.path());
    assert!(warning.is_some());
    assert!(preferences.favorites.is_empty());
    assert!(preferences.place.is_none());
    let mut app = loaded_app();
    app.preferences.favorites.insert(147);
    press(&mut app, KeyCode::Char('3'));
    type_text(&mut app, "Denver");
    press(&mut app, KeyCode::Enter);
    save_preferences(directory.path(), &app.preferences).unwrap();
    let (saved, warning) = load_preferences(directory.path());
    assert!(warning.is_none());
    assert_eq!(
        saved.favorites.iter().copied().collect::<Vec<_>>(),
        vec![147]
    );
    assert_eq!(saved.place.as_ref().unwrap().name, "Denver");
    #[expect(
        clippy::float_cmp,
        reason = "Serialization must preserve the exact coordinate"
    )]
    {
        assert_eq!(saved.place.unwrap().coordinates.latitude, 39.739);
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);

    app.preferences.favorites.insert(142);
    save_preferences(directory.path(), &app.preferences).unwrap();
    let (saved, warning) = load_preferences(directory.path());
    assert!(warning.is_none());
    assert_eq!(
        saved.favorites.into_iter().collect::<Vec<_>>(),
        vec![142, 147]
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn today_uses_the_selected_timezones_calendar_date() {
    let mut app = loaded_app();
    app.now = instant("2026-07-05T03:59:00Z");
    assert_eq!(press(&mut app, KeyCode::Char('t')), Action::Request);
    assert_eq!(app.query.date, date("2026-07-04"));
    app.now = instant("2026-07-05T04:00:00Z");
    press(&mut app, KeyCode::Char('t'));
    assert_eq!(app.query.date, date("2026-07-05"));
}

#[test]
fn local_times_handle_both_daylight_saving_transitions() {
    let app = loaded_app();
    for (utc, local) in [
        ("2026-03-08T06:59:00Z", "2026-03-08 01:59 -05:00"),
        ("2026-03-08T07:00:00Z", "2026-03-08 03:00 -04:00"),
        ("2026-11-01T05:30:00Z", "2026-11-01 01:30 -04:00"),
        ("2026-11-01T06:30:00Z", "2026-11-01 01:30 -05:00"),
    ] {
        assert_eq!(
            app.local_time(instant(utc))
                .format("%Y-%m-%d %H:%M %:z")
                .to_string(),
            local
        );
    }
}

#[test]
fn start_labels_mark_games_on_an_adjacent_local_day() {
    let app = loaded_app();
    let mut game = app.games()[0].clone();
    game.starts_at = Some(instant("2026-07-04T01:00:00Z"));
    assert_eq!(app.start_label(&game), "9:00 PM -1d");
    game.starts_at = Some(instant("2026-07-05T05:00:00Z"));
    assert_eq!(app.start_label(&game), "1:00 AM +1d");
    game.starts_at = None;
    assert_eq!(app.start_label(&game), "Time TBD");
}

#[test]
fn distances_are_sensible_at_city_and_dateline_scales() {
    let denver = Coordinates::new(39.7392, -104.9903).unwrap();
    let new_york = Coordinates::new(40.7128, -74.0060).unwrap();
    #[expect(
        clippy::float_cmp,
        reason = "Distance to the identical point must be exactly zero"
    )]
    {
        assert_eq!(denver.miles_to(denver), 0.0);
    }
    assert!((denver.miles_to(new_york) - 1628.0).abs() < 3.0);
    let west = Coordinates::new(0.0, 179.0).unwrap();
    let east = Coordinates::new(0.0, -179.0).unwrap();
    assert!((west.miles_to(east) - 138.2).abs() < 0.2);
    assert!(Coordinates::new(91.0, 0.0).is_none());
    assert!(Coordinates::new(0.0, f64::NAN).is_none());
}

#[test]
fn control_c_quits_even_when_help_is_open() {
    let mut app = loaded_app();
    press(&mut app, KeyCode::Char('?'));
    assert!(matches!(app.input, Input::Help));
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        Action::Quit
    );
}

#[test]
fn q_quits_non_text_modes_while_text_modes_accept_it_and_escape_closes() {
    let mut app = loaded_app();
    assert_eq!(press(&mut app, KeyCode::Char('q')), Action::Quit);

    for input in [
        Input::Favorite { index: 0 },
        Input::League { index: 0 },
        Input::Help,
        Input::Details,
    ] {
        app.input = input;
        assert_eq!(press(&mut app, KeyCode::Char('q')), Action::Quit);
    }

    app.input = Input::Search;
    app.search.clear();
    assert_eq!(press(&mut app, KeyCode::Char('q')), Action::None);
    assert_eq!(app.search, "q");
    app.input = Input::Place {
        value: String::new(),
        error: None,
    };
    assert_eq!(press(&mut app, KeyCode::Char('q')), Action::None);
    assert!(matches!(&app.input, Input::Place { value, .. } if value == "q"));

    for input in [
        Input::Favorite { index: 0 },
        Input::League { index: 0 },
        Input::Help,
        Input::Details,
    ] {
        app.input = input;
        assert_eq!(press(&mut app, KeyCode::Esc), Action::None);
        assert!(matches!(app.input, Input::Normal));
    }
}

#[test]
fn unknown_venues_remain_distinct_while_known_venues_group() {
    let mut app = loaded_app();
    let mut second = app.games()[1].clone();
    let first = &mut app.snapshot.as_mut().unwrap().games[0];
    first.venue.name = "Mystery Park".into();
    first.venue.city = "Nowhere".into();
    first.venue.id = None;
    second.venue = first.venue.clone();
    second.venue.id = None;
    app.snapshot.as_mut().unwrap().games = vec![first.clone(), second.clone()];
    assert!(resolve_place("Mystery Park", app.games()).is_err());

    app.snapshot.as_mut().unwrap().games[0].venue.id = Some(77);
    app.snapshot.as_mut().unwrap().games[1].venue.id = Some(77);
    assert_eq!(
        resolve_place("Mystery Park", app.games()).unwrap().name,
        "Mystery Park"
    );
}
