//! Forecast: the week ahead from Open-Meteo for the station's location, and
//! the months ahead from the station's own history (a pattern analysis).

use chrono::Datelike;
use eframe::egui::{Align, Layout, Ui};
use isobar_core::analytics::compass;
use isobar_core::forecast::{self as fc, Forecast};
use isobar_core::outlook::{self as ol, Outlook};

use crate::app::{Route, Snapshot};
use crate::chart::{self, Chart, Series, XAxis};
use crate::format::{self, num, signed, MONTHS, MONTHS_LONG};
use crate::settings::Settings;
use crate::theme::{self, alpha, Palette};
use crate::ui::{self, cell, col, KeyMark, Stat};
use crate::worker::{Cmd, Handle};

#[derive(Default)]
pub struct ForecastView {
    outlook: Option<(u64, Option<Outlook>)>,
}

impl ForecastView {
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        snap: &Snapshot,
        settings: &Settings,
        worker: &Handle,
        go: &mut Option<Route>,
    ) {
        ui::h1(ui, "Forecast");
        week(ui, snap, settings, worker, go);
        if self
            .outlook
            .as_ref()
            .is_none_or(|o| o.0 != snap.days_version)
        {
            self.outlook = Some((snap.days_version, ol::outlook(&snap.days)));
        }
        let o = self.outlook.as_ref().and_then(|o| o.1.as_ref());
        months_ahead(ui, snap, o);
    }
}

fn day_name(d: chrono::NaiveDate, today: chrono::NaiveDate) -> String {
    match (d - today).num_days() {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        _ => format!("{} {}", d.format("%a"), d.day()),
    }
}

/// The week ahead from Open-Meteo.
fn week(
    ui: &mut Ui,
    snap: &Snapshot,
    settings: &Settings,
    worker: &Handle,
    go: &mut Option<Route>,
) {
    let p = Palette::of(ui.ctx());
    let state = worker.shared.lock().unwrap().forecast.clone();
    ui::panel(ui, |ui| {
        ui.horizontal(|ui| {
            ui::h2(ui, "The week ahead");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !snap.demo
                    && settings.longitude.is_some()
                    && ui::chip(
                        ui,
                        if state.loading {
                            "Fetching…"
                        } else {
                            "Refresh"
                        },
                        false,
                    )
                    .clicked()
                {
                    worker.send(Cmd::RefreshForecast);
                }
            });
        });
        if settings.latitude.is_none() || settings.longitude.is_none() {
            if ui::empty(
                ui,
                "Where's the station?",
                "The forecast needs the station's latitude and longitude. In Settings → Station, find your town to fill them in, then save.",
                Some("Open settings"),
            ) {
                *go = Some(Route::Settings);
            }
            return;
        }
        let Some(f) = state.data.as_ref() else {
            let text = state
                .error
                .clone()
                .unwrap_or_else(|| "Fetching the forecast from Open-Meteo…".into());
            ui::text(
                ui,
                text,
                theme::body(15.0),
                if state.error.is_some() {
                    p.heat
                } else {
                    p.ink2
                },
            );
            return;
        };
        let place = settings
            .place
            .clone()
            .unwrap_or_else(|| format!("{:.2}, {:.2}", f.latitude, f.longitude));
        ui::muted(
            ui,
            format!(
                "For {place}, updated {}. A computer model's forecast for the area, so it won't match your backyard exactly.",
                format::ago(Some(f.fetched_at), snap.now_ms)
            ),
        );
        if let Some(e) = &state.error {
            ui::small(ui, e, p.heat);
        }
        days(ui, snap, f, &p);
        hours(ui, snap, f, &p);
        ui::small(
            ui,
            "Weather forecast by Open-Meteo.com, under CC BY 4.0.",
            p.ink2,
        );
    });
}

