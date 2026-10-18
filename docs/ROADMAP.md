# Isobar roadmap

Everything that could be added, from the data the owner's Cumulus MX 5.1.5 station sends. Added in full on 2026-10-18 so the owner can trim it. The owner reviews it on a printable checklist page with the same IDs, and marks each item **Yes** (build it), *Later* (keep, not now) or no (drop it). Items with no mark are still undecided; don't build them.

## Decisions so far (2026-10-18)

- **Yes:** A1, A2, B1, B2, B4, B6, C1, Z1
- **Later:** A3, B5, B7, B8, D1, D2, D3
- **Undecided:** B3, E1, E2, E3, F1, F2, G1, G2, H1, H2, H3, I1, I2, J1, J2, J3, K1, K2, K3

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
- [x] **B4** Rain yesterday, this week, last 24 hours, storm total and start, last bucket tip. *Live* — **Yes**, done 2026-10-18. Owner's note: also the live rain rate, in mm/h or a unit chosen in Settings. The rate now always shows, in the station's unit per hour; which unit choices the owner wants is still to ask.
- [~] **B5** Wind run today and Beaufort force with description. *Live* — *Later*
- [x] **B6** Indoor temperature and humidity, with a 24-hour line against outside. *Live* — **Yes**, done 2026-10-18
- [~] **B7** Cloud base. *Live* — *Later*
- [~] **B8** Heat index, humidex and apparent temperature, with today's extremes. *Live* — *Later*

## C. Station health
- [ ] **C1** ★ Data health panel: dayfile gaps, `DataStopped`, triggered MX alarms, last bucket tip, MX version. *Build* — **Yes**

## D. Garden and growing
- [~] **D1** ★ Water balance: rain minus evapotranspiration, running total for the month or season, against normal. *Stored* — *Later* Owner's note: the household runs on water tanks. Show rain as a volume (litres or gallons) caught by the roof, with a tank capacity and level estimate and daily usage. The owner keeps tank history in a spreadsheet that can seed this; ask for it and for roof area and tank sizes when this starts.
- [~] **D2** Frost calendar: average first and last frost, frost nights a year, this year against them. *Stored* — *Later*
- [~] **D3** Growing degree days: season running total against other years. *Build* — *Later*

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

- [x] **Z1** The side rail slides out of the way, or can be pinned open. — **Yes**, done 2026-10-18

## Notes for building
- Most of D–J use dayfile fields already parsed into `DayRecord` but unused: `et`, `sunshine`, `hdd`, `cdd`, `wind_run`, `rain_hour_max`, `dew_*`, `hum_*`, `feels_*`, `apparent_*`. Check each against the station's real dayfile before relying on it; older lines leave the later fields empty.
- Wettest 24 hours, heat index, humidex, low wind chill and chill hours are in the dayfile but not parsed yet; add their positions in `crates/isobar-core/src/dayfile.rs` from the Cumulus wiki table.
- Put new statistics in `crates/isobar-core/src/analytics.rs` with tests, as the existing ones are.
