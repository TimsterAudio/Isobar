//! Now: the current temperature with its trend, the 24-hour thermograph,
//! and wind, pressure, rain, humidity and sun readouts with sparklines.

use eframe::egui::{
    self, pos2, text::LayoutJob, vec2, Align, Layout, Sense, Shape, Stroke, TextFormat, Ui,
};
use isobar_core::analytics::{self as an, compass};
use isobar_core::LiveReading;

use crate::app::{Route, Snapshot};
use crate::chart::{self, Chart, Series, XAxis};
use crate::dial;
use crate::format::{self, num, signed};
use crate::settings::{NowCard, Settings};
use crate::theme::{self, alpha, Palette};
use crate::ui;
use crate::views::tanks;

/// A stretch longer than this with no readings (Isobar closed, station
/// offline) shows as a break in the 24-hour charts, not a straight line.
const GAP_SECS: f64 = 20.0 * 60.0;

/// The instrument cards: reading size, detail text size and sparkline height.
const READING: f32 = 34.0;
const DETAIL: f32 = 13.0;
const SPARK_H: f32 = 32.0;

#[derive(Default)]
pub struct Now {
    /// A new card order from a drag, for the app to save.
    pub reordered: Option<Vec<(NowCard, bool)>>,
    /// Readings the big chart needs from the history, for the app to ask
    /// the worker for: (key, metrics, from, to).
    pub request: Option<(String, Vec<String>, i64, i64)>,
    requested: Option<(String, std::time::Instant)>,
    tanks: Option<(String, Option<tanks::Glance>)>,
    /// The card shown in detail in the big chart; none is temperature.
    focus: Option<NowCard>,
    /// The chosen span for each, in hours.
    spans: std::collections::HashMap<Option<NowCard>, i64>,
}

/// The spans offered for each card's detail, in hours, and the default.
fn spans_for(card: Option<NowCard>) -> (&'static [i64], i64) {
    match card {
        None => (&[24, 72, 168, 720], 24),
        Some(NowCard::Pressure) => (&[24, 72, 168, 720], 72),
        Some(NowCard::Rain) => (&[24, 168, 720, 2160], 168),
        Some(NowCard::Tanks) => (&[720, 2160, 8760], 2160),
        Some(_) => (&[24, 72, 168], 24),
    }
}

fn span_label(h: i64) -> String {
    match h {
        24 => "24 hours".into(),
        h if h < 24 * 60 => format!("{} days", h / 24),
        h if h < 24 * 300 => format!("{} months", (h as f64 / (24.0 * 30.4)).round()),
        _ => "1 year".into(),
    }
}

