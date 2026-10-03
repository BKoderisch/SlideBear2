//! Änderungen an Serienterminen „nur für diesen Termin“ oder „für alle Termine der Serie“.
//!
//! „Alle“ wird in der Serie gespeichert (`SeriesOverrides`) und auf jeden Termin als dessen
//! lokale Änderung übertragen; so bleiben ChurchTools-Werte erhalten und neue Termine aus dem
//! Sync bekommen dieselben Änderungen. Das Datum ist immer termin-spezifisch, für alle Termine
//! gilt nur die Uhrzeit.

use chrono::{NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};

use crate::event::{Event, EventFields, Series};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SeriesOverrides {
    pub title: Option<String>,
    pub start_time: Option<NaiveTime>,
    /// `Some(None)`: Endzeit für alle entfernt.
    pub end_time: Option<Option<NaiveTime>>,
    pub all_day: Option<bool>,
    pub location: Option<String>,
    pub subtitle: Option<String>,
}

impl SeriesOverrides {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Was in der Oberfläche einzeln zurückgesetzt werden kann.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Title,
    Time,
    Location,
    Subtitle,
}

/// Setzt eine lokale Änderung, oder entfernt sie, wenn der Wert dem ChurchTools-Wert entspricht.
fn set<T: PartialEq>(base: &T, over: &mut Option<T>, v: T) {
    *over = (v != *base).then_some(v);
}

/// Überträgt die Serien-Änderungen auf einen Termin (dessen eigenes Datum bleibt).
pub fn apply_to(o: &SeriesOverrides, e: &mut Event) {
    if let Some(t) = &o.title {
        set(&e.base.title, &mut e.overrides.title, t.clone());
    }
    if let Some(l) = &o.location {
        set(&e.base.location, &mut e.overrides.location, l.clone());
    }
    if let Some(s) = &o.subtitle {
        set(&e.base.subtitle, &mut e.overrides.subtitle, s.clone());
    }
    if let Some(all_day) = o.all_day {
        set(&e.base.all_day, &mut e.overrides.all_day, all_day);
    }
    let date = e.fields().start.date();
    if let Some(t) = o.start_time {
        set(&e.base.start, &mut e.overrides.start, NaiveDateTime::new(date, t));
    }
    if let Some(end) = o.end_time {
        let start = e.fields().start;
        let end = end.map(|t| NaiveDateTime::new(date, t)).filter(|x| *x > start);
        set(&e.base.end, &mut e.overrides.end, end);
    }
}

/// Nach einer Änderung an `edited` (vorher: `before`): die geänderten Felder für die ganze
/// Serie übernehmen und auf alle Termine der Serie übertragen.
pub fn apply_to_all(before: &EventFields, edited: &Event, events: &mut [Event], series: &mut Series) {
    let after = edited.fields();
    let o = &mut series.overrides;
    if after.title != before.title {
        o.title = Some(after.title.clone());
    }
    if after.location != before.location {
        o.location = Some(after.location.clone());
    }
    if after.subtitle != before.subtitle {
        o.subtitle = Some(after.subtitle.clone());
    }
    if after.all_day != before.all_day {
        o.all_day = Some(after.all_day);
    }
    if after.start.time() != before.start.time() {
        o.start_time = Some(after.start.time());
    }
    if after.end.map(|e| e.time()) != before.end.map(|e| e.time()) {
        o.end_time = Some(after.end.map(|e| e.time()));
    }
    let overrides = series.overrides.clone();
    let one = std::slice::from_ref(series);
    for e in events.iter_mut().filter(|e| Series::find(one, e).is_some()) {
        apply_to(&overrides, e);
    }
}

