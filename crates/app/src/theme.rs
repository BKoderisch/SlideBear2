//! Comic-Eisbär-Theme (nur hell): Schneeweiß und Gletscherblau, dicke Tinten-Konturen,
//! harte versetzte Schatten, sonnengelbe Hervorhebungen und die runde Schrift Fredoka.
//! Das Eisbär-Logo ist einmal als Formenliste definiert und wird für Oberfläche (egui) und
//! Fenster-Icon (tiny-skia) gleich gezeichnet.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, FontTweak, Pos2, Shadow, Shape, Stroke, TextStyle, Vec2,
    epaint::text::VariationCoords,
};

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// Tinte: Konturen, Schatten und Text.
pub const INK: Color32 = hex(0x16324F);
pub const SNOW: Color32 = hex(0xFFFFFF);
pub const ICE: Color32 = hex(0xEAF6FF);
pub const ICE_DEEP: Color32 = hex(0xBDE6FF);
pub const GLACIER: Color32 = hex(0x2FA4F0);
pub const SUN: Color32 = hex(0xFFE27A);

pub const BADGE_CT: Color32 = hex(0x9FD3F7);
pub const BADGE_EDITED: Color32 = hex(0xFFE27A);
pub const BADGE_CANCELLED: Color32 = hex(0xFF9AA2);
pub const BADGE_OK: Color32 = hex(0x9BE8C8);
pub const BADGE_MUTED: Color32 = hex(0xD5DEE6);
pub const OK_TEXT: Color32 = hex(0x13865F);
pub const CANCELLED_TEXT: Color32 = hex(0xD63A4A);
pub const CANVAS_BG: Color32 = hex(0xCFE9FA);

const RADIUS: u8 = 14;
const OUTLINE: f32 = 2.5;

/// Familie für Überschriften und Buttons (Fredoka, kräftiger Schnitt).
pub fn bold() -> FontFamily {
    FontFamily::Name("fredoka-bold".into())
}

fn widget(bg: Color32, stroke: f32, expansion: f32) -> egui::style::WidgetVisuals {
    egui::style::WidgetVisuals {
        bg_fill: bg,
        weak_bg_fill: bg,
        bg_stroke: Stroke::new(stroke, INK),
        corner_radius: CornerRadius::same(RADIUS),
        fg_stroke: Stroke::new(2.0, INK),
        expansion,
    }
}

fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::light();
    v.override_text_color = Some(INK);
    v.panel_fill = ICE;
    v.window_fill = SNOW;
    v.window_stroke = Stroke::new(3.0, INK);
    v.window_shadow = Shadow { offset: [6, 6], blur: 0, spread: 0, color: INK };
    v.popup_shadow = Shadow { offset: [4, 4], blur: 0, spread: 0, color: INK };
    v.window_corner_radius = CornerRadius::same(18);
    v.menu_corner_radius = CornerRadius::same(RADIUS);
    v.extreme_bg_color = SNOW;
    v.text_edit_bg_color = Some(SNOW);
    v.faint_bg_color = hex(0xD9EEFB);
    v.code_bg_color = hex(0xD9EEFB);
    v.hyperlink_color = hex(0x0E6FB8);
    v.warn_fg_color = hex(0xB86E00);
    v.error_fg_color = CANCELLED_TEXT;
    v.selection.bg_fill = SUN;
    v.selection.stroke = Stroke::new(2.0, INK);
    v.widgets.noninteractive = widget(ICE, 2.0, 0.0);
    v.widgets.inactive = widget(SNOW, OUTLINE, 0.0);
    v.widgets.hovered = widget(ICE_DEEP, 3.0, 2.0);
    v.widgets.active = widget(hex(0x7CC9F5), 3.0, 1.0);
    v.widgets.open = widget(ICE_DEEP, 3.0, 1.0);
    v.slider_trailing_fill = true;
    v
}

/// Fredoka als Oberflächenschrift; egui-Standardschriften bleiben als Fallback für Symbole.
fn fonts() -> FontDefinitions {
    let data: &'static [u8] = include_bytes!("../../../assets/fonts/Fredoka.ttf");
    let weight = |w: f32| FontTweak { coords: VariationCoords::new([(b"wght", w)]), ..FontTweak::default() };
    let mut defs = FontDefinitions::default();
    defs.font_data.insert("fredoka".into(), Arc::new(FontData::from_static(data).tweak(weight(500.0))));
    defs.font_data.insert("fredoka-bold".into(), Arc::new(FontData::from_static(data).tweak(weight(650.0))));
    let fallback = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    if let Some(f) = defs.families.get_mut(&FontFamily::Proportional) {
        f.insert(0, "fredoka".into());
    }
    defs.families.insert(bold(), std::iter::once("fredoka-bold".to_string()).chain(fallback).collect());
    defs
}

