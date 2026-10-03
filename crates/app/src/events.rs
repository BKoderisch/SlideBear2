//! Terminübersicht (links, nach Woche gruppiert) und Detailansicht mit Vorschau und Feldern.

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime};
use eframe::egui::{self, Color32, RichText, Vec2};
use slidebear_core::export::{file_name, plan};
use slidebear_core::format::{DateStyle, TimeStyle, format_date, format_time};
use slidebear_core::series_edit::Field;
use slidebear_core::{Event, EventStatus, Series, is_hidden, placeholder, slide_for};
use uuid::Uuid;

use crate::app::{App, SlideOwner, slide_buttons};
use crate::theme::{self, badge};

/// Eingabepuffer für die Detailfelder (Datum/Uhrzeit als Text, bis sie gültig sind).
#[derive(Default)]
pub struct EventForm {
    loaded: Option<Uuid>,
    title: String,
    date: String,
    start: String,
    end: String,
    location: String,
    subtitle: String,
}

impl EventForm {
    pub fn invalidate(&mut self) {
        self.loaded = None;
    }

    fn load(&mut self, e: &Event) {
        let f = e.fields();
        self.loaded = Some(e.id);
        self.title = f.title;
        self.date = f.start.format("%d.%m.%Y").to_string();
        self.start = if f.all_day { String::new() } else { f.start.format("%H:%M").to_string() };
        self.end = f.end.map(|e| e.format("%H:%M").to_string()).unwrap_or_default();
        self.location = f.location;
        self.subtitle = f.subtitle;
    }
}

/// Übernimmt Datum, Beginn und Ende aus Texteingaben. Leerer Beginn = ganztags, leeres Ende = offen.
/// Liefert `false`, solange die Eingabe (noch) ungültig ist; der Termin bleibt dann unverändert.
pub(crate) fn apply_datetime(ev: &mut Event, date: &str, start: &str, end: &str) -> bool {
    let Some(date) = parse_date(date) else {
        return false;
    };
    let from_ct = ev.is_from_churchtools();
    let start_t = parse_time(start);
    let all_day = start.trim().is_empty();
    if !all_day && start_t.is_none() {
        return false;
    }
    let start_dt = NaiveDateTime::new(date, start_t.unwrap_or(NaiveTime::MIN));
    let end_dt = parse_time(end).map(|t| NaiveDateTime::new(date, t)).filter(|e| *e > start_dt);
    set(from_ct, &mut ev.base.start, &mut ev.overrides.start, start_dt);
    set(from_ct, &mut ev.base.all_day, &mut ev.overrides.all_day, all_day);
    if end.trim().is_empty() || end_dt.is_some() {
        set(from_ct, &mut ev.base.end, &mut ev.overrides.end, end_dt);
    }
    true
}

pub(crate) fn set_title(ev: &mut Event, title: String) {
    let from_ct = ev.is_from_churchtools();
    set(from_ct, &mut ev.base.title, &mut ev.overrides.title, title);
}

pub(crate) fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    // Zweistellige Jahre zuerst als `%y` lesen, sonst wird „26“ zum Jahr 26 statt 2026
    let short_year = s.rsplit('.').next().is_some_and(|y| y.len() == 2);
    let formats: &[&str] = if short_year { &["%d.%m.%y"] } else { &["%d.%m.%Y", "%Y-%m-%d"] };
    for fmt in formats {
        if let Ok(d) = NaiveDate::parse_from_str(s, fmt) {
            return Some(d);
        }
    }
    None
}

pub(crate) fn parse_time(s: &str) -> Option<NaiveTime> {
    let s = s.trim().trim_end_matches("Uhr").trim();
    NaiveTime::parse_from_str(s, "%H:%M").or_else(|_| NaiveTime::parse_from_str(&format!("{s}:00"), "%H:%M")).ok()
}

