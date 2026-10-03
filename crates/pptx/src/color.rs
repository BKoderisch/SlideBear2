//! Farben aus DrawingML: `srgbClr`, `schemeClr` (über das Theme), `sysClr`, `prstClr`
//! inklusive der gängigen Modifikatoren (`alpha`, `lumMod`, `lumOff`, `shade`, `tint`).

use std::collections::HashMap;

use roxmltree::Node;
use slidebear_core::Color;

use crate::xml::{attr, child, children};

#[derive(Debug, Default, Clone)]
pub struct Theme {
    pub colors: HashMap<String, Color>,
    pub major_font: Option<String>,
    pub minor_font: Option<String>,
}

impl Theme {
    pub fn parse(xml: &str) -> Self {
        let mut theme = Theme::default();
        let Ok(doc) = roxmltree::Document::parse(xml) else { return theme };
        let root = doc.root_element();
        if let Some(scheme) = root.descendants().find(|n| n.tag_name().name() == "clrScheme") {
            for c in scheme.children().filter(Node::is_element) {
                let color = child(c, "srgbClr")
                    .and_then(|n| attr(n, "val"))
                    .or_else(|| child(c, "sysClr").and_then(|n| attr(n, "lastClr")))
                    .and_then(Color::from_hex);
                if let Some(color) = color {
                    theme.colors.insert(c.tag_name().name().to_string(), color);
                }
            }
        }
        let font = |kind: &str| {
            root.descendants()
                .find(|n| n.tag_name().name() == kind)
                .and_then(|n| child(n, "latin"))
                .and_then(|n| attr(n, "typeface"))
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        theme.major_font = font("majorFont");
        theme.minor_font = font("minorFont");
        theme
    }

    fn scheme(&self, name: &str) -> Option<Color> {
        let key = match name {
            "bg1" => "lt1",
            "tx1" => "dk1",
            "bg2" => "lt2",
            "tx2" => "dk2",
            other => other,
        };
        self.colors.get(key).copied()
    }
}

/// Liest die Farbe aus einem Container wie `a:solidFill`.
pub fn parse_fill_color(container: Node, theme: &Theme) -> Option<Color> {
    let n = container.children().find(|n| matches!(n.tag_name().name(), "srgbClr" | "schemeClr" | "sysClr" | "prstClr" | "scrgbClr"))?;
    let base = match n.tag_name().name() {
        "srgbClr" => attr(n, "val").and_then(Color::from_hex),
        "schemeClr" => attr(n, "val").and_then(|v| theme.scheme(v)),
        "sysClr" => attr(n, "lastClr").and_then(Color::from_hex),
        "prstClr" => attr(n, "val").and_then(preset),
        "scrgbClr" => {
            let pct = |k| attr(n, k).and_then(|v| v.parse::<f32>().ok()).map(|v| (v / 100_000.0 * 255.0).round() as u8);
            Some(Color::rgb(pct("r")?, pct("g")?, pct("b")?))
        }
        _ => None,
    }?;
    Some(apply_modifiers(base, n))
}

fn preset(name: &str) -> Option<Color> {
    Some(match name {
        "black" => Color::BLACK,
        "white" => Color::WHITE,
        "red" => Color::rgb(255, 0, 0),
        "green" => Color::rgb(0, 128, 0),
        "blue" => Color::rgb(0, 0, 255),
        "yellow" => Color::rgb(255, 255, 0),
        "gray" | "grey" => Color::rgb(128, 128, 128),
        "orange" => Color::rgb(255, 165, 0),
        _ => return None,
    })
}

fn apply_modifiers(mut c: Color, n: Node) -> Color {
    let val = |m: Node| attr(m, "val").and_then(|v| v.parse::<f32>().ok()).map(|v| v / 100_000.0);
    let (mut h, mut s, mut l) = to_hsl(c);
    let mut hsl_dirty = false;
    for m in children(n) {
        let Some(v) = val(m) else { continue };
        match m.tag_name().name() {
            "alpha" => c.a = (v.clamp(0.0, 1.0) * 255.0).round() as u8,
            "lumMod" => {
                l *= v;
                hsl_dirty = true;
            }
            "lumOff" => {
                l += v;
                hsl_dirty = true;
            }
            "satMod" => {
                s *= v;
                hsl_dirty = true;
            }
            "shade" => {
                let (r, g, b) = (c.r as f32 * v, c.g as f32 * v, c.b as f32 * v);
                c = Color::rgba(r as u8, g as u8, b as u8, c.a);
                (h, s, l) = to_hsl(c);
            }
            "tint" => {
                let t = |x: u8| (x as f32 + (255.0 - x as f32) * (1.0 - v)) as u8;
                c = Color::rgba(t(c.r), t(c.g), t(c.b), c.a);
                (h, s, l) = to_hsl(c);
            }
            _ => {}
        }
    }
    if hsl_dirty {
        let a = c.a;
        c = from_hsl(h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
        c.a = a;
    }
    c
}

fn to_hsl(c: Color) -> (f32, f32, f32) {
    let (r, g, b) = (c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn from_hsl(h: f32, s: f32, l: f32) -> Color {
    if s == 0.0 {
        let v = (l * 255.0).round() as u8;
        return Color::rgb(v, v, v);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue = |mut t: f32| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (v * 255.0).round() as u8
    };
    Color::rgb(hue(h + 1.0 / 3.0), hue(h), hue(h - 1.0 / 3.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsl_roundtrip() {
        for c in [Color::rgb(255, 128, 0), Color::rgb(10, 20, 30), Color::rgb(200, 200, 200)] {
            let (h, s, l) = to_hsl(c);
            let back = from_hsl(h, s, l);
            assert!((back.r as i16 - c.r as i16).abs() <= 1);
            assert!((back.g as i16 - c.g as i16).abs() <= 1);
            assert!((back.b as i16 - c.b as i16).abs() <= 1);
        }
    }

    #[test]
    fn scheme_with_alpha() {
        let theme = Theme { colors: [("dk1".to_string(), Color::BLACK)].into(), ..Theme::default() };
        let xml = r#"<a:solidFill xmlns:a="x"><a:schemeClr val="tx1"><a:alpha val="50000"/></a:schemeClr></a:solidFill>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        assert_eq!(parse_fill_color(doc.root_element(), &theme), Some(Color::rgba(0, 0, 0, 128)));
    }
}
