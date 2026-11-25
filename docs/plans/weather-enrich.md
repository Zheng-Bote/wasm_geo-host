---
feature: weather-enrich
status: draft
archive-after:
---

# Plan — weather-enrich (`weather-enrich`)

Reihenfolge der Umsetzung. Jede Task nennt das Akzeptanzkriterium, auf das sie einzahlt.

## Tasks

- [x] AC-1 `WeatherInfo` in `crates/models/src/lib.rs` definieren und in `UnifiedDocument` einbinden.
- [x] AC-2 Crate `plugins/weather-enrich` mit `Cargo.toml` (`cdylib` und `[[bin]]`) anlegen und im Root-Workspace registrieren.
- [x] AC-3 Plugin-Logik mit KV-Cache-Abfrage vor ausgehenden HTTP-Aufrufen implementieren.
- [x] AC-4 WMO-Code-Mapper für Wetterbedingungen und passende Emoji-Icons implementieren.
- [x] AC-5 Migration um Tabelle `weather` erweitern und `Storage::save_document` / `get_document` für Wetter persistieren.
- [x] AC-6 Integrationstests in `crates/host/tests/weather_test.rs` mit Kriterien-Referenzen (`// weather-enrich AC-<n>: ...`) erstellen und verifizieren.
