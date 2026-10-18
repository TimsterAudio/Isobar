//! Charts drawn on chart paper: lines, dashed lines, bars, bands between two
//! series and fills, with day-first time axes, a hover read-out and, where
//! asked for, drag-to-zoom (double-click to reset).
//!
//! Data is aligned like a table: one `xs` column (Unix seconds for time
//! charts, an index otherwise) and one `ys` column per series.

use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, Id, LayerId, Mesh, Order, Pos2, Rect, Sense,
    Shape, Stroke, StrokeKind, Ui,
};

use crate::format::{self, MONTHS};
use crate::theme::{self, alpha, Palette};

#[derive(Clone, Copy, PartialEq)]
pub enum Style {
    Line {
        width: f32,
    },
    Dashed {
        width: f32,
    },
    /// Bars up from zero (or the bottom of the chart).
    Bars,
}

pub struct Series {
    pub label: String,
    pub ys: Vec<Option<f64>>,
    pub color: Color32,
    pub style: Style,
    /// On the right-hand scale.
    pub right: bool,
    /// Fill between the line and the bottom of the chart.
    pub fill: Option<Color32>,
    /// Join across missing values instead of breaking the line.
    pub span_gaps: bool,
    /// Break the line where neighbouring values are further apart than this
    /// (in x units), so a stretch with no data shows as a gap.
    pub max_gap: Option<f64>,
    /// Draw markers on each value when there's room.
    pub points: bool,
    /// Shown in the hover read-out.
    pub in_readout: bool,
    /// Drawn at all; an undrawn series can still be the edge of a band.
    pub drawn: bool,
    pub digits: usize,
    pub unit: String,
}

impl Series {
    pub fn line(label: impl Into<String>, ys: Vec<Option<f64>>, color: Color32) -> Series {
        Series {
            label: label.into(),
            ys,
            color,
            style: Style::Line { width: 1.75 },
            right: false,
            fill: None,
            span_gaps: false,
            max_gap: None,
            points: true,
            in_readout: true,
            drawn: true,
            digits: 1,
            unit: String::new(),
        }
    }

    pub fn width(mut self, w: f32) -> Self {
        self.style = match self.style {
            Style::Dashed { .. } => Style::Dashed { width: w },
            _ => Style::Line { width: w },
        };
        self
    }

    pub fn dashed(mut self) -> Self {
        let w = match self.style {
            Style::Line { width } | Style::Dashed { width } => width,
            Style::Bars => 1.0,
        };
        self.style = Style::Dashed { width: w };
        self
    }

    pub fn bars(mut self) -> Self {
        self.style = Style::Bars;
        self
    }

    pub fn right(mut self) -> Self {
        self.right = true;
        self
    }

    pub fn fill(mut self, c: Color32) -> Self {
        self.fill = Some(c);
        self
    }

    pub fn max_gap(mut self, gap: f64) -> Self {
        self.max_gap = Some(gap);
        self
    }

    pub fn span_gaps(mut self) -> Self {
        self.span_gaps = true;
        self
    }

    pub fn no_points(mut self) -> Self {
        self.points = false;
        self
    }

    pub fn hidden_in_readout(mut self) -> Self {
        self.in_readout = false;
        self
    }

    /// Not drawn and not in the read-out: only the edge of a band.
    pub fn edge(label: impl Into<String>, ys: Vec<Option<f64>>) -> Series {
        let mut s = Series::line(label, ys, Color32::TRANSPARENT);
        s.drawn = false;
        s.in_readout = false;
        s
    }

    pub fn unit(mut self, unit: impl Into<String>, digits: usize) -> Self {
        self.unit = unit.into();
        self.digits = digits;
        self
    }
}

pub enum XAxis {
    /// `xs` are Unix seconds, shown in local time.
    Time,
    /// `xs` are indexes. `splits` fixes where ticks go; `label` names a tick.
    Index {
        splits: Option<Vec<f64>>,
        label: Box<dyn Fn(f64) -> String>,
    },
}

