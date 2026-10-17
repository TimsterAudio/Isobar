//! Isobar's look: the paper charts of a weather station's recording
//! instruments. Pale green-grey graph paper, slate ink, and the violet pen of
//! a thermograph. Violet is reserved for "the trace": the thing being
//! recorded.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
    Theme,
};

/// One corner radius runs through the app.
pub const RADIUS: u8 = 6;

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// The window background.
    pub paper: Color32,
    /// Panels and inputs.
    pub sheet: Color32,
    pub grid: Color32,
    pub faint: Color32,
    pub ink: Color32,
    pub ink2: Color32,
    pub trace: Color32,
    pub heat: Color32,
    pub cold: Color32,
    pub rain: Color32,
    pub wind: Color32,
    pub warn: Color32,
    pub ok: Color32,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const LIGHT: Palette = Palette {
    dark: false,
    paper: hex(0xe9eee8),
    sheet: hex(0xf6f8f4),
    grid: hex(0xcdd8cb),
    faint: hex(0xdfe6dd),
    ink: hex(0x1c2633),
    ink2: hex(0x5a6570),
    trace: hex(0x5b3a9e),
    heat: hex(0xb3362b),
    cold: hex(0x2a6896),
    rain: hex(0x2b68ad),
    wind: hex(0x23796a),
    warn: hex(0xa8700c),
    ok: hex(0x2f7a3e),
};

pub const DARK: Palette = Palette {
    dark: true,
    paper: hex(0x131a20),
    sheet: hex(0x19222a),
    grid: hex(0x2a3741),
    faint: hex(0x222d36),
    ink: hex(0xe6ebee),
    ink2: hex(0x9aa7b2),
    trace: hex(0xb49cf0),
    heat: hex(0xf0806c),
    cold: hex(0x7fb6e4),
    rain: hex(0x6ea7ea),
    wind: hex(0x5cc4a7),
    warn: hex(0xe2b04e),
    ok: hex(0x7fcf8b),
};

impl Palette {
    pub fn of(ctx: &egui::Context) -> Palette {
        match ctx.theme() {
            Theme::Dark => DARK,
            Theme::Light => LIGHT,
        }
    }
}

/// `c` at opacity `a` (0–1).
pub fn alpha(c: Color32, a: f32) -> Color32 {
    let [r, g, b, _] = c.to_array();
    Color32::from_rgba_unmultiplied(r, g, b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// A mix of `a` and `b`, `t` of the way to `b`.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    let (a, b) = (a.to_array(), b.to_array());
    Color32::from_rgba_unmultiplied(l(a[0], b[0]), l(a[1], b[1]), l(a[2], b[2]), l(a[3], b[3]))
}

// ---------------------------------------------------------------- fonts

const F_BODY: &str = "barlow";
const F_MEDIUM: &str = "barlow-medium";
const F_SEMI: &str = "barlow-semibold";
const F_DISPLAY: &str = "barlow-condensed-semibold";
const F_DISPLAY_MEDIUM: &str = "barlow-condensed-medium";

/// Barlow, regular.
pub fn body(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(F_BODY.into()))
}

/// Barlow, medium.
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(F_MEDIUM.into()))
}

/// Barlow, semibold.
pub fn semi(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(F_SEMI.into()))
}

/// Barlow Condensed, semibold: headings and big readings.
pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(F_DISPLAY.into()))
}

/// Barlow Condensed, medium.
pub fn display_medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(F_DISPLAY_MEDIUM.into()))
}

