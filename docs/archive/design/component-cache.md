---
feature: component-cache
status: archived
archive-after:
touches:
  - crates/host/src/**
  - crates/host/tests/**
  - crates/host/Cargo.toml
  - Cargo.lock
  - config.yaml
  - README.md
  - docs/deployment-guide.md
  - docs/arc42.md
  - CHANGELOG.md
release: 0.3.0
---

# Tech Spec — Cache für kompilierte Wasm-Komponenten (`component-cache`)

Setzt das PRD `docs/prd/component-cache.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern. Grundlage ist [ADR 0004](../../adr/0004-component-cache.md).

## Architektur

**`crates/host/src/config.rs`, `AppConfig`**

- Neue Felder:
  - `component_cache: bool` (serde-Standard `true`);
  - `component_cache_dir: String` (serde-Standard `cache/components`, Konstante `DEFAULT_COMPONENT_CACHE_DIR`).
- Der YAML-Leser kennt beide als Schlüssel der obersten Ebene.
  - `component_cache` nimmt `true`/`false` an.
  - Ein anderer Wert lässt den Standard stehen und erzeugt eine Warnung.
- `AppConfig::component_cache_dir() -> Option<PathBuf>` liefert den Ordner, wenn der Cache an ist, sonst `None`.

**`crates/host/src/component.rs`, `ComponentEngine`**

- `ComponentEngine::new(cache_dir: Option<&Path>)`.
- Mit `Some(dir)` läuft `prepare_cache_dir(dir)` (siehe unten). Danach entsteht `wasmtime::Cache::new(CacheConfig::new().with_directory(abs))`, und `Config::cache(Some(cache.clone()))` schaltet ihn ein.
- Ein Fehler in einem dieser Schritte führt zu `tracing::warn!("component cache disabled: '<dir>': <Grund>")`. Die Engine entsteht dann ohne Cache; es gibt keinen Abbruch.
- Ist der Cache aktiv, meldet `tracing::debug!` den Ordner. Ist er abgeschaltet, meldet `debug` das ebenfalls.
- `ComponentEngine::cache_stats() -> Option<CacheStats>` mit `CacheStats { hits, misses }` liest die Zähler `Cache::cache_hits`/`cache_misses`. Bei abgeschaltetem Cache ist das Ergebnis `None`.
- `load` bleibt `Component::from_file`. Den Cache nutzt wasmtime intern (`compile_cached`). Der Host ruft keine `deserialize`-Funktion auf.

**`prepare_cache_dir(dir) -> Result<PathBuf, String>`** (Unix)

1. Ein relativer Pfad wird mit `std::env::current_dir()` absolut gemacht, weil wasmtime einen absoluten Pfad verlangt. Es wird nicht kanonisiert, damit kein symbolischer Link aufgelöst wird.
2. `symlink_metadata(dir)`:
   - **fehlt:** `create_dir_all(parent)` und danach `DirBuilder::new().mode(0o700).create(dir)`. `AlreadyExists` aus einem gleichzeitigen Start gilt nicht als Fehler; dann wird neu gelesen.
   - **anderer Fehler:** Abbruch mit `cannot inspect: …`.
3. Prüfungen auf den Metadaten, ohne einem Link zu folgen:
   - symbolischer Link → `is a symbolic link`;
   - kein Ordner → `is not a directory`;
   - `uid != rustix::process::geteuid()` → `is owned by uid <n>, not by the host user (uid <m>)`;
   - `mode & 0o022 != 0` → `is writable by group or others (mode <oct>)`.
4. Ergebnis ist der absolute Pfad.

Auf anderen Plattformen liefert die Funktion `Err("owner and permissions cannot be checked on this platform")`.

**`crates/host/src/runtime.rs`, `PluginRuntime`**

- `PluginRuntime::new` ruft `ComponentEngine::new(config.component_cache_dir().as_deref())` auf.
- Neu: `PluginRuntime::component_cache_stats() -> Option<CacheStats>`.

**`crates/host/Cargo.toml`**

- `[target.'cfg(unix)'.dependencies] rustix = { version = "1", features = ["process"] }`. Die Crate liegt über wasmtime schon im Lockfile. `geteuid` ist dort eine sichere Funktion; ein eigenes `libc`/`unsafe` entfällt.

## Schnittstellen

`config.yaml`:

```yaml
component_cache: true # Standard; false schaltet den Cache ab
component_cache_dir: 'cache/components' # relativ zum Arbeitsverzeichnis wie plugin_dir
```

JSON-Konfiguration: dieselben Schlüssel über serde.

Rust (öffentlich, für Tests):

- `ComponentEngine::new(Option<&Path>)`;
- `ComponentEngine::cache_stats()`;
- `PluginRuntime::component_cache_stats()`;
- `component::CacheStats`.

## Datenmodell

Kein Schema der Datenbank ändert sich. Der Cache-Ordner gehört wasmtime: `modules/wasmtime-<version>/<sha256-Schlüssel>`, mit zstd komprimiert und atomar geschrieben. Größe und Anzahl der Einträge folgen den Standardgrenzen von wasmtime (weiche Grenze 512 MiB).

## Umsetzung der Akzeptanzkriterien

Alle Tests liegen in `crates/host/tests/component_cache_test.rs`.

| AC   | Umsetzung                                   | Test                                                           |
| ---- | ------------------------------------------- | -------------------------------------------------------------- |
| AC-1 | `Config::cache`, `cache_stats`              | `test_ac1_second_start_hits_cache`                             |
| AC-2 | Schlüssel enthält die Wasm-Bytes (wasmtime) | `test_ac2_changed_component_recompiled`                        |
| AC-3 | Ergebnis der Pipeline unabhängig vom Cache  | `test_ac3_same_result_with_and_without_cache`                  |
| AC-4 | `component_cache_dir()` → `None`            | `test_ac4_disabled_writes_nothing`                             |
| AC-5 | `prepare_cache_dir` legt mit `0700` an      | `test_ac5_missing_dir_created_0700`                            |
| AC-6 | Prüfungen und Rückfall mit `warn!`          | `test_ac6_unsafe_dir_falls_back`, `test_ac6_warning_on_stderr` |
| AC-7 | `config.rs` (serde und YAML-Leser)          | `test_ac7_config_keys`                                         |
| AC-8 | Quelltextprüfung von `crates/host/src`      | `test_ac8_no_unsafe_no_deserialize`                            |

## Sicherheit

- _Strikte Regeln_ 1: Es gibt kein `unsafe` im Host. `geteuid` kommt aus der sicheren API von `rustix`.
- _Security_ 1–3: Ein Eintrag im Cache wird als nativer Code in den Host geladen. Die Rechteprüfung schließt Schreiber anderer Benutzer aus. Prozesse desselben Benutzers bleiben ein Restrisiko; das Restrisiko nennt ADR 0004.
- Geprüft werden der Ordner selbst und seine Art (kein Link). Übergeordnete Ordner prüft der Host nicht. Ein angreifbarer übergeordneter Ordner, etwa ein für alle beschreibbares `cache/`, könnte den Cache-Ordner austauschen; nach dem Austausch fällt er aber bei der Prüfung am nächsten Start durch. Zwischen Prüfung und Nutzung bleibt ein kurzes Zeitfenster (TOCTOU). Es wird hingenommen, weil derselbe Angreifer den Ordner nach dem Start ohnehin nicht beschreiben dürfte.
- Wasm-Gäste sehen den Cache nicht. Er liegt außerhalb jeder Capability und braucht keinen Grant.

## Offene Punkte

- Keine.