pub struct Chart {
    id: Id,
    height: f32,
    xs: Vec<f64>,
    x_axis: XAxis,
    series: Vec<Series>,
    bands: Vec<(usize, usize, Color32)>,
    markers: Vec<(f64, String)>,
    axes: bool,
    zoom: bool,
    hover: bool,
    left_zero: bool,
    right_zero: bool,
    left_fmt: Box<dyn Fn(f64, f64) -> String>,
    right_fmt: Box<dyn Fn(f64, f64) -> String>,
    title: Box<dyn Fn(f64) -> String>,
}

/// Axis labels with as many decimals as the tick step needs.
pub fn plain_tick(v: f64, step: f64) -> String {
    let digits = if step >= 1.0 {
        0
    } else if step >= 0.1 {
        1
    } else {
        2
    };
    format::num(Some(v), digits)
}

impl Chart {
    pub fn new(
        id_salt: impl std::hash::Hash + std::fmt::Debug,
        height: f32,
        xs: Vec<f64>,
        x_axis: XAxis,
    ) -> Chart {
        Chart {
            id: Id::new("chart").with(id_salt),
            height,
            xs,
            x_axis,
            series: Vec::new(),
            bands: Vec::new(),
            markers: Vec::new(),
            axes: true,
            zoom: false,
            hover: true,
            left_zero: false,
            right_zero: false,
            left_fmt: Box::new(plain_tick),
            right_fmt: Box::new(plain_tick),
            title: Box::new(|x| format!("{x}")),
        }
    }

    /// A small chart with no axes or read-out.
    pub fn sparkline(
        id_salt: impl std::hash::Hash + std::fmt::Debug,
        height: f32,
        xs: Vec<f64>,
    ) -> Chart {
        let mut c = Chart::new(id_salt, height, xs, XAxis::Time);
        c.axes = false;
        c.hover = false;
        c
    }

    pub fn series(mut self, s: Series) -> Self {
        self.series.push(s);
        self
    }

    /// Shade between series `a` and `b`.
    pub fn band(mut self, a: usize, b: usize, c: Color32) -> Self {
        self.bands.push((a, b, c));
        self
    }

    /// A labelled upright line at `x`, such as today.
    pub fn marker(mut self, x: f64, label: impl Into<String>) -> Self {
        self.markers.push((x, label.into()));
        self
    }

    pub fn zoomable(mut self) -> Self {
        self.zoom = true;
        self
    }

    /// Start the left scale at zero.
    pub fn left_from_zero(mut self) -> Self {
        self.left_zero = true;
        self
    }

    pub fn right_from_zero(mut self) -> Self {
        self.right_zero = true;
        self
    }

    pub fn left_ticks(mut self, f: impl Fn(f64, f64) -> String + 'static) -> Self {
        self.left_fmt = Box::new(f);
        self
    }

    /// The heading of the hover read-out for an x value.
    pub fn title(mut self, f: impl Fn(f64) -> String + 'static) -> Self {
        self.title = Box::new(f);
        self
    }

