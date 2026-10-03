//! Termine, Vorlagen und Serien-Verknüpfungen.

use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::format::{DateStyle, TimeStyle};
use crate::scene::Scene;

/// Die inhaltlichen Felder eines Termins, so wie sie auf der Slide landen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventFields {
    pub title: String,
    pub start: NaiveDateTime,
    pub end: Option<NaiveDateTime>,
    pub all_day: bool,
    pub location: String,
    pub subtitle: String,
}

/// Lokale Änderungen pro Feld. `None` heißt: Wert aus der Quelle (ChurchTools) verwenden.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FieldOverrides {
    pub title: Option<String>,
    pub start: Option<NaiveDateTime>,
    pub end: Option<Option<NaiveDateTime>>,
    pub all_day: Option<bool>,
    pub location: Option<String>,
    pub subtitle: Option<String>,
}

impl FieldOverrides {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn apply(&self, base: &EventFields) -> EventFields {
        EventFields {
            title: self.title.clone().unwrap_or_else(|| base.title.clone()),
            start: self.start.unwrap_or(base.start),
            end: self.end.unwrap_or(base.end),
            all_day: self.all_day.unwrap_or(base.all_day),
            location: self.location.clone().unwrap_or_else(|| base.location.clone()),
            subtitle: self.subtitle.clone().unwrap_or_else(|| base.subtitle.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventSource {
    Manual,
    ChurchTools {
        /// ID des Termins; bei Serien gemeinsam für alle Wiederholungen.
        appointment_id: i64,
        calendar_id: i64,
        /// Ursprüngliches Datum dieser Wiederholung (unterscheidet Serien-Instanzen).
        occurrence: NaiveDate,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventStatus {
    Active,
    /// In ChurchTools gelöscht oder abgesagt.
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: Uuid,
    pub source: EventSource,
    /// Werte aus der Quelle (bei manuellen Terminen die einzigen Werte).
    pub base: EventFields,
    pub overrides: FieldOverrides,
    pub template_id: Option<Uuid>,
    /// `Some`, sobald die Slide frei bearbeitet wurde (Kopie der Vorlagen-Szene).
    pub custom_scene: Option<Scene>,
    /// Soll exportiert werden.
    pub enabled: bool,
    pub status: EventStatus,
}

impl Event {
    pub fn manual(fields: EventFields, template_id: Option<Uuid>) -> Self {
        Self {
            id: Uuid::new_v4(),
            source: EventSource::Manual,
            base: fields,
            overrides: FieldOverrides::default(),
            template_id,
            custom_scene: None,
            enabled: true,
            status: EventStatus::Active,
        }
    }

    /// Die tatsächlich angezeigten Werte (Quelle + lokale Änderungen).
    pub fn fields(&self) -> EventFields {
        self.overrides.apply(&self.base)
    }

    pub fn is_from_churchtools(&self) -> bool {
        matches!(self.source, EventSource::ChurchTools { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    pub id: Uuid,
    pub name: String,
    pub scene: Scene,
    pub date_style: DateStyle,
    pub time_style: TimeStyle,
}

impl Template {
    pub fn new(name: impl Into<String>, scene: Scene) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            scene,
            date_style: DateStyle::default(),
            time_style: TimeStyle::default(),
        }
    }
}

/// Verknüpft eine ChurchTools-Terminserie mit einer Vorlage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriesLink {
    pub id: Uuid,
    pub calendar_id: i64,
    pub appointment_id: i64,
    /// Fallback, falls ChurchTools Einzeltermine mit gleichem Titel ohne Serie anlegt.
    pub title: String,
    pub template_id: Uuid,
    pub default_subtitle: String,
    /// Wie viele Tage vorher die Slide exportiert wird (`None` = globaler Standard).
    pub lead_days: Option<u32>,
}

impl SeriesLink {
    pub fn matches(&self, calendar_id: i64, appointment_id: i64, title: &str) -> bool {
        self.calendar_id == calendar_id
            && (self.appointment_id == appointment_id || self.title.trim().eq_ignore_ascii_case(title.trim()))
    }

    pub fn find<'a>(links: &'a [SeriesLink], event: &Event) -> Option<&'a SeriesLink> {
        let EventSource::ChurchTools { appointment_id, calendar_id, .. } = event.source else {
            return None;
        };
        links.iter().find(|l| l.matches(calendar_id, appointment_id, &event.base.title))
    }
}
