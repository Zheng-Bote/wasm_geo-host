---
feature: wasm-geo-host
status: draft
archive-after:
touches:
  - "crates/**"
  - "plugins/**"
  - "wit/**"
  - "Cargo.toml"
  - "Cargo.lock"
  - "config.yaml"
  - "docs/**"
  - "data/**"
  - "Examples/**"
---

# PRD — wasm-geo-host (`wasm-geo-host`)

## Problem

Medienverarbeitungs-Pipelines (insbesondere für Fotos mit Metadaten, Georeferenzierung, Thumbnails und KI-Klassifizierung) sind oft monolithisch aufgebaut, schwer erweiterbar oder führen Drittanbieter-Plugins unsicher mit vollen Systemrechten aus. Es fehlt an einer hochperformanten, isolierten und modularen Plattform, die Medienpipelines deterministisch steuert.

## Ziel

Bereitstellung einer produktionsreifen, erweiterbaren Medienverarbeitungsplattform ("WASM Geo Host") in Rust, basierend auf WebAssembly Components (Wasmtime, WASI, WIT) mit strikter Capability-basierter Rechtesteuerung, konfigurierbarer Pipeline, SQLite-Persistenz und typisierter Datenweitergabe über ein einheitliches Dokumentenmodell.

## Nutzer

- Entwickler von Medienplattformen und Geodiensten, die sichere Bildanalyse- und Tagging-Pipelines betreiben.
- Plugin-Entwickler, die spezialisierte Analysemodule (Geocoding, Bildskalierung, KI) entwickeln und isoliert ausführen wollen.

## Umfang

- Rust-Workspace mit Host (`crates/host`), SDK (`crates/sdk`), Modellen (`crates/models`), Schnittstellen (`wit/`) und 7 WASM-Plugins (`plugins/*`).
- Einheitliches Dokumentenmodell (`UnifiedDocument`) mit Serde.
- Plugin-SDK (`flagme-sdk`) für isolierte Komponenten.
- Wasmtime-Host (`flagme-host`) mit Capability-Management (Filesystem, HTTP, SQLite, KV, Secrets).
- 7 WASM-Komponenten: Metadata, ReverseGeo, CountryFlag, Thumbnail, AI-Tagging, Duplicate, Timeline.
- SQLite-Datenbankschema mit Sqlx-Migrationen.
- Konfigurierbare Pipeline (`config.yaml`) und Observability (`tracing`).
- Testsuite (Unit-, Integrations-, Contract-Tests) und Dokumentation.

## Nicht-Ziele

- Keine GUI / Frontend-Oberfläche (reine Host-/CLI- und Engine-Plattform).
- Keine Verteilung auf verteilte Cluster / Kubernetes-Orchestrierung (fokussiert auf Single-Host-Engine mit Nebenläufigkeit).
- Keine proprietären Cloud-API-Abhängigkeiten im Kern.

## Akzeptanzkriterien

- **AC-1** Workspace & Unified Document Model: Der Cargo-Workspace enthält die Crates `host`, `sdk` und `models`. `UnifiedDocument` bildet Datei-, Metadaten-, Orts-, Länder-, Thumbnail-, KI-, Duplikat- und Zeitstempel-Daten typsicher und serialisierbar ab.
- **AC-2** WIT-Schnittstellen & Plugin SDK: Deklarative WIT-Definitionen in `wit/` beschreiben den Plugin-Vertrag und Host-Fähigkeiten. Das Crate `flagme-sdk` stellt den Plugin-Trait und Abstraktionen für Fehler und Logging bereit.
- **AC-3** Capability-basierter Plugin Host: `flagme-host` lädt WASM-Komponenten dynamisch via Wasmtime und gewährt Capabilities (Dateisystem, HTTP, SQLite, KV-Store, Secrets) strikt per Plugin-Konfiguration mit Ressourcenlimits.
- **AC-4** Metadata-Plugin (`metadata.wasm`): Extrahiert verlustfrei EXIF-, IPTC- und XMP-Metadaten aus Mediendateien und speichert alle Tags einschließlich `metadata.raw_tags` im Dokument.
- **AC-5** ReverseGeo- & CountryFlag-Plugins: `reversegeo.wasm` löst Koordinaten über Online- und Offline-Provider mit Caching auf; `countryflag.wasm` liefert Offline-Länderinformationen (ISO-Codes, Namen, Flaggen-Emojis, Kontinente) ohne externe Netzaufrufe.
- **AC-6** Media Processing (Thumbnail- & AI-Tagging-Plugins): `thumbnail.wasm` erzeugt deterministisch gecachte Bildgrößen (128px, 512px, 1024px) für Standardformate (JPEG, PNG, WebP, etc.); `ai-tagging.wasm` bietet eine ONNX-Runtime-kompatible Abstraktion für austauschbare Tagging-Modelle.
- **AC-7** Duplicate- & Timeline-Plugins: `duplicate.wasm` berechnet SHA256 und Perceptual Hashes (`phash`) zur Duplikaterkennung; `timeline.wasm` konsolidiert und normalisiert Zeitstempel mit Konfliktauflösung.
- **AC-8** SQLite-Persistenz & Pipeline-Steuerung: `flagme-host` orchestriert die Pipeline konfigurierbar (`config.yaml`) und persistiert verarbeitete Dokumente relational über Sqlx-Migrationen in SQLite.
- **AC-9** Observability, Tests & Dokumentation: Ausführliche Tracing-Instrumentierung, vollständige Testabdeckung mit Akzeptanzkriterien-Kommentaren (`// wasm-geo-host AC-<n>: ...`) sowie englische Plugin-Dokumentationen.
- **AC-10** Log-Ausgabe (Issue #34): `flagme-host` gibt Host- und Plugin-Logmeldungen auf stderr aus, inklusive Plugin-Name. Das Level steuert `RUST_LOG` (Standard `warn`). stdout enthält ausschließlich die Nutzausgabe (z. B. das Dokument-JSON bei `process`).
- **AC-11** Beispieldateien (Issue #35): `Examples/sample.jpg`, `sample.png` und `sample.heic` enthalten die in `Examples/README.md` dokumentierten EXIF-Werte (Kamera, Objektiv, Belichtung, GPS), die `metadata` vollständig ausliest; `sample.jpg` ist ein dekodierbares JPEG.
- **AC-12** Dokumentierte Beispielausgabe (Issue #36): Die Beispielausgabe in `docs/pipeline_host_supervisor.md` stammt aus einem echten Lauf; ihre Metadaten-, Orts-, Länder- und Zeitwerte entsprechen dem, was die Plugins aus `Examples/sample.jpg` ermitteln.
