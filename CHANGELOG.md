# Changelog

## Unreleased

- **Wind in light air (A1):** when the average wind is below about 3 km/h (or the same in the station's units), the dial and its text use MX's 10-minute average bearing, which reads truer than the latest. The text says "Calm" or "Light" when it does.
- **24-hour times (A2):** times show as 16:52 by default, including chart axes and the station's own times, which MX 5 sends as "4:52 PM". Settings → Appearance → Times switches to 12-hour.
- **Sun (B1, B2):** sunrise, sunset and day length (moonrise and moonset on hover), sunshine hours today, and a cloudiness gauge: the sunshine now against what a clear sky would give at this time of day, read as clear, hazy, cloudy or overcast.
- **Rain (B4):** the rain rate always shows, plus yesterday, the last 24 hours, this week, the current storm's total and start, and when it last rained.
- **Indoor (B6):** a sixth card with indoor temperature and humidity, how it compares with outside, and a 24-hour line of inside against outside. It appears when the station reports indoor readings; with six cards, Now shows them in two rows of three.
- **Gaps in the 24-hour charts:** a stretch of more than 20 minutes with no readings (Isobar closed, or the station offline) now shows as a break, not a straight line across it.
- **Filling gaps from MX:** Isobar fetches MX's recent detail (up to three days, a reading a minute) when it starts and again after any gap of more than 15 minutes, such as the computer sleeping or the station being unreachable, so the 24-hour charts fill in. Settings → History shows how many readings the last fill brought, or why MX didn't send them.
- **Dashed lines** (the dew point on the thermograph) stay even with a reading a minute; before, they broke up into dots.
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
