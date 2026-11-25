---
feature: weather-enrich
status: draft
archive-after:
touches:
  - "crates/models/**"
  - "plugins/weather-enrich/**"
  - "crates/host/**"
  - "crates/host/migrations/**"
  - "Cargo.toml"
---

# PRD — weather-enrich (`weather-enrich`)

## Problem

Fotos erzählen reichhaltigere Geschichten, wenn sie mit den Umwelt- und Wetterbedingungen zum exakten Aufnahmezeitpunkt kontextualisiert werden. Kameras speichern zwar Zeitstempel (`taken_at`) und GPS-Koordinaten (`latitude`, `longitude`), jedoch keine meteorologischen Messwerte (Temperatur, Wetterlage wie Sonnig, Regen, Schnee, Bewölkung).

## Ziel

Bereitstellung des Plugins `flagme-plugin-weather-enrich`, das anhand von Zeitstempel und Koordinaten historische Wetterdaten (Open-Meteo Historical Weather API & WMO Interpretation Codes) abfragt, über den Host-KV-Cache zwischenspeichert, im `UnifiedDocument` als `WeatherInfo` abbildet und in der SQLite-Datenbank `media.db` persistiert.

## Nutzer

- Fotografen und Foto-Plattformen, die Medien nach atmosphärischen Bedingungen ("Schnee", "Gewitter", "über 30°C", "goldene Stunde") filtern, suchen und anzeigen möchten.

## Umfang

Die Umsetzung umfasst:
- Definition von `WeatherInfo` in `wasm-geo-models` mit Serde-Serialisierung.
- Plugin `flagme-plugin-weather-enrich` mit CDYLib & WASI-Binärunterstützung.
- KV-Cache-Prüfung (`weather:<lat_2dec>:<lon_2dec>:<date_hour>`) vor ausgehenden HTTP-Abfragen.
- WMO-Code-Mapping (0–99) auf verständliche Zustandsbeschreibungen ("Clear Sky", "Moderate Rain", etc.) und Emoji-Icons ("☀️", "🌧️", "❄️").
- SQLite-Schema-Erweiterung um Tabelle `weather` mit Indizes auf `condition` und `temperature`.
- Host-Pipeline-Registrierung und SQLite-Persistierung in `Storage::save_document`.
- Integrationstests je Kriterium mit Kennzeichnung.

## Nicht-Ziele

- Keine kostenpflichtigen oder API-Key-pflichtigen Wetterdienste (ausschließlich lizenzfreies Open-Meteo).
- Keine Wettervorhersagen (ausschließlich historische Reanalyse-Daten zum Aufnahmezeitpunkt).

## Akzeptanzkriterien

- **AC-1** Weather Model: `wasm-geo-models` stellt `WeatherInfo` mit vollständiger Serde-Serialisierung bereit (`temperature`, `condition`, `wmo_code`, `icon_emoji`, `humidity_percent`, `precipitation_mm`, `is_day`).
- **AC-2** Plugin Compilation: `flagme-plugin-weather-enrich` kompiliert als Plugin-Crate mit `cdylib` und `[[bin]]` unter `wasm32-wasip2`.
- **AC-3** KV Cache Verification: Das Plugin prüft vor dem Absetzen von HTTP-Anfragen den Host-KV-Cache und nutzt bei Treffern das gecachte Ergebnis.
- **AC-4** WMO Code Resolution: WMO-Codes werden deterministisch auf menschenlesbare Wetterbeschreibungen und passende Emoji-Icons abgebildet.
- **AC-5** SQLite Relational Persistence: `Storage::save_document` persistiert Wetterdaten in der relationalen Tabelle `weather`.
- **AC-6** End-to-End Pipeline Integration: Integrationstests verifizieren die Anreicherung von Dokumenten mit Zeitstempel und GPS-Koordinaten sowohl bei Cache-Miss als auch bei Cache-Hit.
