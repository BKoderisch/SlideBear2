//! Freier Folien-Editor (für Vorlagen und einzelne Slides).
//!
//! Die Leinwand zeigt das echte Render-Ergebnis (identisch zum Export), egui zeichnet nur
//! Auswahlrahmen, Anfasser und Hilfslinien darüber.

use eframe::egui::{self, Color32, Key, Pos2, RichText, Sense, Stroke, StrokeKind, Vec2};
use slidebear_core::placeholder::PLACEHOLDERS;
use slidebear_core::scene::{
    Color, Element, ElementKind, Filter, HAlign, ImageFit, ImageStyle, Outline, Rect, Scene, Shadow, ShapeKind,
    ShapeStyle, Stroke as ShapeStroke, TextStyle, VAlign,
};
use slidebear_core::format::{DateStyle, TimeStyle};
use slidebear_core::{EventFields, Series, Slide, SlideRef};
use uuid::Uuid;

use crate::app::{sample_fields, App};
use crate::preview::{LiveTexture, Previews};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditTarget {
    /// Layout bearbeiten.
    Template(Uuid),
    /// Slide einer wiederkehrenden Veranstaltung.
    Series(Uuid),
    /// Eigene Slide eines einzelnen Termins.
    Event(Uuid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Handle {
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

const HANDLES: [Handle; 8] = [Handle::N, Handle::S, Handle::E, Handle::W, Handle::NE, Handle::NW, Handle::SE, Handle::SW];

impl Handle {
    /// Position relativ zum Rahmen (0..1).
    fn anchor(self) -> (f32, f32) {
        match self {
            Handle::N => (0.5, 0.0),
            Handle::S => (0.5, 1.0),
            Handle::E => (1.0, 0.5),
            Handle::W => (0.0, 0.5),
            Handle::NE => (1.0, 0.0),
            Handle::NW => (0.0, 0.0),
            Handle::SE => (1.0, 1.0),
            Handle::SW => (0.0, 1.0),
        }
    }

    fn cursor(self) -> egui::CursorIcon {
        match self {
            Handle::N | Handle::S => egui::CursorIcon::ResizeVertical,
            Handle::E | Handle::W => egui::CursorIcon::ResizeHorizontal,
            Handle::NE | Handle::SW => egui::CursorIcon::ResizeNeSw,
            Handle::NW | Handle::SE => egui::CursorIcon::ResizeNwSe,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum DragKind {
    Move,
    Resize(Handle),
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    kind: DragKind,
    id: Uuid,
    start_frame: Rect,
    start_pos: (f32, f32),
}

#[derive(Debug, Clone, Copy)]
enum Guide {
    V(f32),
    H(f32),
}

pub struct Editor {
    target: EditTarget,
    scene: Scene,
    date_style: DateStyle,
    time_style: TimeStyle,
    selected: Option<Uuid>,
    undo: Vec<Scene>,
    redo: Vec<Scene>,
    /// Zustand vor der laufenden Geste (Ziehen, Tippen), wird als ein Undo-Schritt gespeichert.
    pending: Option<Scene>,
    drag: Option<Drag>,
    guides: Vec<Guide>,
    focus_text: bool,
    font_filter: String,
    live: LiveTexture,
    snap: bool,
}

impl Editor {
    pub fn new(target: EditTarget, slide: Slide) -> Self {
        Self {
            target,
            scene: slide.scene,
            date_style: slide.date_style,
            time_style: slide.time_style,
            selected: None,
            undo: Vec::new(),
            redo: Vec::new(),
            pending: None,
            drag: None,
            guides: Vec::new(),
            focus_text: false,
            font_filter: String::new(),
            live: LiveTexture::default(),
            snap: true,
        }
    }

    fn slide(&self) -> Slide {
        Slide { scene: self.scene.clone(), date_style: self.date_style.clone(), time_style: self.time_style.clone() }
    }

    fn write_back(&self, app: &mut App) {
        let d = &mut app.store.data;
        match self.target {
            EditTarget::Template(id) => {
                if let Some(t) = d.template_mut(id) {
                    t.scene = self.scene.clone();
                    t.date_style = self.date_style.clone();
                    t.time_style = self.time_style.clone();
                }
            }
            EditTarget::Series(id) => {
                let slide = self.slide();
                if let Some(s) = d.series_mut(id) {
                    s.slide = Some(slide);
                }
            }
            EditTarget::Event(id) => {
                let slide = self.slide();
                if let Some(e) = d.event_mut(id) {
                    e.slide = Some(slide);
                }
            }
        }
        app.store.mark_dirty();
    }

    /// Termindaten für die Vorschau: der nächste Termin der Serie, der Termin selbst oder Beispieldaten.
    fn preview_fields(&self, app: &App) -> EventFields {
        let d = &app.store.data;
        match self.target {
            EditTarget::Event(id) => d.event(id).map(|e| e.fields()),
            EditTarget::Series(id) => {
                let today = chrono::Local::now().date_naive();
                d.series(id).and_then(|s| {
                    d.events
                        .iter()
                        .filter(|e| Series::find(std::slice::from_ref(s), e).is_some())
                        .map(|e| e.fields())
                        .filter(|f| f.start.date() >= today)
                        .min_by_key(|f| f.start)
                })
            }
            EditTarget::Template(_) => None,
        }
        .unwrap_or_else(sample_fields)
    }

    fn commit_pending(&mut self) {
        if let Some(p) = self.pending.take() {
            self.undo.push(p);
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
    }

    fn undo(&mut self) {
        self.commit_pending();
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.scene, prev));
        }
    }

    fn redo(&mut self) {
        self.commit_pending();
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.scene, next));
        }
    }

    /// Liefert `false`, wenn der Editor geschlossen wurde.
    pub fn show(&mut self, ui: &mut egui::Ui, app: &mut App) -> bool {
        let before = self.scene.clone();
        let styles_before = (self.date_style.clone(), self.time_style.clone());
        let mut keep_open = true;

        egui::Panel::top("editor_top").show(ui, |ui| {
            keep_open = self.toolbar(ui, app);
        });
        egui::Panel::left("layers").default_size(220.0).resizable(true).show(ui, |ui| self.layers(ui));
        egui::Panel::right("props").default_size(320.0).resizable(true).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| self.properties(ui, app));
        });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(crate::theme::CANVAS_BG).inner_margin(16.0)).show(ui, |ui| self.canvas(ui, app));

        self.shortcuts(ui.ctx());

        let scene_changed = self.scene != before;
        if scene_changed && self.pending.is_none() {
            self.pending = Some(before);
        }
        let changed = scene_changed || styles_before != (self.date_style.clone(), self.time_style.clone());
        let busy = ui.ctx().input(|i| i.pointer.any_down()) || ui.ctx().egui_wants_keyboard_input();
        if !busy {
            self.commit_pending();
        }
        if changed {
            self.write_back(app);
        }
        if !keep_open {
            self.commit_pending();
            app.store.mark_dirty();
        }
        keep_open
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (undo, redo, dup, del, esc, nudge) = ctx.input(|i| {
            let cmd = i.modifiers.command;
            let step = if i.modifiers.shift { 10.0 } else { 1.0 };
            let mut n = (0.0, 0.0);
            if i.key_pressed(Key::ArrowLeft) {
                n.0 -= step;
            }
            if i.key_pressed(Key::ArrowRight) {
                n.0 += step;
            }
            if i.key_pressed(Key::ArrowUp) {
                n.1 -= step;
            }
            if i.key_pressed(Key::ArrowDown) {
                n.1 += step;
            }
            (
                cmd && !i.modifiers.shift && i.key_pressed(Key::Z),
                cmd && ((i.modifiers.shift && i.key_pressed(Key::Z)) || i.key_pressed(Key::Y)),
                cmd && i.key_pressed(Key::D),
                i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
                i.key_pressed(Key::Escape),
                n,
            )
        });
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        if dup {
            self.duplicate();
        }
        if del {
            self.delete();
        }
        if esc {
            self.selected = None;
        }
        if nudge != (0.0, 0.0)
            && let Some(e) = self.selected.and_then(|id| self.scene.element_mut(id)) {
                e.frame.x += nudge.0;
                e.frame.y += nudge.1;
            }
    }

    fn duplicate(&mut self) {
        let Some(idx) = self.selected.and_then(|id| self.scene.index_of(id)) else { return };
        let mut copy = self.scene.elements[idx].duplicate();
        copy.frame.x += 20.0;
        copy.frame.y += 20.0;
        copy.locked = false;
        self.selected = Some(copy.id);
        self.scene.elements.insert(idx + 1, copy);
    }

    fn delete(&mut self) {
        if let Some(id) = self.selected.take() {
            self.scene.elements.retain(|e| e.id != id);
        }
    }

    fn insert(&mut self, el: Element) {
        self.selected = Some(el.id);
        self.scene.elements.push(el);
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, app: &mut App) -> bool {
        let mut keep = true;
        let (w, h) = (self.scene.width as f32, self.scene.height as f32);
        ui.horizontal_wrapped(|ui| {
            if ui.button("✔ Fertig").clicked() {
                keep = false;
            }
            let title = match self.target {
                EditTarget::Template(id) => format!("Layout: {}", app.store.data.template(id).map(|t| t.name.as_str()).unwrap_or("")),
                EditTarget::Series(id) => format!("Slide für alle Termine: {}", app.store.data.series(id).map(|s| s.title.as_str()).unwrap_or("")),
                EditTarget::Event(id) => format!("Slide: {}", app.store.data.event(id).map(|e| e.fields().title).unwrap_or_default()),
            };
            ui.label(RichText::new(title).strong());
            ui.separator();

            if ui.add_enabled(!self.undo.is_empty() || self.pending.is_some(), egui::Button::new("↶")).on_hover_text("Rückgängig (Cmd/Strg+Z)").clicked() {
                self.undo();
            }
            if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("↷")).on_hover_text("Wiederholen").clicked() {
                self.redo();
            }
            ui.separator();

            if ui.button("T Text").clicked() {
                let mut el = Element::new("Text", Rect::new(w / 2.0 - 500.0, h / 2.0 - 100.0, 1000.0, 200.0), ElementKind::Text(TextStyle { text: "Text".into(), size_px: 96.0, ..TextStyle::default() }));
                el.name = "Text".into();
                self.insert(el);
                self.focus_text = true;
            }
            if ui.button("🖼 Bild").clicked()
                && let Some(el) = pick_image(app, |asset, iw, ih| {
                    let s = (800.0 / iw).min(600.0 / ih).min(1.0);
                    let (ew, eh) = (iw * s, ih * s);
                    Element::new("Bild", Rect::new((w - ew) / 2.0, (h - eh) / 2.0, ew, eh), ElementKind::Image(ImageStyle { asset, fit: ImageFit::Contain, filters: Vec::new() }))
                }) {
                    self.insert(el);
                }
            if ui.button("🌄 Hintergrund").on_hover_text("Bild über die ganze Slide, ganz unten").clicked()
                && let Some(el) = pick_image(app, |asset, _, _| slidebear_core::presets::background(&asset, 0.0)) {
                    let existing = self.scene.elements.first().filter(|e| e.locked && matches!(e.kind, ElementKind::Image(_)) && e.frame.w >= w - 1.0).map(|e| e.id);
                    match existing.and_then(|id| self.scene.element_mut(id)) {
                        Some(bg) => {
                            if let (ElementKind::Image(old), ElementKind::Image(new)) = (&mut bg.kind, el.kind) {
                                old.asset = new.asset;
                            }
                        }
                        None => {
                            self.selected = Some(el.id);
                            self.scene.elements.insert(0, el);
                        }
                    }
                }
            ui.menu_button("⬛ Form", |ui| {
                let shapes = [("Rechteck", ShapeKind::Rect), ("Abgerundet", ShapeKind::RoundedRect { radius: 30.0 }), ("Ellipse", ShapeKind::Ellipse)];
                for (label, shape) in shapes {
                    if ui.button(label).clicked() {
                        self.insert(Element::new(label, Rect::new(w / 2.0 - 300.0, h / 2.0 - 150.0, 600.0, 300.0), ElementKind::Shape(ShapeStyle { shape, ..ShapeStyle::default() })));
                    }
                }
            });
            ui.separator();

            let sel = self.selected.and_then(|id| self.scene.index_of(id));
            ui.add_enabled_ui(sel.is_some(), |ui| {
                if ui.button("⤒").on_hover_text("Ganz nach vorne").clicked() {
                    self.reorder(|_, n| n - 1);
                }
                if ui.button("⬆").on_hover_text("Eine Ebene nach vorne").clicked() {
                    self.reorder(|i, n| (i + 1).min(n - 1));
                }
                if ui.button("⬇").on_hover_text("Eine Ebene nach hinten").clicked() {
                    self.reorder(|i, _| i.saturating_sub(1));
                }
                if ui.button("⤓").on_hover_text("Ganz nach hinten").clicked() {
                    self.reorder(|_, _| 0);
                }
                ui.separator();
                if ui.button("↔").on_hover_text("Horizontal zentrieren").clicked()
                    && let Some(e) = self.selected.and_then(|id| self.scene.element_mut(id)) {
                        e.frame.x = (w - e.frame.w) / 2.0;
                    }
                if ui.button("↕").on_hover_text("Vertikal zentrieren").clicked()
                    && let Some(e) = self.selected.and_then(|id| self.scene.element_mut(id)) {
                        e.frame.y = (h - e.frame.h) / 2.0;
                    }
                if ui.button("⧉").on_hover_text("Duplizieren (Cmd/Strg+D)").clicked() {
                    self.duplicate();
                }
                if ui.button("🗑").on_hover_text("Löschen (Entf)").clicked() {
                    self.delete();
                }
            });
            ui.separator();
            ui.checkbox(&mut self.snap, "Einrasten");

            ui.separator();
            ui.label("Datum");
            let current = DATE_PATTERNS.iter().find(|(p, _)| *p == self.date_style.pattern).map(|(_, ex)| *ex).unwrap_or("eigenes");
            egui::ComboBox::from_id_salt("date_style").selected_text(current).show_ui(ui, |ui| {
                for (p, ex) in DATE_PATTERNS {
                    if ui.selectable_label(self.date_style.pattern == p, ex).clicked() {
                        self.date_style.pattern = p.into();
                    }
                }
            });
            ui.checkbox(&mut self.time_style.omit_zero_minutes, "10 statt 10:00");
            ui.checkbox(&mut self.time_style.show_end, "Endzeit");
        });
        keep
    }

    fn reorder(&mut self, to: impl Fn(usize, usize) -> usize) {
        let Some(i) = self.selected.and_then(|id| self.scene.index_of(id)) else { return };
        let n = self.scene.elements.len();
        let el = self.scene.elements.remove(i);
        let target = to(i, n).min(n - 1);
        self.scene.elements.insert(target, el);
    }

    fn layers(&mut self, ui: &mut egui::Ui) {
        ui.heading("Ebenen");
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let mut select = None;
            for e in self.scene.elements.iter_mut().rev() {
                ui.horizontal(|ui| {
                    let eye = if e.visible { "👁" } else { "◌" };
                    if ui.button(eye).on_hover_text("Sichtbar").clicked() {
                        e.visible = !e.visible;
                    }
                    let lock = if e.locked { "🔒" } else { "🔓" };
                    if ui.button(lock).on_hover_text("Gesperrt: auf der Leinwand nicht anklickbar").clicked() {
                        e.locked = !e.locked;
                    }
                    let icon = match e.kind {
                        ElementKind::Text(_) => "T",
                        ElementKind::Image(_) => "🖼",
                        ElementKind::Shape(_) => "⬛",
                    };
                    if ui.selectable_label(self.selected == Some(e.id), format!("{icon} {}", e.name)).clicked() {
                        select = Some(e.id);
                    }
                });
            }
            if let Some(id) = select {
                self.selected = Some(id);
            }
        });
    }

    fn canvas(&mut self, ui: &mut egui::Ui, app: &mut App) {
        let (sw, sh) = (self.scene.width as f32, self.scene.height as f32);
        let avail = ui.available_size();
        let scale = (avail.x / sw).min(avail.y / sh).max(0.05);
        let size = Vec2::new(sw * scale, sh * scale);
        let rect = egui::Rect::from_center_size(ui.max_rect().center(), size);
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());

        let fields = self.preview_fields(app);
        let ppp = ui.ctx().pixels_per_point();
        let quality = if self.drag.is_some() { 0.5 } else { 1.0 };
        let slide = SlideRef { scene: &self.scene, date_style: &self.date_style, time_style: &self.time_style };
        let tex = Previews::live(&mut self.live, ui.ctx(), &mut app.renderer, slide, &fields, size.x * ppp * quality);
        let painter = ui.painter_at(rect);
        painter.image(tex.id(), rect, egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);

        let to_screen = |x: f32, y: f32| rect.min + Vec2::new(x * scale, y * scale);
        let to_slide = |p: Pos2| ((p.x - rect.min.x) / scale, (p.y - rect.min.y) / scale);
        let frame_rect = |f: Rect| egui::Rect::from_min_max(to_screen(f.x, f.y), to_screen(f.x + f.w, f.y + f.h));
        let handle_pos = |f: Rect, h: Handle| {
            let (ax, ay) = h.anchor();
            to_screen(f.x + f.w * ax, f.y + f.h * ay)
        };

        let pointer = ui.ctx().input(|i| i.pointer.hover_pos());
        let selected_frame = self.selected.and_then(|id| self.scene.element(id)).map(|e| e.frame);

        // Anfasser unter dem Mauszeiger?
        let handle_at = |p: Pos2| -> Option<Handle> {
            let f = selected_frame?;
            HANDLES.into_iter().find(|h| handle_pos(f, *h).distance(p) < 8.0)
        };
        if let Some(p) = pointer.filter(|p| rect.contains(*p)) {
            if let Some(h) = handle_at(p) {
                ui.ctx().set_cursor_icon(h.cursor());
            } else if self.scene.hit_test(to_slide(p).0, to_slide(p).1).is_some() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
            }
        }

        if resp.drag_started()
            && let Some(origin) = ui.ctx().input(|i| i.pointer.press_origin()) {
                let sp = to_slide(origin);
                if let (Some(h), Some(id), Some(f)) = (handle_at(origin), self.selected, selected_frame) {
                    self.drag = Some(Drag { kind: DragKind::Resize(h), id, start_frame: f, start_pos: sp });
                } else if let Some(id) = self.scene.hit_test(sp.0, sp.1) {
                    self.selected = Some(id);
                    let f = self.scene.element(id).map(|e| e.frame).unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
                    self.drag = Some(Drag { kind: DragKind::Move, id, start_frame: f, start_pos: sp });
                } else {
                    self.selected = None;
                }
            }
        if resp.dragged()
            && let (Some(d), Some(p)) = (self.drag, pointer) {
                let (px, py) = to_slide(p);
                let (dx, dy) = (px - d.start_pos.0, py - d.start_pos.1);
                let (shift, alt) = ui.ctx().input(|i| (i.modifiers.shift, i.modifiers.alt));
                let snap = self.snap && !alt;
                let threshold = 8.0 / scale;
                let new_frame = match d.kind {
                    DragKind::Move => {
                        let mut f = Rect::new(d.start_frame.x + dx, d.start_frame.y + dy, d.start_frame.w, d.start_frame.h);
                        self.guides.clear();
                        if snap {
                            let (xs, ys) = self.snap_lines(d.id);
                            if let Some((off, line)) = best_snap(&[f.x, f.x + f.w / 2.0, f.x + f.w], &xs, threshold) {
                                f.x += off;
                                self.guides.push(Guide::V(line));
                            }
                            if let Some((off, line)) = best_snap(&[f.y, f.y + f.h / 2.0, f.y + f.h], &ys, threshold) {
                                f.y += off;
                                self.guides.push(Guide::H(line));
                            }
                        }
                        f
                    }
                    DragKind::Resize(h) => {
                        self.guides.clear();
                        let lines = if snap { Some(self.snap_lines(d.id)) } else { None };
                        resize(d.start_frame, h, dx, dy, shift, lines.as_ref(), threshold, &mut self.guides)
                    }
                };
                if let Some(e) = self.scene.element_mut(d.id) {
                    e.frame = new_frame;
                }
            }
        if resp.drag_stopped() {
            self.drag = None;
            self.guides.clear();
        }
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos() {
                let sp = to_slide(p);
                self.selected = self.scene.hit_test(sp.0, sp.1);
            }
        if resp.double_clicked()
            && let Some(p) = resp.interact_pointer_pos() {
                let sp = to_slide(p);
                if let Some(id) = self.scene.hit_test(sp.0, sp.1) {
                    self.selected = Some(id);
                    self.focus_text = matches!(self.scene.element(id).map(|e| &e.kind), Some(ElementKind::Text(_)));
                }
            }

        // Overlays
        let accent = crate::theme::GLACIER;
        if let Some(p) = pointer.filter(|p| rect.contains(*p) && self.drag.is_none()) {
            let sp = to_slide(p);
            if let Some(e) = self.scene.hit_test(sp.0, sp.1).filter(|id| Some(*id) != self.selected).and_then(|id| self.scene.element(id)) {
                painter.rect_stroke(frame_rect(e.frame), 0.0, Stroke::new(1.0, accent.gamma_multiply(0.6)), StrokeKind::Outside);
            }
        }
        if let Some(e) = self.selected.and_then(|id| self.scene.element(id)) {
            painter.rect_stroke(frame_rect(e.frame), 0.0, Stroke::new(1.5, accent), StrokeKind::Outside);
            if !e.locked {
                for h in HANDLES {
                    let c = handle_pos(e.frame, h);
                    let r = egui::Rect::from_center_size(c, Vec2::splat(9.0));
                    painter.rect_filled(r, 1.0, Color32::WHITE);
                    painter.rect_stroke(r, 1.0, Stroke::new(1.0, accent), StrokeKind::Inside);
                }
            }
        }
        let guide = Stroke::new(1.0, Color32::from_rgb(255, 60, 200));
        for g in &self.guides {
            match *g {
                Guide::V(x) => painter.line_segment([to_screen(x, 0.0), to_screen(x, sh)], guide),
                Guide::H(y) => painter.line_segment([to_screen(0.0, y), to_screen(sw, y)], guide),
            };
        }
    }

    /// Einrast-Linien: Slide-Ränder und -Mitte sowie Kanten/Mitten aller anderen Elemente.
    fn snap_lines(&self, except: Uuid) -> (Vec<f32>, Vec<f32>) {
        let (w, h) = (self.scene.width as f32, self.scene.height as f32);
        let mut xs = vec![0.0, w / 2.0, w];
        let mut ys = vec![0.0, h / 2.0, h];
        for e in self.scene.elements.iter().filter(|e| e.id != except && e.visible) {
            let f = e.frame;
            xs.extend([f.x, f.x + f.w / 2.0, f.x + f.w]);
            ys.extend([f.y, f.y + f.h / 2.0, f.y + f.h]);
        }
        (xs, ys)
    }

    fn properties(&mut self, ui: &mut egui::Ui, app: &mut App) {
        let Some(id) = self.selected else {
            ui.heading("Slide");
            ui.label(RichText::new("Element auf der Leinwand oder in den Ebenen auswählen.").weak());
            ui.add_space(8.0);
            ui.label(RichText::new("Tipps").strong());
            ui.label("• Ziehen verschiebt, Anfasser ändern die Größe (Shift: Seitenverhältnis)");
            ui.label("• Alt beim Ziehen: ohne Einrasten");
            ui.label("• Pfeiltasten: 1 px, mit Shift 10 px");
            ui.label("• Doppelklick auf Text: Text bearbeiten");
            return;
        };
        let families = app.font_families.clone();
        let Some(e) = self.scene.element_mut(id) else {
            self.selected = None;
            return;
        };

        ui.horizontal(|ui| {
            ui.label("Name");
            crate::theme::text_field(ui, egui::TextEdit::singleline(&mut e.name), 200.0);
        });
        egui::Grid::new("geom").num_columns(4).spacing([6.0, 4.0]).show(ui, |ui| {
            ui.label("X");
            ui.add(egui::DragValue::new(&mut e.frame.x).speed(1.0));
            ui.label("Y");
            ui.add(egui::DragValue::new(&mut e.frame.y).speed(1.0));
            ui.end_row();
            ui.label("B");
            ui.add(egui::DragValue::new(&mut e.frame.w).speed(1.0).range(1.0..=10_000.0));
            ui.label("H");
            ui.add(egui::DragValue::new(&mut e.frame.h).speed(1.0).range(1.0..=10_000.0));
            ui.end_row();
        });
        ui.horizontal(|ui| {
            ui.label("Deckkraft");
            ui.add(egui::Slider::new(&mut e.opacity, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut e.visible, "Sichtbar");
            ui.checkbox(&mut e.locked, "Gesperrt");
        });
        ui.separator();

        match &mut e.kind {
            ElementKind::Text(t) => text_props(ui, t, &families, &mut self.font_filter, &mut self.focus_text),
            ElementKind::Image(img) => image_props(ui, img, app),
            ElementKind::Shape(s) => shape_props(ui, s),
        }
    }
}

