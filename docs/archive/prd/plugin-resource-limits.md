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

# PRD — Ressourcengrenzen je Plugin (`plugin-resource-limits`)

## Problem

`resource_limits.max_memory_bytes` und `resource_limits.timeout_ms` gelten heute für jedes Plugin gleich ([ADR 0003](../../adr/0003-wasm-component-execution.md), Entscheidung 5). ADR 0003 nennt Grenzen je Plugin ausdrücklich als spätere Erweiterung.

Die Plugins brauchen aber sehr verschieden viel:

- `thumbnail`, `duplicate` und `privacy-redact` dekodieren ganze Bilder. Bei hochauflösenden Dateien brauchen sie viel Speicher und Zeit.
- `countryflag` und `timeline` arbeiten nur auf Metadaten.
- `weather-enrich` und `reversegeo` warten auf das Netz.

Eine globale Grenze muss deshalb so großzügig sein wie das hungrigste Plugin. Für alle anderen schützt sie dann kaum. Die Alternative ist eine knappe Grenze, an der große Bilder scheitern.

## Ziel

Jedes Plugin kann eigene Grenzen bekommen. Die globalen `resource_limits` bleiben der Standard.

- Ein Wert je Plugin ersetzt den globalen Wert, nach oben wie nach unten.
- Ein fehlendes Feld erbt den globalen Wert.
- Ein ungültiger Wert führt zu einer Warnung, und es gilt der globale Wert.
- Ohne Einträge je Plugin verhält sich der Host genau wie bisher.

## Nutzer

- **Betreiber:** Sie geben bildlastigen Plugins mehr Speicher und Zeit und halten alle anderen knapp.
- **Plugin-Entwickler:** Sie sehen in der Fehlermeldung, welche Grenze ihr Plugin überschritten hat.

## Umfang

- Neuer optionaler Abschnitt `plugins.<name>.resource_limits` mit `max_memory_bytes` und `timeout_ms`, aus YAML und JSON.
- Die wirksamen Grenzen eines Plugins werden einmal beim Start bestimmt:
  - Feld gesetzt und gültig → Wert je Plugin;
  - sonst → globaler Wert.
- Gültig ist eine ganze Zahl größer 0. Wird ein ungültiger Wert oder ein unbekannter Schlüssel gefunden, warnt der Host einmal beim Start und nennt Plugin, Schlüssel und Wert.
- Die wirksamen Grenzen gelten für jeden Aufruf des Plugins: für `process` und für die Abfrage von `name` und `version` beim Laden.
- Eine überschrittene Grenze nennt wie bisher den Grenzwert, also den wirksamen Wert des Plugins.
- `debug`-Meldung beim Start mit den wirksamen Grenzen je Plugin.
- Gilt in allen Modi über `PluginRuntime`: `process`, `stream`, `watch`, `serve`.
- Dokumentation: README, `docs/deployment-guide.md`, `docs/pipeline_host_supervisor.md`, `docs/arc42.md`, CHANGELOG. In `config.yaml` und `config.mac-studio.yaml` steht ein auskommentiertes Beispiel.

## Nicht-Ziele

- Keine globale Obergrenze. Ein Plugin-Wert darf über dem globalen Wert liegen.
- Keine weiteren Grenzen, etwa für Tabellen, Instanzen, Fuel oder host-seitige Puffer.
- Keine Prüfung der globalen `resource_limits` über das heutige Verhalten hinaus.
- Keine Grenzen aus den `settings` des Plugins. Grenzen sind Host-Konfiguration und für den Gast unsichtbar.
- `config.docker.yaml` wird nicht angepasst.

## Akzeptanzkriterien

- **AC-1** Ein Plugin mit `resource_limits.max_memory_bytes` unter dem globalen Wert scheitert an seiner eigenen Grenze. Die Fehlermeldung nennt den Wert des Plugins. Ohne Eintrag je Plugin gelingt dieselbe Speichermenge unter den globalen Grenzen.
- **AC-2** Ein Plugin mit `resource_limits.timeout_ms` über dem globalen Wert läuft länger als die globale Grenze, ohne abgebrochen zu werden. Es scheitert erst an seiner eigenen Grenze, und die Fehlermeldung nennt diesen Wert.
- **AC-3** Setzt ein Plugin nur eines der beiden Felder, erbt es das andere von den globalen `resource_limits`.
- **AC-4** Ist ein Wert 0, keine ganze Zahl oder der Schlüssel unbekannt, startet der Host. Er warnt mit Plugin, Schlüssel und Wert, und für das betroffene Feld gilt der globale Wert.
- **AC-5** `plugins.<name>.resource_limits` wird aus YAML (Block-Form unter dem Plugin) und JSON gelesen. Ohne den Abschnitt sind die wirksamen Grenzen gleich den globalen.
- **AC-6** Die wirksamen Grenzen gelten auch beim Laden: Die Abfrage von `name`/`version` läuft mit den Grenzen des Plugins.
