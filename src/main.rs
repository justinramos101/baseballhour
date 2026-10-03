use std::{
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use baseballhour::{
    app::{self, Action, App, DataState, Preferences, View},
    data::{self, ScheduleClient},
    model::{League, Query, Snapshot, parse_date},
    render::{self, Format},
    ui,
};
use chrono::{Local, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use clap::{Parser, ValueEnum};
use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

#[derive(Parser)]
#[command(
    version,
    about = "Baseball schedules, live scores, and a ballpark atlas. No API key needed.",
    after_help = "Start with baseballhour. Arrows browse games and dates; ? shows the keys.\nTry baseballhour --demo for an offline tour with illustrative scores."
)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Each boolean represents an independent CLI switch"
)]
struct Args {
    #[arg(long, help = "Explore an illustrative slate without using the network")]
    demo: bool,
    #[arg(long, value_parser = parse_date, help = "Official schedule date, YYYY-MM-DD")]
    date: Option<NaiveDate>,
    #[arg(long, value_enum, default_value = "mlb")]
    league: League,
    #[arg(long, help = "Filter this day's teams or ballparks")]
    team: Option<String>,
    #[arg(
        long,
        help = "Sort by straight-line distance from a city or latitude,longitude"
    )]
    near: Option<String>,
    #[arg(long, help = "Use saved schedules only, without any network requests")]
    offline: bool,
    #[arg(
        long,
        help = "IANA timezone, such as America/Denver; default is your local zone"
    )]
    timezone: Option<Tz>,
    #[arg(long, help = "Print one frame and exit")]
    once: bool,
    #[arg(long, default_value = "140x42", value_parser = parse_size, help = "Dimensions for --once")]
    size: (u16, u16),
    #[arg(long, value_enum, help = "Output format for --once")]
    format: Option<Format>,
    #[arg(long, help = "Print a normalized schedule as JSON and exit")]
    json: bool,
    #[arg(long, help = "Draw with ASCII characters only")]
    ascii: bool,
    #[arg(
        long,
        value_enum,
        default_value = "auto",
        help = "Color depth; auto detects truecolor from COLORTERM and the terminal"
    )]
    color: ColorDepth,
    #[arg(long, help = "Override the schedule cache directory")]
    cache_dir: Option<PathBuf>,
    #[arg(long, help = "Override the local preferences directory")]
    config_dir: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum ColorDepth {
    Auto,
    Truecolor,
    #[value(name = "256")]
    Indexed,
}

/// Whether the terminal renders 24-bit color. Terminals that do usually say so
/// in `COLORTERM`; a few well-known ones do not.
fn truecolor(depth: ColorDepth) -> bool {
    match depth {
        ColorDepth::Truecolor => true,
        ColorDepth::Indexed => false,
        ColorDepth::Auto => {
            let var = |name: &str| std::env::var(name).unwrap_or_default().to_lowercase();
            let colorterm = var("COLORTERM");
            let term = var("TERM");
            let program = var("TERM_PROGRAM");
            colorterm.contains("truecolor")
                || colorterm.contains("24bit")
                || term.contains("direct")
                || ["kitty", "alacritty", "wezterm", "ghostty", "foot"]
                    .iter()
                    .any(|name| term.contains(name))
                || [
                    "iterm.app",
                    "wezterm",
                    "vscode",
                    "ghostty",
                    "tabby",
                    "hyper",
                ]
                .contains(&program.as_str())
                || std::env::var_os("WT_SESSION").is_some()
        }
    }
}

