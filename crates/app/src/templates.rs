//! Vorlagen-Galerie: anlegen, duplizieren, umbenennen, Datums-/Zeitformat, Editor öffnen.

use eframe::egui::{self, RichText, Vec2};
use slidebear_core::{presets, Scene, Template};
use uuid::Uuid;

use crate::app::{sample_fields, App};
use crate::editor::EditTarget;

const DATE_PATTERNS: [(&str, &str); 5] = [
    ("%d.%m.%Y", "07.12.2024"),
    ("%d.%m.%y", "07.12.24"),
    ("%-d.%-m.%Y", "7.12.2024"),
    ("%d.%m.", "07.12."),
    ("%Y-%m-%d", "2024-12-07"),
];

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Vorlagen");
            ui.add_space(16.0);
            if ui.button("➕ Standard-Layout").on_hover_text("Titel, Infozeile und Untertitel, zentriert").clicked() {
                add(app, presets::event_template("Neue Vorlage", None));
            }
            if ui.button("➕ Leere Vorlage").clicked() {
                add(app, Template::new("Leere Vorlage", Scene::default()));
            }
            if ui.button("🖼 Aus Bild …").on_hover_text("Standard-Layout mit eigenem Hintergrundbild").clicked()
                && let Some(path) = rfd::FileDialog::new().add_filter("Bilder", &["png", "jpg", "jpeg", "webp"]).pick_file() {
                    match app.store.import_asset_file(&path) {
                        Ok(asset) => {
                            let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Vorlage".into());
                            add(app, presets::event_template(&name, Some(&asset)));
                        }
                        Err(e) => app.error(e.to_string()),
                    }
                }
            if ui.button("📥 Aus PPTX …").clicked() {
                app.pick_pptx();
            }
        });
        ui.separator();

        let fields = sample_fields();
        let card_w = 300.0;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let ids: Vec<Uuid> = app.store.data.templates.iter().map(|t| t.id).collect();
                for id in ids {
                    card(ui, app, id, &fields, card_w);
                }
            });
        });
    });

    if let Some(id) = app.confirm_delete_template {
        let used = app.store.data.events.iter().filter(|e| e.template_id == Some(id)).count();
        let name = app.store.data.template(id).map(|t| t.name.clone()).unwrap_or_default();
        let modal = egui::Modal::new(egui::Id::new("del_tpl")).show(ui.ctx(), |ui| {
            ui.heading(format!("Vorlage „{name}“ löschen?"));
            if used > 0 {
                ui.label(format!("{used} Termine verwenden diese Vorlage und haben danach keine Slide mehr."));
            }
            ui.horizontal(|ui| {
                if ui.button("Löschen").clicked() {
                    app.store.data.templates.retain(|t| t.id != id);
                    app.store.data.links.retain(|l| l.template_id != id);
                    for e in &mut app.store.data.events {
                        if e.template_id == Some(id) {
                            e.template_id = None;
                        }
                    }
                    app.store.mark_dirty();
                    app.confirm_delete_template = None;
                }
                if ui.button("Abbrechen").clicked() {
                    app.confirm_delete_template = None;
                }
            });
        });
        if modal.should_close() {
            app.confirm_delete_template = None;
        }
    }
}

fn add(app: &mut App, t: Template) {
    let id = t.id;
    app.store.data.templates.push(t);
    app.store.mark_dirty();
    app.open_editor(EditTarget::Template(id));
}

fn card(ui: &mut egui::Ui, app: &mut App, id: Uuid, fields: &slidebear_core::EventFields, w: f32) {
    let Some(t) = app.store.data.template(id).cloned() else { return };
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(w);
        ui.vertical(|ui| {
            let size = Vec2::new(w, w * t.scene.height as f32 / t.scene.width as f32);
            let ppp = ui.ctx().pixels_per_point();
            let tex = app.previews.get(ui.ctx(), &mut app.renderer, &t.scene, fields, &t, size.x * ppp);
            let img = ui.add(egui::Image::new(&tex).fit_to_exact_size(size).sense(egui::Sense::click()));
            if img.double_clicked() {
                app.open_editor(EditTarget::Template(id));
            }

            let mut name = t.name.clone();
            if ui.add(egui::TextEdit::singleline(&mut name).desired_width(w).font(egui::TextStyle::Heading)).changed() {
                if let Some(t) = app.store.data.template_mut(id) {
                    t.name = name;
                }
                app.store.mark_dirty();
            }

            let used = app.store.data.events.iter().filter(|e| e.template_id == Some(id)).count();
            let links = app.store.data.links.iter().filter(|l| l.template_id == id).count();
            ui.label(RichText::new(format!("{used} Termine · {links} Serien")).weak());

            let mut tpl = t.clone();
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Datum");
                let current = DATE_PATTERNS.iter().find(|(p, _)| *p == tpl.date_style.pattern).map(|(_, ex)| *ex).unwrap_or("eigenes");
                egui::ComboBox::from_id_salt(("date", id)).selected_text(current).show_ui(ui, |ui| {
                    for (p, ex) in DATE_PATTERNS {
                        if ui.selectable_label(tpl.date_style.pattern == p, ex).clicked() {
                            tpl.date_style.pattern = p.into();
                            changed = true;
                        }
                    }
                });
            });
            ui.horizontal(|ui| {
                changed |= ui.checkbox(&mut tpl.time_style.omit_zero_minutes, "10 statt 10:00").changed();
                changed |= ui.checkbox(&mut tpl.time_style.show_end, "Endzeit").changed();
            });
            ui.horizontal(|ui| {
                ui.label("Zeit-Zusatz");
                changed |= ui.add(egui::TextEdit::singleline(&mut tpl.time_style.suffix).desired_width(60.0)).changed();
            });
            if changed {
                if let Some(t) = app.store.data.template_mut(id) {
                    t.date_style = tpl.date_style;
                    t.time_style = tpl.time_style;
                }
                app.store.mark_dirty();
            }

            ui.horizontal(|ui| {
                if ui.button("✏ Bearbeiten").clicked() {
                    app.open_editor(EditTarget::Template(id));
                }
                if ui.button("⧉ Duplizieren").clicked() {
                    let mut copy = t.clone();
                    copy.id = Uuid::new_v4();
                    copy.name = format!("{} Kopie", t.name);
                    for e in &mut copy.scene.elements {
                        e.id = Uuid::new_v4();
                    }
                    app.store.data.templates.push(copy);
                    app.store.mark_dirty();
                }
                if ui.button("🗑").on_hover_text("Löschen").clicked() {
                    app.confirm_delete_template = Some(id);
                }
            });
        });
    });
}
