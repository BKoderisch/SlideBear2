//! Terminübersicht (links, nach Woche gruppiert) und Detailansicht mit Vorschau und Feldern.

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime};
use eframe::egui::{self, Color32, RichText, Vec2};
use slidebear_core::export::{lead_days, scene_for};
use slidebear_core::format::{format_date, format_time};
use slidebear_core::sync::apply_link;
use slidebear_core::{Event, EventSource, EventStatus, SeriesLink};
use uuid::Uuid;

use crate::app::{template_or_default, App};
use crate::editor::EditTarget;

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
    let Some(date) = parse_date(date) else { return false };
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
    egui::Panel::left("event_list").default_size(380.0).resizable(true).show(ui, |ui| list(ui, app));
    egui::CentralPanel::default().show(ui, |ui| match app.selected_event {
        Some(id) if app.store.data.event(id).is_some() => detail(ui, app, id),
        _ => {
            ui.centered_and_justified(|ui| ui.label("Termin links auswählen oder „➕ Neue Slide“ anlegen"));
        }
    });
}

fn list(ui: &mut egui::Ui, app: &mut App) {
    ui.horizontal(|ui| {
        if ui.button("➕ Neue Slide").clicked() {
            new_manual(app);
        }
        ui.checkbox(&mut app.show_past, "Vergangene");
        ui.checkbox(&mut app.show_unlinked, "Ohne Vorlage");
    });
    ui.separator();

    let today = Local::now().date_naive();
    let data = &app.store.data;
    let mut ids: Vec<(Uuid, NaiveDateTime)> = data
        .events
        .iter()
        .filter(|e| app.show_past || e.fields().end.unwrap_or(e.fields().start).date() >= today)
        .filter(|e| app.show_unlinked || e.template_id.is_some() || e.custom_scene.is_some())
        .map(|e| (e.id, e.fields().start))
        .collect();
    ids.sort_by_key(|(_, s)| *s);

    if ids.is_empty() {
        ui.label("Keine Termine. ChurchTools in den Einstellungen verbinden oder eine Slide anlegen.");
        return;
    }

    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        let mut last_week = None;
        for (id, start) in ids {
            let week = start.iso_week();
            if last_week != Some(week) {
                last_week = Some(week);
                let monday = NaiveDate::from_isoywd_opt(week.year(), week.week(), chrono::Weekday::Mon).unwrap_or(start.date());
                let sunday = monday + chrono::Duration::days(6);
                ui.add_space(6.0);
                ui.label(RichText::new(format!("KW {} · {} – {}", week.week(), monday.format("%d.%m."), sunday.format("%d.%m.%Y"))).strong().weak());
            }
            row(ui, app, id, today);
        }
    });
}

