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

# Tech Spec — Plugins laufen als eingebettete Wasm-Komponenten (`wasm-component-runtime`)

Setzt das PRD `docs/prd/wasm-component-runtime.md` und [ADR 0003](../../adr/0003-wasm-component-execution.md) um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern.

## Architektur

```
config.yaml ──► PluginRuntime::new ──► ComponentEngine (Engine, Linker, Epoch-Ticker)
                    │  lädt je aktiviertem Pipeline-Plugin: Component → MediaPluginPre
                    ▼
Pipeline::process ──► PluginRuntime::execute_plugin ──► frischer Store<HostState>
                                                          ├─ WasiCtx ohne Ambient Authority
                                                          ├─ CapabilityManager (Grants des Plugins)
                                                          ├─ MemoryLimiter (max_memory_bytes)
                                                          └─ Epoch-Deadline (timeout_ms)
                                     process(document, settings) ─► Gast: flagme_sdk::component::run
                                                                     └─ Plugin::process(UnifiedDocument, PluginContext)
```

**WIT (`wit/media-plugin.wit`, `wasmgeohost:media@0.2.0`)**
- `types` bildet `UnifiedDocument` vollständig ab.
  - Zeitstempel sind RFC-3339-Strings in UTC, `HashMap<String, String>` wird zu `list<tuple<string, string>>`, `usize` zu `u64`.
  - `custom` wird als JSON-Text `custom-json` übergeben.
  - `plugin-error` spiegelt `PluginError`.
- `host-capabilities` enthält `fs-read`, `fs-write`, `fs-modified`, `http-get`, `kv-get`, `kv-set` und `secret-get`, jeweils mit `result<_, plugin-error>`, sowie `log-event`.
- Die Welt `media-plugin` exportiert `name`, `version` und `process(doc, settings)`.

**SDK (`crates/sdk/src/component.rs`)**
- `wit_bindgen::generate!` mit `pub_export_macro`.
- `ComponentCapabilities` implementiert `HostCapabilities` über die Host-Imports.
- `run` wandelt das Dokument um, baut den `PluginContext` aus den `settings` und ruft `Plugin::process`.
- Das Makro `export_plugin!(Typ, Konstruktor)` erzeugt die `Guest`-Implementierung. Die Plugin-Instanz liegt in einem `OnceLock`, also eine Instanz je Komponenteninstanz.
- Das Makro ist nur aktiv mit `cfg(all(target_arch = "wasm32", feature = "component"))`. Ohne das Feature des Plugin-Crates exportiert die Bibliothek nichts. Das ist nötig, weil das CLI-Binary dieselbe Bibliothek linkt und sonst `process` mitexportieren und `host-capabilities` importieren würde (AC-8).

**Plugins**
- Jedes Plugin hat das Feature `component = []` und ruft am Ende von `lib.rs` `flagme_sdk::export_plugin!(XyzPlugin, XyzPlugin::new);` auf.
- Plugin-Code bleibt unverändert.

**Host**
- **`crates/host/src/component.rs`:**
  - `wasmtime::component::bindgen!` für die Welt `media-plugin`;
  - `HostState` mit `WasiView`, `host_capabilities::Host` und `types::Host`;
  - `ComponentEngine` lädt (`load`), liest die Identität (`component_identity`) und führt aus (`process`);
  - Umwandlungen auf der Host-Seite.
- **`crates/host/src/runtime.rs`:**
  - `PluginRuntime::new(config) -> anyhow::Result<Self>` lädt alle aktivierten Plugins aus `config.pipeline`.
  - Fehler beim Laden: Konfigurationseintrag fehlt, Datei fehlt, Welt passt nicht, oder der `name`-Export weicht vom Schlüssel ab.
  - `register_plugin` entfällt.
