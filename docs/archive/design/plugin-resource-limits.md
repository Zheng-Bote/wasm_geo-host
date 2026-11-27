---
feature: plugin-resource-limits
status: archived
archive-after:
touches:
  - crates/host/src/**
  - crates/host/tests/**
  - config.yaml
  - config.mac-studio.yaml
  - README.md
  - docs/deployment-guide.md
  - docs/pipeline_host_supervisor.md
  - docs/arc42.md
  - CHANGELOG.md
release: 0.3.0
---

# Tech Spec — Ressourcengrenzen je Plugin (`plugin-resource-limits`)

Setzt das PRD `docs/prd/plugin-resource-limits.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern. Grundlage ist [ADR 0003](../../adr/0003-wasm-component-execution.md), Entscheidung 5 („Grenzen je Plugin sind eine spätere Erweiterung“).

## Architektur

**`crates/host/src/config.rs`**

- `PluginConfig` bekommt `resource_limits: HashMap<String, serde_json::Value>` (serde-Standard leer).
  - Die Rohwerte bleiben ungeprüft stehen, wie bei `settings`.
  - Ein falscher Typ im JSON, etwa `"timeout_ms": "abc"`, lässt deshalb nicht das Parsen der ganzen Konfiguration scheitern. Er wird erst bei der Auflösung gemeldet.
- Der YAML-Leser erkennt unter einem Plugin (Einrückung 4) den Schlüssel `resource_limits:`. Die Zeilen darunter (Einrückung ≥ 6, `key: value`) landen als `Value::String` in der Map, analog zu `settings`.
- Neu: `AppConfig::plugin_limits(name) -> (ResourceLimits, Vec<String>)`. Die Funktion liefert die wirksamen Grenzen und die Warnungen:
  - Für `max_memory_bytes` und `timeout_ms` gilt jeweils:
    - `Value::Number` als `u64` > 0 oder `Value::String`, die sich als `u64` > 0 lesen lässt → Wert je Plugin;
    - fehlt der Schlüssel → globaler Wert;
    - sonst → globaler Wert und die Warnung `plugin '<name>': resource_limits.<key> = <value> is not a positive integer; using the global value <n>`.
  - Jeder andere Schlüssel führt zur Warnung `plugin '<name>': unknown resource_limits key '<key>'; ignored`.
  - Die Reihenfolge der Warnungen ist sortiert nach Schlüssel, damit sie stabil bleibt.

**`crates/host/src/runtime.rs`, `PluginRuntime`**

- `LoadedPlugin` bekommt `limits: ResourceLimits`.
- `PluginRuntime::new` bestimmt die Grenzen je aktiviertem Plugin vor dem Laden:
  - Jede Warnung geht über `tracing::warn!(plugin = %name, …)`.
  - `tracing::debug!` meldet die wirksamen Werte.
- `component_identity` und `execute_plugin` nutzen `plugin.limits` statt `config.resource_limits`.
- Neu: `PluginRuntime::plugin_limits(name) -> Option<&ResourceLimits>` für Tests und Diagnose.

**`crates/host/src/component.rs`**

- Keine Änderung der Mechanik. `store` und `guest_failure` bekommen schon heute die Grenzen als Parameter. Die Fehlermeldung nennt damit automatisch den wirksamen Wert, etwa `guest exceeded timeout_ms (… ms)`.

## Schnittstellen

`config.yaml`:

```yaml
resource_limits: # global: Standard für alle Plugins
  max_memory_bytes: 268435456
  timeout_ms: 10000

plugins:
  thumbnail:
    enabled: true
    capabilities: ['filesystem']
    resource_limits: # optional; fehlende Felder erben die globalen Werte
      max_memory_bytes: 1073741824
      timeout_ms: 30000
```

JSON: `"plugins": {"thumbnail": {"resource_limits": {"max_memory_bytes": 1073741824}}}`. Zahlen und Zahl-Strings sind gültig.

## Datenmodell

Keine Persistenz und kein WIT ändern sich. Die Grenzen sind reine Host-Konfiguration; der Gast sieht sie nicht.

## Umsetzung der Akzeptanzkriterien

Die Tests liegen in `crates/host/tests/plugin_limits_test.rs`. Sie nutzen die Test-Probe (`tests/probe-plugin`, Modi `memory` und `spin`). Sie ist unter zwei Namen konfiguriert und lädt dieselbe Komponente.

| AC   | Umsetzung                                        | Test                                                                       |
| ---- | ------------------------------------------------ | -------------------------------------------------------------------------- |
| AC-1 | `plugin_limits`, `LoadedPlugin.limits`           | `test_ac1_lower_memory_limit_per_plugin`                                   |
| AC-2 | wie AC-1                                         | `test_ac2_higher_timeout_per_plugin`                                       |
| AC-3 | Erben je Feld                                    | `test_ac3_missing_field_inherits_global`                                   |
| AC-4 | Warnungen in `plugin_limits`, `warn!` beim Start | `test_ac4_invalid_values_warn_and_fall_back`, `test_ac4_warning_on_stderr` |
| AC-5 | YAML-Leser, serde                                | `test_ac5_config_yaml_and_json`                                            |
| AC-6 | `component_identity` mit `plugin.limits`         | `test_ac6_limits_apply_when_loading`                                       |

Die Test-Probe ist unter zwei Namen konfiguriert. Ihr `name`-Export lautet aber `probe`, und `PluginRuntime::new` lehnt einen abweichenden Namen ab (`wasm-component-runtime` AC-1). Die Tests für AC-1 und AC-2 nutzen deshalb zwei Laufzeiten mit je einem Eintrag `probe`, eine mit und eine ohne Grenzen je Plugin. AC-1 belegt so, dass dieselbe Speichermenge unter den globalen Grenzen gelingt und unter der Grenze des Plugins scheitert.

## Sicherheit

- _Security_ 3 bleibt erfüllt. Jeder Gast hat weiterhin eine Speicher- und eine Zeitgrenze; ungültige Werte fallen auf die globalen Grenzen zurück und schalten keine Grenze ab.
- Ein Betreiber kann einem Plugin bewusst mehr geben als global. Das ist gewollt (PRD, Nicht-Ziele). Die Konfiguration bleibt die Vertrauensquelle, wie bei den Capabilities.

## Offene Punkte

- Keine.
