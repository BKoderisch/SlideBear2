//! Welche Slides exportiert werden und unter welchem Dateinamen.

use chrono::{Duration, NaiveDate};

use crate::assets::content_hash;
use std::collections::HashSet;

use uuid::Uuid;

use crate::event::{Event, EventFields, EventStatus, Series, SlideRef, is_hidden, slide_for};

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

/// Fingerabdruck aller Eingaben eines Renders, um unveränderte Slides nicht neu zu schreiben.
pub fn render_key(slide: SlideRef, fields: &EventFields) -> String {
    let mut buf = serde_json::to_vec(slide.scene).unwrap_or_default();
    buf.extend(serde_json::to_vec(fields).unwrap_or_default());
    buf.extend(serde_json::to_vec(&(slide.date_style, slide.time_style)).unwrap_or_default());
    format!("{:016x}", content_hash(&buf))
}

#[derive(Debug)]
pub struct Planned<'a> {
    pub event: &'a Event,
    pub slide: SlideRef<'a>,
    pub fields: EventFields,
    pub file: String,
}

/// Lead-Zeit eines Termins: aus der Serie oder der globale Standard.
pub fn lead_days(event: &Event, series: &[Series], default_days: u32) -> u32 {
    Series::find(series, event).and_then(|s| s.lead_days).unwrap_or(default_days)
}

fn last_day(f: &EventFields) -> NaiveDate {
    f.end.unwrap_or(f.start).date().max(f.start.date())
}

/// Alle Slides, die heute im Export-Ordner liegen sollen. Von einer Serie wird nur der
/// nächste Termin exportiert (die Slide zeigt dessen Datum).
/// Ausgeblendete Termine (siehe [`is_hidden`]) werden nie exportiert.
pub fn plan<'a>(events: &'a [Event], series: &'a [Series], hide_rules: &[String], today: NaiveDate, default_days: u32) -> Vec<Planned<'a>> {
    let mut sorted: Vec<&Event> =
        events.iter().filter(|e| e.enabled && e.status == EventStatus::Active && !is_hidden(e, series, hide_rules)).collect();
    sorted.sort_by_key(|e| e.fields().start);

    let mut seen_series: HashSet<Uuid> = HashSet::new();
    let mut out: Vec<Planned> = Vec::new();
    for e in sorted {
        let fields = e.fields();
        let horizon = today + Duration::days(lead_days(e, series, default_days) as i64);
        if last_day(&fields) < today || fields.start.date() > horizon {
            continue;
        }
        if let Some(s) = Series::find(series, e)
            && (!s.enabled || !seen_series.insert(s.id))
        {
            continue;
        }
        let Some(slide) = slide_for(e, series) else { continue };
        let file = file_name(&fields);
        out.push(Planned { event: e, slide, fields, file });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file));
    // Gleiche Dateinamen (zwei Termine gleicher Titel zur gleichen Zeit) eindeutig machen
    for i in 1..out.len() {
        if out[..i].iter().any(|p| p.file == out[i].file) {
            let stem = out[i].file.trim_end_matches(".png").to_string();
            out[i].file = format!("{stem}-{}.png", i + 1);
        }
    }
    out
}

/// Zeilen der Schnellansicht: Termine derselben Serie bzw. desselben ChurchTools-Termins
/// werden zusammengefasst.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroupKey {
    Series(Uuid),
    Appointment { calendar_id: i64, appointment_id: i64 },
    Single(Uuid),
}

pub fn group_key(event: &Event, series: &[Series]) -> GroupKey {
    if let Some(s) = Series::find(series, event) {
        return GroupKey::Series(s.id);
    }
    match event.source {
        crate::event::EventSource::ChurchTools { appointment_id, calendar_id, .. } => GroupKey::Appointment { calendar_id, appointment_id },
        crate::event::EventSource::Manual => GroupKey::Single(event.id),
    }
}

#[derive(Debug)]
pub struct Group<'a> {
    pub key: GroupKey,
    /// Nächster Termin der Gruppe im Zeitraum (wird angezeigt und exportiert).
    pub next: &'a Event,
    /// Weitere Termine derselben Gruppe im Zeitraum.
    pub more: usize,
}

