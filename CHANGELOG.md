# Changelog

This file records user-visible changes to Baseball Hour.

## [0.2.0] - 2026-10-03

- Redrew the ballpark atlas with an Albers equal-area projection, shaded land, coastal water, a graticule, and a scale bar.
- Added a glow and sonar ping on the selected ballpark, pulsing live markers, and a dashed route from your Nearby place.
- Grouped the slate into live, upcoming, final, and schedule-change sections with two-line scoreboard cards, club color chips, and a scrollbar.
- Added a fuller scorebug with the inning-by-inning line, the base diamond, and the ball, strike, and out count.
- Cards briefly glow when a refresh changes a game's score.
- Added a date strip, view counts, keycap hints, and dimmed backdrops behind dialogs.
- Nearby now centers the map on your place with true-distance range rings. Slates outside North America get a wider view with real coastlines.
- Map labels keep clear of other ballparks, and detail scales down on small maps.
- Live slate cards show bases, outs, and the count. Upcoming cards show probable pitchers on wide panels.
- Enter opens a full game view with every inning, the count, game facts, and a locator map.
- Combined the header, tabs, and footer into three rows, and reserved green for live play.
- Added `--color` with 256-color fallback detection. `--ascii` now produces ASCII-only output.
- Demo dates before or after the demo day read as final or not yet played.

## [0.1.0] - 2026-10-02

Initial public release.

- Added interactive All games, Following, and Nearby views.
- Added MLB, Triple-A, Double-A, High-A, and Single-A schedules from the public MLB StatsAPI without an API key.
- Added live scores, game details, probable pitchers, broadcasts, status alerts, and actual venue locations when the provider supplies them.
- Added 30-second refreshes, honest cache fallback, and a network-free offline mode.
- Added a labeled synthetic demo and plain text, ANSI, SVG, and JSON output.
- Added native release archives for Intel and ARM64 macOS and Linux systems.

[0.2.0]: https://github.com/justinramos101/baseballhour/releases/tag/v0.2.0
[0.1.0]: https://github.com/justinramos101/baseballhour/releases/tag/v0.1.0
