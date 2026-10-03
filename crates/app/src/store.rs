//! Persistenz: alle Daten als JSON im App-Datenordner, Bilder content-adressiert in `assets/`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use slidebear_core::assets::content_name;
use slidebear_core::{Event, Series, Template};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub ct_url: String,
    pub calendars: Vec<i64>,
    pub sync_on_start: bool,
    /// Wie weit in die Zukunft Termine aus ChurchTools geholt werden.
    pub sync_days: u32,
    pub export_dir: Option<PathBuf>,
    /// Standard-Vorlauf: Slides für Termine der nächsten N Tage exportieren.
    pub export_days: u32,
    pub export_on_sync: bool,
    /// Zoom der Oberfläche (1.0 = Normal).
    pub ui_scale: f32,
    /// ChurchTools-Termine mit diesen Begriffen im Titel ausblenden.
    pub hide_rules: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ct_url: String::new(),
            calendars: Vec::new(),
            sync_on_start: true,
            sync_days: 120,
            export_dir: None,
            export_days: 28,
            export_on_sync: false,
            ui_scale: 1.0,
            hide_rules: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Data {
    pub templates: Vec<Template>,
    pub events: Vec<Event>,
    /// Wiederkehrende Veranstaltungen mit ihrer Slide (früher „links“).
    #[serde(alias = "links")]
    pub series: Vec<Series>,
    pub settings: Settings,
}

impl Data {
    pub fn template(&self, id: uuid::Uuid) -> Option<&Template> {
        self.templates.iter().find(|t| t.id == id)
    }

    pub fn template_mut(&mut self, id: uuid::Uuid) -> Option<&mut Template> {
        self.templates.iter_mut().find(|t| t.id == id)
    }

    pub fn series(&self, id: uuid::Uuid) -> Option<&Series> {
        self.series.iter().find(|s| s.id == id)
    }

    pub fn series_mut(&mut self, id: uuid::Uuid) -> Option<&mut Series> {
        self.series.iter_mut().find(|s| s.id == id)
    }

    pub fn event(&self, id: uuid::Uuid) -> Option<&Event> {
        self.events.iter().find(|e| e.id == id)
    }

    pub fn event_mut(&mut self, id: uuid::Uuid) -> Option<&mut Event> {
        self.events.iter_mut().find(|e| e.id == id)
    }
}

pub struct Store {
    root: PathBuf,
    pub data: Data,
    dirty_since: Option<Instant>,
}

const KEYRING_SERVICE: &str = "SlideBear";
const KEYRING_USER: &str = "churchtools-token";

impl Store {
    pub fn default_root() -> PathBuf {
        directories::ProjectDirs::from("de", "ProKode", "SlideBear")
            .map(|d| d.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("slidebear-data"))
    }

    pub fn open(root: PathBuf) -> anyhow::Result<Self> {
        std::fs::create_dir_all(root.join("assets")).with_context(|| format!("{} anlegen", root.display()))?;
        let file = root.join("data.json");
        let mut data: Data = if file.exists() {
            let raw = std::fs::read_to_string(&file)?;
            serde_json::from_str(&raw).with_context(|| format!("{} ist beschädigt", file.display()))?
        } else {
            Data::default()
        };
        let migrated = slidebear_core::event::migrate(&mut data.events, &mut data.series, &data.templates);
        let mut store = Self { root, data, dirty_since: None };
        if migrated {
            store.mark_dirty();
        }
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.root.join("assets")
    }

    pub fn mark_dirty(&mut self) {
        self.dirty_since.get_or_insert_with(Instant::now);
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty_since.is_some()
    }

    /// Speichert gebündelt, höchstens einmal pro Sekunde.
    pub fn autosave(&mut self) -> anyhow::Result<()> {
        if self.dirty_since.is_some_and(|t| t.elapsed() > Duration::from_secs(1)) {
            self.save()?;
        }
        Ok(())
    }

    /// Atomar speichern (temporäre Datei + Umbenennen), vorherigen Stand als `.bak` behalten.
    pub fn save(&mut self) -> anyhow::Result<()> {
        let file = self.root.join("data.json");
        let tmp = self.root.join("data.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&self.data)?)?;
        if file.exists() {
            let _ = std::fs::copy(&file, self.root.join("data.json.bak"));
        }
        std::fs::rename(&tmp, &file)?;
        self.dirty_since = None;
        Ok(())
    }

    /// Kopiert eine Bilddatei in den Asset-Ordner, liefert den Asset-Namen.
    pub fn import_asset_file(&self, path: &Path) -> anyhow::Result<String> {
        let data = std::fs::read(path)?;
        image::load_from_memory(&data).with_context(|| format!("{} ist kein unterstütztes Bild", path.display()))?;
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png");
        self.add_asset(&content_name(&data, ext), &data)
    }

    pub fn add_asset(&self, name: &str, data: &[u8]) -> anyhow::Result<String> {
        let target = self.assets_dir().join(name);
        if !target.exists() {
            std::fs::write(&target, data)?;
        }
        Ok(name.to_string())
    }

    pub fn load_token() -> Option<String> {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()?.get_password().ok()
    }

    pub fn store_token(token: &str) -> anyhow::Result<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
        if token.is_empty() {
            let _ = entry.delete_credential();
        } else {
            entry.set_password(token)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prüft die Migration an einer Kopie echter Daten: `SLIDEBEAR_MIGRATE_CHECK=/ordner cargo test`.
    #[test]
    fn migrate_real_copy() {
        let Some(dir) = std::env::var_os("SLIDEBEAR_MIGRATE_CHECK") else { return };
        let mut store = Store::open(PathBuf::from(dir)).unwrap();
        let d = &store.data;
        let today = chrono::Local::now().date_naive();
        println!("series: {}", d.series.len());
        for s in &d.series {
            println!("  {} slide={} enabled={}", s.title, s.slide.is_some(), s.enabled);
        }
        let with_slide = d.events.iter().filter(|e| slidebear_core::slide_for(e, &d.series).is_some()).count();
        println!("events: {} mit Slide: {with_slide}", d.events.len());
        let groups = slidebear_core::export::groups(&d.events, &d.series, &d.settings.hide_rules, today, today + chrono::Duration::days(60));
        for g in &groups {
            println!("  Zeile: {} {} (+{})", g.next.fields().title, g.next.fields().start, g.more);
        }
        let plan = slidebear_core::export::plan(&d.events, &d.series, &d.settings.hide_rules, today, d.settings.export_days);
        println!("export: {:?}", plan.iter().map(|p| p.file.clone()).collect::<Vec<_>>());
        store.save().unwrap();
        let reloaded = Store::open(store.root().to_path_buf()).unwrap();
        assert_eq!(reloaded.data.series.len(), store.data.series.len());
        assert!(reloaded.data.events.iter().all(|e| e.template_id.is_none() && e.custom_scene.is_none()));
    }
}
