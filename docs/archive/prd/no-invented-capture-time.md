---
feature: no-invented-capture-time
status: archived
archive-after:
touches:
  - plugins/timeline/**
  - crates/host/src/**
  - crates/host/ui/**
  - crates/host/tests/**
  - CHANGELOG.md
---

# PRD — Keine erfundene Aufnahmezeit (`no-invented-capture-time`)

## Problem

Findet `timeline` in keiner Quelle (EXIF, XMP, Dateiname, Änderungszeit) eine Aufnahmezeit, setzt es `taken_at` auf den Importzeitpunkt (`resolution_source = "imported_timestamp_fallback"`). Das ist eine erfundene Aufnahmezeit und verstößt gegen die Verfassung (_Strikte Regeln_ 2) und [ADR 0002](../../adr/0002-plugins-without-engine-no-output.md), Regel 1 (Issue #44). Folgen:

- `weather-enrich` fragt das Wetter des Importtages ab und speichert es als Wetter zum Aufnahmezeitpunkt.
- Der Watcher sortiert solche Fotos in den Importmonat; er greift bei fehlendem `taken_at` zusätzlich selbst auf `imported_at` zurück.
- Web-UI und Suche zeigen den Import als Aufnahmezeit.

## Ziel

`taken_at` enthält nur eine aus der Quelle ermittelte Aufnahmezeit oder bleibt leer. Der Importzeitpunkt steht weiterhin in `imported_at`.

## Nutzer

- Betreiber der Pipeline und Nutzer der Web-UI und des Watchers.

## Umfang

- `timeline`: kein Rückgriff auf die aktuelle Zeit; `resolution_source = "none"` und Warnung.
- Watcher: ohne `taken_at` der Ordner `Unknown_Year/00` (bestehende Konvention für Dokumente ohne Zeitleiste).
- Web-UI: unbekannte Aufnahmezeit als „unknown“ anzeigen.
- README und CHANGELOG.

## Nicht-Ziele

- Keine neuen Zeitquellen.

## Akzeptanzkriterien

- **AC-1** Liefert keine Quelle eine Aufnahmezeit, setzt `timeline` `taken_at = None`, `resolution_source = "none"`, setzt `imported_at` und loggt eine Warnung.
- **AC-2** `weather-enrich` sendet für ein solches Dokument keine Anfrage.
- **AC-3** Der Watcher legt Fotos ohne `taken_at` unter `Unknown_Year/00/…` ab, nicht im Importmonat.