fn days(ui: &mut Ui, snap: &Snapshot, f: &Forecast, p: &Palette) {
    let u = snap.units();
    let today = chrono::Local::now().date_naive();
    let shown: Vec<&fc::ForecastDay> = f.days.iter().filter(|d| d.date >= today).take(7).collect();
    let cols = if ui.available_width() >= 900.0 {
        shown.len().max(1)
    } else {
        4
    };
    for row in shown.chunks(cols) {
        ui::columns(ui, cols, 14.0, true, |i, ui| {
            let Some(d) = row.get(i) else { return };
            ui.spacing_mut().item_spacing.y = 2.0;
            ui::text(
                ui,
                day_name(d.date, today),
                theme::display_medium(17.0),
                p.ink,
            );
            ui::small(ui, d.code.map_or("–", fc::words), p.ink2);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui::text(
                    ui,
                    format!("{}°", num(d.max, 0)),
                    theme::display(26.0),
                    p.heat,
                );
                ui::text(
                    ui,
                    format!("{}°", num(d.min, 0)),
                    theme::display(20.0),
                    p.cold,
                );
            });
            let rain = d.rain.unwrap_or(0.0);
            let chance = d.rain_chance.unwrap_or(0.0);
            ui::text(
                ui,
                if rain >= 0.1 || chance >= 20.0 {
                    format!(
                        "{} {} · {}%",
                        num(Some(rain), 1),
                        u.rain,
                        num(Some(chance), 0)
                    )
                } else {
                    "Dry".into()
                },
                theme::body(13.0),
                if chance >= 50.0 { p.rain } else { p.ink2 },
            )
            .on_hover_text("The day's rain, and the highest chance of rain in any hour.");
            ui::text(
                ui,
                format!(
                    "{} {}{}",
                    d.dir.map_or("", compass),
                    num(d.wind, 0),
                    d.gust
                        .map_or(String::new(), |g| format!(", gusts {}", num(Some(g), 0)))
                ),
                theme::body(13.0),
                p.ink2,
            )
            .on_hover_text(format!(
                "Wind in {}: where it comes from, its strongest average and gusts.",
                u.wind
            ));
            if let Some(uv) = d.uv {
                ui::small(ui, format!("UV {}", num(Some(uv), 0)), p.ink2);
            }
        });
    }
}

fn hours(ui: &mut Ui, snap: &Snapshot, f: &Forecast, p: &Palette) {
    let u = snap.units();
    let pts: Vec<&fc::ForecastHour> = f
        .hours
        .iter()
        .filter(|h| h.ts >= snap.now_ms - 3_600_000)
        .collect();
    if pts.len() < 2 {
        return;
    }
    let xs: Vec<f64> = pts.iter().map(|h| h.ts as f64 / 1000.0).collect();
    Chart::new("forecast-hours", 170.0, xs, XAxis::Time)
        .series(
            Series::line(
                "Chance of rain",
                pts.iter().map(|h| h.rain_chance).collect(),
                alpha(p.rain, 0.55),
            )
            .bars()
            .right()
            .unit("%", 0),
        )
        .series(
            Series::line("Temperature", pts.iter().map(|h| h.temp).collect(), p.trace)
                .width(2.0)
                .no_points()
                .unit(&u.temp, 1),
        )
        .right_from_zero()
        .left_ticks(|v, step| format!("{}°", chart::plain_tick(v, step)))
        .title(|x| {
            format!(
                "{}, {}",
                format::nice_date(chart::ts_day(x)),
                chart::ts_time(x)
            )
        })
        .show(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        ui::key(ui, "Temperature, the next 48 hours", p.trace, KeyMark::Line);
        ui::key(ui, "Chance of rain (%, right)", p.rain, KeyMark::Square);
    });
}

fn month_name(y: i32, m: u32) -> String {
    format!("{} {}", MONTHS[m as usize - 1], y)
}

/// "Aug to Oct".
fn window_name(end: (i32, u32)) -> String {
    let start = (end.1 as i32 - 3).rem_euclid(12) as usize;
    format!(
        "{} to {}",
        MONTHS_LONG[start],
        MONTHS_LONG[end.1 as usize - 1]
    )
}

