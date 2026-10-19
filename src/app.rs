//! The window: the rail on the left (name, sections, connection status) and
//! the current screen.

use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{
    self, pos2, vec2, Align2, CornerRadius, CursorIcon, Frame, Margin, Rect, Sense, Stroke,
    ThemePreference,
};
use isobar_core::{DayRecord, LiveReading, Units};

use crate::format;
use crate::settings::{Paths, Settings, ThemeChoice};
use crate::theme::{self, Palette};
use crate::views;
use crate::worker::{Handle, Samples, Status};

/// The rail's width, and the strip left when it slides away.
const RAIL_W: f32 = 188.0;
const STRIP_W: f32 = 44.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Now,
    History,
    Compare,
    Insights,
    Seasons,
    Tanks,
    Health,
    Settings,
    /// Not a screen: switch to the demo station.
    Demo,
}

impl Route {
    const ALL: [Route; 8] = [
        Route::Now,
        Route::History,
        Route::Compare,
        Route::Insights,
        Route::Seasons,
        Route::Tanks,
        Route::Health,
        Route::Settings,
    ];

    fn label(self) -> &'static str {
        match self {
            Route::Now => "Now",
            Route::History => "History",
            Route::Compare => "Compare",
            Route::Insights => "Insights",
            Route::Seasons => "Seasons",
            Route::Tanks => "Tanks",
            Route::Health => "Health",
            Route::Settings => "Settings",
            Route::Demo => "Demo",
        }
    }
}

/// What every screen reads in a frame.
pub struct Snapshot {
    pub live: Option<LiveReading>,
    pub status: Status,
    pub days: Arc<Vec<DayRecord>>,
    pub days_version: u64,
    pub samples: Arc<Samples>,
    pub tank_log: Arc<Vec<isobar_core::tanks::LogDay>>,
    pub tank_version: u64,
    pub log_summary: Arc<crate::worker::LogSummary>,
    pub detail: Option<Arc<crate::worker::Detail>>,
    pub now_ms: i64,
    pub demo: bool,
}

impl Snapshot {
    /// Units come from the station; before the first reading, metric.
    pub fn units(&self) -> Units {
        self.live
            .as_ref()
            .map(|l| l.units.clone())
            .unwrap_or_default()
    }
}

pub struct IsobarApp {
    worker: Handle,
    pub settings: Settings,
    paths: Option<Paths>,
    route: Route,
    /// The slide-out rail is showing (only when it isn't pinned).
    rail_open: bool,
    /// Where the slide-out rail was last drawn.
    rail_rect: Rect,
    /// A section was just picked from the slide-out rail; wait for the pointer
    /// to leave the edge before opening it again.
    rail_hold: bool,
    now: views::now::Now,
    history: views::history::History,
    compare: views::compare::Compare,
    insights: views::insights::Insights,
    seasons: views::seasons::Seasons,
    tanks: views::tanks::TanksView,
    health: views::health::Health,
    settings_view: views::settings::SettingsView,
}

impl IsobarApp {
    pub fn new(
        cc: &eframe::CreationContext,
        settings: Settings,
        paths: Option<Paths>,
        worker: Handle,
    ) -> Self {
        theme::install(&cc.egui_ctx);
        apply_theme(&cc.egui_ctx, settings.theme);
        format::set_24h(settings.clock_24h);
        let first_run = paths.is_some()
            && !settings.has_station()
            && settings.dayfile().is_none()
            && worker.lock().days.is_empty();
        IsobarApp {
            settings_view: views::settings::SettingsView::new(&settings),
            worker,
            settings,
            paths,
            route: if first_run {
                Route::Settings
            } else {
                Route::Now
            },
            rail_open: false,
            rail_rect: Rect::NOTHING,
            rail_hold: false,
            now: Default::default(),
            history: Default::default(),
            compare: Default::default(),
            insights: Default::default(),
            seasons: Default::default(),
            tanks: Default::default(),
            health: Default::default(),
        }
    }

    fn snapshot(&self) -> Snapshot {
        let s = self.worker.lock();
        Snapshot {
            live: s.live.clone(),
            status: s.status.clone(),
            days: s.days.clone(),
            days_version: s.days_version,
            samples: s.samples.clone(),
            tank_log: s.tank_log.clone(),
            tank_version: s.tank_version,
            log_summary: s.log_summary.clone(),
            detail: s.detail.clone(),
            now_ms: chrono::Utc::now().timestamp_millis(),
            demo: self.paths.is_none(),
        }
    }

