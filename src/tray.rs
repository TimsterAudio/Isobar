//! A readout in the system tray: the temperature drawn into the icon, the
//! other readings in its tooltip, and a menu to bring Isobar forward or
//! quit. On Windows it's a notification-area icon; on Linux a
//! StatusNotifierItem over D-Bus (KDE, GNOME with the AppIndicator
//! extension, Xfce, Cinnamon and others), with nothing extra to install.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use eframe::egui;
use isobar_core::LiveReading;

/// What the tray shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Readout {
    /// In the icon: "18°", or "–" with no reading.
    pub icon: String,
    /// The tooltip's first line.
    pub title: String,
    /// The tooltip's other lines.
    pub lines: Vec<String>,
}

impl Readout {
    pub fn of(station: &str, live: Option<&LiveReading>, units: &isobar_core::Units) -> Readout {
        let title = if station.trim().is_empty() {
            "Isobar".to_string()
        } else {
            format!("Isobar · {}", station.trim())
        };
        let Some(r) = live else {
            return Readout {
                icon: "–".into(),
                title,
                lines: vec!["No reading yet".into()],
            };
        };
        let t = |v: Option<f64>| v.map(|v| format!("{v:.1}{}", units.temp));
        let mut lines = Vec::new();
        if let Some(temp) = t(r.temp) {
            let mut s = temp;
            if let Some(f) = t(r.feels_like) {
                s.push_str(&format!(", feels {f}"));
            }
            lines.push(s);
        }
        if let Some(w) = r.wind_avg {
            let mut s = format!("Wind {w:.0} {}", units.wind);
            if let Some(b) = r.bearing.filter(|_| w > 0.0) {
                s.push(' ');
                s.push_str(isobar_core::analytics::compass(b));
            }
            if let Some(g) = r.gust_recent {
                s.push_str(&format!(", gusts {g:.0}"));
            }
            lines.push(s);
        }
        if let Some(rain) = r.rain_today {
            let digits = if units.rain.trim().eq_ignore_ascii_case("in") {
                2
            } else {
                1
            };
            lines.push(format!("Rain today {rain:.digits$} {}", units.rain));
        }
        let mut s = String::new();
        if let Some(h) = r.humidity {
            s.push_str(&format!("Humidity {h:.0}%"));
        }
        if let Some(p) = r.pressure {
            if !s.is_empty() {
                s.push_str(", ");
            }
            let digits = if units.press.to_ascii_lowercase().contains("in") {
                2
            } else {
                1
            };
            s.push_str(&format!("{p:.digits$} {}", units.press));
        }
        if !s.is_empty() {
            lines.push(s);
        }
        Readout {
            icon: r
                .temp
                .map_or_else(|| "–".into(), |v| format!("{:.0}°", v.round() + 0.0)),
            title,
            lines,
        }
    }

    /// The tooltip in one piece. Windows keeps 127 characters of it.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn tooltip(&self) -> String {
        let mut s = self.title.clone();
        for l in &self.lines {
            if s.chars().count() + l.chars().count() + 1 > 127 {
                break;
            }
            s.push('\n');
            s.push_str(l);
        }
        s
    }
}

