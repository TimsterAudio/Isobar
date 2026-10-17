//! Compare: one month (or the whole year) overlaid for every year on record
//! with a ranking table, or two date ranges head to head with wind roses.

use chrono::{Datelike, Duration, NaiveDate};
use eframe::egui::{Align, Layout, Ui};
use isobar_core::analytics::{self as an, CompareMetric, Field, Kind, YearSeries, MONTH_STARTS};
use isobar_core::DayRecord;

use crate::app::{Route, Snapshot};
use crate::chart::{Chart, Series, XAxis};
use crate::dial;
use crate::format::{self, nice_date, num, signed, MONTHS, MONTHS_LONG};
use crate::theme::{self, alpha, Palette};
use crate::ui::{self, cell, col};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Years,
    Periods,
}

struct YearsCache {
    key: (u64, Option<u32>, CompareMetric),
    series: Vec<YearSeries>,
}

pub struct Compare {
    mode: Mode,
    /// 1–12, or `None` for the whole year.
    month: Option<Option<u32>>,
    metric: CompareMetric,
    highlight: Option<i32>,
    periods: Option<[NaiveDate; 4]>,
    cache: Option<YearsCache>,
}

impl Default for Compare {
    fn default() -> Self {
        Compare {
            mode: Mode::Years,
            month: None,
            // Rain so far this month is the most-asked question.
            metric: CompareMetric::RainCum,
            highlight: None,
            periods: None,
            cache: None,
        }
    }
}

