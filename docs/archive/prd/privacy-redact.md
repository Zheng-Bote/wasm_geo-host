---
feature: privacy-redact
status: archived
archive-after:
touches:
  - 'crates/models/**'
  - 'plugins/privacy-redact/**'
  - 'crates/host/**'
  - 'crates/host/migrations/**'
  - 'Cargo.toml'
---

# PRD — privacy-redact (`privacy-redact`)

## Problem

Öffentlich publizierte Medien (Websites, Blogs, Galerien) enthalten häufig sensible personenbezogene Daten, die dem Datenschutz (z. B. DSGVO/GDPR) unterliegen oder Sicherheitsrisiken bergen: Gesichter unbeteiligter Passanten, Autokennzeichen, metergenaue GPS-Koordinaten privater Wohnorte sowie Hardware-Seriennummern von Kamera und Objektiven.

## Ziel

Bereitstellung des Plugins `flagme-plugin-privacy-redact` zur automatisierten Bild-Anonymisierung und Metadaten-Bereinigung. Das Plugin unterstützt optische Verpixelung (Pixelate), Weichzeichnen (Blur) und Schwärzung (Blackout) über erkannte Bounding-Boxen (z. B. aus `doc.faces`), GPS-Fuzzing (Reduktion der Dezimalstellen auf Stadtteil-Ebene) und die Entfernung von Hardware-Seriennummern aus den Metadaten. Die Ergebnisse werden in `PrivacyInfo` und in SQLite festgehalten.

## Nutzer

- Fotografen, Verlage und Plattformbetreiber, die Bildinhalte vor dem Export oder der Veröffentlichung DSGVO-konform anonymisieren müssen.

## Umfang

Die Umsetzung umfasst:

- Datenstruktur `PrivacyInfo` in `wasm-geo-models` mit Serde-Serialisierung.
- Plugin-Crate `plugins/privacy-redact` mit CDYLib & WASI-Binärsupport (`wasm32-wasip2`).
- Optische Redaktions-Algorithmen (Pixelate, Blur, Blackout) für Bounding-Boxen, angewandt auf dekodierte Pixel der Quelldatei und als gültiges JPEG neu kodiert (Issue #22).
- Metadaten-Sanitisierung: GPS-Fuzzing (z. B. Rundung auf 2 Dezimalstellen ~1 km) und Säuberung von Seriennummern (`CameraSerialNumber`, `LensSerialNumber`).
- SQLite-Persistierung von Anonymisierungs-Status und Exportpfaden.
- Host-Pipeline-Registrierung in `flagme-host`.
- Integrationstests je Kriterium mit Kennzeichnung.

## Nicht-Ziele

- Keine Wiederherstellung originaler Pixel nach destruktivem Blackout (gewollte irreversible Sicherheit).
- Keine Eingabeformate außer JPEG für die Bildredaktion; andere Formate gelten als nicht dekodierbar (AC-11).
- Keine Auswertung der EXIF-Orientierung; Bounding-Boxen beziehen sich auf das gespeicherte Pixelraster.

## Akzeptanzkriterien

- **AC-1** Model Extension: `wasm-geo-models` stellt `PrivacyInfo` bereit und serialisiert diese in `UnifiedDocument.privacy`.
- **AC-2** Plugin Compilation: `flagme-plugin-privacy-redact` kompiliert als Plugin-Crate mit `cdylib` und `[[bin]]` unter `wasm32-wasip2`.
- **AC-3** Visual Obfuscation: Das Plugin wendet Verpixelung (Pixelate) und Schwärzung (Blackout) destruktiv auf erkannte Bounding-Boxen an.
- **AC-4** GPS Coordinate Fuzzing: GPS-Koordinaten werden auf konfigurierte Dezimalstellen gerundet, wenn `fuzz_gps_coordinates` aktiviert ist.
- **AC-5** Hardware Serial Purging: Hardware-Seriennummern werden aus `raw_tags` entfernt, wenn `strip_serial_numbers` aktiviert ist.
- **AC-6** Integration Verification: Integrationstests weisen die korrekte Anonymisierung von Bildbereichen und Metadaten nach.
- **AC-7** Bounding-Box-genaue Redaktion: Pixel innerhalb jeder `DetectedFace.bbox` (normiert 0.0–1.0) weichen von der Quelle ab; Pixel außerhalb aller Bounding-Boxen bleiben unverändert (bei der Pixeloperation exakt, nach JPEG-Neukodierung innerhalb der Kodiertoleranz).
- **AC-8** Blur-Stärke: Bei `style: blur` verändert `blur_sigma` das Ergebnis messbar (größeres Sigma → geringere Pixelvarianz innerhalb der Bounding-Box).
- **AC-9** Mosaik-Blockgröße: Bei `style: pixelate` bestimmt `block_size` die Kantenlänge der einfarbigen Mosaikblöcke in Pixeln.
- **AC-10** Gültige Ausgabe: `<output_dir>/<doc.id>.redacted.jpg` ist ein dekodierbares JPEG mit denselben Abmessungen wie die Quelle.
- **AC-11** Ehrliche Protokollierung: Schlägt Lesen, Dekodieren oder Schreiben fehl oder hat keine Bounding-Box eine gültige Fläche, wird keine Regel `redacted_<N>_faces_via_<style>` und kein `redacted_image_path` gesetzt und eine Warnung geloggt; `faces_redacted_count` und `<N>` zählen nur tatsächlich bearbeitete Gesichter.
