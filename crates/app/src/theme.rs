//! Eisbär-Theme: arktische Farben (Polarnacht dunkel, Schnee hell), Eisbär-Logo und App-Icon.
//! Das Logo ist einmal als Formenliste definiert und wird für die Oberfläche (egui) und das
//! Fenster-Icon (tiny-skia) gleich gezeichnet.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Pos2, Shape, Stroke, TextStyle, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemeKind {
    /// Dunkles Nachtblau mit Eis-Akzenten.
    #[default]
    Polarnacht,
    /// Helles Schneeweiß mit Gletscherblau.
    Schnee,
}

/// Farben, die auch außerhalb der egui-Visuals gebraucht werden (Badges, Buttons, Leinwand).
#[derive(Clone, Copy)]
pub struct Palette {
    pub accent: Color32,
    pub accent_strong: Color32,
    pub on_accent: Color32,
    pub canvas_bg: Color32,
    pub badge_ct: Color32,
    pub badge_edited: Color32,
    pub badge_cancelled: Color32,
    pub badge_ok: Color32,
    pub badge_muted: Color32,
    pub ok_text: Color32,
    pub cancelled_text: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub fn palette(kind: ThemeKind) -> Palette {
    match kind {
        ThemeKind::Polarnacht => Palette {
            accent: hex(0x7CC4F0),
            accent_strong: hex(0x2F8FD0),
            on_accent: Color32::WHITE,
            canvas_bg: hex(0x08121B),
            badge_ct: hex(0x2F6E9E),
            badge_edited: hex(0x8A6A2E),
            badge_cancelled: hex(0x9E3D48),
            badge_ok: hex(0x2E8C86),
            badge_muted: hex(0x3A4C5C),
            ok_text: hex(0x7FDCCB),
            cancelled_text: hex(0xF0848A),
        },
        ThemeKind::Schnee => Palette {
            accent: hex(0x1F78B4),
            accent_strong: hex(0x1F78B4),
            on_accent: Color32::WHITE,
            canvas_bg: hex(0xD5E3EE),
            badge_ct: hex(0x3C7DB0),
            badge_edited: hex(0xB08431),
            badge_cancelled: hex(0xC0505C),
            badge_ok: hex(0x2E9A92),
            badge_muted: hex(0x8A9BAA),
            ok_text: hex(0x1F8A7E),
            cancelled_text: hex(0xC0404C),
        },
    }
}

fn widget(bg: Color32, stroke: Color32, fg: Color32, radius: u8) -> egui::style::WidgetVisuals {
    egui::style::WidgetVisuals {
        bg_fill: bg,
        weak_bg_fill: bg,
        bg_stroke: Stroke::new(1.0, stroke),
        corner_radius: CornerRadius::same(radius),
        fg_stroke: Stroke::new(1.5, fg),
        expansion: 0.0,
    }
}

fn visuals(kind: ThemeKind) -> egui::Visuals {
    let p = palette(kind);
    match kind {
        ThemeKind::Polarnacht => {
            let mut v = egui::Visuals::dark();
            let text = hex(0xE8F1F8);
            v.panel_fill = hex(0x0E1A26);
            v.window_fill = hex(0x12212F);
            v.window_stroke = Stroke::new(1.0, hex(0x2A4A63));
            v.extreme_bg_color = hex(0x08121B);
            v.text_edit_bg_color = Some(hex(0x0A1620));
            v.faint_bg_color = hex(0x13283A);
            v.code_bg_color = hex(0x13283A);
            v.hyperlink_color = p.accent;
            v.warn_fg_color = hex(0xF5C26B);
            v.error_fg_color = hex(0xF0848A);
            v.override_text_color = Some(text);
            v.selection.bg_fill = hex(0x2B6C9E);
            v.selection.stroke = Stroke::new(1.0, hex(0xBFE3FA));
            v.widgets.noninteractive = widget(hex(0x0E1A26), hex(0x1E3A52), text, 8);
            v.widgets.inactive = widget(hex(0x1A3043), hex(0x2A4A63), text, 8);
            v.widgets.hovered = widget(hex(0x24435C), p.accent, Color32::WHITE, 8);
            v.widgets.active = widget(hex(0x2E5677), hex(0xBFE3FA), Color32::WHITE, 8);
            v.widgets.open = widget(hex(0x24435C), p.accent, Color32::WHITE, 8);
            v.window_corner_radius = CornerRadius::same(12);
            v.menu_corner_radius = CornerRadius::same(10);
            v
        }
        ThemeKind::Schnee => {
            let mut v = egui::Visuals::light();
            let text = hex(0x13293D);
            v.panel_fill = hex(0xF4F8FB);
            v.window_fill = Color32::WHITE;
            v.window_stroke = Stroke::new(1.0, hex(0xC6D8E6));
            v.extreme_bg_color = Color32::WHITE;
            v.text_edit_bg_color = Some(Color32::WHITE);
            v.faint_bg_color = hex(0xE8F0F6);
            v.code_bg_color = hex(0xE8F0F6);
            v.hyperlink_color = p.accent;
            v.warn_fg_color = hex(0xB07A10);
            v.error_fg_color = hex(0xC0404C);
            v.override_text_color = Some(text);
            v.selection.bg_fill = hex(0x9CCBEF);
            v.selection.stroke = Stroke::new(1.0, hex(0x1F78B4));
            v.widgets.noninteractive = widget(hex(0xF4F8FB), hex(0xD3E2EE), text, 8);
            v.widgets.inactive = widget(hex(0xE1EBF3), hex(0xC6D8E6), text, 8);
            v.widgets.hovered = widget(hex(0xCFE2F1), p.accent, text, 8);
            v.widgets.active = widget(hex(0xB5D3EC), p.accent, text, 8);
            v.widgets.open = widget(hex(0xCFE2F1), p.accent, text, 8);
            v.window_corner_radius = CornerRadius::same(12);
            v.menu_corner_radius = CornerRadius::same(10);
            v
        }
    }
}

/// Theme, Schriftgrößen, Abstände und Zoom anwenden.
pub fn apply(ctx: &egui::Context, kind: ThemeKind, scale: f32) {
    ctx.set_visuals(visuals(kind));
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(16.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(16.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(15.0, FontFamily::Monospace)),
            (TextStyle::Heading, FontId::new(24.0, FontFamily::Proportional)),
        ]
        .into();
        let sp = &mut style.spacing;
        sp.button_padding = egui::vec2(12.0, 7.0);
        sp.interact_size = egui::vec2(40.0, 32.0);
        sp.item_spacing = egui::vec2(10.0, 8.0);
        sp.icon_width = 20.0;
        sp.icon_width_inner = 12.0;
        sp.icon_spacing = 6.0;
    });
    ctx.set_zoom_factor(scale.clamp(0.75, 2.0));
}