/// Einmal beim Start: Schrift, Farben, Größen.
pub fn install(ctx: &egui::Context, scale: f32) {
    ctx.set_fonts(fonts());
    apply(ctx, scale);
}

/// Farben, Schriftgrößen, Abstände und Zoom anwenden.
pub fn apply(ctx: &egui::Context, scale: f32) {
    ctx.all_styles_mut(|style| {
        style.visuals = visuals();
        style.text_styles = [
            (TextStyle::Small, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(17.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(17.0, bold())),
            (TextStyle::Monospace, FontId::new(15.0, FontFamily::Monospace)),
            (TextStyle::Heading, FontId::new(28.0, bold())),
        ]
        .into();
        let sp = &mut style.spacing;
        sp.button_padding = egui::vec2(14.0, 8.0);
        sp.interact_size = egui::vec2(40.0, 34.0);
        sp.item_spacing = egui::vec2(10.0, 9.0);
        sp.icon_width = 22.0;
        sp.icon_width_inner = 13.0;
        sp.icon_spacing = 7.0;
        sp.window_margin = egui::Margin::same(16);
    });
    ctx.set_zoom_factor(scale.clamp(0.75, 2.0));
}

/// Höhe einzeiliger Textfelder: Platz für Fredoka plus runde Comic-Ecken.
pub const FIELD_H: f32 = 42.0;

fn padded(edit: egui::TextEdit<'_>) -> egui::TextEdit<'_> {
    edit.margin(egui::Margin::symmetric(12, 6))
}

/// Einzeiliges Textfeld mit fester Breite. Feste Größe statt `desired_width`, weil Felder in
/// `egui::Grid` sonst an der (anfangs schmalen) Spaltenbreite des Vorframes kleben bleiben.
pub fn text_field(ui: &mut egui::Ui, edit: egui::TextEdit<'_>, width: f32) -> egui::Response {
    ui.add_sized([width, FIELD_H], padded(edit).vertical_align(egui::Align::Center))
}

/// Mehrzeiliges Textfeld mit fester Breite und Zeilenzahl.
pub fn text_area(ui: &mut egui::Ui, edit: egui::TextEdit<'_>, width: f32, rows: usize) -> egui::Response {
    let height = rows as f32 * 24.0 + 18.0;
    ui.add_sized([width, height], padded(edit.desired_rows(rows)))
}

/// Button mit hartem Comic-Schatten (versetzte Tinten-Fläche dahinter).
pub fn comic_button(ui: &mut egui::Ui, button: egui::Button) -> egui::Response {
    let shadow_slot = ui.painter().add(Shape::Noop);
    let resp = ui.add(button);
    let offset = if resp.is_pointer_button_down_on() { 1.0 } else { 4.0 };
    let rect = resp.rect.translate(Vec2::splat(offset));
    ui.painter().set(shadow_slot, Shape::rect_filled(rect, CornerRadius::same(RADIUS), INK));
    resp
}

/// Titel in Schneeweiß mit dicker Tinten-Kontur und versetztem Schatten.
pub fn comic_title(ui: &mut egui::Ui, text: &str, size: f32) {
    let font = FontId::new(size, bold());
    let size = ui.painter().layout_no_wrap(text.to_string(), font.clone(), SNOW).size();
    let (rect, _) = ui.allocate_exact_size(size + Vec2::splat(5.0), egui::Sense::hover());
    let p = ui.painter();
    let layout = |c: Color32| p.layout_no_wrap(text.to_string(), font.clone(), c);
    let snow = layout(SNOW);
    let o = rect.min + Vec2::splat(1.5);
    p.galley(o + Vec2::splat(3.5), layout(INK), INK);
    for (dx, dy) in [(-1.6, 0.0), (1.6, 0.0), (0.0, -1.6), (0.0, 1.6), (-1.1, -1.1), (1.1, 1.1), (-1.1, 1.1), (1.1, -1.1)] {
        p.galley(o + Vec2::new(dx, dy), layout(INK), INK);
    }
    p.galley(o, snow, SNOW);
}

const BUBBLE_RADIUS: f32 = 12.0;

/// Comic-Sprechblase mit hartem Schatten und einem Zipfel, dessen Spitze auf `speaker` zeigt
/// (z. B. das Maul des Eisbären links daneben).
pub fn speech_bubble<R>(ui: &mut egui::Ui, fill: Color32, speaker: Pos2, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.add_space(14.0);
    let p = ui.painter().clone();
    let shadow_slot = p.add(Shape::Noop);
    let tail_shadow_slot = p.add(Shape::Noop);
    let inner = egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(OUTLINE, INK))
        .corner_radius(CornerRadius::same(BUBBLE_RADIUS as u8))
        .inner_margin(egui::Margin { left: 14, right: 16, top: 6, bottom: 6 })
        .show(ui, add_contents);
    let r = inner.response.rect;
    let off = Vec2::splat(3.0);
    // Zipfel: breite Basis auf Höhe des Sprechers, knapp innerhalb der Kontur (überdeckt sie dort),
    // Spitze kurz vor dem Sprecher
    // nur auf dem geraden Stück der linken Kante ansetzen, nicht in den runden Ecken
    const HALF: f32 = 6.0;
    let (lo, hi) = (r.top() + BUBBLE_RADIUS + HALF, r.bottom() - BUBBLE_RADIUS - HALF);
    let base_y = if lo <= hi { speaker.y.clamp(lo, hi) } else { r.center().y };
    let top = Pos2::new(r.left() + OUTLINE, base_y - HALF);
    let bottom = Pos2::new(r.left() + OUTLINE, base_y + HALF);
    let tip = Pos2::new(speaker.x + 2.0, speaker.y);
    p.set(shadow_slot, Shape::rect_filled(r.translate(off), CornerRadius::same(BUBBLE_RADIUS as u8), INK));
    p.set(tail_shadow_slot, Shape::convex_polygon(vec![top + off, tip + off, bottom + off], INK, Stroke::NONE));
    p.add(Shape::convex_polygon(vec![top, tip, bottom], fill, Stroke::NONE));
    let ink = Stroke::new(OUTLINE, INK);
    p.line_segment([Pos2::new(r.left(), top.y), tip], ink);
    p.line_segment([tip, Pos2::new(r.left(), bottom.y)], ink);
    inner.inner
}

/// Pillen-Badge mit Tinten-Kontur.
pub fn badge(ui: &mut egui::Ui, text: &str, fill: Color32) {
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(2.0, INK))
        .corner_radius(CornerRadius::same(255))
        .inner_margin(egui::Margin::symmetric(8, 1))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).small().color(INK).family(bold()));
        });
}