/// Setzt ein Feld für die ganze Serie auf die ChurchTools-Werte zurück.
pub fn reset_all(field: Field, events: &mut [Event], series: &mut Series) {
    let o = &mut series.overrides;
    match field {
        Field::Title => o.title = None,
        Field::Location => o.location = None,
        Field::Subtitle => o.subtitle = None,
        Field::Time => {
            o.start_time = None;
            o.end_time = None;
            o.all_day = None;
        }
    }
    let one = std::slice::from_ref(series);
    for e in events.iter_mut().filter(|e| Series::find(one, e).is_some()) {
        let ov = &mut e.overrides;
        match field {
            Field::Title => ov.title = None,
            Field::Location => ov.location = None,
            Field::Subtitle => ov.subtitle = None,
            Field::Time => {
                // Eine termin-spezifische Datumsänderung bleibt erhalten, nur die Uhrzeit fällt zurück
                let date = ov.start.map(|s| s.date());
                ov.start = date.filter(|d| *d != e.base.start.date()).map(|d| NaiveDateTime::new(d, e.base.start.time()));
                ov.end = None;
                ov.all_day = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventSource;
    use crate::sync::{merge, RemoteAppointment};

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    fn occurrence(start: &str) -> Event {
        let mut e = Event::manual(
            EventFields { title: "Gebet".into(), start: dt(start), end: Some(dt(start) + chrono::Duration::hours(1)), all_day: false, location: "Saal".into(), subtitle: String::new() },
            None,
        );
        e.source = EventSource::ChurchTools { appointment_id: 7, calendar_id: 1, occurrence: e.base.start.date() };
        e
    }

    fn setup() -> (Vec<Event>, Series) {
        (vec![occurrence("2026-10-07 19:00"), occurrence("2026-10-14 19:00"), occurrence("2026-10-21 19:00")], Series::new(1, 7, "Gebet", None))
    }

    #[test]
    fn time_change_for_all_keeps_each_date() {
        let (mut events, mut s) = setup();
        let before = events[1].fields();
        let mut edited = events[1].clone();
        edited.overrides.start = Some(dt("2026-10-14 19:30"));
        edited.overrides.end = Some(Some(dt("2026-10-14 21:00")));
        events[1] = edited.clone();
        apply_to_all(&before, &edited, &mut events, &mut s);

        let starts: Vec<_> = events.iter().map(|e| e.fields().start).collect();
        assert_eq!(starts, [dt("2026-10-07 19:30"), dt("2026-10-14 19:30"), dt("2026-10-21 19:30")]);
        assert!(events.iter().all(|e| e.fields().end.unwrap().time() == NaiveTime::from_hms_opt(21, 0, 0).unwrap()));
        // Titel unverändert, also keine Serien-Änderung dafür
        assert!(s.overrides.title.is_none());
    }

    #[test]
    fn date_change_stays_on_this_occurrence() {
        let (mut events, mut s) = setup();
        let before = events[0].fields();
        let mut edited = events[0].clone();
        edited.overrides.start = Some(dt("2026-10-08 19:00"));
        edited.overrides.location = Some("Kapelle".into());
        events[0] = edited.clone();
        apply_to_all(&before, &edited, &mut events, &mut s);
        assert_eq!(events[0].fields().start, dt("2026-10-08 19:00"));
        assert_eq!(events[1].fields().start, dt("2026-10-14 19:00"));
        assert!(events.iter().all(|e| e.fields().location == "Kapelle"));
    }

    #[test]
    fn new_synced_occurrences_get_series_changes() {
        let (mut events, mut s) = setup();
        let before = events[0].fields();
        let mut edited = events[0].clone();
        edited.overrides.title = Some("Gebetsabend".into());
        events[0] = edited.clone();
        apply_to_all(&before, &edited, &mut events, &mut s);

        let new = RemoteAppointment { appointment_id: 7, calendar_id: 1, occurrence: dt("2026-10-28 19:00").date(), fields: occurrence("2026-10-28 19:00").base };
        merge(&mut events, &[new], std::slice::from_ref(&s), &[1], dt("2026-10-01 00:00"), dt("2026-11-30 00:00"));
        let added = events.iter().find(|e| e.base.start == dt("2026-10-28 19:00")).unwrap();
        assert_eq!(added.fields().title, "Gebetsabend");
    }

    #[test]
    fn reset_all_restores_churchtools_values() {
        let (mut events, mut s) = setup();
        let before = events[0].fields();
        let mut edited = events[0].clone();
        edited.overrides.start = Some(dt("2026-10-08 20:00"));
        events[0] = edited.clone();
        apply_to_all(&before, &edited, &mut events, &mut s);
        reset_all(Field::Time, &mut events, &mut s);
        assert!(s.overrides.is_empty());
        // Datum vom ersten Termin bleibt verschoben, Uhrzeit wieder original
        assert_eq!(events[0].fields().start, dt("2026-10-08 19:00"));
        assert_eq!(events[1].fields().start, dt("2026-10-14 19:00"));
        assert!(events[1].overrides.is_empty());
    }
}
