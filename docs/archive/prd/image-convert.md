---
feature: image-convert
status: archived
archive-after:
touches:
  - plugins/image-convert/**
  - plugins/face-detection/**
  - plugins/privacy-redact/**
  - plugins/ocr-text/**
  - plugins/ai-tagging/**
  - plugins/thumbnail/**
  - plugins/duplicate/**
  - crates/models/**
  - crates/sdk/**
  - crates/host/src/**
  - crates/host/tests/**
  - wit/**
  - Cargo.toml
  - Cargo.lock
  - Makefile
  - config*.yaml
  - NOTICE
  - README.md
  - CHANGELOG.md
  - docs/arc42.md
  - docs/pipeline_host_supervisor.md
  - docs/plugin-development-guide.md
release: 0.5.0
---

# PRD — Arbeitsbild für alle Eingabeformate (`image-convert`)

## Problem

Der Watcher nimmt JPEG, PNG, WebP, TIFF, HEIC und AVIF an. Die Plugins, die Pixel verarbeiten, dekodieren aber jeweils selbst und können nur einen Teil davon:

- `face-detection`, `ocr-text` und `ai-tagging` lesen JPEG und PNG.
- `privacy-redact` liest nur JPEG.
- `thumbnail` und `duplicate` lesen kein HEIC und kein AVIF.

Ein HEIC-Foto vom Smartphone wird deshalb weder auf Gesichter geprüft noch geschwärzt. Für Betreiber, die sich auf die Schwärzung verlassen, ist das eine Datenschutzlücke. Außerdem drehen nur `thumbnail` und `duplicate` nach EXIF. Bei gedrehten Fotos beziehen sich Gesichtsrahmen und Schwärzung deshalb auf ein anderes Bild als die Thumbnails.

## Ziel

Jedes unterstützte Eingabeformat wird einmal in ein richtig gedrehtes JPEG-Arbeitsbild gewandelt ([ADR 0011](../../adr/0011-working-image-pure-rust-decoders.md)). Alle Bild-Plugins lesen dieses Arbeitsbild. Gesichtserkennung und Schwärzung funktionieren damit für JPEG/JPG, PNG, TIFF, BMP, WebP, HEIC/HEIF und AVIF.

Den Prototyp vom 2026-11-28 mit Messwerten zeigt ADR 0011. Kurz: Ein Foto mit 12 MP braucht in Wasm 0,5–0,8 s, ein Foto mit 48 MP mehr als 512 MB Speicher.

## Nutzer

- **Betreiber und Fotografen:** Fotos vom Smartphone (HEIC) und aus dem Web (WebP, AVIF) werden wie JPEGs erkannt, geschwärzt, durchsucht und mit Thumbnails versehen.
- **Plugin-Entwickler:** Eine SDK-Funktion nennt das Bild, das ein Bild-Plugin lesen soll. Es muss nur noch JPEG dekodieren.

## Umfang

- **Datenmodell und WIT:**
  - `UnifiedDocument.working_image: Option<WorkingImage>` mit den Feldern
    - `path`;
    - `source_format`: `jpeg`, `png`, `tiff`, `bmp`, `webp`, `heic` oder `avif`;
    - `width` und `height` des Arbeitsbilds;
    - `converted: bool`.
  - Das WIT-Paket wird zu `wasmgeohost:media@0.3.0` mit dem Datensatz `working-image`. Host und SDK wandeln verlustfrei um.
- **SDK:** Die Funktion `pixel_source(&doc) -> &str` liefert `working_image.path`, falls gesetzt, sonst `file.path`.
- **Plugin `image-convert`** (neu, `plugins/image-convert`):
  1. Es liest `file.path` über `fs-read` und erkennt das Format an den Bytes, nicht an der Endung.
  2. **Kein Umweg:** Ein vollständiges JPEG mit Orientation 1 oder ohne Orientation setzt `working_image` mit `path = file.path` und `converted = false`, ohne Kopie.
  3. **Dekodieren:**
     - `image` 0.25 für JPEG, PNG, TIFF, BMP und WebP;
     - `heic-rs` (`=0.1.5`, ohne `parallel`) für HEIC/HEIF;
     - `oxideav-avif` (`=0.0.18`) für AVIF.
  4. **Drehen:** EXIF-Orientation bei JPEG, PNG, TIFF und WebP. Container-Transformationen (`irot`, `imir`, `clap`) bei HEIC und AVIF.
  5. **Kodieren:**
     - Baseline-JPEG, 8 Bit RGB, Qualität aus dem Setting `quality`;
     - Transparenz auf Weiß gelegt;
     - ICC-Profil der Quelle übernommen, sonst keine Metadaten (kein EXIF, GPS oder XMP).
  6. **Ablage:** `<output_dir>/<sha256>.working.jpg` über `fs-write`, der SHA-256 kommt aus den Quellbytes. Existiert die Datei schon, wird sie nicht neu geschrieben; Maße und Merker werden aus ihr gelesen.
  7. **Grenze:** Bilder über `max_pixels` werden vor dem Dekodieren abgelehnt. Die Maße kommen aus dem Kopf der Datei.
- **Settings von `image-convert`:**
  - `output_dir`, Standard `cache/working`;
  - `quality`, 1–100, Standard 95;
  - `max_pixels`, 1.000.000–100.000.000, Standard 50.000.000.

  Werte außerhalb des Bereichs werden mit einer Warnung auf die Bereichsgrenze gesetzt, nicht lesbare Werte durch den Standard (wie bei allen Plugins).

- **Bild-Plugins** lesen `pixel_source(&doc)` statt `file.path`:
  - `thumbnail` und der dHash von `duplicate`;
  - `ai-tagging`, `ocr-text` und `face-detection`;
  - `privacy-redact`.

  `metadata` und der SHA-256 von `duplicate` lesen weiter das Original. Das geschwärzte Bild von `privacy-redact` entsteht aus dem Arbeitsbild.

- **Watcher:**
  - BMP kommt in die Liste der Endungen, mit MIME-Typ `image/bmp`;
  - Dateien auf `.working.jpg` werden wie `.redacted.jpg` übergangen.
- **Konfiguration:**
  - `image-convert` steht in `config.yaml`, `config_mac-studio_32GB.yaml` und `AppConfig::default()` direkt nach `metadata`;
  - es ist an, mit Grant `filesystem` und `resource_limits` (Speicher 1 GB, Zeit 30 s);
  - die genauen Grenzen bestätigt AC-11.
- **Build:** `image-convert` kommt in `PLUGINS` im `Makefile` und in die Release-Pakete.
- **Testbilder** unter `crates/host/tests/fixtures/formats/` mit `SOURCES.md`:
  - abgeleitet von `faces/apollo11.jpg` (NASA, gemeinfrei) mit ImageMagick und libheif, längste Seite 800 px;
  - `apollo11.png`, `.tiff`, `.bmp`, `.webp`, `.heic`, `-10bit.avif`, `.avif`;
  - `apollo11-alpha.png` mit Transparenz;
  - `apollo11-rot90.jpg` (EXIF Orientation 6), `apollo11-rot90.heic` und `apollo11-rot90.avif` (`irot` 270°).

  Fotos des Maintainers kommen nicht ins Repository.

- **Dokumentation:**
  - README von `image-convert`;
  - Abschnitt „Arbeitsbild“ im `docs/plugin-development-guide.md`;
  - Haupt-README (unterstützte Formate);
  - `NOTICE` (`heic-rs`, `oxideav-avif`);
  - arc42, CHANGELOG;
  - Beispielausgabe in `docs/pipeline_host_supervisor.md`.

## Nicht-Ziele

- **Keine AVIF-Thumbnails.** `thumbnail` schreibt weiter JPEG; AVIF-Ausgabe folgt als eigenes Feature.
- **Kein #76.** Thumbnails aus dem geschwärzten Bild sind ein eigenes Feature.
- **Keine weiteren Formate:** kein RAW (DNG, CR3, NEF …), kein GIF, kein JPEG XL, kein mehrseitiges TIFF und keine HEIC-Sequenzen. Bei mehreren Bildern gilt nur das Hauptbild.
- **Keine Farbumrechnung:** Das ICC-Profil wird übernommen, Display P3 wird nicht nach sRGB gerechnet.
- **Kein HDR:** 10-Bit-Quellen werden auf 8 Bit gebracht; Gain-Maps werden ignoriert.
- **Kein Aufräumen des Caches:** Arbeitsbilder gelöschter Fotos bleiben liegen.
- **Kein Speichern in `media.db`:** `working_image` steht im Dokument, nicht in einer Tabelle.
- **Kein Neuverarbeiten:** Ergebnisse älterer Läufe werden nicht automatisch neu verarbeitet.

## Akzeptanzkriterien

- **AC-1** Für jedes Testbild in PNG, TIFF, BMP, WebP, HEIC, AVIF und 10-Bit-AVIF setzt `image-convert` `working_image` mit `converted = true`, `source_format` des Formats und den Maßen der Quelle. Die Datei ist ein dekodierbares JPEG mit diesen Maßen und einem PSNR von mindestens 35 dB zu `apollo11.png`.
- **AC-2** Ein vollständiges JPEG ohne Drehung (`faces/apollo11.jpg`) ergibt `working_image.path == file.path`, `converted = false` und `source_format = jpeg`. Es wird keine Datei geschrieben.
- **AC-3** `apollo11-rot90.jpg` (EXIF 6), `apollo11-rot90.heic` und `apollo11-rot90.avif` (`irot`) ergeben ein hochkant stehendes Arbeitsbild mit vertauschten Maßen. Sein PSNR zum um 90° im Uhrzeigersinn gedrehten `apollo11.png` beträgt mindestens 35 dB.
- **AC-4** `apollo11-alpha.png` ergibt ein Arbeitsbild ohne Alphakanal; durchsichtige Flächen sind weiß.
- **AC-5** Das Arbeitsbild enthält kein EXIF, kein GPS und kein XMP. Hat die Quelle ein ICC-Profil, ist es im Arbeitsbild enthalten.
- **AC-6** Eine nicht lesbare Datei, eine abgeschnittene HEIC- oder AVIF-Datei, ein nicht unterstütztes Format (GIF) und ein Bild über `max_pixels` lassen `working_image = None`. Es wird keine Datei geschrieben, und das Plugin warnt mit Pfad und Ursache.
- **AC-7** Ein zweiter Lauf mit derselben Quelle schreibt nicht neu und liefert dasselbe `working_image`. Der Dateiname ist `<output_dir>/<sha256 der Quellbytes>.working.jpg`.
- **AC-8** Über den Host findet `face-detection` im HEIC-, AVIF- und WebP-Testbild die 3 Gesichter von `apollo11`. `privacy-redact` schreibt dafür ein geschwärztes JPEG, und `privacy.faces_redacted_count` ist 3.
- **AC-9** `thumbnail`, `duplicate` (dHash), `ai-tagging`, `ocr-text`, `face-detection` und `privacy-redact` lesen `pixel_source(&doc)`. Ohne `working_image` verhalten sie sich wie bisher; mit gesetztem `working_image` lesen sie dieses Bild. `metadata` und der SHA-256 von `duplicate` lesen weiter `file.path`.
- **AC-10** Bei `apollo11-rot90.jpg` liegen die Gesichtsrahmen von `face-detection` im hochkant stehenden Arbeitsbild; die Schwärzung von `privacy-redact` trifft dort die Gesichter.
- **AC-11** `image-convert` steht in `config.yaml`, `config_mac-studio_32GB.yaml` und `AppConfig::default()` direkt nach `metadata`, ist an und hat Grant `filesystem` und eigene `resource_limits`. Ein HEIC mit 48 MP (8064 × 6048), im Test erzeugt, läuft über den Host innerhalb dieser Grenzen.
- **AC-12** `working_image` übersteht den Weg Host → Komponente → Host verlustfrei (WIT `0.3.0`), auch im JSON des Dokuments.
- **AC-13** Der Watcher nimmt `.bmp` an (MIME `image/bmp`) und übergeht Dateien auf `.working.jpg`.
- **AC-14** Settings außerhalb des Bereichs (`quality` 0 oder 101, `max_pixels` 999.999 oder 100.000.001) werden mit einer Warnung auf die Bereichsgrenze gesetzt, wie bei allen Plugins (`SettingsReader`). Nicht lesbare Werte (`quality: high`) werden mit einer Warnung durch den Standard ersetzt.
- **AC-15** `crates/host/tests/fixtures/formats/SOURCES.md` nennt für jedes Testbild Herkunft, Lizenz, Befehl der Erzeugung und SHA-256. `NOTICE` und README nennen `heic-rs` und `oxideav-avif` mit Lizenz.