// ---------------------------------------------------------------------------------------------
// Eisbär-Logo

#[derive(Clone, Copy)]
enum Part {
    Circle { c: (f32, f32), r: f32, color: u32 },
    Ellipse { c: (f32, f32), rx: f32, ry: f32, color: u32 },
    Line { a: (f32, f32), b: (f32, f32), w: f32, color: u32 },
}

const FUR: u32 = 0xF7FBFF;
const FUR_SHADE: u32 = 0xDCE8F2;
const EAR_INNER: u32 = 0xB9CCDD;
const DARK: u32 = 0x18232E;

/// Eisbärkopf in Einheitskoordinaten (0..1), von hinten nach vorne.
const BEAR: [Part; 14] = [
    Part::Circle { c: (0.24, 0.27), r: 0.14, color: FUR },
    Part::Circle { c: (0.76, 0.27), r: 0.14, color: FUR },
    Part::Circle { c: (0.24, 0.27), r: 0.07, color: EAR_INNER },
    Part::Circle { c: (0.76, 0.27), r: 0.07, color: EAR_INNER },
    Part::Ellipse { c: (0.5, 0.56), rx: 0.40, ry: 0.37, color: FUR },
    Part::Ellipse { c: (0.5, 0.71), rx: 0.19, ry: 0.15, color: FUR_SHADE },
    Part::Circle { c: (0.36, 0.50), r: 0.037, color: DARK },
    Part::Circle { c: (0.64, 0.50), r: 0.037, color: DARK },
    Part::Ellipse { c: (0.5, 0.64), rx: 0.075, ry: 0.052, color: DARK },
    // Lächeln: kurzer Strich unter der Nase, dann ein „ω“
    Part::Line { a: (0.5, 0.67), b: (0.5, 0.715), w: 0.02, color: DARK },
    Part::Line { a: (0.5, 0.715), b: (0.46, 0.755), w: 0.02, color: DARK },
    Part::Line { a: (0.46, 0.755), b: (0.415, 0.73), w: 0.02, color: DARK },
    Part::Line { a: (0.5, 0.715), b: (0.54, 0.755), w: 0.02, color: DARK },
    Part::Line { a: (0.54, 0.755), b: (0.585, 0.73), w: 0.02, color: DARK },
];

