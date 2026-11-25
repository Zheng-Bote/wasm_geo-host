---
feature: directory-watcher
status: draft
archive-after:
---

# Plan — directory-watcher (`directory-watcher`)

Reihenfolge der Umsetzung. Jede Task nennt das Akzeptanzkriterium, auf das sie einzahlt.

## Tasks

- [x] AC-1 `notify`-Abhängigkeit in `crates/host/Cargo.toml` aufnehmen und konfigurieren.
- [x] AC-1 `crates/host/src/watcher.rs` implementieren: `DirectoryWatcher` mit rekursiver Überwachung über `notify` ohne Busy-Polling.
- [x] AC-2 Worker-Pool Architektur mit Tokio `mpsc::channel` zur nebenläufigen Abarbeitung von Ingestion-Jobs implementieren.
- [x] AC-3 `Storage::has_sha256` bzw. Duplikatsprüfung im Ingestion-Workflow integrieren, um bereits erfasste Dateien zu überspringen.
- [x] AC-4 `MediaOrganizer` implementieren: Pfad-Templating `{year}/{month}/{country}/{city}/{filename}` und Dateiablage (Copy / Move).
- [x] AC-5 CLI-Befehl `flagme-host watch <DIR> --target-dir <DIR>` in `crates/host/src/main.rs` integrieren.
- [x] AC-1, AC-2, AC-3, AC-4, AC-5 Integrationstests in `crates/host/tests/watcher_test.rs` mit Kriterien-Referenzen (`// directory-watcher AC-<n>: ...`) erstellen und verifizieren.
