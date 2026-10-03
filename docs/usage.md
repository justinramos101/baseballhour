# Use Baseball Hour

Baseball Hour starts in an interactive terminal and opens today's MLB schedule.

```sh
baseballhour
```

Use a UTF-8 terminal with color support. The full atlas works best at 110 by 28 cells or larger. An 80 by 24 terminal keeps the map, slate, and a compact game summary. Smaller terminals prioritize the schedule.

## Views

| View | Contents | Ordering |
| --- | --- | --- |
| All games | Every game in the selected slate | Live games, active delays, scheduled games, final games, then other states |
| Following | Games that involve a saved team | The same state and time order as All games |
| Nearby | Games with known venue coordinates | Straight-line distance, then start time |

One selection controls the slate, map marker, and game details. Following matches either team in a game. Nearby excludes games without venue coordinates and reports the number excluded. Those games remain in All games and Following.

## Keyboard controls

| Key | Action |
| --- | --- |
| `Up`, `k` | Select the previous game |
| `Down`, `j` | Select the next game |
| `Page Up`, `Page Down` | Move by eight games |
| `Home`, `End` | Select the first or last game |
| `Left`, `h`, `[` | Open the previous schedule date |
| `Right`, `]` | Open the next schedule date |
| `g` | Enter a schedule date in `YYYY-MM-DD` form |
| `t` | Return to today in the displayed timezone |
| `1` | Open All games |
| `2` | Open Following |
| `3` | Open Nearby, or ask for a place if none is saved |
| `/` | Search teams, abbreviations, cities, and ballparks in the loaded slate |
| `f` | Follow or unfollow either team in the selected game |
| `p`, `n` | Choose a nearby place |
| `l` | Choose a league |
| `Enter` | Open game details or confirm a dialog |
| `Esc` | Close the current dialog. Outside a dialog, clear search and return to All games |
| `r` | Refresh the current schedule |
| `?` | Open or close the keyboard guide |
| `q` | Quit outside a text entry field |
| `Ctrl-C` | Quit from any screen or dialog |

The league menu contains MLB, MLB plus MiLB, Triple-A, Double-A, High-A, and Single-A. Favorites and a nearby place persist between normal sessions. Demo mode keeps its choices only until exit.

## Nearby places

The place dialog accepts one of the built-in cities, a unique city or ballpark from the loaded slate, or coordinates in `latitude,longitude` form. Coordinates must fall within valid latitude and longitude bounds.

```text
Denver
Fenway Park
39.75,-104.99
```

The `--near` command-line option accepts a built-in city or coordinates. It resolves the place before the schedule loads, so it cannot resolve a ballpark from that schedule.

Distances use the great-circle distance between coordinates. They are straight-line estimates, not driving distances or travel times. Baseball Hour does not request your device location or send your chosen place to a geocoding service.

## Command-line options

| Option | Value | Behavior |
| --- | --- | --- |
| `--demo` | None | Use a labeled synthetic schedule without network access |
| `--date` | `YYYY-MM-DD` | Select the official schedule date between 1900 and 2200 |
| `--league` | `mlb`, `all`, `aaa`, `aa`, `high-a`, or `single-a` | Select the league set. The default is `mlb` |
| `--team` | Text | Filter teams, abbreviations, cities, and ballparks |
| `--near` | City or `latitude,longitude` | Open Nearby and sort by straight-line distance |
| `--offline` | None | Read saved schedules without making network requests |
| `--timezone` | IANA timezone | Display times in the selected timezone |
| `--once` | None | Print one rendered frame and exit |
| `--size` | `WIDTHxHEIGHT` | Set the `--once` frame size. The default is `140x42` and the maximum is `500x200` |
| `--format` | `plain`, `ansi`, or `svg` | Set the `--once` output format. With no explicit format, output is ANSI on a terminal and plain text when redirected |
| `--json` | None | Print the filtered normalized schedule as JSON and exit |
| `--ascii` | None | Use plain map markers in place of braille markers |
| `--cache-dir` | Directory | Override the schedule cache directory |
| `--config-dir` | Directory | Override the preferences directory |

`--json` and `--once` do not start the interactive interface. If both are present, `--json` takes precedence.

## Examples

Browse every supported league near Denver:

```sh
baseballhour --league all --near Denver
```

Open a selected Triple-A date:

```sh
baseballhour --league aaa --date 2026-07-04
```

Filter a live schedule and convert first pitch times:

```sh
baseballhour --team Mariners --timezone America/Denver
```

Export a plain compact frame from demo data:

```sh
baseballhour --demo --once --size 80x24 --format plain
```

Write an SVG capture:

```sh
baseballhour --demo --once --format svg > baseballhour.svg
```

Read a schedule as JSON:

```sh
baseballhour --date 2026-07-04 --json
```

## Schedule dates and timezones

`--date` and date navigation select the provider's official schedule date. Baseball Hour converts each first pitch time to the system timezone or the timezone from `--timezone`. A `+1d` suffix means the local start falls on the next calendar day. A `-1d` suffix means it falls on the previous calendar day.

`--timezone` accepts IANA names such as `America/Denver` and observes daylight saving transitions for the game date. Demo mode uses `America/New_York` unless you supply another timezone.

## Live, cached, offline, and demo data

Normal interactive sessions request schedules from the public MLB StatsAPI. Baseball Hour refreshes the current date and league every 30 seconds. The status line shows the age and source of the displayed data.

Each successful response is cached by schedule date and league selection. At startup, the app can display a matching cached schedule while it checks for current data. If a request fails, the app keeps that schedule and labels it `CACHED` with its age. It does not present cached data as live.

`--offline` makes no network requests. It requires a matching saved schedule for each selected date and league. If none exists, the app shows an error with recovery steps.

`--demo` uses bundled synthetic games, scores, and states. The interface labels demo data as illustrative. Demo mode does not read or write your saved preferences.

Schedules can omit scores, start times, probable pitchers, broadcasts, or venue coordinates. Baseball Hour shows an unknown value instead of inventing one. It keeps an available partial schedule and reports malformed entries as a warning.

## Local files

Baseball Hour stores preferences and schedule caches in the operating system's standard configuration and cache directories under `baseballhour`. Use `--config-dir` and `--cache-dir` to choose other directories.

The preferences file contains followed team IDs and the nearby place that you entered. Cache files contain the provider response for one date and league selection.