/// The icon: the reading in white on a violet rounded square, as RGBA.
pub fn icon_rgba(text: &str, size: u32) -> Vec<u8> {
    let n = size as usize;
    let mut px = vec![0u8; n * n * 4];
    let (r, g, b) = (0x5b, 0x45, 0xa8);
    let radius = size as f32 * 0.22;
    let s = size as f32;
    for y in 0..n {
        for x in 0..n {
            // Coverage of a rounded square, softened at the edge.
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let dx = (radius - fx).max(fx - (s - radius)).max(0.0);
            let dy = (radius - fy).max(fy - (s - radius)).max(0.0);
            let d = (dx * dx + dy * dy).sqrt();
            let a = (radius - d + 0.5).clamp(0.0, 1.0);
            let i = (y * n + x) * 4;
            px[i..i + 4].copy_from_slice(&[r, g, b, (a * 255.0) as u8]);
        }
    }
    let font = FontRef::try_from_slice(include_bytes!(
        "../assets/fonts/BarlowCondensed-SemiBold.ttf"
    ))
    .expect("the built-in font loads");
    // As large as fits, leaving a little margin.
    let fit = |scale: f32| {
        let f = font.as_scaled(PxScale::from(scale));
        text.chars()
            .map(|c| f.h_advance(f.glyph_id(c)))
            .sum::<f32>()
    };
    let room = s * 0.92;
    let mut scale = s * 0.95;
    while scale > 4.0 && fit(scale) > room {
        scale -= 0.5;
    }
    let f = font.as_scaled(PxScale::from(scale));
    let width = fit(scale);
    let mut x = (s - width) / 2.0;
    // Centre the digits' height (ascent above the baseline, less the
    // empty space a cap height leaves).
    let cap = scale * 0.7;
    let baseline = (s + cap) / 2.0;
    for c in text.chars() {
        let id = f.glyph_id(c);
        let glyph = id.with_scale_and_position(PxScale::from(scale), ab_glyph::point(x, baseline));
        x += f.h_advance(id);
        if let Some(outline) = font.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            outline.draw(|gx, gy, cov| {
                let px_x = gx as i32 + bounds.min.x as i32;
                let px_y = gy as i32 + bounds.min.y as i32;
                if px_x < 0 || px_y < 0 || px_x >= size as i32 || px_y >= size as i32 {
                    return;
                }
                let i = (px_y as usize * n + px_x as usize) * 4;
                let cov = cov.clamp(0.0, 1.0);
                for k in 0..3 {
                    let v = px[i + k] as f32;
                    px[i + k] = (v + (255.0 - v) * cov) as u8;
                }
                px[i + 3] = px[i + 3].max((cov * 255.0) as u8);
            });
        }
    }
    px
}

/// What the tray asks of the window.
fn show_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.request_repaint();
}

fn quit(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    ctx.request_repaint();
}