fn parse_size(value: &str) -> Result<(u16, u16), String> {
    value
        .split_once('x')
        .and_then(|(w, h)| w.parse::<u16>().ok().zip(h.parse::<u16>().ok()))
        .filter(|(w, h)| (1..=500).contains(w) && (1..=200).contains(h))
        .ok_or_else(|| "expected WIDTHxHEIGHT, up to 500x200".into())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let timezone = args
        .timezone
        .or_else(|| args.demo.then_some(chrono_tz::America::New_York));
    let today = timezone.map_or_else(
        || Local::now().date_naive(),
        |tz| Utc::now().with_timezone(&tz).date_naive(),
    );
    let date = args.date.unwrap_or_else(|| {
        if args.demo {
            NaiveDate::from_ymd_opt(2026, 7, 4).unwrap()
        } else {
            today
        }
    });
    let query = Query {
        date,
        league: args.league,
    };
    let config = args.config_dir.unwrap_or_else(|| {
        dirs::config_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("baseballhour")
    });
    let cache = args.cache_dir.unwrap_or_else(|| {
        dirs::cache_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("baseballhour")
    });
    let (preferences, notice) = if args.demo {
        (Preferences::default(), None)
    } else {
        app::load_preferences(&config)
    };
    let mut app = App::new(query, preferences, timezone);
    app.demo = args.demo;
    app.ascii = args.ascii;
    app.message = notice;
    app.search = args.team.unwrap_or_default();
    if let Some(place) = args.near {
        app.preferences.place = Some(app::resolve_place(&place, &[]).map_err(anyhow::Error::msg)?);
        app.view = View::Nearby;
    }
    if app.demo {
        app.now = demo_evening(date, timezone);
        set_demo(&mut app);
    }
    if args.json || args.once {
        if !app.demo {
            let client = ScheduleClient::new(Some(cache))?;
            let (snapshot, state) = load_once(&client, query, args.offline)?;
            app.accept(snapshot, state);
        }
        if args.json {
            let games = app.visible_games();
            serde_json::to_writer_pretty(
                io::stdout().lock(),
                &serde_json::json!({
                    "query": app.query, "demo": app.demo,
                    "source": match app.state { DataState::Cached(_) => "cache", _ if app.demo => "demo", _ => "network" },
                    "fetched_at": app.snapshot.as_ref().map(|s| s.fetched_at),
                    "warnings": app.snapshot.as_ref().map(|s| &s.warnings), "games": games,
                }),
            )?;
            println!();
        } else {
            let mut buffer = render::capture(&app, args.size.0, args.size.1);
            let format = args.format.unwrap_or_else(|| {
                if io::stdout().is_terminal() {
                    Format::Ansi
                } else {
                    Format::Plain
                }
            });
            if matches!(format, Format::Ansi) && !truecolor(args.color) {
                ui::quantize(&mut buffer);
            }
            render::write(&buffer, format, &mut io::stdout().lock())?;
        }
        return Ok(());
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!(
            "An interactive terminal is required. Use --once for a frame, or --json for schedule data."
        );
    }
    let worker = if app.demo {
        None
    } else {
        Some(start_worker(ScheduleClient::new(Some(cache))?))
    };
    run(
        app,
        worker.as_ref(),
        &config,
        args.offline,
        truecolor(args.color),
    )
}

/// Demo time is 7:10 PM on the demo date in the displayed timezone.
fn demo_evening(date: NaiveDate, timezone: Option<Tz>) -> chrono::DateTime<Utc> {
    let evening = date.and_hms_opt(19, 10, 0).unwrap_or_default();
    timezone
        .map_or_else(
            || {
                Local
                    .from_local_datetime(&evening)
                    .earliest()
                    .map(|t| t.to_utc())
            },
            |tz| {
                tz.from_local_datetime(&evening)
                    .earliest()
                    .map(|t| t.to_utc())
            },
        )
        .unwrap_or_else(|| Utc.from_utc_datetime(&evening))
}

/// Demo time stands still on the first demo day; other dates read as past or future.
fn set_demo(app: &mut App) {
    app.accept(
        data::demo_from(app.query, app.local_time(app.now).date_naive()),
        DataState::Fresh,
    );
}

fn load_once(
    client: &ScheduleClient,
    query: Query,
    offline: bool,
) -> Result<(Snapshot, DataState)> {
    if offline {
        return client
            .cached(query)
            .map(|s| (s, DataState::Cached("Offline mode".into())))
            .context("No saved schedule for this date and league. Connect once, or use --demo.");
    }
    match client.fetch(query) {
        Ok(snapshot) => Ok((snapshot, DataState::Fresh)),
        Err(error) => client
            .cached(query)
            .map(|s| (s, DataState::Cached(format!("{error}"))))
            .ok_or(error),
    }
}