    /// The full rail: name, sections and connection status. Returns true when
    /// a section was picked. `height` is the window's height.
    fn rail(&mut self, ui: &mut egui::Ui, snap: &Snapshot, height: f32) -> bool {
        let p = Palette::of(ui.ctx());
        let top = ui.max_rect().top();
        let mut picked = false;
        ui.add_space(26.0);
        // The mark: a recording drum seen end on, with the violet pen ring.
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
            drum_mark(ui.painter(), r.center(), &p);
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Isobar")
                    .font(theme::display(26.0))
                    .color(p.ink),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let pinned = self.settings.rail_pinned;
                let (r, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                let hovered = resp.hovered();
                if hovered {
                    ui.painter()
                        .rect_filled(r, CornerRadius::same(theme::RADIUS), p.faint);
                }
                let c = if hovered { p.ink } else { p.ink2 };
                if pinned {
                    chevrons_left(ui.painter(), r.center(), c);
                } else {
                    pin_icon(ui.painter(), r.center(), c);
                }
                let tip = if pinned {
                    "Hide the menu. Point at the left edge to bring it back."
                } else {
                    "Keep the menu open"
                };
                if resp
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_text(tip)
                    .clicked()
                {
                    self.set_rail_pinned(!pinned);
                }
            });
        });
        ui.add_space(22.0);
        for route in Route::ALL {
            let on = self.route == route;
            let (rect, resp) =
                ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
            let hovered = resp.hovered();
            if on || hovered {
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(theme::RADIUS),
                    if on { p.sheet } else { p.faint },
                );
            }
            if on {
                ui.painter().rect_filled(
                    Rect::from_min_size(
                        pos2(rect.left() + 4.0, rect.top() + 10.0),
                        vec2(3.0, rect.height() - 20.0),
                    ),
                    2,
                    p.trace,
                );
            }
            let label = ui.painter().text(
                pos2(rect.left() + 14.0, rect.center().y),
                Align2::LEFT_CENTER,
                route.label(),
                theme::medium(15.0),
                if on || hovered { p.ink } else { p.ink2 },
            );
            if route == Route::Health && views::health::issues(snap, &self.settings) > 0 {
                ui.painter().circle_filled(
                    pos2(label.right() + 8.0, label.center().y),
                    3.5,
                    p.warn,
                );
            }
            if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                self.route = route;
                picked = true;
            }
            ui.add_space(2.0);
        }

        // Connection status, pinned to the bottom.
        let (dot, label) = self.status_line(snap, &p);
        let bottom = top + height - 24.0;
        let at = pos2(ui.max_rect().left() + 10.0, bottom);
        let resp = ui.interact(
            Rect::from_min_size(pos2(at.x - 4.0, bottom - 10.0), vec2(160.0, 20.0)),
            ui.id().with("rail-status"),
            Sense::hover(),
        );
        ui.painter().circle_filled(at, 4.0, dot);
        ui.painter().text(
            at + vec2(12.0, 0.0),
            Align2::LEFT_CENTER,
            label,
            theme::body(13.0),
            p.ink2,
        );
        if let Some(e) = &snap.status.last_error {
            resp.on_hover_text(e);
        }
        picked
    }

    /// The colour and words for the connection status.
    fn status_line(&self, snap: &Snapshot, p: &Palette) -> (egui::Color32, String) {
        let s = &snap.status;
        let has_station = snap.demo || self.settings.has_station();
        if !has_station {
            let label = if self.settings.dayfile().is_some() {
                "Reading a dayfile"
            } else {
                "No station"
            };
            (p.ink2, label.to_string())
        } else if s.syncing {
            let label = match s.sync_progress {
                Some((d, t)) if t > 0 => format!("Syncing {d} of {t} days"),
                _ => "Syncing history".into(),
            };
            (p.warn, label)
        } else if s.connected {
            (
                p.ok,
                format!("Live, {}", format::ago(s.last_ok, snap.now_ms)),
            )
        } else {
            (p.heat, "Can't reach station".into())
        }
    }

    /// The narrow strip left when the rail isn't pinned: the mark, a menu
    /// button and the status dot. Returns true if the menu button was pressed.
    fn strip(&self, ui: &mut egui::Ui, snap: &Snapshot, height: f32) -> bool {
        let p = Palette::of(ui.ctx());
        let r = ui.max_rect();
        let cx = r.center().x;
        drum_mark(ui.painter(), pos2(cx, r.top() + 26.0 + 12.0), &p);
        let menu = Rect::from_center_size(pos2(cx, r.top() + 88.0), vec2(30.0, 30.0));
        let resp = ui.interact(menu, ui.id().with("rail-menu"), Sense::click());
        if resp.hovered() {
            ui.painter()
                .rect_filled(menu, CornerRadius::same(theme::RADIUS), p.faint);
        }
        for k in -1..=1 {
            let y = menu.center().y + k as f32 * 5.5;
            ui.painter().line_segment(
                [pos2(cx - 7.0, y), pos2(cx + 7.0, y)],
                Stroke::new(1.75, p.ink2),
            );
        }
        let (dot, label) = self.status_line(snap, &p);
        let at = pos2(cx, r.top() + height - 24.0);
        ui.painter().circle_filled(at, 4.0, dot);
        let status = ui.interact(
            Rect::from_center_size(at, vec2(24.0, 24.0)),
            ui.id().with("strip-status"),
            Sense::hover(),
        );
        status.on_hover_text(label);
        resp.on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text("Menu")
            .clicked()
    }

    fn set_rail_pinned(&mut self, pinned: bool) {
        self.settings.rail_pinned = pinned;
        self.rail_open = false;
        self.rail_hold = !pinned;
        if let Some(paths) = &self.paths {
            let _ = self.settings.save(&paths.settings);
        }
    }
}

