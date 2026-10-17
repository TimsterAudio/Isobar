//! Shared pieces of the interface: panels, chips, buttons, legend keys,
//! headings, columns and tables.

use eframe::egui::{
    self, pos2, text::LayoutJob, vec2, Align, Align2, Color32, CornerRadius, CursorIcon, FontId,
    Frame, Layout, Margin, Rect, Response, Sense, Stroke, StrokeKind, TextFormat, Ui,
};

use crate::theme::{self, alpha, mix, Palette, RADIUS};

pub fn pal(ui: &Ui) -> Palette {
    Palette::of(ui.ctx())
}

/// A sheet of paper on the desk: the frame round each chart and table.
pub fn panel<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = pal(ui);
    Frame::new()
        .fill(p.sheet)
        .stroke(Stroke::new(1.0, p.grid))
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin {
            left: 24,
            right: 24,
            top: 18,
            bottom: 22,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 12.0;
            add(ui)
        })
        .inner
}

pub fn h1(ui: &mut Ui, text: &str) {
    let p = pal(ui);
    ui.label(
        egui::RichText::new(text)
            .font(theme::display(32.0))
            .color(p.ink),
    );
}

pub fn h2(ui: &mut Ui, text: &str) {
    let p = pal(ui);
    ui.label(
        egui::RichText::new(text)
            .font(theme::display(20.0))
            .color(p.ink),
    );
}

pub fn muted(ui: &mut Ui, text: impl Into<String>) -> Response {
    let p = pal(ui);
    ui.label(
        egui::RichText::new(text.into())
            .font(theme::body(14.0))
            .color(p.ink2),
    )
}

pub fn small(ui: &mut Ui, text: impl Into<String>, color: Color32) -> Response {
    ui.label(
        egui::RichText::new(text.into())
            .font(theme::body(13.0))
            .color(color),
    )
}

pub fn text(ui: &mut Ui, text: impl Into<String>, font: FontId, color: Color32) -> Response {
    ui.label(egui::RichText::new(text.into()).font(font).color(color))
}

/// Wrapped text no wider than `max`.
pub fn para(ui: &mut Ui, s: impl Into<String>, font: FontId, color: Color32, max: f32) {
    let w = ui.available_width().min(max);
    ui.allocate_ui_with_layout(vec2(w, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.set_max_width(w);
        ui.add(egui::Label::new(egui::RichText::new(s.into()).font(font).color(color)).wrap());
    });
}

/// A big reading and its unit: "1011.7 hPa".
pub fn reading(ui: &mut Ui, value: &str, unit: &str, size: f32, color: Color32) -> Response {
    let p = pal(ui);
    let mut job = LayoutJob::default();
    job.append(
        value,
        0.0,
        TextFormat {
            font_id: theme::display(size),
            color,
            ..Default::default()
        },
    );
    if !unit.is_empty() {
        job.append(
            unit,
            4.0,
            TextFormat {
                font_id: theme::display_medium(size * 0.42),
                color: p.ink2,
                ..Default::default()
            },
        );
    }
    ui.label(job)
}

/// A pill-shaped choice, dark when chosen.
pub fn chip(ui: &mut Ui, label: &str, on: bool) -> Response {
    let p = pal(ui);
    let font = theme::medium(14.0);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, p.ink);
    let size = vec2(galley.size().x + 26.0, 32.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let hovered = response.hovered();
    let (fill, border, color) = if on {
        (p.ink, p.ink, p.paper)
    } else if hovered {
        (Color32::TRANSPARENT, p.ink2, p.ink)
    } else {
        (Color32::TRANSPARENT, p.grid, p.ink2)
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(16),
        fill,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        theme::medium(14.0),
        color,
    );
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A row of chips; returns the index clicked.
pub fn chips(ui: &mut Ui, labels: &[&str], selected: Option<usize>) -> Option<usize> {
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for (i, l) in labels.iter().enumerate() {
            if chip(ui, l, selected == Some(i)).clicked() {
                clicked = Some(i);
            }
        }
    });
    clicked
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Primary,
    Secondary,
    Danger,
}

