//! Platzhalter wie `{titel}` in Texten durch Termindaten ersetzen.

use crate::event::{EventFields, SlideRef};
use crate::format::{format_date, format_time, weekday_de};

pub const PLACEHOLDERS: [(&str, &str); 6] = [
    ("titel", "Titel"),
    ("datum", "Datum"),
    ("zeit", "Uhrzeit"),
    ("ort", "Ort"),
    ("untertitel", "Untertitel"),
    ("wochentag", "Wochentag"),
];

/// Ersetzt alle bekannten `{name}`-Platzhalter. Unbekannte bleiben unverändert stehen,
/// damit Tippfehler im Editor sichtbar sind.
pub fn resolve(text: &str, fields: &EventFields, slide: SlideRef) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let key = &after[..close];
                match value(key, fields, slide) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('{');
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    tidy_separators(&out)
}

fn value(key: &str, f: &EventFields, t: SlideRef) -> Option<String> {
    Some(match key {
        "titel" => f.title.clone(),
        "datum" => format_date(f, t.date_style),
        "zeit" => format_time(f, t.time_style),
        "ort" => f.location.clone(),
        "untertitel" => f.subtitle.clone(),
        "wochentag" => weekday_de(f.start.date()).to_string(),
        _ => return None,
    })
}

/// Entfernt leere Segmente in `a | | b` oder `a | ` wenn z. B. kein Ort gesetzt ist.
fn tidy_separators(s: &str) -> String {
    s.lines()
        .map(|line| {
            if !line.contains(" | ") && !line.trim_end().ends_with('|') {
                return line.to_string();
            }
            line.split('|').map(str::trim).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" | ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Template;
    use crate::scene::Scene;
    use chrono::NaiveDateTime;

    fn forum() -> EventFields {
        EventFields {
            title: "Gemeindeforum".into(),
            start: NaiveDateTime::parse_from_str("2025-09-17 19:30", "%Y-%m-%d %H:%M").unwrap(),
            end: None,
            all_day: false,
            location: "FeG Adlershof".into(),
            subtitle: String::new(),
        }
    }

    #[test]
    fn resolves_info_line() {
        let t = Template::new("t", Scene::default());
        assert_eq!(resolve("{datum} | {zeit} | {ort}", &forum(), t.as_slide_ref()), "17.09.2025 | 19:30 Uhr | FeG Adlershof");
    }

    #[test]
    fn drops_empty_segments() {
        let t = Template::new("t", Scene::default());
        let mut f = forum();
        f.location.clear();
        assert_eq!(resolve("{datum} | {zeit} | {ort}", &f, t.as_slide_ref()), "17.09.2025 | 19:30 Uhr");
    }

    #[test]
    fn keeps_unknown_and_unclosed() {
        let t = Template::new("t", Scene::default());
        assert_eq!(resolve("{foo} {titel} {", &forum(), t.as_slide_ref()), "{foo} Gemeindeforum {");
    }
}
