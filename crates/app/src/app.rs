//! Hauptfenster: Navigation, Statuszeile, Hintergrund-Jobs, Autosave.

use chrono::{Datelike, Duration, Local, NaiveDateTime, NaiveTime};
use eframe::egui::{self, RichText};
use slidebear_churchtools::Calendar;
use slidebear_core::{EventFields, Template};
use slidebear_render::Renderer;
use uuid::Uuid;

use crate::editor::{EditTarget, Editor};
use crate::events::EventForm;
use crate::import::ImportDialog;
use crate::jobs::{JobResult, Jobs};
use crate::preview::Previews;
use crate::store::Store;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Quick,
    Events,
    Templates,
    Settings,
}

pub struct App {
    pub store: Store,
    pub renderer: Renderer,
    pub previews: Previews,
    pub jobs: Jobs,
    pub view: View,
    status: String,
    status_error: bool,
    pub token: String,
    pub ct_calendars: Vec<Calendar>,
    pub font_families: Vec<String>,
    pub selected_event: Option<Uuid>,
    pub event_form: EventForm,
    pub editor: Option<Editor>,
    pub import: Option<ImportDialog>,
    pub show_past: bool,
    pub show_unlinked: bool,
    pub confirm_delete_template: Option<Uuid>,
    pub quick: crate::quick::QuickState,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, store: Store) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        crate::theme::apply(&cc.egui_ctx, store.data.settings.theme, store.data.settings.ui_scale);
        let renderer = Renderer::new(store.assets_dir());
        let font_families = renderer_families(&renderer);
        let quick_days = store.data.settings.export_days;
        let mut app = Self {
            store,
            renderer,
            previews: Previews::default(),
            jobs: Jobs::default(),
            view: View::Quick,
            status: String::new(),
            status_error: false,
            token: Store::load_token().unwrap_or_default(),
            ct_calendars: Vec::new(),
            font_families,
            selected_event: None,
            event_form: EventForm::default(),
            editor: None,
            import: None,
            show_past: false,
            show_unlinked: true,
            confirm_delete_template: None,
            quick: crate::quick::QuickState::new(quick_days),
        };
        if app.store.data.templates.is_empty() {
            app.store.data.templates.push(slidebear_core::presets::event_template("Standard", None));
            app.store.mark_dirty();
        }
        let s = &app.store.data.settings;
        if s.sync_on_start && app.can_sync() {
            app.start_sync(&cc.egui_ctx);
        }
        app
    }

    pub fn palette(&self) -> crate::theme::Palette {
        crate::theme::palette(self.store.data.settings.theme)
    }

    pub fn info(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_error = false;
    }

    pub fn error(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_error = true;
    }

    pub fn can_sync(&self) -> bool {
        let s = &self.store.data.settings;
        !s.ct_url.trim().is_empty() && !self.token.trim().is_empty() && !s.calendars.is_empty()
    }

    pub fn start_sync(&mut self, ctx: &egui::Context) {
        if !self.can_sync() {
            self.error("ChurchTools ist noch nicht eingerichtet (Einstellungen: URL, Token, Kalender)");
            return;
        }
        let s = &self.store.data.settings;
        self.jobs.sync(ctx, s.ct_url.clone(), self.token.clone(), s.calendars.clone(), s.sync_days);
        self.info("Synchronisiere mit ChurchTools …");
    }

    pub fn export_now(&mut self) {
        let Some(dir) = self.store.data.settings.export_dir.clone() else {
            self.error("Bitte zuerst in den Einstellungen einen Export-Ordner wählen");
            return;
        };
        match crate::export::run(&self.store.data, &mut self.renderer, &dir, Local::now().date_naive()) {
            Ok(r) => self.info(format!("Export nach {}: {r}", dir.display())),
            Err(e) => self.error(format!("Export fehlgeschlagen: {e}")),
        }
    }

    pub fn open_editor(&mut self, target: EditTarget) {
        let scene = match target {
            EditTarget::Template(id) => self.store.data.template(id).map(|t| t.scene.clone()),
            EditTarget::Event(id) => self.store.data.event(id).and_then(|e| {
                e.custom_scene.clone().or_else(|| {
                    e.template_id.and_then(|t| self.store.data.template(t)).map(|t| t.scene.clone())
                })
            }),
        };
        match scene {
            Some(scene) => self.editor = Some(Editor::new(target, scene)),
            None => self.error("Diese Slide hat noch keine Vorlage"),
        }
    }

    fn handle_jobs(&mut self, ctx: &egui::Context) {
        for r in self.jobs.poll() {
            match r {
                JobResult::Sync { calendars, from, to, result } => match result {
                    Ok(remote) => {
                        let data = &mut self.store.data;
                        let report = slidebear_core::sync::merge(
                            &mut data.events,
                            &remote,
                            &data.links,
                            &calendars,
                            from.and_time(NaiveTime::MIN),
                            to.and_time(NaiveTime::MIN),
                        );
                        self.store.mark_dirty();
                        self.event_form.invalidate();
                        self.info(format!(
                            "ChurchTools: {} Termine, {} neu, {} geändert, {} entfallen",
                            remote.len(),
                            report.added,
                            report.updated,
                            report.cancelled
                        ));
                        if self.store.data.settings.export_on_sync && self.store.data.settings.export_dir.is_some() {
                            self.export_now();
                        }
                    }
                    Err(e) => self.error(format!("Sync fehlgeschlagen: {e}")),
                },
                JobResult::Calendars(r) => match r {
                    Ok(c) => {
                        self.info(format!("{} Kalender geladen", c.len()));
                        self.ct_calendars = c;
                    }
                    Err(e) => self.error(format!("Kalender laden fehlgeschlagen: {e}")),
                },
                JobResult::Whoami(r) => match r {
                    Ok(name) => {
                        self.info(format!("Verbunden als {name}"));
                        let s = &self.store.data.settings;
                        self.jobs.calendars(ctx, s.ct_url.clone(), self.token.clone());
                    }
                    Err(e) => self.error(format!("Verbindung fehlgeschlagen: {e}")),
                },
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top").show(ui, |ui| {
            ui.horizontal(|ui| {
                crate::theme::bear_logo(ui, 34.0);
                ui.heading(RichText::new("SlideBear").strong());
                ui.separator();
                for (v, label) in [(View::Quick, "⚡ Schnellexport"), (View::Events, "📅 Termine"), (View::Templates, "🎨 Vorlagen"), (View::Settings, "⚙ Einstellungen")] {
                    if ui.selectable_label(self.view == v, label).clicked() {
                        self.view = v;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("⬆ Exportieren").on_hover_text("Alle anstehenden Slides als PNG in den Export-Ordner schreiben").clicked() {
                        self.export_now();
                    }
                    let sync = ui.add_enabled(self.jobs.running == 0, egui::Button::new("🔄 Sync"));
                    if sync.on_hover_text("Termine aus ChurchTools holen").clicked() {
                        let ctx = ui.ctx().clone();
                        self.start_sync(&ctx);
                    }
                    if ui.button("📥 PPTX importieren").clicked() {
                        self.pick_pptx();
                    }
                });
            });
        });
    }

    pub fn pick_pptx(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("PowerPoint", &["pptx"]).pick_file() else { return };
        match ImportDialog::open(&path, &self.store) {
            Ok(d) => {
                if !d.missing_fonts.is_empty() {
                    let fonts = &self.font_families;
                    let missing: Vec<_> = d.missing_fonts.iter().filter(|f| !fonts.iter().any(|g| g.eq_ignore_ascii_case(f))).cloned().collect();
                    if !missing.is_empty() {
                        self.info(format!("Nicht installierte Schriften werden durch Roboto ersetzt: {}", missing.join(", ")));
                    }
                }
                self.import = Some(d);
            }
            Err(e) => self.error(format!("Import fehlgeschlagen: {e}")),
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if self.jobs.running > 0 {
                    ui.spinner();
                }
                let text = RichText::new(&self.status);
                ui.label(if self.status_error { text.color(ui.visuals().error_fg_color) } else { text });
            });
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_jobs(&ctx);

        // Zoom per Cmd/Strg +/- merken
        let zoom = ctx.zoom_factor();
        if (zoom - self.store.data.settings.ui_scale).abs() > 0.01 {
            self.store.data.settings.ui_scale = zoom;
            self.store.mark_dirty();
        }

        if let Some(mut editor) = self.editor.take() {
            let keep = editor.show(ui, self);
            if keep {
                self.editor = Some(editor);
            }
        } else {
            self.top_bar(ui);
            self.status_bar(ui);
            match self.view {
                View::Quick => crate::quick::show(ui, self),
                View::Events => crate::events::show(ui, self),
                View::Templates => crate::templates::show(ui, self),
                View::Settings => crate::settings::show(ui, self),
            }
        }

        if let Some(mut dialog) = self.import.take()
            && dialog.show(&ctx, self) {
                self.import = Some(dialog);
            }

        if let Err(e) = self.store.autosave() {
            self.error(format!("Speichern fehlgeschlagen: {e}"));
        }
        if self.store.is_dirty() {
            // egui zeichnet nur bei Eingaben neu; so wird auch im Leerlauf gespeichert
            ctx.request_repaint_after(std::time::Duration::from_millis(1100));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.store.save();
    }
}

fn renderer_families(r: &Renderer) -> Vec<String> {
    r.font_library().families()
}

/// Beispieldaten für die Vorlagen-Vorschau: nächster Sonntag 10 Uhr.
pub fn sample_fields() -> EventFields {
    let today = Local::now().date_naive();
    let sunday = today + Duration::days((7 - today.weekday().num_days_from_sunday() as i64) % 7);
    let start = NaiveDateTime::new(sunday, NaiveTime::from_hms_opt(10, 0, 0).unwrap());
    EventFields {
        title: "Veranstaltung".into(),
        start,
        end: Some(start + Duration::minutes(90)),
        all_day: false,
        location: "Gemeindesaal".into(),
        subtitle: "Untertitel".into(),
    }
}

/// Vorlage für die Platzhalter-Auflösung (Fallback mit Standard-Formaten).
pub fn template_or_default(t: Option<&Template>) -> Template {
    t.cloned().unwrap_or_else(|| Template::new("", Default::default()))
}