/// Barlow is built into the binary. egui's own fonts stay behind it for the
/// few symbols Barlow lacks (arrows, σ).
pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let faces: [(&str, &'static [u8]); 5] = [
        (F_BODY, include_bytes!("../assets/fonts/Barlow-Regular.ttf")),
        (
            F_MEDIUM,
            include_bytes!("../assets/fonts/Barlow-Medium.ttf"),
        ),
        (
            F_SEMI,
            include_bytes!("../assets/fonts/Barlow-SemiBold.ttf"),
        ),
        (
            F_DISPLAY,
            include_bytes!("../assets/fonts/BarlowCondensed-SemiBold.ttf"),
        ),
        (
            F_DISPLAY_MEDIUM,
            include_bytes!("../assets/fonts/BarlowCondensed-Medium.ttf"),
        ),
    ];
    let fallback = defs.families[&FontFamily::Proportional].clone();
    for (name, bytes) in faces {
        defs.font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
        let mut chain = vec![name.to_owned()];
        chain.extend(fallback.iter().cloned());
        defs.families.insert(FontFamily::Name(name.into()), chain);
    }
    defs.families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(0, F_BODY.to_owned());
    ctx.set_fonts(defs);
}

// ---------------------------------------------------------------- egui style

fn visuals(p: &Palette) -> egui::Visuals {
    let mut v = if p.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    let r = CornerRadius::same(RADIUS);
    v.override_text_color = Some(p.ink);
    v.panel_fill = p.paper;
    v.window_fill = p.sheet;
    v.window_stroke = Stroke::new(1.0, p.grid);
    v.window_corner_radius = r;
    v.menu_corner_radius = r;
    v.window_shadow = egui::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: alpha(Color32::BLACK, if p.dark { 0.4 } else { 0.12 }),
    };
    v.popup_shadow = v.window_shadow;
    v.extreme_bg_color = p.sheet;
    v.faint_bg_color = p.faint;
    v.code_bg_color = p.faint;
    v.hyperlink_color = p.trace;
    v.selection.bg_fill = alpha(p.trace, 0.25);
    v.selection.stroke = Stroke::new(1.0, p.trace);
    v.text_cursor.stroke = Stroke::new(2.0, p.trace);
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.heat;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.sheet;
    w.noninteractive.weak_bg_fill = p.sheet;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.grid);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.ink);
    for (state, border) in [
        (&mut w.inactive, p.grid),
        (&mut w.hovered, p.ink2),
        (&mut w.active, p.trace),
        (&mut w.open, p.ink2),
    ] {
        state.bg_fill = p.sheet;
        state.weak_bg_fill = p.sheet;
        state.bg_stroke = Stroke::new(1.0, border);
        state.fg_stroke = Stroke::new(1.5, p.ink);
        state.corner_radius = r;
        state.expansion = 0.0;
    }
    w.hovered.weak_bg_fill = mix(p.sheet, p.faint, 0.6);
    w.hovered.bg_fill = mix(p.sheet, p.faint, 0.6);
    w.active.fg_stroke = Stroke::new(1.5, p.trace);
    v
}

fn style(p: &Palette) -> egui::Style {
    let mut s = egui::Style {
        visuals: visuals(p),
        ..Default::default()
    };
    s.text_styles = [
        (TextStyle::Small, body(13.0)),
        (TextStyle::Body, body(15.0)),
        (TextStyle::Button, medium(14.0)),
        (TextStyle::Heading, display(32.0)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();
    s.spacing.item_spacing = egui::vec2(8.0, 8.0);
    s.spacing.button_padding = egui::vec2(10.0, 7.0);
    s.spacing.interact_size.y = 34.0;
    s.spacing.combo_width = 160.0;
    s.spacing.text_edit_width = 280.0;
    s.spacing.menu_margin = egui::Margin::same(6);
    s.spacing.icon_width = 16.0;
    s.spacing.icon_width_inner = 10.0;
    s.spacing.scroll = egui::style::ScrollStyle::solid();
    s.interaction.selectable_labels = false;
    s.url_in_tooltip = false;
    s
}

/// Install both looks; egui picks one from the theme preference.
pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_style_of(Theme::Light, style(&LIGHT));
    ctx.set_style_of(Theme::Dark, style(&DARK));
}
