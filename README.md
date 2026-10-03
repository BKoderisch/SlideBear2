# SlideBear

Veranstaltungs-Slides für ProPresenter: Termine kommen aus ChurchTools, werden über ihre Slides automatisch
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
   „Verbindung testen“, Kalender anhaken. Unter „Begrüßung“ den Dienst eintragen (Standard: „Präs“, findet z. B. „Präsi“):
   Wer ihn am nächsten Sonntag hat, wird beim Start vom Eisbären mit Vornamen begrüßt.
2. **Einstellungen → Export**: Ordner wählen, den ProPresenter einliest.
3. **Layouts**: Ausgangspunkte für Slides (Standard-Layout, eigenes Hintergrundbild oder PPTX-Import).
4. **🔄 Sync**, dann im Schnellexport pro Veranstaltung **➕ Slide anlegen** und ein Layout wählen.
   Die Slide ist eine eigene Kopie und gilt für alle Termine dieser wiederkehrenden Veranstaltung.
5. **⬆ Exportieren** (oder „Nach jedem Sync automatisch exportieren“).

## Slides und Layouts

- **Layout**: Vorlage für das Aussehen, z. B. „Foto + Titel + Infozeile“. Ändern wirkt nur auf neue Slides.
- **Slide**: gehört einer Veranstaltung. Wiederkehrende ChurchTools-Termine (gleiche Serie) teilen sich eine
  Slide; Datum und Uhrzeit kommen per Platzhalter vom jeweiligen Termin. Einzeltermine haben ihre eigene.
- Eine vorhandene PowerPoint-Folie wird über **➕ Neu › 📥 Aus PPTX …** zur Slide eines Termins bzw. einer Serie;
  Datum, Uhrzeit und Titel auf der Folie werden dabei durch Platzhalter ersetzt.
- Exportiert wird pro Veranstaltung nur der **nächste** Termin. Ein einzelnes Datum lässt sich unter
  „Termine“ überspringen.

## Am Sonntag: Schnellexport

Die App startet im Tab **⚡ Schnellexport**: eine Zeile pro Veranstaltung mit Export-Häkchen, Termin, Datum,
Beginn, Ende und den Knöpfen **✏ Bearbeiten** / **➕ Neu** für die Slide. Wiederkehrende Termine erscheinen
nur einmal („🔁 +3“ zeigt weitere Termine). Vorschau beim Darüberfahren, Doppelklick öffnet den Editor.
Ein Klick auf **⬆ N Slides exportieren** schreibt alle angehakten Slides in den Export-Ordner.
Unwichtige ChurchTools-Termine blendet man per Rechtsklick auf Vorschau oder Titel aus, oder dauerhaft per
Titel-Filter unter Einstellungen → Ausgeblendete Termine.

## Darstellung

Comic-Eisbär-Theme (nur hell): Schneeweiß und Gletscherblau, dicke Tinten-Konturen, harte versetzte Schatten,
sonnengelbe Auswahl, Statuszeile als Sprechblase und die runde Schrift Fredoka (OFL, nur für die Oberfläche).
Logo und App-Icon sind ein selbst gezeichneter Comic-Eisbär (`crates/app/src/theme.rs`).
Die Größe der Oberfläche lässt sich unter Einstellungen → Darstellung ändern (auch per Cmd/Strg +/-).

## Platzhalter

`{titel}` `{datum}` `{zeit}` `{ort}` `{untertitel}` `{wochentag}`. Das Datumsformat und die Zeitdarstellung
(`15-19 Uhr`, `19:30 Uhr`) werden pro Slide eingestellt (in der Editor-Leiste). Lokale Änderungen an ChurchTools-Terminen bleiben
beim Sync erhalten und lassen sich pro Feld zurücksetzen.

## Aufbau

| Crate | Aufgabe |
|---|---|
| `crates/core` | Datenmodell (Scene, Event, Template), Formatierung, Platzhalter, Sync-Merge, Export-Plan, Smart-Platzhalter |
| `crates/render` | Scene → Bild mit tiny-skia + cosmic-text (Roboto mitgeliefert, OFL) |
| `crates/pptx` | PPTX → bearbeitbare Scene (Text, Bilder, Formen, Hintergründe inkl. Layout/Master) |
| `crates/churchtools` | REST-Client (Kalender, Termine) |
| `crates/app` | egui-Oberfläche: Schnellexport, Termine, Layouts, freier Editor, Import, Einstellungen |

Beispiele: `cargo run -p slidebear-render --example render_demo -- ausgabe/` und
`cargo run -p slidebear-render --example import_render -- datei.pptx ausgabe/`.

## Tests

```bash
cargo test --workspace
```