fn best_snap(edges: &[f32], lines: &[f32], threshold: f32) -> Option<(f32, f32)> {
    let mut best: Option<(f32, f32)> = None;
    for &e in edges {
        for &l in lines {
            let d = l - e;
            if d.abs() <= threshold && best.is_none_or(|(b, _)| d.abs() < b.abs()) {
                best = Some((d, l));
            }
        }
    }
    best
}

#[allow(clippy::too_many_arguments)]
fn resize(start: Rect, h: Handle, dx: f32, dy: f32, keep_aspect: bool, lines: Option<&(Vec<f32>, Vec<f32>)>, threshold: f32, guides: &mut Vec<Guide>) -> Rect {
    let (mut l, mut t, mut r, mut b) = (start.x, start.y, start.x + start.w, start.y + start.h);
    let (ax, ay) = h.anchor();
    let snap = |v: f32, list: &[f32], vertical: bool, guides: &mut Vec<Guide>| -> f32 {
        match best_snap(&[v], list, threshold) {
            Some((off, line)) => {
                guides.push(if vertical { Guide::V(line) } else { Guide::H(line) });
                v + off
            }
            None => v,
        }
    };
    if ax == 0.0 {
        l += dx;
        if let Some((xs, _)) = lines {
            l = snap(l, xs, true, guides);
        }
    }
    if ax == 1.0 {
        r += dx;
        if let Some((xs, _)) = lines {
            r = snap(r, xs, true, guides);
        }
    }
    if ay == 0.0 {
        t += dy;
        if let Some((_, ys)) = lines {
            t = snap(t, ys, false, guides);
        }
    }
    if ay == 1.0 {
        b += dy;
        if let Some((_, ys)) = lines {
            b = snap(b, ys, false, guides);
        }
    }
    const MIN: f32 = 10.0;
    if r - l < MIN {
        if ax == 0.0 { l = r - MIN } else { r = l + MIN }
    }
    if b - t < MIN {
        if ay == 0.0 { t = b - MIN } else { b = t + MIN }
    }
    if keep_aspect && start.h > 0.0 && ax != 0.5 && ay != 0.5 {
        let ratio = start.w / start.h;
        let w = r - l;
        let new_h = w / ratio;
        if ay == 0.0 { t = b - new_h } else { b = t + new_h }
        guides.clear();
    }
    Rect::new(l, t, r - l, b - t)
}

