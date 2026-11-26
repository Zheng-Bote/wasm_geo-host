---
feature: capability-only-io
status: draft
archive-after:
---

# Plan — Datei- und Prozesszugriffe nur über Capabilities (`capability-only-io`)

Reihenfolge der Umsetzung. Jede Task nennt das Akzeptanzkriterium, auf das sie einzahlt.

## Tasks

- [x] AC-3 `HostCapabilities::fs_modified` (SDK-Default, `CapabilityManager`, `NoopCapabilities`) und WIT `fs-modified`.
- [x] AC-4 `timeline`: Änderungszeit über `fs_modified` statt `std::fs::metadata`.
- [x] AC-1 `metadata`-CLI: Dateigröße über `fs_read`; Quelltext-Prüfung als Test.
- [x] AC-2 Test ohne `filesystem`-Freigabe für alle dateibasierten Plugins; READMEs und CHANGELOG.