/// `order` with `from` moved to where `to` is: before it when moving back,
/// after it when moving on.
pub fn move_card(order: &[(NowCard, bool)], from: NowCard, to: NowCard) -> Vec<(NowCard, bool)> {
    let mut out = order.to_vec();
    let (Some(a), Some(b)) = (
        out.iter().position(|c| c.0 == from),
        out.iter().position(|c| c.0 == to),
    ) else {
        return out;
    };
    let item = out.remove(a);
    out.insert(b, item);
    out
}

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
            ui::split_with(
                ui,
                0.27,
                32.0,
                self,
                |ui, s| s.reading(ui, r),
                |ui, s| s.thermograph(ui, snap),
            );
        } else {
            self.reading(ui, r);
            self.thermograph(ui, snap);
        }

        ui.spacing_mut().item_spacing.y = 0.0;
        ui::rule(ui, p.grid);
        ui.spacing_mut().item_spacing.y = 24.0;
        if let Some(t) = settings.tanks.as_ref().filter(|t| t.is_set_up()) {
            let key = format!(
                "{}/{}/{}/{:?}",
                snap.days_version,
                snap.tank_version,
                serde_json::to_string(t).unwrap_or_default(),
                tanks::today_key(snap)
            );
            if self.tanks.as_ref().is_none_or(|c| c.0 != key) {
                self.tanks = Some((key, tanks::glance(snap, t)));
            }
        } else {
            self.tanks = None;
        }
        let have_tanks = self.tanks.as_ref().is_some_and(|c| c.1.is_some());
        let order = settings.cards();
        let cards: Vec<NowCard> = order
            .iter()
            .filter(|(c, shown)| {
                *shown
                    && match c {
                        NowCard::Indoor => r.indoor_temp.is_some(),
                        NowCard::Tanks => have_tanks,
                        NowCard::Feels => r.feels_like.or(r.apparent).is_some(),
                        _ => true,
                    }
            })
            .map(|c| c.0)
            .collect();
        // All in one row while each card gets about 165 px (text wraps in
        // narrower cards); otherwise two even rows, or rows of three in a
        // narrow window.
        let n = cards.len().max(1);
        let width = ui.available_width();
        let cols = if width >= 1100.0 && width / n as f32 >= 165.0 {
            n
        } else if width >= 1100.0 {
            n.div_ceil(2)
        } else {
            3
        };
        let dragging = egui::DragAndDrop::has_payload_of_type::<NowCard>(ui.ctx());
        let mut dropped: Option<(NowCard, NowCard)> = None;
        for (k, row) in cards.chunks(cols).enumerate() {
            if k > 0 {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui::rule(ui, p.faint);
                ui.spacing_mut().item_spacing.y = 24.0;
            }
            ui::columns(ui, cols, 22.0, true, |i, ui| {
                let Some(&card) = row.get(i) else { return };
                // A click anywhere on the card (but its own controls) shows
                // it in the big chart; laid under the card using last
                // frame's size, so the card's widgets still come first.
                let rect_id = egui::Id::new(("now-card-rect", card));
                if let Some(prev) = ui.data(|d| d.get_temp::<egui::Rect>(rect_id)) {
                    let pick =
                        ui.interact(prev, egui::Id::new(("now-card-pick", card)), Sense::click());
                    if pick.hovered() && !dragging {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    let on = self.focus == Some(card);
                    if pick
                        .on_hover_text(if on {
                            "Back to temperature in the chart above"
                        } else {
                            "Show this in the chart above"
                        })
                        .clicked()
                    {
                        self.focus = if on { None } else { Some(card) };
                    }
                    if on {
                        ui.painter().line_segment(
                            [
                                pos2(prev.left(), prev.top() + 2.0),
                                pos2(prev.right(), prev.top() + 2.0),
                            ],
                            Stroke::new(2.0, p.trace),
                        );
                    }
                }
                ui.add_space(12.0);
                ui.spacing_mut().item_spacing.y = 4.0;
                // The title is the handle for dragging the card.
                let title = Self::title(ui, card.label());
                let handle = ui.interact(
                    title.rect.expand2(vec2(6.0, 3.0)),
                    egui::Id::new(("now-card", card)),
                    Sense::drag(),
                );
                handle.dnd_set_drag_payload(card);
                if handle.hovered() && !dragging {
                    handle.on_hover_text("Drag to move this card");
                }
                match card {
                    NowCard::Wind => self.wind(ui, snap, r),
                    NowCard::Pressure => self.pressure(ui, snap, r),
                    NowCard::Rain => self.rain(ui, snap, r),
                    NowCard::Humidity => self.humidity(ui, snap, r),
                    NowCard::Feels => self.feels(ui, snap, r),
                    NowCard::Sun => self.sun(ui, snap, r),
                    NowCard::Indoor => self.indoor(ui, snap, r),
                    NowCard::Tanks => {
                        if let Some(g) = self.tanks.as_ref().and_then(|c| c.1.as_ref()) {
                            Self::tank_card(ui, g);
                        }
                    }
                }
                let card_rect = ui.min_rect();
                ui.data_mut(|d| d.insert_temp(rect_id, card_rect));
                if dragging {
                    let rect = ui.min_rect().expand2(vec2(10.0, 4.0));
                    let zone = ui.interact(rect, egui::Id::new(("now-drop", card)), Sense::hover());
                    if let Some(src) = zone.dnd_hover_payload::<NowCard>() {
                        if *src != card {
                            ui.painter().rect_stroke(
                                rect,
                                theme::RADIUS,
                                Stroke::new(2.0, p.trace),
                                egui::StrokeKind::Inside,
                            );
                        }
                    }
                    if let Some(src) = zone.dnd_release_payload::<NowCard>() {
                        if *src != card {
                            dropped = Some((*src, card));
                        }
                    }
                }
            });
        }
        // The card being carried follows the pointer.
        if let (Some(src), Some(pos)) = (
            egui::DragAndDrop::payload::<NowCard>(ui.ctx()),
            ui.ctx().pointer_hover_pos(),
        ) {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::new("now-card-drag"),
            ));
            let galley =
                painter.layout_no_wrap(src.label().to_string(), theme::display_medium(16.0), p.ink);
            let r =
                egui::Rect::from_min_size(pos + vec2(12.0, 8.0), galley.size() + vec2(20.0, 10.0));
            painter.rect(
                r,
                theme::RADIUS,
                p.sheet,
                Stroke::new(1.0, p.trace),
                egui::StrokeKind::Inside,
            );
            painter.galley(r.min + vec2(10.0, 5.0), galley, p.ink);
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if let Some((from, to)) = dropped {
            self.reordered = Some(move_card(&order, from, to));
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
        } else if snap
            .live
            .as_ref()
            .is_some_and(|r| r.data_stopped == Some(true))
        {
            (
                p.warn,
                "Station stopped sending: MX is running but has no new readings".to_string(),
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
                            &format::clock(t),
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
    fn thermograph(&mut self, ui: &mut Ui, snap: &Snapshot) {
        let p = Palette::of(ui.ctx());
        let u = snap.units();
        let paper = ui.painter().add(Shape::Noop);
        // A card hidden or gone since it was picked: back to temperature.
        if self.focus == Some(NowCard::Tanks) && self.tanks.as_ref().is_none_or(|t| t.1.is_none()) {
            self.focus = None;
        }
        let focus = self.focus;
        let (options, default) = spans_for(focus);
        let hours = self
            .spans
            .get(&focus)
            .copied()
            .filter(|h| options.contains(h))
            .unwrap_or(default);
        if focus.is_some() || hours != 24 {
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
                    self.detail(ui, snap, focus, hours, &p);
                });
            graph_paper(ui, resp.response.rect, paper, &p);
            self.span_row(ui, focus);
            return;
        }
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
                    // Rain since midnight on its own scale at the right, drawn
                    // first so the temperature trace sits on top of it.
                    let rain = rain_today(snap);
                    let show_rain = rain.iter().any(|p| p.1 > 0.0);
                    let mut chart = Chart::new("thermograph", 226.0, xs.clone(), XAxis::Time);
                    if show_rain {
                        chart = chart
                            .series(
                                Series::line("Rain since midnight", align(&xs, &rain), p.rain)
                                    .width(1.25)
                                    .fill(alpha(p.rain, 0.12))
                                    .right()
                                    .span_gaps()
                                    .max_gap(GAP_SECS)
                                    .no_points()
                                    .unit(format!(" {}", u.rain), 1),
                            )
                            .right_from_zero();
                    }
                    chart
                        .series(
                            Series::line("Temperature", temp, p.trace)
                                .width(2.5)
                                .span_gaps()
                                .max_gap(GAP_SECS)
                                .no_points()
                                .unit(&u.temp, 1),
                        )
                        .series(
                            Series::line("Dew point", dew, alpha(p.ink2, 0.75))
                                .width(1.25)
                                .dashed()
                                .span_gaps()
                                .max_gap(GAP_SECS)
                                .no_points()
                                .unit(&u.temp, 1),
                        )
                        .left_ticks(|v, step| format!("{}°", chart::plain_tick(v, step)))
                        .title(chart::ts_time)
                        .show(ui);
                } else {
                    let (rect, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 226.0), Sense::hover());
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
                    if rain_today(snap).iter().any(|p| p.1 > 0.0) {
                        ui::key(
                            ui,
                            &format!("Rain since midnight ({})", u.rain),
                            p.rain,
                            ui::KeyMark::Square,
                        );
                    }
                });
            });
        graph_paper(ui, resp.response.rect, paper, &p);
        self.span_row(ui, None);
    }

    /// The span chips, and the way back to temperature, in a row under the
    /// big chart.
    fn span_row(&mut self, ui: &mut Ui, focus: Option<NowCard>) {
        ui.add_space(-12.0);
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), 32.0),
            Layout::right_to_left(Align::Center),
            |ui| {
                span_chips(ui, &mut self.spans, focus);
                if focus.is_some()
                    && ui::chip(ui, "Temperature", false)
                        .on_hover_text("Back to the thermograph")
                        .clicked()
                {
                    self.focus = None;
                }
            },
        );
    }

    /// The big chart showing one card's measure over a chosen span.
    fn detail(
        &mut self,
        ui: &mut Ui,
        snap: &Snapshot,
        focus: Option<NowCard>,
        hours: i64,
        p: &Palette,
    ) {
        let u = snap.units();
        let height = 226.0;
        let to = snap.now_ms;
        let from = to - hours * 3_600_000;
        let span_secs = (hours * 3600) as f64;
        // Readings from the history, asked of the worker; the last day
        // comes straight from the live samples.
        let metrics: &[&str] = match focus {
            None => &["temp", "dew", "rain_today"],
            Some(NowCard::Wind) => &["wind", "gust"],
            Some(NowCard::Pressure) => &["press"],
            Some(NowCard::Humidity) => &["hum"],
            Some(NowCard::Sun) => &["solar", "solar_max"],
            Some(NowCard::Indoor) => &["intemp", "temp"],
            Some(NowCard::Feels) => &["feels", "temp"],
            _ => &[],
        };
        let mut data: Option<&crate::worker::Samples> = None;
        if !metrics.is_empty() {
            let key = format!("{focus:?}-{hours}");
            let fresh = snap
                .detail
                .as_ref()
                .filter(|d| d.key == key)
                .map(|d| d.at.elapsed().as_secs() < 120);
            let asked = self
                .requested
                .as_ref()
                .is_some_and(|(k, at)| *k == key && at.elapsed().as_secs() < 5);
            if fresh != Some(true) && !asked {
                self.request = Some((
                    key.clone(),
                    metrics.iter().map(|m| m.to_string()).collect(),
                    from,
                    to,
                ));
                self.requested = Some((key.clone(), std::time::Instant::now()));
            }
            data = snap
                .detail
                .as_ref()
                .filter(|d| d.key == key)
                .map(|d| &d.series);
        }
        let pts = |k: &str| -> Vec<(i64, f64)> {
            if hours <= 24 && k != "solar_max" {
                snap.samples.get(k).cloned().unwrap_or_default()
            } else {
                data.and_then(|d| d.get(k)).cloned().unwrap_or_default()
            }
        };
        let covered = |v: &[(i64, f64)]| match (v.first(), v.last()) {
            (Some(a), Some(b)) => (b.0 - a.0) as f64 / 1000.0 / span_secs,
            _ => 0.0,
        };
        // Daily highs and lows from the history, for long spans the detailed
        // readings don't reach (MX's logs not fetched yet, or none kept).
        let daily = |hi: fn(&isobar_core::DayRecord) -> Option<f64>,
                     lo: fn(&isobar_core::DayRecord) -> Option<f64>| {
            let start = chart::ts_day((from / 1000) as f64);
            let days: Vec<&isobar_core::DayRecord> =
                snap.days.iter().filter(|d| d.date >= start).collect();
            let xs: Vec<f64> = days
                .iter()
                .map(|d| chart::day_ts(d.date) + 12.0 * 3600.0)
                .collect();
            let a: Vec<Option<f64>> = days.iter().map(|d| hi(d)).collect();
            let b: Vec<Option<f64>> = days.iter().map(|d| lo(d)).collect();
            (xs, a, b)
        };
        let mut note: Option<String> = None;
        let mut keys: Vec<(String, egui::Color32, ui::KeyMark)> = Vec::new();
        let gap = |n: usize| GAP_SECS.max(3.0 * span_secs / n.max(1) as f64);
        match focus {
            Some(NowCard::Rain) if hours > 24 => {
                // Daily totals, today's so far from the live reading.
                let start = chart::ts_day((from / 1000) as f64);
                let today = chrono::Local::now().date_naive();
                let mut days: Vec<(chrono::NaiveDate, Option<f64>)> = snap
                    .days
                    .iter()
                    .filter(|d| d.date >= start && d.date < today)
                    .map(|d| (d.date, d.rain))
                    .collect();
                days.push((today, snap.live.as_ref().and_then(|r| r.rain_today)));
                let total: f64 = days.iter().filter_map(|d| d.1).sum();
                let xs: Vec<f64> = days
                    .iter()
                    .map(|d| chart::day_ts(d.0) + 12.0 * 3600.0)
                    .collect();
                Chart::new(("detail-rain", hours), height, xs, XAxis::Time)
                    .series(
                        Series::line("Rain", days.iter().map(|d| d.1).collect(), p.rain)
                            .bars()
                            .unit(format!(" {}", u.rain), 1),
                    )
                    .title(|x| format::nice_date(chart::ts_day(x)))
                    .show(ui);
                keys.push((
                    format!("Rain each day ({}), {} in all", u.rain, num(Some(total), 1)),
                    p.rain,
                    ui::KeyMark::Square,
                ));
            }
            Some(NowCard::Rain) => {
                let rain = rain_today(snap);
                let xs: Vec<f64> = rain.iter().map(|r| r.0 as f64 / 1000.0).collect();
                let rate = snap
                    .samples
                    .get("rain_rate")
                    .map(|r| align(&xs, r))
                    .unwrap_or_default();
                Chart::new("detail-rain-day", height, xs, XAxis::Time)
                    .series(
                        Series::line(
                            "Rain since midnight",
                            rain.iter().map(|r| Some(r.1)).collect(),
                            p.rain,
                        )
                        .fill(alpha(p.rain, 0.14))
                        .no_points()
                        .max_gap(GAP_SECS)
                        .unit(format!(" {}", u.rain), 1),
                    )
                    .series(
                        Series::line("Rate", rate, alpha(p.ink2, 0.8))
                            .width(1.0)
                            .dashed()
                            .right()
                            .no_points()
                            .max_gap(GAP_SECS)
                            .unit(format!(" {}/h", u.rain), 1),
                    )
                    .left_from_zero()
                    .right_from_zero()
                    .title(chart::ts_time)
                    .show(ui);
                keys.push((
                    format!("Rain since midnight ({})", u.rain),
                    p.rain,
                    ui::KeyMark::Square,
                ));
                keys.push((
                    format!("Rate ({}/h, right)", u.rain),
                    p.ink2,
                    ui::KeyMark::Dashed,
                ));
            }
            Some(NowCard::Tanks) => {
                if let Some(g) = self.tanks.as_ref().and_then(|t| t.1.as_ref()) {
                    let start = chart::ts_day((from / 1000) as f64);
                    let rows: Vec<&(chrono::NaiveDate, f64)> =
                        g.year.iter().filter(|d| d.0 >= start).collect();
                    let xs: Vec<f64> = rows.iter().map(|d| chart::day_ts(d.0)).collect();
                    Chart::new(("detail-tanks", hours), height, xs.clone(), XAxis::Time)
                        .series(
                            Series::line("Level", rows.iter().map(|d| Some(d.1)).collect(), p.rain)
                                .fill(alpha(p.rain, 0.14))
                                .no_points()
                                .unit(" L", 0),
                        )
                        .series(
                            Series::line(
                                "Capacity",
                                vec![Some(g.capacity); xs.len()],
                                alpha(p.ink2, 0.5),
                            )
                            .width(1.0)
                            .no_points()
                            .hidden_in_readout(),
                        )
                        .left_from_zero()
                        .left_ticks(|v, _| {
                            if v == 0.0 {
                                "0".into()
                            } else {
                                format!("{}k", num(Some(v / 1000.0), 0))
                            }
                        })
                        .title(|x| format::nice_date(chart::ts_day(x)))
                        .show(ui);
                    keys.push(("Tank level (litres)".into(), p.rain, ui::KeyMark::Square));
                }
            }
            _ => {
                // Line charts: the first metric leads, the second aligns to it.
                let (a_key, b_key) = (metrics[0], metrics.get(1).copied());
                let a = pts(a_key);
                let fallback = hours > 24 && covered(&a) < 0.5;
                let (label_a, color_a, unit_a, label_b, color_b, unit_b, dashed_b): (
                    String,
                    egui::Color32,
                    String,
                    String,
                    egui::Color32,
                    String,
                    bool,
                ) = match focus {
                    None => (
                        "Temperature".into(),
                        p.trace,
                        u.temp.clone(),
                        "Dew point".into(),
                        p.ink2,
                        u.temp.clone(),
                        true,
                    ),
                    Some(NowCard::Wind) => (
                        "Average".into(),
                        p.wind,
                        format!(" {}", u.wind),
                        "Gust".into(),
                        alpha(p.wind, 0.5),
                        format!(" {}", u.wind),
                        false,
                    ),
                    Some(NowCard::Pressure) => (
                        "Pressure".into(),
                        p.trace,
                        format!(" {}", u.press),
                        String::new(),
                        p.ink2,
                        String::new(),
                        false,
                    ),
                    Some(NowCard::Humidity) => (
                        "Humidity".into(),
                        p.trace,
                        "%".into(),
                        String::new(),
                        p.ink2,
                        String::new(),
                        false,
                    ),
                    Some(NowCard::Sun) => (
                        "Solar".into(),
                        p.warn,
                        " W/m²".into(),
                        "Clear-sky maximum".into(),
                        p.ink2,
                        " W/m²".into(),
                        true,
                    ),
                    Some(NowCard::Feels) => (
                        "Feels like".into(),
                        p.trace,
                        u.temp.clone(),
                        "Temperature".into(),
                        p.ink2,
                        u.temp.clone(),
                        true,
                    ),
                    _ => (
                        "Inside".into(),
                        p.trace,
                        u.temp.clone(),
                        "Outside".into(),
                        p.ink2,
                        u.temp.clone(),
                        false,
                    ),
                };
                if fallback {
                    type Pick = fn(&isobar_core::DayRecord) -> Option<f64>;
                    let pair: Option<(Pick, Pick, &str)> = match focus {
                        None => Some((|d| d.temp_max, |d| d.temp_min, "Daily high and low")),
                        Some(NowCard::Wind) => Some((
                            |d| d.gust_max,
                            |d| d.wind_avg_max,
                            "Daily peak gust and highest average",
                        )),
                        Some(NowCard::Pressure) => {
                            Some((|d| d.press_max, |d| d.press_min, "Daily high and low"))
                        }
                        Some(NowCard::Humidity) => {
                            Some((|d| d.hum_max, |d| d.hum_min, "Daily high and low"))
                        }
                        Some(NowCard::Sun) => Some((|d| d.solar_max, |_| None, "Daily peak")),
                        Some(NowCard::Feels) => {
                            Some((|d| d.feels_max, |d| d.feels_min, "Daily high and low"))
                        }
                        _ => None,
                    };
                    if let Some((hi, lo, what)) = pair {
                        let (xs, h, l) = daily(hi, lo);
                        let daily_chart = Chart::new(
                            ("detail-daily", focus.map(|c| c.id()), hours),
                            height,
                            xs,
                            XAxis::Time,
                        )
                        .series(
                            Series::line("High", h, color_a)
                                .no_points()
                                .unit(&unit_a, 1),
                        )
                        .series(
                            Series::line("Low", l, alpha(color_a, 0.6))
                                .no_points()
                                .unit(&unit_a, 1),
                        )
                        .band(0, 1, alpha(color_a, 0.1));
                        // Each day's rain on its own scale, for temperature.
                        let rain_days: Vec<Option<f64>> = if focus.is_none() {
                            let start = chart::ts_day((from / 1000) as f64);
                            snap.days
                                .iter()
                                .filter(|d| d.date >= start)
                                .map(|d| d.rain)
                                .collect()
                        } else {
                            Vec::new()
                        };
                        let rained = rain_days.iter().any(|r| r.is_some_and(|r| r > 0.0));
                        let daily_chart = if rained {
                            daily_chart
                                .series(
                                    Series::line("Rain", rain_days, alpha(p.rain, 0.55))
                                        .bars()
                                        .right()
                                        .unit(format!(" {}", u.rain), 1),
                                )
                                .right_from_zero()
                        } else {
                            daily_chart
                        };
                        daily_chart
                            .title(|x| format::nice_date(chart::ts_day(x)))
                            .show(ui);
                        keys.push((what.into(), color_a, ui::KeyMark::Line));
                        if rained {
                            keys.push((
                                format!("Rain each day ({}, right)", u.rain),
                                p.rain,
                                ui::KeyMark::Square,
                            ));
                        }
                        note = Some(if snap.demo {
                            "Daily figures: the demo station only has a day of detailed readings."
                                .into()
                        } else {
                            "Daily figures for now. The detailed readings fill in once Isobar has fetched MX's log files (see Health).".into()
                        });
                    }
                } else if a.len() > 1 {
                    let xs: Vec<f64> = a.iter().map(|q| q.0 as f64 / 1000.0).collect();
                    let g = gap(a.len());
                    let mut c = Chart::new(
                        ("detail", focus.map(|c| c.id()), hours),
                        height,
                        xs.clone(),
                        XAxis::Time,
                    )
                    .series(
                        Series::line(
                            label_a.clone(),
                            a.iter().map(|q| Some(q.1)).collect(),
                            color_a,
                        )
                        .width(2.0)
                        .span_gaps()
                        .max_gap(g)
                        .no_points()
                        .unit(&unit_a, 1),
                    )
                    .title(if hours <= 24 {
                        chart::ts_time
                    } else {
                        |x| {
                            format!(
                                "{}, {}",
                                format::nice_date(chart::ts_day(x)),
                                chart::ts_time(x)
                            )
                        }
                    });
                    if let Some(b) = b_key {
                        let mut line = Series::line(label_b.clone(), align(&xs, &pts(b)), color_b)
                            .width(1.25)
                            .span_gaps()
                            .max_gap(g)
                            .no_points()
                            .unit(&unit_b, 1);
                        if dashed_b {
                            line = line.dashed();
                        }
                        c = c.series(line);
                    }
                    if matches!(
                        focus,
                        Some(NowCard::Wind) | Some(NowCard::Sun) | Some(NowCard::Humidity)
                    ) {
                        c = c.left_from_zero();
                    }
                    // Rain since midnight on its own scale, starting again
                    // each midnight, as on the 24-hour thermograph.
                    let mut rained = false;
                    if focus.is_none() {
                        let rain =
                            isobar_core::analytics::rain_since_midnight(&pts("rain_today"), |t| {
                                chart::ts_day(t as f64 / 1000.0)
                            });
                        if rain.iter().any(|r| r.1 > 0.0) {
                            rained = true;
                            c = c
                                .series(
                                    Series::line("Rain since midnight", align(&xs, &rain), p.rain)
                                        .width(1.25)
                                        .fill(alpha(p.rain, 0.12))
                                        .right()
                                        .span_gaps()
                                        .max_gap(g)
                                        .no_points()
                                        .unit(format!(" {}", u.rain), 1),
                                )
                                .right_from_zero();
                        }
                    }
                    c.show(ui);
                    keys.push((label_a, color_a, ui::KeyMark::Line));
                    if b_key.is_some() {
                        keys.push((
                            label_b,
                            color_b,
                            if dashed_b {
                                ui::KeyMark::Dashed
                            } else {
                                ui::KeyMark::Line
                            },
                        ));
                    }
                    if rained {
                        keys.push((
                            format!("Rain since midnight ({})", u.rain),
                            p.rain,
                            ui::KeyMark::Square,
                        ));
                    }
                } else {
                    let (rect, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Loading readings…",
                        theme::body(15.0),
                        p.ink2,
                    );
                }
            }
        }
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.spacing_mut().item_spacing.x = 16.0;
            for (label, color, mark) in &keys {
                ui::key(ui, label, *color, *mark);
            }
            if let Some(n) = &note {
                ui::small(ui, n, p.ink2);
            }
        });
    }

    fn title(ui: &mut Ui, s: &str) -> egui::Response {
        let p = Palette::of(ui.ctx());
        ui::text(ui, s, theme::display_medium(16.0), p.ink2)
    }

    fn tank_card(ui: &mut Ui, g: &tanks::Glance) {
        let p = Palette::of(ui.ctx());
        ui::reading(ui, &tanks::litres(g.level), "L", READING, p.ink);
        ui::text(
            ui,
            format!("{}% full", num(Some(g.level / g.capacity * 100.0), 0)),
            theme::body(DETAIL),
            p.ink,
        );
        let today = chrono::Local::now().date_naive();
        ui::text(
            ui,
            match g.dry_low {
                Some(d) => format!(
                    "No rain: {} days, to {}",
                    (d - today).num_days().max(0),
                    format::short_date(d)
                ),
                None => "No rain: lasts over a year".into(),
            },
            theme::body(DETAIL),
            p.ink2,
        );
        ui::text(
            ui,
            format!(
                "30 days: {} L in, {} L over",
                tanks::litres(g.caught30),
                tanks::litres(g.overflow30)
            ),
            theme::body(DETAIL),
            p.ink2,
        );
        bottom(ui, |ui| {
            if g.recent.len() > 1 {
                let xs: Vec<f64> = g.recent.iter().map(|d| chart::day_ts(d.0)).collect();
                let ys: Vec<Option<f64>> = g.recent.iter().map(|d| Some(d.1)).collect();
                Chart::sparkline("spark-tanks", SPARK_H, xs)
                    .series(
                        Series::line("Level", ys, p.rain)
                            .no_points()
                            .fill(alpha(p.rain, 0.14)),
                    )
                    .show(ui);
            }
        });
    }

    fn spark(ui: &mut Ui, snap: &Snapshot, key: &str, color: egui::Color32, fill: bool) {
        let (xs, ys) = series(snap, key);
        if xs.len() > 1 {
            let mut s = Series::line(key, ys, color)
                .span_gaps()
                .max_gap(GAP_SECS)
                .no_points();
            if fill {
                s = s.fill(alpha(color, 0.14));
            }
            Chart::sparkline(("spark", key), SPARK_H, xs)
                .series(s)
                .show(ui);
        } else {
            ui.allocate_exact_size(vec2(ui.available_width(), SPARK_H), Sense::hover());
        }
    }

    fn wind(&self, ui: &mut Ui, snap: &Snapshot, r: &LiveReading) {
        let p = Palette::of(ui.ctx());
        let u = &r.units;
        let shown = r.shown_bearing();
        dial::wind_dial(ui, 84.0, shown.map(|s| s.0), r.wind_avg, &u.wind);
        // MX's compass point belongs to the latest bearing; work one out when
        // the dial shows the average instead.
        let from = match shown {
            Some((b, false)) => r
                .bearing_compass
                .clone()
                .unwrap_or_else(|| compass(b).to_string()),
            Some((b, true)) => compass(b).to_string(),
            None => "–".into(),
        };
        let deg = shown
            .map(|(b, _)| format!(" ({b:.0}°)"))
            .unwrap_or_default();
        let line = match shown {
            Some((_, true)) if r.wind_avg.unwrap_or(0.0) <= 0.0 => {
                format!("Calm, mostly from {from}{deg}")
            }
            Some((_, true)) => format!("Light, mostly from {from}{deg}"),
            _ => format!("From {from}{deg}"),
        };
        let resp = ui::text(ui, line, theme::body(DETAIL), p.ink);
        if shown.is_some_and(|s| s.1) {
            resp.on_hover_text("In light air the dial shows the 10-minute average direction, which reads truer than the latest.");
        }
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
                font_id: theme::body(DETAIL),
                color: p.ink,
                ..Default::default()
            },
        );
        if let Some(t) = &r.gust_high_time {
            job.append(
                &format!("at {}", format::clock(t)),
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
        ui::reading(ui, &num(r.pressure, 1), &r.units.press, READING, p.ink);
        // The three-hour tendency in forecasters' words, from the recorded
        // readings; MX's own hourly trend until there are three hours.
        let pts = snap.samples.get("press").map(Vec::as_slice).unwrap_or(&[]);
        let per = an::hpa_per_unit(&r.units.press);
        let (trend, color) = match (
            an::three_hour_change(pts),
            r.press_trend,
            &r.press_trend_text,
        ) {
            (Some(c), _, _) => {
                let hpa = c * per;
                let digits = if per > 5.0 { 2 } else { 1 };
                let big_fall = hpa <= -3.6;
                (
                    format!(
                        "{}, {} {} in 3 h",
                        an::pressure_tendency(hpa),
                        signed(Some(c), digits),
                        r.units.press
                    ),
                    if big_fall { p.warn } else { p.ink },
                )
            }
            (None, Some(t), _) => (
                format!("{}, {} per hour", trend_word(t, 0.1), signed(Some(t), 1)),
                p.ink,
            ),
            (None, None, Some(text)) => (text.clone(), p.ink),
            _ => (String::new(), p.ink),
        };
        let resp = ui::text(ui, trend, theme::body(DETAIL), color);
        if color == p.warn {
            resp.on_hover_text("A big fall in three hours: often wind and rain on the way.");
        }
        ui::text(
            ui,
            format!("Today {} to {}", num(r.press_low, 1), num(r.press_high, 1)),
            theme::body(DETAIL),
            p.ink2,
        );
        bottom(ui, |ui| Self::spark(ui, snap, "press", p.trace, false));
    }

    fn rain(&self, ui: &mut Ui, snap: &Snapshot, r: &LiveReading) {
        let p = Palette::of(ui.ctx());
        let u = &r.units;
        ui::reading(ui, &num(r.rain_today, 1), &u.rain, READING, p.ink);
        let rate = r.rain_rate.unwrap_or(0.0);
        if rate > 0.0 {
            ui::text(
                ui,
                format!("Raining, {} {}/h", num(Some(rate), 1), u.rain),
                theme::semi(DETAIL),
                p.rain,
            );
        } else {
            ui::text(
                ui,
                format!("Not raining, rate {} {}/h", num(r.rain_rate, 1), u.rain),
                theme::body(DETAIL),
                p.ink,
            );
        }
        if r.storm_rain.is_some_and(|s| s > 0.0) {
            let since = r
                .storm_start
                .as_deref()
                .map(storm_since)
                .unwrap_or_default();
            ui::text(
                ui,
                format!("Storm {} {}{since}", num(r.storm_rain, 1), u.rain),
                theme::body(DETAIL),
                p.rain,
            );
        }
        ui::text(
            ui,
            format!(
                "Yesterday {} · 24 h {}",
                num(r.rain_yesterday, 1),
                num(r.rain_24h, 1)
            ),
            theme::body(DETAIL),
            p.ink2,
        );
        ui::text(
            ui,
            format!(
                "Week {} · month {} · year {}",
                num(r.rain_week, 1),
                num(r.rain_month, 1),
                num(r.rain_year, 1)
            ),
            theme::body(DETAIL),
            p.ink2,
        );
        if rate <= 0.0 {
            if let Some((when, ago)) = r
                .last_rain_tip
                .as_deref()
                .and_then(|t| format::station_moment(t, snap.now_ms))
            {
                ui::text(
                    ui,
                    format!("Last rain {when} ({ago})"),
                    theme::body(13.0),
                    p.ink2,
                );
            }
        }
        bottom(ui, |ui| Self::spark(ui, snap, "rain_today", p.rain, true));
    }

    fn humidity(&self, ui: &mut Ui, snap: &Snapshot, r: &LiveReading) {
        let p = Palette::of(ui.ctx());
        ui::reading(ui, &num(r.humidity, 0), "%", READING, p.ink);
        ui::text(
            ui,
            format!("Dew point {}{}", num(r.dew_point, 1), r.units.temp),
            theme::body(DETAIL),
            p.ink,
        );
        ui::text(
            ui,
            format!("Today {}% to {}%", num(r.hum_low, 0), num(r.hum_high, 0)),
            theme::body(DETAIL),
            p.ink2,
        );
        if let (Some(lo), Some(hi)) = (r.dew_low, r.dew_high) {
            let at = |t: &Option<String>| {
                t.as_deref()
                    .map_or(String::new(), |t| format!(" at {}", format::clock(t)))
            };
            ui::text(
                ui,
                format!("Dew today {} to {}{}", num(Some(lo), 1), num(Some(hi), 1), r.units.temp),
                theme::body(DETAIL),
                p.ink2,
            )
            .on_hover_text(format!(
                "Lowest{}, highest{}. Dew point is the honest measure of mugginess: under about 10° feels dry, over 16° sticky, over 20° oppressive.",
                at(&r.dew_low_time),
                at(&r.dew_high_time)
            ));
        }
        if let Some(cb) = r.cloud_base {
            let unit = r.cloud_base_unit.as_deref().unwrap_or("m");
            ui::text(
                ui,
                format!("Cloud base about {} {unit}", tanks::litres(cb)),
                theme::body(DETAIL),
                p.ink2,
            )
            .on_hover_text("Where cumulus cloud would form, worked out by MX from the gap between temperature and dew point: about 125 m (400 ft) for each degree.");
        }
        bottom(ui, |ui| Self::spark(ui, snap, "hum", p.ink2, false));
    }

    /// Feels like: apparent temperature, wind chill, heat index and humidex,
    /// with today's extremes.
    fn feels(&self, ui: &mut Ui, snap: &Snapshot, r: &LiveReading) {
        let p = Palette::of(ui.ctx());
        let u = &r.units.temp;
        ui::reading(ui, &num(r.feels_like.or(r.apparent), 1), u, READING, p.ink).on_hover_text(
            "MX's feels-like temperature: wind chill when it's cool, heat index when it's warm.",
        );
        let at = |t: &Option<String>| {
            t.as_deref()
                .map_or(String::new(), |t| format!(" at {}", format::clock(t)))
        };
        // One line each: now, then today's extreme; the times on hover.
        let lines = [
            (
                "Apparent",
                r.apparent,
                match (r.apparent_low, r.apparent_high) {
                    (Some(lo), Some(hi)) => Some(format!("{} to {}", num(Some(lo), 1), num(Some(hi), 1))),
                    _ => None,
                },
                format!(
                    "The Australian apparent temperature: the air adjusted for humidity and wind. Today's lowest{}, highest{}.",
                    at(&r.apparent_low_time),
                    at(&r.apparent_high_time)
                ),
                true,
            ),
            (
                "Wind chill",
                r.wind_chill,
                r.chill_low.map(|v| format!("low {}", num(Some(v), 1))),
                format!("How cold the wind makes it feel on bare skin. Today's lowest{}.", at(&r.chill_low_time)),
                true,
            ),
            (
                "Heat index",
                r.heat_index,
                r.heat_index_high.map(|v| format!("high {}", num(Some(v), 1))),
                format!(
                    "How hot humid air feels, the US way. It only parts from the temperature from about 27°C (80°F). Today's highest{}.",
                    at(&r.heat_index_high_time)
                ),
                true,
            ),
            (
                "Humidex",
                r.humidex,
                r.humidex_high.map(|v| format!("high {}", num(Some(v), 1))),
                format!(
                    "The Canadian measure of humid heat: over 30 some discomfort, over 40 great discomfort, over 45 dangerous. Today's highest{}.",
                    at(&r.humidex_high_time)
                ),
                false,
            ),
        ];
        for (k, (label, now, today, hover, degrees)) in lines.into_iter().enumerate() {
            if now.is_none() && today.is_none() {
                continue;
            }
            let unit = if degrees { u.as_str() } else { "" };
            let mut line = format!("{label} {}{unit}", num(now, 1));
            if let Some(t) = today {
                line.push_str(&format!(" · {t}"));
            }
            ui::text(
                ui,
                line,
                theme::body(DETAIL),
                if k == 0 { p.ink } else { p.ink2 },
            )
            .on_hover_text(hover);
        }
        bottom(ui, |ui| Self::spark(ui, snap, "feels", p.heat, false));
    }

    fn sun(&self, ui: &mut Ui, snap: &Snapshot, r: &LiveReading) {
        let p = Palette::of(ui.ctx());
        ui::reading(ui, &num(r.solar, 0), "W/m²", READING, p.ink)
            .on_hover_text(format!("Peak today {} W/m²", num(r.solar_high, 0)));
        // How much of the clear-sky sunshine is getting through.
        match (r.solar, r.solar_max_now) {
            (Some(now), Some(max)) if max >= 20.0 => {
                let frac = (now / max).clamp(0.0, 1.0);
                let sky = match frac {
                    f if f >= 0.85 => "Clear",
                    f if f >= 0.6 => "Hazy",
                    f if f >= 0.3 => "Cloudy",
                    _ => "Overcast",
                };
                let resp = ui
                    .horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui::meter(ui, 48.0, frac as f32, p.warn);
                        ui::text(
                            ui,
                            format!("{sky}, {:.0}% of clear sky", frac * 100.0),
                            theme::body(DETAIL),
                            p.ink,
                        );
                    })
                    .response;
                resp.on_hover_text(format!(
                    "Sunshine now against the {} W/m² a clear sky would give at this time of day.",
                    num(Some(max), 0)
                ));
            }
            (_, Some(_)) => {
                ui::text(ui, "Sun down", theme::body(DETAIL), p.ink);
            }
            _ => {}
        }
        let uv = match r.uv {
            Some(uv) => {
                let peak = r
                    .uv_high
                    .map(|v| format!(" (peak {})", num(Some(v), 1)))
                    .unwrap_or_default();
                format!("UV {}, {}{peak}", num(Some(uv), 1), uv_band(uv))
            }
            None => "No UV sensor".into(),
        };
        ui::text(ui, uv, theme::body(DETAIL), p.ink);
        if let Some(h) = r.sunshine_hours {
            ui::text(
                ui,
                format!("Sunshine {} h today", num(Some(h), 1)),
                theme::body(DETAIL),
                p.ink,
            );
        }
        if let (Some(rise), Some(set)) = (&r.sunrise, &r.sunset) {
            ui::text(
                ui,
                format!(
                    "Sunrise {} · sunset {}",
                    format::clock(rise),
                    format::clock(set)
                ),
                theme::body(DETAIL),
                p.ink2,
            );
            if let Some(len) = format::span_between(rise, set) {
                let moon = match (&r.moonrise, &r.moonset) {
                    (Some(mr), Some(ms)) => format!(
                        "Moonrise {} · moonset {}",
                        format::clock(mr),
                        format::clock(ms)
                    ),
                    _ => String::new(),
                };
                let resp = ui::text(ui, format!("Day length {len}"), theme::body(DETAIL), p.ink2);
                if !moon.is_empty() {
                    resp.on_hover_text(moon);
                }
            }
        }
        bottom(ui, |ui| Self::spark(ui, snap, "solar", p.warn, true));
    }

    fn indoor(&self, ui: &mut Ui, snap: &Snapshot, r: &LiveReading) {
        let p = Palette::of(ui.ctx());
        let u = &r.units;
        ui::reading(ui, &num(r.indoor_temp, 1), &u.temp, READING, p.ink);
        ui::text(
            ui,
            format!("Humidity {}%", num(r.indoor_hum, 0)),
            theme::body(DETAIL),
            p.ink,
        );
        if let (Some(i), Some(o)) = (r.indoor_temp, r.temp) {
            let d = i - o;
            let rel = if d.abs() < 0.5 {
                "about the same inside".to_string()
            } else if d > 0.0 {
                format!("{}{} warmer inside", num(Some(d), 1), u.temp)
            } else {
                format!("{}{} cooler inside", num(Some(-d), 1), u.temp)
            };
            ui::text(
                ui,
                format!("Outside {}{}, {rel}", num(Some(o), 1), u.temp),
                theme::body(DETAIL),
                p.ink2,
            );
        }
        bottom(ui, |ui| {
            // Inside against outside over the last 24 hours, on one scale.
            let (xs, inside) = series(snap, "intemp");
            if xs.len() > 1 {
                let outside = snap
                    .samples
                    .get("temp")
                    .map(|d| align(&xs, d))
                    .unwrap_or_default();
                Chart::sparkline("spark-indoor", SPARK_H, xs)
                    .series(
                        Series::line("Outside", outside, alpha(p.trace, 0.45))
                            .width(1.25)
                            .span_gaps()
                            .max_gap(GAP_SECS)
                            .no_points(),
                    )
                    .series(
                        Series::line("Inside", inside, p.ink2)
                            .span_gaps()
                            .max_gap(GAP_SECS)
                            .no_points(),
                    )
                    .show(ui);
            } else {
                ui.allocate_exact_size(vec2(ui.available_width(), SPARK_H), Sense::hover());
            }
        });
        if snap.samples.get("intemp").is_some_and(|v| v.len() > 1) {
            ui::small(ui, "Inside, against outside (violet)", p.ink2);
        }
    }
}

