//! Einstellungen: ChurchTools-Zugang, Kalender, Export-Ordner, Serien-Verknüpfungen.

use eframe::egui::{self, RichText};

use crate::app::App;
use crate::theme::ThemeKind;
use crate::store::Store;

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.set_max_width(760.0);
            appearance(ui, app);
            ui.add_space(16.0);
            churchtools(ui, app);
            ui.add_space(16.0);
            export(ui, app);
            ui.add_space(16.0);
            links(ui, app);
            ui.add_space(16.0);
            ui.label(RichText::new(format!("Daten: {}", app.store.root().display())).weak().small());
        });
    });
}

fn churchtools(ui: &mut egui::Ui, app: &mut App) {
    ui.heading("ChurchTools");
    let mut changed = false;
    egui::Grid::new("ct").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("Adresse");
        changed |= ui
            .add(egui::TextEdit::singleline(&mut app.store.data.settings.ct_url).hint_text("https://gemeinde.church.tools").desired_width(400.0))
            .changed();
        ui.end_row();

        ui.label("Login-Token");
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut app.token).password(true).desired_width(300.0));
            if ui.button("Speichern").on_hover_text("Im Schlüsselbund des Systems sichern").clicked() {
                match Store::store_token(&app.token) {
                    Ok(()) => app.info("Token im Schlüsselbund gespeichert"),
                    Err(e) => app.error(format!("Token speichern fehlgeschlagen: {e}")),
                }
            }
        });
        ui.end_row();
        ui.label("");
        ui.label(RichText::new("ChurchTools → Profil → Login-Token. Der Token wird im System-Schlüsselbund gespeichert.").weak().small());
        ui.end_row();

        ui.label("");
        ui.horizontal(|ui| {
            let ctx = ui.ctx().clone();
            let url = app.store.data.settings.ct_url.clone();
            if ui.button("Verbindung testen").clicked() {
                app.jobs.whoami(&ctx, url.clone(), app.token.clone());
            }
            if ui.button("Kalender laden").clicked() {
                app.jobs.calendars(&ctx, url, app.token.clone());
            }
        });
        ui.end_row();

        ui.label("Kalender");
        ui.vertical(|ui| {
            if app.ct_calendars.is_empty() {
                let n = app.store.data.settings.calendars.len();
                ui.label(RichText::new(format!("{n} ausgewählt. „Kalender laden“, um die Auswahl zu ändern.")).weak());
            }
            for c in app.ct_calendars.clone() {
                let sel = &mut app.store.data.settings.calendars;
                let mut on = sel.contains(&c.id);
                if ui.checkbox(&mut on, &c.name).changed() {
                    if on {
                        sel.push(c.id);
                    } else {
                        sel.retain(|x| *x != c.id);
                    }
                    changed = true;
                }
            }
        });
        ui.end_row();

        let s = &mut app.store.data.settings;
        ui.label("Zeitraum");
        ui.horizontal(|ui| {
            changed |= ui.add(egui::DragValue::new(&mut s.sync_days).range(7..=730).suffix(" Tage")).changed();
            ui.label("im Voraus holen");
        });
        ui.end_row();

        ui.label("");
        changed |= ui.checkbox(&mut s.sync_on_start, "Beim Start automatisch synchronisieren").changed();
        ui.end_row();
    });
    if changed {
        app.store.mark_dirty();
    }
}

