---
feature: no-invented-capture-time
status: draft
archive-after:
touches:
  - plugins/timeline/**
  - crates/host/src/**
  - crates/host/ui/**
  - crates/host/tests/**
  - CHANGELOG.md
---

# Tech Spec — Keine erfundene Aufnahmezeit (`no-invented-capture-time`)

Setzt das PRD `docs/prd/no-invented-capture-time.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern. Grundlage ist [ADR 0002](../adr/0002-plugins-without-engine-no-output.md).

## Architektur

`timeline` → `doc.timeline.taken_at` → `weather-enrich`, `MediaOrganizer` (Watcher), Storage (`timelines.taken_at`), Web-UI.

## Schnittstellen

- `plugins/timeline/src/lib.rs`: Der Block „Fallback: current time“ entfällt. Ohne Treffer: `taken_at = None`, `resolution_source = "none"`, `warn` `timeline: no capture time found for '<path>' (sources: <liste>)`. Die Konstante `RESOLUTION_NONE = "none"` ist öffentlich.
- `crates/host/src/watcher.rs` `MediaOrganizer::format_destination_path`: `taken_at` → Jahr/Monat; `None` → `Unknown_Year`/`00` (kein Rückgriff auf `imported_at`).
- `crates/host/ui/index.html`: Detailansicht zeigt `unknown`, wenn `taken_at` fehlt.
- `weather-enrich`: unverändert (fragt seit #40 ohne `taken_at` nicht an); AC-2 wird per Test abgesichert.

## Datenmodell

Keine Änderung; `TimelineInfo.taken_at` ist bereits `Option`. In `timelines.taken_at` steht bei `None` eine leere Zeichenkette (wie bisher für fehlende Werte).

## Umsetzung der Akzeptanzkriterien

| AC | Umsetzung | Test (`crates/host/tests/capture_time_test.rs`) |
|---|---|---|
| AC-1 | `timeline` | `test_ac1_no_source_no_capture_time` |
| AC-2 | `weather-enrich` (bestehend) | `test_ac2_no_weather_query_without_capture_time` |
| AC-3 | `MediaOrganizer` | `test_ac3_watcher_unknown_date_folder` |

## Sicherheit

Keine Auswirkungen.

## Offene Punkte

- Keine.
