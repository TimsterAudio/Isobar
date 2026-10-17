//! History: a date range, a summary against the station's own normals,
//! temperature, rain, wind and pressure charts, and a year heatmap.

use chrono::{Datelike, Duration, NaiveDate};
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, Layout, Mesh, Rect, Sense, Ui};
use isobar_core::analytics::{self as an, Agg, Bucket, Field, Normal, PeriodSummary};
use isobar_core::DayRecord;

use crate::app::{Route, Snapshot};
use crate::chart::{self, Chart, Series, XAxis};
use crate::format::{self, nice_date, num, signed, MONTHS};
use crate::theme::{self, alpha, Palette};
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Preset {
    Days7,
    Days30,
    Days90,
    Month,
    Year,
    Months12,
    All,
    Custom,
}

const PRESETS: [(Preset, &str); 7] = [
    (Preset::Days7, "7 days"),
    (Preset::Days30, "30 days"),
    (Preset::Days90, "90 days"),
    (Preset::Month, "This month"),
    (Preset::Year, "This year"),
    (Preset::Months12, "12 months"),
    (Preset::All, "All"),
];

impl Preset {
    fn range(self, first: NaiveDate, last: NaiveDate) -> (NaiveDate, NaiveDate) {
        let start = match self {
            Preset::Days7 => last - Duration::days(6),
            Preset::Days30 | Preset::Custom => last - Duration::days(29),
            Preset::Days90 => last - Duration::days(89),
            Preset::Month => format::first_of_month(last),
            Preset::Year => format::first_of_year(last),
            Preset::Months12 => last - Duration::days(364),
            Preset::All => first,
        };
        (start.max(first), last)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeatMetric {
    MaxVsNormal,
    MinVsNormal,
    Rain,
}

struct Cache {
    key: (u64, NaiveDate, NaiveDate),
    summary: PeriodSummary,
    bucket: Bucket,
    agg: Vec<Agg>,
    norm_max: Vec<Option<Normal>>,
    norm_min: Vec<Option<Normal>>,
}

struct HeatCache {
    key: (u64, i32, HeatMetric),
    normals: Option<Vec<Option<Normal>>>,
}

pub struct History {
    preset: Preset,
    range: Option<(NaiveDate, NaiveDate)>,
    seen_version: u64,
    heat_metric: HeatMetric,
    heat_year: Option<i32>,
    cache: Option<Cache>,
    heat_cache: Option<HeatCache>,
}

impl Default for History {
    fn default() -> Self {
        History {
            preset: Preset::Days30,
            range: None,
            seen_version: 0,
            heat_metric: HeatMetric::MaxVsNormal,
            heat_year: None,
            cache: None,
            heat_cache: None,
        }
    }
}

impl History {
    pub fn ui(&mut self, ui: &mut Ui, snap: &Snapshot, go: &mut Option<Route>) {
        let p = Palette::of(ui.ctx());
        let days = snap.days.as_slice();
        let (Some(first), Some(last)) = (days.first().map(|d| d.date), days.last().map(|d| d.date))
        else {
            ui::h1(ui, "History");
            ui::panel(ui, |ui| {
                if ui::empty(
                    ui,
                    "No history yet",
                    "Isobar reads your station's daily records from Cumulus MX. Connect a station or import a dayfile in Settings.",
                    Some("Open settings"),
                ) {
                    *go = Some(Route::Settings);
                }
            });
            return;
        };
        if self.range.is_none()
            || (snap.days_version != self.seen_version && self.preset != Preset::Custom)
        {
            self.range = Some(self.preset.range(first, last));
        }
        self.seen_version = snap.days_version;
        let (mut start, mut end) = self.range.unwrap();

        ui.horizontal(|ui| {
            ui::h1(ui, "History");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui::text(
                    ui,
                    format!("{} to {}", nice_date(start), nice_date(end)),
                    theme::body(15.0),
                    p.ink2,
                );
            });
        });

        // Range presets and a custom range.
        ui.horizontal_wrapped(|ui| {
            let labels: Vec<&str> = PRESETS.iter().map(|p| p.1).collect();
            let sel = PRESETS.iter().position(|x| x.0 == self.preset);
            if let Some(i) = ui::chips(ui, &labels, sel) {
                self.preset = PRESETS[i].0;
                self.range = Some(self.preset.range(first, last));
                (start, end) = self.range.unwrap();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let years = first.year()..=last.year();
                let mut changed = ui::date_input(ui, "hist-end", &mut end, years.clone());
                ui::small(ui, "to", p.ink2);
                changed |= ui::date_input(ui, "hist-start", &mut start, years);
                if changed {
                    self.preset = Preset::Custom;
                    if start > end {
                        std::mem::swap(&mut start, &mut end);
                    }
                    self.range = Some((start, end));
                }
            });
        });