/// Isobar's mark: a recording drum seen end on, with the violet pen ring.
fn drum_mark(painter: &egui::Painter, c: egui::Pos2, p: &Palette) {
    painter.circle_stroke(c, 10.0, Stroke::new(2.0, p.ink));
    painter.circle_stroke(c, 5.5, Stroke::new(2.0, p.trace));
}

fn chevrons_left(painter: &egui::Painter, c: egui::Pos2, color: egui::Color32) {
    let stroke = Stroke::new(1.75, color);
    for dx in [-3.0, 3.0] {
        let x = c.x + dx;
        painter.line_segment([pos2(x + 2.5, c.y - 5.0), pos2(x - 2.5, c.y)], stroke);
        painter.line_segment([pos2(x - 2.5, c.y), pos2(x + 2.5, c.y + 5.0)], stroke);
    }
}

fn pin_icon(painter: &egui::Painter, c: egui::Pos2, color: egui::Color32) {
    let stroke = Stroke::new(1.75, color);
    painter.circle_stroke(c + vec2(0.0, -3.5), 3.5, stroke);
    painter.line_segment([c + vec2(-5.5, 1.5), c + vec2(5.5, 1.5)], stroke);
    painter.line_segment([c + vec2(0.0, 1.5), c + vec2(0.0, 8.0)], stroke);
}

pub fn apply_theme(ctx: &egui::Context, t: ThemeChoice) {
    ctx.set_theme(match t {
        ThemeChoice::System => ThemePreference::System,
        ThemeChoice::Light => ThemePreference::Light,
        ThemeChoice::Dark => ThemePreference::Dark,
    });
}

