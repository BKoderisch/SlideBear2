//! Welche Slides exportiert werden und unter welchem Dateinamen.

use chrono::{Duration, NaiveDate};

use crate::assets::content_hash;
use crate::event::{Event, EventFields, EventStatus, SeriesLink, Template};
use crate::scene::Scene;

/// Dateiname-tauglich: `Gemeindeforum Süd!` → `gemeindeforum-sued`.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().to_lowercase().chars() {
        match c {
            'ä' => out.push_str("ae"),
            'ö' => out.push_str("oe"),
            'ü' => out.push_str("ue"),
            'ß' => out.push_str("ss"),
            c if c.is_ascii_alphanumeric() => out.push(c),
            _ => {
                if !out.ends_with('-') {
                    out.push('-');
                }
            }
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { "slide".into() } else { out }
}

/// `2025-09-17_1930_gemeindeforum.png`: sortiert in ProPresenter automatisch chronologisch.
pub fn file_name(fields: &EventFields) -> String {
    let time = if fields.all_day { "0000".to_string() } else { fields.start.format("%H%M").to_string() };
    format!("{}_{}_{}.png", fields.start.format("%Y-%m-%d"), time, slug(&fields.title))
}

/// Szene, aus der die Slide gerendert wird: frei bearbeitete Kopie oder Vorlage.
pub fn scene_for<'a>(event: &'a Event, template: Option<&'a Template>) -> Option<&'a Scene> {
    event.custom_scene.as_ref().or(template.map(|t| &t.scene))
}

/// Fingerabdruck aller Eingaben eines Renders, um unveränderte Slides nicht neu zu schreiben.
pub fn render_key(scene: &Scene, fields: &EventFields, template: Option<&Template>) -> String {
    let mut buf = serde_json::to_vec(scene).unwrap_or_default();
    buf.extend(serde_json::to_vec(fields).unwrap_or_default());
    if let Some(t) = template {
        buf.extend(serde_json::to_vec(&(&t.date_style, &t.time_style)).unwrap_or_default());
    }
    format!("{:016x}", content_hash(&buf))
}

#[derive(Debug)]
pub struct Planned<'a> {
    pub event: &'a Event,
    pub template: Option<&'a Template>,
    pub fields: EventFields,
    pub file: String,
}

/// Lead-Zeit eines Termins: aus der Serien-Verknüpfung oder der globale Standard.
pub fn lead_days(event: &Event, links: &[SeriesLink], default_days: u32) -> u32 {
    SeriesLink::find(links, event).and_then(|l| l.lead_days).unwrap_or(default_days)
}

/// Alle Slides, die heute im Export-Ordner liegen sollen.
pub fn plan<'a>(
    events: &'a [Event],
    templates: &'a [Template],
    links: &[SeriesLink],
    today: NaiveDate,
    default_days: u32,
) -> Vec<Planned<'a>> {
    let mut out: Vec<Planned> = events
        .iter()
        .filter(|e| e.enabled && e.status == EventStatus::Active)
        .filter_map(|e| {
            let template = e.template_id.and_then(|id| templates.iter().find(|t| t.id == id));
            scene_for(e, template)?;
            let fields = e.fields();
            let last_day = fields.end.unwrap_or(fields.start).date().max(fields.start.date());
            let horizon = today + Duration::days(lead_days(e, links, default_days) as i64);
            if last_day < today || fields.start.date() > horizon {
                return None;
            }
            let file = file_name(&fields);
            Some(Planned { event: e, template, fields, file })
        })
        .collect();
    out.sort_by(|a, b| a.file.cmp(&b.file));
    // Gleiche Dateinamen (zwei Termine gleicher Titel zur gleichen Zeit) eindeutig machen
    for i in 1..out.len() {
        if out[i].file == out[i - 1].file || out[..i].iter().any(|p| p.file == out[i].file) {
            let stem = out[i].file.trim_end_matches(".png").to_string();
            out[i].file = format!("{stem}-{}.png", i + 1);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;
    use uuid::Uuid;

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    fn ev(title: &str, start: &str, tpl: Uuid) -> Event {
        Event::manual(
            EventFields { title: title.into(), start: dt(start), end: None, all_day: false, location: String::new(), subtitle: String::new() },
            Some(tpl),
        )
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Gemeindeforum Süd!"), "gemeindeforum-sued");
        assert_eq!(slug("  Fleißige   Helfer  "), "fleissige-helfer");
        assert_eq!(slug("!!!"), "slide");
    }

    #[test]
    fn file_names_sort_chronologically() {
        let e = ev("Gemeindeforum", "2025-09-17 19:30", Uuid::new_v4());
        assert_eq!(file_name(&e.fields()), "2025-09-17_1930_gemeindeforum.png");
    }

    #[test]
    fn plan_respects_window_and_flags() {
        let tpl = Template::new("t", Scene::default());
        let today = NaiveDate::from_ymd_opt(2026, 10, 4).unwrap();
        let mut disabled = ev("Aus", "2026-10-10 10:00", tpl.id);
        disabled.enabled = false;
        let no_template = ev("Ohne", "2026-10-10 10:00", Uuid::new_v4());
        let events = vec![
            ev("Vergangen", "2026-10-03 10:00", tpl.id),
            ev("Heute", "2026-10-04 18:00", tpl.id),
            ev("Bald", "2026-10-20 10:00", tpl.id),
            ev("Zu weit", "2026-12-20 10:00", tpl.id),
            disabled,
            no_template,
        ];
        let templates = [tpl];
        let p = plan(&events, &templates, &[], today, 28);
        let titles: Vec<_> = p.iter().map(|p| p.fields.title.as_str()).collect();
        assert_eq!(titles, ["Heute", "Bald"]);
    }

    #[test]
    fn duplicate_names_get_suffix() {
        let tpl = Template::new("t", Scene::default());
        let today = NaiveDate::from_ymd_opt(2026, 10, 4).unwrap();
        let events = vec![ev("X", "2026-10-10 10:00", tpl.id), ev("X", "2026-10-10 10:00", tpl.id)];
        let templates = [tpl];
        let p = plan(&events, &templates, &[], today, 28);
        assert_ne!(p[0].file, p[1].file);
    }
}