/// Dekorative Schneeflocken im Hintergrund eines Bereichs.
pub fn snowflakes(ui: &egui::Ui, rect: egui::Rect) {
    let p = ui.painter_at(rect);
    let flakes = [(0.30, 0.28, 18.0), (0.40, 0.72, 12.0), (0.52, 0.30, 14.0), (0.63, 0.70, 20.0), (0.74, 0.30, 12.0), (0.86, 0.68, 16.0)];
    for (fx, fy, size) in flakes {
        let pos = Pos2::new(rect.left() + rect.width() * fx, rect.top() + rect.height() * fy);
        p.text(pos, egui::Align2::CENTER_CENTER, "❄", FontId::proportional(size), ICE_DEEP);
    }
}

// ---------------------------------------------------------------------------------------------
// Eisbär-Logo

#[derive(Clone, Copy)]
enum Part {
    Circle { c: (f32, f32), r: f32, color: u32, outline: bool },
    Ellipse { c: (f32, f32), rx: f32, ry: f32, color: u32, outline: bool },
    Line { a: (f32, f32), b: (f32, f32) },
}

const FUR: u32 = 0xFFFFFF;
const FUR_SHADE: u32 = 0xE3F1FB;
const EAR_INNER: u32 = 0xFFB3C1;
const CHEEK: u32 = 0xFFC7D1;
const INK_HEX: u32 = 0x16324F;
/// Konturstärke relativ zur Logogröße.
const OUTLINE_W: f32 = 0.045;
const LINE_W: f32 = 0.03;

