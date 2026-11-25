---
feature: ocr-text
status: draft
archive-after:
touches:
  - "crates/models/**"
  - "plugins/ocr-text/**"
  - "crates/host/**"
  - "crates/host/migrations/**"
  - "Cargo.toml"
---

# PRD — ocr-text (`ocr-text`)

## Problem

Fotos in Medienbibliotheken enthalten häufig reichhaltigen textuellen Kontext (z. B. Straßenschilder, Haltestellenanzeigen, Denkmäler, Produktlogos, Buchcover oder handschriftliche Notizen), der in EXIF/IPTC-Metadaten nicht enthalten ist. Ohne optische Zeichenerkennung (OCR) ist eine Volltextsuche nach diesen abgebildeten Begriffen unmöglich.

## Ziel

Bereitstellung des Plugins `flagme-plugin-ocr-text` zur Extraktion sichtbarer Texte und Bounding-Box-Regionen aus Bilddateien. Die erkannten Texte werden im `UnifiedDocument` unter `OcrInfo` aggregiert, in relationalen SQLite-Tabellen (`ocr_texts`, `ocr_regions`) persistiert und in die Volltextsuche `photos_fts` für sekundenschnelle Textsuche indiziert.

## Nutzer

- Endnutzer und Forscher, die ihre Bildbestände gezielt nach abgebildetem Text, Schildern, Dokumenten oder Marken durchsuchen wollen.

## Umfang

Die Umsetzung umfasst:
- Datenstrukturen `OcrInfo`, `TextRegion` und `BoundingBox` in `wasm-geo-models` mit Serde-Serialisierung.
- Plugin-Crate `plugins/ocr-text` mit CDYLib & WASI-Binärsupport (`wasm32-wasip2`).
- Zweistufige Erkennungslogik: Textdetektion (Bounding Boxes) und Texterkennung (Transkription UTF-8 + Konfidenz).
- SQLite-Schema-Erweiterung (`ocr_texts`, `ocr_regions`) sowie Synchronisation des kombinierten OCR-Texts in die FTS5-Tabelle `photos_fts`.
- Host-Pipeline-Registrierung in `flagme-host`.
- Integrationstests je Kriterium mit Kennzeichnung.

## Nicht-Ziele

- Kein interaktiver Texteditor zur manuellen Korrektur.
- Keine Übersetzung fremdsprachiger OCR-Texte (reine Transkription).

## Akzeptanzkriterien

- **AC-1** Model Extension: `wasm-geo-models` stellt `OcrInfo`, `TextRegion` und `BoundingBox` mit vollständiger Serde-Serialisierung bereit.
- **AC-2** Plugin Compilation: `flagme-plugin-ocr-text` kompiliert als Plugin-Crate mit `cdylib` und `[[bin]]` unter `wasm32-wasip2`.
- **AC-3** Text Detection & Recognition Pipeline: Textdetektor und -erkenner extrahieren Textzeilen mit Bounding-Box-Koordinaten und Konfidenzwerten.
- **AC-4** Storage & FTS5 Synchronization: `Storage::save_document` persistiert `ocr_texts` und `ocr_regions` und indiziert den OCR-Volltext in `photos_fts`.
- **AC-5** Integration Verification: Integrationstests weisen die Erkennung, Persistierung und Auffindbarkeit von Bildtexten über die Volltextsuche nach.
