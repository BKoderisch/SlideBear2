//! Schnellansicht für den Sonntag: alle anstehenden Termine als Tabelle, direkt editierbar,
//! mit Export-Häkchen und einem großen Export-Knopf.

use std::collections::{HashMap, HashSet};

use chrono::{Duration, Local};
use eframe::egui::{self, Color32, RichText, Vec2};
use slidebear_core::export::{plan, scene_for};
use slidebear_core::{Event, EventStatus};
use uuid::Uuid;

use crate::app::{template_or_default, App};
use crate::editor::EditTarget;
use crate::events::{apply_datetime, parse_date, parse_time, set_title};

/// Texteingaben einer Zeile, solange sie bearbeitet wird.
#[derive(Default, Clone)]
struct RowBuf {
    title: String,
    date: String,
    start: String,
    end: String,
}

impl RowBuf {
    fn from_event(e: &Event) -> Self {
        let f = e.fields();
        Self {
            title: f.title,
            date: f.start.format("%d.%m.%Y").to_string(),
            start: if f.all_day { String::new() } else { f.start.format("%H:%M").to_string() },
            end: f.end.map(|e| e.format("%H:%M").to_string()).unwrap_or_default(),
        }
    }
}

pub struct QuickState {
    rows: HashMap<Uuid, RowBuf>,
    /// Angezeigter Zeitraum in Tagen ab heute.
    pub days: u32,
}

impl QuickState {
    pub fn new(days: u32) -> Self {
        Self { rows: HashMap::new(), days }
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    let pal = app.palette();
    let today = Local::now().date_naive();
    let horizon = today + Duration::days(app.quick.days as i64);

    // Was ein Export jetzt tatsächlich schreiben würde
    let planned: HashSet<Uuid> = {
        let d = &app.store.data;
        plan(&d.events, &d.templates, &d.links, today, d.settings.export_days).iter().map(|p| p.event.id).collect()
    };

    let mut ids: Vec<(Uuid, chrono::NaiveDateTime)> = app
        .store
        .data
        .events
        .iter()
        .filter(|e| {
            let f = e.fields();
            f.end.unwrap_or(f.start).date() >= today && f.start.date() <= horizon
        })
        .map(|e| (e.id, e.fields().start))
        .collect();
    ids.sort_by_key(|(_, s)| *s);

    egui::Panel::top("quick_head").show(ui, |ui| {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.heading("Schnellexport");
            ui.label("Termine der nächsten");
            ui.add(egui::DragValue::new(&mut app.quick.days).range(1..=365).suffix(" Tage"));
            ui.separator();
            if ui.button("Alle an").clicked() {
                set_all(app, &ids, true);
            }
            if ui.button("Alle aus").clicked() {
                set_all(app, &ids, false);
            }
            if ui.button("➕ Termin").clicked() {
                let mut fields = crate::app::sample_fields();
                fields.title = "Neue Veranstaltung".into();
                fields.subtitle.clear();
                fields.location.clear();
                let e = Event::manual(fields, app.store.data.templates.first().map(|t| t.id));
                app.store.data.events.push(e);
                app.store.mark_dirty();
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let n = planned.len();
            let export = egui::Button::new(RichText::new(format!("⬆  {n} Slides exportieren")).size(20.0).strong().color(pal.on_accent))
                .fill(pal.accent_strong)
                .min_size(Vec2::new(280.0, 44.0));
            if ui.add(export).clicked() {
                app.export_now();
            }
            if ui.add_enabled(app.jobs.running == 0, egui::Button::new("🔄 Erst mit ChurchTools abgleichen").min_size(Vec2::new(0.0, 44.0))).clicked() {
                let ctx = ui.ctx().clone();
                app.start_sync(&ctx);
            }
            match app.store.data.settings.export_dir.clone() {
                Some(dir) => {
                    if ui.link(format!("📂 {}", dir.display())).on_hover_text("Ordner öffnen").clicked() {
                        let _ = open::that(dir);
                    }
                }
                None => {
                    ui.label(RichText::new("Kein Export-Ordner gewählt (Einstellungen)").color(ui.visuals().warn_fg_color));
                }
            }
        });
        ui.add_space(6.0);
    });

    egui::CentralPanel::default().show(ui, |ui| {
        if ids.is_empty() {
            ui.centered_and_justified(|ui| ui.label("Keine Termine in diesem Zeitraum."));
            return;
        }
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("quick").num_columns(8).striped(true).spacing([14.0, 6.0]).min_row_height(40.0).show(ui, |ui| {
                for h in ["Export", "", "Termin", "Datum", "Beginn", "Ende", "Vorlage", ""] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for (id, _) in &ids {
                    row(ui, app, *id, planned.contains(id));
                    ui.end_row();
                }
            });
            ui.add_space(8.0);
            ui.label(
                RichText::new("Beginn leer = ganztags, Ende leer = offen. Änderungen an ChurchTools-Terminen bleiben beim Sync erhalten.")
                    .weak()
                    .small(),
            );
        });
    });
}

