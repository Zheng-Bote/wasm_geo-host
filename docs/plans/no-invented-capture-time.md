---
feature: no-invented-capture-time
status: draft
archive-after:
---

# Plan — Keine erfundene Aufnahmezeit (`no-invented-capture-time`)

Reihenfolge der Umsetzung. Jede Task nennt das Akzeptanzkriterium, auf das sie einzahlt.

## Tasks

- [x] AC-1 `timeline`: Rückgriff auf die aktuelle Zeit entfernen, `resolution_source = "none"`, Warnung.
- [x] AC-2 Test: kein Wetter-Request ohne Aufnahmezeit über die Pipeline timeline → weather-enrich.
- [x] AC-3 `MediaOrganizer`: `Unknown_Year/00` ohne `taken_at`; Web-UI „unknown“; README, CHANGELOG.
