//! Rendert eine `Scene` zu einem Bild. Wird für Export (PNG) und Editor-Vorschau gleichermaßen
//! genutzt, damit die Vorschau exakt dem Export entspricht.

mod blur;
mod image_ops;
mod text;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use slidebear_core::scene::{Color, Element, ElementKind, ImageStyle, ShapeKind, ShapeStyle};
use slidebear_core::Scene;
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform};

pub use text::FontLibrary;
pub use tiny_skia::Pixmap as RenderedImage;

pub struct Renderer {
    assets_dir: PathBuf,
    fonts: FontLibrary,
    /// Fertig skalierte und gefilterte Bilder, Schlüssel aus Asset, Größe und Filtern.
    image_cache: HashMap<String, Option<image_ops::Placed>>,
}

impl Renderer {
    pub fn new(assets_dir: impl Into<PathBuf>) -> Self {
        Self { assets_dir: assets_dir.into(), fonts: FontLibrary::new(), image_cache: HashMap::new() }
    }

    pub fn assets_dir(&self) -> &Path {
        &self.assets_dir
    }

    pub fn fonts(&mut self) -> &mut FontLibrary {
        &mut self.fonts
    }

    pub fn font_library(&self) -> &FontLibrary {
        &self.fonts
    }

    /// Verwirft gecachte Bilder, z. B. nachdem ein Asset ersetzt wurde.
    pub fn clear_cache(&mut self) {
        self.image_cache.clear();
    }

    /// Rendert die Szene in voller Größe. `resolve` ersetzt Platzhalter in Texten.
    pub fn render(&mut self, scene: &Scene, resolve: &dyn Fn(&str) -> String) -> Pixmap {
        self.render_scaled(scene, 1.0, resolve)
    }

    /// Rendert verkleinert (z. B. 0.25 für Thumbnails). Layout bleibt identisch.
    pub fn render_scaled(&mut self, scene: &Scene, scale: f32, resolve: &dyn Fn(&str) -> String) -> Pixmap {
        let w = ((scene.width as f32 * scale).round() as u32).max(1);
        let h = ((scene.height as f32 * scale).round() as u32).max(1);
        let mut pixmap = Pixmap::new(w, h).expect("Slide-Größe muss > 0 sein");
        pixmap.fill(tiny_skia::Color::BLACK);
        for el in scene.elements.iter().filter(|e| e.visible && e.opacity > 0.0) {
            self.draw_element(&mut pixmap, el, scale, resolve);
        }
        pixmap
    }

    fn draw_element(&mut self, pm: &mut Pixmap, el: &Element, scale: f32, resolve: &dyn Fn(&str) -> String) {
        match &el.kind {
            ElementKind::Shape(s) => draw_shape(pm, el, s, scale),
            ElementKind::Image(s) => self.draw_image(pm, el, s, scale),
            ElementKind::Text(s) => {
                let text = resolve(&s.text);
                self.fonts.draw_text(pm, el.frame, el.opacity, s, &text, scale);
            }
        }
    }

    fn draw_image(&mut self, pm: &mut Pixmap, el: &Element, style: &ImageStyle, scale: f32) {
        let fw = (el.frame.w * scale).round().max(1.0) as u32;
        let fh = (el.frame.h * scale).round().max(1.0) as u32;
        let key = format!("{}|{fw}x{fh}|{:?}|{:?}|{scale}", style.asset, style.fit, style.filters);
        let placed = self.image_cache.entry(key).or_insert_with(|| {
            image_ops::prepare(&self.assets_dir.join(&style.asset), fw, fh, style.fit, &style.filters, scale)
        });
        let x = (el.frame.x * scale).round() as i32;
        let y = (el.frame.y * scale).round() as i32;
        match placed {
            Some(p) => {
                let paint = PixmapPaint { opacity: el.opacity.clamp(0.0, 1.0), ..PixmapPaint::default() };
                pm.draw_pixmap(x + p.offset_x, y + p.offset_y, p.pixmap.as_ref(), &paint, Transform::identity(), None);
            }
            None => draw_missing(pm, el, scale),
        }
    }
}