fn color_edit(ui: &mut egui::Ui, c: &mut Color) -> bool {
    let mut rgba = [c.r, c.g, c.b, c.a];
    let changed = ui.color_edit_button_srgba_unmultiplied(&mut rgba).changed();
    if changed {
        *c = Color::rgba(rgba[0], rgba[1], rgba[2], rgba[3]);
    }
    changed
}

/// Datumsformate zur Auswahl (chrono-Muster, Beispiel).
pub const DATE_PATTERNS: [(&str, &str); 5] = [
    ("%d.%m.%Y", "07.12.2024"),
    ("%d.%m.%y", "07.12.24"),
    ("%-d.%-m.%Y", "7.12.2024"),
    ("%d.%m.", "07.12."),
    ("%Y-%m-%d", "2024-12-07"),
];

const WEIGHTS: [(u16, &str); 6] = [(100, "Thin"), (300, "Light"), (400, "Regular"), (500, "Medium"), (700, "Bold"), (900, "Black")];

fn text_props(ui: &mut egui::Ui, t: &mut TextStyle, families: &[String], filter: &mut String, focus: &mut bool) {
    ui.label(RichText::new("Text").strong());
    let edit_id = ui.make_persistent_id("text_edit");
    let resp = crate::theme::text_area(ui, egui::TextEdit::multiline(&mut t.text).id(edit_id), ui.available_width(), 3);
    if *focus {
        resp.request_focus();
        *focus = false;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Einfügen:").small());
        for (key, label) in PLACEHOLDERS {
            if ui.button(label).on_hover_text(format!("{{{key}}}")).clicked() {
                let token = format!("{{{key}}}");
                let cursor = egui::TextEdit::load_state(ui.ctx(), edit_id).and_then(|s| s.cursor.char_range()).map(|r| r.primary.index);
                match cursor {
                    Some(ci) => {
                        let byte = t.text.char_indices().nth(ci.into()).map(|(b, _)| b).unwrap_or(t.text.len());
                        t.text.insert_str(byte, &token);
                    }
                    None => t.text.push_str(&token),
                }
            }
        }
    });
    ui.add_space(6.0);

    egui::Grid::new("text_props").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Schrift");
        egui::ComboBox::from_id_salt("font").selected_text(&t.font_family).height(400.0).show_ui(ui, |ui| {
            crate::theme::text_field(ui, egui::TextEdit::singleline(filter).hint_text("Suchen …"), 220.0);
            let f = filter.to_lowercase();
            for fam in families.iter().filter(|n| f.is_empty() || n.to_lowercase().contains(&f)) {
                ui.selectable_value(&mut t.font_family, fam.clone(), fam);
            }
        });
        ui.end_row();

        ui.label("Stärke");
        let wl = WEIGHTS.iter().find(|(w, _)| *w == t.weight).map(|(_, n)| n.to_string()).unwrap_or_else(|| t.weight.to_string());
        egui::ComboBox::from_id_salt("weight").selected_text(wl).show_ui(ui, |ui| {
            for (w, n) in WEIGHTS {
                ui.selectable_value(&mut t.weight, w, n);
            }
        });
        ui.end_row();

        ui.label("Größe");
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut t.size_px).range(4.0..=1000.0).suffix(" px"));
            ui.checkbox(&mut t.italic, "Kursiv");
        });
        ui.end_row();

        ui.label("Farbe");
        color_edit(ui, &mut t.color);
        ui.end_row();

        ui.label("Ausrichtung");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut t.align_h, HAlign::Left, "⬅");
            ui.selectable_value(&mut t.align_h, HAlign::Center, "↔");
            ui.selectable_value(&mut t.align_h, HAlign::Right, "➡");
            ui.separator();
            ui.selectable_value(&mut t.align_v, VAlign::Top, "⬆");
            ui.selectable_value(&mut t.align_v, VAlign::Middle, "↕");
            ui.selectable_value(&mut t.align_v, VAlign::Bottom, "⬇");
        });
        ui.end_row();

        ui.label("Zeilenabstand");
        ui.add(egui::DragValue::new(&mut t.line_height).range(0.5..=3.0).speed(0.01));
        ui.end_row();

        ui.label("");
        ui.checkbox(&mut t.auto_shrink, "Automatisch verkleinern").on_hover_text("Schrift wird kleiner, wenn der Text nicht in den Rahmen passt");
        ui.end_row();
    });

    ui.add_space(6.0);
    let mut has_shadow = t.shadow.is_some();
    if ui.checkbox(&mut has_shadow, "Schatten").changed() {
        t.shadow = has_shadow.then(|| Shadow { color: Color::rgba(0, 0, 0, 160), offset_x: 0.0, offset_y: 4.0, blur: 8.0 });
    }
    if let Some(s) = &mut t.shadow {
        egui::Grid::new("shadow").num_columns(2).show(ui, |ui| {
            ui.label("Farbe");
            color_edit(ui, &mut s.color);
            ui.end_row();
            ui.label("Versatz");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut s.offset_x).speed(0.5).prefix("x "));
                ui.add(egui::DragValue::new(&mut s.offset_y).speed(0.5).prefix("y "));
            });
            ui.end_row();
            ui.label("Weichheit");
            ui.add(egui::DragValue::new(&mut s.blur).range(0.0..=100.0).speed(0.2));
            ui.end_row();
        });
    }
    let mut has_outline = t.outline.is_some();
    if ui.checkbox(&mut has_outline, "Umriss").changed() {
        t.outline = has_outline.then_some(Outline { color: Color::BLACK, width: 3.0 });
    }
    if let Some(o) = &mut t.outline {
        ui.horizontal(|ui| {
            color_edit(ui, &mut o.color);
            ui.add(egui::DragValue::new(&mut o.width).range(0.5..=40.0).speed(0.1).suffix(" px"));
        });
    }
}