        let key = (snap.days_version, start, end);
        if self.cache.as_ref().is_none_or(|c| c.key != key) {
            let span = (end - start).num_days() + 1;
            let bucket = an::bucket_for(span);
            self.cache = Some(Cache {
                key,
                summary: an::summarize(days, start, end),
                bucket,
                agg: an::aggregate(an::in_range(days, start, end), bucket),
                norm_max: an::climatology(days, Field::TempMax, 7, None),
                norm_min: an::climatology(days, Field::TempMin, 7, None),
            });
        }
        let cache = self.cache.as_ref().unwrap();
        self.summary(ui, snap, &cache.summary);

        if cache.agg.is_empty() {
            ui::panel(ui, |ui| {
                ui::empty(
                    ui,
                    "No days in this range",
                    "Choose a range that overlaps your station's history.",
                    None,
                );
            });
        } else {
            if cache.bucket != Bucket::Day {
                ui.add_space(-12.0);
                ui::small(
                    ui,
                    format!(
                        "Showing {} values for this long range: extremes for temperature and wind, totals for rain. Drag across a chart to zoom in.",
                        if cache.bucket == Bucket::Week { "weekly" } else { "monthly" }
                    ),
                    p.ink2,
                );
            }
            Self::charts(ui, snap, cache);
        }
        self.heatmap(ui, snap);
    }

    fn summary(&self, ui: &mut Ui, snap: &Snapshot, s: &PeriodSummary) {
        let p = Palette::of(ui.ctx());
        let u = snap.units();
        let vs = |v: Option<f64>, n: Option<f64>| match (v, n) {
            (Some(v), Some(n)) => format!("{}{} vs normal", signed(Some(v - n), 1), u.temp),
            _ => "no normal yet".to_string(),
        };
        let tone = |v: Option<f64>, n: Option<f64>| match (v, n) {
            (Some(v), Some(n)) if v - n > 0.5 => p.heat,
            (Some(v), Some(n)) if v - n < -0.5 => p.cold,
            _ => p.ink2,
        };
        let rain_pct = s
            .normal
            .rain
            .filter(|n| *n > 0.0)
            .map(|n| (s.rain / n * 100.0).round());
        let stats: Vec<(&str, String, Color32, String, Color32)> = vec![
            (
                "Highest",
                s.hottest.as_ref().map_or("–".into(), |e| {
                    format!("{}{}", num(Some(e.value), 1), u.temp)
                }),
                p.heat,
                s.hottest
                    .as_ref()
                    .map_or(String::new(), |e| nice_date(e.date)),
                p.ink2,
            ),
            (
                "Lowest",
                s.coldest.as_ref().map_or("–".into(), |e| {
                    format!("{}{}", num(Some(e.value), 1), u.temp)
                }),
                p.cold,
                s.coldest
                    .as_ref()
                    .map_or(String::new(), |e| nice_date(e.date)),
                p.ink2,
            ),
            (
                "Average max",
                format!("{}{}", num(s.mean_max, 1), u.temp),
                p.ink,
                vs(s.mean_max, s.normal.mean_max),
                tone(s.mean_max, s.normal.mean_max),
            ),
            (
                "Average min",
                format!("{}{}", num(s.mean_min, 1), u.temp),
                p.ink,
                vs(s.mean_min, s.normal.mean_min),
                tone(s.mean_min, s.normal.mean_min),
            ),
            (
                "Rain",
                format!("{} {}", num(Some(s.rain), 1), u.rain),
                p.rain,
                format!(
                    "{} rain days{}",
                    s.rain_days,
                    rain_pct
                        .map(|r| format!(", {r:.0}% of normal"))
                        .unwrap_or_default()
                ),
                p.ink2,
            ),
            (
                "Peak gust",
                s.gust.as_ref().map_or("–".into(), |e| {
                    format!("{} {}", num(Some(e.value), 0), u.wind)
                }),
                p.wind,
                s.gust.as_ref().map_or(String::new(), |e| nice_date(e.date)),
                p.ink2,
            ),
        ];
        let cols = if ui.available_width() >= 1100.0 { 6 } else { 3 };
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui::rule(ui, p.grid);
            for row in stats.chunks(cols) {
                ui::columns(ui, cols, 32.0, true, |i, ui| {
                    let Some((label, value, vc, detail, dc)) = row.get(i) else {
                        return;
                    };
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.add_space(12.0);
                    ui::small(ui, *label, p.ink2);
                    ui::text(ui, value, theme::display(31.0), *vc);
                    ui::small(ui, detail, *dc);
                    ui.add_space(12.0);
                });
                ui::rule(ui, p.grid);
            }
        });
    }

    fn charts(ui: &mut Ui, snap: &Snapshot, c: &Cache) {
        let p = Palette::of(ui.ctx());
        let u = snap.units();
        let xs: Vec<f64> = c.agg.iter().map(|a| chart::day_ts(a.key)).collect();
        let col = |f: fn(&Agg) -> Option<f64>| c.agg.iter().map(f).collect::<Vec<_>>();
        let bucket = c.bucket;
        let title = move |x: f64| {
            let d = chart::ts_day(x);
            match bucket {
                Bucket::Day => nice_date(d),
                Bucket::Week => format!("Week of {}", nice_date(d)),
                Bucket::Month => format::month_year(d),
            }
        };
        let caption = |ui: &mut Ui, heading: &str, keys: &[(&str, Color32, ui::KeyMark)]| {
            ui.horizontal(|ui| {
                ui::h2(ui, heading);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    for (label, color, mark) in keys.iter().rev() {
                        ui::key(ui, label, *color, *mark);
                    }
                });
            });
        };

        // Temperature: the band between max and min, with normals for daily views.
        ui::panel(ui, |ui| {
            caption(
                ui,
                "Temperature",
                &[
                    ("Max", p.heat, ui::KeyMark::Line),
                    ("Min", p.cold, ui::KeyMark::Line),
                    ("Normal", p.ink2, ui::KeyMark::Dashed),
                ],
            );
            let mut chart = Chart::new(
                ("hist-temp", c.key.1, c.key.2),
                300.0,
                xs.clone(),
                XAxis::Time,
            )
            .series(Series::line("Max", col(|a| a.t_max), p.heat).unit(&u.temp, 1))
            .series(Series::line("Min", col(|a| a.t_min), p.cold).unit(&u.temp, 1))
            .band(0, 1, alpha(p.trace, 0.08))
            .zoomable()
            .title(title);
            if bucket == Bucket::Day {
                let nm = |n: &[Option<Normal>]| {
                    c.agg
                        .iter()
                        .map(|a| n[an::doy_index(a.key)].map(|n| n.mean))
                        .collect::<Vec<_>>()
                };
                chart = chart
                    .series(
                        Series::line("Normal max", nm(&c.norm_max), alpha(p.heat, 0.5))
                            .width(1.0)
                            .dashed()
                            .no_points()
                            .unit(&u.temp, 1),
                    )
                    .series(
                        Series::line("Normal min", nm(&c.norm_min), alpha(p.cold, 0.5))
                            .width(1.0)
                            .dashed()
                            .no_points()
                            .unit(&u.temp, 1),
                    );
            }
            chart.show(ui);
        });

        ui::split(
            ui,
            0.5,
            24.0,
            |ui| {
                ui::panel(ui, |ui| {
                    caption(
                        ui,
                        "Rain",
                        &[
                            (
                                if bucket == Bucket::Day {
                                    "Daily"
                                } else {
                                    "Total"
                                },
                                p.rain,
                                ui::KeyMark::Square,
                            ),
                            ("Running total", p.trace, ui::KeyMark::Line),
                        ],
                    );
                    let mut acc = 0.0;
                    let cum: Vec<Option<f64>> = c
                        .agg
                        .iter()
                        .map(|a| {
                            acc += a.rain;
                            Some((acc * 10.0f64).round() / 10.0)
                        })
                        .collect();
                    Chart::new(
                        ("hist-rain", c.key.1, c.key.2),
                        240.0,
                        xs.clone(),
                        XAxis::Time,
                    )
                    .series(
                        Series::line(
                            if bucket == Bucket::Day {
                                "Rain"
                            } else {
                                "Rain total"
                            },
                            col(|a| Some(a.rain)),
                            p.rain,
                        )
                        .bars()
                        .unit(format!(" {}", u.rain), 1),
                    )
                    .series(
                        Series::line("Running total", cum, p.trace)
                            .right()
                            .no_points()
                            .unit(format!(" {}", u.rain), 1),
                    )
                    .left_from_zero()
                    .right_from_zero()
                    .zoomable()
                    .title(title)
                    .show(ui);
                });
            },
            |ui| {
                ui::panel(ui, |ui| {
                    caption(
                        ui,
                        "Wind",
                        &[
                            ("Peak gust", p.wind, ui::KeyMark::Line),
                            ("Highest average", p.ink2, ui::KeyMark::Line),
                        ],
                    );
                    Chart::new(
                        ("hist-wind", c.key.1, c.key.2),
                        240.0,
                        xs.clone(),
                        XAxis::Time,
                    )
                    .series(
                        Series::line("Peak gust", col(|a| a.gust), p.wind)
                            .unit(format!(" {}", u.wind), 0),
                    )
                    .series(
                        Series::line("Highest average", col(|a| a.wind_max), p.ink2)
                            .width(1.25)
                            .unit(format!(" {}", u.wind), 0),
                    )
                    .left_from_zero()
                    .zoomable()
                    .title(title)
                    .show(ui);
                });
            },
        );

        ui::panel(ui, |ui| {
            caption(ui, "Pressure range", &[]);
            Chart::new(("hist-press", c.key.1, c.key.2), 200.0, xs, XAxis::Time)
                .series(
                    Series::line("Highest", col(|a| a.p_max), p.trace)
                        .width(1.25)
                        .no_points()
                        .unit(format!(" {}", u.press), 1),
                )
                .series(
                    Series::line("Lowest", col(|a| a.p_min), p.trace)
                        .width(1.25)
                        .no_points()
                        .unit(format!(" {}", u.press), 1),
                )
                .band(0, 1, alpha(p.trace, 0.12))
                .zoomable()
                .title(title)
                .show(ui);
        });
    }

    fn heatmap(&mut self, ui: &mut Ui, snap: &Snapshot) {
        let p = Palette::of(ui.ctx());
        let u = snap.units();
        let days = snap.days.as_slice();
        let mut years: Vec<i32> = days.iter().map(|d| d.date.year()).collect();
        years.dedup();
        years.reverse();
        let year = match self.heat_year.filter(|y| years.contains(y)) {
            Some(y) => y,
            None => years[0],
        };
        self.heat_year = Some(year);

        ui::panel(ui, |ui| {
            ui.horizontal(|ui| {
                ui::h2(ui, "Year at a glance");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let mut y = year;
                    let opts: Vec<(i32, String)> =
                        years.iter().map(|y| (*y, y.to_string())).collect();
                    if ui::select(ui, "heat-year", &mut y, &opts, 90.0) {
                        self.heat_year = Some(y);
                    }
                    ui::select(
                        ui,
                        "heat-metric",
                        &mut self.heat_metric,
                        &[
                            (HeatMetric::MaxVsNormal, "Max temperature vs normal".into()),
                            (HeatMetric::MinVsNormal, "Min temperature vs normal".into()),
                            (HeatMetric::Rain, "Rainfall".into()),
                        ],
                        220.0,
                    );
                });
            });
            let year = self.heat_year.unwrap_or(year);
            let metric = self.heat_metric;
            let field = if metric == HeatMetric::MinVsNormal {
                Field::TempMin
            } else {
                Field::TempMax
            };
            let key = (snap.days_version, year, metric);
            if self.heat_cache.as_ref().is_none_or(|c| c.key != key) {
                self.heat_cache = Some(HeatCache {
                    key,
                    normals: (metric != HeatMetric::Rain)
                        .then(|| an::climatology(days, field, 7, Some(year))),
                });
            }
            let normals = self.heat_cache.as_ref().and_then(|c| c.normals.as_ref());
            let first = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
            let last = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();
            let year_days = an::in_range(days, first, last);

            let label_w = 36.0;
            let gap = 2.0;
            let w = ui.available_width();
            let cell = ((w - label_w - gap * 31.0) / 31.0).clamp(10.0, 40.0);
            let head = 18.0;
            let height = head + 12.0 * (cell + gap);
            let (rect, resp) = ui.allocate_exact_size(vec2(w, height), Sense::hover());
            let painter = ui.painter_at(rect);
            let origin = rect.min + vec2(label_w, head);
            for d in 1..=31 {
                if d % 5 == 1 || d == 31 {
                    let x = origin.x + (d - 1) as f32 * (cell + gap) + cell / 2.0;
                    painter.text(
                        pos2(x, rect.top() + head / 2.0),
                        Align2::CENTER_CENTER,
                        d.to_string(),
                        theme::body(11.0),
                        p.ink2,
                    );
                }
            }
            let mut hovered: Option<String> = None;
            let hover = resp.hover_pos();
            for m in 0..12u32 {
                let y = origin.y + m as f32 * (cell + gap);
                painter.text(
                    pos2(rect.left(), y + cell / 2.0),
                    Align2::LEFT_CENTER,
                    MONTHS[m as usize],
                    theme::body(12.0),
                    p.ink2,
                );
                for d in 1..=31u32 {
                    let Some(date) = NaiveDate::from_ymd_opt(year, m + 1, d) else {
                        continue;
                    };
                    let r = Rect::from_min_size(
                        pos2(origin.x + (d - 1) as f32 * (cell + gap), y),
                        vec2(cell, cell),
                    );
                    let rec: Option<&DayRecord> = year_days
                        .binary_search_by_key(&date, |x| x.date)
                        .ok()
                        .map(|i| &year_days[i]);
                    let (color, tip) = heat_cell(&p, &u, metric, field, date, rec, normals);
                    painter.rect_filled(r, 2, p.faint);
                    if let Some(c) = color {
                        painter.rect_filled(r, 2, c);
                    }
                    if hover.is_some_and(|h| r.contains(h)) {
                        hovered = tip;
                    }
                }
            }
            if let Some(t) = hovered {
                resp.on_hover_text_at_pointer(t);
            }

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let (lo, hi, a, mid, b) = if metric == HeatMetric::Rain {
                    ("Dry", "Wet", p.faint, None, p.rain)
                } else {
                    (
                        "Colder than normal",
                        "Warmer than normal",
                        p.cold,
                        Some(p.faint),
                        p.heat,
                    )
                };
                ui::small(ui, lo, p.ink2);
                gradient(ui, a, mid, b);
                ui::small(ui, hi, p.ink2);
                if metric != HeatMetric::Rain
                    && !normals.is_some_and(|n| n.iter().any(Option::is_some))
                {
                    ui::small(ui, "Normals need data from other years first.", p.ink2);
                }
            });
        });
    }
}