pub fn button(ui: &mut Ui, label: &str, kind: Kind) -> Response {
    let p = pal(ui);
    let font = theme::semi(14.0);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font.clone(), p.ink);
    let size = vec2(galley.size().x + 32.0, 36.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let h = response.hovered();
    let (fill, border, color) = match kind {
        Kind::Primary if h => (
            p.trace,
            p.trace,
            if p.dark { p.paper } else { Color32::WHITE },
        ),
        Kind::Primary => (p.ink, p.ink, p.paper),
        Kind::Secondary => (Color32::TRANSPARENT, if h { p.ink2 } else { p.grid }, p.ink),
        Kind::Danger => (
            Color32::TRANSPARENT,
            if h { p.heat } else { p.grid },
            p.heat,
        ),
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(RADIUS),
        fill,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    ui.painter()
        .text(rect.center(), Align2::CENTER_CENTER, label, font, color);
    response.on_hover_cursor(CursorIcon::PointingHand)
}

#[derive(Clone, Copy, PartialEq)]
pub enum KeyMark {
    Line,
    Dashed,
    Square,
}

/// A legend entry: a short stroke in the series colour, then its name.
pub fn key(ui: &mut Ui, label: &str, color: Color32, mark: KeyMark) {
    let p = pal(ui);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), theme::body(13.0), p.ink2);
    let (rect, _) = ui.allocate_exact_size(vec2(20.0 + galley.size().x, 18.0), Sense::hover());
    let y = rect.center().y;
    let x = rect.left();
    match mark {
        KeyMark::Line => {
            ui.painter().rect_filled(
                Rect::from_min_size(pos2(x, y - 1.5), vec2(14.0, 3.0)),
                2,
                color,
            );
        }
        KeyMark::Dashed => {
            for k in 0..3 {
                let x0 = x + k as f32 * 5.5;
                ui.painter()
                    .line_segment([pos2(x0, y), pos2(x0 + 3.5, y)], Stroke::new(2.0, color));
            }
        }
        KeyMark::Square => {
            ui.painter().rect_filled(
                Rect::from_min_size(pos2(x + 3.0, y - 4.0), vec2(8.0, 8.0)),
                2,
                color,
            );
        }
    }
    ui.painter()
        .galley(pos2(x + 20.0, y - galley.size().y / 2.0), galley, p.ink2);
}

/// `n` equal columns `gap` apart. With `dividers`, a faint rule separates
/// them. `f` fills column `i`.
pub fn columns(ui: &mut Ui, n: usize, gap: f32, dividers: bool, mut f: impl FnMut(usize, &mut Ui)) {
    let n = n.max(1);
    let total = ui.available_width();
    let w = ((total - gap * (n - 1) as f32) / n as f32).max(10.0);
    let p = pal(ui);
    let r = ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        let mut rects = Vec::new();
        for i in 0..n {
            let r = ui.vertical(|ui| {
                ui.set_width(w);
                f(i, ui);
            });
            rects.push(r.response.rect);
        }
        rects
    });
    if dividers {
        let rects = r.inner;
        let top = r.response.rect.top();
        let bottom = r.response.rect.bottom();
        for rr in rects.iter().skip(1) {
            let x = rr.left() - gap / 2.0;
            ui.painter()
                .line_segment([pos2(x, top), pos2(x, bottom)], Stroke::new(1.0, p.faint));
        }
    }
}

/// Two columns sharing the width `left : right`.
pub fn split<A, B>(
    ui: &mut Ui,
    ratio: f32,
    gap: f32,
    left: impl FnOnce(&mut Ui) -> A,
    right: impl FnOnce(&mut Ui) -> B,
) {
    let total = ui.available_width();
    let lw = ((total - gap) * ratio).floor();
    let rw = total - gap - lw;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.vertical(|ui| {
            ui.set_width(lw);
            left(ui);
        });
        ui.vertical(|ui| {
            ui.set_width(rw);
            right(ui);
        });
    });
}

