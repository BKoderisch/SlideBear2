//! PNG-Export in den ProPresenter-Ordner. Die App merkt sich in einem Manifest, welche Dateien
//! sie selbst geschrieben hat, und löscht nur diese, wenn Termine vorbei oder entfallen sind.

use std::collections::BTreeMap;
use std::path::Path;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use slidebear_core::export::{plan, render_key};
use slidebear_core::placeholder;
use slidebear_render::{Renderer, save_png};

use crate::store::Data;

const MANIFEST: &str = ".slidebear-manifest.json";

#[derive(Default, Serialize, Deserialize)]
struct Manifest {
    /// Dateiname → Render-Fingerabdruck
    files: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
pub struct Report {
    pub written: usize,
    pub unchanged: usize,
    pub removed: usize,
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} neu/geändert, {} unverändert, {} entfernt", self.written, self.unchanged, self.removed)
    }
}

pub fn run(data: &Data, renderer: &mut Renderer, dir: &Path, today: NaiveDate) -> anyhow::Result<Report> {
    std::fs::create_dir_all(dir)?;
    let manifest_path = dir.join(MANIFEST);
    let old: Manifest = std::fs::read(&manifest_path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();

    let mut new = Manifest::default();
    let mut report = Report::default();

    for p in plan(&data.events, &data.series, &data.settings.hide_rules, today, data.settings.export_days) {
        let key = render_key(p.slide, &p.fields);
        let target = dir.join(&p.file);
        if old.files.get(&p.file) == Some(&key) && target.exists() {
            report.unchanged += 1;
        } else {
            let (slide, fields) = (p.slide, p.fields.clone());
            let pm = renderer.render(slide.scene, &|s| placeholder::resolve(s, &fields, slide));
            save_png(&pm, &target)?;
            report.written += 1;
        }
        new.files.insert(p.file, key);
    }

    for file in old.files.keys().filter(|f| !new.files.contains_key(*f)) {
        let path = dir.join(file);
        if path.exists() {
            std::fs::remove_file(&path)?;
            report.removed += 1;
        }
    }

    std::fs::write(&manifest_path, serde_json::to_vec_pretty(&new)?)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Local, NaiveTime};
    use slidebear_core::{Event, EventFields, Slide, presets};

    #[test]
    fn writes_skips_and_cleans_up() {
        let dir = std::env::temp_dir().join(format!("slidebear-export-{}", uuid::Uuid::new_v4()));
        let today = Local::now().date_naive();
        let tpl = presets::event_template("Standard", None);
        let fields = EventFields {
            title: "Gebetsabend".into(),
            start: (today + Duration::days(3)).and_time(NaiveTime::from_hms_opt(19, 0, 0).unwrap()),
            end: None,
            all_day: false,
            location: String::new(),
            subtitle: String::new(),
        };
        let mut data = Data::default();
        data.events.push(Event::manual(fields, Some(Slide::from_layout(&tpl))));
        let mut renderer = Renderer::new(dir.join("assets"));

        let r = run(&data, &mut renderer, &dir, today).unwrap();
        assert_eq!((r.written, r.unchanged, r.removed), (1, 0, 0));
        let png =
            std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).find(|e| e.path().extension().is_some_and(|x| x == "png")).unwrap();
        assert!(png.file_name().to_string_lossy().ends_with("_1900_gebetsabend.png"));

        // Zweiter Lauf: nichts geändert
        let r = run(&data, &mut renderer, &dir, today).unwrap();
        assert_eq!((r.written, r.unchanged), (0, 1));

        // Fremde Datei bleibt, eigene verschwindet, wenn der Termin deaktiviert wird
        std::fs::write(dir.join("fremd.png"), b"x").unwrap();
        data.events[0].enabled = false;
        let r = run(&data, &mut renderer, &dir, today).unwrap();
        assert_eq!(r.removed, 1);
        assert!(!png.path().exists());
        assert!(dir.join("fremd.png").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