/// Comic-Eisbärkopf in Einheitskoordinaten (0..1), von hinten nach vorne.
const BEAR: [Part; 18] = [
    Part::Circle { c: (0.22, 0.26), r: 0.15, color: FUR, outline: true },
    Part::Circle { c: (0.78, 0.26), r: 0.15, color: FUR, outline: true },
    Part::Circle { c: (0.22, 0.26), r: 0.075, color: EAR_INNER, outline: false },
    Part::Circle { c: (0.78, 0.26), r: 0.075, color: EAR_INNER, outline: false },
    Part::Ellipse { c: (0.5, 0.57), rx: 0.41, ry: 0.37, color: FUR, outline: true },
    Part::Ellipse { c: (0.27, 0.66), rx: 0.065, ry: 0.04, color: CHEEK, outline: false },
    Part::Ellipse { c: (0.73, 0.66), rx: 0.065, ry: 0.04, color: CHEEK, outline: false },
    Part::Ellipse { c: (0.5, 0.71), rx: 0.19, ry: 0.15, color: FUR_SHADE, outline: true },
    // Augen mit Glanzpunkt
    Part::Circle { c: (0.35, 0.50), r: 0.048, color: INK_HEX, outline: false },
    Part::Circle { c: (0.65, 0.50), r: 0.048, color: INK_HEX, outline: false },
    Part::Circle { c: (0.365, 0.485), r: 0.016, color: FUR, outline: false },
    Part::Circle { c: (0.665, 0.485), r: 0.016, color: FUR, outline: false },
    Part::Ellipse { c: (0.5, 0.635), rx: 0.08, ry: 0.055, color: INK_HEX, outline: false },
    // Lächeln: Strich unter der Nase, dann ein „ω“
    Part::Line { a: (0.5, 0.67), b: (0.5, 0.715) },
    Part::Line { a: (0.5, 0.715), b: (0.455, 0.76) },
    Part::Line { a: (0.455, 0.76), b: (0.41, 0.735) },
    Part::Line { a: (0.5, 0.715), b: (0.545, 0.76) },
    Part::Line { a: (0.545, 0.76), b: (0.59, 0.735) },
];

/// Zeichnet den Comic-Eisbärkopf in ein Quadrat der Kantenlänge `size`.
pub fn bear_logo(ui: &mut egui::Ui, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    // Beim Darüberfahren hüpft der Bär ein kleines Stück
    let rect = if resp.hovered() { rect.translate(Vec2::new(0.0, -size * 0.06)) } else { rect };
    let painter = ui.painter_at(rect.expand(size * 0.1));
    let at = |p: (f32, f32)| Pos2::new(rect.min.x + p.0 * size, rect.min.y + p.1 * size);
    let ink = Stroke::new((OUTLINE_W * size).max(1.5), INK);
    let line = Stroke::new((LINE_W * size).max(1.2), INK);
    for part in BEAR {
        match part {
            Part::Circle { c, r, color, outline } => {
                painter.circle(at(c), r * size, hex(color), if outline { ink } else { Stroke::NONE });
            }
            Part::Ellipse { c, rx, ry, color, outline } => {
                let radius = Vec2::new(rx * size, ry * size);
                painter.add(Shape::ellipse_filled(at(c), radius, hex(color)));
                if outline {
                    painter.add(Shape::ellipse_stroke(at(c), radius, ink));
                }
            }
            Part::Line { a, b } => {
                painter.line_segment([at(a), at(b)], line);
            }
        }
    }
    resp
}

