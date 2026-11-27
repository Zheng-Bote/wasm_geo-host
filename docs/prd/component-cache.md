---
feature: component-cache
status: accepted
archive-after:
touches:
  - crates/host/src/**
  - crates/host/tests/**
  - crates/host/Cargo.toml
  - Cargo.lock
  - config.yaml
  - config.mac-studio.yaml
  - README.md
  - docs/deployment-guide.md
  - docs/arc42.md
  - CHANGELOG.md
---

# PRD — Cache für kompilierte Wasm-Komponenten (`component-cache`)

## Problem

Der Host kompiliert bei jedem Start alle aktivierten Komponenten neu (ADR 0003). Gemessen am 2026-11-27 (Release-Build, macOS, 8 Plugins) dauert ein Aufruf von `flagme-host process Examples/sample.jpg` etwa 0,8 s. Davon entfallen etwa 0,6 s auf das Kompilieren, auf die Plugins selbst nur etwa 0,1 bis 0,2 s.

Wer `process` oder `stream` für viele Dateien einzeln aufruft, zahlt diese Zeit bei jeder Datei, obwohl sich die Komponenten nicht geändert haben.

## Ziel

Ab dem zweiten Start mit unveränderten Komponenten lädt der Host den kompilierten Code aus einem Cache, statt neu zu kompilieren. So legt es [ADR 0004](../adr/0004-component-cache.md) fest.

- Der Cache ist standardmäßig an.
- Der Host nutzt ihn nur in einem Ordner, den allein der eigene Benutzer beschreiben kann.
- Ist das nicht gegeben, kompiliert der Host wie bisher und warnt.
- Das Ergebnis der Pipeline ist mit und ohne Cache dasselbe.

## Nutzer

- **Betreiber und Skripte**, die `process` oder `stream` je Datei aufrufen: Sie bekommen kürzere Laufzeiten.
- **Betreiber von `watch` und `serve`**: Sie bekommen einen schnelleren Start.
- **Plugin-Entwickler**: Eine neu gebaute Komponente wird ohne Zutun neu kompiliert; ein veralteter Cache-Eintrag wird nie geladen.

## Umfang

- Eingebauter Cache von wasmtime (`Config::cache`) in `ComponentEngine`. Kein `unsafe`, kein eigenes Serialisieren.
- Neue Konfiguration in `config.yaml` (YAML- und JSON-Pfad von `AppConfig`):
  - `component_cache` (bool, Standard `true`);
  - `component_cache_dir` (Standard `cache/components`, relativ zum Arbeitsverzeichnis wie `plugin_dir`).
- Prüfung des Ordners vor dem Einschalten:
  - Ein fehlender Ordner wird mit Rechten `0700` angelegt.
  - Ein vorhandener Ordner muss dem eigenen Benutzer gehören, darf weder für Gruppe noch andere beschreibbar sein und darf kein symbolischer Link sein.
- Rückfall ohne Cache mit Warnung (Pfad und Grund), wenn die Prüfung oder die Cache-Konfiguration scheitert.
- `debug`-Meldung beim Start: Cache aktiv oder nicht, Ordner.
- Gilt für alle Modi, die `PluginRuntime` nutzen: `process`, `stream`, `watch`, `serve`.
- Dokumentation: README (Konfiguration), `docs/deployment-guide.md`, `docs/arc42.md` (Querschnitt/Risiken), CHANGELOG, Beispielkonfigurationen.

## Nicht-Ziele

- Kein Vorkompilieren beim Build (`.cwasm`), kein `Component::deserialize`.
- Keine Signatur oder Prüfsumme der Cache-Dateien über das hinaus, was wasmtime selbst prüft.
- Keine eigenen Grenzen für Größe und Anzahl der Einträge; es gelten die Standardwerte von wasmtime.
- Kein eigener CLI-Befehl zum Leeren des Caches. Löschen des Ordners genügt.
- Keine Rechteprüfung außerhalb von Unix. Dort läuft der Host ohne Cache und warnt.
- `Dockerfile` und `config.docker.yaml` werden nicht angepasst.

## Akzeptanzkriterien

- **AC-1** Ist der Cache an und der Ordner sicher, kompiliert der erste Start jede Komponente und legt Einträge im Ordner an. Ein zweiter Start mit denselben Komponenten lädt jede Komponente aus dem Cache: Die Zahl der Cache-Treffer entspricht der Zahl der geladenen Komponenten, und es gibt keinen Fehlschlag.
- **AC-2** Ändert sich die Datei einer Komponente, lädt der nächste Start sie nicht aus dem alten Eintrag, sondern kompiliert sie neu. Die geänderte Komponente meldet ihre neue Identität (`name`/`version`).
- **AC-3** Ein Dokument durchläuft die Pipeline mit und ohne Cache mit demselben Ergebnis.
- **AC-4** Mit `component_cache: false` legt der Host keinen Cache-Ordner an und schreibt keine Cache-Dateien.
- **AC-5** Fehlt der Ordner, legt der Host ihn an. Auf Unix hat er danach die Rechte `0700`.
- **AC-6** Ist der Ordner für Gruppe oder andere beschreibbar, ein symbolischer Link oder lässt er sich nicht anlegen, startet der Host trotzdem. Er nutzt keinen Cache, schreibt nichts in den Ordner und protokolliert eine Warnung mit Pfad und Grund.
- **AC-7** `component_cache` und `component_cache_dir` werden aus YAML und JSON gelesen. Fehlen sie, gelten `true` und `cache/components`.
- **AC-8** Der Host-Code enthält kein `unsafe` und ruft weder `Component::deserialize` noch `Component::deserialize_file` auf.
