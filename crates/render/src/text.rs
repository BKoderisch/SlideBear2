//! Textsatz mit cosmic-text: Schriften, Umbruch, Auto-Verkleinern, Schatten und Umriss.

use cosmic_text::{Align, Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, SwashCache, Weight, Wrap};
use slidebear_core::scene::{Color, HAlign, Rect, TextStyle, VAlign};
use tiny_skia::Pixmap;

use crate::blur::blur;

pub const DEFAULT_FAMILY: &str = "Roboto";

const BUNDLED: [&[u8]; 7] = [
    include_bytes!("../../../assets/fonts/Roboto-Light.ttf"),
    include_bytes!("../../../assets/fonts/Roboto-LightItalic.ttf"),
    include_bytes!("../../../assets/fonts/Roboto-Regular.ttf"),
    include_bytes!("../../../assets/fonts/Roboto-Italic.ttf"),
    include_bytes!("../../../assets/fonts/Roboto-Medium.ttf"),
    include_bytes!("../../../assets/fonts/Roboto-Bold.ttf"),
    include_bytes!("../../../assets/fonts/Roboto-BoldItalic.ttf"),
];

pub struct FontLibrary {
    fs: FontSystem,
    cache: SwashCache,
}

/// Ergebnis des Textsatzes, z. B. für die Editor-Anzeige der tatsächlichen Schriftgröße.
pub struct TextLayout {
    pub size_px: f32,
    pub height: f32,
    pub width: f32,
}

impl Default for FontLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl FontLibrary {
    /// Lädt die Systemschriften und die mitgelieferte Roboto.
    pub fn new() -> Self {
        let mut fs = FontSystem::new();
        for data in BUNDLED {
            fs.db_mut().load_font_data(data.to_vec());
        }
        Self { fs, cache: SwashCache::new() }
    }

    /// Lädt eine zusätzliche Schriftdatei (z. B. aus einer importierten PPTX).
    pub fn load_font_file(&mut self, data: Vec<u8>) {
        self.fs.db_mut().load_font_data(data);
    }

    pub fn families(&self) -> Vec<String> {
        let mut names: Vec<String> =
            self.fs.db().faces().filter_map(|f| f.families.first().map(|(n, _)| n.clone())).filter(|n| !n.starts_with('.')).collect();
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        names
    }

    pub fn has_family(&self, name: &str) -> bool {
        self.fs.db().faces().any(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)))
    }

    fn effective_family<'a>(&self, name: &'a str) -> &'a str {
        if self.has_family(name) { name } else { DEFAULT_FAMILY }
    }

    fn shape(&mut self, text: &str, s: &TextStyle, size: f32, width: f32, wrap: Wrap) -> Buffer {
        let family = self.effective_family(&s.font_family);
        let attrs =
            Attrs::new().family(Family::Name(family)).weight(Weight(s.weight)).style(if s.italic { Style::Italic } else { Style::Normal });
        let mut buf = Buffer::new(&mut self.fs, Metrics::new(size, size * s.line_height.max(0.5)));
        buf.set_wrap(&mut self.fs, wrap);
        buf.set_size(&mut self.fs, Some(width.max(1.0)), None);
        buf.set_text(&mut self.fs, text, &attrs, Shaping::Advanced);
        let align = match s.align_h {
            HAlign::Left => Align::Left,
            HAlign::Center => Align::Center,
            HAlign::Right => Align::Right,
        };
        for line in buf.lines.iter_mut() {
            line.set_align(Some(align));
        }
        buf.shape_until_scroll(&mut self.fs, false);
        buf
    }

    /// Setzt den Text in den Rahmen; mit `auto_shrink` wird die Schrift verkleinert, bis alles passt.
    fn fit(&mut self, text: &str, s: &TextStyle, fw: f32, fh: f32, scale: f32) -> (Buffer, TextLayout) {
        let mut size = (s.size_px * scale).max(1.0);
        let min = (6.0 * scale).max(1.0);
        loop {
            let wrap = if s.auto_shrink { Wrap::Word } else { Wrap::WordOrGlyph };
            let buf = self.shape(text, s, size, fw, wrap);
            let (width, height) = measure(&buf);
            let fits = height <= fh + 0.5 && width <= fw + 0.5;
            if !s.auto_shrink || fits || size <= min {
                return (buf, TextLayout { size_px: size / scale, height, width });
            }
            size = (size * 0.94).max(min);
        }
    }

    /// Nur Layout berechnen (ohne Zeichnen), in Slide-Pixeln.
    pub fn layout(&mut self, frame: Rect, s: &TextStyle, text: &str) -> TextLayout {
        self.fit(text, s, frame.w, frame.h, 1.0).1
    }

    pub fn draw_text(&mut self, pm: &mut Pixmap, frame: Rect, opacity: f32, s: &TextStyle, text: &str, scale: f32) {
        if text.trim().is_empty() {
            return;
        }
        let (fw, fh) = (frame.w * scale, frame.h * scale);
        let (buf, layout) = self.fit(text, s, fw, fh, scale);
        let y_off = match s.align_v {
            VAlign::Top => 0.0,
            VAlign::Middle => (fh - layout.height) / 2.0,
            VAlign::Bottom => fh - layout.height,
        };

        let outline_w = s.outline.as_ref().map_or(0.0, |o| o.width * scale);
        let shadow_ext = s.shadow.as_ref().map_or(0.0, |sh| sh.blur * scale * 3.0 + sh.offset_x.abs().max(sh.offset_y.abs()) * scale);
        let pad = (layout.size_px * scale * 0.3 + outline_w + shadow_ext).ceil() as i32 + 2;

        let top = y_off.min(0.0);
        let mask_w = (fw.ceil() as i32 + 2 * pad).max(1) as usize;
        let mask_h = ((fh.max(y_off + layout.height) - top).ceil() as i32 + 2 * pad).max(1) as usize;
        let origin_x = (frame.x * scale).round() as i32 - pad;
        let origin_y = (frame.y * scale + top).round() as i32 - pad;
        let glyph_dy = (y_off - top).round() as i32 + pad;

        let mut mask = vec![0u8; mask_w * mask_h];
        buf.draw(&mut self.fs, &mut self.cache, cosmic_text::Color::rgb(255, 255, 255), |x, y, _, _, c| {
            let (mx, my) = (x + pad, y + glyph_dy);
            if mx >= 0 && my >= 0 && (mx as usize) < mask_w && (my as usize) < mask_h {
                let i = my as usize * mask_w + mx as usize;
                mask[i] = mask[i].max(c.a());
            }
        });

        if let Some(sh) = &s.shadow {
            let mut m = mask.clone();
            if outline_w > 0.0 {
                dilate(&mut m, mask_w, mask_h, outline_w);
            }
            blur(&mut m, mask_w, mask_h, 1, sh.blur * scale);
            let dx = (sh.offset_x * scale).round() as i32;
            let dy = (sh.offset_y * scale).round() as i32;
            composite(pm, &m, mask_w, origin_x + dx, origin_y + dy, sh.color, opacity);
        }
        if let Some(o) = &s.outline {
            let mut m = mask.clone();
            dilate(&mut m, mask_w, mask_h, outline_w);
            composite(pm, &m, mask_w, origin_x, origin_y, o.color, opacity);
        }
        composite(pm, &mask, mask_w, origin_x, origin_y, s.color, opacity);
    }
}

