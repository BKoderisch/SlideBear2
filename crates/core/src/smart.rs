//! Smart-Platzhalter: erkennt in importierten Slides Titel, Datum, Uhrzeit und Ort und schlägt
//! vor, sie durch `{titel}`, `{datum}`, `{zeit}`, `{ort}` zu ersetzen.

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;
use std::sync::LazyLock;
use uuid::Uuid;

use crate::event::EventFields;
use crate::scene::{ElementKind, Scene};

static DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d{1,2})\.(\d{1,2})\.(\d{4}|\d{2})\b").unwrap());
static TIME_RANGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(\d{1,2})(?::(\d{2}))?\s*(?:-|–|bis)\s*(\d{1,2})(?::(\d{2}))?\s*Uhr\b").unwrap()
});
static TIME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d{1,2})(?::(\d{2}))?\s*Uhr\b").unwrap());

#[derive(Debug, Clone, PartialEq)]
pub struct Replacement {
    pub element_id: Uuid,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Detection {
    pub replacements: Vec<Replacement>,
    pub title: Option<String>,
    pub date: Option<NaiveDate>,
    /// chrono-Muster passend zum gefundenen Datum, z. B. `%d.%m.%y`.
    pub date_pattern: Option<String>,
    pub start_time: Option<NaiveTime>,
    pub end_time: Option<NaiveTime>,
    pub location: Option<String>,
}

impl Detection {
    pub fn is_empty(&self) -> bool {
        self.replacements.is_empty()
    }

    /// Termindaten für „als Slide importieren“ (ohne Datum nicht möglich).
    pub fn to_fields(&self) -> Option<EventFields> {
        let date = self.date?;
        let start = NaiveDateTime::new(date, self.start_time.unwrap_or(NaiveTime::MIN));
        Some(EventFields {
            title: self.title.clone().unwrap_or_default(),
            start,
            end: self.end_time.map(|t| NaiveDateTime::new(date, t)),
            all_day: self.start_time.is_none(),
            location: self.location.clone().unwrap_or_default(),
            subtitle: String::new(),
        })
    }

