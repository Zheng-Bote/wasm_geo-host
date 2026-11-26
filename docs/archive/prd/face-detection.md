---
feature: face-detection
status: archived
archive-after:
touches:
  - 'crates/models/**'
  - 'plugins/face-detection/**'
  - 'crates/host/**'
  - 'crates/host/migrations/**'
  - 'Cargo.toml'
release: 0.2.0
---

# PRD — face-detection (`face-detection`)

## Problem

Medienbibliotheken erfordern häufig eine Organisation nach Personen ("Fotos mit Alice", "Familienporträts", "Gruppenbilder"). Bislang extrahiert die Plattform rein technische Metadaten, Orte, Zeitstempel, Szenen-Tags und OCR-Texte, besitzt aber keine Gesichtserkennung (Face Detection) oder Gesichts-Vektorisierung (Face Embeddings).

## Ziel

Bereitstellung des Plugins `flagme-plugin-face-detection` zur Erkennung von Gesichtern (Bounding Boxes, Gesichtsmerkmale/Landmarks, Konfidenz) und Generierung von Gesichts-Embeddings (128-D / 512-D Float-Vektoren) mit konfigurierbaren Modell-Tiers (`lightweight` für Container vs. `heavy` für Workstations/GPU). Die Daten werden im `UnifiedDocument` als `FacesInfo` modelliert und in relationalen SQLite-Tabellen (`faces`, `persons`) persistiert.

## Nutzer

- Endnutzer und Medienverwaltungs-Software zur automatischen Gruppierung und Erkennung von Personen und Porträts.

## Umfang

Die Umsetzung umfasst:

- Datenstrukturen `FacesInfo`, `DetectedFace` und `FaceLandmarks` in `wasm-geo-models` mit Serde-Serialisierung.
- Plugin-Crate `plugins/face-detection` mit CDYLib & WASI-Binärsupport (`wasm32-wasip2`).
- Konfigurierbare Modell-Tiers (`lightweight` und `heavy`).
- Zweistufige Erkennungslogik: Detektion (Bounding Boxes & 5 Landmarks) und Feature-Extraktion (Face Embeddings).
- SQLite-Schema-Erweiterung (`faces`, `persons`) und Persistenz via `Storage::save_document`.
- Host-Pipeline-Registrierung in `flagme-host`.
- Integrationstests je Kriterium mit Kennzeichnung.

## Nicht-Ziele

- Keine Cloud-basierte Gesichtsidentifikation (voller Datenschutz, 100% lokale On-Device-Verarbeitung).
- Kein manuelles Frontend zur Personen-Benennung (Gegenstand von UI Issue #10).

## Akzeptanzkriterien

- **AC-1** Model Extension: `wasm-geo-models` stellt `FacesInfo`, `DetectedFace` und `FaceLandmarks` mit vollständiger Serde-Serialisierung bereit.
- **AC-2** Plugin Compilation: `flagme-plugin-face-detection` kompiliert als Plugin-Crate mit `cdylib` und `[[bin]]` unter `wasm32-wasip2`.
- ~~**AC-3** Configurable Model Tiers: Das Plugin unterstützt konfigurierbare Modell-Tiers (`lightweight` vs. `heavy`) für flexible Ressourcennutzung.~~ Entfallen: Es gibt keine eingebauten Modelle; der Detektor wird über `FaceDetector` übergeben ([ADR 0002](../../adr/0002-plugins-without-engine-no-output.md)).
- **AC-4** Relational Storage: `Storage::save_document` persistiert Gesichter mit Bounding-Boxen und Binär-Embeddings in den relationalen Tabellen `faces` und `persons`.
- **AC-5** Integration Verification: Integrationstests weisen nach, dass das Plugin Gesichter, Bounding-Boxen und Landmarks eines übergebenen `FaceDetector` übernimmt ([ADR 0002](../../adr/0002-plugins-without-engine-no-output.md)).
