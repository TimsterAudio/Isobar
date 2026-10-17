//! Now: the current temperature with its trend, the 24-hour thermograph,
//! and wind, pressure, rain, humidity and sun readouts with sparklines.

use eframe::egui::{
    self, pos2, text::LayoutJob, vec2, Align, Layout, Sense, Shape, Stroke, TextFormat, Ui,
};
use isobar_core::analytics::compass;
use isobar_core::LiveReading;

use crate::app::{Route, Snapshot};
use crate::chart::{self, Chart, Series, XAxis};
use crate::dial;
use crate::format::{self, num, signed};
use crate::settings::Settings;
use crate::theme::{self, alpha, Palette};
use crate::ui;

#[derive(Default)]
pub struct Now;

fn trend_word(v: f64, steady: f64) -> &'static str {
    if v.abs() < steady {
        "steady"
    } else if v > 0.0 {
        "rising"
    } else {
        "falling"
    }
}

fn uv_band(uv: f64) -> &'static str {
    match uv {
        u if u < 3.0 => "low",
        u if u < 6.0 => "moderate",
        u if u < 8.0 => "high",
        u if u < 11.0 => "very high",
        _ => "extreme",
    }
}

/// Seconds and values of one sampled metric.
fn series(snap: &Snapshot, key: &str) -> (Vec<f64>, Vec<Option<f64>>) {
    let pts = snap.samples.get(key).map(Vec::as_slice).unwrap_or(&[]);
    (
        pts.iter().map(|p| p.0 as f64 / 1000.0).collect(),
        pts.iter().map(|p| Some(p.1)).collect(),
    )
}