- **`crates/host/src/stream.rs`** (`StreamSupervisor`) entfällt. In `main.rs` läuft `stream` über denselben Weg wie `process`.
- **`crates/host/Cargo.toml`:** `wasmtime = "51"` und `wasmtime-wasi = "51"`. Die Plugin-Crates stehen nur noch unter `[dev-dependencies]`, für die Tests der Plugin-Logik.

**Build:** Die Komponenten entstehen mit `cargo build --release --target wasm32-wasip2 --lib --features component -p …`. `Cargo.toml` optimiert die Cranelift- und Wasmtime-Crates auch im Dev-Profil (`opt-level = 3`). Sonst dauert das Kompilieren der 11 Komponenten in Debug-Builds und Tests etwa 10 s statt etwa 2 s je Runtime-Start.

## Schnittstellen

- **`config.yaml`**
  - Neuer Schlüssel `plugin_dir`, Standard `target/wasm32-wasip2/release`.
  - Die Komponente eines Plugins ist `plugins.<name>.path`, sonst `<plugin_dir>/flagme_plugin_<name>.wasm`, wobei `-` zu `_` wird (`AppConfig::component_path`).
  - `config.docker.yaml` setzt `/app/plugins`.
  - `AppConfig::default()` setzt `path` nicht mehr; früher stand dort `plugins/<name>.wasm`, ohne genutzt zu werden.
- **CLI:** `process`, `watch` und `stream` scheitern beim Start, wenn eine Komponente fehlt. Die Meldung nennt die Datei. `stream` braucht keine `wasmtime`-CLI mehr.
- **Logging:** `log-event` des Gasts landet über `CapabilityManager::log` im Host-Logging mit dem Plugin-Namen. Meldungen, die mit `settings[` beginnen, gibt der Host je Plugin und Meldung nur einmal aus. Der Grund: Jede Instanz ist frisch, der `SettingsCache` des Gasts kann Wiederholungen also nicht mehr unterdrücken (AC-9).
- **Fehler:** Einen Trap übersetzt der Host in `PluginError::ProcessingFailed`, je nach Ursache:
  - `guest exceeded max_memory_bytes (<n> bytes)`;
  - `guest exceeded timeout_ms (<n> ms)`;
  - sonst `guest trapped: …`.

  Fehler, die der Gast zurückgibt, behalten ihre Art.

## Datenmodell

Keine Änderung an `crates/models` oder an der Persistenz. Der WIT-Vertrag ist eine verlustfreie Abbildung von `UnifiedDocument`.

## Umsetzung der Akzeptanzkriterien

| AC | Umsetzung | Test |
|---|---|---|
| AC-1 | Feature `component` und `export_plugin!` in allen Plugins; `PluginRuntime::new` prüft Welt, `name` und `version` | `component_runtime_test::test_ac1_all_plugins_are_components` |
| AC-2 | Plugin-Crates nur als `dev-dependencies`; Laden über `AppConfig::component_path`, ohne Rückfall | `component_runtime_test::test_ac2_no_native_fallback` |
| AC-3 | `WasiCtxBuilder` ohne Preopens, Umgebung, stdin und Netzwerk; Capabilities über `CapabilityManager` | `component_runtime_test::test_ac3_no_ambient_authority` (Test-Probe) |
| AC-4 | `MemoryLimiter` als `ResourceLimiter`; Epoch-Ticker alle 10 ms und `set_epoch_deadline`; ein frischer `Store` je Aufruf | `component_runtime_test::test_ac4_resource_limits` |
| AC-5 | `settings` werden als `list<tuple<string, string>>` an `PluginContext.settings` übergeben | `component_runtime_test::test_ac5_settings_reach_guest` |
| AC-6 | Umwandlungen in `crates/sdk/src/component.rs` und `crates/host/src/component.rs`; `plugin-error`-Variante | `component_runtime_test::test_ac6_document_round_trip` |
| AC-7 | `stream` in `main.rs` über `PluginRuntime`; `StreamSupervisor` entfernt | `component_runtime_test::test_ac7_stream_uses_embedded_runtime` |
| AC-8 | `cfg(feature = "component")` im Export-Makro | `component_runtime_test::test_ac8_cli_binary_stays_plain_command` |
| AC-9 | Host-seitige Deduplizierung der Meldungen `settings[…]` | `component_runtime_test::test_ac9_settings_warning_once` |

