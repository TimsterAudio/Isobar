# Isobar roadmap

Everything that could be added, from the data the owner's Cumulus MX 5.1.5 station sends. Added in full on 2026-10-18 so the owner can trim it; nothing here is agreed yet unless it's marked. The owner reviews it on a printable checklist page with the same IDs.

Tags: **Stored** is already in the local history (no new downloading), **Live** is sent by MX in `currentdata` but not read yet (add it to `LiveReading` in `crates/isobar-core/src/live.rs` and check it against the fixture), **Build** is worked out by Isobar. ★ marks a suggested starting point.

Status: `[ ]` open, `[x]` done, `[-]` dropped, `[~]` later.

## A. Quick fixes
- [ ] **A1** Wind dial uses the 10-minute average bearing when it's calm. *Live*
- [ ] **A2** 24-hour times, or a setting for it (MX 5 sends "4:52 AM"). *Build*
- [ ] **A3** Warn when MX's `DataStopped` flag is set, even while MX still answers. *Live*

## B. More on Now
- [ ] **B1** Sunrise, sunset and day length; moonrise and moonset. *Live*
- [ ] **B2** Sunshine hours today, and a cloudiness gauge from `SolarRad` against `CurrentSolarMax`. *Live*
- [ ] **B3** Today's dew point and wind chill highs and lows with times. *Live*
- [ ] **B4** Rain yesterday, this week, last 24 hours, storm total and start, last bucket tip. *Live*
- [ ] **B5** Wind run today and Beaufort force with description. *Live*
- [ ] **B6** Indoor temperature and humidity, with a 24-hour line against outside. *Live*
- [ ] **B7** Cloud base. *Live*
- [ ] **B8** Heat index, humidex and apparent temperature, with today's extremes. *Live*

## C. Station health
- [ ] **C1** ★ Data health panel: dayfile gaps, `DataStopped`, triggered MX alarms, last bucket tip, MX version. *Build*

## D. Garden and growing
- [ ] **D1** ★ Water balance: rain minus evapotranspiration, running total for the month or season, against normal. *Stored*
- [ ] **D2** Frost calendar: average first and last frost, frost nights a year, this year against them. *Stored*
- [ ] **D3** Growing degree days: season running total against other years. *Build*

## E. Energy and comfort
- [ ] **E1** ★ Heating and cooling degree days by month and season, against normal and last year. *Stored*
- [ ] **E2** Comfort days by dew point band, counted by month. *Stored*
- [ ] **E3** Indoor against outdoor: how the house holds its heat. *Live*

## F. Sun
- [ ] **F1** ★ Sunshine hours per month against normal, sunniest and dullest months on record. *Stored*
- [ ] **F2** Day length through the year. *Build*

## G. Rain
- [ ] **G1** Rain intensity: share of rain in heavy bursts against drizzle; wettest hour on record. *Stored*
- [ ] **G2** Current dry spell against the longest on record for this time of year. *Stored*

## H. Wind
- [ ] **H1** Wind run per day and month. *Stored*
- [ ] **H2** Seasonal wind roses. *Stored*
- [ ] **H3** Gust factor: peak gust against average speed. *Stored*

## I. Pressure
- [ ] **I1** Three-hour pressure tendency in standard words, with a marker for big drops. *Build*
- [ ] **I2** Biggest 24-hour pressure falls on record. *Stored*

## J. Long-term climate
- [ ] **J1** ★ Warming trend: each year's average against normal, one bar a year. *Stored*
- [ ] **J2** Daily warm and cold records set each year. *Stored*
- [ ] **J3** Daily temperature range by month. *Stored*

## K. Bigger features
- [ ] **K1** Readout in the system tray. *Build*
- [ ] **K2** Export the daily history to CSV. *Build*
- [ ] **K3** MX password in the system keychain instead of `settings.json`. *Build*

## Z. The owner's own tweaks
Changes to the views and anything not listed, added as the owner sends them.

## Notes for building
- Most of D–J use dayfile fields already parsed into `DayRecord` but unused: `et`, `sunshine`, `hdd`, `cdd`, `wind_run`, `rain_hour_max`, `dew_*`, `hum_*`, `feels_*`, `apparent_*`. Check each against the station's real dayfile before relying on it; older lines leave the later fields empty.
- Wettest 24 hours, heat index, humidex, low wind chill and chill hours are in the dayfile but not parsed yet; add their positions in `crates/isobar-core/src/dayfile.rs` from the Cumulus wiki table.
- Put new statistics in `crates/isobar-core/src/analytics.rs` with tests, as the existing ones are.
