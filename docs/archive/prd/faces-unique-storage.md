---
feature: faces-unique-storage
status: archived
archive-after:
touches:
  - crates/host/src/**
  - crates/host/tests/**
  - plugins/face-detection/README.md
  - CHANGELOG.md
---

# PRD — Eindeutige Gesichter in SQLite (`faces-unique-storage`)

## Problem

Gesichts-IDs sind nur pro Foto eindeutig (`face-1`, `face-2`, …), `faces.id` ist aber der Primärschlüssel der Tabelle. Der Host speichert mit `INSERT OR REPLACE`; ein zweites Foto überschreibt deshalb die Gesichtszeilen früherer Fotos, und `persons.representative_face_id` zeigt auf das falsche Foto (Issue #24). Die IDs stammen von austauschbaren Detektoren ([ADR 0002](../../adr/0002-plugins-without-engine-no-output.md)); der Speicher darf sich nicht auf global eindeutige IDs verlassen.

## Ziel

Jedes Gesicht jedes Fotos hat genau eine Zeile in `faces`; erneute Verarbeitung erzeugt keine Duplikate; Personen verweisen auf existierende Gesichter.

## Nutzer

- Nutzer der Gesichter- und Personendaten in SQLite (Web-UI, Auswertungen).

## Umfang

- Zeilenschlüssel `faces.id = <photo_id>/<face.id>` in `Storage::save_document`.
- Vor dem Einfügen alte Gesichtszeilen des Fotos löschen.
- `persons.representative_face_id` auf ein existierendes Gesicht der Person setzen.
- README `face-detection` und CHANGELOG.

## Nicht-Ziele

- Keine Schemaänderung und keine Migration; das Dokument-JSON (`documents_store`) behält die IDs des Detektors.
- Keine Bereinigung bereits überschriebener Zeilen aus früheren Läufen (sie werden bei erneuter Verarbeitung ersetzt).

## Akzeptanzkriterien

- **AC-1** Zwei Fotos mit je einem Gesicht gleicher Detektor-ID ergeben zwei Zeilen in `faces`, jede mit dem richtigen `photo_id`.
- **AC-2** Erneute Verarbeitung desselben Fotos erzeugt keine zusätzlichen Gesichtszeilen; entfallene Gesichter verschwinden.
- **AC-3** `persons.representative_face_id` verweist auf eine existierende Zeile in `faces`, die zu dieser Person gehört, auch nachdem das Foto des bisherigen Repräsentanten neu verarbeitet wurde.
