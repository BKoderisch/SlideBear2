//! Einstellungen: ChurchTools-Zugang, Kalender, Export-Ordner, wiederkehrende Veranstaltungen.

use eframe::egui::{self, RichText};

use crate::app::App;
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
            hidden(ui, app);
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
            .add_sized([440.0, crate::theme::FIELD_H], egui::TextEdit::singleline(&mut app.store.data.settings.ct_url).hint_text("https://gemeinde.church.tools").margin(egui::Margin::symmetric(12, 6)).vertical_align(egui::Align::Center))
            .changed();
        ui.end_row();

        ui.label("Login-Token");
        ui.horizontal(|ui| {
            crate::theme::text_field(ui, egui::TextEdit::singleline(&mut app.token).password(true), 320.0);
            if ui.button("Speichern").on_hover_text("Im Schlüsselbund des Systems sichern").clicked() {
                match Store::store_token(&app.token) {
                    Ok(()) => app.info("Token im Schlüsselbund gespeichert"),
                    Err(e) => app.error(format!("Token speichern fehlgeschlagen: {e}")),
                }
            }
        });
        ui.end_row();
        ui.label("");
        ui.label(RichText::new("ChurchTools › Profil › Login-Token. Der Token wird im System-Schlüsselbund gespeichert.").weak().small());
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

        ui.label("Begrüßung");
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label("Dienst");
                changed |= crate::theme::text_field(ui, egui::TextEdit::singleline(&mut app.store.data.settings.greet_service).hint_text("z. B. Präsi"), 220.0).changed();
                if ui.button("Jetzt prüfen").clicked() {
                    let ctx = ui.ctx().clone();
                    app.check_presenter(&ctx, true);
                }
            });
            ui.label(RichText::new("Wer am nächsten Sonntag diesen Dienst hat, wird beim Start vom Eisbären begrüßt. Es reicht ein Teil des Dienstnamens, „Präs“ findet z. B. „Präsi“ und „Präsentation“.").weak().small());
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
                && let Some(dir) = rfd::FileDialog::new().pick_folder()
            {
                s.export_dir = Some(dir);
                changed = true;
            }
            if let Some(dir) = &s.export_dir
                && ui.button("Öffnen").clicked()
            {
                let _ = open::that(dir);
            }
        });
        ui.end_row();

        ui.label("Vorlauf");
        ui.horizontal(|ui| {
            changed |= ui.add(egui::DragValue::new(&mut s.export_days).range(1..=365).suffix(" Tage")).changed();
            ui.label("(pro Veranstaltung in der Liste unten änderbar)");
        });
        ui.end_row();

        ui.label("");
        changed |= ui.checkbox(&mut s.export_on_sync, "Nach jedem Sync automatisch exportieren").changed();
        ui.end_row();
    });
    ui.label(
        RichText::new("Dateinamen: JJJJ-MM-TT_HHMM_titel.png. Die App löscht nur Dateien, die sie selbst angelegt hat.").weak().small(),
    );
    if changed {
        app.store.mark_dirty();
    }
}

fn links(ui: &mut egui::Ui, app: &mut App) {
    ui.heading("Wiederkehrende Veranstaltungen");
    if app.store.data.series.is_empty() {
        ui.label(RichText::new("Noch keine. Im Schnellexport bei einem ChurchTools-Termin „➕ Slide anlegen“ wählen.").weak());
        return;
    }
    let mut remove = None;
    let mut changed = false;
    egui::Grid::new("series").num_columns(6).striped(true).spacing([12.0, 6.0]).show(ui, |ui| {
        for h in ["Veranstaltung", "Slide", "Export", "Vorlauf", "Standard-Untertitel", ""] {
            ui.label(RichText::new(h).strong());
        }
        ui.end_row();
        for (i, s) in app.store.data.series.iter_mut().enumerate() {
            ui.label(&s.title);
            ui.label(if s.slide.is_some() { "✔" } else { "fehlt" });
            changed |= ui.checkbox(&mut s.enabled, "").changed();
            let mut custom = s.lead_days.is_some();
            ui.horizontal(|ui| {
                if ui.checkbox(&mut custom, "").changed() {
                    s.lead_days = custom.then_some(14);
                    changed = true;
                }
                if let Some(d) = &mut s.lead_days {
                    changed |= ui.add(egui::DragValue::new(d).range(1..=365).suffix(" Tage")).changed();
                } else {
                    ui.label(RichText::new("Standard").weak());
                }
            });
            changed |= crate::theme::text_field(ui, egui::TextEdit::singleline(&mut s.default_subtitle), 240.0).changed();
            if ui.button("🗑").on_hover_text("Veranstaltung samt ihrer Slide entfernen (die Termine bleiben)").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        app.store.data.series.remove(i);
        changed = true;
    }
    if changed {
        app.store.mark_dirty();
    }
}

fn hidden(ui: &mut egui::Ui, app: &mut App) {
    ui.heading("Ausgeblendete Termine");
    ui.label(RichText::new("ChurchTools-Termine, deren Titel einen dieser Begriffe enthält, werden nicht angezeigt und nicht exportiert (Groß-/Kleinschreibung egal).").weak().small());
    let mut changed = false;
    let mut remove = None;
    for (i, rule) in app.store.data.settings.hide_rules.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            changed |= crate::theme::text_field(ui, egui::TextEdit::singleline(rule).hint_text("z. B. Putzdienst"), 280.0).changed();
            if ui.button("🗑").on_hover_text("Filter entfernen").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        app.store.data.settings.hide_rules.remove(i);
        changed = true;
    }
    if ui.button("➕ Filter hinzufügen").clicked() {
        app.store.data.settings.hide_rules.push(String::new());
        changed = true;
    }

    ui.add_space(8.0);
    ui.label(RichText::new("Einzeln ausgeblendete Veranstaltungen").strong());
    let hidden: Vec<(uuid::Uuid, String)> = app.store.data.series.iter().filter(|s| s.hidden).map(|s| (s.id, s.title.clone())).collect();
    if hidden.is_empty() {
        ui.label(RichText::new("Keine. Ausblenden per Rechtsklick auf eine Zeile im Schnellexport oder unter Termine.").weak());
    }
    for (id, title) in hidden {
        ui.horizontal(|ui| {
            ui.label(&title);
            if ui.button("Einblenden").clicked() {
                if let Some(s) = app.store.data.series_mut(id) {
                    s.hidden = false;
                }
                changed = true;
            }
        });
    }
    if changed {
        app.store.mark_dirty();
    }
}

fn appearance(ui: &mut egui::Ui, app: &mut App) {
    ui.heading("Darstellung");
    ui.horizontal(|ui| {
        ui.label("Größe der Oberfläche");
        let current = app.store.data.settings.ui_scale;
        for (scale, label) in [(1.0, "Normal"), (1.2, "Groß"), (1.4, "Sehr groß")] {
            if ui.selectable_label((current - scale).abs() < 0.01, label).clicked() {
                app.store.data.settings.ui_scale = scale;
                crate::theme::apply(ui.ctx(), scale);
                app.store.mark_dirty();
            }
        }
    });
    ui.label(RichText::new("Auch per Cmd/Strg + Plus/Minus.").weak().small());
}
