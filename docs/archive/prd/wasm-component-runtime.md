---
feature: wasm-component-runtime
status: archived
archive-after:
touches:
  - crates/**
  - plugins/**
  - wit/**
  - config.yaml
  - config.docker.yaml
  - config.mac-studio.yaml
  - Cargo.toml
  - Cargo.lock
  - Deployments/**
  - docs/pipeline_host_supervisor.md
  - docs/design/wasm-geo-host.md
  - docs/build-instructions.md
  - docs/deployment-guide.md
  - docs/plugin-development-guide.md
  - README.md
  - CHANGELOG.md
---

# PRD — Plugins laufen als eingebettete Wasm-Komponenten (`wasm-component-runtime`)

## Problem

Die Verfassung verlangt Plugins als WebAssembly-Komponenten mit WIT-Schnittstellen (_Architektur/Layering_ 2) und isolierte Gäste mit Speicher- und Zeitgrenzen (_Security_ 3). Tatsächlich linkt der Host alle Plugins nativ und ruft sie als Rust-Trait-Objekte auf (Issue #30):

- Die `wasm32-wasip2`-Bibliotheken exportieren nichts (`world root {}`).
- `PluginConfig.path` und `resource_limits` werden gelesen, aber nicht genutzt.
- `flagme-host stream` startet die externe `wasmtime`-CLI. Capabilities, `enabled` und `settings` greifen dort nicht.
- `wit/media-plugin.wit` ist veraltet: Es fehlen Felder des Dokuments, `fs-write` und die `settings`.

Die Sandbox und die Capability-Isolation, die README, CHANGELOG 0.1.0 und die Tech Spec `wasm-geo-host` versprechen, gibt es zur Laufzeit nicht. [ADR 0003](../../adr/0003-wasm-component-execution.md) legt fest, wie das behoben wird.

## Ziel

Jedes Plugin läuft als Wasm-Komponente im eingebetteten Wasmtime des Hosts. Das gilt in jedem Modus: `process`, `watch` und `stream`.

- **Rechte:** Jede Komponente bekommt nur die Rechte, die `config.yaml` ihr gewährt.
- **Grenzen:** Speicher- und Zeitgrenzen werden durchgesetzt.
- **Vertrag:** Er steht vollständig und typisiert in WIT.

## Nutzer

- **Betreiber von `flagme-host`:** Sie verlassen sich auf die Sandbox und auf die Grants in `config.yaml`.
- **Plugin-Entwickler:** Sie implementieren weiter nur `Plugin` und exportieren die Komponente mit einer Zeile.

## Umfang

- **WIT-Vertrag** `wasmgeohost:media@0.2.0`:
  - vollständiges, typisiertes `document`;
  - `plugin-error`;
  - `fs-write`;
  - `process(doc, settings)`.
- **SDK-Adapter:** `flagme_sdk::component` mit dem Makro `export_plugin!` und den Umwandlungen zwischen den Typen von `crates/models` und WIT.
- **Export in allen 11 Plugins:** über das Cargo-Feature `component`. Die eigenständigen CLI-Binaries bleiben einfache WASI-Kommandos.
- **Host:**
  - `wasmtime` und `wasmtime-wasi` eingebettet;
  - die Komponenten werden über `plugin_dir` bzw. `path` geladen;
  - Capability-Imports an den `CapabilityManager` gebunden;
  - `StoreLimits` für den Speicher, Epoch Interruption für die Zeit;
  - keine Ambient Authority.
- **Kein nativer Modus mehr:** Die Plugin-Crates sind nur noch `dev-dependencies` des Hosts. `StreamSupervisor` entfällt, und `stream` nutzt den eingebetteten Runtime.
- **Konfiguration:** `plugin_dir` in allen `config*.yaml`.
- **Docker-Image:** Es baut die Komponenten und nutzt `config.docker.yaml`.
- **Dokumentation:** README, Plugin-READMEs, `docs/pipeline_host_supervisor.md`, CHANGELOG.

## Nicht-Ziele

- Grenzen je Plugin (vorerst gelten `resource_limits` global).
- Ein Cache für kompilierte Komponenten (`Component::serialize`).
- Echte Inferenz-Engines (ADR 0002).
- Die Pakete unter `dist/` (v0.1.1). Sie werden erst beim nächsten Release neu gebaut.
- Eine Änderung der Verfassung.

## Akzeptanzkriterien

- **AC-1** Jedes der 11 Plugins baut mit `cargo build --release --target wasm32-wasip2 --lib --features component` zu einer Komponente. Diese exportiert die Welt `media-plugin`. Der Host lädt sie, und ihr `name`-Export entspricht dem Plugin-Namen in der Konfiguration.
- **AC-2** `flagme-host process` führt die Plugins ausschließlich aus ihren Komponenten aus, ohne nativen Rückfall: Fehlt eine Komponente, beendet sich der Host mit einer Fehlermeldung, die die Datei nennt, und `flagme-host` hängt in `[dependencies]` von keinem Plugin-Crate ab.
- **AC-3** Ein Gast hat keine Ambient Authority (kein Dateisystem, keine Umgebungsvariablen, kein Netzwerk über WASI); `fs-read` liefert ohne den Grant `filesystem` den Fehler `capability-denied` und mit dem Grant die Datei.
- **AC-4** Überschreitet ein Gast `max_memory_bytes` oder `timeout_ms`, endet der Aufruf mit einem Fehler, der die Grenze nennt. Danach verarbeiten Runtime und Pipeline das nächste Dokument normal.
- **AC-5** `settings` aus `config.yaml` erreichen den Gast unverändert. Der `SettingsReader` im Gast wertet Bool-, Ganzzahl-, Gleitkomma- und Listenwerte aus.
- **AC-6** Ein vollständig befülltes `UnifiedDocument` übersteht die WIT-Grenze in beide Richtungen verlustfrei. Fehlerarten (`PluginError`) bleiben über die Grenze erhalten.
- **AC-7** `flagme-host stream` läuft über den eingebetteten Runtime: Es liefert dasselbe Ergebnis wie `process` und braucht keine `wasmtime`-CLI auf dem `PATH`.
- **AC-8** Das CLI-Binary eines Plugins (ohne das Feature `component`) importiert `host-capabilities` nicht und bleibt damit unter `wasmtime run` lauffähig.
- **AC-9** Eine Settings-Warnung erscheint je Plugin und Meldung nur einmal, obwohl jedes Dokument eine frische Instanz bekommt.