    pub fn show(self, ui: &mut Ui) -> egui::Response {
        let p = Palette::of(ui.ctx());
        let width = ui.available_width().max(120.0);
        let sense = if self.zoom {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(vec2(width, self.height), sense);
        if !ui.is_rect_visible(rect) || self.xs.is_empty() {
            return response;
        }
        let has_right = self.series.iter().any(|s| s.right);
        let plot = if self.axes {
            Rect::from_min_max(
                rect.min + vec2(48.0, 8.0),
                rect.max - vec2(if has_right { 50.0 } else { 10.0 }, 26.0),
            )
        } else {
            rect.shrink2(vec2(2.0, 3.0))
        };

        // ----- x range, with room for bars at the ends and any zoom.
        let full = (self.xs[0], *self.xs.last().unwrap());
        let has_bars = self.series.iter().any(|s| s.style == Style::Bars);
        let step = if self.xs.len() > 1 {
            (full.1 - full.0) / (self.xs.len() - 1) as f64
        } else {
            1.0
        };
        let pad = if has_bars { step * 0.6 } else { 0.0 };
        let mut xr = (full.0 - pad, full.1 + pad);
        if xr.1 - xr.0 <= 0.0 {
            xr = (xr.0 - 1.0, xr.1 + 1.0);
        }
        let zoom_key = self.id.with("zoom");
        let zoom: Option<(f64, f64, f64, f64)> = ui.data(|d| d.get_temp(zoom_key));
        if let Some((a, b, f0, f1)) = zoom {
            if f0 == full.0 && f1 == full.1 {
                xr = (a, b);
            }
        }
        let fx = |x: f64| plot.left() + ((x - xr.0) / (xr.1 - xr.0)) as f32 * plot.width();
        let inv_x = |px: f32| xr.0 + ((px - plot.left()) / plot.width()) as f64 * (xr.1 - xr.0);
        let vis: Vec<usize> = (0..self.xs.len())
            .filter(|&i| self.xs[i] >= xr.0 && self.xs[i] <= xr.1)
            .collect();

        // ----- y ranges
        let range_of = |right: bool, zero: bool| -> (f64, f64, f64) {
            let mut lo = f64::INFINITY;
            let mut hi = f64::NEG_INFINITY;
            for s in self.series.iter().filter(|s| s.right == right) {
                for &i in &vis {
                    if let Some(v) = s.ys.get(i).copied().flatten() {
                        lo = lo.min(v);
                        hi = hi.max(v);
                    }
                }
            }
            if !lo.is_finite() {
                (lo, hi) = (0.0, 1.0);
            }
            if zero
                || self
                    .series
                    .iter()
                    .any(|s| s.right == right && s.style == Style::Bars)
            {
                lo = lo.min(0.0);
            }
            if hi - lo < 1e-9 {
                hi += 1.0;
                if !zero {
                    lo -= 1.0;
                }
            }
            let target = (plot.height() / 46.0).clamp(2.0, 8.0) as f64;
            let step = nice_step((hi - lo) / target);
            if self.axes {
                let lo2 = if zero && lo >= 0.0 {
                    0.0
                } else {
                    (lo / step).floor() * step
                };
                let hi2 = (hi / step).ceil() * step;
                (lo2, hi2.max(lo2 + step), step)
            } else {
                let m = (hi - lo) * 0.08;
                (if zero { lo } else { lo - m }, hi + m, step)
            }
        };
        let (l0, l1, lstep) = range_of(false, self.left_zero);
        let (r0, r1, rstep) = range_of(true, self.right_zero);
        let fy = |v: f64, right: bool| {
            let (a, b) = if right { (r0, r1) } else { (l0, l1) };
            plot.bottom() - ((v - a) / (b - a)) as f32 * plot.height()
        };

        let painter = ui.painter_at(rect);
        let grid = Stroke::new(1.0, p.grid);
        let label_font = theme::medium(12.0);

        // ----- paper, grid and axes
        if self.axes {
            let mut v = l0;
            while v <= l1 + lstep * 0.01 {
                let y = fy(v, false).round() + 0.5;
                painter.line_segment([pos2(plot.left(), y), pos2(plot.right(), y)], grid);
                painter.text(
                    pos2(plot.left() - 8.0, y),
                    Align2::RIGHT_CENTER,
                    (self.left_fmt)(v, lstep),
                    label_font.clone(),
                    p.ink2,
                );
                v += lstep;
            }
            if has_right {
                let mut v = r0;
                while v <= r1 + rstep * 0.01 {
                    let y = fy(v, true);
                    painter
                        .line_segment([pos2(plot.right(), y), pos2(plot.right() + 4.0, y)], grid);
                    painter.text(
                        pos2(plot.right() + 8.0, y),
                        Align2::LEFT_CENTER,
                        (self.right_fmt)(v, rstep),
                        label_font.clone(),
                        p.ink2,
                    );
                    v += rstep;
                }
            }
            let ticks: Vec<(f64, String)> = match &self.x_axis {
                XAxis::Time => time_ticks(xr.0, xr.1, plot.width()),
                XAxis::Index { splits, label } => {
                    let xs: Vec<f64> = match splits {
                        Some(s) => s
                            .iter()
                            .copied()
                            .filter(|x| *x >= xr.0 && *x <= xr.1)
                            .collect(),
                        None => {
                            let target = (plot.width() / 64.0).max(2.0) as f64;
                            let st = nice_step((xr.1 - xr.0) / target).max(1.0);
                            let mut out = Vec::new();
                            let mut x = (xr.0 / st).ceil() * st;
                            while x <= xr.1 {
                                out.push(x);
                                x += st;
                            }
                            out
                        }
                    };
                    xs.into_iter().map(|x| (x, label(x))).collect()
                }
            };
            for (x, text) in ticks {
                let px = fx(x).round() + 0.5;
                painter.line_segment([pos2(px, plot.top()), pos2(px, plot.bottom())], grid);
                painter.line_segment(
                    [pos2(px, plot.bottom()), pos2(px, plot.bottom() + 4.0)],
                    grid,
                );
                painter.text(
                    pos2(px, plot.bottom() + 7.0),
                    Align2::CENTER_TOP,
                    text,
                    label_font.clone(),
                    p.ink2,
                );
            }
        }

        let clip = painter.with_clip_rect(plot.expand(3.0));
        let pt = |i: usize, v: f64, right: bool| pos2(fx(self.xs[i]), fy(v, right));

        // ----- bands
        for &(a, b, color) in &self.bands {
            let (Some(sa), Some(sb)) = (self.series.get(a), self.series.get(b)) else {
                continue;
            };
            let mut mesh = Mesh::default();
            let mut prev: Option<(Pos2, Pos2)> = None;
            for &i in &vis {
                match (
                    sa.ys.get(i).copied().flatten(),
                    sb.ys.get(i).copied().flatten(),
                ) {
                    (Some(va), Some(vb)) => {
                        let cur = (pt(i, va, sa.right), pt(i, vb, sb.right));
                        if let Some(pr) = prev {
                            quad(&mut mesh, pr.0, cur.0, cur.1, pr.1, color);
                        }
                        prev = Some(cur);
                    }
                    _ => prev = None,
                }
            }
            clip.add(Shape::mesh(mesh));
        }

        // ----- markers
        for (x, label) in &self.markers {
            if *x < xr.0 || *x > xr.1 {
                continue;
            }
            let px = fx(*x).round() + 0.5;
            clip.extend(dashed(
                &[pos2(px, plot.top()), pos2(px, plot.bottom())],
                Stroke::new(1.0, p.ink2),
                3.0,
                3.0,
            ));
            painter.text(
                pos2(px + 5.0, plot.top() + 2.0),
                Align2::LEFT_TOP,
                label,
                theme::medium(12.0),
                p.ink2,
            );
        }

        // ----- series
        let px_per_point = plot.width() / vis.len().max(1) as f32;
        for s in self.series.iter().filter(|s| s.drawn) {
            match s.style {
                Style::Bars => {
                    let w = (px_per_point * 0.75).clamp(1.0, 24.0);
                    let base = fy(0f64.max(if s.right { r0 } else { l0 }), s.right);
                    for &i in &vis {
                        if let Some(v) = s.ys.get(i).copied().flatten() {
                            if v <= 0.0 {
                                continue;
                            }
                            let x = fx(self.xs[i]);
                            let top = fy(v, s.right).min(base - 1.0);
                            clip.rect_filled(
                                Rect::from_min_max(pos2(x - w / 2.0, top), pos2(x + w / 2.0, base)),
                                CornerRadius::same(1),
                                alpha(s.color, 0.6),
                            );
                        }
                    }
                }
                Style::Line { width } | Style::Dashed { width } => {
                    let mut runs: Vec<Vec<Pos2>> = vec![Vec::new()];
                    let mut last_x: Option<f64> = None;
                    for &i in &vis {
                        match s.ys.get(i).copied().flatten() {
                            Some(v) => {
                                let x = self.xs[i];
                                if s.max_gap.zip(last_x).is_some_and(|(g, l)| x - l > g) {
                                    runs.push(Vec::new());
                                }
                                last_x = Some(x);
                                runs.last_mut().unwrap().push(pt(i, v, s.right));
                            }
                            None if !s.span_gaps => runs.push(Vec::new()),
                            None => {}
                        }
                    }
                    if let Some(fill) = s.fill {
                        let mut mesh = Mesh::default();
                        for run in &runs {
                            for w in run.windows(2) {
                                quad(
                                    &mut mesh,
                                    w[0],
                                    w[1],
                                    pos2(w[1].x, plot.bottom()),
                                    pos2(w[0].x, plot.bottom()),
                                    fill,
                                );
                            }
                        }
                        clip.add(Shape::mesh(mesh));
                    }
                    let stroke = Stroke::new(width, s.color);
                    for run in runs.into_iter().filter(|r| !r.is_empty()) {
                        if run.len() == 1 {
                            clip.circle_filled(run[0], width, s.color);
                        } else if matches!(s.style, Style::Dashed { .. }) {
                            clip.extend(dashed(&run, stroke, 5.0, 4.0));
                        } else {
                            clip.add(Shape::line(run, stroke));
                        }
                    }
                    if s.points && self.axes && px_per_point > 14.0 {
                        for &i in &vis {
                            if let Some(v) = s.ys.get(i).copied().flatten() {
                                let c = pt(i, v, s.right);
                                clip.circle_filled(c, 2.6, p.sheet);
                                clip.circle_stroke(c, 2.6, Stroke::new(1.2, s.color));
                            }
                        }
                    }
                }
            }
        }

        // ----- zoom by dragging
        if self.zoom {
            let drag_key = self.id.with("drag");
            if response.drag_started() {
                if let Some(pos) = response.interact_pointer_pos() {
                    ui.data_mut(|d| d.insert_temp(drag_key, pos.x));
                }
            }
            let start: Option<f32> = ui.data(|d| d.get_temp(drag_key));
            if let (Some(x0), Some(pos)) = (start, response.interact_pointer_pos()) {
                let (a, b) = (
                    x0.min(pos.x).max(plot.left()),
                    x0.max(pos.x).min(plot.right()),
                );
                if response.dragged() {
                    painter.rect_filled(
                        Rect::from_x_y_ranges(a..=b, plot.y_range()),
                        0,
                        alpha(p.trace, 0.12),
                    );
                }
                if response.drag_stopped() {
                    ui.data_mut(|d| d.remove::<f32>(drag_key));
                    if b - a > 6.0 {
                        let z = (inv_x(a), inv_x(b), full.0, full.1);
                        ui.data_mut(|d| d.insert_temp(zoom_key, z));
                    }
                }
            }
            if response.double_clicked() {
                ui.data_mut(|d| d.remove::<(f64, f64, f64, f64)>(zoom_key));
            }
            if zoom.is_some_and(|z| z.2 == full.0 && z.3 == full.1) {
                painter.text(
                    pos2(plot.right() - 4.0, plot.top() + 4.0),
                    Align2::RIGHT_TOP,
                    "Zoomed. Double-click to show all",
                    theme::body(12.0),
                    p.ink2,
                );
            }
        }

        // ----- hover read-out
        if self.hover {
            if let Some(pos) = response
                .hover_pos()
                .filter(|p| plot.expand(4.0).contains(*p))
            {
                let x = inv_x(pos.x);
                let i = nearest(&self.xs, x);
                if self.xs[i] >= xr.0 && self.xs[i] <= xr.1 {
                    self.readout(ui, &p, plot, i, pos, &fx, &fy);
                }
            }
        }
        response
    }

    #[allow(clippy::too_many_arguments)]
    fn readout(
        &self,
        ui: &Ui,
        p: &Palette,
        plot: Rect,
        i: usize,
        pointer: Pos2,
        fx: &dyn Fn(f64) -> f32,
        fy: &dyn Fn(f64, bool) -> f32,
    ) {
        let x = fx(self.xs[i]);
        let layer = ui
            .ctx()
            .layer_painter(LayerId::new(Order::Tooltip, self.id.with("readout")));
        layer.line_segment(
            [pos2(x, plot.top()), pos2(x, plot.bottom())],
            Stroke::new(1.0, alpha(p.ink2, 0.5)),
        );
        let mut rows: Vec<(Color32, String, String)> = Vec::new();
        for s in self.series.iter().filter(|s| s.drawn) {
            let Some(v) = s.ys.get(i).copied().flatten() else {
                continue;
            };
            if s.style != Style::Bars {
                let c = pos2(x, fy(v, s.right));
                layer.circle_filled(c, 3.5, s.color);
                layer.circle_stroke(c, 3.5, Stroke::new(1.5, p.sheet));
            }
            if s.in_readout {
                rows.push((
                    s.color,
                    s.label.clone(),
                    format!("{}{}", format::num(Some(v), s.digits), s.unit),
                ));
            }
        }
        let title = (self.title)(self.xs[i]);
        let font = theme::body(13.0);
        let bold = theme::semi(13.0);
        let label_w = rows
            .iter()
            .map(|r| {
                layer
                    .layout_no_wrap(r.1.clone(), font.clone(), p.ink)
                    .size()
                    .x
            })
            .fold(0.0, f32::max);
        let value_w = rows
            .iter()
            .map(|r| {
                layer
                    .layout_no_wrap(r.2.clone(), bold.clone(), p.ink)
                    .size()
                    .x
            })
            .fold(0.0, f32::max);
        let title_w = layer
            .layout_no_wrap(title.clone(), bold.clone(), p.ink)
            .size()
            .x;
        let w = (20.0 + label_w + 16.0 + value_w).max(title_w) + 24.0;
        let h = 12.0 + 18.0 + rows.len() as f32 * 19.0 + 8.0;
        let screen = ui.ctx().content_rect();
        let mut origin = pointer + vec2(16.0, 12.0);
        if origin.x + w > screen.right() - 8.0 {
            origin.x = pointer.x - w - 16.0;
        }
        if origin.y + h > screen.bottom() - 8.0 {
            origin.y = pointer.y - h - 12.0;
        }
        let r = Rect::from_min_size(origin, vec2(w, h));
        layer.add(egui::epaint::RectShape::filled(
            r.translate(vec2(0.0, 2.0)),
            6,
            alpha(Color32::BLACK, if p.dark { 0.35 } else { 0.08 }),
        ));
        layer.rect(r, 6, p.sheet, Stroke::new(1.0, p.grid), StrokeKind::Inside);
        layer.text(
            r.min + vec2(12.0, 10.0),
            Align2::LEFT_TOP,
            title,
            bold.clone(),
            p.ink,
        );
        for (k, (c, label, value)) in rows.into_iter().enumerate() {
            let y = r.top() + 12.0 + 18.0 + k as f32 * 19.0 + 8.0;
            layer.rect_filled(
                Rect::from_center_size(pos2(r.left() + 18.0, y), vec2(10.0, 3.0)),
                2,
                c,
            );
            layer.text(
                pos2(r.left() + 30.0, y),
                Align2::LEFT_CENTER,
                label,
                font.clone(),
                p.ink2,
            );
            layer.text(
                pos2(r.right() - 12.0, y),
                Align2::RIGHT_CENTER,
                value,
                bold.clone(),
                p.ink,
            );
        }
    }
}

/// A dashed polyline whose pattern runs on along the whole line, so it stays
/// even when the points are closer together than a dash (a reading a minute
/// across a day). Each dash follows the line's bends.
fn dashed(points: &[Pos2], stroke: Stroke, dash: f32, gap: f32) -> Vec<Shape> {
    let period = dash + gap;
    let mut out = Vec::new();
    let mut current: Vec<Pos2> = Vec::new();
    // Distance along the pattern at the start of the current segment.
    let mut phase = 0.0f32;
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = a.distance(b);
        if len <= f32::EPSILON {
            continue;
        }
        let mut t = 0.0f32;
        while t < len {
            let in_dash = phase < dash;
            let left = if in_dash {
                dash - phase
            } else {
                period - phase
            };
            let step = left.min(len - t);
            let p0 = a + (b - a) * (t / len);
            let p1 = a + (b - a) * ((t + step) / len);
            if in_dash {
                if current.is_empty() {
                    current.push(p0);
                }
                current.push(p1);
            }
            t += step;
            phase += step;
            if phase >= period - f32::EPSILON {
                phase = 0.0;
            }
            if phase >= dash - f32::EPSILON && in_dash && !current.is_empty() {
                out.push(dash_shape(std::mem::take(&mut current), stroke));
            }
        }
    }
    if current.len() > 1 {
        out.push(dash_shape(current, stroke));
    }
    out
}

