---
feature: no-synthetic-derivatives
status: archived
archive-after:
---

# Plan — Keine erfundenen Hashes, Wetterdaten und Thumbnails (`no-synthetic-derivatives`)

Reihenfolge der Umsetzung. Jede Task nennt das Akzeptanzkriterium, auf das sie einzahlt.

## Tasks

- [x] AC-1 `duplicate`: Pfad-Fallback entfernen, Lesefehler warnen.
- [x] AC-2 `duplicate`: `compute_dhash` über dekodierte Pixel; `DuplicateInfo.phash` als `Option<String>`, Speicherung und Web-UI anpassen.
- [x] AC-3 `weather-enrich`: Klima-Fallback und Ersatzdatum entfernen, Warnungen, kein Cache ohne Messung.
- [x] AC-4 `weather-enrich`: `offline_fallback` über `not_supported` ignorieren.
- [x] AC-5 `thumbnail`: Skalierung mit `image`, tatsächliche Maße, EXIF-Orientierung.
- [x] AC-6 `thumbnail`: Fehlerpfade ohne Ersatzbild, ohne `sips`/`std::fs`.
- [x] AC-2 Tests je Kriterium in `crates/host/tests/derivatives_test.rs`; bestehende Tests anpassen; READMEs, Streaming-Doku und CHANGELOG.
