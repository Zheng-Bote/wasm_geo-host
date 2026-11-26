---
feature: wasm-geo-host
status: archived
archive-after:
touches:
  - "crates/**"
  - "plugins/**"
  - "wit/**"
  - "Cargo.toml"
  - "config.yaml"
  - "docs/**"
release: 0.2.0
---

# Tech Spec — wasm-geo-host (`wasm-geo-host`)

Setzt das PRD `docs/prd/wasm-geo-host.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern.

## Architektur

Die Architektur folgt dem WebAssembly Component Model unter Verwendung von Wasmtime als Host-Laufzeitumgebung.

```text
+-------------------------------------------------------------+
|                     flagme-host (Host)                     |
|  - Konfiguration (config.yaml)                             |
|  - Capability Manager & Sandbox (Wasmtime + WASI)          |
|  - Pipeline Orchestrator                                    |
|  - SQLite Storage Engine (sqlx)                             |
+------------------------------+------------------------------+
                               |
                   WIT Interface (Plugin Contract)
                               |
+------------------------------v------------------------------+
|                   flagme-sdk (Guest SDK)                    |
+------------------------------+------------------------------+
                               |
        +----------------------+----------------------+
        |                      |                      |
+-------v--------+     +-------v--------+     +-------v--------+
| metadata.wasm  |     | reversegeo.wasm|     | countryflag.wasm|
+----------------+     +----------------+     +----------------+
| thumbnail.wasm |     | ai-tagging.wasm|     | duplicate.wasm |
+----------------+     +----------------+     +----------------+
| timeline.wasm  |
+----------------+
```

1. **Host (`crates/host`)**: Lädt und instantiiert Wasm-Komponenten über Wasmtime. Implementiert Host-Capabilities als importierbare Wasmtime-Funktionen bzw. WASI-Host-Bindings.
2. **SDK (`crates/sdk`)**: Definiert den `Plugin`-Trait, Wrapper um generierte WIT-Bindings, Error-Typen und Helfer für Serialisierung/Logging.
3. **Models (`crates/models`)**: Enthält das serialisierbare `UnifiedDocument` mit Unterstrukturen für alle Verarbeitungsstufen.
4. **Plugins (`plugins/*`)**: Einzelne WASM-Komponenten, kompiliert auf `wasm32-wasip2` (Feature `component`, Welt `media-plugin`), die ausschließlich mit dem SDK interagieren ([ADR 0003](../../adr/0003-wasm-component-execution.md)).
5. **Storage**: Relationale Speicherung via SQLite (`sqlx`) mit automatischen Schema-Migrationen.

## Schnittstellen

1. **WIT-Definitionen (`wit/media-plugin.wit`)**:
   - `plugin-types`: Definition von `document`, `status`, `error`.
   - `plugin`: Exportiert `process(doc: document) -> result<document, string>`.
   - `host-capabilities`: Imports für Dateisystem (sandboxed read), HTTP (sandboxed fetch), SQLite, KV-Store und Secrets.
2. **CLI & Konfiguration**:
   - `flagme-host process <FILE> --config config.yaml`
   - `config.yaml`: Deklaration aktivierter Plugins, Reihenfolge der Ausführung und Capability-Grants.

## Datenmodell

Das Datenmodell `UnifiedDocument` umfasst:
- `file`: Pfad, Dateiname, Dateigröße, MIME-Type, SHA256-Hash.
- `metadata`: EXIF-, IPTC-, XMP-Werte sowie `raw_tags: HashMap<String, String>`.
- `location`: Breitengrad, Längengrad, Höhe, Genauigkeit.
- `country`: ISO-Code, Name, Flaggen-Emoji, Kontinent, Region.
- `thumbnails`: Liste generierter Miniaturbilder (Breite, Höhe, Pfad, Cache-Key).
- `ai`: Liste von KI-Tags mit Konfidenzwerten und optionaler Beschreibung.
- `duplicate`: Berechnete Hashes (phash, sha256) und erkannte Duplikat-IDs.
- `timeline`: Konsolidierte Zeitstempel (`taken_at`, `imported_at`, `modified_at`).

### SQLite Schema (`crates/host/migrations`)
- Tabellen: `photos`, `metadata`, `locations`, `countries`, `thumbnails`, `ai_tags`, `duplicates`.

## Umsetzung der Akzeptanzkriterien

| AC | Umsetzung | Test |
|---|---|---|
| AC-1 | `crates/models/src/lib.rs` (`UnifiedDocument`, Serde-Typen) | `crates/models/tests/model_serialization_test.rs` |
| AC-2 | `wit/plugin.wit` & `crates/sdk/src/lib.rs` (Plugin Trait, WIT) | `crates/sdk/tests/sdk_contract_test.rs` |
| AC-3 | `crates/host/src/runtime/` & `crates/host/src/capabilities/` | `crates/host/tests/host_capability_test.rs` |
| AC-4 | `plugins/metadata/src/lib.rs` (Exif/Xmp-Parser mit raw_tags) | `plugins/metadata/tests/metadata_extraction_test.rs` |
| AC-5 | `plugins/reversegeo/` & `plugins/countryflag/` | `plugins/reversegeo/tests/reversegeo_test.rs`, `plugins/countryflag/tests/countryflag_test.rs` |
| AC-6 | `plugins/thumbnail/` & `plugins/ai-tagging/` | `plugins/thumbnail/tests/thumbnail_test.rs`, `plugins/ai-tagging/tests/ai_tagging_test.rs` |
| AC-7 | `plugins/duplicate/` & `plugins/timeline/` | `plugins/duplicate/tests/duplicate_test.rs`, `plugins/timeline/tests/timeline_test.rs` |
| AC-8 | `crates/host/src/pipeline/` & `crates/host/src/storage/` (sqlx) | `crates/host/tests/pipeline_storage_test.rs` |
| AC-9 | Tracing in Host/SDK, vollständige Testsuite & Markdown-Doku | `crates/host/tests/integration_test.rs` |
| AC-10 | `init_logging()` in `crates/host/src/main.rs`: `tracing_subscriber::fmt` mit `EnvFilter` aus `RUST_LOG` (Standard `warn`), Writer `stderr`; Plugin-Meldungen tragen das Feld `plugin` (`CapabilityManager::log`) | `crates/host/tests/logging_test.rs` |
| AC-11 | `Examples/generate_samples.py`: TIFF mit IFD0, Exif-IFD und GPS-IFD (gültige Offsets); minimaler Baseline-JPEG-Encoder (64×48 Graustufen-Verlauf, DC-only) ohne Zusatzpakete | `crates/host/tests/examples_test.rs` |
| AC-12 | `docs/pipeline_host_supervisor.md` §3 aus echtem `flagme-host stream`-Lauf; Test führt `flagme-host stream Examples/sample.jpg` auf den Plugin-Komponenten aus und vergleicht den JSON-Block typisiert (ohne `weather`, `imported_at`, `modified_at`) | `crates/host/tests/examples_test.rs` |

## Sicherheit

- Strikte Einhaltung der Verfassung: Keine Ambient Authority für Plugins.
- Capability-Filterung im Host vor jedem Systemaufruf aus dem WASM-Gast.
- Beschränkung von Speicher (`InstanceAllocationStrategy` / Max Memory Limit) und Timeouts (Epoch Interruption) in Wasmtime.

## Offene Punkte

- Keine offenen Punkte. Architektur und Schnittstellen sind vollständig im Prompt spezifiziert.