impl eframe::App for IsobarApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let p = Palette::of(&ctx);
        let snap = self.snapshot();
        // Keep "updated 5 s ago" ticking.
        ctx.request_repaint_after(Duration::from_secs(1));

        let screen = ctx.content_rect();
        let edge_x = if self.settings.rail_pinned {
            egui::Panel::left("rail")
                .exact_size(RAIL_W)
                .resizable(false)
                .show_separator_line(false)
                .frame(
                    Frame::new()
                        .fill(p.paper)
                        .inner_margin(Margin::symmetric(12, 0)),
                )
                .show(ui, |ui| self.rail(ui, &snap, screen.height()));
            RAIL_W
        } else {
            let strip = egui::Panel::left("rail-strip")
                .exact_size(STRIP_W)
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::new().fill(p.paper))
                .show(ui, |ui| self.strip(ui, &snap, screen.height()));
            let pointer = ctx.pointer_hover_pos();
            let over_strip = pointer.is_some_and(|q| strip.response.rect.contains(q));
            let over_rail = pointer.is_some_and(|q| self.rail_rect.contains(q));
            if strip.inner {
                self.rail_open = !self.rail_open;
                self.rail_hold = !self.rail_open;
            } else if over_strip && !self.rail_hold {
                self.rail_open = true;
            } else if !over_strip && !over_rail && !ctx.input(|i| i.pointer.any_down()) {
                self.rail_open = false;
            }
            if !over_strip {
                self.rail_hold = false;
            }
            let t = ctx.animate_bool_with_time(egui::Id::new("rail-slide"), self.rail_open, 0.16);
            if t > 0.0 {
                let shown = egui::Area::new(egui::Id::new("rail-overlay"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(pos2(screen.left() - RAIL_W * (1.0 - t), screen.top()))
                    .show(&ctx, |ui| {
                        Frame::new()
                            .fill(p.paper)
                            .stroke(Stroke::new(1.0, p.grid))
                            .shadow(egui::Shadow {
                                offset: [4, 0],
                                blur: 18,
                                spread: 0,
                                color: theme::alpha(
                                    egui::Color32::BLACK,
                                    if p.dark { 0.45 } else { 0.12 },
                                ),
                            })
                            .inner_margin(Margin::symmetric(12, 0))
                            .show(ui, |ui| {
                                ui.set_width(RAIL_W - 24.0);
                                ui.set_height(screen.height());
                                self.rail(ui, &snap, screen.height())
                            })
                            .inner
                    });
                self.rail_rect = shown.response.rect;
                if shown.inner {
                    self.rail_open = false;
                    self.rail_hold = true;
                }
            } else {
                self.rail_rect = Rect::NOTHING;
            }
            STRIP_W
        };
        // The rail's edge.
        ui.painter().line_segment(
            [pos2(edge_x - 0.5, 0.0), pos2(edge_x - 0.5, screen.bottom())],
            Stroke::new(1.0, p.grid),
        );

        let mut go: Option<Route> = None;
        if self.route == Route::Settings {
            if let Some(saved) = self.settings_view.save_bar(ui, &self.settings) {
                self.save_settings(&ctx, saved);
            }
        }
        egui::CentralPanel::no_frame()
            .frame(Frame::new().fill(p.paper))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(self.route.label())
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let avail = ui.available_width();
                        // Now spreads wider so its cards fit one row on a 1080p screen.
                        let max = if self.route == Route::Now {
                            1640.0
                        } else {
                            1320.0
                        };
                        let width = (avail - 64.0).clamp(300.0, max);
                        let side = ((avail - width) / 2.0).max(0.0);
                        ui.add_space(24.0);
                        ui.horizontal_top(|ui| {
                            ui.add_space(side);
                            ui.vertical(|ui| {
                                ui.set_width(width);
                                ui.spacing_mut().item_spacing.y = 24.0;
                                match self.route {
                                    Route::Now => self.now.ui(ui, &snap, &self.settings, &mut go),
                                    Route::History => self.history.ui(ui, &snap, &mut go),
                                    Route::Compare => self.compare.ui(ui, &snap, &mut go),
                                    Route::Insights => self.insights.ui(ui, &snap, &mut go),
                                    Route::Seasons => {
                                        self.seasons.ui(ui, &snap, &self.settings, &mut go)
                                    }
                                    Route::Tanks => {
                                        self.tanks.ui(ui, &snap, &self.settings, &mut go)
                                    }
                                    Route::Health => {
                                        self.health.ui(ui, &snap, &self.settings, &self.worker)
                                    }
                                    Route::Settings => {
                                        if self.settings_view.ui(
                                            ui,
                                            &snap,
                                            &self.settings,
                                            &self.worker,
                                        ) {
                                            go = Some(Route::Demo);
                                        }
                                    }
                                    Route::Demo => {}
                                }
                            });
                        });
                        ui.add_space(48.0);
                    });
            });
        if let Some((key, metrics, from_ms, to_ms)) = self.now.request.take() {
            self.worker.send(crate::worker::Cmd::Detail {
                key,
                metrics,
                from_ms,
                to_ms,
            });
        }
        if let Some(order) = self.now.reordered.take() {
            self.settings.set_cards(&order);
            if let Some(paths) = &self.paths {
                let _ = self.settings.save(&paths.settings);
            }
        }
        match go {
            Some(Route::Demo) => self.start_demo(&ctx),
            Some(r) => self.route = r,
            None => {}
        }
    }
}

impl IsobarApp {
    /// Swap to the demo station, for a look round before setting one up.
    /// Nothing about the real station is saved or changed.
    fn start_demo(&mut self, ctx: &egui::Context) {
        self.worker.send(crate::worker::Cmd::Stop);
        let settings = Settings {
            station_name: "Demo station".into(),
            theme: self.settings.theme,
            clock_24h: self.settings.clock_24h,
            rail_pinned: self.settings.rail_pinned,
            latitude: Some(isobar_core::demo::LATITUDE),
            tanks: Some(isobar_core::demo::tanks()),
            ..Default::default()
        };
        self.worker = crate::worker::spawn(ctx.clone(), settings.clone(), None);
        self.settings_view = views::settings::SettingsView::new(&settings);
        self.settings = settings;
        self.paths = None;
        self.now = Default::default();
        self.history = Default::default();
        self.compare = Default::default();
        self.insights = Default::default();
        self.seasons = Default::default();
        self.tanks = Default::default();
        self.health = Default::default();
        self.route = Route::Now;
    }

    fn save_settings(&mut self, ctx: &egui::Context, next: Settings) {
        let result = match &self.paths {
            Some(paths) => next.save(&paths.settings).map_err(|e| e.to_string()),
            None => Ok(()),
        };
        apply_theme(ctx, next.theme);
        format::set_24h(next.clock_24h);
        self.settings = next.clone();
        self.worker
            .send(crate::worker::Cmd::Settings(Box::new(next)));
        self.settings_view.saved(result);
    }
}
