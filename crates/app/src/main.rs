//! SlideBear2: Veranstaltungs-Slides aus ChurchTools für ProPresenter.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod confetti;
mod editor;
mod events;
mod export;
mod import;
mod jobs;
mod preview;
mod quick;
mod settings;
mod store;
mod templates;
mod theme;

fn main() -> eframe::Result<()> {
    let root = std::env::var_os("SLIDEBEAR_DATA").map(std::path::PathBuf::from).unwrap_or_else(store::Store::default_root);
    let store = match store::Store::open(root) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Daten konnten nicht geladen werden: {e:#}");
            std::process::exit(1);
        }
    };
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("SlideBear2")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([900.0, 600.0])
            .with_icon(std::sync::Arc::new(theme::app_icon(256))),
        ..Default::default()
    };
    eframe::run_native("SlideBear2", options, Box::new(|cc| Ok(Box::new(app::App::new(cc, store)))))
}
