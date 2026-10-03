//! PPTX-Import-Dialog: Folien ansehen, Smart-Platzhalter prüfen, als Layout oder Termin übernehmen.

use std::path::Path;

use eframe::egui::{self, RichText, Vec2};
use slidebear_core::scene::{DEFAULT_HEIGHT, DEFAULT_WIDTH};
use slidebear_core::smart::{Detection, detect};
use slidebear_core::{Event, Slide, Template};
use slidebear_pptx::ImportedSlide;

use crate::app::{App, SlideOwner, sample_fields};
use crate::store::Store;
use slidebear_core::EventFields;

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
    /// Gesetzt, wenn eine Folie als Slide eines bestimmten Termins gewählt werden soll.
    target: Option<Target>,
}

struct Target {
    owner: SlideOwner,
    title: String,
    fields: EventFields,
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
        Ok(Self { file_name, items, refit: true, aspect_differs, missing_fonts: imp.fonts, target: None })
    }

    /// Modus „Folie für diesen Termin wählen“: Vorschau mit den echten Termindaten.
    pub fn for_target(mut self, owner: SlideOwner, title: String, fields: EventFields) -> Self {
        self.target = Some(Target { owner, title, fields });
        self
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
            // Stand auf der Folie nur eine Startzeit („19:30 Uhr“), soll auch die Slide keine Endzeit zeigen
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
        let mut chosen: Option<usize> = None;
        let window_title = match &self.target {
            Some(t) => format!("PPTX für „{}“: {}", t.title, self.file_name),
            None => format!("PPTX-Import: {}", self.file_name),
        };
        egui::Window::new(window_title).open(&mut open).default_size([980.0, 720.0]).collapsible(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.aspect_differs {
                    ui.checkbox(&mut self.refit, "An 16:9 (1920x1080) anpassen");
                }
                ui.label(RichText::new("Smart-Platzhalter ersetzen Titel, Datum, Uhrzeit und Ort durch Felder aus ChurchTools.").weak());
            });
            ui.separator();

            if self.target.is_some() {
                ui.label(
                    RichText::new("Wähle die Folie für diesen Termin. Datum, Uhrzeit und Titel kommen danach automatisch aus ChurchTools.")
                        .strong(),
                );
            }
            let preview_fields = self.target.as_ref().map(|t| t.fields.clone()).unwrap_or_else(sample_fields);
            let for_target = self.target.is_some();
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
                        let tex = app.previews.get(ui.ctx(), &mut app.renderer, original.as_slide_ref(), &preview_fields, w * ppp);
                        ui.add(egui::Image::new(&tex).fit_to_exact_size(size));
                        if item.smart && (for_target || item.action != Action::Skip) {
                            let tex = app.previews.get(ui.ctx(), &mut app.renderer, tpl.as_slide_ref(), &preview_fields, w * ppp);
                            ui.add(egui::Image::new(&tex).fit_to_exact_size(size)).on_hover_text("Vorschau mit Beispieldaten");
                        }
                        ui.vertical(|ui| {
                            ui.label(RichText::new(format!("Folie {}", item.slide.number)).strong());
                            if for_target {
                                let usable = !item.slide.scene.elements.is_empty();
                                if ui.add_enabled(usable, egui::Button::new("✔ Diese Folie verwenden")).clicked() {
                                    chosen = Some(i);
                                }
                                if !item.detection.is_empty() {
                                    ui.checkbox(&mut item.smart, "Smart-Platzhalter");
                                    if item.smart {
                                        for r in &item.detection.replacements {
                                            ui.label(RichText::new(format!("„{}“ › {}", r.before.replace('\n', " "), r.after)).small());
                                        }
                                    }
                                }
                            }
                            if !for_target {
                                ui.horizontal(|ui| {
                                    ui.selectable_value(&mut item.action, Action::Skip, "Überspringen");
                                    ui.selectable_value(&mut item.action, Action::Template, "Als Layout");
                                    let can_event = item.detection.date.is_some();
                                    ui.add_enabled_ui(can_event, |ui| {
                                        ui.selectable_value(&mut item.action, Action::TemplateAndEvent, "Als Termin mit Slide")
                                            .on_disabled_hover_text("Kein Datum auf der Folie erkannt");
                                    });
                                });
                                if item.action != Action::Skip {
                                    ui.horizontal(|ui| {
                                        ui.label("Name");
                                        crate::theme::text_field(ui, egui::TextEdit::singleline(&mut item.name), 300.0);
                                    });
                                    if !item.detection.is_empty() {
                                        ui.checkbox(&mut item.smart, "Smart-Platzhalter");
                                        if item.smart {
                                            for r in &item.detection.replacements {
                                                ui.label(RichText::new(format!("„{}“ › {}", r.before.replace('\n', " "), r.after)).small());
                                            }
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
                if !for_target && ui.add_enabled(n > 0, egui::Button::new(format!("{n} Folien importieren"))).clicked() {
                    self.commit(app);
                    finished = true;
                }
                if ui.button("Abbrechen").clicked() {
                    finished = true;
                }
            });
        });
        if let (Some(i), Some(t)) = (chosen, &self.target) {
            let tpl = self.prepared(&self.items[i]);
            let slide = Slide { scene: tpl.scene, date_style: tpl.date_style, time_style: tpl.time_style };
            let title = t.title.clone();
            app.set_slide(&t.owner, slide);
            app.renderer.clear_cache();
            app.info(format!("Folie {} ist jetzt die Slide für „{title}“.", self.items[i].slide.number));
            finished = true;
        }
        open && !finished
    }

    fn commit(&self, app: &mut App) {
        let mut templates = 0;
        let mut events = 0;
        for item in self.items.iter().filter(|i| i.action != Action::Skip) {
            let tpl = self.prepared(item);
            match item.action {
                Action::TemplateAndEvent => {
                    let Some(fields) = item.detection.to_fields() else {
                        continue;
                    };
                    let slide = Slide { scene: tpl.scene, date_style: tpl.date_style, time_style: tpl.time_style };
                    let e = Event::manual(fields, Some(slide));
                    app.selected_event = Some(e.id);
                    app.store.data.events.push(e);
                    events += 1;
                }
                _ => {
                    app.store.data.templates.push(tpl);
                    templates += 1;
                }
            }
        }
        app.store.mark_dirty();
        app.renderer.clear_cache();
        app.info(format!("Import: {templates} Layouts, {events} Termine angelegt"));
    }
}