/// [`split`], for two halves that both need the same state.
pub fn split_with<S>(
    ui: &mut Ui,
    ratio: f32,
    gap: f32,
    state: &mut S,
    left: impl FnOnce(&mut Ui, &mut S),
    right: impl FnOnce(&mut Ui, &mut S),
) {
    let total = ui.available_width();
    let lw = ((total - gap) * ratio).floor();
    let rw = total - gap - lw;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.vertical(|ui| {
            ui.set_width(lw);
            left(ui, state);
        });
        ui.vertical(|ui| {
            ui.set_width(rw);
            right(ui, state);
        });
    });
}

/// A full-width rule.
pub fn rule(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0, color),
    );
}

/// A centred message for a screen with nothing to show yet. Returns true if
/// its button was pressed.
pub fn empty(ui: &mut Ui, title: &str, body: &str, button_label: Option<&str>) -> bool {
    let buttons: Vec<(&str, Kind)> = button_label
        .map(|b| (b, Kind::Primary))
        .into_iter()
        .collect();
    empty_with(ui, title, body, &buttons).is_some()
}

/// [`empty`] with a row of buttons. Returns the index of the one pressed.
pub fn empty_with(ui: &mut Ui, title: &str, body: &str, buttons: &[(&str, Kind)]) -> Option<usize> {
    let p = pal(ui);
    let mut clicked = None;
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = 12.0;
        ui.label(
            egui::RichText::new(title)
                .font(theme::display(22.0))
                .color(p.ink),
        );
        let w = 440.0f32.min(ui.available_width());
        ui.allocate_ui(vec2(w, 0.0), |ui| {
            ui.set_max_width(w);
            ui.vertical_centered(|ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(body)
                            .font(theme::body(15.0))
                            .color(p.ink2),
                    )
                    .wrap(),
                );
            });
        });
        if !buttons.is_empty() {
            let gap = 10.0;
            let widths: f32 = buttons
                .iter()
                .map(|(b, _)| {
                    ui.painter()
                        .layout_no_wrap(b.to_string(), theme::semi(14.0), p.ink)
                        .size()
                        .x
                        + 32.0
                })
                .sum::<f32>()
                + gap * (buttons.len() - 1) as f32;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                ui.add_space(((ui.available_width() - widths) / 2.0).max(0.0));
                for (i, (b, kind)) in buttons.iter().enumerate() {
                    if button(ui, b, *kind).clicked() {
                        clicked = Some(i);
                    }
                }
            });
        }
    });
    ui.add_space(40.0);
    clicked
}

// ---------------------------------------------------------------- tables

pub struct Col {
    pub title: &'static str,
    /// Fixed width, or 0 to share what's left.
    pub width: f32,
    pub right: bool,
}

pub const fn col(title: &'static str, width: f32, right: bool) -> Col {
    Col {
        title,
        width,
        right,
    }
}

#[derive(Default, Clone)]
pub struct Cell {
    pub text: String,
    pub color: Option<Color32>,
    pub strong: bool,
    /// Smaller grey text after the main text ("partial").
    pub note: Option<String>,
    pub tooltip: Option<String>,
}

pub fn cell(text: impl Into<String>) -> Cell {
    Cell {
        text: text.into(),
        ..Default::default()
    }
}

impl Cell {
    pub fn color(mut self, c: Color32) -> Self {
        self.color = Some(c);
        self
    }
    pub fn strong(mut self) -> Self {
        self.strong = true;
        self
    }
    pub fn note(mut self, n: impl Into<String>) -> Self {
        self.note = Some(n.into());
        self
    }
    pub fn tooltip(mut self, t: impl Into<String>) -> Self {
        self.tooltip = Some(t.into());
        self
    }
}

