# Changelog

## Unreleased

- **Wind in light air (A1):** when the average wind is below about 3 km/h (or the same in the station's units), the dial and its text use MX's 10-minute average bearing, which reads truer than the latest. The text says "Calm" or "Light" when it does.
- **24-hour times (A2):** times show as 16:52 by default, including chart axes and the station's own times, which MX 5 sends as "4:52 PM". Settings → Appearance → Times switches to 12-hour.
- **Side rail (Z1):** the « button beside the name hides the rail to a narrow strip; point at the strip to slide the rail out over the screen, and use the pin to keep it open. Isobar remembers the choice.

## 0.2.0 – 2026-10-17

A rewrite as one native Rust program. Isobar is now a single `isobar.exe` on Windows and a single `isobar` on Linux, with nothing else to install: no web view, no Node.

- The window is eframe/egui. Charts, the wind dial and wind roses are drawn by hand in the chart-paper look, with the same screens as before: Now, History, Compare, Insights and Settings.
- Charts show a read-out on hover, and on History and Compare you can drag across one to zoom in (double-click to zoom out).
- The analytics (normals, anomalies, summaries, records, the climate table) moved from TypeScript into `isobar-core`, with tests.
- **Try the demo station** on first run, or `isobar --demo`, shows eight years of made-up weather without touching your settings.
- Settings shows "Unsaved changes", checks the alert boxes are numbers before saving, and asks before deleting local history.
- Settings and history stay where the first version kept them, so nothing needs setting up again.
- GitHub Actions builds both programs and publishes them on the Releases page.

## 0.1.0 – 2026-10-17

The first version, built in a claude.ai conversation: Tauri 2 with a TypeScript UI. Kept on the `tauri-original` branch.