    pub fn apply(&self, scene: &mut Scene) {
        for r in &self.replacements {
            if let Some(el) = scene.element_mut(r.element_id)
                && let ElementKind::Text(t) = &mut el.kind {
                    t.text = r.after.clone();
                }
        }
    }
}

fn hm(h: &str, m: Option<regex::Match>) -> Option<NaiveTime> {
    NaiveTime::from_hms_opt(h.parse().ok()?, m.map_or(Some(0), |m| m.as_str().parse().ok())?, 0)
}

pub fn detect(scene: &Scene) -> Detection {
    let mut d = Detection::default();
    let texts: Vec<(Uuid, String, f32)> = scene
        .elements
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Text(t) if !t.text.trim().is_empty() && !t.text.contains('{') => Some((e.id, t.text.clone(), t.size_px)),
            _ => None,
        })
        .collect();

    let mut date_or_time_ids = Vec::new();
    for (id, text, _) in &texts {
        let mut new = text.clone();

        if let Some(c) = DATE.captures(&new) {
            let (day, month, year) = (&c[1], &c[2], &c[3]);
            let full_year = if year.len() == 2 { format!("20{year}") } else { year.to_string() };
            if let (Ok(dd), Ok(mm), Ok(yy)) = (day.parse(), month.parse(), full_year.parse())
                && let Some(date) = NaiveDate::from_ymd_opt(yy, mm, dd) {
                    d.date.get_or_insert(date);
                    let dpat = if day.len() == 1 { "%-d" } else { "%d" };
                    let mpat = if month.len() == 1 { "%-m" } else { "%m" };
                    let ypat = if year.len() == 2 { "%y" } else { "%Y" };
                    d.date_pattern.get_or_insert(format!("{dpat}.{mpat}.{ypat}"));
                    new = new.replacen(&c[0], "{datum}", 1);
                }
        }

        if let Some(c) = TIME_RANGE.captures(&new) {
            d.start_time = d.start_time.or(hm(&c[1], c.get(2)));
            d.end_time = d.end_time.or(hm(&c[3], c.get(4)));
            let whole = c[0].to_string();
            new = new.replacen(&whole, "{zeit}", 1);
        } else if let Some(c) = TIME.captures(&new) {
            d.start_time = d.start_time.or(hm(&c[1], c.get(2)));
            let whole = c[0].to_string();
            new = new.replacen(&whole, "{zeit}", 1);
        }

        // Ort: ein freies Segment nach `{zeit}` in einer `|`-getrennten Infozeile
        if new.contains("{zeit}") && new.contains('|') {
            let parts: Vec<String> = new.split('|').map(|p| p.trim().to_string()).collect();
            if let Some(pos) = parts.iter().position(|p| p == "{zeit}")
                && let Some(loc) = parts.get(pos + 1).filter(|p| !p.is_empty() && !p.contains('{')) {
                    d.location = Some(loc.clone());
                    let mut parts = parts.clone();
                    parts[pos + 1] = "{ort}".into();
                    new = parts.join(" | ");
                }
        }

        if new != *text {
            date_or_time_ids.push(*id);
            d.replacements.push(Replacement { element_id: *id, before: text.clone(), after: new });
        }
    }

    // Titel: größter Text, der kein Datum/keine Zeit enthält
    if let Some((id, text, _)) = texts
        .iter()
        .filter(|(id, _, _)| !date_or_time_ids.contains(id))
        .max_by(|a, b| a.2.total_cmp(&b.2))
    {
        d.title = Some(text.trim().to_string());
        d.replacements.push(Replacement { element_id: *id, before: text.clone(), after: "{titel}".into() });
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::text;
    use crate::scene::Rect;

    fn scene(lines: &[(&str, f32)]) -> Scene {
        let elements = lines
            .iter()
            .enumerate()
            .map(|(i, (t, size))| text("t", Rect::new(0.0, i as f32 * 200.0, 1800.0, 150.0), t, *size, 400))
            .collect();
        Scene { elements, ..Scene::default() }
    }

    fn after(d: &Detection, before: &str) -> String {
        d.replacements.iter().find(|r| r.before == before).map(|r| r.after.clone()).unwrap_or_default()
    }

    #[test]
    fn adventsmarkt() {
        let s = scene(&[("Adventsmarkt", 200.0), ("07.12.2024 | 15-19 Uhr", 90.0), ("Fleißige Helfer gesucht!", 70.0)]);
        let d = detect(&s);
        assert_eq!(after(&d, "Adventsmarkt"), "{titel}");
        assert_eq!(after(&d, "07.12.2024 | 15-19 Uhr"), "{datum} | {zeit}");
        assert_eq!(d.date_pattern.as_deref(), Some("%d.%m.%Y"));
        assert_eq!(d.start_time, NaiveTime::from_hms_opt(15, 0, 0));
        assert_eq!(d.end_time, NaiveTime::from_hms_opt(19, 0, 0));
        // Untertitel bleibt wörtlich stehen
        assert_eq!(after(&d, "Fleißige Helfer gesucht!"), "");
    }

    #[test]
    fn herbstputz_short_year() {
        let d = detect(&scene(&[("Herbstputz", 200.0), ("12.10.24 | 10 Uhr", 90.0)]));
        assert_eq!(d.date, NaiveDate::from_ymd_opt(2024, 10, 12));
        assert_eq!(d.date_pattern.as_deref(), Some("%d.%m.%y"));
        assert_eq!(after(&d, "12.10.24 | 10 Uhr"), "{datum} | {zeit}");
    }

    #[test]
    fn gemeindeforum_location() {
        let d = detect(&scene(&[("Gemeindeforum", 200.0), ("17.09.2025 | 19:30 Uhr | FeG Adlershof", 90.0)]));
        assert_eq!(after(&d, "17.09.2025 | 19:30 Uhr | FeG Adlershof"), "{datum} | {zeit} | {ort}");
        assert_eq!(d.location.as_deref(), Some("FeG Adlershof"));
        let f = d.to_fields().unwrap();
        assert_eq!(f.title, "Gemeindeforum");
        assert_eq!(f.start.to_string(), "2025-09-17 19:30:00");
    }
}