fn set_all(app: &mut App, ids: &[(Uuid, chrono::NaiveDateTime)], on: bool) {
    for (id, _) in ids {
        if let Some(e) = app.store.data.event_mut(*id)
            && (e.template_id.is_some() || e.custom_scene.is_some()) {
                e.enabled = on;
            }
    }
    app.store.mark_dirty();
}

fn row(ui: &mut egui::Ui, app: &mut App, id: Uuid, planned: bool) {
    let Some(mut ev) = app.store.data.event(id).cloned() else { return };
    let template = ev.template_id.and_then(|t| app.store.data.template(t)).cloned();
    let tpl = template_or_default(template.as_ref());
    let has_scene = scene_for(&ev, template.as_ref()).is_some();
    let cancelled = ev.status == EventStatus::Cancelled;
    let pal = app.palette();

    let mut changed = false;

    // Export-Häkchen
    ui.add_enabled_ui(has_scene && !cancelled, |ui| {
        if ui.checkbox(&mut ev.enabled, "").on_disabled_hover_text("Erst eine Vorlage wählen").changed() {
            changed = true;
        }
    });

    // Vorschau, groß beim Darüberfahren, Doppelklick öffnet den Editor
    match scene_for(&ev, template.as_ref()) {
        Some(scene) => {
            let f = ev.fields();
            let tex = app.previews.get(ui.ctx(), &mut app.renderer, scene, &f, &tpl, 128.0);
            let resp = ui.add(egui::Image::new(&tex).fit_to_exact_size(Vec2::new(64.0, 36.0)).sense(egui::Sense::click()));
            let big = app.previews.get(ui.ctx(), &mut app.renderer, scene, &f, &tpl, 960.0);
            if resp.double_clicked() {
                app.open_editor(EditTarget::Event(id));
            }
            resp.on_hover_ui(|ui| {
                ui.add(egui::Image::new(&big).fit_to_exact_size(Vec2::new(480.0, 270.0)));
                ui.label(RichText::new("Doppelklick: Slide bearbeiten").small());
            });
        }
        None => {
            ui.label("");
        }
    }

    // Eingabe-IDs, um zu erkennen, ob gerade in dieser Zeile getippt wird
    let ids = ["title", "date", "start", "end"].map(|k| ui.make_persistent_id((id, k)));
    let editing = ui.memory(|m| ids.iter().any(|i| m.has_focus(*i)));
    let buf = app.quick.rows.entry(id).or_default();
    if !editing {
        *buf = RowBuf::from_event(&ev);
    }

    let red = |ok: bool| (!ok).then_some(Color32::LIGHT_RED);
    let title_r = ui.add(egui::TextEdit::singleline(&mut buf.title).id(ids[0]).desired_width(280.0));
    let date_ok = parse_date(&buf.date).is_some();
    let date_r = ui.add(egui::TextEdit::singleline(&mut buf.date).id(ids[1]).desired_width(90.0).hint_text("TT.MM.JJJJ").text_color_opt(red(date_ok)));
    let start_ok = buf.start.trim().is_empty() || parse_time(&buf.start).is_some();
    let start_r = ui.add(egui::TextEdit::singleline(&mut buf.start).id(ids[2]).desired_width(56.0).hint_text("ganztags").text_color_opt(red(start_ok)));
    let end_ok = buf.end.trim().is_empty() || parse_time(&buf.end).is_some();
    let end_r = ui.add(egui::TextEdit::singleline(&mut buf.end).id(ids[3]).desired_width(56.0).hint_text("offen").text_color_opt(red(end_ok)));

    if title_r.changed() {
        set_title(&mut ev, buf.title.clone());
        changed = true;
    }
    if (date_r.changed() || start_r.changed() || end_r.changed()) && apply_datetime(&mut ev, &buf.date, &buf.start, &buf.end) {
        changed = true;
    }

    // Vorlage
    let current = template.as_ref().map(|t| t.name.clone()).unwrap_or_else(|| "wählen …".into());
    egui::ComboBox::from_id_salt((id, "tpl")).width(150.0).selected_text(current).show_ui(ui, |ui| {
        for t in &app.store.data.templates {
            if ui.selectable_label(ev.template_id == Some(t.id), &t.name).clicked() {
                ev.template_id = Some(t.id);
                ev.enabled = true;
                changed = true;
            }
        }
    });

    // Status
    ui.horizontal(|ui| {
        if cancelled {
            ui.label(RichText::new("entfallen").color(pal.cancelled_text));
        } else if !has_scene {
            ui.label(RichText::new("keine Vorlage").weak());
        } else if planned {
            ui.label(RichText::new("✔ wird exportiert").color(pal.ok_text));
        } else if ev.enabled {
            ui.label(RichText::new("noch nicht dran").weak()).on_hover_text("Liegt außerhalb des Vorlaufs (Einstellungen bzw. Serien-Verknüpfung)");
        }
        if ev.is_from_churchtools() && !ev.overrides.is_empty()
            && ui.button("↺").on_hover_text("Alle Änderungen verwerfen, ChurchTools-Werte nutzen").clicked() {
                ev.overrides = Default::default();
                changed = true;
            }
    });

    if changed {
        if let Some(target) = app.store.data.event_mut(id) {
            *target = ev;
        }
        app.store.mark_dirty();
    }
}