/// Gruppiert die Termine im Zeitraum `from..=to`; vergangene und entfallene Termine zählen nur,
/// wenn es sonst nichts gibt (damit z. B. eine Absage sichtbar bleibt).
pub fn groups<'a>(events: &'a [Event], series: &[Series], hide_rules: &[String], from: NaiveDate, to: NaiveDate) -> Vec<Group<'a>> {
    let mut sorted: Vec<&Event> = events
        .iter()
        .filter(|e| !is_hidden(e, series, hide_rules))
        .filter(|e| {
            let f = e.fields();
            last_day(&f) >= from && f.start.date() <= to
        })
        .collect();
    sorted.sort_by_key(|e| (e.status != EventStatus::Active, !e.enabled, e.fields().start));

    let mut out: Vec<Group> = Vec::new();
    for e in sorted {
        let key = group_key(e, series);
        match out.iter_mut().find(|g| g.key == key) {
            Some(g) => g.more += 1,
            None => out.push(Group { key, next: e, more: 0 }),
        }
    }
    out.sort_by_key(|g| g.next.fields().start);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventSource, Slide, Template};
    use crate::scene::Scene;
    use chrono::NaiveDateTime;

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    fn slide() -> Slide {
        Slide::from_layout(&Template::new("L", Scene::default()))
    }

    fn ev(title: &str, start: &str) -> Event {
        Event::manual(
            EventFields {
                title: title.into(),
                start: dt(start),
                end: None,
                all_day: false,
                location: String::new(),
                subtitle: String::new(),
            },
            Some(slide()),
        )
    }

    fn ct(appointment_id: i64, title: &str, start: &str) -> Event {
        let mut e = ev(title, start);
        e.slide = None;
        e.source = EventSource::ChurchTools { appointment_id, calendar_id: 1, occurrence: e.base.start.date() };
        e
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 4).unwrap()
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Gemeindeforum Süd!"), "gemeindeforum-sued");
        assert_eq!(slug("  Fleißige   Helfer  "), "fleissige-helfer");
        assert_eq!(slug("!!!"), "slide");
    }

    #[test]
    fn file_names_sort_chronologically() {
        assert_eq!(file_name(&ev("Gemeindeforum", "2025-09-17 19:30").fields()), "2025-09-17_1930_gemeindeforum.png");
    }

    #[test]
    fn plan_respects_window_and_flags() {
        let mut disabled = ev("Aus", "2026-10-10 10:00");
        disabled.enabled = false;
        let mut no_slide = ev("Ohne", "2026-10-10 10:00");
        no_slide.slide = None;
        let events = vec![
            ev("Vergangen", "2026-10-03 10:00"),
            ev("Heute", "2026-10-04 18:00"),
            ev("Bald", "2026-10-20 10:00"),
            ev("Zu weit", "2026-12-20 10:00"),
            disabled,
            no_slide,
        ];
        let p = plan(&events, &[], &[], today(), 28);
        let titles: Vec<_> = p.iter().map(|p| p.fields.title.as_str()).collect();
        assert_eq!(titles, ["Heute", "Bald"]);
    }

    #[test]
    fn series_exports_only_next_enabled_occurrence() {
        let series = vec![Series::new(1, 7, "Gebet", Some(slide()))];
        let mut skipped = ct(7, "Gebet", "2026-10-07 19:00");
        skipped.enabled = false;
        let events = vec![skipped, ct(7, "Gebet", "2026-10-14 19:00"), ct(7, "Gebet", "2026-10-21 19:00")];
        let p = plan(&events, &series, &[], today(), 28);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].fields.start, dt("2026-10-14 19:00"));

        let mut off = series.clone();
        off[0].enabled = false;
        assert!(plan(&events, &off, &[], today(), 28).is_empty());
    }

    #[test]
    fn ct_event_without_series_slide_is_not_exported() {
        let events = vec![ct(8, "Konzert", "2026-10-10 19:00")];
        assert!(plan(&events, &[], &[], today(), 28).is_empty());
        let series = vec![Series::new(1, 8, "Konzert", None)];
        assert!(plan(&events, &series, &[], today(), 28).is_empty());
    }

    #[test]
    fn groups_collapse_recurring_events() {
        let series = vec![Series::new(1, 7, "Gebet", Some(slide()))];
        let events = vec![
            ct(7, "Gebet", "2026-10-07 19:00"),
            ct(7, "Gebet", "2026-10-14 19:00"),
            ct(7, "Gebet", "2026-10-21 19:00"),
            ct(9, "Jugend", "2026-10-09 18:00"),
            ct(9, "Jugend", "2026-10-16 18:00"),
            ev("Konzert", "2026-10-11 17:00"),
        ];
        let g = groups(&events, &series, &[], today(), today() + Duration::days(60));
        assert_eq!(g.len(), 3);
        assert_eq!((g[0].next.base.title.as_str(), g[0].more), ("Gebet", 2));
        assert_eq!((g[1].next.base.title.as_str(), g[1].more), ("Jugend", 1));
        assert!(matches!(g[1].key, GroupKey::Appointment { appointment_id: 9, .. }));
        assert_eq!(g[2].next.base.title, "Konzert");
    }

    #[test]
    fn hidden_events_are_neither_listed_nor_exported() {
        let mut series = vec![Series::new(1, 7, "Gebet", Some(slide()))];
        let events = vec![ct(7, "Gebet", "2026-10-07 19:00"), ct(8, "Putzdienst", "2026-10-08 10:00")];
        let rules = vec!["putz".to_string()];
        let to = today() + Duration::days(60);
        assert_eq!(groups(&events, &series, &rules, today(), to).len(), 1);
        assert_eq!(plan(&events, &series, &rules, today(), 28).len(), 1);
        series[0].hidden = true;
        assert!(groups(&events, &series, &rules, today(), to).is_empty());
        assert!(plan(&events, &series, &rules, today(), 28).is_empty());
    }

    #[test]
    fn duplicate_names_get_suffix() {
        let events = vec![ev("X", "2026-10-10 10:00"), ev("X", "2026-10-10 10:00")];
        let p = plan(&events, &[], &[], today(), 28);
        assert_ne!(p[0].file, p[1].file);
    }
}
