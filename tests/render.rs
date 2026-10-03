use baseballhour::{
    app::{App, DataState, Input, Preferences, View, resolve_place},
    data,
    model::{GameStatus, League, Query},
    render::{self, Format},
};
use chrono::NaiveDate;

fn app() -> App {
    let query = Query {
        date: NaiveDate::from_ymd_opt(2026, 7, 4).unwrap(),
        league: League::Mlb,
    };
    let mut app = App::new(
        query,
        Preferences::default(),
        Some(chrono_tz::America::New_York),
    );
    app.demo = true;
    app.accept(data::demo(query), DataState::Fresh);
    app
}

fn plain(app: &App, width: u16, height: u16) -> String {
    let mut output = Vec::new();
    render::write(
        &render::capture(app, width, height),
        Format::Plain,
        &mut output,
    )
    .unwrap();
    String::from_utf8(output).unwrap()
}

#[test]
fn layouts_keep_schedule_and_exit_controls_readable() {
    let app = app();
    for (width, height) in [
        (140, 42),
        (120, 32),
        (110, 28),
        (100, 30),
        (80, 24),
        (60, 20),
        (40, 12),
    ] {
        let screen = plain(&app, width, height);
        assert_eq!(screen.lines().count(), height as usize);
        assert!(screen.contains("BASEBALL HOUR"), "{width}x{height}");
        assert!(screen.contains("q Quit"), "{width}x{height}");
        assert!(screen.contains("PIT"), "{width}x{height}");
    }
    assert!(plain(&app, 30, 8).contains("Resize to at least"));
    let smallest = plain(&app, 40, 12);
    assert!(smallest.contains("2026-07-04"));
    assert!(smallest.contains("1 All"));
    assert!(smallest.contains("2 Following"));
    assert!(smallest.contains("3 Nearby"));
    assert!(smallest.contains("DEMO · Illustrative"));
    for size in [(1, 1), (1, 200), (500, 1)] {
        assert_eq!(plain(&app, size.0, size.1).lines().count(), size.1 as usize);
    }
}

#[test]
fn compact_help_keeps_close_and_quit_controls_visible() {
    let mut app = app();
    app.input = Input::Help;
    let screen = plain(&app, 40, 12);
    assert!(screen.contains("Esc Close · q Quit"));
    assert!(screen.contains("1/2/3 Views"));
}

#[test]
fn redirected_once_defaults_to_plain_but_explicit_ansi_is_retained() {
    let binary = env!("CARGO_BIN_EXE_baseballhour");
    let plain = std::process::Command::new(binary)
        .args(["--demo", "--once", "--size", "40x12"])
        .output()
        .unwrap();
    assert!(plain.status.success());
    assert!(!plain.stdout.contains(&0x1b));
    assert!(String::from_utf8_lossy(&plain.stdout).contains("BASEBALL HOUR"));

    let ansi = std::process::Command::new(binary)
        .args(["--demo", "--once", "--size", "40x12", "--format", "ansi"])
        .output()
        .unwrap();
    assert!(ansi.status.success());
    assert!(ansi.stdout.contains(&0x1b));
}

#[test]
fn nearby_shows_numeric_miles_in_compact_terminal() {
    let mut app = app();
    app.preferences.place = Some(resolve_place("Denver", app.games()).unwrap());
    app.view = View::Nearby;
    app.selected = None;
    app.normalize_selection();
    let screen = plain(&app, 80, 24);
    assert!(screen.contains("NEAR Denver"));
    assert!(screen.contains("1mi"));
    assert!(screen.contains("Coors Field"));
}

#[test]
fn postponed_games_show_a_matchup_and_alert_legend() {
    let mut app = app();
    let selected = app
        .games()
        .iter()
        .find(|g| g.status == GameStatus::Postponed)
        .unwrap()
        .id;
    app.selected = Some(selected);
    let screen = plain(&app, 140, 42);
    assert!(screen.contains("Postponed"));
    assert!(screen.contains("TB at HOU"));
    assert!(screen.contains("! Status alert"));
}

#[test]
fn empty_error_and_following_views_explain_recovery() {
    let mut app = app();
    app.view = View::Following;
    app.normalize_selection();
    assert!(plain(&app, 140, 42).contains("Your teams belong here"));
    app.view = View::All;
    app.snapshot = None;
    app.state = DataState::Failed("Request timed out".into());
    let screen = plain(&app, 140, 42);
    assert!(screen.contains("Could not load the schedule"));
    assert!(screen.contains("Press r to retry"));
    app.input = Input::Help;
    let help = plain(&app, 80, 24);
    assert!(help.contains("Keyboard guide"));
    assert!(help.contains("Esc Close · q Quit"));
    assert!(help.contains("Nearby: straight-line miles"));
}

#[test]
fn svg_exports_real_text_and_escapes_provider_names() {
    let mut app = app();
    app.snapshot.as_mut().unwrap().games[0].venue.name = "Park <A&B>".into();
    app.selected = Some(app.games()[0].id);
    let mut output = Vec::new();
    render::write(&render::capture(&app, 140, 42), Format::Svg, &mut output).unwrap();
    let svg = String::from_utf8(output).unwrap();
    assert!(svg.contains("Park &lt;A&amp;B&gt;"));
    assert!(svg.contains("width=\"1260\" height=\"798\""));
}

#[test]
fn cache_age_stays_visible_with_warnings_and_notices() {
    let mut app = app();
    app.demo = false;
    app.state = DataState::Cached("Offline mode".into());
    app.snapshot.as_mut().unwrap().fetched_at = app.now - chrono::Duration::seconds(90);
    app.snapshot.as_mut().unwrap().warnings = vec!["Skipped one malformed game".into()];
    for size in [(140, 42), (80, 24), (40, 12)] {
        assert!(plain(&app, size.0, size.1).contains("CACHED · 1m old"));
    }
    app.message = Some("Following the Nationals".into());
    assert!(plain(&app, 80, 24).contains("CACHED · 1m old"));
    app.state = DataState::Loading;
    assert!(plain(&app, 80, 24).contains("SAVED · 90s old"));
}

#[test]
fn historical_slates_do_not_use_todays_dst_abbreviation() {
    let mut app = app();
    app.timezone = Some(chrono_tz::Pacific::Auckland);
    app.now = "2026-10-02T12:00:00Z".parse().unwrap();
    app.selected = Some(823_526);
    let screen = plain(&app, 140, 42);
    assert!(screen.contains("Pacific/Auckland"));
    assert!(screen.contains("5:35 AM +1d"));
    assert!(!screen.contains("NZDT"));
}
