---
feature: capability-only-io
status: archived
archive-after:
touches:
  - plugins/**
  - crates/sdk/**
  - crates/host/src/**
  - crates/host/tests/**
  - wit/**
  - CHANGELOG.md
---

# Tech Spec — Datei- und Prozesszugriffe nur über Capabilities (`capability-only-io`)

Setzt das PRD `docs/prd/capability-only-io.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern.

## Architektur

```text
Plugin ──ctx.capabilities──► HostCapabilities
                               ├─ CapabilityManager (Host): prüft Grant "filesystem", verbietet "..", nutzt std::fs
                               └─ NoopCapabilities (Standalone-CLI / Tests): std::fs innerhalb der WASI-Preopens
```

Nur die Capability-Implementierungen berühren das Dateisystem; Plugins nie.

## Schnittstellen

```rust
// crates/sdk/src/lib.rs, trait HostCapabilities
/// Last modification time of `path` as Unix seconds.
fn fs_modified(&self, path: &str) -> PluginResult<u64> {
    let _ = path;
    Err(PluginError::CapabilityDenied("filesystem".to_string()))
}
```

- `CapabilityManager::fs_modified`: `check_capability("filesystem")`, `..` → `PluginError::Io("Path traversal forbidden")`, sonst `std::fs::metadata(path)?.modified()` → Unix-Sekunden.
- `NoopCapabilities::fs_modified`: wie `fs_read` (`..` verboten), Unix-Sekunden.
- WIT (`wit/media-plugin.wit`, `interface host-capabilities`): `fs-modified: func(path: string) -> result<u64, string>;`
- `timeline`: `ctx.capabilities.fs_modified(path).ok()` → `DateTime<Utc>`; Fehler bedeuten „keine Änderungszeit“ (kein Fallback auf `std::fs`).
- `metadata`-CLI (`src/main.rs`): `size_bytes` aus der Länge von `ctx.capabilities.fs_read(path)`.

## Datenmodell

Keine Änderung.

## Umsetzung der Akzeptanzkriterien

| AC | Umsetzung | Test (`crates/host/tests/capability_io_test.rs`) |
|---|---|---|
| AC-1 | Quelltext-Prüfung aller `.rs`-Dateien unter `plugins/*/src` | `test_ac1_no_direct_io_in_plugins` |
| AC-2 | `PluginRuntime` mit `capabilities: []` je Plugin, echte Datei, Ausgabeverzeichnisse prüfen | `test_ac2_no_file_access_without_grant` |
| AC-3 | `fs_modified` in SDK, Host, WIT | `test_ac3_fs_modified_capability` |
| AC-4 | `timeline` über `fs_modified` | `test_ac4_timeline_uses_fs_modified` |

## Sicherheit

Schließt die letzten direkten Dateizugriffe der Plugins (Verfassung *Security* 1–2, #25).

## Offene Punkte

- Durchsetzung im Wasm-Sandbox-Modus erfolgt erst mit #30 (Komponenten-Ausführung).