/// One dash. Points packed closer than a pixel draw unevenly at thin widths,
/// so a dash keeps only points at least 2 px apart (and its ends).
fn dash_shape(points: Vec<Pos2>, stroke: Stroke) -> Shape {
    let last = *points.last().unwrap();
    let mut kept: Vec<Pos2> = Vec::with_capacity(points.len());
    for p in points {
        if kept.last().is_none_or(|k: &Pos2| k.distance(p) >= 2.0) {
            kept.push(p);
        }
    }
    if kept.last() != Some(&last) {
        kept.push(last);
    }
    if kept.len() == 2 {
        Shape::line_segment([kept[0], kept[1]], stroke)
    } else {
        Shape::line(kept, stroke)
    }
}

fn quad(mesh: &mut Mesh, a: Pos2, b: Pos2, c: Pos2, d: Pos2, color: Color32) {
    let i = mesh.vertices.len() as u32;
    for v in [a, b, c, d] {
        mesh.colored_vertex(v, color);
    }
    mesh.add_triangle(i, i + 1, i + 2);
    mesh.add_triangle(i, i + 2, i + 3);
}

fn nearest(xs: &[f64], x: f64) -> usize {
    let i = xs.partition_point(|v| *v < x);
    if i == 0 {
        0
    } else if i >= xs.len() {
        xs.len() - 1
    } else if (xs[i] - x).abs() < (x - xs[i - 1]).abs() {
        i
    } else {
        i - 1
    }
}

