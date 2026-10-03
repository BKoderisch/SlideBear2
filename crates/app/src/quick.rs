//! Schnellansicht für den Sonntag: eine Zeile pro Veranstaltung (wiederkehrende Termine
//! zusammengefasst), direkt editierbar, mit Export-Häkchen und einem großen Export-Knopf.

use std::collections::{HashMap, HashSet};

use chrono::{Duration, Local, NaiveDate};
use eframe::egui::{self, RichText, Vec2};
use slidebear_core::export::{GroupKey, group_key, groups, plan};
use slidebear_core::format::weekday_de;
use slidebear_core::{Event, EventStatus, SlideRef, slide_for};
use uuid::Uuid;

use crate::app::{App, EditScope, slide_buttons};
use crate::events::{apply_datetime, parse_date, parse_time, set_title};
use crate::theme;

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

/// Eine Tabellenzeile: Veranstaltung, ihr nächster Termin und wie viele weitere folgen.
#[derive(Clone, Copy)]
struct Row {
    key: GroupKey,
    next: Uuid,
    more: usize,
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    let today = Local::now().date_naive();
    let horizon = today + Duration::days(app.quick.days as i64);

    // Was ein Export jetzt tatsächlich schreiben würde, und die Zeilen (eine pro Veranstaltung)
    let (planned, rows): (HashSet<Uuid>, Vec<Row>) = {
        let d = &app.store.data;
        let planned =
            plan(&d.events, &d.series, &d.settings.hide_rules, today, d.settings.export_days).iter().map(|p| p.event.id).collect();
        let rows = groups(&d.events, &d.series, &d.settings.hide_rules, today, horizon)
            .iter()
            .map(|g| Row { key: g.key, next: g.next.id, more: g.more })
            .collect();
        (planned, rows)
    };

    egui::Panel::top("quick_head").show(ui, |ui| {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.heading("Schnellexport");
            ui.label("Termine der nächsten");
            ui.add(egui::DragValue::new(&mut app.quick.days).range(1..=365).suffix(" Tage"));
            ui.separator();
            if ui.button("Alle an").clicked() {
                set_all(app, &rows, true);
            }
            if ui.button("Alle aus").clicked() {
                set_all(app, &rows, false);
            }
            if ui.button("➕ Termin").on_hover_text("Einzelnen Termin ohne ChurchTools anlegen").clicked() {
                crate::events::new_manual(app);
            }
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let n = planned.len();
            let export = egui::Button::new(RichText::new(format!("⬆  {n} Slides exportieren")).size(24.0).color(theme::SNOW))
                .fill(theme::GLACIER)
                .stroke(egui::Stroke::new(3.0, theme::INK))
                .min_size(Vec2::new(320.0, 56.0));
            if theme::comic_button(ui, export).clicked() {
                app.export_now();
            }
            ui.add_space(8.0);
            let sync = egui::Button::new("🔄 Erst mit ChurchTools abgleichen").min_size(Vec2::new(0.0, 56.0));
            if ui.add_enabled_ui(app.jobs.running == 0, |ui| theme::comic_button(ui, sync)).inner.clicked() {
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
        ui.add_space(8.0);
    });

    egui::CentralPanel::default().show(ui, |ui| {
        if rows.is_empty() {
            ui.centered_and_justified(|ui| ui.label("Keine Termine in diesem Zeitraum."));
            return;
        }
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("quick").num_columns(8).striped(true).spacing([14.0, 6.0]).min_row_height(54.0).show(ui, |ui| {
                for h in ["Export", "", "Termin", "Datum", "Beginn", "Ende", "Slide", ""] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for r in &rows {
                    row(ui, app, *r, planned.contains(&r.next), (today, horizon));
                    ui.end_row();
                }
            });
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "Wiederkehrende Termine erscheinen einmal und nutzen immer dieselbe Slide; exportiert wird der nächste Termin. \
                     Beginn leer = ganztags, Ende leer = offen.",
                )
                .weak()
                .small(),
            );
        });
    });
}

fn set_all(app: &mut App, rows: &[Row], on: bool) {
    for r in rows {
        match r.key {
            GroupKey::Series(id) => {
                if let Some(s) = app.store.data.series_mut(id) {
                    s.enabled = on;
                }
            }
            GroupKey::Single(id) => {
                if let Some(e) = app.store.data.event_mut(id) {
                    e.enabled = on;
                }
            }
            // Ohne Slide gibt es nichts zu exportieren
            GroupKey::Appointment { .. } => {}
        }
    }
    app.store.mark_dirty();
}