fn row(ui: &mut egui::Ui, app: &mut App, id: Uuid, today: NaiveDate) {
    let Some(e) = app.store.data.event(id).cloned() else { return };
    let f = e.fields();
    let template = e.template_id.and_then(|t| app.store.data.template(t)).cloned();
    let selected = app.selected_event == Some(id);
    let pal = app.palette();
    let tpl = template_or_default(template.as_ref());

    let frame = egui::Frame::group(ui.style()).fill(if selected { ui.visuals().selection.bg_fill.gamma_multiply(0.4) } else { Color32::TRANSPARENT });
    let resp = frame
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let thumb = Vec2::new(128.0, 72.0);
                match scene_for(&e, template.as_ref()) {
                    Some(scene) => {
                        let tex = app.previews.get(ui.ctx(), &mut app.renderer, scene, &f, &tpl, thumb.x * 2.0);
                        ui.add(egui::Image::new(&tex).fit_to_exact_size(thumb));
                    }
                    None => {
                        let (r, _) = ui.allocate_exact_size(thumb, egui::Sense::hover());
                        ui.painter().rect_filled(r, 4.0, ui.visuals().faint_bg_color);
                        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "keine Vorlage", egui::FontId::proportional(11.0), ui.visuals().weak_text_color());
                    }
                }
                ui.vertical(|ui| {
                    let title = RichText::new(&f.title).strong();
                    ui.label(if e.status == EventStatus::Cancelled { title.strikethrough() } else { title });
                    let time = format_time(&f, &tpl.time_style);
                    ui.label(format!("{} {}", slidebear_core::format::weekday_de(f.start.date()).get(..2).unwrap_or(""), format_date(&f, &tpl.date_style)) + if time.is_empty() { String::new() } else { format!(" · {time}") }.as_str());
                    ui.horizontal_wrapped(|ui| {
                        if e.is_from_churchtools() {
                            badge(ui, "CT", pal.badge_ct);
                        }
                        if !e.overrides.is_empty() || e.custom_scene.is_some() {
                            badge(ui, "bearbeitet", pal.badge_edited);
                        }
                        if e.status == EventStatus::Cancelled {
                            badge(ui, "entfallen", pal.badge_cancelled);
                        } else if scene_for(&e, template.as_ref()).is_none() {
                            badge(ui, "Vorlage fehlt", pal.badge_muted);
                        } else if e.enabled {
                            let lead = lead_days(&e, &app.store.data.links, app.store.data.settings.export_days);
                            if f.start.date() <= today + chrono::Duration::days(lead as i64) {
                                badge(ui, "wird exportiert", pal.badge_ok);
                            }
                        } else {
                            badge(ui, "aus", pal.badge_muted);
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

fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new().fill(color).corner_radius(4.0).inner_margin(egui::Margin::symmetric(5, 1)).show(ui, |ui| {
        ui.label(RichText::new(text).small().color(Color32::WHITE));
    });
}

fn new_manual(app: &mut App) {
    let mut fields = crate::app::sample_fields();
    fields.title = "Neue Veranstaltung".into();
    fields.subtitle.clear();
    fields.location.clear();
    let template = app.store.data.templates.first().map(|t| t.id);
    let e = Event::manual(fields, template);
    app.selected_event = Some(e.id);
    app.event_form.invalidate();
    app.store.data.events.push(e);
    app.store.mark_dirty();
}

fn detail(ui: &mut egui::Ui, app: &mut App, id: Uuid) {
    if app.event_form.loaded != Some(id)
        && let Some(e) = app.store.data.event(id) {
            app.event_form.load(e);
        }
    let Some(e) = app.store.data.event(id).cloned() else { return };
    let template = e.template_id.and_then(|t| app.store.data.template(t)).cloned();
    let tpl = template_or_default(template.as_ref());
    let f = e.fields();

    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        // Vorschau
        let w = ui.available_width().min(960.0);
        let size = Vec2::new(w, w * 9.0 / 16.0);
        match scene_for(&e, template.as_ref()) {
            Some(scene) => {
                let ppp = ui.ctx().pixels_per_point();
                let tex = app.previews.get(ui.ctx(), &mut app.renderer, scene, &f, &tpl, size.x * ppp);
                let resp = ui.add(egui::Image::new(&tex).fit_to_exact_size(size).sense(egui::Sense::click()));
                if resp.double_clicked() {
                    app.open_editor(EditTarget::Event(id));
                }
                resp.on_hover_text("Doppelklick: frei bearbeiten");
            }
            None => {
                let (r, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                ui.painter().rect_filled(r, 6.0, ui.visuals().faint_bg_color);
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "Bitte eine Vorlage wählen", egui::FontId::proportional(18.0), ui.visuals().weak_text_color());
            }
        }
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui.button("✏ Frei bearbeiten").clicked() {
                app.open_editor(EditTarget::Event(id));
            }
            if e.custom_scene.is_some() && ui.button("↺ Auf Vorlage zurücksetzen").on_hover_text("Freie Änderungen an dieser Slide verwerfen").clicked() {
                if let Some(ev) = app.store.data.event_mut(id) {
                    ev.custom_scene = None;
                }
                app.store.mark_dirty();
            }
            if let Some(scene) = scene_for(&e, template.as_ref())
                && ui.button("💾 PNG speichern …").clicked() {
                    let name = slidebear_core::export::file_name(&f);
                    if let Some(path) = rfd::FileDialog::new().set_file_name(name).add_filter("PNG", &["png"]).save_file() {
                        let pm = app.renderer.render(scene, &|s| slidebear_core::placeholder::resolve(s, &f, &tpl));
                        match slidebear_render::save_png(&pm, &path) {
                            Ok(()) => app.info(format!("Gespeichert: {}", path.display())),
                            Err(err) => app.error(format!("Speichern fehlgeschlagen: {err}")),
                        }
                    }
                }
        });
        ui.add_space(8.0);

        let mut changed = false;
        let from_ct = e.is_from_churchtools();
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
            if ui.add(egui::TextEdit::singleline(&mut form.title).desired_width(400.0)).changed() {
                set(from_ct, &mut ev.base.title, &mut ev.overrides.title, form.title.clone());
                changed = true;
            }
            if reset_btn(ui, ev.overrides.title.is_some(), ev.base.title.clone()) {
                ev.overrides.title = None;
                form.loaded = None;
                changed = true;
            }
            ui.end_row();

            ui.label("Datum");
            ui.horizontal(|ui| {
                let date_ok = parse_date(&form.date).is_some();
                let r1 = ui.add(egui::TextEdit::singleline(&mut form.date).desired_width(100.0).hint_text("TT.MM.JJJJ").text_color_opt((!date_ok).then_some(Color32::LIGHT_RED)));
                ui.label("von");
                let r2 = ui.add(egui::TextEdit::singleline(&mut form.start).desired_width(60.0).hint_text("ganztags"));
                ui.label("bis");
                let r3 = ui.add(egui::TextEdit::singleline(&mut form.end).desired_width(60.0).hint_text("offen"));
                if (r1.changed() || r2.changed() || r3.changed()) && apply_datetime(&mut ev, &form.date, &form.start, &form.end) {
                    changed = true;
                }
            });
            let time_over = ev.overrides.start.is_some() || ev.overrides.end.is_some() || ev.overrides.all_day.is_some();
            let ct_time = format!("{} {}", format_date(&ev.base, &tpl.date_style), format_time(&ev.base, &tpl.time_style));
            if reset_btn(ui, time_over, ct_time) {
                ev.overrides.start = None;
                ev.overrides.end = None;
                ev.overrides.all_day = None;
                form.loaded = None;
                changed = true;
            }
            ui.end_row();

            ui.label("Ort");
            if ui.add(egui::TextEdit::singleline(&mut form.location).desired_width(400.0)).changed() {
                set(from_ct, &mut ev.base.location, &mut ev.overrides.location, form.location.clone());
                changed = true;
            }
            if reset_btn(ui, ev.overrides.location.is_some(), ev.base.location.clone()) {
                ev.overrides.location = None;
                form.loaded = None;
                changed = true;
            }
            ui.end_row();

            ui.label("Untertitel");
            if ui.add(egui::TextEdit::multiline(&mut form.subtitle).desired_width(400.0).desired_rows(2)).changed() {
                set(from_ct, &mut ev.base.subtitle, &mut ev.overrides.subtitle, form.subtitle.clone());
                changed = true;
            }
            if reset_btn(ui, ev.overrides.subtitle.is_some(), ev.base.subtitle.clone()) {
                ev.overrides.subtitle = None;
                form.loaded = None;
                changed = true;
            }
            ui.end_row();

            ui.label("Vorlage");
            let current = template.as_ref().map(|t| t.name.clone()).unwrap_or_else(|| "– keine –".into());
            egui::ComboBox::from_id_salt("tpl").selected_text(current).show_ui(ui, |ui| {
                for t in &app.store.data.templates {
                    if ui.selectable_label(ev.template_id == Some(t.id), &t.name).clicked() {
                        ev.template_id = Some(t.id);
                        ev.enabled = true;
                        changed = true;
                    }
                }
            });
            ui.label("");
            ui.end_row();

            ui.label("Export");
            ui.checkbox(&mut ev.enabled, "Slide exportieren").changed().then(|| changed = true);
            ui.label("");
            ui.end_row();
        });

        if changed {
            if let Some(target) = app.store.data.event_mut(id) {
                *target = ev.clone();
            }
            app.store.mark_dirty();
        }

        // Serien-Verknüpfung für ChurchTools-Termine
        if let EventSource::ChurchTools { appointment_id, calendar_id, .. } = e.source {
            ui.add_space(12.0);
            ui.separator();
            let link = SeriesLink::find(&app.store.data.links, &e).cloned();
            match link {
                Some(l) => {
                    let name = app.store.data.template(l.template_id).map(|t| t.name.clone()).unwrap_or_default();
                    ui.label(format!("🔗 Serie „{}“ ist mit Vorlage „{name}“ verknüpft. Neue Termine dieser Serie bekommen automatisch eine Slide.", l.title));
                }
                None => {
                    ui.label("Diese Terminserie ist noch mit keiner Vorlage verknüpft.");
                    if let Some(tid) = ev.template_id {
                        if ui.button("🔗 Vorlage für die ganze Serie übernehmen").clicked() {
                            let link = SeriesLink {
                                id: Uuid::new_v4(),
                                calendar_id,
                                appointment_id,
                                title: e.base.title.clone(),
                                template_id: tid,
                                default_subtitle: String::new(),
                                lead_days: None,
                            };
                            let n = apply_link(&mut app.store.data.events, &link);
                            app.store.data.links.push(link);
                            app.store.mark_dirty();
                            app.info(format!("Serie verknüpft, {} weitere Termine haben jetzt eine Slide", n));
                        }
                    } else {
                        ui.label(RichText::new("Zuerst oben eine Vorlage wählen.").weak());
                    }
                }
            }
        } else {
            ui.add_space(12.0);
            if ui.button("🗑 Slide löschen").clicked() {
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
    use slidebear_core::{EventFields, FieldOverrides};

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
