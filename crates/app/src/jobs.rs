//! Netzwerkzugriffe laufen in Hintergrund-Threads, damit die Oberfläche nicht hängt.

use std::sync::mpsc::{Receiver, Sender, channel};

use chrono::{Datelike, Duration, Local, NaiveDate};
use slidebear_churchtools::{Calendar, Client};
use slidebear_core::sync::RemoteAppointment;

pub enum JobResult {
    Sync {
        calendars: Vec<i64>,
        from: chrono::NaiveDate,
        to: chrono::NaiveDate,
        result: Result<Vec<RemoteAppointment>, String>,
    },
    Calendars(Result<Vec<Calendar>, String>),
    Whoami(Result<String, String>),
    /// Vornamen beim gesuchten Dienst am nächsten Sonntag; `manual` = vom Nutzer angestoßen.
    Presenter {
        result: Result<slidebear_churchtools::ServiceReport, String>,
        manual: bool,
    },
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
            let result = Client::new(&url, &token).and_then(|c| c.appointments(&calendars, from, to)).map_err(|e| e.to_string());
            JobResult::Sync { calendars, from, to, result }
        });
    }

    pub fn calendars(&mut self, ctx: &eframe::egui::Context, url: String, token: String) {
        self.spawn(ctx, move || JobResult::Calendars(Client::new(&url, &token).and_then(|c| c.calendars()).map_err(|e| e.to_string())));
    }

    pub fn presenter(&mut self, ctx: &eframe::egui::Context, url: String, token: String, term: String, manual: bool) {
        let sunday = next_sunday(Local::now().date_naive());
        self.spawn(ctx, move || {
            let result = Client::new(&url, &token).and_then(|c| c.service_people(sunday, &term)).map_err(|e| e.to_string());
            JobResult::Presenter { result, manual }
        });
    }

    pub fn whoami(&mut self, ctx: &eframe::egui::Context, url: String, token: String) {
        self.spawn(ctx, move || JobResult::Whoami(Client::new(&url, &token).and_then(|c| c.whoami()).map_err(|e| e.to_string())));
    }
}

/// Heute, wenn Sonntag ist, sonst der kommende Sonntag.
pub fn next_sunday(today: NaiveDate) -> NaiveDate {
    today + Duration::days((7 - today.weekday().num_days_from_sunday() as i64) % 7)
}

/// „Anna“, „Anna und Ben“, „Anna, Ben und Carla“
pub fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} und {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sunday_and_names() {
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        assert_eq!(next_sunday(d("2026-10-04")), d("2026-10-04"));
        assert_eq!(next_sunday(d("2026-10-05")), d("2026-10-11"));
        assert_eq!(next_sunday(d("2026-10-10")), d("2026-10-11"));
        let n = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(join_names(&n(&["Anna"])), "Anna");
        assert_eq!(join_names(&n(&["Anna", "Ben"])), "Anna und Ben");
        assert_eq!(join_names(&n(&["Anna", "Ben", "Carla"])), "Anna, Ben und Carla");
    }
}