/// Setzt einen Wert: bei ChurchTools-Terminen als lokale Änderung (oder entfernt sie,
/// wenn der Wert wieder dem ChurchTools-Wert entspricht), sonst direkt.
fn set<T: PartialEq + Clone>(from_ct: bool, base: &mut T, over: &mut Option<T>, v: T) {
    if from_ct {
        *over = (v != *base).then_some(v);
    } else {
        *base = v;
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    egui::Panel::left("event_list").default_size(400.0).resizable(true).show(ui, |ui| list(ui, app));
    egui::CentralPanel::default().show(ui, |ui| match app.selected_event {
        Some(id) if app.store.data.event(id).is_some() => detail(ui, app, id),
        _ => {
            ui.centered_and_justified(|ui| ui.label("Termin links auswählen oder „➕ Termin“ anlegen"));
        }
    });
}

/// Formate der Slide eines Termins (Standard, wenn es noch keine gibt).
fn styles(e: &Event, series: &[Series]) -> (DateStyle, TimeStyle) {
    slide_for(e, series).map(|s| (s.date_style.clone(), s.time_style.clone())).unwrap_or_default()
}

fn list(ui: &mut egui::Ui, app: &mut App) {
    ui.horizontal(|ui| {
        if ui.button("➕ Termin").clicked() {
            new_manual(app);
        }
        ui.checkbox(&mut app.show_past, "Vergangene");
        ui.checkbox(&mut app.show_unlinked, "Ohne Slide");
        ui.checkbox(&mut app.show_hidden, "Ausgeblendete");
    });
    ui.separator();

    let today = Local::now().date_naive();
    let data = &app.store.data;
    let mut ids: Vec<(Uuid, NaiveDateTime)> = data
        .events
        .iter()
        .filter(|e| app.show_past || e.fields().end.unwrap_or(e.fields().start).date() >= today)
        .filter(|e| app.show_unlinked || slide_for(e, &data.series).is_some())
        .filter(|e| app.show_hidden || !is_hidden(e, &data.series, &data.settings.hide_rules))
        .map(|e| (e.id, e.fields().start))
        .collect();
    ids.sort_by_key(|(_, s)| *s);

    if ids.is_empty() {
        ui.label("Keine Termine. ChurchTools in den Einstellungen verbinden oder einen Termin anlegen.");
        return;
    }

    let planned: Vec<Uuid> =
        plan(&data.events, &data.series, &data.settings.hide_rules, today, data.settings.export_days).iter().map(|p| p.event.id).collect();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        let mut last_week = None;
        for (id, start) in ids {
            let week = start.iso_week();
            if last_week != Some(week) {
                last_week = Some(week);
                let monday = NaiveDate::from_isoywd_opt(week.year(), week.week(), chrono::Weekday::Mon).unwrap_or(start.date());
                let sunday = monday + chrono::Duration::days(6);
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!("KW {} · {} - {}", week.week(), monday.format("%d.%m."), sunday.format("%d.%m.%Y")))
                        .strong()
                        .weak(),
                );
            }
            row(ui, app, id, planned.contains(&id));
        }
    });
}

