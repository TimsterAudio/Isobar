# Isobar: context for Claude Code

Isobar was designed and first built in a claude.ai conversation, then rewritten as a single native Rust program (0.2.0). This file carries the decisions and context forward. The README covers architecture and setup; read it too.

## What the owner wants

- A native desktop replacement for an earlier single-file HTML dashboard (`Dashboard_2.html`) that charted Cumulus MX NOAA reports and showed a few live readings.
- **One program, nothing else to install.** A single `.exe` on Windows and a single binary on Linux, easy for the owner and for anyone they share it with to run and test. This is why 0.2.0 replaced Tauri (see below).
- Linux and Windows first. Built mainly for the owner, but good enough to share with other Cumulus users later, so avoid anything specific to one station: units come from the station, and the station address is a setting.
- Ambition is high ("go hard or go home"). Usability and looks matter as much as features.
- The owner's station is on a LAN; the MX interface is on port 8998. Location is Australia: day-first dates, °C, mm, km/h, hPa.

## Decisions already made (don't revisit without asking)

- **Stack:** pure Rust. eframe/egui 0.36 for the window, matching the owner's TAW apps; charts, the wind dial and wind roses are drawn by hand in `src/chart.rs` and `src/dial.rs`. No web view, no Node. (The first version was Tauri 2 with a TypeScript + Vite UI and uPlot; the owner chose on 2026-10-17 to rewrite it as one native program. That version is kept on the `tauri-original` branch.)
- **Data source:** the MX API, not NOAA text reports. Live data comes from `/api/data/currentdata` and history from `/api/data/dayfile` (paged), cached in SQLite. Reading `dayfile.txt` directly is a fallback for offline use or a station on the same machine.
- **Where code lives:** `crates/isobar-core` holds everything that doesn't need a window and is unit tested: parsing, the MX client (blocking HTTP, run off the UI thread), storage, analytics (`analytics.rs`, ported from the first version's TypeScript) and the demo station. The app in `src/` is the window, the background worker (`worker.rs`: polling, samples, alerts, sync) and the screens.
- **Anomalies** are measured against a day-of-year climatology (±7 days across other years), not one global mean. The old dashboard used a global mean and so flagged ordinary seasonal days.
- **Visual identity:** recording-instrument chart paper. Pale green-grey paper, slate ink, and a violet "pen" colour (`trace`) reserved for the primary recorded series. Fonts are Barlow and Barlow Condensed, built into the binary from `assets/fonts`. The large thermograph panel on the Now screen is the one bold element; keep everything else quiet. Colours are in `src/theme.rs`. Isobar keeps its own look rather than the TAW hi-fi look.
- **Copy style:** sentence case, plain words, active voice, specific error messages.
- **Files:** the same folders as the first version (`app.isobar.desktop` under the config and data directories), so settings and history carry over.

## Checks before pushing

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo clippy --target x86_64-pc-windows-gnu -- -D warnings
```

For anything visible, run it and look: `cargo run -- --demo` (the demo station) under Xvfb in a cloud session, in light and dark (Settings → Appearance). Xvfb also needs `libxkbcommon-x11-0` installed. To test the real path without a station, a small fake MX server serving `currentdata` and a paged `dayfile` works well.

GitHub Actions (`.github/workflows/build.yml`) runs the checks, builds both programs, and publishes them on the Releases page: "Latest build" for each push to `main`, a named release for a `vX.Y.Z` tag.

## State

Verified in a Linux cloud session (2026-10-17):
- `cargo test --workspace`: 38 tests pass (dayfile parsing, live mapping, URL normalisation, store, analytics, demo, settings, alerts, chart ticks, formatting).
- Every screen rendered with the demo station in light and dark (screenshots in `docs/`).
- End to end against a fake MX server: 2,665 days synced in three pages, live reading mapped, graph-data backfill filled the thermograph, and an unreachable station shows a clear message.
- The Windows `.exe` cross-compiles (12 MB, only system DLLs) and runs under Wine.

Verified by the owner on their station (2026-10-18, Cumulus MX 5.1.5 build 5012): runs on Linux and Windows, connects quickly, and the full history loads almost instantly. Every live field Isobar reads maps correctly from their `currentdata`, which is kept as the fixture `crates/isobar-core/tests/fixtures/currentdata-mx-5.1.5.json`. That response also carries fields Isobar doesn't use yet (sunrise and sunset, dew point and wind chill extremes, cloud base, sunshine hours, rain this week and last 24 hours, indoor readings, Beaufort, `DataStopped`), handy when adding data points. MX 5 reports times as 12-hour text ("4:52 AM") and sends no compass point (`BearingCP`), so Isobar works that out from the bearing.

Not verified:
- The `/api/graphdata/*.json` backfill format is assumed to be `{ "series": [[ms, value], ...] }`. Unconfirmed; failures are silent by design.
- Whether the owner's MX interface needs a login. Basic auth is supported but untested.

## Suggested order of work

1. The owner's list of extra data points and view tweaks (coming).
2. Ideas the owner may want (ask first): a system-tray readout, a degree-days view, sunshine and ET, CSV export, storing the MX password in the OS keychain instead of `settings.json`.
