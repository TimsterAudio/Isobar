# Isobar: context for Claude Code

Isobar was designed and first built in a claude.ai conversation. This file carries over the decisions and context from that conversation. The README covers architecture and setup; read it too.

## What the owner wants

- A native desktop replacement for an earlier single-file HTML dashboard (`Dashboard_2.html`) that charted Cumulus MX NOAA reports and showed a few live readings.
- Linux and Windows first. Built mainly for the owner, but good enough to share with other Cumulus users later, so avoid anything specific to one station: units come from the station, and the station address is a setting.
- Ambition is high ("go hard or go home"). Usability and looks matter as much as features.
- The owner's station is on a LAN; the MX interface is on port 8998. Location is Australia: day-first dates, °C, mm, km/h, hPa.

## Decisions already made (don't revisit without asking)

- **Stack:** Tauri 2 with a Rust backend and a TypeScript + Vite UI. uPlot for charts because it stays fast with 10+ years of daily data. No UI framework, just small view modules.
- **Data source:** the MX API, not NOAA text reports. Live data comes from `/api/data/currentdata` and history from `/api/data/dayfile` (paged), cached in SQLite. Reading `dayfile.txt` directly is a fallback for offline use or a station on the same machine.
- **Analytics live in TypeScript** (`src/lib/analytics.ts`); Rust handles fetching, parsing, storage, polling and alerts.
- **Anomalies** are measured against a day-of-year climatology (±7 days across other years), not one global mean. The old dashboard used a global mean and so flagged ordinary seasonal days.
- **Visual identity:** recording-instrument chart paper. Pale green-grey paper, slate ink, and a violet "pen" colour (`--trace`) reserved for the primary recorded series. Fonts are Barlow and Barlow Condensed, bundled locally. The large thermograph panel on the Now screen is the one bold element; keep everything else quiet. Design tokens are at the top of `src/styles.css`.
- **Copy style:** sentence case, plain words, active voice, specific error messages.

## State at handoff

Verified in a Linux sandbox:
- `crates/isobar-core`: 15 unit tests pass (dayfile parsing from file and API, live JSON mapping, URL normalisation, store).
- `src-tauri`: `cargo check` passes against WebKitGTK 4.1 with no warnings.
- The frontend type-checks and builds. Every screen was rendered with demo data in light and dark themes (screenshots in `docs/`).

Not verified:
- **Never run against a real MX instance.** Live field names in `live.rs` came from documentation and the old dashboard's code. Check them against real `currentdata` output first, and add the real responses as test fixtures.
- The `/api/graphdata/*.json` backfill format is assumed to be `{ "series": [[ms, value], ...] }`. Unconfirmed; failures are silent by design.
- Whether the owner's MX interface needs a login. Basic auth is supported but untested.
- No Windows build yet. Nothing in the code is Linux-specific.
- `npm run tauri dev` has not been run end to end; only `cargo check` and `vite build` have.

## Suggested order of work

1. Get `npm run tauri dev` running; fix any build or runtime errors.
2. Point it at the real station. Fix field mapping and add fixtures.
3. Confirm history sync works on the full dayfile, and check how long it takes.
4. Review each screen with real data and fix anything that looks wrong.
5. Set up a GitHub Actions workflow to build Windows and Linux installers.
6. Ideas the owner may want (ask first): a system-tray readout, a degree-days view, sunshine and ET, CSV export, storing the MX password in the OS keychain instead of `settings.json`.