pub use imp::Tray;

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use ksni::blocking::{Handle, TrayMethods};

    struct Item {
        ctx: egui::Context,
        readout: Readout,
    }

    impl ksni::Tray for Item {
        fn id(&self) -> String {
            "isobar".into()
        }
        fn title(&self) -> String {
            self.readout.title.clone()
        }
        fn category(&self) -> ksni::Category {
            ksni::Category::ApplicationStatus
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            [22u32, 32, 64]
                .into_iter()
                .map(|size| {
                    let mut data = icon_rgba(&self.readout.icon, size);
                    // RGBA to ARGB.
                    for p in data.chunks_exact_mut(4) {
                        p.rotate_right(1);
                    }
                    ksni::Icon {
                        width: size as i32,
                        height: size as i32,
                        data,
                    }
                })
                .collect()
        }
        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: self.readout.title.clone(),
                description: self.readout.lines.join("\n"),
                ..Default::default()
            }
        }
        fn activate(&mut self, _x: i32, _y: i32) {
            show_window(&self.ctx);
        }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::StandardItem;
            let mut items: Vec<ksni::MenuItem<Self>> = self
                .readout
                .lines
                .iter()
                .map(|l| {
                    StandardItem {
                        label: l.clone(),
                        enabled: false,
                        ..Default::default()
                    }
                    .into()
                })
                .collect();
            if !items.is_empty() {
                items.push(ksni::MenuItem::Separator);
            }
            items.push(
                StandardItem {
                    label: "Show Isobar".into(),
                    activate: Box::new(|t: &mut Self| show_window(&t.ctx)),
                    ..Default::default()
                }
                .into(),
            );
            items.push(
                StandardItem {
                    label: "Quit".into(),
                    activate: Box::new(|t: &mut Self| quit(&t.ctx)),
                    ..Default::default()
                }
                .into(),
            );
            items
        }
    }

    pub struct Tray {
        handle: Handle<Item>,
        shown: Readout,
    }

    impl Tray {
        /// Into the tray, if the desktop has one.
        pub fn start(ctx: &egui::Context) -> Result<Tray, String> {
            let handle = Item {
                ctx: ctx.clone(),
                readout: Readout::default(),
            }
            .spawn()
            .map_err(|e| e.to_string())?;
            Ok(Tray {
                handle,
                shown: Readout::default(),
            })
        }

        pub fn show(&mut self, r: &Readout) {
            if *r != self.shown {
                self.shown = r.clone();
                let r = r.clone();
                self.handle.update(move |t| t.readout = r);
            }
        }
    }

    impl Drop for Tray {
        fn drop(&mut self) {
            let _ = self.handle.shutdown();
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{
        Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    };

    pub struct Tray {
        icon: TrayIcon,
        shown: Readout,
    }

    fn icon(text: &str) -> Option<Icon> {
        Icon::from_rgba(icon_rgba(text, 32), 32, 32).ok()
    }

    impl Tray {
        /// Into the notification area. On the window's own thread, whose
        /// message loop serves the icon too.
        pub fn start(ctx: &egui::Context) -> Result<Tray, String> {
            let show = MenuItem::new("Show Isobar", true, None);
            let quit_item = MenuItem::new("Quit", true, None);
            let menu = Menu::new();
            menu.append_items(&[&show, &PredefinedMenuItem::separator(), &quit_item])
                .map_err(|e| e.to_string())?;
            let (show_id, quit_id) = (show.id().clone(), quit_item.id().clone());
            let c = ctx.clone();
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                if e.id == show_id {
                    show_window(&c);
                } else if e.id == quit_id {
                    quit(&c);
                }
            }));
            let c = ctx.clone();
            TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = e
                {
                    show_window(&c);
                }
            }));
            let mut b = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false)
                .with_tooltip("Isobar");
            if let Some(i) = icon("–") {
                b = b.with_icon(i);
            }
            let icon = b.build().map_err(|e| e.to_string())?;
            Ok(Tray {
                icon,
                shown: Readout::default(),
            })
        }

        pub fn show(&mut self, r: &Readout) {
            if *r == self.shown {
                return;
            }
            if r.icon != self.shown.icon {
                let _ = self.icon.set_icon(icon(&r.icon));
            }
            let _ = self.icon.set_tooltip(Some(r.tooltip()));
            self.shown = r.clone();
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    use super::*;

    pub struct Tray;

    impl Tray {
        pub fn start(_ctx: &egui::Context) -> Result<Tray, String> {
            Err("no tray on this system".into())
        }
        pub fn show(&mut self, _r: &Readout) {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readout_and_icon() {
        let r = LiveReading {
            temp: Some(18.34),
            feels_like: Some(16.9),
            wind_avg: Some(12.0),
            bearing: Some(200.0),
            gust_recent: Some(25.0),
            rain_today: Some(2.4),
            humidity: Some(64.0),
            pressure: Some(1016.2),
            ..Default::default()
        };
        let x = Readout::of("Home", Some(&r), &isobar_core::Units::default());
        assert_eq!(x.icon, "18°");
        assert_eq!(x.title, "Isobar · Home");
        assert_eq!(x.lines[0], "18.3°C, feels 16.9°C");
        assert!(x.lines[1].starts_with("Wind 12 km/h S"), "{:?}", x.lines);
        assert_eq!(x.lines[2], "Rain today 2.4 mm");
        assert!(x.tooltip().chars().count() <= 127);
        let none = Readout::of("", None, &isobar_core::Units::default());
        assert_eq!(none.icon, "–");
        let px = icon_rgba("-12°", 32);
        assert_eq!(px.len(), 32 * 32 * 4);
        // Some white text in the middle row, and clear corners.
        let row = 16 * 32 * 4;
        assert!((0..32).any(|x| px[row + x * 4] > 240));
        assert_eq!(px[3], 0);
    }
}
