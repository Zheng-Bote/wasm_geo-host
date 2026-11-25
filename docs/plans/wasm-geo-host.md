---
feature: wasm-geo-host
status: draft
archive-after:
---

# Plan — wasm-geo-host (`wasm-geo-host`)

Reihenfolge der Umsetzung. Jede Task nennt das Akzeptanzkriterium, auf das sie einzahlt.

## Tasks

- [x] AC-1 Cargo Workspace anlegen mit `crates/models`, `crates/sdk`, `crates/host` und `UnifiedDocument`-Modell mit vollständiger Serde-Serialisierung.
- [x] AC-2 WIT-Schnittstellen (`wit/`) für Plugin- und Host-Fähigkeiten definieren und `flagme-sdk` mit Plugin-Trait und Abstraktionen implementieren.
- [x] AC-3 `flagme-host` mit Wasmtime-Laufzeitumgebung, Capability-Grant-System, Ressourcenlimits und Konfigurationsparser (`config.yaml`) aufbauen.
- [x] AC-4 Plugin `metadata.wasm` zur verlustfreien Extraktion von EXIF, IPTC, XMP und `raw_tags` implementieren.
- [x] AC-5 Plugins `reversegeo.wasm` (Online-/Offline-Geocoding mit Geo-Lookup & Cache) und `countryflag.wasm` (Offline-Länderdatenbank mit Flaggen-Emojis) implementieren.
- [x] AC-6 Plugins `thumbnail.wasm` (deterministische Generierung von 128px, 512px, 1024px) und `ai-tagging.wasm` (ONNX-Runtime-Abstraktion) implementieren.
- [x] AC-7 Plugins `duplicate.wasm` (SHA256, phash) und `timeline.wasm` (Zeitstempel-Normalisierung mit Konfliktauflösung) implementieren.
- [x] AC-8 SQLite-Speicher mit sqlx-Migrationen und konfigurierbarer Pipeline-Orchestrierungslogik in `flagme-host` umsetzen.
- [x] AC-9 Observability (Tracing), Integrationstests je Kriterium (`// wasm-geo-host AC-<n>: ...`) und englische Plugin-Dokumentation erstellen.
