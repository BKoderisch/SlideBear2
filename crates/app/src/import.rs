//! PPTX-Import-Dialog: Folien ansehen, Smart-Platzhalter prüfen, als Vorlage oder Termin übernehmen.

use std::path::Path;

use eframe::egui::{self, RichText, Vec2};
use slidebear_core::scene::{DEFAULT_HEIGHT, DEFAULT_WIDTH};
use slidebear_core::smart::{detect, Detection};
use slidebear_core::{Event, Template};
use slidebear_pptx::ImportedSlide;

use crate::app::{sample_fields, App};
use crate::store::Store;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Skip,
    Template,
    TemplateAndEvent,
}

struct Item {
    slide: ImportedSlide,
    detection: Detection,
    action: Action,
    smart: bool,
    name: String,
}

pub struct ImportDialog {
    file_name: String,
    items: Vec<Item>,
    refit: bool,
    aspect_differs: bool,
    pub missing_fonts: Vec<String>,
}

impl ImportDialog {
    pub fn open(path: &Path, store: &Store) -> anyhow::Result<Self> {
        let imp = slidebear_pptx::import_file(path)?;
        for (name, data) in &imp.assets {
            store.add_asset(name, data)?;
        }
        let file_name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let aspect_differs = imp.slides.iter().any(|s| s.scene.width * DEFAULT_HEIGHT != s.scene.height * DEFAULT_WIDTH);
        let items = imp
            .slides
            .into_iter()
            .map(|slide| {
                let detection = detect(&slide.scene);
                let name = detection.title.clone().unwrap_or_else(|| format!("{file_name} {}", slide.number));
                let action = if slide.hidden || slide.scene.elements.is_empty() { Action::Skip } else { Action::Template };
                Item { smart: !detection.is_empty(), detection, action, name, slide }
            })
            .collect();
        Ok(Self { file_name, items, refit: true, aspect_differs, missing_fonts: imp.fonts })
    }

    fn prepared(&self, item: &Item) -> Template {
        let mut scene = item.slide.scene.clone();
        if item.smart {
            item.detection.apply(&mut scene);
        }
        if self.refit && self.aspect_differs {
            scene.refit(DEFAULT_WIDTH, DEFAULT_HEIGHT);
        }
        let mut t = Template::new(item.name.clone(), scene);
        if item.smart {
            if let Some(p) = &item.detection.date_pattern {
                t.date_style.pattern = p.clone();
            }
            // Stand auf der Folie nur eine Startzeit („19:30 Uhr“), soll die Vorlage auch keine Endzeit zeigen
            if item.detection.start_time.is_some() {
                t.time_style.show_end = item.detection.end_time.is_some();
            }
        }
        t
    }

    /// Liefert `false`, wenn der Dialog geschlossen werden soll.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut App) -> bool {
        let mut open = true;
        let mut finished = false;
        egui::Window::new(format!("PPTX-Import: {}", self.file_name))
            .open(&mut open)
            .default_size([980.0, 720.0])
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.aspect_differs {
                        ui.checkbox(&mut self.refit, "An 16:9 (1920x1080) anpassen");
                    }
                    ui.label(RichText::new("Smart-Platzhalter ersetzen Titel, Datum, Uhrzeit und Ort durch Felder aus ChurchTools.").weak());
                });
                ui.separator();

                let preview_fields = sample_fields();
                egui::ScrollArea::vertical().max_height(ui.available_height() - 50.0).auto_shrink(false).show(ui, |ui| {
                    for i in 0..self.items.len() {
                        let tpl = self.prepared(&self.items[i]);
                        let item = &mut self.items[i];
                        ui.horizontal(|ui| {
                            let w = 320.0;
                            let size = Vec2::new(w, w * tpl.scene.height as f32 / tpl.scene.width as f32);
                            let ppp = ui.ctx().pixels_per_point();
                            // Original (ohne Platzhalter) und Ergebnis mit Beispieldaten
                            let original = Template::new("", item.slide.scene.clone());
                            let tex = app.previews.get(ui.ctx(), &mut app.renderer, &original.scene, &preview_fields, &original, w * ppp);
                            ui.add(egui::Image::new(&tex).fit_to_exact_size(size));
                            if item.smart && item.action != Action::Skip {
                                let tex = app.previews.get(ui.ctx(), &mut app.renderer, &tpl.scene, &preview_fields, &tpl, w * ppp);
                                ui.add(egui::Image::new(&tex).fit_to_exact_size(size)).on_hover_text("Vorschau mit Beispieldaten");
                            }
                            ui.vertical(|ui| {
                                ui.label(RichText::new(format!("Folie {}", item.slide.number)).strong());
                                ui.horizontal(|ui| {
                                    ui.selectable_value(&mut item.action, Action::Skip, "Überspringen");
                                    ui.selectable_value(&mut item.action, Action::Template, "Als Vorlage");
                                    let can_event = item.detection.date.is_some();
                                    ui.add_enabled_ui(can_event, |ui| {
                                        ui.selectable_value(&mut item.action, Action::TemplateAndEvent, "Vorlage + Termin")
                                            .on_disabled_hover_text("Kein Datum auf der Folie erkannt");
                                    });
                                });
                                if item.action != Action::Skip {
                                    ui.horizontal(|ui| {
                                        ui.label("Name");
                                        ui.text_edit_singleline(&mut item.name);
                                    });
                                    if !item.detection.is_empty() {
                                        ui.checkbox(&mut item.smart, "Smart-Platzhalter");
                                        if item.smart {
                                            for r in &item.detection.replacements {
                                                ui.label(RichText::new(format!("„{}“ → {}", r.before.replace('\n', " "), r.after)).small());
                                            }
                                        }
                                    }
                                }
                                for w in &item.slide.warnings {
                                    ui.label(RichText::new(format!("⚠ {w}")).small().color(ui.visuals().warn_fg_color));
                                }
                            });
                        });
                        ui.separator();
                    }
                });

                ui.horizontal(|ui| {
                    let n = self.items.iter().filter(|i| i.action != Action::Skip).count();
                    if ui.add_enabled(n > 0, egui::Button::new(format!("{n} Folien importieren"))).clicked() {
                        self.commit(app);
                        finished = true;
                    }
                    if ui.button("Abbrechen").clicked() {
                        finished = true;
                    }
                });
            });
        open && !finished
    }

    fn commit(&self, app: &mut App) {
        let mut templates = 0;
        let mut events = 0;
        for item in self.items.iter().filter(|i| i.action != Action::Skip) {
            let tpl = self.prepared(item);
            let tid = tpl.id;
            app.store.data.templates.push(tpl);
            templates += 1;
            if item.action == Action::TemplateAndEvent
                && let Some(fields) = item.detection.to_fields() {
                    let e = Event::manual(fields, Some(tid));
                    app.selected_event = Some(e.id);
                    app.store.data.events.push(e);
                    events += 1;
                }
        }
        app.store.mark_dirty();
        app.renderer.clear_cache();
        app.info(format!("Import: {templates} Vorlagen, {events} Termine angelegt"));
    }
}