/// 1, 2, 2.5 or 5 times a power of ten, at least `raw`.
pub fn nice_step(raw: f64) -> f64 {
    if !raw.is_finite() || raw <= 0.0 {
        return 1.0;
    }
    let mag = 10f64.powf(raw.log10().floor());
    let n = raw / mag;
    let f = if n <= 1.0 {
        1.0
    } else if n <= 2.0 {
        2.0
    } else if n <= 2.5 {
        2.5
    } else if n <= 5.0 {
        5.0
    } else {
        10.0
    };
    f * mag
}

/// Ticks for a time axis from `a` to `b` (Unix seconds), at least ~72 px
/// apart, labelled day first: "3pm" and "16 Oct" for hours, "16 Sep" for
/// days, "Sep 2026" for months and "2026" for years.
pub fn time_ticks(a: f64, b: f64, width: f32) -> Vec<(f64, String)> {
    const H: f64 = 3600.0;
    const D: f64 = 86400.0;
    let span = (b - a).max(1.0);
    let fits = |approx: f64| span / approx * 72.0 <= width as f64;
    let local = |s: f64| Local.timestamp_opt(s as i64, 0).single();
    let at = |d: NaiveDate, h: u32| {
        Local
            .from_local_datetime(&d.and_hms_opt(h, 0, 0).unwrap())
            .earliest()
            .map(|t| t.timestamp() as f64)
    };
    let mut out = Vec::new();
    let (Some(start), Some(_)) = (local(a), local(b)) else {
        return out;
    };
    let first_day = start.date_naive();

    if let Some(&hours) = [1u32, 2, 3, 6, 12].iter().find(|&&h| fits(h as f64 * H)) {
        let mut d = first_day;
        'outer: loop {
            for h in (0..24).filter(|h| h % hours == 0) {
                let Some(t) = at(d, h) else { continue };
                if t > b {
                    break 'outer;
                }
                if t >= a {
                    let label = if h == 0 {
                        format::short_date(d)
                    } else {
                        format::axis_hour(h)
                    };
                    out.push((t, label));
                }
            }
            d += Duration::days(1);
        }
        return out;
    }
    if let Some(&days) = [1i64, 2, 7, 14].iter().find(|&&n| fits(n as f64 * D)) {
        let mut d = first_day;
        if days == 7 || days == 14 {
            d -= Duration::days(d.weekday().num_days_from_monday() as i64);
        }
        while let Some(t) = at(d, 0).filter(|t| *t <= b) {
            // Every day, every other day, Mondays, or every other Monday.
            let on_step = match days {
                1 | 7 => true,
                2 => d.num_days_from_ce() % 2 == 0,
                _ => d.iso_week().week().is_multiple_of(2),
            };
            if t >= a && on_step {
                out.push((t, format::short_date(d)));
            }
            d += Duration::days(if days >= 7 { 7 } else { 1 });
        }
        return out;
    }
    if let Some(&months) = [1u32, 2, 3, 6].iter().find(|&&n| fits(n as f64 * 30.5 * D)) {
        let mut d = first_day.with_day(1).unwrap();
        while let Some(t) = at(d, 0).filter(|t| *t <= b) {
            if t >= a && d.month0().is_multiple_of(months) {
                out.push((t, format!("{} {}", MONTHS[d.month0() as usize], d.year())));
            }
            d = if d.month() == 12 {
                NaiveDate::from_ymd_opt(d.year() + 1, 1, 1).unwrap()
            } else {
                NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1).unwrap()
            };
        }
        return out;
    }
    let years = [1i32, 2, 5, 10, 20]
        .into_iter()
        .find(|&n| fits(n as f64 * 365.25 * D))
        .unwrap_or(50);
    let mut y = start.year();
    while let Some(t) = NaiveDate::from_ymd_opt(y, 1, 1).and_then(|d| at(d, 0)) {
        if t > b {
            break;
        }
        if t >= a && y % years == 0 {
            out.push((t, y.to_string()));
        }
        y += 1;
    }
    out
}

