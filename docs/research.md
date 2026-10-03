# Research notes

## What informed the product

[Terrahour](https://github.com/ACoci86/terrahour) uses a custom Python terminal renderer and a braille land mask. Its map and compact mode establish the visual baseline. Baseball Hour uses the map to identify actual game venues, with a synchronized schedule and game details.

[awesome-tuis](https://github.com/rothgar/awesome-tuis) led to comparisons of dashboard layouts and terminal frameworks. [bottom](https://github.com/ClementTsang/bottom) demonstrates dense, responsive terminal panels and documents macOS and Linux support. [btop](https://github.com/aristocratos/btop) provides a useful reference for strong information hierarchy, restrained borders, and visible controls. [gh-dash](https://github.com/dlvhdr/gh-dash/blob/main/README.md) informed the shared list-and-detail interaction.

[Ratatui's canvas documentation](https://docs.rs/ratatui/latest/ratatui/widgets/canvas/index.html) describes braille drawing, geographic maps, points, and lines. Its shared buffer renderer supports repeatable visual inspection. [Bubble Tea](https://github.com/charmbracelet/bubbletea) was the main alternative for a distributable native executable. The architecture comparison favors Ratatui for the cartographic drawing and shared interactive and snapshot rendering.

## Public baseball data

The [MLB sports endpoint](https://statsapi.mlb.com/api/v1/sports) identifies MLB as sport 1, Triple-A as 11, Double-A as 12, High-A as 13, and Single-A as 14.

The schedule request uses `/api/v1/schedule`, the selected sport and official schedule date, and `hydrate=team,venue(location),linescore,probablePitcher,broadcasts`. Credential-free requests made during this work returned the following results for July 4, 2026.

| Scope | Games | Venue locations |
| --- | ---: | ---: |
| MLB | 15 | 15 |
| Triple-A | 17 | 17 |
| Double-A, High-A, Single-A | 47 | 47 |

These are observed examples, not a guarantee that all games contain coordinates. Missing locations must remain visible in the schedule. Hydrated team records, probable pitchers, innings, and broadcast names can be absent. Unknown scores must remain unknown. Postponements must override a generic preview status. A game's venue takes precedence over a team's home stadium.

The app displays the provider's official schedule date. Start times convert from UTC to the user's timezone. A start on another local calendar day needs a day-offset label. A public endpoint can change without notice, so data parsing remains separate from UI state and has recorded response tests.

## Geographic data

[Natural Earth](https://www.naturalearthdata.com/about/terms-of-use/) publishes public-domain map data. The build uses [110m country outlines](https://github.com/nvkelso/natural-earth-vector/blob/master/geojson/ne_110m_admin_0_countries.geojson) to generate a bundled map. Runtime map downloads and map service accounts are unnecessary.

The [architecture guide](architecture.md) describes how the app separates provider data, interaction state, and rendering.
