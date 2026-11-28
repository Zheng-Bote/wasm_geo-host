---
feature: redacted-avif-thumbnails
status: archived
archive-after:
touches:
  - plugins/thumbnail/**
  - plugins/privacy-redact/**
  - crates/models/**
  - crates/sdk/**
  - crates/host/src/**
  - crates/host/ui/**
  - crates/host/tests/**
  - wit/**
  - Cargo.lock
  - config*.yaml
  - NOTICE
  - README.md
  - CHANGELOG.md
  - docs/arc42.md
  - docs/pipeline_host_supervisor.md
  - docs/plugin-development-guide.md
release: 0.5.0
---

# PRD — Geschwärzte Thumbnails als AVIF (`redacted-avif-thumbnails`)

## Problem

Seit 0.4.0 schwärzt `privacy-redact` echte Gesichter. Web-Oberfläche und API zeigen aber ungeschwärzte Bilder (Issue #76):

- **Reihenfolge:** `thumbnail` läuft als Stufe 5, vor `face-detection` (11) und `privacy-redact` (12).
- **Ablage:** Das geschwärzte Bild liegt in `cache/redacted`, und nichts zeigt es an.
- **Server-Rückfall:** `GET /api/photos/:id/thumbnail/:size` liefert ohne Thumbnail-Datei das Original in voller Auflösung, sonst ein erfundenes 1×1-JPEG, beides mit `Cache-Control: public`.

Mit dem Standard `redact_faces: true` bleiben Gesichter so überall sichtbar. Außerdem sind die JPEG-Thumbnails größer als nötig. Der Maintainer wünscht AVIF.

## Ziel

Thumbnails entstehen nur aus geschwärzten Bildern, wenn Schwärzen verlangt ist, und als AVIF ([ADR 0012](../../adr/0012-redacted-avif-thumbnails.md)). Scheitert das Schwärzen, gibt es keine Thumbnails (fail closed). Der Server liefert nur Thumbnail-Dateien aus.

Die Messwerte vom 2026-11-28 zeigt ADR 0012. Kurz: AVIF q70 ist etwa halb so groß wie JPEG q75 und kostet für 128/512/1024 px ≈ 2,2 s je Foto im Gast.

## Nutzer

- **Betreiber und Fotografen:** Vorschauen in Oberfläche und API zeigen keine ungeschwärzten Gesichter und laden schneller.
- **Plugin-Entwickler:** Eine SDK-Funktion nennt das Bild, aus dem Anzeigebilder entstehen dürfen.

## Umfang

- **Datenmodell und WIT:**
  - `PrivacyInfo.face_redaction` mit `applied`, `no_faces`, `disabled` oder `failed`; als WIT-`enum face-redaction`;
  - das WIT-Paket bleibt `wasmgeohost:media@0.3.0`, weil 0.3.0 noch nicht veröffentlicht ist;
  - in JSON und Speicherung als Text (`"applied"` …); ältere Dokumente ohne das Feld werden als „unbekannt“ (`None`) gelesen.
- **SDK:** `display_source(&doc) -> Option<&str>`:
  - `applied`: `privacy.redacted_image_path`;
  - `no_faces`, `disabled`, kein `doc.privacy` oder kein `face_redaction`: `pixel_source(&doc)` (ADR 0011);
  - `failed`, oder `applied` ohne Pfad: `None`.
- **`privacy-redact`:**
  - setzt `face_redaction` bei jedem Lauf;
  - `failed`, wenn `redact_faces` gilt und das Gesichtsergebnis fehlt, das Bild nicht lesbar ist oder das Schreiben scheitert;
  - der Standard von `output_dir` wird `cache/thumbnails`; die Datei heißt weiter `<id>.redacted.jpg`.
- **`thumbnail`:**
  1. liest `display_source(&doc)`. Bei `None` gibt es keine Thumbnails, `doc.thumbnails` bleibt leer, und die Warnung lautet `thumbnail: '<path>': faces could not be redacted; no thumbnails (ADR 0012)`;
  2. **AVIF** mit `oxideav-avif` (`=0.0.18`): `EncodeSpeed::Fast`, ein Thread, 4:2:0, Kacheln von höchstens 1024 px;
  3. Setting **`format`**: `avif` (Standard) oder `jpeg`;
  4. **`quality`** 1–100 gilt für das gewählte Format, Standard 70 bei AVIF und 75 bei JPEG;
  5. Dateiname `<key>_<size>.avif` bzw. `.jpg`; `cache_key` und Größenlogik bleiben;
  6. `ThumbnailEntry` bleibt unverändert; das Format folgt aus der Endung von `path`.
- **Host:**
  - **Pipeline-Prüfung** beim Laden der Konfiguration (`AppConfig`, auch `AppConfig::default()`). Ist `privacy-redact` aktiv, in der Pipeline und gilt `redact_faces` (Setting fehlt oder ist wahr), muss gelten:
    - `face-detection` ist aktiv und steht vor `privacy-redact`;
    - `thumbnail` steht, falls aktiv, nach `privacy-redact`.

    Sonst bricht das Laden mit einer Fehlermeldung ab, die Plugin und Position nennt.

  - **Reihenfolge** in `AppConfig::default()`: `thumbnail` ans Ende.

- **Server** (`get_photo_thumbnail`):
  - liefert nur eine Datei aus `doc.thumbnails` (Größe nach Name, sonst die erste);
  - fehlt sie, antwortet er mit `404`. Es gibt keinen Rückfall auf `file.path` und kein Ersatzbild; `FALLBACK_JPEG_1X1` entfällt;
  - `Content-Type` `image/avif` für `.avif`, `image/jpeg` für `.jpg`/`.jpeg`;
  - `Cache-Control: private, max-age=86400`.
- **Web-Oberfläche:** Fehlt ein Thumbnail (`404`), zeigt sie einen Platzhalter (wie bisher bei Fehlern, `onerror`), nie das Original.
- **Profile** (`config.yaml` und die vier `config_*.yaml`):
  - `thumbnail` steht am Ende der Pipeline; `format: 'avif'` wird ausdrücklich gesetzt;
  - Grenzen für `thumbnail`:

    | Profil                        | Grenzen                        |
    | :---------------------------- | :----------------------------- |
    | `config.yaml`                 | globale Grenzen (256 MB, 10 s) |
    | `config_2cores_4GB.yaml`      | `timeout_ms: 60000`            |
    | `config_cpu_8GB.yaml`         | `timeout_ms: 30000`            |
    | `config_mac-studio_32GB.yaml` | global (4 GB, 60 s)            |
    | `config_mac-studio_64GB.yaml` | global (4 GB, 60 s); die eigene Grenze von 6 GB entfällt |

  - die Mac-Studio-Profile erzeugen höchstens 1024 px (`variants: '128,256,512,1024'` statt bis 2048 px), weil 2048 px als AVIF ≈ 9,4 s je Foto kosten (Entscheidung des Maintainers vom 2026-11-28);
  - der Standard `output_dir` von `privacy-redact` gilt, die Profile setzen ihn nicht.

- **Dokumentation:**
  - READMEs von `thumbnail` und `privacy-redact`, Haupt-README (Profile, Plugins);
  - `NOTICE` (`oxideav-avif` jetzt auch in `thumbnail`);
  - `docs/plugin-development-guide.md` (Abschnitt „Anzeigebilder“);
  - arc42 (Qualitätsziel 1, Risiko), CHANGELOG, Beispielausgabe.

## Nicht-Ziele

- **Kein Neuverarbeiten:** Gespeicherte Dokumente mit alten JPEG-Thumbnails werden nicht automatisch neu verarbeitet; sie gelten weiter, bis das Foto neu verarbeitet wird.
- **Kein Schwärzen von Text oder Kennzeichen:** `license_plates_redacted_count` bleibt 0.
- **Keine Formatverhandlung:** Der Server wandelt nicht je nach `Accept` zwischen AVIF und JPEG um.
- **Kein Aufräumen:** Alte Dateien in `cache/redacted` bleiben liegen.
- **Kein Endpunkt für Bilder in voller Größe.**

## Akzeptanzkriterien

- **AC-1** Mit `AppConfig::default()` hat `apollo11.jpg` über den Host Thumbnails in allen Größen. In jeder Größe sind die Gesichtsmitten geschwärzt (bei `style: blackout` schwarz), während dieselben Stellen im Thumbnail des Originals nicht schwarz sind.
- **AC-2** Nach dem Lauf liegt nichts in `cache/redacted`. `privacy.redacted_image_path` und alle Thumbnails liegen unter dem `output_dir` von `thumbnail` (Standard `cache/thumbnails`). `face_redaction` ist `applied`.
- **AC-3** Mit `redact_faces: true` setzt `privacy-redact` `face_redaction = failed`, wenn das Gesichtsergebnis fehlt oder das Bild nicht lesbar bzw. nicht schreibbar ist. `thumbnail` schreibt dann keine Datei, `doc.thumbnails` bleibt leer, und eine Warnung nennt Pfad und Grund.
- **AC-4** Mit `redact_faces: false` (`disabled`) oder bei einem Foto ohne Gesichter (`no_faces`) entstehen die Thumbnails aus dem Arbeitsbild. Ohne `doc.privacy` (z. B. `privacy-redact` aus) ebenso.
- **AC-5** `GET /api/photos/:id/thumbnail/:size` antwortet mit `404`, wenn die Thumbnail-Datei fehlt oder das Dokument keine Thumbnails hat. Er liefert nie das Original oder ein Ersatzbild. Ein AVIF kommt mit `Content-Type: image/avif`, ein JPEG mit `image/jpeg`, beide mit `Cache-Control: private, max-age=86400`.
- **AC-6** Der Host lehnt eine Konfiguration ab, in der bei aktivem `privacy-redact` mit `redact_faces` `thumbnail` vor `privacy-redact` steht, `face-detection` nach `privacy-redact` steht oder `face-detection` fehlt bzw. aus ist. Die Meldung nennt die Plugins. Mit `redact_faces: false` oder abgeschaltetem `privacy-redact` gibt es keine Bedingung. `AppConfig::default()` und alle fünf Profile erfüllen die Regel.
- **AC-7** Mit dem Standard `format: avif` ist jede Thumbnail-Datei ein AVIF (Marke `avif` im `ftyp`). Es ist dekodierbar, hat die Maße aus `ThumbnailEntry` und endet auf `.avif`. Mit `format: jpeg` sind es JPEGs auf `.jpg` wie bisher.
- **AC-8** `format` mit einem anderen Wert warnt und nimmt `avif`. Ohne `quality` gelten 70 (AVIF) bzw. 75 (JPEG). Ein AVIF-Thumbnail mit 1024 px von `apollo11.jpg` ist kleiner als das JPEG-Thumbnail mit der Standardqualität.
- **AC-9** `face_redaction` übersteht den Weg Host → Komponente → Host und das JSON verlustfrei. Ein älteres Dokument ohne das Feld wird mit `None` gelesen, und `display_source` liefert dafür das Arbeitsbild.
- **AC-10** Ein Bild mit 4000 × 3000 Pixeln, im Test erzeugt, bekommt über den Host mit `AppConfig::default()` AVIF-Thumbnails in 128, 512 und 1024 px innerhalb der Grenzen von `thumbnail`.
- **AC-11** Die READMEs von `thumbnail` und `privacy-redact`, das Haupt-README und `docs/plugin-development-guide.md` beschreiben `format`, die Reihenfolge, `face_redaction` und „fail closed“. `NOTICE` nennt `oxideav-avif` für `thumbnail`.
