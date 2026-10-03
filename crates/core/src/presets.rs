//! Standard-Layouts, nachgebaut nach den bestehenden Veranstaltungs-Slides.

use crate::event::Template;
use crate::scene::{Color, Element, ElementKind, Filter, ImageFit, ImageStyle, Rect, Scene, TextStyle};

/// Foto-Hintergrund + zentrierter Titel, Infozeile und Untertitel.
pub fn event_scene(background_asset: Option<&str>) -> Scene {
    let mut elements = Vec::new();
    if let Some(asset) = background_asset {
        elements.push(background(asset, 0.3));
    }
    elements.push(text("Titel", Rect::new(60.0, 230.0, 1800.0, 320.0), "{titel}", 250.0, 400));
    elements.push(text("Infozeile", Rect::new(60.0, 560.0, 1800.0, 120.0), "{datum} | {zeit} | {ort}", 92.0, 300));
    elements.push(text("Untertitel", Rect::new(60.0, 700.0, 1800.0, 110.0), "{untertitel}", 72.0, 400));
    Scene { elements, ..Scene::default() }
}

pub fn event_template(name: &str, background_asset: Option<&str>) -> Template {
    Template::new(name, event_scene(background_asset))
}

pub fn background(asset: &str, darken: f32) -> Element {
    let mut filters = Vec::new();
    if darken > 0.0 {
        filters.push(Filter::Darken { amount: darken });
    }
    let mut e = Element::new(
        "Hintergrund",
        Rect::new(0.0, 0.0, crate::scene::DEFAULT_WIDTH as f32, crate::scene::DEFAULT_HEIGHT as f32),
        ElementKind::Image(ImageStyle { asset: asset.into(), fit: ImageFit::Cover, filters }),
    );
    e.locked = true;
    e
}

pub fn text(name: &str, frame: Rect, content: &str, size_px: f32, weight: u16) -> Element {
    Element::new(
        name,
        frame,
        ElementKind::Text(TextStyle { text: content.into(), size_px, weight, color: Color::WHITE, ..TextStyle::default() }),
    )
}