fn export(ui: &mut egui::Ui, app: &mut App) {
    ui.heading("Export für ProPresenter");
    let mut changed = false;
    let s = &mut app.store.data.settings;
    egui::Grid::new("export").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("Ordner");
        ui.horizontal(|ui| {
            let label = s.export_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "nicht gewählt".into());
            ui.label(label);
            if ui.button("Wählen …").clicked()
                && let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    s.export_dir = Some(dir);
                    changed = true;
                }
            if let Some(dir) = &s.export_dir
                && ui.button("Öffnen").clicked() {
                    let _ = open::that(dir);
                }
        });
        ui.end_row();

        ui.label("Vorlauf");
        ui.horizontal(|ui| {
            changed |= ui.add(egui::DragValue::new(&mut s.export_days).range(1..=365).suffix(" Tage")).changed();
            ui.label("(pro Serie in der Liste unten änderbar)");
        });
        ui.end_row();

        ui.label("");
        changed |= ui.checkbox(&mut s.export_on_sync, "Nach jedem Sync automatisch exportieren").changed();
        ui.end_row();
    });
    ui.label(
        RichText::new("Dateinamen: JJJJ-MM-TT_HHMM_titel.png. Die App löscht nur Dateien, die sie selbst angelegt hat.")
            .weak()
            .small(),
    );
    if changed {
        app.store.mark_dirty();
    }
}

fn links(ui: &mut egui::Ui, app: &mut App) {
    ui.heading("Serien-Verknüpfungen");
    if app.store.data.links.is_empty() {
        ui.label(RichText::new("Noch keine. Bei einem ChurchTools-Termin „Vorlage für die ganze Serie übernehmen“ wählen.").weak());
        return;
    }
    let mut remove = None;
    let mut changed = false;
    let templates: Vec<(uuid::Uuid, String)> = app.store.data.templates.iter().map(|t| (t.id, t.name.clone())).collect();
    egui::Grid::new("links").num_columns(5).striped(true).spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label(RichText::new("Serie").strong());
        ui.label(RichText::new("Vorlage").strong());
        ui.label(RichText::new("Vorlauf").strong());
        ui.label(RichText::new("Standard-Untertitel").strong());
        ui.label("");
        ui.end_row();
        for (i, l) in app.store.data.links.iter_mut().enumerate() {
            ui.label(&l.title);
            let current = templates.iter().find(|(id, _)| *id == l.template_id).map(|(_, n)| n.clone()).unwrap_or_default();
            egui::ComboBox::from_id_salt(("link_tpl", i)).selected_text(current).show_ui(ui, |ui| {
                for (id, name) in &templates {
                    changed |= ui.selectable_value(&mut l.template_id, *id, name).changed();
                }
            });
            let mut custom = l.lead_days.is_some();
            ui.horizontal(|ui| {
                if ui.checkbox(&mut custom, "").changed() {
                    l.lead_days = custom.then_some(14);
                    changed = true;
                }
                if let Some(d) = &mut l.lead_days {
                    changed |= ui.add(egui::DragValue::new(d).range(1..=365).suffix(" Tage")).changed();
                } else {
                    ui.label(RichText::new("Standard").weak());
                }
            });
            changed |= ui.add(egui::TextEdit::singleline(&mut l.default_subtitle).desired_width(200.0)).changed();
            if ui.button("🗑").on_hover_text("Verknüpfung entfernen").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        app.store.data.links.remove(i);
        changed = true;
    }
    if changed {
        app.store.mark_dirty();
    }
}

fn appearance(ui: &mut egui::Ui, app: &mut App) {
    ui.heading("Darstellung");
    ui.horizontal(|ui| {
        ui.label("Theme");
        let current = app.store.data.settings.theme;
        for (kind, label) in [(ThemeKind::Polarnacht, "❄ Polarnacht"), (ThemeKind::Schnee, "☃ Schnee")] {
            if ui.selectable_label(current == kind, label).clicked() {
                app.store.data.settings.theme = kind;
                crate::theme::apply(ui.ctx(), kind, app.store.data.settings.ui_scale);
                app.store.mark_dirty();
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Größe der Oberfläche");
        let current = app.store.data.settings.ui_scale;
        for (scale, label) in [(1.0, "Normal"), (1.2, "Groß"), (1.4, "Sehr groß")] {
            if ui.selectable_label((current - scale).abs() < 0.01, label).clicked() {
                app.store.data.settings.ui_scale = scale;
                crate::theme::apply(ui.ctx(), app.store.data.settings.theme, scale);
                app.store.mark_dirty();
            }
        }
    });
    ui.label(RichText::new("Auch per Cmd/Strg + Plus/Minus.").weak().small());
}
