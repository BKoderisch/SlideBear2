//! Abgleich von ChurchTools-Terminen mit den lokalen Events.
//!
//! Regeln:
//! - Neue Termine werden angelegt; gehören sie zu einer Serie mit Slide, nutzen sie automatisch deren Slide.
//! - Bestehende Termine bekommen die neuen Quellwerte in `base`, lokale `overrides` bleiben unangetastet.
//! - Termine im Sync-Zeitraum, die in ChurchTools fehlen, werden als `Cancelled` markiert (nicht gelöscht).

use chrono::{NaiveDate, NaiveDateTime};

use crate::event::{Event, EventFields, EventSource, EventStatus, FieldOverrides, Series};

#[derive(Debug, Clone, PartialEq)]
pub struct RemoteAppointment {
    pub appointment_id: i64,
    pub calendar_id: i64,
    pub occurrence: NaiveDate,
    pub fields: EventFields,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncReport {
    pub added: usize,
    pub updated: usize,
    pub cancelled: usize,
    pub restored: usize,
}

fn key(source: &EventSource) -> Option<(i64, NaiveDate)> {
    match source {
        EventSource::ChurchTools { appointment_id, occurrence, .. } => Some((*appointment_id, *occurrence)),
        EventSource::Manual => None,
    }
}

/// `calendars`: die abgefragten Kalender. Nur deren Termine im Zeitraum `from..to` können als
/// entfallen markiert werden, damit ein Sync mit anderer Kalenderauswahl nichts kaputt macht.
pub fn merge(
    events: &mut Vec<Event>,
    remote: &[RemoteAppointment],
    series: &[Series],
    calendars: &[i64],
    from: NaiveDateTime,
    to: NaiveDateTime,
) -> SyncReport {
    let mut report = SyncReport::default();

    for r in remote {
        let existing = events.iter_mut().find(|e| key(&e.source) == Some((r.appointment_id, r.occurrence)));
        match existing {
            Some(e) => {
                if e.base != r.fields {
                    e.base = r.fields.clone();
                    report.updated += 1;
                }
                if e.status == EventStatus::Cancelled {
                    e.status = EventStatus::Active;
                    report.restored += 1;
                }
            }
            None => {
                let mut e = Event::manual(r.fields.clone(), None);
                e.source = EventSource::ChurchTools {
                    appointment_id: r.appointment_id,
                    calendar_id: r.calendar_id,
                    occurrence: r.occurrence,
                };
                e.overrides = FieldOverrides::default();
                if let Some(s) = Series::find(series, &e) {
                    if e.base.subtitle.is_empty() {
                        e.base.subtitle = s.default_subtitle.clone();
                    }
                    crate::series_edit::apply_to(&s.overrides, &mut e);
                }
                events.push(e);
                report.added += 1;
            }
        }
    }

    for e in events.iter_mut() {
        let EventSource::ChurchTools { appointment_id, calendar_id, occurrence } = e.source else {
            continue;
        };
        let in_scope = calendars.contains(&calendar_id) && e.base.start >= from && e.base.start < to;
        let still_there = remote.iter().any(|r| r.appointment_id == appointment_id && r.occurrence == occurrence);
        if in_scope && !still_there && e.status == EventStatus::Active {
            e.status = EventStatus::Cancelled;
            report.cancelled += 1;
        }
    }

    events.sort_by_key(|e| e.fields().start);
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{slide_for, Slide, Template};
    use crate::scene::Scene;

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    fn remote(id: i64, start: &str, title: &str) -> RemoteAppointment {
        RemoteAppointment {
            appointment_id: id,
            calendar_id: 1,
            occurrence: dt(start).date(),
            fields: EventFields {
                title: title.into(),
                start: dt(start),
                end: None,
                all_day: false,
                location: String::new(),
                subtitle: String::new(),
            },
        }
    }

    const FROM: &str = "2026-10-01 00:00";
    const TO: &str = "2026-11-01 00:00";

    #[test]
    fn new_occurrences_use_series_slide() {
        let mut s = Series::new(1, 10, "Gebetsabend", Some(Slide::from_layout(&Template::new("L", Scene::default()))));
        s.default_subtitle = "Alle sind willkommen".into();
        let series = vec![s];
        let mut events = Vec::new();
        let r = [remote(10, "2026-10-07 19:00", "Gebetsabend"), remote(10, "2026-10-14 19:00", "Gebetsabend"), remote(99, "2026-10-08 19:00", "Sonstiges")];
        let rep = merge(&mut events, &r, &series, &[1], dt(FROM), dt(TO));
        assert_eq!(rep.added, 3);
        assert!(events.iter().all(|e| e.enabled && e.slide.is_none()));
        let gebet: Vec<_> = events.iter().filter(|e| e.base.title == "Gebetsabend").collect();
        assert!(gebet.iter().all(|e| slide_for(e, &series).is_some()));
        assert_eq!(gebet[0].base.subtitle, "Alle sind willkommen");
        let other = events.iter().find(|e| e.base.title == "Sonstiges").unwrap();
        assert!(slide_for(other, &series).is_none());
    }

    #[test]
    fn keeps_local_overrides() {
        let mut events = Vec::new();
        merge(&mut events, &[remote(10, "2026-10-07 19:00", "Gebetsabend")], &[], &[1], dt(FROM), dt(TO));
        events[0].overrides.start = Some(dt("2026-10-07 19:30"));

        let mut changed = remote(10, "2026-10-07 19:00", "Gebetsabend neu");
        changed.fields.location = "Saal".into();
        let rep = merge(&mut events, &[changed], &[], &[1], dt(FROM), dt(TO));
        assert_eq!(rep.updated, 1);
        let f = events[0].fields();
        assert_eq!(f.title, "Gebetsabend neu");
        assert_eq!(f.location, "Saal");
        assert_eq!(f.start, dt("2026-10-07 19:30"));
    }

    #[test]
    fn marks_missing_as_cancelled_only_in_scope() {
        let mut events = Vec::new();
        let r = [remote(10, "2026-10-07 19:00", "A"), remote(11, "2026-12-07 19:00", "B")];
        merge(&mut events, &r, &[], &[1], dt(FROM), dt("2027-01-01 00:00"));
        let rep = merge(&mut events, &[], &[], &[1], dt(FROM), dt(TO));
        assert_eq!(rep.cancelled, 1);
        assert_eq!(events[0].status, EventStatus::Cancelled);
        assert_eq!(events[1].status, EventStatus::Active);
        // anderer Kalender ausgewählt: nichts wird entfallen
        let rep = merge(&mut events, &[], &[], &[2], dt(FROM), dt("2027-01-01 00:00"));
        assert_eq!(rep.cancelled, 0);
    }
}