fn heat_cell(
    p: &Palette,
    u: &isobar_core::Units,
    metric: HeatMetric,
    field: Field,
    date: NaiveDate,
    rec: Option<&DayRecord>,
    normals: Option<&Vec<Option<Normal>>>,
) -> (Option<Color32>, Option<String>) {
    let Some(rec) = rec else {
        return (None, Some(format!("{}: no data", nice_date(date))));
    };
    if metric == HeatMetric::Rain {
        let r = rec.rain.unwrap_or(0.0);
        let c = (r >= an::RAIN_DAY)
            .then(|| alpha(p.rain, (0.15 + (1.0 + r).log10() / 1.6).min(1.0) as f32));
        return (
            c,
            Some(format!(
                "{}: {} {}",
                nice_date(date),
                num(Some(r), 1),
                u.rain
            )),
        );
    }
    let v = field.get(rec);
    let nm = normals.and_then(|n| n[an::doy_index(date)]);
    match (v, nm) {
        (Some(v), Some(nm)) if nm.sd > 0.0 => {
            let z = ((v - nm.mean) / nm.sd).clamp(-2.5, 2.5);
            let c = alpha(
                if z >= 0.0 { p.heat } else { p.cold },
                (z.abs() / 2.5 + 0.06).min(0.9) as f32,
            );
            (
                Some(c),
                Some(format!(
                    "{}: {}{}, {}{} vs normal",
                    nice_date(date),
                    num(Some(v), 1),
                    u.temp,
                    signed(Some(v - nm.mean), 1),
                    u.temp
                )),
            )
        }
        (Some(v), _) => (
            None,
            Some(format!(
                "{}: {}{}",
                nice_date(date),
                num(Some(v), 1),
                u.temp
            )),
        ),
        _ => (None, Some(format!("{}: no data", nice_date(date)))),
    }
}

/// A small legend bar from `a` (through `mid`) to `b`.
fn gradient(ui: &mut Ui, a: Color32, mid: Option<Color32>, b: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(120.0, 16.0), Sense::hover());
    let bar = Rect::from_center_size(rect.center(), vec2(120.0, 8.0));
    let mut mesh = Mesh::default();
    let stops: Vec<(f32, Color32)> = match mid {
        Some(m) => vec![(0.0, a), (0.5, m), (1.0, b)],
        None => vec![(0.0, a), (1.0, b)],
    };
    for w in stops.windows(2) {
        let x0 = bar.left() + bar.width() * w[0].0;
        let x1 = bar.left() + bar.width() * w[1].0;
        let i = mesh.vertices.len() as u32;
        mesh.colored_vertex(pos2(x0, bar.top()), w[0].1);
        mesh.colored_vertex(pos2(x1, bar.top()), w[1].1);
        mesh.colored_vertex(pos2(x1, bar.bottom()), w[1].1);
        mesh.colored_vertex(pos2(x0, bar.bottom()), w[0].1);
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i, i + 2, i + 3);
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}
