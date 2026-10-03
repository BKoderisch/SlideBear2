//! Minimaler ChurchTools-REST-Client für Kalender und Termine.
//! Auth über Login-Token (ChurchTools: Profil → Login-Token), Header `Authorization: Login <token>`.

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime};
use serde::Deserialize;
use serde_json::Value;
use slidebear_core::sync::RemoteAppointment;
use slidebear_core::EventFields;

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
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("SlideBear")
            .build()?;
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
            let message = body["message"].as_str().unwrap_or("unbekannter Fehler").to_string();
            return Err(Error::Api { status: status.as_u16(), message });
        }
        Ok(body)
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