/// " since 16 Oct, 16:10" from MX's storm start, which is the station's own
/// date and time text ("16/10/2026 4:10 PM"); the time part follows the
/// chosen clock.
fn storm_since(start: &str) -> String {
    let mut parts = start.trim().splitn(2, ' ');
    let date = parts.next().unwrap_or_default();
    let time = parts.next().map(format::clock);
    let date = isobar_core::dayfile::parse_date(date)
        .map(format::short_date)
        .unwrap_or_else(|| date.to_string());
    match time {
        Some(t) => format!(" since {date}, {t}"),
        None => format!(" since {date}"),
    }
}

/// Put `add` at the bottom of an instrument column, so the sparklines line up.
fn bottom(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    // Every card's sparkline sits just below the tallest card's text. The
    // tallest is only known once all are drawn, so use last frame's.
    let id = egui::Id::new("now-card-text-height");
    let pass = ui.ctx().cumulative_pass_nr();
    let (seen, prev, this): (u64, f32, f32) =
        ui.data(|d| d.get_temp(id)).unwrap_or((pass, 0.0, 0.0));
    let (prev, this) = if seen == pass {
        (prev, this)
    } else {
        (this, 0.0)
    };
    let used = ui.cursor().top() - ui.min_rect().top();
    ui.data_mut(|d| d.insert_temp(id, (pass, prev, this.max(used))));
    ui.add_space((prev.max(used) - used).max(0.0) + 6.0);
    add(ui);
}