fn row(ui: &mut egui::Ui, app: &mut App, r: Row, planned: bool, range: (NaiveDate, NaiveDate)) {
    let id = r.next;
    let Some(mut ev) = app.store.data.event(id).cloned() else {
        return;
    };
    let owner = app.owner_of(&ev);
    let cancelled = ev.status == EventStatus::Cancelled;
    let slide = slide_for(&ev, &app.store.data.series).map(|s| (s.scene.clone(), s.date_style.clone(), s.time_style.clone()));
    let mut changed = false;

    // Export-Häkchen: bei Serien für die ganze Reihe
    let mut on = match r.key {
        GroupKey::Series(sid) => app.store.data.series(sid).is_some_and(|s| s.enabled),
        _ => ev.enabled,
    };
    ui.add_enabled_ui(slide.is_some() && !cancelled, |ui| {
        if ui.checkbox(&mut on, "").on_disabled_hover_text("Erst eine Slide anlegen").changed() {
            match r.key {
                GroupKey::Series(sid) => {
                    if let Some(s) = app.store.data.series_mut(sid) {
                        s.enabled = on;
                    }
                    app.store.mark_dirty();
                }
                _ => {
                    ev.enabled = on;
                    changed = true;
                }
            }
        }
    });

    // Vorschau, groß beim Darüberfahren, Doppelklick öffnet den Editor
    let thumb_resp = match &slide {
        Some((scene, ds, ts)) => {
            let sr = SlideRef { scene, date_style: ds, time_style: ts };
            let f = ev.fields();
            let tex = app.previews.get(ui.ctx(), &mut app.renderer, sr, &f, 128.0);
            let big = app.previews.get(ui.ctx(), &mut app.renderer, sr, &f, 960.0);
            let resp = ui.add(egui::Image::new(&tex).fit_to_exact_size(Vec2::new(64.0, 36.0)).sense(egui::Sense::click()));
            if resp.double_clicked() {
                app.edit_slide(&owner);
            }
            Some(resp.on_hover_ui(|ui| {
                ui.add(egui::Image::new(&big).fit_to_exact_size(Vec2::new(480.0, 270.0)));
                ui.label(RichText::new("Doppelklick: Slide bearbeiten").small());
            }))
        }
        None => {
            ui.label("");
            None
        }
    };

    // Eingabe-IDs, um zu erkennen, ob gerade in dieser Zeile getippt wird
    let ids = ["title", "date", "start", "end"].map(|k| ui.make_persistent_id((id, k)));
    let editing = ui.memory(|m| ids.iter().any(|i| m.has_focus(*i)));
    let buf = app.quick.rows.entry(id).or_default();
    if !editing {
        *buf = RowBuf::from_event(&ev);
    }

    let red = |ok: bool| (!ok).then_some(theme::CANCELLED_TEXT);
    let title_r = theme::text_field(ui, egui::TextEdit::singleline(&mut buf.title).id(ids[0]), 300.0);
    let date_ok = parse_date(&buf.date).is_some();
    let date_r = theme::text_field(
        ui,
        egui::TextEdit::singleline(&mut buf.date).id(ids[1]).hint_text("TT.MM.JJJJ").text_color_opt(red(date_ok)),
        140.0,
    );
    let start_ok = buf.start.trim().is_empty() || parse_time(&buf.start).is_some();
    let start_r = theme::text_field(
        ui,
        egui::TextEdit::singleline(&mut buf.start).id(ids[2]).hint_text("ganztags").text_color_opt(red(start_ok)),
        110.0,
    );
    let end_ok = buf.end.trim().is_empty() || parse_time(&buf.end).is_some();
    let end_r =
        theme::text_field(ui, egui::TextEdit::singleline(&mut buf.end).id(ids[3]).hint_text("offen").text_color_opt(red(end_ok)), 110.0);

    if title_r.changed() {
        set_title(&mut ev, buf.title.clone());
        changed = true;
    }
    if (date_r.changed() || start_r.changed() || end_r.changed()) && apply_datetime(&mut ev, &buf.date, &buf.start, &buf.end) {
        changed = true;
    }
    if changed {
        // Im Schnellexport immer nur der angezeigte Termin; „alle Termine“ gibt es in den Termin-Details
        app.commit_event_scoped(ev.clone(), EditScope::This);
    }

    // Slide bearbeiten oder neu anlegen
    ui.horizontal(|ui| {
        slide_buttons(ui, app, &owner, &ev);
    });

    // Status
    ui.horizontal(|ui| {
        if cancelled {
            ui.label(RichText::new("entfallen").color(theme::CANCELLED_TEXT));
        } else if slide.is_none() {
            ui.label(RichText::new("keine Slide").weak());
        } else if planned {
            ui.label(RichText::new("✔ wird exportiert").color(theme::OK_TEXT));
        } else if !on {
            ui.label(RichText::new("Export aus").weak());
        } else {
            ui.label(RichText::new("noch nicht dran").weak())
                .on_hover_text("Liegt außerhalb des Vorlaufs (Einstellungen bzw. Veranstaltung)");
        }
        if r.more > 0 {
            let dates = other_dates(app, r.key, id, range);
            ui.label(RichText::new(format!("🔁 +{}", r.more)).strong())
                .on_hover_text(format!("Weitere Termine mit derselben Slide:\n{}", dates.join("\n")));
        }
    });

    // Selten gebraucht, daher nur per Rechtsklick auf Vorschau oder Titel
    if ev.is_from_churchtools() {
        let mut hide = false;
        for resp in [thumb_resp, Some(title_r)].into_iter().flatten() {
            resp.context_menu(|ui| {
                if ui
                    .button("Veranstaltung ausblenden")
                    .on_hover_text("Alle Termine, auch künftige. Rückgängig unter Einstellungen.")
                    .clicked()
                {
                    hide = true;
                    ui.close();
                }
            });
        }
        if hide {
            app.set_hidden(&ev, true);
        }
    }
}

/// Daten der anderen Termine derselben Zeile im Zeitraum (für den Tooltip).
fn other_dates(app: &App, key: GroupKey, except: Uuid, (from, to): (NaiveDate, NaiveDate)) -> Vec<String> {
    let d = &app.store.data;
    let mut list: Vec<_> = d
        .events
        .iter()
        .filter(|e| e.id != except && group_key(e, &d.series) == key)
        .map(|e| e.fields().start)
        .filter(|s| s.date() >= from && s.date() <= to)
        .collect();
    list.sort();
    list.iter().map(|s| format!("{} {}", weekday_de(s.date()).get(..2).unwrap_or(""), s.format("%d.%m. %H:%M"))).collect()
}