/// Zeichnet den Eisbärkopf in ein Quadrat der Kantenlänge `size`.
pub fn bear_logo(ui: &mut egui::Ui, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let painter = ui.painter_at(rect.expand(2.0));
    let at = |p: (f32, f32)| Pos2::new(rect.min.x + p.0 * size, rect.min.y + p.1 * size);
    // dezenter Eis-Rand um den Kopf, damit er auch auf hellem Grund sichtbar ist
    painter.add(Shape::ellipse_stroke(at((0.5, 0.56)), Vec2::new(0.40 * size, 0.37 * size), Stroke::new(1.0, hex(0x9CCBEF))));
    for part in BEAR {
        match part {
            Part::Circle { c, r, color } => {
                painter.circle_filled(at(c), r * size, hex(color));
            }
            Part::Ellipse { c, rx, ry, color } => {
                painter.add(Shape::ellipse_filled(at(c), Vec2::new(rx * size, ry * size), hex(color)));
            }
            Part::Line { a, b, w, color } => {
                painter.line_segment([at(a), at(b)], Stroke::new((w * size).max(1.0), hex(color)));
            }
        }
    }
    resp
}

/// App-Icon: Eisbär auf einer Eisscholle (abgerundetes Quadrat mit Gletscherverlauf).
pub fn app_icon(size: u32) -> egui::IconData {
    use tiny_skia::{FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, Point, SpreadMode, Transform};
    let mut pm = Pixmap::new(size, size).expect("Icongröße > 0");
    let s = size as f32;
    let color = |rgb: u32| tiny_skia::Color::from_rgba8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255);

    // Hintergrund
    let r = s * 0.22;
    let (x0, y0, x1, y1) = (s * 0.04, s * 0.04, s * 0.96, s * 0.96);
    let mut pb = PathBuilder::new();
    pb.move_to(x0 + r, y0);
    pb.line_to(x1 - r, y0);
    pb.quad_to(x1, y0, x1, y0 + r);
    pb.line_to(x1, y1 - r);
    pb.quad_to(x1, y1, x1 - r, y1);
    pb.line_to(x0 + r, y1);
    pb.quad_to(x0, y1, x0, y1 - r);
    pb.line_to(x0, y0 + r);
    pb.quad_to(x0, y0, x0 + r, y0);
    pb.close();
    if let Some(path) = pb.finish() {
        let shader = LinearGradient::new(
            Point::from_xy(0.0, 0.0),
            Point::from_xy(0.0, s),
            vec![GradientStop::new(0.0, color(0x6EC1F2)), GradientStop::new(1.0, color(0x174A75))],
            SpreadMode::Pad,
            Transform::identity(),
        )
        .unwrap_or(tiny_skia::Shader::SolidColor(color(0x2F8FD0)));
        let paint = Paint { shader, anti_alias: true, ..Paint::default() };
        pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }

    // Bär, etwas kleiner und nach unten versetzt
    let (scale, ox, oy) = (s * 0.74, s * 0.13, s * 0.17);
    let at = |p: (f32, f32)| (ox + p.0 * scale, oy + p.1 * scale);
    let mut paint = Paint { anti_alias: true, ..Paint::default() };
    for part in BEAR {
        match part {
            Part::Circle { c, r, color: col } => {
                let (cx, cy) = at(c);
                if let Some(p) = PathBuilder::from_circle(cx, cy, r * scale) {
                    paint.set_color(color(col));
                    pm.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
                }
            }
            Part::Ellipse { c, rx, ry, color: col } => {
                let (cx, cy) = at(c);
                if let Some(p) = tiny_skia::Rect::from_xywh(cx - rx * scale, cy - ry * scale, 2.0 * rx * scale, 2.0 * ry * scale).and_then(PathBuilder::from_oval) {
                    paint.set_color(color(col));
                    pm.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
                }
            }
            Part::Line { a, b, w, color: col } => {
                let mut pb = PathBuilder::new();
                let (ax, ay) = at(a);
                let (bx, by) = at(b);
                pb.move_to(ax, ay);
                pb.line_to(bx, by);
                if let Some(p) = pb.finish() {
                    paint.set_color(color(col));
                    let stroke = tiny_skia::Stroke { width: w * scale, line_cap: tiny_skia::LineCap::Round, ..Default::default() };
                    pm.stroke_path(&p, &paint, &stroke, Transform::identity(), None);
                }
            }
        }
    }

    egui::IconData { rgba: slidebear_render::to_rgba(&pm), width: size, height: size }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_has_white_bear_on_blue_ice() {
        let icon = app_icon(256);
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        let px = |x: usize, y: usize| &icon.rgba[(y * 256 + x) * 4..(y * 256 + x) * 4 + 4];
        // Stirn des Bären: fast weiß
        assert!(px(128, 100).iter().take(3).all(|&c| c > 230), "{:?}", px(128, 100));
        // Ecke innerhalb der Scholle: blau
        let c = px(30, 230);
        assert!(c[2] > c[0], "{c:?}");
        // ganz außen transparent (abgerundete Ecke)
        assert_eq!(px(1, 1)[3], 0);
        if let Some(path) = std::env::var_os("SLIDEBEAR_ICON_OUT") {
            image::RgbaImage::from_raw(256, 256, icon.rgba.clone()).unwrap().save(path).unwrap();
        }
    }
}
