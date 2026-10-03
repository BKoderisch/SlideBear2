//! Gerenderte Slides als egui-Texturen, gecacht nach Inhalt.

use std::collections::HashMap;

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use slidebear_core::assets::content_hash;
use slidebear_core::{placeholder, EventFields, SlideRef};
use slidebear_render::{to_rgba, Renderer};

/// Obergrenze für gecachte Texturen (Pixel), danach wird der Cache geleert.
const MAX_PIXELS: usize = 60_000_000;

#[derive(Default)]
pub struct Previews {
    cache: HashMap<u64, TextureHandle>,
    pixels: usize,
}

/// Eine einzelne, laufend aktualisierte Textur (z. B. die Editor-Leinwand).
#[derive(Default)]
pub struct LiveTexture {
    key: u64,
    tex: Option<TextureHandle>,
}

fn key_for(slide: SlideRef, fields: &EventFields, scale: f32) -> u64 {
    let mut buf = serde_json::to_vec(slide.scene).unwrap_or_default();
    buf.extend(serde_json::to_vec(fields).unwrap_or_default());
    buf.extend(serde_json::to_vec(&(slide.date_style, slide.time_style)).unwrap_or_default());
    buf.extend(scale.to_le_bytes());
    content_hash(&buf)
}

fn render(renderer: &mut Renderer, slide: SlideRef, fields: &EventFields, scale: f32) -> ColorImage {
    let pm = renderer.render_scaled(slide.scene, scale, &|s| placeholder::resolve(s, fields, slide));
    ColorImage::from_rgba_unmultiplied([pm.width() as usize, pm.height() as usize], &to_rgba(&pm))
}

/// Skalierung in Stufen, damit Fenster-Resize nicht ständig neu rendert.
fn quantize(slide: SlideRef, width_px: f32) -> f32 {
    let scale = (width_px / slide.scene.width as f32).clamp(0.05, 1.0);
    (scale * 20.0).ceil() / 20.0
}

impl Previews {
    /// Textur für Slide + Termindaten in der gewünschten Breite (in Pixeln).
    pub fn get(&mut self, ctx: &egui::Context, renderer: &mut Renderer, slide: SlideRef, fields: &EventFields, width_px: f32) -> TextureHandle {
        let scale = quantize(slide, width_px);
        let key = key_for(slide, fields, scale);
        if let Some(t) = self.cache.get(&key) {
            return t.clone();
        }
        let img = render(renderer, slide, fields, scale);
        let px = img.width() * img.height();
        if self.pixels + px > MAX_PIXELS {
            self.cache.clear();
            self.pixels = 0;
        }
        let tex = ctx.load_texture(format!("slide-{key:x}"), img, TextureOptions::LINEAR);
        self.cache.insert(key, tex.clone());
        self.pixels += px;
        tex
    }

    /// Rendert nur neu, wenn sich der Inhalt geändert hat, und überschreibt dieselbe Textur.
    pub fn live(
        live: &mut LiveTexture,
        ctx: &egui::Context,
        renderer: &mut Renderer,
        slide: SlideRef,
        fields: &EventFields,
        width_px: f32,
    ) -> TextureHandle {
        let scale = quantize(slide, width_px);
        let key = key_for(slide, fields, scale);
        match &mut live.tex {
            Some(tex) if live.key == key => tex.clone(),
            Some(tex) => {
                tex.set(render(renderer, slide, fields, scale), TextureOptions::LINEAR);
                live.key = key;
                tex.clone()
            }
            None => {
                let tex = ctx.load_texture("editor-canvas", render(renderer, slide, fields, scale), TextureOptions::LINEAR);
                live.key = key;
                live.tex = Some(tex.clone());
                tex
            }
        }
    }
}