fn to_skia(c: Color, opacity: f32) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, (c.a as f32 * opacity.clamp(0.0, 1.0)).round() as u8)
}

fn shape_path(shape: ShapeKind, x: f32, y: f32, w: f32, h: f32, scale: f32) -> Option<tiny_skia::Path> {
    let rect = tiny_skia::Rect::from_xywh(x, y, w.max(0.5), h.max(0.5))?;
    match shape {
        ShapeKind::Rect => Some(PathBuilder::from_rect(rect)),
        ShapeKind::Ellipse => PathBuilder::from_oval(rect),
        ShapeKind::RoundedRect { radius } => {
            let r = (radius * scale).min(w / 2.0).min(h / 2.0).max(0.0);
            if r < 0.5 {
                return Some(PathBuilder::from_rect(rect));
            }
            // Kreisbogen-Näherung mit kubischen Bézierkurven
            let k = 0.552_284_8 * r;
            let (x2, y2) = (x + w, y + h);
            let mut pb = PathBuilder::new();
            pb.move_to(x + r, y);
            pb.line_to(x2 - r, y);
            pb.cubic_to(x2 - r + k, y, x2, y + r - k, x2, y + r);
            pb.line_to(x2, y2 - r);
            pb.cubic_to(x2, y2 - r + k, x2 - r + k, y2, x2 - r, y2);
            pb.line_to(x + r, y2);
            pb.cubic_to(x + r - k, y2, x, y2 - r + k, x, y2 - r);
            pb.line_to(x, y + r);
            pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
            pb.close();
            pb.finish()
        }
    }
}

fn draw_shape(pm: &mut Pixmap, el: &Element, s: &ShapeStyle, scale: f32) {
    let f = el.frame;
    let Some(path) = shape_path(s.shape, f.x * scale, f.y * scale, f.w * scale, f.h * scale, scale) else {
        return;
    };
    if let Some(fill) = s.fill {
        let mut paint = Paint::default();
        paint.set_color(to_skia(fill, el.opacity));
        paint.anti_alias = true;
        pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }
    if let Some(stroke) = &s.stroke {
        let mut paint = Paint::default();
        paint.set_color(to_skia(stroke.color, el.opacity));
        paint.anti_alias = true;
        let st = Stroke { width: (stroke.width * scale).max(0.5), ..Stroke::default() };
        pm.stroke_path(&path, &paint, &st, Transform::identity(), None);
    }
}

/// Platzhalter für fehlende Bilder: grauer Rahmen mit Kreuz.
fn draw_missing(pm: &mut Pixmap, el: &Element, scale: f32) {
    let f = el.frame;
    let (x, y, w, h) = (f.x * scale, f.y * scale, f.w * scale, f.h * scale);
    let Some(rect) = tiny_skia::Rect::from_xywh(x, y, w.max(1.0), h.max(1.0)) else { return };
    let mut paint = Paint::default();
    paint.set_color_rgba8(90, 90, 90, 160);
    pm.fill_rect(rect, &paint, Transform::identity(), None);
    let mut pb = PathBuilder::new();
    pb.move_to(x, y);
    pb.line_to(x + w, y + h);
    pb.move_to(x + w, y);
    pb.line_to(x, y + h);
    if let Some(p) = pb.finish() {
        paint.set_color_rgba8(200, 200, 200, 200);
        pm.stroke_path(&p, &paint, &Stroke { width: 2.0, ..Stroke::default() }, Transform::identity(), None);
    }
}

/// Wandelt das Ergebnis in nicht-vormultipliziertes RGBA (z. B. für egui oder `image`).
pub fn to_rgba(pm: &Pixmap) -> Vec<u8> {
    pm.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }).collect()
}

pub fn save_png(pm: &Pixmap, path: &Path) -> anyhow::Result<()> {
    let img = image::RgbaImage::from_raw(pm.width(), pm.height(), to_rgba(pm))
        .ok_or_else(|| anyhow::anyhow!("ungültige Bildgröße"))?;
    img.save(path)?;
    Ok(())
}
