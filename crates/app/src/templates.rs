//! Layout-Galerie: Ausgangspunkte für neue Slides. Eine Slide ist eine Kopie, Änderungen am
//! Layout wirken sich auf bestehende Slides nicht aus.

use eframe::egui::{self, RichText, Vec2};
use slidebear_core::{Scene, Template, presets};
use uuid::Uuid;

use crate::app::{App, sample_fields};
use crate::editor::EditTarget;

use crate::editor::DATE_PATTERNS;

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Layouts");
            ui.add_space(16.0);
            if ui.button("➕ Standard-Layout").on_hover_text("Titel, Infozeile und Untertitel, zentriert").clicked() {
                add(app, presets::event_template("Neues Layout", None));
            }
            if ui.button("➕ Leeres Layout").clicked() {
                add(app, Template::new("Leeres Layout", Scene::default()));
            }
            if ui.button("🖼 Aus Bild …").on_hover_text("Standard-Layout mit eigenem Hintergrundbild").clicked()
                && let Some(path) = rfd::FileDialog::new().add_filter("Bilder", &["png", "jpg", "jpeg", "webp"]).pick_file()
            {
                match app.store.import_asset_file(&path) {
                    Ok(asset) => {
                        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Layout".into());
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
        let name = app.store.data.template(id).map(|t| t.name.clone()).unwrap_or_default();
        let modal = egui::Modal::new(egui::Id::new("del_tpl")).show(ui.ctx(), |ui| {
            ui.heading(format!("Layout „{name}“ löschen?"));
            ui.label("Bestehende Slides bleiben erhalten, sie sind eigenständige Kopien.");
            ui.horizontal(|ui| {
                if ui.button("Löschen").clicked() {
                    app.store.data.templates.retain(|t| t.id != id);
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
    let Some(t) = app.store.data.template(id).cloned() else {
        return;
    };
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(w);
        ui.vertical(|ui| {
            let size = Vec2::new(w, w * t.scene.height as f32 / t.scene.width as f32);
            let ppp = ui.ctx().pixels_per_point();
            let tex = app.previews.get(ui.ctx(), &mut app.renderer, t.as_slide_ref(), fields, size.x * ppp);
            let img = ui.add(egui::Image::new(&tex).fit_to_exact_size(size).sense(egui::Sense::click()));
            if img.double_clicked() {
                app.open_editor(EditTarget::Template(id));
            }

            let mut name = t.name.clone();
            if ui
                .add_sized(
                    [w, 46.0],
                    egui::TextEdit::singleline(&mut name).font(egui::TextStyle::Heading).margin(egui::Margin::symmetric(12, 6)),
                )
                .changed()
            {
                if let Some(t) = app.store.data.template_mut(id) {
                    t.name = name;
                }
                app.store.mark_dirty();
            }

            ui.label(RichText::new("Formate für neue Slides aus diesem Layout:").weak().small());

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
                changed |= crate::theme::text_field(ui, egui::TextEdit::singleline(&mut tpl.time_style.suffix), 90.0).changed();
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
                if ui.button("📋 Duplizieren").clicked() {
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
