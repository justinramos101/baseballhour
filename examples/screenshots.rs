use std::{fs, path::PathBuf};

use baseballhour::{
    app::{App, DataState, Input, Preferences, View, resolve_place},
    data,
    model::{League, Query},
    render::{self, Format},
};
use chrono::{NaiveDate, TimeZone, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn save(
    app: &App,
    directory: &std::path::Path,
    name: &str,
    size: (u16, u16),
) -> anyhow::Result<()> {
    let buffer = render::capture(app, size.0, size.1);
    render::write(
        &buffer,
        Format::Svg,
        &mut fs::File::create(directory.join(format!("{name}.svg")))?,
    )?;
    render::write(
        &buffer,
        Format::Plain,
        &mut fs::File::create(directory.join(format!("{name}.txt")))?,
    )?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .unwrap_or_else(|| "docs/screenshots".into()),
    );
    fs::create_dir_all(&directory)?;
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
    app.now = Utc.with_ymd_and_hms(2026, 7, 4, 23, 10, 0).unwrap();
    app.accept(data::demo(query), DataState::Fresh);
    save(&app, &directory, "atlas", (140, 42))?;
    save(&app, &directory, "compact", (80, 24))?;
    app.input = Input::Help;
    save(&app, &directory, "help", (80, 24))?;
    app.input = Input::Normal;
    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
    save(&app, &directory, "following", (140, 42))?;
    app.query.league = League::All;
    app.accept(data::demo(app.query), DataState::Fresh);
    app.preferences.place = Some(resolve_place("Denver", app.games()).map_err(anyhow::Error::msg)?);
    app.view = View::Nearby;
    app.selected = None;
    app.normalize_selection();
    save(&app, &directory, "nearby", (140, 42))?;
    save(&app, &directory, "nearby-compact", (80, 24))?;
    println!(
        "Captured All games, Following, Nearby, compact layouts, and help in {}",
        directory.display()
    );
    Ok(())
}