#[derive(Clone, Copy)]
struct Request {
    generation: u64,
    query: Query,
    offline: bool,
}

struct Update {
    request: Request,
    result: Result<(Snapshot, DataState), String>,
    complete: bool,
}

type Worker = (mpsc::Sender<Request>, mpsc::Receiver<Update>);

fn start_worker(client: ScheduleClient) -> Worker {
    let (requests, receiver) = mpsc::channel::<Request>();
    let (results, updates) = mpsc::channel();
    thread::spawn(move || {
        while let Ok(mut request) = receiver.recv() {
            for newer in receiver.try_iter() {
                request = newer;
            }
            if !request.offline
                && let Some(snapshot) = client.cached(request.query)
                && results
                    .send(Update {
                        request,
                        result: Ok((snapshot, DataState::Cached("Checking for updates".into()))),
                        complete: false,
                    })
                    .is_err()
            {
                break;
            }
            let result =
                load_once(&client, request.query, request.offline).map_err(|e| e.to_string());
            if results
                .send(Update {
                    request,
                    result,
                    complete: true,
                })
                .is_err()
            {
                break;
            }
        }
    });
    (requests, updates)
}

struct TerminalGuard;

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

#[expect(clippy::too_many_lines, reason = "Keep the event loop together")]
fn run(
    mut app: App,
    worker: Option<&Worker>,
    config: &Path,
    offline: bool,
    truecolor: bool,
) -> Result<()> {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        previous_hook(info);
    }));
    enable_raw_mode()?;
    let _guard = TerminalGuard;
    execute!(io::stdout(), EnterAlternateScreen, Hide)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut generation = 1;
    let mut pending = (!app.demo).then_some(Instant::now());
    let mut in_flight = false;
    let mut last_refresh = Instant::now();
    let mut last_clock = Instant::now();
    let mut last_frame = Instant::now();
    let mut dirty = true;
    loop {
        if let Some((requests, updates)) = worker {
            if pending.is_some_and(|deadline| Instant::now() >= deadline) {
                requests.send(Request {
                    generation,
                    query: app.query,
                    offline,
                })?;
                pending = None;
                in_flight = true;
                last_refresh = Instant::now();
            }
            for update in updates.try_iter() {
                if update.request.generation != generation || update.request.query != app.query {
                    continue;
                }
                match update.result {
                    Ok((snapshot, state)) => {
                        app.accept(snapshot, state);
                    }
                    Err(error) => {
                        app.state = if app.snapshot.is_some() {
                            DataState::Cached(error)
                        } else {
                            DataState::Failed(error)
                        };
                    }
                }
                if update.complete {
                    in_flight = false;
                }
                dirty = true;
            }
            if !offline
                && !in_flight
                && pending.is_none()
                && last_refresh.elapsed() >= Duration::from_secs(30)
            {
                generation += 1;
                pending = Some(Instant::now());
            }
        }
        if last_clock.elapsed() >= Duration::from_secs(1) {
            if !app.demo {
                app.now = Utc::now();
            }
            last_clock = Instant::now();
            dirty = true;
        }
        if last_frame.elapsed() >= Duration::from_millis(100) && ui::animates(&app) {
            app.tick = app.tick.wrapping_add(1);
            last_frame = Instant::now();
            dirty = true;
        }
        if dirty {
            terminal.draw(|f| {
                ui::draw(f, &app);
                if !truecolor {
                    ui::quantize(f.buffer_mut());
                }
            })?;
            dirty = false;
        }
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    match app.handle_key(key) {
                        Action::Quit => break,
                        Action::Request => {
                            if app.demo {
                                set_demo(&mut app);
                            } else {
                                generation += 1;
                                pending = Some(Instant::now() + Duration::from_millis(160));
                                in_flight = false;
                            }
                        }
                        Action::Save if !app.demo => {
                            if let Err(error) = app::save_preferences(config, &app.preferences) {
                                app.message = Some(format!(
                                    "Could not save preferences: {error}. Kept for this session."
                                ));
                            }
                        }
                        _ => {}
                    }
                    dirty = true;
                }
                Event::Resize(_, _) => dirty = true,
                _ => {}
            }
        }
    }
    Ok(())
}
