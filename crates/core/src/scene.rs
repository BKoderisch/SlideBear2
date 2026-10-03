//! Szenen-Modell: gemeinsame Grundlage für Editor, Renderer und PPTX-Import.
//! Alle Koordinaten sind Slide-Pixel (Standard 1920x1080).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const DEFAULT_WIDTH: u32 = 1920;
pub const DEFAULT_HEIGHT: u32 = 1080;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    pub const BLACK: Color = Color::rgb(0, 0, 0);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parst `RRGGBB` oder `RRGGBBAA`, optional mit führendem `#`.
    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim_start_matches('#');
        let byte = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
        match s.len() {
            6 => Some(Self::rgb(byte(0)?, byte(2)?, byte(4)?)),
            8 => Some(Self::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub width: u32,
    pub height: u32,
    /// Ebenen-Reihenfolge: Index 0 liegt ganz unten.
    pub elements: Vec<Element>,
}

impl Default for Scene {
    fn default() -> Self {
        Self { width: DEFAULT_WIDTH, height: DEFAULT_HEIGHT, elements: Vec::new() }
    }
}

impl Scene {
    pub fn element(&self, id: Uuid) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == id)
    }

    pub fn element_mut(&mut self, id: Uuid) -> Option<&mut Element> {
        self.elements.iter_mut().find(|e| e.id == id)
    }

    pub fn index_of(&self, id: Uuid) -> Option<usize> {
        self.elements.iter().position(|e| e.id == id)
    }

    /// Passt die Szene an ein anderes Format an (z. B. 4:3 → 16:9): Inhalte werden gleichmäßig
    /// skaliert und zentriert, Elemente über die ganze Fläche (Hintergründe) füllen das neue Format.
    pub fn refit(&mut self, width: u32, height: u32) {
        let (ow, oh) = (self.width as f32, self.height as f32);
        let (nw, nh) = (width as f32, height as f32);
        let s = (nw / ow).min(nh / oh);
        let (dx, dy) = ((nw - ow * s) / 2.0, (nh - oh * s) / 2.0);
        for e in &mut self.elements {
            let f = e.frame;
            let full = f.x <= 0.5 && f.y <= 0.5 && f.w >= ow - 0.5 && f.h >= oh - 0.5;
            e.frame = if full { Rect::new(0.0, 0.0, nw, nh) } else { Rect::new(f.x * s + dx, f.y * s + dy, f.w * s, f.h * s) };
            match &mut e.kind {
                ElementKind::Text(t) => {
                    t.size_px *= s;
                    if let Some(sh) = &mut t.shadow {
                        sh.offset_x *= s;
                        sh.offset_y *= s;
                        sh.blur *= s;
                    }
                    if let Some(o) = &mut t.outline {
                        o.width *= s;
                    }
                }
                ElementKind::Shape(sh) => {
                    if let ShapeKind::RoundedRect { radius } = &mut sh.shape {
                        *radius *= s;
                    }
                    if let Some(st) = &mut sh.stroke {
                        st.width *= s;
                    }
                }
                ElementKind::Image(_) => {}
            }
        }
        self.width = width;
        self.height = height;
    }

    /// Oberstes sichtbares, nicht gesperrtes Element unter dem Punkt.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<Uuid> {
        self.elements
            .iter()
            .rev()
            .find(|e| e.visible && !e.locked && e.frame.contains(x, y))
            .map(|e| e.id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub id: Uuid,
    pub name: String,
    pub frame: Rect,
    pub opacity: f32,
    pub locked: bool,
    pub visible: bool,
    pub kind: ElementKind,
}

