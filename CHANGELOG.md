# Changelog

## Unreleased

- **Detailed history from MX's logs (K4):** Isobar fetches MX's monthly log files (a reading every 10 minutes or so) in the background, newest month first, and keeps the current month topped up. Health shows the progress. With them, Insights gives the true biggest 24-hour pressure falls (with the time of the low) and splits each year's rain by how hard it fell, in mm an hour.
- **Any card in the big chart (Z6):** click a card on Now to show it in the chart above, over a span that suits it: pressure over 3 days, rain as daily totals over 7 days, tanks over 3 months, the rest over 24 hours. Chips change the span (up to 30 days, 3 months for rain, a year for tanks); click the card again, or Temperature, to go back. Long spans use MX's logs, or daily highs and lows until they've arrived.
- **Choose and arrange the cards on Now (Z3, Z4):** Settings → Now screen turns cards on and off; drag a card by its name on Now to move it. Isobar remembers the order, and Settings can put it back.
- **Tanks card on Now:** litres, how full, days of water with no rain, and the last 30 days caught and overflowed, with a 30-day line.
- **Rain intensity (G1):** Insights shows each year's rain split by how heavy the day was (light, moderate, heavy, very heavy), and the wettest hours on record.
- **Dry spell (G2):** dry days in a row to the last day on record, against the longest dry spell at this time of year and on record, and how many years had one this long now.
- **Biggest pressure falls (I2):** the ten deepest drops from one day's highest pressure to the next day's lowest, one per storm, with that day's peak gust and rain.
- **Three-hour pressure tendency (I1):** the pressure card says how pressure has moved over three hours in forecasters' words (steady, rising slowly, falling quickly and so on), with the change. A fall of 3.6 hPa or more shows in amber.
- **Rainwater tank settings are clearer:** the example values in the boxes read "e.g. 385", so they don't look filled in; a note says when the tank details aren't saved yet; and an import that can't open the file suggests closing it in Excel first.
- **Windows file details:** the .exe now carries its company, copyright, version and original file name, and a standard application manifest, so it looks less unfamiliar to antivirus checks. Each release also lists SHA-256 checksums (`SHA256SUMS.txt`) to check a download against.
- **Wind in light air (A1):** when the average wind is below about 3 km/h (or the same in the station's units), the dial and its text use MX's 10-minute average bearing, which reads truer than the latest. The text says "Calm" or "Light" when it does.
- **24-hour times (A2):** times show as 16:52 by default, including chart axes and the station's own times, which MX 5 sends as "4:52 PM". Settings → Appearance → Times switches to 12-hour.
- **Sun (B1, B2):** sunrise, sunset and day length (moonrise and moonset on hover), sunshine hours today, and a cloudiness gauge: the sunshine now against what a clear sky would give at this time of day, read as clear, hazy, cloudy or overcast.
- **Rain (B4):** the rain rate always shows, plus yesterday, the last 24 hours, this week, the current storm's total and start, and when it last rained.
- **Indoor (B6):** a sixth card with indoor temperature and humidity, how it compares with outside, and a 24-hour line of inside against outside. It appears when the station reports indoor readings.
- **Rain on the thermograph (Z2):** rain since midnight as a line with a fill underneath, on its own scale at the right. Isobar adds up the rain itself, so the total starts again at midnight even when the station's rain day starts at 9am. It shows once there's been rain in the last 24 hours.
- **Compact cards on Now:** the instrument cards are smaller (readings, wind dial, sparklines and text), sized to their content, and sit in one row on any window from about 1,300 px wide, so the whole of Now fits on a 1080p screen, or a 1440 × 900 one. Now also uses more of a wide screen than the other pages.
- **Gaps in the 24-hour charts:** a stretch of more than 20 minutes with no readings (Isobar closed, or the station offline) now shows as a break, not a straight line across it.
- **Filling gaps from MX:** Isobar fetches MX's recent detail (up to three days, a reading a minute) when it starts and again after any gap of more than 15 minutes, such as the computer sleeping or the station being unreachable, so the 24-hour charts fill in. Settings → History shows how many readings the last fill brought, or why MX didn't send them.
- **Dashed lines** (the dew point on the thermograph) stay even with a reading a minute; before, they broke up into dots.
- **Seasons page (D1, D2, D3, F1, F2):** a new page for the year's rhythm.
  - *Water balance:* rain less evaporation as a running total for this month, season or year, against the normal and the range of other years. It uses the station's own evapotranspiration, or estimates it from each day's temperatures when the station doesn't record it.
  - *Frost:* the average first and last frost, frost nights a year and this year so far, with a calendar of every frost night on record. Choose 0° or 2° (nights that frost the grass).
  - *Growing degree days:* this growing year's running total against every other year, with base 10° or 5°.
  - *Sunshine:* hours each month against normal, the sunniest and dullest months, and a table by calendar month.
  - *Day length:* through the year at the station's latitude, with the average sunshine against it.
  - Seasons follow the station's hemisphere.
- **Tanks page (D4):** for houses on rainwater. How full the tanks are, worked out day by day from the rain, the roof area and what the house uses; how many days the water lasts with no rain; and whether it would last a year with the rain of each past year. A chart shows the level for the last year (or everything) and a year ahead, and a table gives each year's rain, litres caught, used and overflowed, and the lowest level. The level chart covers 3 months, 12 months, everything or chosen dates (an end date after today adds the outlook up to it). Overflow shows as bars on the level chart, in the last 30 days, and for the year ahead (what the past years' rain would spill). Set it up in Settings → Rainwater tanks: roof area, one to four tanks (each with its own size and, optionally, inside diameter; they are taken as linked), daily use, a level to keep above, and a known level ("full", litres, or a measured depth such as 1350 mm).
- **Import a tank record:** a spreadsheet you've kept (Excel, LibreOffice or CSV) with Date and mm columns, and optionally Use and Adjust. Its rain fills in the days before your station's history, and its daily use and adjustments replace the usual figure on those days.
- **Latitude (Settings → Station):** used for day length and the evaporation estimate. Leave it empty and Isobar works it out from the day's sunrise and sunset.
- **Health page (C1):** whether it's the station or the weather. It shows the link to MX, MX's own "data stopped" flag, its alarms, its version, when the rain gauge last tipped, missing and incomplete days in the history, and gaps in the last 24 hours of readings. A dot beside Health in the rail means something needs a look. Update notices from MX don't count.
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