fn row(ui: &mut egui::Ui, app: &mut App, id: Uuid, planned: bool) {
    let Some(e) = app.store.data.event(id).cloned() else {
        return;
    };
    let f = e.fields();
    let series = app.store.data.series.clone();
    let slide = slide_for(&e, &series);
    let in_series = Series::find(&series, &e).is_some();
    let hidden = is_hidden(&e, &series, &app.store.data.settings.hide_rules);
    let (ds, ts) = styles(&e, &series);
    let selected = app.selected_event == Some(id);

    let frame = egui::Frame::group(ui.style()).fill(if selected { theme::SUN } else { Color32::TRANSPARENT });
    let resp = frame
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let thumb = Vec2::new(128.0, 72.0);
                match slide {
                    Some(sl) => {
                        let tex = app.previews.get(ui.ctx(), &mut app.renderer, sl, &f, thumb.x * 2.0);
                        ui.add(egui::Image::new(&tex).fit_to_exact_size(thumb));
                    }
                    None => {
                        let (r, _) = ui.allocate_exact_size(thumb, egui::Sense::hover());
                        ui.painter().rect_filled(r, 8.0, ui.visuals().faint_bg_color);
                        ui.painter().text(
                            r.center(),
                            egui::Align2::CENTER_CENTER,
                            "keine Slide",
                            egui::FontId::proportional(13.0),
                            ui.visuals().weak_text_color(),
                        );
                    }
                }
                ui.vertical(|ui| {
                    let title = RichText::new(&f.title).strong();
                    ui.label(if e.status == EventStatus::Cancelled { title.strikethrough() } else { title });
                    let time = format_time(&f, &ts);
                    let weekday = slidebear_core::format::weekday_de(f.start.date()).get(..2).unwrap_or("").to_string();
                    ui.label(format!(
                        "{weekday} {}{}",
                        format_date(&f, &ds),
                        if time.is_empty() { String::new() } else { format!(" · {time}") }
                    ));
                    ui.horizontal_wrapped(|ui| {
                        if in_series {
                            badge(ui, "Serie", theme::BADGE_CT);
                        } else if e.is_from_churchtools() {
                            badge(ui, "CT", theme::BADGE_CT);
                        }
                        if !e.overrides.is_empty() {
                            badge(ui, "bearbeitet", theme::BADGE_EDITED);
                        }
                        if hidden {
                            badge(ui, "ausgeblendet", theme::BADGE_MUTED);
                        }
                        if e.status == EventStatus::Cancelled {
                            badge(ui, "entfallen", theme::BADGE_CANCELLED);
                        } else if slide.is_none() {
                            badge(ui, "Slide fehlt", theme::BADGE_MUTED);
                        } else if planned {
                            badge(ui, "wird exportiert", theme::BADGE_OK);
                        } else if !e.enabled {
                            badge(ui, "übersprungen", theme::BADGE_MUTED);
                        }
                    });
                });
            });
        })
        .response
        .interact(egui::Sense::click());
    if resp.clicked() {
        app.selected_event = Some(id);
    }
}

pub(crate) fn new_manual(app: &mut App) -> Uuid {
    let mut fields = crate::app::sample_fields();
    fields.title = "Neue Veranstaltung".into();
    fields.subtitle.clear();
    fields.location.clear();
    let e = Event::manual(fields, None);
    let id = e.id;
    app.selected_event = Some(id);
    app.event_form.invalidate();
    app.store.data.events.push(e);
    app.store.mark_dirty();
    id
}