fn image_props(ui: &mut egui::Ui, img: &mut ImageStyle, app: &mut App) {
    ui.label(RichText::new("Bild").strong());
    ui.horizontal(|ui| {
        ui.label(RichText::new(&img.asset).small().weak());
        if ui.button("Ersetzen …").clicked()
            && let Some(path) = rfd::FileDialog::new().add_filter("Bilder", &["png", "jpg", "jpeg", "webp", "gif", "bmp"]).pick_file() {
                match app.store.import_asset_file(&path) {
                    Ok(a) => img.asset = a,
                    Err(e) => app.error(e.to_string()),
                }
            }
    });
    ui.horizontal(|ui| {
        ui.label("Einpassen");
        ui.selectable_value(&mut img.fit, ImageFit::Cover, "Füllen");
        ui.selectable_value(&mut img.fit, ImageFit::Contain, "Einpassen");
        ui.selectable_value(&mut img.fit, ImageFit::Stretch, "Strecken");
    });
    ui.add_space(6.0);
    ui.label(RichText::new("Filter").strong());
    let mut remove = None;
    for (i, f) in img.filters.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            match f {
                Filter::Darken { amount } => {
                    ui.label("Abdunkeln");
                    ui.add(egui::Slider::new(amount, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));
                }
                Filter::Blur { radius } => {
                    ui.label("Weichzeichnen");
                    ui.add(egui::Slider::new(radius, 0.0..=60.0));
                }
                Filter::Tint { color, amount } => {
                    ui.label("Einfärben");
                    color_edit(ui, color);
                    ui.add(egui::Slider::new(amount, 0.0..=1.0).show_value(false));
                }
                Filter::Grayscale => {
                    ui.label("Graustufen");
                }
            }
            if ui.button("✕").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        img.filters.remove(i);
    }
    ui.menu_button("➕ Filter", |ui| {
        if ui.button("Abdunkeln").clicked() {
            img.filters.push(Filter::Darken { amount: 0.3 });
        }
        if ui.button("Weichzeichnen").clicked() {
            img.filters.push(Filter::Blur { radius: 6.0 });
        }
        if ui.button("Einfärben").clicked() {
            img.filters.push(Filter::Tint { color: Color::rgb(230, 140, 40), amount: 0.3 });
        }
        if ui.button("Graustufen").clicked() {
            img.filters.push(Filter::Grayscale);
        }
    });
}