/// A plain data table: a ruled header and faintly ruled rows.
pub fn table(ui: &mut Ui, cols: &[Col], rows: &[Vec<Cell>], font_size: f32) {
    let p = pal(ui);
    let total = ui.available_width();
    // Fixed columns shrink in proportion when the table is narrow, leaving
    // each flexible column at least 44 px.
    let fixed: f32 = cols.iter().map(|c| c.width).sum();
    let n_flex = cols.iter().filter(|c| c.width == 0.0).count();
    let room = (total - 44.0 * n_flex as f32).max(0.0);
    let scale = if fixed > room && fixed > 0.0 {
        room / fixed
    } else {
        1.0
    };
    let flex_w = if n_flex > 0 {
        ((total - fixed * scale) / n_flex as f32).max(44.0)
    } else {
        0.0
    };
    let widths: Vec<f32> = cols
        .iter()
        .map(|c| {
            if c.width == 0.0 {
                flex_w
            } else {
                c.width * scale
            }
        })
        .collect();
    let row_h = font_size * 2.15;
    let head_h = 30.0;
    let height = head_h + row_h * rows.len() as f32;
    let (rect, response) = ui.allocate_exact_size(vec2(total, height), Sense::hover());
    let painter = ui.painter_at(rect.expand(1.0));
    let pad = 8.0;
    let mut x = rect.left();
    for (c, w) in cols.iter().zip(&widths) {
        let (pos, align) = if c.right {
            (
                pos2(x + w - pad, rect.top() + head_h / 2.0),
                Align2::RIGHT_CENTER,
            )
        } else {
            (
                pos2(x + pad, rect.top() + head_h / 2.0),
                Align2::LEFT_CENTER,
            )
        };
        painter.text(pos, align, c.title, theme::semi(13.0), p.ink2);
        x += w;
    }
    painter.line_segment(
        [
            pos2(rect.left(), rect.top() + head_h),
            pos2(rect.right(), rect.top() + head_h),
        ],
        Stroke::new(1.0, p.grid),
    );
    let hover = response.hover_pos();
    let mut tip: Option<String> = None;
    for (ri, row) in rows.iter().enumerate() {
        let top = rect.top() + head_h + ri as f32 * row_h;
        let mid = top + row_h / 2.0;
        let mut x = rect.left();
        for ((c, w), cell) in cols.iter().zip(&widths).zip(row) {
            let font = if cell.strong {
                theme::semi(font_size)
            } else {
                theme::body(font_size)
            };
            let color = cell.color.unwrap_or(p.ink);
            let galley = painter.layout_no_wrap(cell.text.clone(), font, color);
            let note = cell
                .note
                .as_ref()
                .map(|n| painter.layout_no_wrap(n.clone(), theme::body(12.0), p.ink2));
            let content_w = galley.size().x + note.as_ref().map_or(0.0, |n| n.size().x + 4.0);
            let left = if c.right {
                x + w - pad - content_w
            } else {
                x + pad
            };
            let gh = galley.size().y;
            painter.galley(pos2(left, mid - gh / 2.0), galley, color);
            if let Some(n) = note {
                let nh = n.size().y;
                painter.galley(
                    pos2(left + content_w - n.size().x, mid - nh / 2.0 + 1.0),
                    n,
                    p.ink2,
                );
            }
            if let (Some(t), Some(h)) = (&cell.tooltip, hover) {
                if Rect::from_min_size(pos2(x, top), vec2(*w, row_h)).contains(h) {
                    tip = Some(t.clone());
                }
            }
            x += w;
        }
        painter.line_segment(
            [
                pos2(rect.left(), top + row_h),
                pos2(rect.right(), top + row_h),
            ],
            Stroke::new(1.0, p.faint),
        );
    }
    if let Some(t) = tip {
        response.on_hover_text_at_pointer(t);
    }
}

/// A short bar showing how far a value is along 0..=1.
pub fn meter(ui: &mut Ui, width: f32, frac: f32, color: Color32) {
    let p = pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 16.0), Sense::hover());
    let bar = Rect::from_center_size(rect.center(), vec2(width, 6.0));
    ui.painter().rect_filled(bar, 3, p.faint);
    let mut fill = bar;
    fill.set_width(width * frac.clamp(0.0, 1.0));
    ui.painter().rect_filled(fill, 3, color);
}

/// A faint banner, for notices like "You're looking at the demo station".
pub fn notice(ui: &mut Ui, s: &str) {
    let p = pal(ui);
    Frame::new()
        .fill(mix(p.faint, p.trace, 0.06))
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(s).font(theme::body(14.0)).color(p.ink));
        });
}

