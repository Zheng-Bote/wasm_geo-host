---
feature: no-synthetic-output
status: draft
archive-after:
touches:
  - plugins/metadata/**
  - plugins/ocr-text/**
  - plugins/ai-tagging/**
  - plugins/face-detection/**
  - plugins/privacy-redact/**
  - crates/sdk/**
  - crates/host/tests/**
  - crates/host/src/**
  - config*.yaml
  - README.md
  - CHANGELOG.md
---

# PRD — Keine synthetischen Plugin-Ergebnisse (`no-synthetic-output`)

## Problem

Die Plugins `metadata`, `ocr-text`, `ai-tagging` und `face-detection` liefern im Produktionspfad erfundene Ergebnisse: Demo-Metadaten bei nicht lesbaren Dateien, Texte und Gesichter aus Stichwörtern im Dateinamen, Tags aus Byte-Statistiken, Gesichter aus einer Signatur-Tabelle der Beispielfotos (Issue #23). Diese Daten werden gespeichert, durchsucht und von `privacy-redact` als Grundlage der Schwärzung genutzt. Das verstößt gegen die Verfassung (*Strikte Regeln* 2).

## Ziel

Nach der Umsetzung erfindet kein Plugin mehr Daten. Nicht bestimmbare Werte bleiben leer und werden mit einer Warnung gemeldet. OCR, Tagging und Gesichtserkennung laufen nur über austauschbare Engines; ohne Engine liefern sie kein Ergebnis und sind standardmäßig deaktiviert ([ADR 0002](../adr/0002-plugins-without-engine-no-output.md)).

## Nutzer

- Betreiber der Pipeline, die sich auf gespeicherte Metadaten, Suche und Schwärzung verlassen.
- Entwickler, die später echte Engines anbinden.

## Umfang

- `metadata`: synthetischen Fallback und Ersatz-Bytes bei Lesefehlern entfernen.
- `ocr-text`, `ai-tagging`, `face-detection`: alle eingebauten Pseudo-Engines entfernen (Dateinamen-Regeln, Signatur-Tabelle, Byte-Statistik, feste Tags, Hautton-Detektor mit `djpeg`, Byte-Embeddings). Engine-Schnittstellen auf Bildbytes umstellen.
- Settings, die nicht vorhandene Engines auswählen, warnen „not supported yet“.
- Mitgelieferte Konfigurationen deaktivieren die drei Plugins.
- `privacy-redact` warnt, wenn Gesichter geschwärzt werden sollen, aber kein Ergebnis der Gesichtserkennung vorliegt.
- Tests, die auf erfundenes Verhalten bauen, auf Test-Engines umstellen; READMEs und CHANGELOG anpassen.

## Nicht-Ziele

- Keine echte OCR-, Tagging- oder Gesichtserkennung (eigenes Feature mit eigenem ADR).
- Keine Bereinigung bestehender Datenbanken mit Einträgen aus früheren Läufen.

## Akzeptanzkriterien

- **AC-1** `metadata`: Ist die Datei nicht lesbar, bleibt `doc.metadata` unverändert, es entsteht kein `doc.location` und eine Warnung wird geloggt. Eine lesbare Datei ohne Metadaten (auch kleiner als 128 Byte) liefert keine Kamera-, Objektiv-, Belichtungs- oder GPS-Werte.
- **AC-2** Keine Ausgabe von `metadata`, `ocr-text`, `ai-tagging` und `face-detection` hängt vom Dateinamen oder Pfad ab: Dieselben Bytes unter verschiedenen Namen ergeben dasselbe Ergebnis.
- **AC-3** Ohne Engine lassen `ocr-text`, `ai-tagging` und `face-detection` `doc.ocr`, `doc.ai` bzw. `doc.faces` auf `None` und loggen eine Warnung; es gibt keine eingebauten Pseudo-Engines (keine Datei-Signaturen, keine Byte-Statistik, keine festen Tags).
- **AC-4** Mit einer übergebenen Engine reichen die Plugins deren Ergebnis für die per `fs_read` gelesenen Bildbytes weiter. Ist die Datei nicht lesbar oder meldet die Engine einen Fehler, bleibt das Feld `None` und eine Warnung wird geloggt. `ai-tagging` normiert Embeddings auf Länge 1 und verwirft Embeddings mit falscher Dimension.
- **AC-5** Settings `model`, `model_tier`, `tier`, `engine` und `device` werden mit einer Warnung „not supported yet“ ignoriert; `min_confidence` und `embedding_dim` wirken weiter.
- **AC-6** In `config.yaml`, `config.docker.yaml`, `config.mac-studio.yaml` und im eingebauten `AppConfig::default()` sind `ocr-text`, `ai-tagging` und `face-detection` mit `enabled: false` deaktiviert.
- **AC-7** `privacy-redact` loggt bei `redact_faces = true` und `doc.faces = None` eine Warnung und meldet keine Gesichtsschwärzung.
