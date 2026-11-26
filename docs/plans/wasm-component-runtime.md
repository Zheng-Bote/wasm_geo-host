---
feature: wasm-component-runtime
status: draft
archive-after:
---

# Plan — Plugins laufen als eingebettete Wasm-Komponenten (`wasm-component-runtime`)

Reihenfolge der Umsetzung. Jede Task nennt das Akzeptanzkriterium, auf das sie einzahlt.

## Tasks

- [x] AC-6 `wit/media-plugin.wit` auf `wasmgeohost:media@0.2.0` gebracht: vollständiges `document`, `plugin-error`, `fs-write`, `process(doc, settings)`.
- [x] AC-1 SDK-Adapter `flagme_sdk::component` mit `export_plugin!` und Umwandlungen; Feature `component` und Export in allen 11 Plugins.
- [x] AC-8 Export nur mit dem Feature `component`, damit die CLI-Binaries reine WASI-Kommandos bleiben.
- [x] AC-3 `ComponentEngine` im Host (`wasmtime` 51, `wasmtime-wasi` 51): WASI ohne Ambient Authority, Capability-Imports über den `CapabilityManager`.
- [x] AC-4 `MemoryLimiter` und Epoch Interruption; Fehlertexte nennen die Grenze.
- [x] AC-2 `PluginRuntime` lädt aus `plugin_dir` bzw. `path`, ohne Rückfall; Plugin-Crates nur als `dev-dependencies`; `plugin_dir` in allen `config*.yaml`.
- [x] AC-7 `StreamSupervisor` entfernt; `stream` läuft über den eingebetteten Runtime.
- [x] AC-5 `settings` über WIT an den Gast.
- [x] AC-9 Settings-Warnungen im Host je Plugin und Meldung nur einmal.
- [x] AC-1 Test-Infrastruktur: `common::components_dir()` baut die Komponenten; Test-Probe `crates/host/tests/probe-plugin`; bestehende Runtime-Tests auf Komponenten umgestellt; Cranelift im Dev-Profil optimiert.
- [x] AC-7 `docs/pipeline_host_supervisor.md` mit echter Ausgabe; README, Build-, Deployment- und Plugin-Anleitung, Plugin-READMEs, Dockerfile, CHANGELOG.
