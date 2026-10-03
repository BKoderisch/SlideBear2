//! Abgleich von ChurchTools-Terminen mit den lokalen Events.
//!
//! Regeln:
//! - Neue Termine werden angelegt; passt eine Serien-Verknüpfung, bekommen sie gleich die Vorlage.
//! - Bestehende Termine bekommen die neuen Quellwerte in `base`, lokale `overrides` bleiben unangetastet.
//! - Termine im Sync-Zeitraum, die in ChurchTools fehlen, werden als `Cancelled` markiert (nicht gelöscht).

use chrono::{NaiveDate, NaiveDateTime};

use crate::event::{Event, EventFields, EventSource, EventStatus, FieldOverrides, SeriesLink};

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
    links: &[SeriesLink],
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
                if e.template_id.is_none()
                    && let Some(link) = SeriesLink::find(links, e) {
                        e.template_id = Some(link.template_id);
                        e.enabled = true;
                    }
            }
            None => {
                let mut e = Event {
                    id: uuid::Uuid::new_v4(),
                    source: EventSource::ChurchTools {
                        appointment_id: r.appointment_id,
                        calendar_id: r.calendar_id,
                        occurrence: r.occurrence,
                    },
                    base: r.fields.clone(),
                    overrides: FieldOverrides::default(),
                    template_id: None,
                    custom_scene: None,
                    enabled: false,
                    status: EventStatus::Active,
                };
                if let Some(link) = SeriesLink::find(links, &e) {
                    e.template_id = Some(link.template_id);
                    e.enabled = true;
                    if e.base.subtitle.is_empty() {
                        e.base.subtitle = link.default_subtitle.clone();
                    }
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

/// Wendet eine (neue) Serien-Verknüpfung auf alle passenden Termine ohne Vorlage an.
pub fn apply_link(events: &mut [Event], link: &SeriesLink) -> usize {
    let mut n = 0;
    for e in events.iter_mut() {
        if e.template_id.is_none() && SeriesLink::find(std::slice::from_ref(link), e).is_some() {
            e.template_id = Some(link.template_id);
            e.enabled = true;
            if e.base.subtitle.is_empty() {
                e.base.subtitle = link.default_subtitle.clone();
            }
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

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

    fn link(template: Uuid) -> SeriesLink {
        SeriesLink {
            id: Uuid::new_v4(),
            calendar_id: 1,
            appointment_id: 10,
            title: "Gebetsabend".into(),
            template_id: template,
            default_subtitle: "Alle sind willkommen".into(),
            lead_days: None,
        }
    }

    const FROM: &str = "2026-10-01 00:00";
    const TO: &str = "2026-11-01 00:00";

    #[test]
    fn adds_and_links_series() {
        let tpl = Uuid::new_v4();
        let mut events = Vec::new();
        let r = [remote(10, "2026-10-07 19:00", "Gebetsabend"), remote(99, "2026-10-08 19:00", "Sonstiges")];
        let rep = merge(&mut events, &r, &[link(tpl)], &[1], dt(FROM), dt(TO));
        assert_eq!(rep.added, 2);
        assert_eq!(events[0].template_id, Some(tpl));
        assert!(events[0].enabled);
        assert_eq!(events[0].base.subtitle, "Alle sind willkommen");
        assert_eq!(events[1].template_id, None);
        assert!(!events[1].enabled);
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