fn pct(r: f64) -> String {
    format!("{}% of normal", num(Some(r * 100.0), 0))
}

/// The months ahead from the station's history.
fn months_ahead(ui: &mut Ui, snap: &Snapshot, o: Option<&Outlook>) {
    let p = Palette::of(ui.ctx());
    let u = snap.units();
    ui::panel(ui, |ui| {
        ui::h2(ui, "The months ahead");
        let Some(o) = o else {
            ui::para(
                ui,
                "A look ahead from your station's own history, once it has four years of full months to compare. Each month needs at least 25 days on record.",
                theme::body(14.0),
                p.ink2,
                900.0,
            );
            return;
        };
        ui::para(
            ui,
            format!(
                "A pattern analysis from your station's {} years, not a weather forecast. {} was {} and {}. The past years most like it, and what came after them, give the odds below.",
                o.years,
                window_name(o.window_end),
                match o.recent_temp {
                    t if t > ol::NEAR_TEMP => format!("{}° warmer than normal", num(Some(t), 1)),
                    t if t < -ol::NEAR_TEMP => format!("{}° cooler than normal", num(Some(-t), 1)),
                    _ => "about as warm as normal".into(),
                },
                match o.recent_rain {
                    r if r > 1.0 + ol::NEAR_RAIN => format!("wetter, with {}", pct(r)),
                    r if r < 1.0 - ol::NEAR_RAIN => format!("drier, with {}", pct(r)),
                    r => format!("near normal for rain ({})", pct(r)),
                },
            ),
            theme::body(14.0),
            p.ink2,
            900.0,
        );
        let n = o.analogues.len();
        let stats: Vec<Stat> = o
            .stretches
            .iter()
            .map(|s| {
                let label = format!("Next {} months", s.months);
                match (s.temp, s.rain) {
                    (Some(t), Some(r)) => Stat::new(
                        label,
                        format!("{} · {}", short_temp(t), short_rain(r)),
                        if r > 1.0 + ol::NEAR_RAIN {
                            p.rain
                        } else if t > ol::NEAR_TEMP {
                            p.heat
                        } else {
                            p.ink
                        },
                    )
                    .detail(
                        format!(
                            "{}{}, {}. Warmer in {} of {}, wetter in {} of {}",
                            signed(Some(t), 1),
                            u.temp,
                            pct(r),
                            s.warmer,
                            s.n,
                            s.wetter,
                            s.n
                        ),
                        p.ink2,
                    ),
                    _ => Stat::new(label, "–", p.ink2).detail("Not enough years on record", p.ink2),
                }
            })
            .collect();
        ui::stats(ui, &stats);

        ui::text(ui, "Why", theme::semi(15.0), p.ink);
        let mut why: Vec<String> = Vec::new();
        why.push(format!(
            "The {n} years whose {} was most like this year's: {}.",
            window_name(o.window_end),
            o.analogues
                .iter()
                .map(|a| format!(
                    "{} ({}{}, {})",
                    a.year,
                    signed(Some(a.temp), 1),
                    u.temp,
                    pct(a.rain)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        if let Some(s) = o.stretches.first().filter(|s| s.n > 0) {
            why.push(format!(
                "In the three months after, {} of them were wetter than normal and {} warmer.",
                s.wetter, s.warmer
            ));
        }
        if let Some(t) = o.trend {
            why.push(format!(
                "Your station's temperatures have {} by about {}° a decade over its records, which is added to the analogue years' figures (their temperatures are taken against that trend).",
                if t >= 0.0 { "risen" } else { "fallen" },
                num(Some(t.abs()), 1)
            ));
        }
        match &o.skill {
            Some(s) => {
                let t = s.temp_hits as f64 / s.temp_tries as f64 * 100.0;
                let r = s.rain_hits as f64 / s.rain_tries as f64 * 100.0;
                let verdict = |x: f64| {
                    if x >= 65.0 {
                        "a useful lean"
                    } else if x >= 55.0 {
                        "a slight lean, better than a coin toss"
                    } else {
                        "no better than a coin toss"
                    }
                };
                why.push(format!(
                    "How far to trust it: tried on {} past starts at your station, leaving each one's year out, this method called whether the next three months would be warmer or cooler right {}% of the time ({}), and wetter or drier {}% of the time ({}).",
                    s.temp_tries,
                    num(Some(t), 0),
                    verdict(t),
                    num(Some(r), 0),
                    verdict(r)
                ));
            }
            None => why.push(
                "There aren't enough years on record yet to test how often this method is right."
                    .into(),
            ),
        }
        why.push("Seasonal outlooks from weather services use ocean temperatures (El Niño and La Niña, the Indian Ocean Dipole) and climate models, which a single station can't see. They're the better guide; this shows what your own records suggest.".into());
        for w in why {
            ui.horizontal_top(|ui| {
                ui::text(ui, "·", theme::body(14.0), p.ink2);
                ui::para(ui, w, theme::body(14.0), p.ink, 860.0);
            });
        }

        let cols = [
            col("Month", 120.0, false),
            col("Temperature", 0.0, false),
            col("", 0.0, true),
            col("Rain", 0.0, false),
            col("", 0.0, true),
            col("Similar years", 0.0, true),
            col("Normal", 0.0, true),
        ];
        let rows: Vec<Vec<ui::Cell>> = o
            .months
            .iter()
            .map(|m| {
                let tc = |t: f64| {
                    if t > ol::NEAR_TEMP {
                        p.heat
                    } else if t < -ol::NEAR_TEMP {
                        p.cold
                    } else {
                        p.ink2
                    }
                };
                let rc = |r: f64| {
                    if r > 1.0 + ol::NEAR_RAIN {
                        p.rain
                    } else if r < 1.0 - ol::NEAR_RAIN {
                        p.warn
                    } else {
                        p.ink2
                    }
                };
                vec![
                    cell(month_name(m.year, m.month)).strong().color(p.ink),
                    cell(m.temp.map_or("–".into(), |t| short_temp(t).to_string()))
                        .color(m.temp.map_or(p.ink2, tc)),
                    cell(m.temp.map_or(String::new(), |t| {
                        format!("{}{}", signed(Some(t), 1), u.temp)
                    }))
                    .color(p.ink2),
                    cell(m.rain.map_or("–".into(), |r| short_rain(r).to_string()))
                        .color(m.rain.map_or(p.ink2, rc)),
                    cell(
                        m.rain
                            .map_or(String::new(), |r| format!("{}%", num(Some(r * 100.0), 0))),
                    )
                    .color(p.ink2),
                    cell(format!(
                        "{} warmer, {} wetter of {}",
                        m.warmer, m.wetter, m.n
                    ))
                    .color(p.ink2),
                    cell(format!(
                        "{}{}, {} {}",
                        num(Some(m.normal_temp), 1),
                        u.temp,
                        num(Some(m.normal_rain), 0),
                        u.rain
                    ))
                    .color(p.ink2),
                ]
            })
            .collect();
        ui::table(ui, &cols, &rows, 13.0);
        ui::small(
            ui,
            "Month by month, the middle of the similar years, with the station's warming trend added. Temperatures within 0.3° and rain within 15% of normal count as near normal.",
            p.ink2,
        );
    });
}

fn short_temp(t: f64) -> &'static str {
    if t > ol::NEAR_TEMP {
        "Warmer"
    } else if t < -ol::NEAR_TEMP {
        "Cooler"
    } else {
        "Near normal"
    }
}

fn short_rain(r: f64) -> &'static str {
    if r > 1.0 + ol::NEAR_RAIN {
        "Wetter"
    } else if r < 1.0 - ol::NEAR_RAIN {
        "Drier"
    } else {
        "Normal rain"
    }
}
