# Isobar roadmap

Everything that could be added, from the data the owner's Cumulus MX 5.1.5 station sends. Added in full on 2026-10-18 so the owner can trim it. The owner reviews it on a printable checklist page with the same IDs, and marks each item **Yes** (build it), *Later* (keep, not now) or no (drop it). Items with no mark are still undecided; don't build them.

## Decisions so far (2026-10-19)

- **Yes:** A1, A2, B1, B2, B4, B6, C1, D1, D2, D3, D4, F1, F2, G1, G2, I1, I2, Z1, Z2, Z3, Z4 (G1 to Z4 added on 2026-10-19)
- **Later:** A3, B5, B7, B8
- **Undecided:** B3, E1, E2, E3, H1, H2, H3, J1, J2, J3, K1, K2, K3, K4, L1, L2, L3, L4, Z5

Tags: **Stored** is already in the local history (no new downloading), **Live** is sent by MX in `currentdata` but not read yet (add it to `LiveReading` in `crates/isobar-core/src/live.rs` and check it against the fixture), **Build** is worked out by Isobar. ★ marks a suggested starting point.

Status: `[ ]` open, `[x]` done, `[-]` dropped, `[~]` later.

## A. Quick fixes
- [x] **A1** Wind dial uses the 10-minute average bearing when it's calm. *Live* — **Yes**, done 2026-10-18
- [x] **A2** 24-hour times, or a setting for it (MX 5 sends "4:52 AM"). *Build* — **Yes**, done 2026-10-18
- [~] **A3** Warn when MX's `DataStopped` flag is set, even while MX still answers. *Live* — *Later*

## B. More on Now
- [x] **B1** Sunrise, sunset and day length; moonrise and moonset. *Live* — **Yes**, done 2026-10-18
- [x] **B2** Sunshine hours today, and a cloudiness gauge from `SolarRad` against `CurrentSolarMax`. *Live* — **Yes**, done 2026-10-18
- [ ] **B3** Today's dew point and wind chill highs and lows with times. *Live*
- [x] **B4** Rain yesterday, this week, last 24 hours, storm total and start, last bucket tip. *Live* — **Yes**, done 2026-10-18. Owner's note: also the live rain rate, in mm/h or a unit chosen in Settings. The rate always shows, in mm/h (the station's unit per hour), which the owner confirmed is what they want.
- [~] **B5** Wind run today and Beaufort force with description. *Live* — *Later*
- [x] **B6** Indoor temperature and humidity, with a 24-hour line against outside. *Live* — **Yes**, done 2026-10-18
- [~] **B7** Cloud base. *Live* — *Later*
- [~] **B8** Heat index, humidex and apparent temperature, with today's extremes. *Live* — *Later*

## C. Station health
- [x] **C1** ★ Data health panel: dayfile gaps, `DataStopped`, triggered MX alarms, last bucket tip, MX version. *Build* — **Yes**, done 2026-10-18 as the Health page (also gaps in the last 24 hours of readings and the gap-filling status)