type Quick = (&'static str, fn(NaiveDate) -> [NaiveDate; 4]);

const QUICK: [Quick; 3] = [
    ("This month vs same month last year", |l| {
        let s = format::first_of_month(l);
        [format::add_years(s, -1), format::add_years(l, -1), s, l]
    }),
    ("This year vs last year, to date", |l| {
        let s = format::first_of_year(l);
        [format::add_years(s, -1), format::add_years(l, -1), s, l]
    }),
    ("Last 30 days vs the 30 before", |l| {
        [
            l - Duration::days(59),
            l - Duration::days(30),
            l - Duration::days(29),
            l,
        ]
    }),
];

impl Compare {
    pub fn ui(&mut self, ui: &mut Ui, snap: &Snapshot, go: &mut Option<Route>) {
        ui.horizontal(|ui| {
            ui::h1(ui, "Compare");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if ui::chip(ui, "Two periods", self.mode == Mode::Periods).clicked() {
                    self.mode = Mode::Periods;
                }
                if ui::chip(ui, "Across years", self.mode == Mode::Years).clicked() {
                    self.mode = Mode::Years;
                }
            });
        });
        let days = snap.days.as_slice();
        let Some(last) = days.last().map(|d| d.date) else {
            ui::panel(ui, |ui| {
                if ui::empty(
                    ui,
                    "Nothing to compare yet",
                    "Comparisons appear once Isobar has your station's daily history.",
                    Some("Open settings"),
                ) {
                    *go = Some(Route::Settings);
                }
            });
            return;
        };
        match self.mode {
            Mode::Years => self.years(ui, snap, last),
            Mode::Periods => self.periods(ui, snap, days, last),
        }
    }

    fn years(&mut self, ui: &mut Ui, snap: &Snapshot, last: NaiveDate) {
        let p = Palette::of(ui.ctx());
        let u = snap.units();
        let days = snap.days.as_slice();
        let month = *self.month.get_or_insert(Some(last.month()));
        let metric = self.metric;
        let key = (snap.days_version, month, metric);
        if self.cache.as_ref().is_none_or(|c| c.key != key) {
            self.cache = Some(YearsCache {
                key,
                series: an::cross_year(days, month, metric),
            });
        }
        let series = &self.cache.as_ref().unwrap().series;
        let years: Vec<i32> = series.iter().map(|s| s.year).collect();
        let hl = self
            .highlight
            .filter(|y| years.contains(y))
            .or(years.last().copied())
            .unwrap_or(last.year());
        self.highlight = Some(hl);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui::small(ui, "Period", p.ink2);
            let mut m = month;
            let mut opts: Vec<(Option<u32>, String)> = vec![(None, "Whole year".into())];
            opts.extend((1..=12).map(|i| (Some(i), MONTHS_LONG[i as usize - 1].to_string())));
            if ui::select(ui, "cmp-month", &mut m, &opts, 130.0) {
                self.month = Some(m);
            }
            ui.add_space(8.0);
            ui::small(ui, "Measure", p.ink2);
            let opts: Vec<(CompareMetric, String)> = CompareMetric::ALL
                .iter()
                .map(|m| (*m, m.label().to_string()))
                .collect();
            ui::select(ui, "cmp-metric", &mut self.metric, &opts, 170.0);
            ui.add_space(8.0);
            ui::small(ui, "Highlight", p.ink2);
            let mut y = hl;
            let opts: Vec<(i32, String)> =
                years.iter().rev().map(|y| (*y, y.to_string())).collect();
            if ui::select(ui, "cmp-year", &mut y, &opts, 80.0) {
                self.highlight = Some(y);
            }
        });

        let unit = match metric {
            CompareMetric::RainCum => format!(" {}", u.rain),
            CompareMetric::GustMax => format!(" {}", u.wind),
            _ => u.temp.clone(),
        };
        let title = format!(
            "{}, {}",
            metric.label(),
            month.map_or("whole year".to_string(), |m| MONTHS_LONG[m as usize - 1]
                .to_string())
        );
        let wide = ui.available_width() >= 1100.0;
        let chart_panel = |ui: &mut Ui| {
            ui::panel(ui, |ui| {
                ui.horizontal(|ui| {
                    ui::h2(ui, &title);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 14.0;
                        ui::key(ui, "Other years", p.grid, ui::KeyMark::Line);
                        ui::key(ui, "Average of other years", p.ink2, ui::KeyMark::Dashed);
                        ui::key(ui, "Highlighted year", p.trace, ui::KeyMark::Line);
                    });
                });
                if series.is_empty() {
                    ui::empty(
                        ui,
                        "No data for this period yet",
                        "Pick another month or measure.",
                        None,
                    );
                    return;
                }
                let length = series[0].values.len();
                let xs: Vec<f64> = (0..length).map(|i| i as f64).collect();
                let avg = an::mean_series(series, Some(hl));
                let gaps = metric != CompareMetric::RainCum;
                let label = move |i: f64| {
                    let i = i.round().max(0.0) as usize;
                    match month {
                        Some(m) => format!("{} {}", i + 1, MONTHS[m as usize - 1]),
                        None => {
                            let m = MONTH_STARTS.iter().rposition(|s| *s <= i).unwrap_or(0);
                            format!("{} {}", i - MONTH_STARTS[m] + 1, MONTHS[m])
                        }
                    }
                };
                let axis = match month {
                    Some(_) => XAxis::Index {
                        splits: None,
                        label: Box::new(|i| format!("{}", i.round() as i64 + 1)),
                    },
                    None => XAxis::Index {
                        splits: Some(MONTH_STARTS.iter().map(|s| *s as f64).collect()),
                        label: Box::new(|i| {
                            MONTH_STARTS
                                .iter()
                                .position(|s| *s as f64 == i)
                                .map_or(String::new(), |m| MONTHS[m].to_string())
                        }),
                    },
                };
                let mut chart = Chart::new(("cmp", month, metric as u8), 380.0, xs, axis)
                    .zoomable()
                    .title(label);
                if metric == CompareMetric::RainCum || metric == CompareMetric::GustMax {
                    chart = chart.left_from_zero();
                }
                for s in series.iter().filter(|s| s.year != hl) {
                    let mut line =
                        Series::line(s.year.to_string(), s.values.clone(), alpha(p.ink2, 0.28))
                            .width(1.0)
                            .no_points()
                            .hidden_in_readout()
                            .unit(&unit, 1);
                    if gaps {
                        line = line.span_gaps();
                    }
                    chart = chart.series(line);
                }
                chart = chart.series(
                    Series::line("Average", avg, p.ink2)
                        .width(1.5)
                        .dashed()
                        .span_gaps()
                        .no_points()
                        .unit(&unit, 1),
                );
                if let Some(main) = series.iter().find(|s| s.year == hl) {
                    let mut line = Series::line(hl.to_string(), main.values.clone(), p.trace)
                        .width(2.75)
                        .unit(&unit, 1);
                    if gaps {
                        line = line.span_gaps();
                    }
                    chart = chart.series(line);
                }
                chart.show(ui);
            });
        };
        let ranking = |ui: &mut Ui| {
            ui::panel(ui, |ui| {
                ui::h2(ui, "How each year compares");
                self.ranking(ui, snap, series, month, metric, hl);
            });
        };
        if wide {
            let total = ui.available_width();
            ui::split(
                ui,
                ((total - 24.0 - 420.0) / (total - 24.0)).max(0.5),
                24.0,
                chart_panel,
                ranking,
            );
        } else {
            chart_panel(ui);
            ranking(ui);
        }
    }

    fn ranking(
        &self,
        ui: &mut Ui,
        snap: &Snapshot,
        series: &[YearSeries],
        month: Option<u32>,
        metric: CompareMetric,
        hl: i32,
    ) {
        let p = Palette::of(ui.ctx());
        let u = snap.units();
        struct Row {
            year: i32,
            n: usize,
            mean_max: Option<f64>,
            mean_min: Option<f64>,
            rain: f64,
            rain_days: usize,
            gust: Option<f64>,
        }
        let days = snap.days.as_slice();
        let mut rows: Vec<Row> = series
            .iter()
            .map(|s| {
                let sel: Vec<DayRecord> = days
                    .iter()
                    .filter(|d| {
                        d.date.year() == s.year && month.is_none_or(|m| d.date.month() == m)
                    })
                    .cloned()
                    .collect();
                Row {
                    year: s.year,
                    n: sel.len(),
                    mean_max: an::mean(&sel, Field::TempMax),
                    mean_min: an::mean(&sel, Field::TempMin),
                    rain: an::total(&sel, Field::Rain),
                    rain_days: an::rain_days(&sel),
                    gust: an::extreme(&sel, Field::GustMax, Kind::Max, false).map(|e| e.value),
                }
            })
            .collect();
        let key = |r: &Row| match metric {
            CompareMetric::TempMax => r.mean_max.unwrap_or(f64::NEG_INFINITY),
            CompareMetric::TempMin => r.mean_min.unwrap_or(f64::NEG_INFINITY),
            CompareMetric::RainCum => r.rain,
            CompareMetric::GustMax => r.gust.unwrap_or(f64::NEG_INFINITY),
        };
        rows.sort_by(|a, b| key(b).total_cmp(&key(a)));
        let expected = if month.is_some() { 28 } else { 365 };
        let cells: Vec<Vec<ui::Cell>> = rows
            .iter()
            .map(|r| {
                let mut c = vec![
                    cell(r.year.to_string()),
                    cell(num(r.mean_max, 1)),
                    cell(num(r.mean_min, 1)),
                    cell(num(Some(r.rain), 1)),
                    cell(r.rain_days.to_string()),
                    cell(num(r.gust, 0)),
                ];
                if r.n < expected {
                    c[0] = c[0]
                        .clone()
                        .note("partial")
                        .tooltip(format!("{} days of data", r.n));
                }
                if r.year == hl {
                    c = c.into_iter().map(|x| x.color(p.trace).strong()).collect();
                }
                c
            })
            .collect();
        ui::table(
            ui,
            &[
                col("Year", 0.0, false),
                col("Avg max", 62.0, true),
                col("Avg min", 62.0, true),
                col("Rain", 54.0, true),
                col("Rain days", 70.0, true),
                col("Gust", 46.0, true),
            ],
            &cells,
            13.5,
        );
        ui::para(
            ui,
            format!(
                "Sorted by {}. Temperatures in {}, rain in {}, gusts in {}.",
                metric.label().to_lowercase(),
                u.temp,
                u.rain,
                u.wind
            ),
            theme::body(13.0),
            p.ink2,
            400.0,
        );
    }

    fn periods(&mut self, ui: &mut Ui, snap: &Snapshot, days: &[DayRecord], last: NaiveDate) {
        let p = Palette::of(ui.ctx());
        let u = snap.units();
        let first = days[0].date;
        let mut pr = *self.periods.get_or_insert_with(|| QUICK[0].1(last));
        let years = first.year()..=last.year();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for (k, tag, color) in [(0usize, "A", p.ink), (2, "B", p.trace)] {
                let (rect, _) = ui.allocate_exact_size(
                    eframe::egui::vec2(24.0, 24.0),
                    eframe::egui::Sense::hover(),
                );
                ui.painter().circle_filled(rect.center(), 12.0, color);
                ui.painter().text(
                    rect.center(),
                    eframe::egui::Align2::CENTER_CENTER,
                    tag,
                    theme::display(15.0),
                    p.paper,
                );
                ui::date_input(ui, &format!("per-{k}"), &mut pr[k], years.clone());
                ui::small(ui, "to", p.ink2);
                ui::date_input(ui, &format!("per-{}", k + 1), &mut pr[k + 1], years.clone());
                ui.add_space(24.0);
            }
        });
        let labels: Vec<&str> = QUICK.iter().map(|q| q.0).collect();
        if let Some(i) = ui::chips(ui, &labels, None) {
            pr = QUICK[i].1(last);
        }
        for k in [0, 2] {
            if pr[k] > pr[k + 1] {
                pr.swap(k, k + 1);
            }
        }
        self.periods = Some(pr);

        let a = an::in_range(days, pr[0], pr[1]);
        let b = an::in_range(days, pr[2], pr[3]);
        let side_by_side = |ui: &mut Ui| {
            ui::panel(ui, |ui| {
                ui::h2(ui, "Side by side");
                let ext = |d: &[DayRecord], f: Field, k: Kind| {
                    an::extreme(d, f, k, false).map(|e| e.value)
                };
                // Label, A, B, unit, decimals.
                type Row<'a> = (&'a str, Option<f64>, Option<f64>, String, usize);
                let rows: Vec<Row> = vec![
                    (
                        "Average max",
                        an::mean(a, Field::TempMax),
                        an::mean(b, Field::TempMax),
                        u.temp.clone(),
                        1,
                    ),
                    (
                        "Average min",
                        an::mean(a, Field::TempMin),
                        an::mean(b, Field::TempMin),
                        u.temp.clone(),
                        1,
                    ),
                    (
                        "Highest",
                        ext(a, Field::TempMax, Kind::Max),
                        ext(b, Field::TempMax, Kind::Max),
                        u.temp.clone(),
                        1,
                    ),
                    (
                        "Lowest",
                        ext(a, Field::TempMin, Kind::Min),
                        ext(b, Field::TempMin, Kind::Min),
                        u.temp.clone(),
                        1,
                    ),
                    (
                        "Rain",
                        Some(an::total(a, Field::Rain)),
                        Some(an::total(b, Field::Rain)),
                        format!(" {}", u.rain),
                        1,
                    ),
                    (
                        "Rain days",
                        Some(an::rain_days(a) as f64),
                        Some(an::rain_days(b) as f64),
                        String::new(),
                        0,
                    ),
                    (
                        "Peak gust",
                        ext(a, Field::GustMax, Kind::Max),
                        ext(b, Field::GustMax, Kind::Max),
                        format!(" {}", u.wind),
                        0,
                    ),
                    (
                        "Days with data",
                        Some(a.len() as f64),
                        Some(b.len() as f64),
                        String::new(),
                        0,
                    ),
                ];
                let fmt = |v: Option<f64>, unit: &str, dp: usize| {
                    v.map_or("–".into(), |v| format!("{}{unit}", num(Some(v), dp)))
                };
                let cells: Vec<Vec<ui::Cell>> = rows
                    .iter()
                    .map(|(label, va, vb, unit, dp)| {
                        let diff = match (va, vb) {
                            (Some(x), Some(y)) => format!("{}{unit}", signed(Some(y - x), *dp)),
                            _ => "–".into(),
                        };
                        vec![
                            cell(*label),
                            cell(fmt(*va, unit, *dp)),
                            cell(fmt(*vb, unit, *dp)).color(p.trace),
                            cell(diff).strong(),
                        ]
                    })
                    .collect();
                ui::table(
                    ui,
                    &[
                        col("", 0.0, false),
                        col("A", 100.0, true),
                        col("B", 100.0, true),
                        col("B − A", 100.0, true),
                    ],
                    &cells,
                    14.0,
                );
            });
        };
        let roses = |ui: &mut Ui| {
            ui::panel(ui, |ui| {
                ui::h2(ui, "Dominant wind direction");
                let ra = an::wind_rose(a);
                let rb = an::wind_rose(b);
                let max = dial::shared_max(&[&ra, &rb]);
                ui::columns(ui, 2, 16.0, false, |i, ui| {
                    ui.vertical_centered(|ui| {
                        let (r, label) = if i == 0 {
                            (
                                &ra,
                                format!("A: {} to {}", nice_date(pr[0]), nice_date(pr[1])),
                            )
                        } else {
                            (
                                &rb,
                                format!("B: {} to {}", nice_date(pr[2]), nice_date(pr[3])),
                            )
                        };
                        let size = ui.available_width().min(250.0);
                        dial::wind_rose(ui, size, &r.0, r.1, max);
                        ui::small(ui, label, p.ink2);
                    });
                });
            });
        };
        if ui.available_width() >= 1100.0 {
            ui::split(ui, 0.5, 24.0, side_by_side, roses);
        } else {
            side_by_side(ui);
            roses(ui);
        }
    }
}
