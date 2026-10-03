//! Minimaler ChurchTools-REST-Client für Kalender und Termine.
//! Auth über Login-Token (ChurchTools: Profil → Login-Token), Header `Authorization: Login <token>`.

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime};
use serde::Deserialize;
use serde_json::Value;
use slidebear_core::EventFields;
use slidebear_core::sync::RemoteAppointment;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Netzwerkfehler: {0}")]
    Http(#[from] reqwest::Error),
    #[error("ChurchTools meldet {status}: {message}")]
    Api { status: u16, message: String },
    #[error("Login-Token ungültig oder fehlt")]
    Unauthorized,
    #[error("Unerwartete Antwort: {0}")]
    Format(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Calendar {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

pub struct Client {
    base: String,
    token: String,
    http: reqwest::blocking::Client,
}

impl Client {
    /// `instance_url`: z. B. `https://feg-adlershof.church.tools` (mit oder ohne `/api`).
    pub fn new(instance_url: &str, token: &str) -> Result<Self> {
        let mut base = instance_url.trim().trim_end_matches('/').to_string();
        if !base.starts_with("http") {
            base = format!("https://{base}");
        }
        if !base.ends_with("/api") {
            base.push_str("/api");
        }
        let http = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(30)).user_agent("SlideBear").build()?;
        Ok(Self { base, token: token.trim().to_string(), http })
    }

    fn get(&self, path: &str, query: &[(String, String)]) -> Result<Value> {
        let resp = self
            .http
            .get(format!("{}{}", self.base, path))
            .header("Authorization", format!("Login {}", self.token))
            .header("Accept", "application/json")
            .query(query)
            .send()?;
        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(Error::Unauthorized);
        }
        let body: Value = resp.json()?;
        if !status.is_success() {
            return Err(Error::Api { status: status.as_u16(), message: api_message(&body) });
        }
        Ok(body)
    }

    /// Lädt mit Dienstbelegung. ChurchTools-Versionen unterscheiden sich darin, ob `include[]`
    /// oder `include` erwartet wird; bei einem Validierungsfehler wird die nächste Variante versucht.
    fn get_with_services(&self, path: &str, query: &[(String, String)]) -> Result<Value> {
        let mut last = None;
        for key in [Some("include[]"), Some("include"), None] {
            let mut q = query.to_vec();
            if let Some(k) = key {
                q.push((k.to_string(), "eventServices".to_string()));
            }
            match self.get(path, &q) {
                Err(e @ Error::Api { status: 400, .. }) => last = Some(e),
                other => return other,
            }
        }
        Err(last.unwrap_or(Error::Format("keine Antwort".into())))
    }

    /// Prüft die Verbindung, liefert den Namen des angemeldeten Benutzers.
    pub fn whoami(&self) -> Result<String> {
        let v = self.get("/whoami", &[])?;
        let d = &v["data"];
        Ok(format!("{} {}", d["firstName"].as_str().unwrap_or(""), d["lastName"].as_str().unwrap_or("")).trim().to_string())
    }

    pub fn calendars(&self) -> Result<Vec<Calendar>> {
        let v = self.get("/calendars", &[])?;
        serde_json::from_value(v["data"].clone()).map_err(|e| Error::Format(e.to_string()))
    }

    /// Vornamen der Personen, die am `date` einen Dienst haben, dessen Name `service_term`
    /// enthält (z. B. „Präsentation“), über alle Events dieses Tages. Liefert zusätzlich eine
    /// Diagnose ohne Personendaten, falls niemand gefunden wird.
    pub fn service_people(&self, date: NaiveDate, service_term: &str) -> Result<ServiceReport> {
        let services = self.get("/services", &[])?;
        let ids = matching_services(&services, service_term);
        let service_names: Vec<String> = services["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|s| s["id"].as_i64().is_some_and(|id| ids.contains(&id)))
            .filter_map(|s| s["name"].as_str().map(str::to_string))
            .collect();
        if ids.is_empty() {
            return Err(Error::Format(format!("kein Dienst mit „{service_term}“ im Namen gefunden")));
        }

        // `to` ist bei ChurchTools exklusiv, daher bis zum Folgetag abfragen
        let q = [
            ("from".to_string(), date.format("%Y-%m-%d").to_string()),
            ("to".to_string(), (date + chrono::Duration::days(1)).format("%Y-%m-%d").to_string()),
        ];
        let mut events = self.get_with_services("/events", &q)?;

        // Fehlt die Dienstbelegung in der Liste, einzeln pro Event nachladen
        if let Some(list) = events["data"].as_array_mut() {
            for ev in list.iter_mut() {
                let on_day = ev["startDate"].as_str().and_then(parse_ct_date).is_some_and(|d| d.date() == date);
                if on_day
                    && !ev["eventServices"].is_array()
                    && let Some(id) = ev["id"].as_i64()
                {
                    let detail = self.get_with_services(&format!("/events/{id}"), &[])?;
                    ev["eventServices"] = detail["data"]["eventServices"].clone();
                }
            }
        }

        let names = people_on(&events, &ids, date);
        let diagnosis = diagnose(&events, &ids, date, &service_names);
        Ok(ServiceReport { names, diagnosis })
    }

    pub fn appointments(&self, calendar_ids: &[i64], from: NaiveDate, to: NaiveDate) -> Result<Vec<RemoteAppointment>> {
        if calendar_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut q: Vec<(String, String)> = calendar_ids.iter().map(|id| ("calendar_ids[]".into(), id.to_string())).collect();
        q.push(("from".into(), from.format("%Y-%m-%d").to_string()));
        q.push(("to".into(), to.format("%Y-%m-%d").to_string()));
        let v = self.get("/calendars/appointments", &q)?;
        parse_appointments(&v)
    }
}

/// Wandelt die `/calendars/appointments`-Antwort in `RemoteAppointment`s.
/// Getrennt vom HTTP-Teil, damit sie mit Fixtures testbar ist.
pub fn parse_appointments(v: &Value) -> Result<Vec<RemoteAppointment>> {
    let items = v["data"].as_array().ok_or_else(|| Error::Format("`data` fehlt".into()))?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let base = &item["base"];
        let calc = &item["calculated"];
        let all_day = base["allDay"].as_bool().unwrap_or(false);
        let start_raw = calc["startDate"].as_str().or(base["startDate"].as_str()).unwrap_or_default();
        let end_raw = calc["endDate"].as_str().or(base["endDate"].as_str());
        let Some(start) = parse_ct_date(start_raw) else { continue };
        let end = end_raw.and_then(parse_ct_date).filter(|e| *e > start);
        let id = base["id"].as_i64().ok_or_else(|| Error::Format("Termin ohne ID".into()))?;
        let calendar_id = base["calendar"]["id"].as_i64().or(base["calendarId"].as_i64()).unwrap_or(0);
        out.push(RemoteAppointment {
            appointment_id: id,
            calendar_id,
            occurrence: start.date(),
            fields: EventFields {
                title: str_field(&base["title"]),
                start,
                end,
                all_day,
                location: location(base),
                subtitle: str_field(&base["subtitle"]),
            },
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServiceReport {
    pub names: Vec<String>,
    /// Zusammenfassung ohne Personendaten, z. B. für „niemand gefunden“.
    pub diagnosis: String,
}

/// Was an dem Tag gefunden wurde: Dienste, Events, Dienstbelegungen (ohne Namen).
pub fn diagnose(events: &Value, service_ids: &[i64], date: NaiveDate, service_names: &[String]) -> String {
    let all = events["data"].as_array().cloned().unwrap_or_default();
    let on_day: Vec<&Value> =
        all.iter().filter(|ev| ev["startDate"].as_str().and_then(parse_ct_date).is_some_and(|d| d.date() == date)).collect();
    let mut parts = vec![format!("Dienst: {}", service_names.join(", "))];
    if on_day.is_empty() {
        parts.push(format!("keine Veranstaltung am {} gefunden ({} insgesamt geliefert)", date.format("%d.%m."), all.len()));
    }
    for ev in on_day {
        let name = ev["name"].as_str().unwrap_or("?");
        match ev["eventServices"].as_array() {
            None => parts.push(format!("„{name}“: keine Dienstbelegung geliefert (Rechte?)")),
            Some(list) => {
                let matching: Vec<&Value> =
                    list.iter().filter(|es| es["serviceId"].as_i64().is_some_and(|id| service_ids.contains(&id))).collect();
                let with_person = matching.iter().filter(|es| !es["person"].is_null() || es["name"].is_string()).count();
                parts.push(format!("„{name}“: {} Dienste, {} passend, davon {} mit Person", list.len(), matching.len(), with_person));
            }
        }
    }
    parts.join(" · ")
}

/// IDs aller Dienste, deren Name den Begriff enthält (ohne Groß-/Kleinschreibung).
pub fn matching_services(services: &Value, term: &str) -> Vec<i64> {
    let term = term.trim().to_lowercase();
    services["data"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter(|s| !term.is_empty() && s["name"].as_str().is_some_and(|n| n.to_lowercase().contains(&term)))
                .filter_map(|s| s["id"].as_i64())
                .collect()
        })
        .unwrap_or_default()
}

/// Vornamen aus den Dienstbelegungen (`eventServices`) aller Events am Tag `date`.
/// Zugesagte Dienste haben Vorrang; gibt es keine Zusage, zählen auch angefragte.
pub fn people_on(events: &Value, service_ids: &[i64], date: NaiveDate) -> Vec<String> {
    let mut agreed = Vec::new();
    let mut requested = Vec::new();
    for ev in events["data"].as_array().into_iter().flatten() {
        let on_day = ev["startDate"].as_str().and_then(parse_ct_date).is_some_and(|d| d.date() == date);
        if !on_day {
            continue;
        }
        for es in ev["eventServices"].as_array().into_iter().flatten() {
            if !es["serviceId"].as_i64().is_some_and(|id| service_ids.contains(&id)) {
                continue;
            }
            let first = es["person"]["domainAttributes"]["firstName"]
                .as_str()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_string)
                .or_else(|| es["name"].as_str().and_then(|n| n.split_whitespace().next()).map(str::to_string));
            let Some(first) = first else { continue };
            let list = if es["agreed"].as_bool().unwrap_or(false) { &mut agreed } else { &mut requested };
            if !list.contains(&first) {
                list.push(first);
            }
        }
    }
    if agreed.is_empty() { requested } else { agreed }
}

/// Fehlermeldung von ChurchTools inklusive Validierungsdetails (`errors`), falls vorhanden.
fn api_message(body: &Value) -> String {
    let mut msg = body["message"].as_str().unwrap_or("unbekannter Fehler").to_string();
    let details: Vec<String> = body["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let field = e["fieldId"].as_str().or(e["field"].as_str()).unwrap_or("");
            let text = e["message"].as_str().or(e["messageKey"].as_str())?;
            Some(if field.is_empty() { text.to_string() } else { format!("{field}: {text}") })
        })
        .collect();
    if !details.is_empty() {
        msg.push_str(&format!(" ({})", details.join("; ")));
    }
    msg
}

fn str_field(v: &Value) -> String {
    v.as_str().unwrap_or("").trim().to_string()
}

fn location(base: &Value) -> String {
    let addr = &base["address"];
    for key in ["meetingAt", "name"] {
        let s = str_field(&addr[key]);
        if !s.is_empty() {
            return s;
        }
    }
    str_field(&base["location"])
}

/// ChurchTools liefert Zeiten in UTC (`2026-10-07T17:00:00Z`), Ganztags-Termine als Datum.
fn parse_ct_date(s: &str) -> Option<NaiveDateTime> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Local).naive_local());
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(dt);
    }
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok().map(|d| d.and_time(NaiveTime::MIN))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_presentation_service_people() {
        let services: Value = serde_json::from_str(include_str!("../tests/fixtures/services.json")).unwrap();
        let events: Value = serde_json::from_str(include_str!("../tests/fixtures/events.json")).unwrap();
        // „Präs“ trifft „Präsi“ und „Präsentation …“
        assert_eq!(matching_services(&services, "Präs"), vec![12, 13]);
        assert_eq!(matching_services(&services, "präsentation"), vec![13]);
        let ids = matching_services(&services, "Präsi");
        assert_eq!(ids, vec![12]);
        let sunday = NaiveDate::from_ymd_opt(2026, 10, 4).unwrap();
        // Anna hat zugesagt, Ben nur angefragt; Carla ist Technik, Dora an einem anderen Tag
        assert_eq!(people_on(&events, &ids, sunday), vec!["Anna".to_string()]);
        // ohne Zusagen zählen Anfragen
        let none_agreed = serde_json::to_string(&events).unwrap().replace("\"agreed\":true", "\"agreed\":false");
        let events2: Value = serde_json::from_str(&none_agreed).unwrap();
        assert_eq!(people_on(&events2, &ids, sunday), vec!["Anna".to_string(), "Ben".to_string()]);

        let d = diagnose(&events, &ids, sunday, &["Präsentation".into()]);
        assert!(d.contains("„Gottesdienst“: 4 Dienste, 3 passend, davon 2 mit Person"), "{d}");
        let monday = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        assert!(diagnose(&events, &ids, monday, &[]).contains("keine Veranstaltung am 05.10."));
    }

    #[test]
    fn api_message_includes_validation_details() {
        let body: Value =
            serde_json::from_str(r#"{"message":"There are validation errors","errors":[{"fieldId":"include","message":"Invalid value"}]}"#)
                .unwrap();
        assert_eq!(api_message(&body), "There are validation errors (include: Invalid value)");
    }

    #[test]
    fn parses_fixture() {
        let v: Value = serde_json::from_str(include_str!("../tests/fixtures/appointments.json")).unwrap();
        let list = parse_appointments(&v).unwrap();
        assert_eq!(list.len(), 3);

        let gebet = &list[0];
        assert_eq!(gebet.appointment_id, 501);
        assert_eq!(gebet.calendar_id, 2);
        assert_eq!(gebet.fields.title, "Gebetsabend");
        assert_eq!(gebet.fields.location, "Gemeindesaal");
        assert!(gebet.fields.end.is_some());

        // zweite Instanz derselben Serie: gleiche ID, anderes Datum
        assert_eq!(list[1].appointment_id, 501);
        assert_ne!(list[1].occurrence, list[0].occurrence);

        let markt = &list[2];
        assert!(markt.fields.all_day);
        assert_eq!(markt.fields.start.date(), NaiveDate::from_ymd_opt(2026, 12, 5).unwrap());
        assert_eq!(markt.fields.subtitle, "Fleißige Helfer gesucht!");
    }
}