/// Rain since local midnight, built up from the station's "rain today"
/// samples so it resets at midnight whatever the station's rain day is.
fn rain_today(snap: &Snapshot) -> Vec<(i64, f64)> {
    let pts = snap
        .samples
        .get("rain_today")
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    isobar_core::analytics::rain_since_midnight(pts, |t| chart::ts_day(t as f64 / 1000.0))
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

/// The chips for choosing a span, right to left.
fn span_chips(
    ui: &mut Ui,
    spans: &mut std::collections::HashMap<Option<NowCard>, i64>,
    focus: Option<NowCard>,
) {
    let (options, default) = spans_for(focus);
    let cur = spans
        .get(&focus)
        .copied()
        .filter(|h| options.contains(h))
        .unwrap_or(default);
    ui.spacing_mut().item_spacing.x = 6.0;
    for h in options.iter().rev() {
        if ui::chip(ui, &span_label(*h), cur == *h).clicked() {
            spans.insert(focus, *h);
        }
    }
}

/// Graph paper behind the big chart: a fine 12 px grid.
fn graph_paper(ui: &mut Ui, rect: egui::Rect, paper: egui::layers::ShapeIdx, p: &Palette) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_cards_both_ways() {
        let order: Vec<(NowCard, bool)> = NowCard::ALL.iter().map(|c| (*c, true)).collect();
        let on = move_card(&order, NowCard::Wind, NowCard::Rain);
        assert_eq!(
            on.iter().take(3).map(|c| c.0).collect::<Vec<_>>(),
            [NowCard::Pressure, NowCard::Rain, NowCard::Wind]
        );
        let back = move_card(&order, NowCard::Sun, NowCard::Pressure);
        assert_eq!(
            back.iter().take(3).map(|c| c.0).collect::<Vec<_>>(),
            [NowCard::Wind, NowCard::Sun, NowCard::Pressure]
        );
        assert_eq!(back.len(), order.len());
    }
}
