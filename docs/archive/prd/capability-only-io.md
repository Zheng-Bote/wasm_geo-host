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

# PRD — Datei- und Prozesszugriffe nur über Capabilities (`capability-only-io`)

## Problem

Die Verfassung verlangt, dass Plugins keine Ambient Authority besitzen und jeder Zugriff auf Systemressourcen eine Capability-Freigabe braucht (_Security_ 1–2). Issue #25 listet Plugins, die direkt über `std::fs` oder externe Prozesse zugreifen. Die meisten Stellen sind mit #22, #23 und #40 entfallen. Übrig sind:

- `timeline` liest die Änderungszeit der Datei mit `std::fs::metadata`, auch ohne `filesystem`-Freigabe.
- Die Standalone-CLI von `metadata` ermittelt die Dateigröße mit `std::fs::metadata`.

Für die Änderungszeit gibt es keine Capability; `HostCapabilities` und WIT kennen nur `fs-read` und `fs-write`.

## Ziel

Plugins greifen ausschließlich über `ctx.capabilities` auf Dateien zu. Ohne `filesystem`-Freigabe liest und schreibt kein Plugin eine Datei.

## Nutzer

- Betreiber, die über `config.yaml` steuern, welches Plugin welche Ressourcen nutzen darf.

## Umfang

- Neue Capability `fs_modified` (SDK-Trait, Host-`CapabilityManager`, `NoopCapabilities`, WIT `fs-modified`).
- `timeline` nutzt `fs_modified`; `metadata`-CLI nutzt `fs_read`.
- Prüfungen: kein `std::fs`/`std::process` in `plugins/*/src`; Verhalten ohne `filesystem`-Freigabe.

## Nicht-Ziele

- Keine Änderung an `NoopCapabilities` als Dateizugriff des Standalone-Modus (sie ist die Capability-Implementierung dieses Modus).
- Keine Komponenten-Ausführung über Wasmtime (#30).

## Akzeptanzkriterien

- **AC-1** Kein Quelltext unter `plugins/*/src` verwendet `std::fs`, `std::process`, `Command::new` oder Dateizugriffe über `File`.
- **AC-2** Ohne `filesystem`-Freigabe (`capabilities: []`) liest und schreibt kein Plugin eine Datei: `metadata`, `thumbnail`, `duplicate`, `timeline` (Änderungszeit), `privacy-redact`, `ocr-text`, `ai-tagging` und `face-detection` liefern dann keine dateibasierten Ergebnisse und schreiben keine Dateien.
- **AC-3** Die Capability `fs_modified(path)` liefert mit `filesystem`-Freigabe die Änderungszeit der Datei (Unix-Sekunden), ohne Freigabe `CapabilityDenied` und für Pfade mit `..` einen Fehler; WIT deklariert `fs-modified`.
- **AC-4** `timeline` ermittelt `modified_at` und die Quelle `filesystem_modified_time` ausschließlich über `fs_modified`.