fn shape_props(ui: &mut egui::Ui, s: &mut ShapeStyle) {
    ui.label(RichText::new("Form").strong());
    ui.horizontal(|ui| {
        let is_round = matches!(s.shape, ShapeKind::RoundedRect { .. });
        if ui.selectable_label(s.shape == ShapeKind::Rect, "Rechteck").clicked() {
            s.shape = ShapeKind::Rect;
        }
        if ui.selectable_label(is_round, "Abgerundet").clicked() && !is_round {
            s.shape = ShapeKind::RoundedRect { radius: 30.0 };
        }
        if ui.selectable_label(s.shape == ShapeKind::Ellipse, "Ellipse").clicked() {
            s.shape = ShapeKind::Ellipse;
        }
    });
    if let ShapeKind::RoundedRect { radius } = &mut s.shape {
        ui.horizontal(|ui| {
            ui.label("Radius");
            ui.add(egui::DragValue::new(radius).range(0.0..=1000.0));
        });
    }
    let mut has_fill = s.fill.is_some();
    ui.horizontal(|ui| {
        if ui.checkbox(&mut has_fill, "Füllung").changed() {
            s.fill = has_fill.then_some(Color::rgba(0, 0, 0, 128));
        }
        if let Some(c) = &mut s.fill {
            color_edit(ui, c);
        }
    });
    let mut has_stroke = s.stroke.is_some();
    ui.horizontal(|ui| {
        if ui.checkbox(&mut has_stroke, "Rahmen").changed() {
            s.stroke = has_stroke.then_some(ShapeStroke { color: Color::WHITE, width: 4.0 });
        }
        if let Some(st) = &mut s.stroke {
            color_edit(ui, &mut st.color);
            ui.add(egui::DragValue::new(&mut st.width).range(0.5..=100.0).speed(0.1));
        }
    });
}

/// Bild auswählen, als Asset übernehmen und daraus ein Element bauen.
fn pick_image(app: &mut App, build: impl FnOnce(String, f32, f32) -> Element) -> Option<Element> {
    let path = rfd::FileDialog::new().add_filter("Bilder", &["png", "jpg", "jpeg", "webp", "gif", "bmp"]).pick_file()?;
    let asset = match app.store.import_asset_file(&path) {
        Ok(a) => a,
        Err(e) => {
            app.error(e.to_string());
            return None;
        }
    };
    let (w, h) = image::image_dimensions(app.store.assets_dir().join(&asset)).unwrap_or((800, 600));
    Some(build(asset, w as f32, h as f32))
}
