//! Netzwerkzugriffe laufen in Hintergrund-Threads, damit die Oberfläche nicht hängt.

use std::sync::mpsc::{channel, Receiver, Sender};

use chrono::{Duration, Local};
use slidebear_churchtools::{Calendar, Client};
use slidebear_core::sync::RemoteAppointment;

pub enum JobResult {
    Sync { calendars: Vec<i64>, from: chrono::NaiveDate, to: chrono::NaiveDate, result: Result<Vec<RemoteAppointment>, String> },
    Calendars(Result<Vec<Calendar>, String>),
    Whoami(Result<String, String>),
}

pub struct Jobs {
    tx: Sender<JobResult>,
    rx: Receiver<JobResult>,
    pub running: usize,
}

impl Default for Jobs {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { tx, rx, running: 0 }
    }
}

impl Jobs {
    fn spawn(&mut self, ctx: &eframe::egui::Context, f: impl FnOnce() -> JobResult + Send + 'static) {
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.running += 1;
        std::thread::spawn(move || {
            let _ = tx.send(f());
            ctx.request_repaint();
        });
    }

    pub fn poll(&mut self) -> Vec<JobResult> {
        let results: Vec<_> = self.rx.try_iter().collect();
        self.running = self.running.saturating_sub(results.len());
        results
    }

    pub fn sync(&mut self, ctx: &eframe::egui::Context, url: String, token: String, calendars: Vec<i64>, days: u32) {
        let from = Local::now().date_naive() - Duration::days(1);
        let to = Local::now().date_naive() + Duration::days(days as i64);
        self.spawn(ctx, move || {
            let result = Client::new(&url, &token)
                .and_then(|c| c.appointments(&calendars, from, to))
                .map_err(|e| e.to_string());
            JobResult::Sync { calendars, from, to, result }
        });
    }

    pub fn calendars(&mut self, ctx: &eframe::egui::Context, url: String, token: String) {
        self.spawn(ctx, move || {
            JobResult::Calendars(Client::new(&url, &token).and_then(|c| c.calendars()).map_err(|e| e.to_string()))
        });
    }

    pub fn whoami(&mut self, ctx: &eframe::egui::Context, url: String, token: String) {
        self.spawn(ctx, move || {
            JobResult::Whoami(Client::new(&url, &token).and_then(|c| c.whoami()).map_err(|e| e.to_string()))
        });
    }
}