/// App-Icon: Comic-Eisbär auf einer Eisscholle mit Tinten-Rand und hartem Schatten.
pub fn app_icon(size: u32) -> egui::IconData {
    use tiny_skia::{FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, Point, SpreadMode, Transform};
    let mut pm = Pixmap::new(size, size).expect("Icongröße > 0");
    let s = size as f32;
    let color = |rgb: u32| tiny_skia::Color::from_rgba8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255);
    let rounded = |x0: f32, y0: f32, x1: f32, y1: f32, r: f32| {
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
        pb.finish()
    };
    let ink_stroke = |w: f32| tiny_skia::Stroke {
        width: w,
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..Default::default()
    };

    // harter Schatten, dann Scholle mit Verlauf und dicker Kontur
    let (x0, y0, x1, y1, r) = (s * 0.05, s * 0.05, s * 0.91, s * 0.91, s * 0.2);
    let mut paint = Paint { anti_alias: true, ..Paint::default() };
    if let Some(p) = rounded(x0 + s * 0.04, y0 + s * 0.04, x1 + s * 0.04, y1 + s * 0.04, r) {
        paint.set_color(color(INK_HEX));
        pm.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
    }
    if let Some(p) = rounded(x0, y0, x1, y1, r) {
        let shader = LinearGradient::new(
            Point::from_xy(0.0, y0),
            Point::from_xy(0.0, y1),
            vec![GradientStop::new(0.0, color(0x9FDBFF)), GradientStop::new(1.0, color(0x2FA4F0))],
            SpreadMode::Pad,
            Transform::identity(),
        )
        .unwrap_or(tiny_skia::Shader::SolidColor(color(0x2FA4F0)));
        let fill = Paint { shader, anti_alias: true, ..Paint::default() };
        pm.fill_path(&p, &fill, FillRule::Winding, Transform::identity(), None);
        paint.set_color(color(INK_HEX));
        pm.stroke_path(&p, &paint, &ink_stroke(s * 0.025), Transform::identity(), None);
    }

    let (scale, ox, oy) = (s * 0.72, s * 0.12, s * 0.15);
    let at = |p: (f32, f32)| (ox + p.0 * scale, oy + p.1 * scale);
    for part in BEAR {
        let (path, fill, outline) = match part {
            Part::Circle { c, r, color: col, outline } => {
                let (cx, cy) = at(c);
                (PathBuilder::from_circle(cx, cy, r * scale), Some(col), outline)
            }
            Part::Ellipse { c, rx, ry, color: col, outline } => {
                let (cx, cy) = at(c);
                let rect = tiny_skia::Rect::from_xywh(cx - rx * scale, cy - ry * scale, 2.0 * rx * scale, 2.0 * ry * scale);
                (rect.and_then(PathBuilder::from_oval), Some(col), outline)
            }
            Part::Line { a, b } => {
                let mut pb = PathBuilder::new();
                let ((ax, ay), (bx, by)) = (at(a), at(b));
                pb.move_to(ax, ay);
                pb.line_to(bx, by);
                (pb.finish(), None, false)
            }
        };
        let Some(path) = path else { continue };
        match fill {
            Some(col) => {
                paint.set_color(color(col));
                pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
                if outline {
                    paint.set_color(color(INK_HEX));
                    pm.stroke_path(&path, &paint, &ink_stroke(OUTLINE_W * scale), Transform::identity(), None);
                }
            }
            None => {
                paint.set_color(color(INK_HEX));
                pm.stroke_path(&path, &paint, &ink_stroke(LINE_W * scale), Transform::identity(), None);
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
        // Stirn des Bären: weiß
        assert!(px(128, 100).iter().take(3).all(|&c| c > 230), "{:?}", px(128, 100));
        // Scholle unten links: blau
        let c = px(40, 210);
        assert!(c[2] > c[0], "{c:?}");
        // ganz außen transparent (abgerundete Ecke)
        assert_eq!(px(1, 1)[3], 0);
        if let Some(path) = std::env::var_os("SLIDEBEAR_ICON_OUT") {
            image::RgbaImage::from_raw(256, 256, icon.rgba.clone()).unwrap().save(path).unwrap();
        }
    }

    #[test]
    fn fredoka_has_weight_axis() {
        let defs = fonts();
        assert!(defs.font_data.contains_key("fredoka"));
        assert!(defs.families[&bold()].first().is_some_and(|f| f == "fredoka-bold"));
    }
}

#[cfg(test)]
mod glyph_tests {
    use super::*;

    /// Jedes Sonderzeichen in den Texten der Oberfläche muss in einer der eingebauten Schriften
    /// (Fredoka + egui-Standardschriften) vorkommen, sonst erscheint ein leeres Kästchen.
    #[test]
    fn all_ui_symbols_have_glyphs() {
        // Nur Schriften der Familien, die die Oberfläche für Text nutzt (nicht Monospace)
        let defs = fonts();
        let used: std::collections::BTreeSet<&String> =
            [FontFamily::Proportional, bold()].iter().flat_map(|f| defs.families[f].iter()).collect();
        let faces: Vec<Vec<u8>> = used.iter().map(|n| defs.font_data[*n].font.to_vec()).collect();
        let covered = |c: char| faces.iter().any(|data| ttf_parser::Face::parse(data, 0).is_ok_and(|f| f.glyph_index(c).is_some()));

        let mut missing = Vec::new();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let src = std::fs::read_to_string(&path).unwrap();
            for (n, line) in src.lines().enumerate() {
                let code = line.trim_start();
                if code.starts_with("//") {
                    continue;
                }
                // nur Text in String-Literalen prüfen
                for (i, part) in code.split('"').enumerate() {
                    if i % 2 == 0 {
                        continue;
                    }
                    for c in part.chars().filter(|c| !c.is_ascii() && !c.is_whitespace()) {
                        if !covered(c) {
                            missing.push(format!("{}:{} {c:?} (U+{:04X})", path.file_name().unwrap().to_string_lossy(), n + 1, c as u32));
                        }
                    }
                }
            }
        }
        assert!(missing.is_empty(), "Zeichen ohne Glyphe:\n{}", missing.join("\n"));
    }
}