impl Element {
    pub fn new(name: impl Into<String>, frame: Rect, kind: ElementKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            frame,
            opacity: 1.0,
            locked: false,
            visible: true,
            kind,
        }
    }

    /// Kopie mit neuer ID (für Duplizieren und Vorlage → Slide).
    pub fn duplicate(&self) -> Self {
        Self { id: Uuid::new_v4(), ..self.clone() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ElementKind {
    Text(TextStyle),
    Image(ImageStyle),
    Shape(ShapeStyle),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shadow {
    pub color: Color,
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outline {
    pub color: Color,
    pub width: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    /// Text mit Platzhaltern wie `{titel}` oder `{datum} | {zeit}`.
    pub text: String,
    pub font_family: String,
    /// CSS-Gewicht: 300 = Light, 400 = Regular, 700 = Bold.
    pub weight: u16,
    pub italic: bool,
    pub size_px: f32,
    pub color: Color,
    pub align_h: HAlign,
    pub align_v: VAlign,
    pub line_height: f32,
    /// Schrift verkleinern, bis der Text in den Rahmen passt.
    pub auto_shrink: bool,
    pub shadow: Option<Shadow>,
    pub outline: Option<Outline>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            text: "Text".into(),
            font_family: "Roboto".into(),
            weight: 400,
            italic: false,
            size_px: 72.0,
            color: Color::WHITE,
            align_h: HAlign::Center,
            align_v: VAlign::Middle,
            line_height: 1.15,
            auto_shrink: true,
            shadow: None,
            outline: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFit {
    /// Füllt den Rahmen, schneidet Überstand ab.
    Cover,
    /// Passt komplett hinein, ggf. mit Rand.
    Contain,
    Stretch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Filter {
    /// 0.0 = unverändert, 1.0 = schwarz.
    Darken { amount: f32 },
    Blur { radius: f32 },
    Tint { color: Color, amount: f32 },
    Grayscale,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageStyle {
    /// Dateiname im Asset-Ordner (content-adressiert, z. B. `ab12cd.png`).
    pub asset: String,
    pub fit: ImageFit,
    pub filters: Vec<Filter>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShapeKind {
    Rect,
    RoundedRect { radius: f32 },
    Ellipse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapeStyle {
    pub shape: ShapeKind,
    pub fill: Option<Color>,
    pub stroke: Option<Stroke>,
}

impl Default for ShapeStyle {
    fn default() -> Self {
        Self { shape: ShapeKind::Rect, fill: Some(Color::rgba(0, 0, 0, 128)), stroke: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parsing() {
        assert_eq!(Color::from_hex("#FF8000"), Some(Color::rgb(255, 128, 0)));
        assert_eq!(Color::from_hex("00000080"), Some(Color::rgba(0, 0, 0, 128)));
        assert_eq!(Color::from_hex("xyz"), None);
    }

    #[test]
    fn refit_4_3_to_16_9() {
        let mut scene = Scene { width: 1920, height: 1440, elements: Vec::new() };
        scene.elements.push(Element::new("bg", Rect::new(0.0, 0.0, 1920.0, 1440.0), ElementKind::Shape(ShapeStyle::default())));
        scene.elements.push(Element::new("t", Rect::new(0.0, 0.0, 1920.0, 720.0), ElementKind::Text(TextStyle { size_px: 100.0, ..TextStyle::default() })));
        scene.elements[1].frame.w = 960.0;
        scene.refit(1920, 1080);
        assert_eq!(scene.elements[0].frame, Rect::new(0.0, 0.0, 1920.0, 1080.0));
        // Faktor 0.75, horizontal zentriert: (1920 - 1440) / 2 = 240
        assert_eq!(scene.elements[1].frame, Rect::new(240.0, 0.0, 720.0, 540.0));
        let ElementKind::Text(t) = &scene.elements[1].kind else { panic!() };
        assert_eq!(t.size_px, 75.0);
    }

    #[test]
    fn hit_test_prefers_top_layer() {
        let mut scene = Scene::default();
        let bottom = Element::new("a", Rect::new(0.0, 0.0, 100.0, 100.0), ElementKind::Shape(ShapeStyle::default()));
        let top = Element::new("b", Rect::new(50.0, 50.0, 100.0, 100.0), ElementKind::Shape(ShapeStyle::default()));
        let (b, t) = (bottom.id, top.id);
        scene.elements = vec![bottom, top];
        assert_eq!(scene.hit_test(75.0, 75.0), Some(t));
        assert_eq!(scene.hit_test(10.0, 10.0), Some(b));
        assert_eq!(scene.hit_test(500.0, 500.0), None);
    }
}
