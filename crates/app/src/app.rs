//! Hauptfenster: Navigation, Statuszeile, Hintergrund-Jobs, Autosave.

use chrono::{Datelike, Duration, Local, NaiveDateTime, NaiveTime};
use eframe::egui::{self, RichText};
use slidebear_churchtools::Calendar;
use slidebear_core::export::{group_key, GroupKey};
use slidebear_core::series_edit::{apply_to_all, reset_all, Field};
use slidebear_core::{Event, EventFields, Series, Slide};
use slidebear_render::Renderer;
use uuid::Uuid;

use crate::editor::{EditTarget, Editor};
use crate::events::EventForm;
use crate::import::ImportDialog;
use crate::jobs::{JobResult, Jobs};
use crate::preview::Previews;
use crate::store::Store;
use crate::theme;

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
    pub show_hidden: bool,
    pub confirm_delete_template: Option<Uuid>,
    pub quick: crate::quick::QuickState,
    /// Ob Änderungen an Serienterminen nur für diesen oder für alle Termine gelten.
    pub edit_scope: EditScope,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, store: Store) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        crate::theme::install(&cc.egui_ctx, store.data.settings.ui_scale);
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
            show_hidden: false,
            confirm_delete_template: None,
            quick: crate::quick::QuickState::new(quick_days),
            edit_scope: EditScope::This,
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
        let d = &self.store.data;
        let slide = match target {
            EditTarget::Template(id) => d.template(id).map(|t| Slide { scene: t.scene.clone(), date_style: t.date_style.clone(), time_style: t.time_style.clone() }),
            EditTarget::Series(id) => d.series(id).and_then(|s| s.slide.clone()),
            EditTarget::Event(id) => d.event(id).and_then(|e| e.slide.clone()),
        };
        match slide {
            Some(slide) => self.editor = Some(Editor::new(target, slide)),
            None => self.error("Hier gibt es noch keine Slide. Bitte erst eine anlegen."),
        }
    }

    /// Wem die Slide eines Termins gehört: der Serie, dem (noch unverknüpften) ChurchTools-Termin
    /// oder dem einzelnen Termin selbst.
    pub fn owner_of(&self, event: &Event) -> SlideOwner {
        match group_key(event, &self.store.data.series) {
            GroupKey::Series(id) => SlideOwner::Series(id),
            GroupKey::Appointment { calendar_id, appointment_id } => {
                SlideOwner::Appointment { calendar_id, appointment_id, title: event.base.title.clone() }
            }
            GroupKey::Single(id) => SlideOwner::Event(id),
        }
    }

    pub fn has_slide(&self, owner: &SlideOwner) -> bool {
        let d = &self.store.data;
        match owner {
            SlideOwner::Series(id) => d.series(*id).is_some_and(|s| s.slide.is_some()),
            SlideOwner::Appointment { .. } => false,
            SlideOwner::Event(id) => d.event(*id).is_some_and(|e| e.slide.is_some()),
        }
    }

    pub fn edit_slide(&mut self, owner: &SlideOwner) {
        match owner {
            SlideOwner::Series(id) => self.open_editor(EditTarget::Series(*id)),
            SlideOwner::Event(id) => self.open_editor(EditTarget::Event(*id)),
            SlideOwner::Appointment { .. } => self.error("Hier gibt es noch keine Slide. Bitte erst eine anlegen."),
        }
    }

    /// Serie eines ChurchTools-Termins, wird bei Bedarf angelegt (ohne Slide).
    fn ensure_series(&mut self, e: &Event) -> Option<Uuid> {
        if let Some(s) = Series::find(&self.store.data.series, e) {
            return Some(s.id);
        }
        let slidebear_core::EventSource::ChurchTools { calendar_id, appointment_id, .. } = e.source else { return None };
        let s = Series::new(calendar_id, appointment_id, &e.base.title, None);
        let id = s.id;
        self.store.data.series.push(s);
        Some(id)
    }

    /// Blendet die Veranstaltung eines ChurchTools-Termins aus (alle Termine, auch künftige).
    pub fn set_hidden(&mut self, e: &Event, hidden: bool) {
        let title = e.base.title.clone();
        let Some(sid) = self.ensure_series(e) else { return };
        if let Some(s) = self.store.data.series_mut(sid) {
            s.hidden = hidden;
        }
        self.store.mark_dirty();
        if hidden {
            self.info(format!("„{title}“ ausgeblendet. Unter Einstellungen → Ausgeblendete Termine wieder einblendbar."));
        } else {
            self.info(format!("„{title}“ wird wieder angezeigt."));
        }
    }

    /// Übernimmt eine bearbeitete Kopie eines Termins. Bei „alle Termine“ werden die geänderten
    /// Felder (außer dem Datum) auf die ganze Serie übertragen.
    pub fn commit_event(&mut self, edited: Event) {
        let Some(before) = self.store.data.event(edited.id).map(|e| e.fields()) else { return };
        if let Some(target) = self.store.data.event_mut(edited.id) {
            *target = edited.clone();
        }
        if self.edit_scope == EditScope::All
            && let Some(sid) = self.ensure_series(&edited)
        {
            let d = &mut self.store.data;
            if let Some(idx) = d.series.iter().position(|s| s.id == sid) {
                let mut s = d.series[idx].clone();
                apply_to_all(&before, &edited, &mut d.events, &mut s);
                d.series[idx] = s;
            }
        }
        self.store.mark_dirty();
    }

    /// Setzt ein Feld auf den ChurchTools-Wert zurück, je nach Umschalter für diesen oder alle Termine.
    pub fn reset_field(&mut self, id: Uuid, field: Field) {
        let Some(e) = self.store.data.event(id).cloned() else { return };
        let series_idx = Series::find(&self.store.data.series, &e).and_then(|s| self.store.data.series.iter().position(|x| x.id == s.id));
        match (self.edit_scope, series_idx) {
            (EditScope::All, Some(idx)) => {
                let d = &mut self.store.data;
                let mut s = d.series[idx].clone();
                reset_all(field, &mut d.events, &mut s);
                d.series[idx] = s;
            }
            _ => {
                if let Some(e) = self.store.data.event_mut(id) {
                    let o = &mut e.overrides;
                    match field {
                        Field::Title => o.title = None,
                        Field::Location => o.location = None,
                        Field::Subtitle => o.subtitle = None,
                        Field::Time => {
                            o.start = None;
                            o.end = None;
                            o.all_day = None;
                        }
                    }
                }
            }
        }
        self.store.mark_dirty();
    }

    /// Legt eine neue Slide als Kopie des Layouts an (ersetzt eine vorhandene) und öffnet den Editor.
    pub fn new_slide(&mut self, owner: &SlideOwner, layout: Uuid) {
        let Some(slide) = self.store.data.template(layout).map(Slide::from_layout) else { return };
        let target = match owner {
            SlideOwner::Series(id) => {
                let Some(s) = self.store.data.series_mut(*id) else { return };
                s.slide = Some(slide);
                EditTarget::Series(*id)
            }
            SlideOwner::Appointment { calendar_id, appointment_id, title } => {
                let s = Series::new(*calendar_id, *appointment_id, title, Some(slide));
                let id = s.id;
                self.store.data.series.push(s);
                EditTarget::Series(id)
            }
            SlideOwner::Event(id) => {
                let Some(e) = self.store.data.event_mut(*id) else { return };
                e.slide = Some(slide);
                EditTarget::Event(*id)
            }
        };
        self.store.mark_dirty();
        self.open_editor(target);
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
                            &data.series,
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
        let frame = egui::Frame::new().fill(theme::SNOW).inner_margin(egui::Margin::symmetric(14, 10)).stroke(egui::Stroke::new(3.0, theme::INK));
        egui::Panel::top("top").frame(frame).show(ui, |ui| {
            theme::snowflakes(ui, ui.max_rect());
            ui.horizontal(|ui| {
                theme::bear_logo(ui, 52.0);
                theme::comic_title(ui, "SlideBear", 34.0);
                ui.add_space(12.0);
                for (v, label) in [(View::Quick, "⚡ Schnellexport"), (View::Events, "📅 Termine"), (View::Templates, "🎨 Layouts"), (View::Settings, "⚙ Einstellungen")] {
                    if ui.selectable_label(self.view == v, label).clicked() {
                        self.view = v;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let export = egui::Button::new(RichText::new("⬆ Exportieren").color(theme::SNOW)).fill(theme::GLACIER);
                    if theme::comic_button(ui, export).on_hover_text("Alle anstehenden Slides als PNG in den Export-Ordner schreiben").clicked() {
                        self.export_now();
                    }
                    ui.add_space(4.0);
                    let sync = ui.add_enabled_ui(self.jobs.running == 0, |ui| theme::comic_button(ui, egui::Button::new("🔄 Sync"))).inner;
                    if sync.on_hover_text("Termine aus ChurchTools holen").clicked() {
                        let ctx = ui.ctx().clone();
                        self.start_sync(&ctx);
                    }
                    ui.add_space(4.0);
                    if theme::comic_button(ui, egui::Button::new("📥 PPTX importieren")).clicked() {
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
        // Statuszeile als Comic-Sprechblase des Eisbären
        egui::Panel::bottom("status").frame(egui::Frame::new().fill(theme::ICE).inner_margin(egui::Margin::symmetric(12, 8))).show(ui, |ui| {
            ui.horizontal(|ui| {
                theme::bear_logo(ui, 30.0);
                let bubble = egui::Frame::new()
                    .fill(if self.status_error { theme::BADGE_CANCELLED } else { theme::SNOW })
                    .stroke(egui::Stroke::new(2.5, theme::INK))
                    .corner_radius(egui::CornerRadius::same(16))
                    .inner_margin(egui::Margin::symmetric(12, 4));
                bubble.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if self.jobs.running > 0 {
                            ui.spinner();
                        }
                        let text = if self.status.is_empty() { "Hallo! Alles bereit für Sonntag." } else { self.status.as_str() };
                        ui.label(RichText::new(text).color(theme::INK));
                    });
                });
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditScope {
    This,
    All,
}

/// Umschalter „Änderungen gelten für: nur diesen Termin / alle Termine der Serie“.
pub fn scope_toggle(ui: &mut egui::Ui, scope: &mut EditScope, this_label: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Änderungen gelten für").strong());
        ui.selectable_value(scope, EditScope::This, this_label);
        ui.selectable_value(scope, EditScope::All, "🔁 alle Termine der Serie");
    });
    if *scope == EditScope::All {
        ui.label(RichText::new("Titel, Uhrzeit, Ort und Untertitel gelten dann für jeden Termin der Serie, auch für künftige. Das Datum ändert sich immer nur beim einzelnen Termin.").small().weak());
    }
}

/// Besitzer einer Slide.
#[derive(Debug, Clone, PartialEq)]
pub enum SlideOwner {
    Series(Uuid),
    /// ChurchTools-Termin ohne Serie: beim Anlegen der Slide entsteht die Serie.
    Appointment { calendar_id: i64, appointment_id: i64, title: String },
    Event(Uuid),
}

/// Buttons „Slide bearbeiten“ und „Neue Slide aus Layout“. Liefert `true`, wenn etwas geöffnet wurde.
pub fn slide_buttons(ui: &mut egui::Ui, app: &mut App, owner: &SlideOwner) -> bool {
    let layouts: Vec<(Uuid, String)> = app.store.data.templates.iter().map(|t| (t.id, t.name.clone())).collect();
    let has = app.has_slide(owner);
    let mut chosen = None;
    let mut edit = false;
    if has {
        edit = ui.button("✏ Bearbeiten").on_hover_text("Slide im Editor öffnen").clicked();
    }
    let label = if has { "➕ Neu" } else { "➕ Slide anlegen" };
    ui.menu_button(label, |ui| {
        ui.label(RichText::new(if has { "Neue Slide aus Layout (ersetzt die jetzige):" } else { "Aus welchem Layout?" }).small());
        for (id, name) in &layouts {
            if ui.button(name).clicked() {
                chosen = Some(*id);
                ui.close();
            }
        }
    });
    if edit {
        app.edit_slide(owner);
    }
    if let Some(layout) = chosen {
        app.new_slide(owner, layout);
    }
    edit || chosen.is_some()
}
