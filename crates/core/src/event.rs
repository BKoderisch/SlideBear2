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

/// Eine konkrete Slide: eigene Szene plus Datums-/Zeitformat. Entsteht als Kopie eines Layouts
/// und gehört dann einer Terminserie oder einem einzelnen Termin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Slide {
    pub scene: Scene,
    pub date_style: DateStyle,
    pub time_style: TimeStyle,
}

impl Slide {
    /// Kopie eines Layouts (Elemente bekommen neue IDs).
    pub fn from_layout(layout: &Template) -> Self {
        let mut scene = layout.scene.clone();
        for e in &mut scene.elements {
            e.id = Uuid::new_v4();
        }
        Self { scene, date_style: layout.date_style.clone(), time_style: layout.time_style.clone() }
    }

    pub fn as_ref(&self) -> SlideRef<'_> {
        SlideRef { scene: &self.scene, date_style: &self.date_style, time_style: &self.time_style }
    }
}

/// Geliehene Sicht auf eine Slide oder ein Layout, alles was zum Rendern nötig ist.
#[derive(Debug, Clone, Copy)]
pub struct SlideRef<'a> {
    pub scene: &'a Scene,
    pub date_style: &'a DateStyle,
    pub time_style: &'a TimeStyle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: Uuid,
    pub source: EventSource,
    /// Werte aus der Quelle (bei manuellen Terminen die einzigen Werte).
    pub base: EventFields,
    pub overrides: FieldOverrides,
    /// Eigene Slide eines einzelnen Termins. Termine einer Serie nutzen die Slide der Serie.
    #[serde(default)]
    pub slide: Option<Slide>,
    /// Diesen Termin exportieren (bei Serien: dieses Datum nicht überspringen).
    pub enabled: bool,
    pub status: EventStatus,
    /// Altes Datenformat (vor eigenen Slides), wird beim Laden migriert.
    #[serde(default, skip_serializing)]
    pub template_id: Option<Uuid>,
    #[serde(default, skip_serializing)]
    pub custom_scene: Option<Scene>,
}

impl Event {
    pub fn manual(fields: EventFields, slide: Option<Slide>) -> Self {
        Self {
            id: Uuid::new_v4(),
            source: EventSource::Manual,
            base: fields,
            overrides: FieldOverrides::default(),
            slide,
            enabled: true,
            status: EventStatus::Active,
            template_id: None,
            custom_scene: None,
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

/// Layout: Ausgangspunkt für neue Slides (z. B. „Foto + Titel + Infozeile“).
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
        Self { id: Uuid::new_v4(), name: name.into(), scene, date_style: DateStyle::default(), time_style: TimeStyle::default() }
    }

    pub fn as_slide_ref(&self) -> SlideRef<'_> {
        SlideRef { scene: &self.scene, date_style: &self.date_style, time_style: &self.time_style }
    }
}

/// Wiederkehrende Veranstaltung aus ChurchTools mit ihrer einen Slide für alle Termine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Series {
    pub id: Uuid,
    pub calendar_id: i64,
    pub appointment_id: i64,
    /// Fallback, falls ChurchTools Einzeltermine mit gleichem Titel ohne Serie anlegt.
    pub title: String,
    #[serde(default)]
    pub slide: Option<Slide>,
    /// Slides dieser Serie exportieren.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Alle Termine dieser Veranstaltung in SlideBear ausblenden.
    #[serde(default)]
    pub hidden: bool,
    pub default_subtitle: String,
    /// Wie viele Tage vorher die Slide exportiert wird (`None` = globaler Standard).
    pub lead_days: Option<u32>,
    /// Änderungen, die für alle Termine der Serie gelten (auch für später synchronisierte).
    #[serde(default)]
    pub overrides: crate::series_edit::SeriesOverrides,
    /// Altes Datenformat (Serie → Vorlage), wird beim Laden migriert.
    #[serde(default, skip_serializing)]
    pub template_id: Option<Uuid>,
}

fn yes() -> bool {
    true
}

impl Series {
    pub fn new(calendar_id: i64, appointment_id: i64, title: &str, slide: Option<Slide>) -> Self {
        Self {
            id: Uuid::new_v4(),
            calendar_id,
            appointment_id,
            title: title.trim().to_string(),
            slide,
            enabled: true,
            hidden: false,
            default_subtitle: String::new(),
            lead_days: None,
            overrides: Default::default(),
            template_id: None,
        }
    }

    pub fn matches(&self, calendar_id: i64, appointment_id: i64, title: &str) -> bool {
        self.calendar_id == calendar_id && (self.appointment_id == appointment_id || self.title.trim().eq_ignore_ascii_case(title.trim()))
    }

    pub fn find<'a>(series: &'a [Series], event: &Event) -> Option<&'a Series> {
        let EventSource::ChurchTools { appointment_id, calendar_id, .. } = event.source else {
            return None;
        };
        series.iter().find(|s| s.matches(calendar_id, appointment_id, &event.base.title))
    }
}