/// Unix seconds for local midnight at the start of `d`.
pub fn day_ts(d: NaiveDate) -> f64 {
    Local
        .from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap())
        .earliest()
        .map(|t| t.timestamp() as f64)
        .unwrap_or(0.0)
}

/// The local date of a Unix-seconds value.
pub fn ts_day(s: f64) -> NaiveDate {
    Local
        .timestamp_opt(s.round() as i64, 0)
        .single()
        .map(|t| t.date_naive())
        .unwrap_or_default()
}

/// Local hour and minute of a Unix-seconds value, for read-outs.
pub fn ts_time(s: f64) -> String {
    format::time_and_date((s * 1000.0) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps() {
        assert_eq!(nice_step(0.7), 1.0);
        assert_eq!(nice_step(3.2), 5.0);
        assert_eq!(nice_step(12.0), 20.0);
        assert_eq!(nice_step(0.21), 0.25);
    }

    #[test]
    fn ticks_suit_the_span() {
        let a = day_ts(NaiveDate::from_ymd_opt(2026, 10, 15).unwrap());
        let day = time_ticks(a, a + 86400.0, 800.0);
        // The clock setting is global, so either form may be in force.
        assert!(day.iter().any(|t| t.1 == "15:00" || t.1 == "3pm"));
        assert!(day.len() >= 6 && day.len() <= 12, "{day:?}");
        let month = time_ticks(a, a + 30.0 * 86400.0, 800.0);
        assert!(
            month
                .iter()
                .all(|t| t.1.contains(' ') && !t.1.contains("pm")),
            "{month:?}"
        );
        let years = time_ticks(a - 8.0 * 365.0 * 86400.0, a, 800.0);
        assert!(
            years.iter().all(|t| t.1.len() == 4 || t.1.contains(' ')),
            "{years:?}"
        );
        assert!(years.len() <= 12);
    }

    #[test]
    fn dashes_run_on_across_short_segments() {
        // 100 points 1 px apart: 99 px of line, so 11 dashes of 5 px.
        let pts: Vec<Pos2> = (0..100).map(|i| pos2(i as f32, 0.0)).collect();
        let shapes = dashed(&pts, Stroke::new(1.0, Color32::BLACK), 5.0, 4.0);
        assert_eq!(shapes.len(), 11);
    }

    #[test]
    fn nearest_index() {
        let xs = [0.0, 10.0, 20.0];
        assert_eq!(nearest(&xs, -5.0), 0);
        assert_eq!(nearest(&xs, 14.0), 1);
        assert_eq!(nearest(&xs, 16.0), 2);
        assert_eq!(nearest(&xs, 99.0), 2);
    }
}
