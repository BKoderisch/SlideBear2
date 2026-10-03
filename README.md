# SlideBear

Veranstaltungs-Slides für ProPresenter: Termine kommen aus ChurchTools, werden über Vorlagen automatisch
als PNG gerendert und in einen Export-Ordner gelegt.

## Starten

```bash
export PATH=/opt/homebrew/opt/rustup/bin:$PATH
cargo run --release -p slidebear
```

Daten liegen unter `~/Library/Application Support/de.ProKode.SlideBear/` (Windows: `%APPDATA%\ProKode\SlideBear\data`).
Mit `SLIDEBEAR_DATA=/pfad` lässt sich ein anderer Ordner nutzen (z. B. zum Testen).

## Einrichtung

1. **Einstellungen → ChurchTools**: Adresse, Login-Token (ChurchTools → Profil → Login-Token), „Speichern“,
   „Verbindung testen“, Kalender anhaken.
2. **Einstellungen → Export**: Ordner wählen, den ProPresenter einliest.
3. **Vorlagen**: Standard-Layout, eigenes Hintergrundbild oder PPTX-Import.
4. **Termine**: nach dem Sync einen Termin wählen, Vorlage setzen, „Vorlage für die ganze Serie übernehmen“.
   Ab dann bekommt jede Wiederholung automatisch ihre Slide.
5. **⬆ Exportieren** (oder „Nach jedem Sync automatisch exportieren“).

## Am Sonntag: Schnellexport

Die App startet im Tab **⚡ Schnellexport**: alle Termine der nächsten Tage als Tabelle mit Export-Häkchen,
Termin, Datum, Beginn, Ende und Vorlage. Alles ist direkt editierbar (Tab springt zum nächsten Feld),
Vorschau beim Darüberfahren, Doppelklick öffnet den Editor. Ein Klick auf **⬆ N Slides exportieren** schreibt
alle angehakten Slides in den Export-Ordner.

## Darstellung

Eisbär-Theme in zwei Varianten: **❄ Polarnacht** (dunkel, Standard) und **☃ Schnee** (hell), umschaltbar unter
Einstellungen → Darstellung, dazu die Oberflächengröße (Normal/Groß/Sehr groß, auch per Cmd/Strg +/-).
Logo und App-Icon sind ein selbst gezeichneter Eisbär (`crates/app/src/theme.rs`).

## Platzhalter

`{titel}` `{datum}` `{zeit}` `{ort}` `{untertitel}` `{wochentag}`. Das Datumsformat und die Zeitdarstellung
(`15-19 Uhr`, `19:30 Uhr`) werden pro Vorlage eingestellt. Lokale Änderungen an ChurchTools-Terminen bleiben
beim Sync erhalten und lassen sich pro Feld zurücksetzen.

## Aufbau

| Crate | Aufgabe |
|---|---|
| `crates/core` | Datenmodell (Scene, Event, Template), Formatierung, Platzhalter, Sync-Merge, Export-Plan, Smart-Platzhalter |
| `crates/render` | Scene → Bild mit tiny-skia + cosmic-text (Roboto mitgeliefert, OFL) |
| `crates/pptx` | PPTX → bearbeitbare Scene (Text, Bilder, Formen, Hintergründe inkl. Layout/Master) |
| `crates/churchtools` | REST-Client (Kalender, Termine) |
| `crates/app` | egui-Oberfläche: Termine, Vorlagen, freier Editor, Import, Einstellungen |

Beispiele: `cargo run -p slidebear-render --example render_demo -- ausgabe/` und
`cargo run -p slidebear-render --example import_render -- datei.pptx ausgabe/`.

## Tests

```bash
cargo test --workspace
```