impl Now {
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        snap: &Snapshot,
        settings: &Settings,
        go: &mut Option<Route>,
    ) {
        let p = Palette::of(ui.ctx());
        let name = if snap.demo {
            "Demo station"
        } else {
            settings.station_name.as_str()
        };
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(if name.is_empty() { "My station" } else { name })
                    .font(theme::display(30.0))
                    .color(p.ink),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                self.updated(ui, snap, settings)
            });
        });

        let Some(r) = snap.live.as_ref() else {
            ui::panel(ui, |ui| {
                if snap.status.last_error.is_some() && settings.has_station() {
                    if ui::empty(
                        ui,
                        "Can't reach your station",
                        &format!(
                            "{}. Check the address in Settings is your MX interface (port 8998 by default) and that this computer can reach it.",
                            format::capitalise(snap.status.last_error.as_deref().unwrap_or_default())
                        ),
                        Some("Open settings"),
                    ) {
                        *go = Some(Route::Settings);
                    }
                } else if settings.has_station() {
                    ui::empty(
                        ui,
                        "Connecting…",
                        "Isobar is asking your station for its current conditions.",
                        None,
                    );
                } else {
                    match ui::empty_with(
                        ui,
                        "No live data yet",
                        "Add your Cumulus MX address in Settings and Isobar will start reading your station. No station handy? Look around with the demo station.",
                        &[("Open settings", ui::Kind::Primary), ("Try the demo station", ui::Kind::Secondary)],
                    ) {
                        Some(0) => *go = Some(Route::Settings),
                        Some(_) => *go = Some(Route::Demo),
                        None => {}
                    }
                }
            });
            return;
        };

        let wide = ui.available_width() >= 980.0;
        if wide {
            ui::split(
                ui,
                0.27,
                32.0,
                |ui| self.reading(ui, r),
                |ui| self.thermograph(ui, snap),
            );
        } else {
            self.reading(ui, r);
            self.thermograph(ui, snap);
        }

        ui.spacing_mut().item_spacing.y = 0.0;
        ui::rule(ui, p.grid);
        ui.spacing_mut().item_spacing.y = 24.0;
        let cols = if ui.available_width() >= 1180.0 { 5 } else { 3 };
        let instruments: [fn(&Self, &mut Ui, &Snapshot, &LiveReading); 5] = [
            Self::wind,
            Self::pressure,
            Self::rain,
            Self::humidity,
            Self::sun,
        ];
        for row in instruments.chunks(cols) {
            ui::columns(ui, cols, 32.0, true, |i, ui| {
                if let Some(f) = row.get(i) {
                    ui.add_space(16.0);
                    ui.spacing_mut().item_spacing.y = 6.0;
                    f(self, ui, snap, r);
                }
            });
        }
    }

    fn updated(&self, ui: &mut Ui, snap: &Snapshot, settings: &Settings) {
        let p = Palette::of(ui.ctx());
        let s = &snap.status;
        let stale = s
            .last_ok
            .is_none_or(|t| snap.now_ms - t > (settings.poll_seconds as i64 * 3000).max(120_000));
        let (color, text) = if !snap.demo && !settings.has_station() {
            (p.ink2, "No station connected".to_string())
        } else if !s.connected && s.last_error.is_some() {
            (
                p.heat,
                format!("Offline: {}", s.last_error.as_deref().unwrap_or_default()),
            )
        } else if stale {
            (
                p.warn,
                format!("Updated {}", format::ago(s.last_ok, snap.now_ms)),
            )
        } else {
            (
                p.ok,
                format!("Updated {}", format::ago(s.last_ok, snap.now_ms)),
            )
        };
        let text_color = if color == p.ok { p.ink2 } else { color };
        ui.label(
            egui::RichText::new(text)
                .font(theme::body(14.0))
                .color(text_color),
        );
        let (rect, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.0, color);
    }

    fn reading(&self, ui: &mut Ui, r: &LiveReading) {
        let p = Palette::of(ui.ctx());
        let u = &r.units;
        ui.spacing_mut().item_spacing.y = 10.0;
        ui.add_space(4.0);
        let mut job = LayoutJob::default();
        job.append(
            &num(r.temp, 1),
            0.0,
            TextFormat {
                font_id: theme::display(148.0),
                color: p.ink,
                extra_letter_spacing: -4.0,
                ..Default::default()
            },
        );
        job.append(
            &u.temp,
            6.0,
            TextFormat {
                font_id: theme::display_medium(44.0),
                color: p.ink2,
                valign: Align::TOP,
                ..Default::default()
            },
        );
        // Barlow's line box is much taller than its figures; pull the next
        // line up under them.
        ui.label(job);
        ui.add_space(-22.0);

        if let Some(tr) = r.temp_trend {
            let (dir, color) = if tr > 0.05 {
                (1, p.heat)
            } else if tr < -0.05 {
                (-1, p.cold)
            } else {
                (0, p.ink2)
            };
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                ui::arrow(ui, dir, color);
                ui::text(
                    ui,
                    format!("{}{} per hour", signed(Some(tr), 1), u.temp),
                    theme::medium(15.0),
                    color,
                );
            });
        }

        ui.add_space(4.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 28.0;
            for (label, v, time, color) in [
                ("High", r.temp_high, &r.temp_high_time, p.heat),
                ("Low", r.temp_low, &r.temp_low_time, p.cold),
            ] {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui::small(ui, label, p.ink2);
                    let mut job = LayoutJob::default();
                    job.append(
                        &format!("{}{}", num(v, 1), u.temp),
                        0.0,
                        TextFormat {
                            font_id: theme::display(27.0),
                            color,
                            ..Default::default()
                        },
                    );
                    if let Some(t) = time {
                        job.append(
                            t,
                            5.0,
                            TextFormat {
                                font_id: theme::body(13.0),
                                color: p.ink2,
                                valign: Align::BOTTOM,
                                ..Default::default()
                            },
                        );
                    }
                    ui.label(job);
                });
            }
        });

        let feels = r.feels_like.or(r.apparent);
        ui::text(
            ui,
            format!(
                "Feels like {}{}, dew point {}{}",
                num(feels, 1),
                u.temp,
                num(r.dew_point, 1),
                u.temp
            ),
            theme::body(15.0),
            p.ink2,
        );
        if let Some(f) = &r.forecast {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(f)
                        .font(theme::body(15.0))
                        .italics()
                        .color(p.ink2),
                )
                .wrap(),
            );
        }
    }

    /// The one loud thing: the 24-hour trace on graph paper.
    fn thermograph(&self, ui: &mut Ui, snap: &Snapshot) {
        let p = Palette::of(ui.ctx());
        let u = snap.units();
        let paper = ui.painter().add(Shape::Noop);
        let resp = egui::Frame::new()
            .stroke(Stroke::new(1.0, p.grid))
            .corner_radius(theme::RADIUS)
            .inner_margin(egui::Margin {
                left: 12,
                right: 16,
                top: 16,
                bottom: 12,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 8.0;
                let (xs, temp) = series(snap, "temp");
                if xs.len() > 1 {
                    let dew = snap
                        .samples
                        .get("dew")
                        .map(|d| align(&xs, d))
                        .unwrap_or_default();
                    Chart::new("thermograph", 262.0, xs, XAxis::Time)
                        .series(
                            Series::line("Temperature", temp, p.trace)
                                .width(2.5)
                                .span_gaps()
                                .no_points()
                                .unit(&u.temp, 1),
                        )
                        .series(
                            Series::line("Dew point", dew, alpha(p.ink2, 0.75))
                                .width(1.25)
                                .dashed()
                                .span_gaps()
                                .no_points()
                                .unit(&u.temp, 1),
                        )
                        .left_ticks(|v, step| format!("{}°", chart::plain_tick(v, step)))
                        .title(chart::ts_time)
                        .show(ui);
                } else {
                    let (rect, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 262.0), Sense::hover());
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "The 24-hour trace fills in as Isobar records readings, one per minute.",
                        theme::body(15.0),
                        p.ink2,
                    );
                }
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    ui.spacing_mut().item_spacing.x = 16.0;
                    ui::key(ui, "Temperature", p.trace, ui::KeyMark::Line);
                    ui::key(ui, "Dew point", p.ink2, ui::KeyMark::Dashed);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui::small(ui, "Last 24 hours", p.ink2);
                    });
                });
            });
        // Graph paper behind it all: a fine 12 px grid.
        let rect = resp.response.rect;
        let mut shapes = vec![Shape::rect_filled(rect, theme::RADIUS, p.sheet)];
        let fine = Stroke::new(1.0, alpha(p.faint, if p.dark { 0.8 } else { 1.0 }));
        let inner = rect.shrink(1.0);
        let mut x = inner.left() + 12.0;
        while x < inner.right() {
            shapes.push(Shape::line_segment(
                [pos2(x, inner.top()), pos2(x, inner.bottom())],
                fine,
            ));
            x += 12.0;
        }
        let mut y = inner.top() + 12.0;
        while y < inner.bottom() {
            shapes.push(Shape::line_segment(
                [pos2(inner.left(), y), pos2(inner.right(), y)],
                fine,
            ));
            y += 12.0;
        }
        ui.painter().set(paper, Shape::Vec(shapes));
    }

    fn title(ui: &mut Ui, s: &str) {
        let p = Palette::of(ui.ctx());
        ui::text(ui, s, theme::display_medium(18.0), p.ink2);
    }

    fn spark(ui: &mut Ui, snap: &Snapshot, key: &str, color: egui::Color32, fill: bool) {
        let (xs, ys) = series(snap, key);
        if xs.len() > 1 {
            let mut s = Series::line(key, ys, color).span_gaps().no_points();
            if fill {
                s = s.fill(alpha(color, 0.14));
            }
            Chart::sparkline(("spark", key), 44.0, xs)
                .series(s)
                .show(ui);
        } else {
            ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
        }
    }

    fn wind(&self, ui: &mut Ui, snap: &Snapshot, r: &LiveReading) {
        let p = Palette::of(ui.ctx());
        let u = &r.units;
        Self::title(ui, "Wind");
        dial::wind_dial(ui, 112.0, r.bearing, r.wind_avg, &u.wind);
        let from = r
            .bearing_compass
            .clone()
            .or_else(|| r.bearing.map(|b| compass(b).to_string()));
        let deg = r
            .bearing
            .map(|b| format!(" ({:.0}°)", b))
            .unwrap_or_default();
        ui::text(
            ui,
            format!("From {}{deg}", from.as_deref().unwrap_or("–")),
            theme::body(14.0),
            p.ink,
        );
        let mut job = LayoutJob::default();
        job.append(
            &format!(
                "Gusting {}, peak {} {}",
                num(r.gust_recent, 0),
                num(r.gust_high, 0),
                u.wind
            ),
            0.0,
            TextFormat {
                font_id: theme::body(14.0),
                color: p.ink,
                ..Default::default()
            },
        );
        if let Some(t) = &r.gust_high_time {
            job.append(
                &format!("at {t}"),
                4.0,
                TextFormat {
                    font_id: theme::body(12.0),
                    color: p.ink2,
                    ..Default::default()
                },
            );
        }
        ui.label(job);
        bottom(ui, |ui| Self::spark(ui, snap, "gust", p.wind, false));
    }

    fn pressure(&self, ui: &mut Ui, snap: &Snapshot, r: &LiveReading) {
        let p = Palette::of(ui.ctx());
        Self::title(ui, "Pressure");
        ui::reading(ui, &num(r.pressure, 1), &r.units.press, 44.0, p.ink);
        let trend = match (r.press_trend, &r.press_trend_text) {
            (Some(t), _) => format!("{}, {} per hour", trend_word(t, 0.1), signed(Some(t), 1)),
            (None, Some(text)) => text.clone(),
            _ => String::new(),
        };
        ui::text(ui, trend, theme::body(14.0), p.ink);
        ui::text(
            ui,
            format!("Today {} to {}", num(r.press_low, 1), num(r.press_high, 1)),
            theme::body(14.0),
            p.ink2,
        );
        bottom(ui, |ui| Self::spark(ui, snap, "press", p.trace, false));
    }

    fn rain(&self, ui: &mut Ui, snap: &Snapshot, r: &LiveReading) {
        let p = Palette::of(ui.ctx());
        let u = &r.units;
        Self::title(ui, "Rain");
        ui::reading(ui, &num(r.rain_today, 1), &u.rain, 44.0, p.ink);
        let rate = r.rain_rate.unwrap_or(0.0);
        if rate > 0.0 {
            ui::text(
                ui,
                format!("Raining {} {}/h", num(Some(rate), 1), u.rain),
                theme::semi(14.0),
                p.rain,
            );
        } else {
            ui::text(ui, "Not raining", theme::body(14.0), p.ink);
        }
        ui::text(
            ui,
            format!(
                "Month {} · year {} {}",
                num(r.rain_month, 1),
                num(r.rain_year, 1),
                u.rain
            ),
            theme::body(14.0),
            p.ink2,
        );
        bottom(ui, |ui| Self::spark(ui, snap, "rain_today", p.rain, true));
    }

    fn humidity(&self, ui: &mut Ui, snap: &Snapshot, r: &LiveReading) {
        let p = Palette::of(ui.ctx());
        Self::title(ui, "Humidity");
        ui::reading(ui, &num(r.humidity, 0), "%", 44.0, p.ink);
        ui::text(
            ui,
            format!("Dew point {}{}", num(r.dew_point, 1), r.units.temp),
            theme::body(14.0),
            p.ink,
        );
        ui::text(
            ui,
            format!("Today {}% to {}%", num(r.hum_low, 0), num(r.hum_high, 0)),
            theme::body(14.0),
            p.ink2,
        );
        bottom(ui, |ui| Self::spark(ui, snap, "hum", p.ink2, false));
    }

    fn sun(&self, ui: &mut Ui, snap: &Snapshot, r: &LiveReading) {
        let p = Palette::of(ui.ctx());
        Self::title(ui, "Sun");
        ui::reading(ui, &num(r.solar, 0), "W/m²", 44.0, p.ink);
        let uv = match r.uv {
            Some(uv) => format!("UV {}, {}", num(Some(uv), 1), uv_band(uv)),
            None => "No UV sensor".into(),
        };
        ui::text(ui, uv, theme::body(14.0), p.ink);
        let peak_uv = r
            .uv_high
            .map(|v| format!(", UV {}", num(Some(v), 1)))
            .unwrap_or_default();
        ui::text(
            ui,
            format!("Peak {} W/m²{peak_uv}", num(r.solar_high, 0)),
            theme::body(14.0),
            p.ink2,
        );
        bottom(ui, |ui| Self::spark(ui, snap, "solar", p.warn, true));
    }
}

/// Put `add` at the bottom of a 196 px instrument column.
fn bottom(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let top = ui.min_rect().top();
    let target = top + 196.0 + 10.0;
    let gap = (target - ui.cursor().top()).max(6.0);
    ui.add_space(gap);
    add(ui);
}

/// Values of `pts` at the timestamps `xs` (seconds), where they line up
/// within a minute.
fn align(xs: &[f64], pts: &[(i64, f64)]) -> Vec<Option<f64>> {
    xs.iter()
        .map(|x| {
            let ms = (*x * 1000.0) as i64;
            let i = pts.partition_point(|p| p.0 < ms - 60_000);
            pts.get(i)
                .filter(|p| (p.0 - ms).abs() <= 60_000)
                .map(|p| p.1)
        })
        .collect()
}