## D. Garden and growing
- [x] **D1** ★ Water balance: rain minus evapotranspiration, running total for the month or season, against normal. *Stored* — **Yes**, done 2026-10-18 on the Seasons page. Uses the station's ET, or a Hargreaves estimate from the temperatures and the latitude when the station doesn't record it.
- [x] **D2** Frost calendar: average first and last frost, frost nights a year, this year against them. *Stored* — **Yes**, done 2026-10-18
- [x] **D3** Growing degree days: season running total against other years. *Build* — **Yes**, done 2026-10-18
- [x] **D4** Water tanks (the owner's note on D1): the household runs on tanks. Rain as a volume caught by the roof (litres or gallons), with tank capacity, a level estimate and daily usage. *Build* — **Yes**, done 2026-10-18 as the Tanks page. Seeded from the owner's spreadsheet (rain, use and adjustments from 2009); the model matches it to the litre. Owner's figures: 385 m² roof, 2 × 22,500 L, 290 L a day, full with 5,023 L overflow on 3 Oct 2026. Their sheet uses 43,000 L capacity and keeps 3,000 L in reserve.

## E. Energy and comfort
- [ ] **E1** ★ Heating and cooling degree days by month and season, against normal and last year. *Stored*
- [ ] **E2** Comfort days by dew point band, counted by month. *Stored*
- [ ] **E3** Indoor against outdoor: how the house holds its heat. *Live*

## F. Sun
- [x] **F1** ★ Sunshine hours per month against normal, sunniest and dullest months on record. *Stored* — **Yes**, done 2026-10-18
- [x] **F2** Day length through the year. *Build* — **Yes**, done 2026-10-18, with average sunshine against it

## G. Rain
- [x] **G1** Rain intensity: share of rain in heavy bursts against drizzle; wettest hour on record. *Stored* — **Yes**, done 2026-10-19 on Insights, by the day's total (light, moderate, heavy, very heavy), with the wettest hours from the dayfile
- [x] **G2** Current dry spell against the longest on record for this time of year. *Stored* — **Yes**, done 2026-10-19 on Insights

## H. Wind
- [ ] **H1** Wind run per day and month. *Stored*
- [ ] **H2** Seasonal wind roses. *Stored*
- [ ] **H3** Gust factor: peak gust against average speed. *Stored*

## I. Pressure
- [x] **I1** Three-hour pressure tendency in standard words, with a marker for big drops. *Build* — **Yes**, done 2026-10-19
- [x] **I2** Biggest 24-hour pressure falls on record. *Stored* — **Yes**, done 2026-10-19 on Insights. The dayfile has only each day's high and low, so a fall is one day's high to the next day's low; one per storm

## J. Long-term climate
- [ ] **J1** ★ Warming trend: each year's average against normal, one bar a year. *Stored*
- [ ] **J2** Daily warm and cold records set each year. *Stored*
- [ ] **J3** Daily temperature range by month. *Stored*

## K. Bigger features
- [ ] **K1** Readout in the system tray. *Build*
- [ ] **K2** Export the daily history to CSV. *Build*
- [ ] **K3** MX password in the system keychain instead of `settings.json`. *Build*
- [ ] **K4** Detailed history from MX's monthly log files (a reading every few minutes, back as far as MX has them), synced like the dayfile. Gives true 24-hour pressure falls, hour-by-hour rain intensity and the thermograph for any past day. *Build*

## L. Reports
Added 2026-10-19 at the owner's request. Each report is a page Isobar writes as a single HTML file (opens in any browser, prints well, and the browser's Print → Save as PDF makes a PDF), with a straight-to-PDF option as a later step.
- [ ] **L1** ★ NOAA-style climate summaries, monthly and yearly: the familiar table of each day's mean, high, low, degree days, rain and wind, with totals and departures from normal. *Stored*
- [ ] **L2** Weather almanac for a year: the year in review, month by month against normal, records set, firsts and lasts (first frost, last 30° day), wettest and driest spells, and the tank year. *Stored*
- [ ] **L3** Monthly summary page: a one-page review of a month with its charts, ready to print or share. *Stored*
- [ ] **L4** Save straight to PDF from Isobar, without going through a browser. *Build*

## Z. The owner's own tweaks
Changes to the views and anything not listed, added as the owner sends them.

- [x] **Z1** The side rail slides out of the way, or can be pinned open. — **Yes**, done 2026-10-18
- [x] **Z2** Rain since midnight on the thermograph: a line with a fill underneath, on its own scale, starting again at midnight (not at the station's rain-day hour). — **Yes**, done 2026-10-18
- [x] **Z3** Choose which cards show under the thermograph: tick boxes in Settings. — **Yes**, done 2026-10-19, with a Tanks card added
- [x] **Z4** Drag a card by its name on Now to reorder; the order is remembered. — **Yes**, done 2026-10-19
- [ ] **Z5** Units of your choice (metric, imperial, or one by one: °C/°F, mm/in, km/h, m/s, mph, knots, hPa/inHg), as a setting that overrides the station's. By default Isobar keeps following the units MX reports. *Build*

## Notes for building
- Most of D–J use dayfile fields already parsed into `DayRecord` but unused: `et`, `sunshine`, `hdd`, `cdd`, `wind_run`, `rain_hour_max`, `dew_*`, `hum_*`, `feels_*`, `apparent_*`. Check each against the station's real dayfile before relying on it; older lines leave the later fields empty.
- Wettest 24 hours, heat index, humidex, low wind chill and chill hours are in the dayfile but not parsed yet; add their positions in `crates/isobar-core/src/dayfile.rs` from the Cumulus wiki table.
- Put new statistics in `crates/isobar-core/src/analytics.rs` with tests, as the existing ones are.