/// Ob ein ChurchTools-Termin ausgeblendet ist: seine Veranstaltung wurde ausgeblendet oder sein
/// Titel enthält einen der Filterbegriffe (ohne Groß-/Kleinschreibung). Eigene Termine nie.
pub fn is_hidden(event: &Event, series: &[Series], rules: &[String]) -> bool {
    if !event.is_from_churchtools() {
        return false;
    }
    if Series::find(series, event).is_some_and(|s| s.hidden) {
        return true;
    }
    let title = event.base.title.to_lowercase();
    rules.iter().map(|r| r.trim().to_lowercase()).any(|r| !r.is_empty() && title.contains(&r))
}

/// Die Slide, mit der ein Termin gerendert wird: Serien-Slide, sonst die eigene.
pub fn slide_for<'a>(event: &'a Event, series: &'a [Series]) -> Option<SlideRef<'a>> {
    match Series::find(series, event) {
        Some(s) => s.slide.as_ref().map(Slide::as_ref),
        None => event.slide.as_ref().map(Slide::as_ref),
    }
}

/// Überführt Daten aus der Zeit, als Vorlagen live mit Terminen verknüpft waren:
/// jede Verknüpfung bekommt eine eigene Kopie der Vorlage als Slide.
pub fn migrate(events: &mut [Event], series: &mut [Series], templates: &[Template]) -> bool {
    let find = |id: Option<Uuid>| id.and_then(|id| templates.iter().find(|t| t.id == id));
    let mut changed = false;
    for s in series.iter_mut() {
        if let Some(t) = find(s.template_id.take()) {
            s.slide.get_or_insert_with(|| Slide::from_layout(t));
            changed = true;
        }
    }
    for e in events.iter_mut() {
        let template = find(e.template_id.take());
        if let Some(scene) = e.custom_scene.take() {
            let mut slide = template.map(Slide::from_layout).unwrap_or_else(|| Slide {
                scene: Scene::default(),
                date_style: DateStyle::default(),
                time_style: TimeStyle::default(),
            });
            slide.scene = scene;
            e.slide.get_or_insert(slide);
            changed = true;
        } else if let Some(t) = template {
            if Series::find(series, e).is_none() {
                e.slide.get_or_insert_with(|| Slide::from_layout(t));
            }
            changed = true;
        }
    }
    // Früher wurden unverknüpfte ChurchTools-Termine deaktiviert angelegt; heute bedeutet
    // `enabled = false` „dieses Datum überspringen“, also alle wieder einschalten.
    if changed {
        for e in events.iter_mut().filter(|e| e.is_from_churchtools()) {
            e.enabled = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

    fn ct(appointment_id: i64, title: &str) -> Event {
        let mut e = Event::manual(
            EventFields {
                title: title.into(),
                start: NaiveDateTime::parse_from_str("2026-10-07 19:00", "%Y-%m-%d %H:%M").unwrap(),
                end: None,
                all_day: false,
                location: String::new(),
                subtitle: String::new(),
            },
            None,
        );
        e.source = EventSource::ChurchTools { appointment_id, calendar_id: 1, occurrence: e.base.start.date() };
        e
    }

    #[test]
    fn series_slide_wins_over_own_slide() {
        let layout = Template::new("L", Scene::default());
        let mut e = ct(5, "Gebet");
        e.slide = Some(Slide::from_layout(&layout));
        let mut series_slide = Slide::from_layout(&layout);
        series_slide.date_style.pattern = "%d.%m.".into();
        let series = vec![Series::new(1, 5, "Gebet", Some(series_slide))];
        assert_eq!(slide_for(&e, &series).unwrap().date_style.pattern, "%d.%m.");
        assert_eq!(slide_for(&e, &[]).unwrap().date_style.pattern, "%d.%m.%Y");
    }

    #[test]
    fn hiding_by_series_and_title_rule() {
        let mut s = Series::new(1, 5, "Gebet", None);
        let gebet = ct(5, "Gebet");
        let putz = ct(6, "Putzdienst Gemeindehaus");
        let manual = Event::manual(gebet.base.clone(), None);
        assert!(!is_hidden(&gebet, std::slice::from_ref(&s), &[]));
        s.hidden = true;
        assert!(is_hidden(&gebet, std::slice::from_ref(&s), &[]));
        let rules = vec!["putzdienst".to_string(), "  ".to_string()];
        assert!(is_hidden(&putz, &[], &rules));
        assert!(!is_hidden(&manual, std::slice::from_ref(&s), &rules));
    }

    #[test]
    fn migrates_old_links_and_custom_scenes() {
        let t = Template::new("Alt", Scene::default());
        let mut series = vec![Series::new(1, 5, "Gebet", None)];
        series[0].template_id = Some(t.id);
        let mut linked = ct(5, "Gebet");
        linked.template_id = Some(t.id);
        let mut single = ct(9, "Konzert");
        single.template_id = Some(t.id);
        single.enabled = false;
        let mut events = vec![linked, single];
        assert!(migrate(&mut events, &mut series, std::slice::from_ref(&t)));
        assert!(series[0].slide.is_some());
        assert!(events[0].slide.is_none(), "Serien-Termine nutzen die Serien-Slide");
        assert!(events[1].slide.is_some());
        assert!(events[1].enabled);
        assert!(events.iter().all(|e| e.template_id.is_none()));
    }
}
