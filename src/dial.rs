//! The wind dial on Now, and wind roses on Compare and Insights.

use std::f32::consts::TAU;

use eframe::egui::{pos2, vec2, Align2, Id, Pos2, Sense, Shape, Stroke, Ui};
use isobar_core::analytics::COMPASS16;

use crate::format;
use crate::theme::{self, alpha, Palette};

/// A point `r` from `c` at compass bearing `deg` (0 is north, clockwise).
fn polar(c: Pos2, r: f32, deg: f32) -> Pos2 {
    let a = deg.to_radians();
    pos2(c.x + a.sin() * r, c.y - a.cos() * r)
}

/// A compass dial whose pointer sits on the rim where the wind comes from,
/// with the average speed in the middle. The pointer swings to new bearings.
pub fn wind_dial(ui: &mut Ui, size: f32, bearing: Option<f64>, speed: Option<f64>, unit: &str) {
    let p = Palette::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let c = rect.center();
    let r = size / 2.0 * 0.86;
    let painter = ui.painter_at(rect);
    painter.circle_stroke(c, r, Stroke::new(1.0, p.grid));
    for i in 0..32 {
        let deg = i as f32 * 11.25;
        let major = i % 8 == 0;
        let inner = if major {
            r * 0.84
        } else if i % 2 == 0 {
            r * 0.9
        } else {
            r * 0.94
        };
        let stroke = if major {
            Stroke::new(1.5, p.ink2)
        } else {
            Stroke::new(1.0, p.grid)
        };
        painter.line_segment([polar(c, inner, deg), polar(c, r, deg)], stroke);
    }
    for (label, deg) in [("N", 0.0), ("E", 90.0), ("S", 180.0), ("W", 270.0)] {
        painter.text(
            polar(c, r * 0.66, deg),
            Align2::CENTER_CENTER,
            label,
            theme::semi(size * 0.085),
            p.ink2,
        );
    }

    // Swing the shortest way round to the new bearing.
    if let Some(b) = bearing {
        let id = Id::new("wind-dial-angle");
        let prev: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(b as f32);
        let delta = (b as f32 - prev).rem_euclid(360.0);
        let target = if delta > 180.0 {
            prev + delta - 360.0
        } else {
            prev + delta
        };
        ui.data_mut(|d| d.insert_temp(id, target));
        let shown = ui
            .ctx()
            .animate_value_with_time(id.with("anim"), target, 0.9);
        let tip = polar(c, r * 1.12, shown);
        let left = polar(c, r * 0.82, shown - 8.0);
        let right = polar(c, r * 0.82, shown + 8.0);
        painter.add(Shape::convex_polygon(
            vec![tip, right, left],
            p.wind,
            Stroke::NONE,
        ));
    }
    painter.text(
        c + vec2(0.0, -2.0),
        Align2::CENTER_CENTER,
        format::num(speed, 0),
        theme::display(size * 0.24),
        p.ink,
    );
    painter.text(
        c + vec2(0.0, size * 0.15),
        Align2::CENTER_CENTER,
        unit,
        theme::medium(size * 0.08),
        p.ink2,
    );
    let from = bearing.map_or("–", isobar_core::analytics::compass);
    response.on_hover_text(format!(
        "Wind from {from} at {} {unit}",
        format::num(speed, 0)
    ));
}

/// A 16-petal rose of how often each direction was the day's dominant wind.
/// `max` is the share the outer ring stands for, so two roses can share a
/// scale.
pub fn wind_rose(ui: &mut Ui, size: f32, counts: &[usize; 16], total: usize, max: f64) {
    let p = Palette::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let c = rect.center();
    let r = size / 2.0 * 0.74;
    let painter = ui.painter_at(rect);
    for f in [0.25, 0.5, 0.75, 1.0] {
        painter.circle_stroke(c, r * f, Stroke::new(0.8, p.grid));
    }
    painter.text(
        c + vec2(4.0, -r + 3.0),
        Align2::LEFT_TOP,
        format!("{:.0}%", max * 100.0),
        theme::medium(11.0),
        p.ink2,
    );
    let share = |i: usize| {
        if total > 0 {
            counts[i] as f64 / total as f64
        } else {
            0.0
        }
    };
    for i in 0..16 {
        let s = share(i);
        if s <= 0.0 {
            continue;
        }
        let pr = (s / max) as f32 * r;
        let mid = i as f32 * 22.5;
        let mut pts = vec![c];
        for k in 0..=6 {
            pts.push(polar(c, pr, mid - 9.0 + 18.0 * k as f32 / 6.0));
        }
        painter.add(Shape::convex_polygon(
            pts,
            alpha(p.wind, 0.7),
            Stroke::new(0.8, p.wind),
        ));
    }
    for (label, deg) in [("N", 0.0), ("E", 90.0), ("S", 180.0), ("W", 270.0)] {
        painter.text(
            polar(c, r + 16.0, deg),
            Align2::CENTER_CENTER,
            label,
            theme::semi(13.0),
            p.ink2,
        );
    }
    if let Some(pos) = response.hover_pos() {
        let d = pos - c;
        if d.length() <= r + 8.0 && total > 0 {
            let deg = (d.x.atan2(-d.y) / TAU * 360.0).rem_euclid(360.0) as f64;
            let i = isobar_core::analytics::compass_index(deg);
            response.on_hover_text_at_pointer(format!(
                "{}: {:.1}% of {} days",
                COMPASS16[i],
                share(i) * 100.0,
                total
            ));
        }
    }
}

/// The largest share across several roses, for a common scale.
pub fn shared_max(roses: &[&([usize; 16], usize)]) -> f64 {
    roses
        .iter()
        .flat_map(|(c, n)| {
            c.iter()
                .map(move |v| if *n > 0 { *v as f64 / *n as f64 } else { 0.0 })
        })
        .fold(0.05, f64::max)
}