fn detail(ui: &mut egui::Ui, app: &mut App, id: Uuid) {
    if app.event_form.loaded != Some(id)
        && let Some(e) = app.store.data.event(id)
    {
        app.event_form.load(e);
    }
    let Some(e) = app.store.data.event(id).cloned() else {
        return;
    };
    let series = app.store.data.series.clone();
    let (ds, ts) = styles(&e, &series);
    let owner = app.owner_of(&e);
    let f = e.fields();

    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        // Vorschau
        let w = ui.available_width().min(960.0);
        let size = Vec2::new(w, w * 9.0 / 16.0);
        match slide_for(&e, &series) {
            Some(slide) => {
                let ppp = ui.ctx().pixels_per_point();
                let tex = app.previews.get(ui.ctx(), &mut app.renderer, slide, &f, size.x * ppp);
                let resp = ui.add(egui::Image::new(&tex).fit_to_exact_size(size).sense(egui::Sense::click()));
                if resp.double_clicked() {
                    app.edit_slide(&owner);
                }
                resp.on_hover_text("Doppelklick: Slide bearbeiten");
            }
            None => {
                let (r, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                ui.painter().rect_filled(r, 12.0, ui.visuals().faint_bg_color);
                ui.painter().text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    "Noch keine Slide: „➕ Slide anlegen“",
                    egui::FontId::proportional(20.0),
                    ui.visuals().weak_text_color(),
                );
            }
        }
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label(RichText::new("Slide").strong());
            slide_buttons(ui, app, &owner, &e);
            if let Some(slide) = slide_for(&e, &series)
                && ui.button("💾 PNG speichern …").clicked()
                && let Some(path) = rfd::FileDialog::new().set_file_name(file_name(&f)).add_filter("PNG", &["png"]).save_file()
            {
                let pm = app.renderer.render(slide.scene, &|s| placeholder::resolve(s, &f, slide));
                match slidebear_render::save_png(&pm, &path) {
                    Ok(()) => app.info(format!("Gespeichert: {}", path.display())),
                    Err(err) => app.error(format!("Speichern fehlgeschlagen: {err}")),
                }
            }
        });
        match &owner {
            SlideOwner::Series(sid) => {
                let title = series.iter().find(|s| s.id == *sid).map(|s| s.title.clone()).unwrap_or_default();
                ui.label(
                    RichText::new(format!(
                        "🔁 Eine Slide für alle Termine von „{title}“. Datum und Uhrzeit kommen automatisch vom jeweiligen Termin."
                    ))
                    .weak(),
                );
            }
            SlideOwner::Appointment { .. } => {
                ui.label(RichText::new("🔁 Eine hier angelegte Slide gilt automatisch für alle Termine dieser Reihe.").weak());
            }
            SlideOwner::Event(_) => {}
        }
        ui.add_space(8.0);

        let from_ct = e.is_from_churchtools();
        if from_ct && !matches!(owner, SlideOwner::Event(_)) {
            crate::app::scope_toggle(ui, &mut app.edit_scope, "nur diesen Termin");
            ui.add_space(6.0);
        }

        let mut changed = false;
        let mut reset: Option<Field> = None;
        let mut ev = e.clone();
        let form = &mut app.event_form;

        egui::Grid::new("fields").num_columns(3).spacing([12.0, 8.0]).show(ui, |ui| {
            let reset_btn = |ui: &mut egui::Ui, overridden: bool, ct_value: String| -> bool {
                if from_ct && overridden {
                    ui.button("↺").on_hover_text(format!("ChurchTools-Wert: {ct_value}")).clicked()
                } else {
                    ui.label("");
                    false
                }
            };

            ui.label("Titel");
            if theme::text_field(ui, egui::TextEdit::singleline(&mut form.title), 420.0).changed() {
                set(from_ct, &mut ev.base.title, &mut ev.overrides.title, form.title.clone());
                changed = true;
            }
            if reset_btn(ui, ev.overrides.title.is_some(), ev.base.title.clone()) {
                reset = Some(Field::Title);
            }
            ui.end_row();

            ui.label("Datum");
            ui.horizontal(|ui| {
                let date_ok = parse_date(&form.date).is_some();
                let r1 = theme::text_field(
                    ui,
                    egui::TextEdit::singleline(&mut form.date)
                        .hint_text("TT.MM.JJJJ")
                        .text_color_opt((!date_ok).then_some(theme::CANCELLED_TEXT)),
                    140.0,
                );
                ui.label("von");
                let r2 = theme::text_field(ui, egui::TextEdit::singleline(&mut form.start).hint_text("ganztags"), 110.0);
                ui.label("bis");
                let r3 = theme::text_field(ui, egui::TextEdit::singleline(&mut form.end).hint_text("offen"), 110.0);
                if (r1.changed() || r2.changed() || r3.changed()) && apply_datetime(&mut ev, &form.date, &form.start, &form.end) {
                    changed = true;
                }
            });
            let time_over = ev.overrides.start.is_some() || ev.overrides.end.is_some() || ev.overrides.all_day.is_some();
            let ct_time = format!("{} {}", format_date(&ev.base, &ds), format_time(&ev.base, &ts));
            if reset_btn(ui, time_over, ct_time) {
                reset = Some(Field::Time);
            }
            ui.end_row();

            ui.label("Ort");
            if theme::text_field(ui, egui::TextEdit::singleline(&mut form.location), 420.0).changed() {
                set(from_ct, &mut ev.base.location, &mut ev.overrides.location, form.location.clone());
                changed = true;
            }
            if reset_btn(ui, ev.overrides.location.is_some(), ev.base.location.clone()) {
                reset = Some(Field::Location);
            }
            ui.end_row();

            ui.label("Untertitel");
            if theme::text_area(ui, egui::TextEdit::multiline(&mut form.subtitle), 420.0, 2).changed() {
                set(from_ct, &mut ev.base.subtitle, &mut ev.overrides.subtitle, form.subtitle.clone());
                changed = true;
            }
            if reset_btn(ui, ev.overrides.subtitle.is_some(), ev.base.subtitle.clone()) {
                reset = Some(Field::Subtitle);
            }
            ui.end_row();

            ui.label("Export");
            let label = if matches!(owner, SlideOwner::Series(_)) {
                "Diesen Termin berücksichtigen (aus = überspringen)"
            } else {
                "Slide exportieren"
            };
            if ui.checkbox(&mut ev.enabled, label).changed() {
                changed = true;
            }
            ui.label("");
            ui.end_row();
        });

        if let Some(field) = reset {
            app.reset_field(id, field);
            app.event_form.invalidate();
        } else if changed {
            app.commit_event(ev);
        }

        ui.add_space(12.0);
        if e.is_from_churchtools() {
            let series_hidden = Series::find(&app.store.data.series, &e).is_some_and(|s| s.hidden);
            if series_hidden {
                if ui.button("Veranstaltung wieder einblenden").clicked() {
                    app.set_hidden(&e, false);
                }
            } else if is_hidden(&e, &app.store.data.series, &app.store.data.settings.hide_rules) {
                ui.label(RichText::new("Ausgeblendet durch einen Titel-Filter (Einstellungen › Ausgeblendete Termine).").weak());
            } else if ui
                .button("Veranstaltung ausblenden")
                .on_hover_text("Alle Termine dieser Veranstaltung ausblenden, auch künftige")
                .clicked()
            {
                app.set_hidden(&e, true);
                app.selected_event = None;
            }
        } else {
            if ui.button("🗑 Termin löschen").clicked() {
                app.store.data.events.retain(|x| x.id != id);
                app.selected_event = None;
                app.store.mark_dirty();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use slidebear_core::{EventFields, EventSource, FieldOverrides};

    fn ct_event() -> Event {
        let mut e = Event::manual(
            EventFields {
                title: "Gebetsabend".into(),
                start: NaiveDateTime::parse_from_str("2026-10-07 19:00", "%Y-%m-%d %H:%M").unwrap(),
                end: None,
                all_day: false,
                location: String::new(),
                subtitle: String::new(),
            },
            None,
        );
        e.source = EventSource::ChurchTools { appointment_id: 1, calendar_id: 1, occurrence: e.base.start.date() };
        e
    }

    #[test]
    fn quick_edit_creates_overrides_for_ct_events() {
        let mut e = ct_event();
        assert!(apply_datetime(&mut e, "07.10.2026", "19:30", "21:00"));
        let f = e.fields();
        assert_eq!(f.start.format("%H:%M").to_string(), "19:30");
        assert_eq!(f.end.unwrap().format("%H:%M").to_string(), "21:00");
        // ChurchTools-Wert bleibt unberührt
        assert_eq!(e.base.start.format("%H:%M").to_string(), "19:00");

        // Zurück auf den Originalwert: Override verschwindet wieder
        assert!(apply_datetime(&mut e, "07.10.2026", "19:00", ""));
        assert_eq!(e.overrides, FieldOverrides::default());
    }

    #[test]
    fn invalid_input_changes_nothing() {
        let mut e = ct_event();
        assert!(!apply_datetime(&mut e, "07.10.", "19:00", ""));
        assert!(!apply_datetime(&mut e, "07.10.2026", "19:7x", ""));
        assert!(e.overrides.is_empty());
    }

    #[test]
    fn empty_start_means_all_day() {
        let mut e = ct_event();
        assert!(apply_datetime(&mut e, "08.10.26", "", ""));
        assert!(e.fields().all_day);
        assert_eq!(e.fields().start.date().to_string(), "2026-10-08");
    }

    #[test]
    fn short_time_input() {
        assert_eq!(parse_time("19"), NaiveTime::from_hms_opt(19, 0, 0));
        assert_eq!(parse_time("19:30 Uhr"), NaiveTime::from_hms_opt(19, 30, 0));
    }
}
