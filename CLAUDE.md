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

## User guide

`docs/guide/index.html` is the user guide: every screen, the analyses behind each figure, data sources, storage, backups and troubleshooting, with screenshots in `docs/guide/img/` (WebP, cropped from the demo station at 1600 × 1000 under Xvfb). Its tone is friendly with a little dry humour, at the owner's request. **Keep it current:** any change someone could see or would want to know about (a screen, a setting, a threshold, a file location) updates the guide in the same commit, with fresh screenshots where the picture changed and a line in its "About this guide" table. It's also published as a claude.ai artifact (https://claude.ai/artifact/4jBzHVTDK2YWYogeVK5BkX): republish it from a copy of `index.html` without its first three lines (the doctype and meta tags, which the artifact adds itself), with the images as files under `img/`.

GitHub Actions (`.github/workflows/build.yml`) runs the checks, builds both programs, and publishes them on the Releases page: "Latest build" for each push to `main`, a named release for a `vX.Y.Z` tag.

## State

Verified in a Linux cloud session (2026-10-17):
- `cargo test --workspace`: 75 tests pass (dayfile parsing, live mapping, URL normalisation, store, analytics, seasons and sun geometry, health checks, tank model and import, demo, settings, alerts, chart ticks, formatting).
- Every screen rendered with the demo station in light and dark (screenshots in `docs/`).
- End to end against a fake MX server: 2,665 days synced in three pages, live reading mapped, graph-data backfill filled the thermograph, and an unreachable station shows a clear message.
- The Windows `.exe` cross-compiles (12 MB, only system DLLs) and runs under Wine.

Verified by the owner on their station (2026-10-18, Cumulus MX 5.1.5 build 5012): runs on Linux and Windows, connects quickly, and the full history loads almost instantly. Every live field Isobar reads maps correctly from their `currentdata`, which is kept as the fixture `crates/isobar-core/tests/fixtures/currentdata-mx-5.1.5.json`. Isobar now also reads its sunrise and sunset, sunshine hours, clear-sky solar maximum, extra rain figures, indoor readings, `DataStopped`, the `Alarms` list (`{id, triggered}`) and `Version`/`Build`. Fields still unused include dew point and wind chill extremes, cloud base and Beaufort. Their sunrise and sunset on 18 October (6:36 and 19:45) put the station at about 38.6° S, which is how Seasons works out the latitude when Settings has none. Their `/api/graphdata/tempdata.json` is `{"intemp": [[ms, value], ...], ...}`: a reading a minute, UTC milliseconds, 72 hours deep (a sample is the fixture `graphdata-tempdata-mx-5.1.5.json`). Isobar refills from it at startup and after any gap over 15 minutes. MX 5 reports times as 12-hour text ("4:52 AM") and sends no compass point (`BearingCP`), so Isobar works that out from the bearing.

The owner's tanks (2026-10-18): 385 m² roof, two 22,500 L tanks (3,530 mm inside diameter), 290 L a day, a 3,000 L reserve, full with 5,023 L overflow on 3 Oct 2026. Their spreadsheet (`Rainfall.xlsx`, not in the repo: personal data) has Date in C, mm in D, Use in F and Adjust in L from row 13's headings, back to 2009, and models each day as level + mm × roof − use + adjust, capped at capacity, with 0.1 mm days catching nothing. `tanks.rs` reproduces it exactly (2026 to date: 233,580 L caught, 150,871 L overflow, no-rain run-out 18 Feb 2027 at their sheet's 43,000 L capacity).

MX's monthly logs (from the Cumulus MX source, checked 2026-10-19 against 5.1.6, same as 5.1.5): `GET /api/data/logfile?from=YYYY-MM-DD&to=YYYY-MM-DD&draw=1&start=N&length=N` returns `{"data": [[line, "dd/MM/yy HH:mm", "unix seconds", temp, hum, dew, wind, gust, avg bearing, rain rate, rain today, pressure, rain counter, in temp, in hum, latest gust, wind chill, heat index, UV, solar, ET, annual ET, apparent, solar max, sunshine, bearing, RG-11, rain since midnight, feels like, humidex, BGT, WBGT], ...], "recordsTotal": N}`, or an empty body when that month's file doesn't exist. Dates are meteorological days. Isobar stores them in the `log` table (values in hundredths as integers), at most one row per 10 minutes, and fetches a month a step. The owner's MX logs every minute (about 140 MB for five years before 10-minute storage, 2026-10-19), so each 10 minutes is combined on the way in: averages, the highest gust and rain rate, the latest bearing and rain totals, timed at the last reading; only the newest step is left as it is while it fills. A store from before this is thinned once at startup (meta `log_step`), then VACUUMed.

Not verified:
- Whether the owner's MX interface needs a login. Basic auth is supported but untested.

## Suggested order of work

Everything possible is listed in `docs/ROADMAP.md` with IDs (A1, D1, …), added in full on 2026-10-18 for the owner to trim. Work only on items the owner has said yes to, and tick them off there. The owner's own view tweaks go under Z. The owner also reviews a printable checklist page (a claude.ai artifact, "Isobar roadmap review") with the same IDs and a Done mark per item; update it when items are added or finished.
