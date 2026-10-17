# Isobar

A desktop companion for [Cumulus MX](https://cumuluswiki.org/a/Cumulus_MX) weather stations, for Linux and Windows. Isobar reads live conditions and your station's full daily history straight from MX, keeps a local copy, and turns it into charts, comparisons and records.

![Now screen](docs/now-light.png)

## Screens

- **Now**: the current temperature with its trend, a 24-hour temperature and dew point trace, and wind, pressure, rain, humidity and sun readouts with 24-hour sparklines. Shows a warning when the data goes stale or the station can't be reached.
- **History**: range presets, a summary compared with your station's own normals ("rain 87% of normal"), temperature, rain, wind and pressure charts, and a year heatmap of each day against normal. Ranges longer than about 200 days switch to weekly values, and longer than three years to monthly values.
- **Compare**: one month (or the whole year) overlaid for every year on record with a ranking table, or two date ranges head-to-head with wind roses.
- **Insights**: unusual days measured against the seasonal normal for that date, a records board, a monthly climate table and prevailing-wind roses.
- **Settings**: station address, optional login, a dayfile path for offline use, alerts and theme.

Desktop notifications cover strong gusts, frost, heat, and rain starting after three dry hours.

## How it talks to Cumulus MX

| What | Where it comes from |
| --- | --- |
| Live conditions | `GET /api/data/currentdata`, every 10 s by default |
| Daily history | `GET /api/data/dayfile?draw=1&start=N&length=1000`, paged; re-checked every 30 minutes and re-downloaded when the line count changes |
| Recent detail (best effort) | `GET /api/graphdata/{temp,press,wind,wdir,rain,hum,solar}data.json`, on first connect |

Isobar also stores one live sample a minute (21 days are kept), so the 24-hour charts fill in even if the graph-data endpoints aren't available.

Without a station address, Isobar can read `dayfile.txt` directly and re-reads it whenever the file changes. *Settings → Import a dayfile* merges an archive copy (for example, from an earlier station location) into the local history.

Field positions follow the [Cumulus wiki dayfile table](https://cumuluswiki.org/a/Dayfile.txt). Older lines with fewer fields, semicolon separators and decimal commas are all handled.

## Layout

```
crates/isobar-core/   Rust library: dayfile parser, MX client, SQLite store (unit tested)
src-tauri/            Tauri 2 app: background poller, alerts, settings, commands
src/                  TypeScript UI (Vite, uPlot)
  lib/analytics.ts      normals, anomalies, records, aggregation, cross-year series
  lib/demo.ts           synthetic station for working on the UI in a browser
  views/                Now, History, Compare, Insights, Settings
```

## Running it

1. Install the Tauri prerequisites for your platform: <https://tauri.app/start/prerequisites/>.
   - **Linux:** WebKitGTK 4.1 and build tools, e.g. on Debian/Ubuntu `sudo apt install libwebkit2gtk-4.1-dev build-essential libssl-dev libayatana-appindicator3-dev librsvg2-dev`.
   - **Windows:** Microsoft C++ Build Tools. WebView2 is already present on Windows 10 and 11.
   - **Both:** Rust from <https://rustup.rs> and Node.js 20 or newer.
2. Install and start:
   ```sh
   npm install
   npm run tauri dev
   ```
3. Open **Settings**, enter your MX address (for example `http://192.168.1.20:8998`), choose **Test connection**, then **Save settings**. History downloads in the background; the rail at the bottom left shows progress.

To work on the interface without a station, run `npm run dev` and open <http://localhost:1420> in a browser. It runs on synthetic data.

Run the core tests with `cd crates/isobar-core && cargo test`.

Build installers with `npm run tauri build`. Output goes to `src-tauri/target/release/bundle/`: `.deb`, `.rpm` and AppImage on Linux, NSIS and MSI on Windows. Build each platform on that platform.

## Where Isobar keeps things

- **Settings:** `settings.json` in the app config directory (`~/.config/app.isobar.desktop/` on Linux, `%APPDATA%\app.isobar.desktop\` on Windows). If you set an MX password, it's stored here **unencrypted**.
- **Data:** `isobar.sqlite` in the app data directory. Delete it to start fresh; it rebuilds from MX.

## Known gaps and next steps

- **Live field names:** these were mapped from MX documentation and an earlier dashboard. If a reading shows "–" on Now, compare the station's `/api/data/currentdata` output with the key lists in `crates/isobar-core/src/live.rs`.
- **Graph-data endpoints:** these vary between MX versions, so they're treated as optional.
- **Units:** taken from the station's live data (`TempUnit` and so on). Charts assume history uses the same units.
- **Ideas:** a system tray readout, a degree-days view, a sunshine/ET panel, CSV export, and secure password storage in the OS keychain.
