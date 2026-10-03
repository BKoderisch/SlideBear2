//! Datums- und Zeitformatierung für Slides (deutsch).

use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};

use crate::event::EventFields;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DateStyle {
    /// chrono-Formatstring, z. B. `%d.%m.%Y` → `07.12.2024`, `%d.%m.%y` → `12.10.24`.
    pub pattern: String,
}

impl Default for DateStyle {
    fn default() -> Self {
        Self { pattern: "%d.%m.%Y".into() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeStyle {
    /// Angehängt an die Uhrzeit, z. B. ` Uhr`.
    pub suffix: String,
    /// `10:00` als `10` schreiben.
    pub omit_zero_minutes: bool,
    /// Endzeit anzeigen (`15-19 Uhr`) oder nur Beginn (`15 Uhr`).
    pub show_end: bool,
}

impl Default for TimeStyle {
    fn default() -> Self {
        Self { suffix: " Uhr".into(), omit_zero_minutes: true, show_end: true }
    }
}

const WEEKDAYS: [&str; 7] = ["Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag", "Sonntag"];

pub fn weekday_de(date: NaiveDate) -> &'static str {
    WEEKDAYS[date.weekday().num_days_from_monday() as usize]
}

/// Datum eines Termins. Mehrtägige Termine im selben Monat werden zusammengezogen:
/// `11.-13.10.2025`, sonst `30.10.2025-02.11.2025`.
pub fn format_date(fields: &EventFields, style: &DateStyle) -> String {
    let start = fields.start.date();
    let end = fields.end.map(|e| e.date()).filter(|e| *e > start);
    let Some(end) = end else {
        return start.format(&style.pattern).to_string();
    };
    let end_str = end.format(&style.pattern).to_string();
    let same_month = start.year() == end.year() && start.month() == end.month();
    if same_month && style.pattern.starts_with("%d.") {
        format!("{:02}.-{}", start.day(), end_str)
    } else {
        format!("{}-{}", start.format(&style.pattern), end_str)
    }
}

/// Einzelne Uhrzeit ohne Suffix: `19:30`, bei `omit_zero_minutes` `10` statt `10:00`.
pub fn format_clock(t: NaiveTime, style: &TimeStyle) -> String {
    if style.omit_zero_minutes && t.minute() == 0 {
        t.hour().to_string()
    } else {
        format!("{}:{:02}", t.hour(), t.minute())
    }
}

/// Uhrzeit bzw. Zeitspanne eines Termins für die Slide.
///
/// Beispiele mit Standard-Stil (` Uhr`, volle Stunden kurz, Ende anzeigen):
/// - 15:00 bis 19:00 → `15-19 Uhr`
/// - 19:30 ohne Ende → `19:30 Uhr`
/// - 10:00 bis 10:00 → `10 Uhr`
pub fn format_time_range(start: NaiveDateTime, end: Option<NaiveDateTime>, style: &TimeStyle) -> String {
    let begin = format_clock(start.time(), style);
    match end.filter(|e| style.show_end && *e > start) {
        Some(e) => format!("{begin}-{}{}", format_clock(e.time(), style), style.suffix),
        None => format!("{begin}{}", style.suffix),
    }
}

/// `{zeit}` für einen Termin: Ganztags-Termine haben keine Uhrzeit,
/// mehrtägige zeigen nur die Startzeit.
pub fn format_time(fields: &EventFields, style: &TimeStyle) -> String {
    if fields.all_day {
        return String::new();
    }
    let end = fields.end.filter(|e| e.date() == fields.start.date());
    format_time_range(fields.start, end, style)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    fn fields(start: &str, end: Option<&str>) -> EventFields {
        EventFields {
            title: "Test".into(),
            start: dt(start),
            end: end.map(dt),
            all_day: false,
            location: String::new(),
            subtitle: String::new(),
        }
    }

    #[test]
    fn time_range_examples_from_real_slides() {
        let s = TimeStyle::default();
        assert_eq!(format_time_range(dt("2024-12-07 15:00"), Some(dt("2024-12-07 19:00")), &s), "15-19 Uhr");
        assert_eq!(format_time_range(dt("2025-09-17 19:30"), None, &s), "19:30 Uhr");
        assert_eq!(format_time_range(dt("2024-10-12 10:00"), None, &s), "10 Uhr");
    }

    #[test]
    fn time_range_edge_cases() {
        let s = TimeStyle::default();
        // Ende gleich Beginn wie ohne Ende behandeln
        assert_eq!(format_time_range(dt("2024-10-12 10:00"), Some(dt("2024-10-12 10:00")), &s), "10 Uhr");
        // gemischt volle/halbe Stunde
        assert_eq!(format_time_range(dt("2024-10-12 18:00"), Some(dt("2024-10-12 21:30")), &s), "18-21:30 Uhr");
        // Ende ausgeblendet
        let only_start = TimeStyle { show_end: false, ..TimeStyle::default() };
        assert_eq!(format_time_range(dt("2024-12-07 15:00"), Some(dt("2024-12-07 19:00")), &only_start), "15 Uhr");
        // immer mit Minuten
        let long = TimeStyle { omit_zero_minutes: false, ..TimeStyle::default() };
        assert_eq!(format_time_range(dt("2024-12-07 15:00"), Some(dt("2024-12-07 19:00")), &long), "15:00-19:00 Uhr");
    }

    #[test]
    fn dates() {
        let d = DateStyle::default();
        let short = DateStyle { pattern: "%d.%m.%y".into() };
        assert_eq!(format_date(&fields("2024-12-07 15:00", None), &d), "07.12.2024");
        assert_eq!(format_date(&fields("2024-10-12 10:00", None), &short), "12.10.24");
        assert_eq!(format_date(&fields("2025-10-03 18:00", Some("2025-10-05 13:00")), &d), "03.-05.10.2025");
        assert_eq!(format_date(&fields("2025-10-30 18:00", Some("2025-11-02 13:00")), &d), "30.10.2025-02.11.2025");
    }

    #[test]
    fn all_day_has_no_time() {
        let mut f = fields("2024-12-07 00:00", None);
        f.all_day = true;
        assert_eq!(format_time(&f, &TimeStyle::default()), "");
    }

    #[test]
    fn weekday() {
        assert_eq!(weekday_de(NaiveDate::from_ymd_opt(2026, 10, 4).unwrap()), "Sonntag");
    }
}