/// A labelled form field: bold label, the input, then optional help text.
pub fn field<R>(ui: &mut Ui, label: &str, help: Option<&str>, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = pal(ui);
    ui.spacing_mut().item_spacing.y = 6.0;
    ui.label(
        egui::RichText::new(label)
            .font(theme::semi(14.0))
            .color(p.ink),
    );
    let r = add(ui);
    if let Some(h) = help {
        para(ui, h, theme::body(13.0), p.ink2, 560.0);
    }
    ui.add_space(6.0);
    ui.spacing_mut().item_spacing.y = 12.0;
    r
}

/// The single-line text input used throughout Settings.
pub fn text_input(ui: &mut Ui, value: &mut String, hint: &str, width: f32) -> Response {
    let p = pal(ui);
    ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(egui::RichText::new(hint).color(alpha(p.ink2, 0.8)))
            .font(theme::medium(14.0))
            .desired_width(width)
            .margin(Margin::symmetric(10, 8)),
    )
}

/// A date with a calendar pop-up, shown day first. Returns true if changed.
pub fn date_input(
    ui: &mut Ui,
    id: &str,
    date: &mut chrono::NaiveDate,
    years: std::ops::RangeInclusive<i32>,
) -> bool {
    use chrono::Datelike;
    let mut j = jiff::civil::date(date.year() as i16, date.month() as i8, date.day() as i8);
    let before = j;
    ui.add(
        egui_extras::DatePickerButton::new(&mut j)
            .id_salt(id)
            .format("%-d %b %Y")
            .calendar_week(false)
            .highlight_weekends(false)
            .start_end_years(*years.start() as i16..=*years.end() as i16),
    );
    if j != before {
        if let Some(d) =
            chrono::NaiveDate::from_ymd_opt(j.year() as i32, j.month() as u32, j.day() as u32)
        {
            *date = d;
            return true;
        }
    }
    false
}

/// A drop-down of `options`, showing the chosen one's label.
pub fn select<T: PartialEq + Copy>(
    ui: &mut Ui,
    id: &str,
    value: &mut T,
    options: &[(T, String)],
    width: f32,
) -> bool {
    let current = options
        .iter()
        .find(|o| o.0 == *value)
        .map(|o| o.1.clone())
        .unwrap_or_default();
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(egui::RichText::new(current).font(theme::medium(14.0)))
        .width(width)
        .height(400.0)
        .show_ui(ui, |ui| {
            for (v, label) in options {
                if ui
                    .selectable_label(
                        *v == *value,
                        egui::RichText::new(label).font(theme::body(14.0)),
                    )
                    .clicked()
                    && *v != *value
                {
                    *value = *v;
                    changed = true;
                }
            }
        });
    changed
}

/// A small arrow pointing up (`dir` > 0), down (< 0) or right (0).
pub fn arrow(ui: &mut Ui, dir: i8, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(12.0, 16.0), Sense::hover());
    let c = rect.center();
    let stroke = Stroke::new(1.75, color);
    let (tip, tail, a, b) = match dir.signum() {
        1 => (
            pos2(c.x, c.y - 6.0),
            pos2(c.x, c.y + 6.0),
            pos2(c.x - 4.5, c.y - 1.5),
            pos2(c.x + 4.5, c.y - 1.5),
        ),
        -1 => (
            pos2(c.x, c.y + 6.0),
            pos2(c.x, c.y - 6.0),
            pos2(c.x - 4.5, c.y + 1.5),
            pos2(c.x + 4.5, c.y + 1.5),
        ),
        _ => (
            pos2(c.x + 6.0, c.y),
            pos2(c.x - 6.0, c.y),
            pos2(c.x + 1.5, c.y - 4.5),
            pos2(c.x + 1.5, c.y + 4.5),
        ),
    };
    let p = ui.painter();
    p.line_segment([tail, tip], stroke);
    p.line_segment([a, tip], stroke);
    p.line_segment([b, tip], stroke);
}