**Test-Probe:** `crates/host/tests/probe-plugin` (`flagme-test-probe`, `publish = false`) ist eine Komponente nur für Tests.
- Je nach `mode` aus `doc.custom` oder den Settings versucht sie absichtlich, was ein Plugin nicht darf: `std::fs`, Umgebungsvariablen, TCP, unbegrenzten Speicher, Endlosschleife.
- Sie gibt das Dokument auch unverändert zurück (Round-Trip) oder meldet einen Fehler.
- Sie gehört zu keiner Pipeline und keiner Konfiguration und wird nicht ausgeliefert. `common::components_dir()` baut sie zusammen mit den Plugins.

**Bestehende Tests:**
- `ac_test` AC-3/AC-9, `watcher_test` und `logging_test` laufen auf den Komponenten (`common::with_components`).
- `capability_io_test` AC-2 führt die Plugins ohne Engine als Komponenten aus. Die Plugins mit Test-Engine (`ocr-text`, `ai-tagging`, `face-detection`) laufen nativ gegen einen `CapabilityManager`, denn Test-Engines dürfen nicht in die ausgelieferten Komponenten (ADR 0002).
- `examples_test` (`wasm-geo-host` AC-12) vergleicht die dokumentierte Ausgabe mit einem echten `stream`-Lauf.

## Sicherheit

- **_Security_ 1 (keine Ambient Authority):**
  - Der WASI-Kontext hat keine Preopens, keine Umgebungsvariablen, kein stdin, kein stdout und kein Netzwerk; `allow_tcp`, `allow_udp` und `allow_ip_name_lookup` stehen auf `false`.
  - Erlaubt sind Uhren und Zufall (keine Ressourcen) sowie stderr für Panic-Meldungen.
- **_Security_ 2 (Capabilities):** Jeder Import aus `host-capabilities` geht durch den `CapabilityManager` des Plugins mit den Grants aus `config.yaml`. KV-Speicher und Secrets sind Host-Zustand.
- **_Security_ 3 (Grenzen):**
  - Der Linear Memory ist über den `ResourceLimiter` je `Store` begrenzt, die Laufzeit über Epoch Interruption.
  - Host-seitige Puffer zählen nicht mit, etwa die Bytes von `fs-read` vor der Übergabe.
  - Die Grenzen gelten global aus `resource_limits`.
- **_Architektur/Layering_ 2–3:** Der Vertrag steht in WIT. Plugins hängen weiter nur vom SDK ab; der Adapter liegt im SDK. Der Host hängt von keinem Plugin ab.

## Offene Punkte

- **Startzeit:** Ein Cache für kompilierte Komponenten (`Component::serialize` oder Wasmtime-Cache) würde den Start beschleunigen. Ein Release-Build braucht etwa 1 s für 11 Komponenten. Das ist eine spätere Erweiterung (ADR 0003).
- **Grenzen je Plugin:** sind eine spätere Erweiterung.
- **`dist/`:** Die Pakete (v0.1.1) werden erst beim nächsten Release neu gebaut.
- **Docker:** Die Docker-Build-Datei ist angepasst, der Image-Build ist ungeprüft (lokal kein Docker-Daemon).
- **`CapabilityManager::http_get`:** Ist das Netz nicht erreichbar, antwortet es mit einem erfundenen JSON (`{"status": "offline", …}`) statt mit einem Fehler. Das ist ein bestehender Fall für ADR 0002 und liegt außerhalb dieses Features; Kandidat für ein eigenes Issue.