fn measure(buf: &Buffer) -> (f32, f32) {
    let mut width: f32 = 0.0;
    let mut height: f32 = 0.0;
    for run in buf.layout_runs() {
        width = width.max(run.line_w);
        height = height.max(run.line_top + run.line_height);
    }
    (width, height)
}

/// Vergrößert die Maske um `radius` Pixel (Maximum-Filter, getrennt in x und y).
fn dilate(mask: &mut [u8], w: usize, h: usize, radius: f32) {
    let r = radius.round() as isize;
    if r <= 0 {
        return;
    }
    let mut tmp = vec![0u8; mask.len()];
    for y in 0..h {
        for x in 0..w {
            let lo = (x as isize - r).max(0) as usize;
            let hi = ((x as isize + r) as usize).min(w - 1);
            tmp[y * w + x] = mask[y * w + lo..=y * w + hi].iter().copied().max().unwrap_or(0);
        }
    }
    for x in 0..w {
        for y in 0..h {
            let lo = (y as isize - r).max(0) as usize;
            let hi = ((y as isize + r) as usize).min(h - 1);
            mask[y * w + x] = (lo..=hi).map(|yy| tmp[yy * w + x]).max().unwrap_or(0);
        }
    }
}

/// Malt `color` mit der Maske als Deckkraft auf das Bild (source-over, vormultipliziert).
fn composite(pm: &mut Pixmap, mask: &[u8], mask_w: usize, ox: i32, oy: i32, color: Color, opacity: f32) {
    let (pw, ph) = (pm.width() as i32, pm.height() as i32);
    let base = color.a as f32 / 255.0 * opacity.clamp(0.0, 1.0);
    let data = pm.data_mut();
    for (i, &m) in mask.iter().enumerate() {
        if m == 0 {
            continue;
        }
        let x = ox + (i % mask_w) as i32;
        let y = oy + (i / mask_w) as i32;
        if x < 0 || y < 0 || x >= pw || y >= ph {
            continue;
        }
        let a = m as f32 / 255.0 * base;
        let o = (y * pw + x) as usize * 4;
        let src = [color.r as f32 * a, color.g as f32 * a, color.b as f32 * a, 255.0 * a];
        for c in 0..4 {
            data[o + c] = (src[c] + data[o + c] as f32 * (1.0 - a)).round().min(255.0) as u8;
        }
    }
}
