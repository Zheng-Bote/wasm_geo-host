---
feature: face-detection
status: draft
archive-after:
---

# Plan — face-detection (`face-detection`)

Reihenfolge der Umsetzung. Jede Task nennt das Akzeptanzkriterium, auf das sie einzahlt.

## Tasks

- [ ] AC-1 `FacesInfo`, `DetectedFace` und `FaceLandmarks` in `crates/models/src/lib.rs` definieren und in `UnifiedDocument` einbinden.
- [ ] AC-2 Crate `plugins/face-detection` mit `Cargo.toml` (`cdylib` und `[[bin]]`) anlegen und im Root-Workspace registrieren.
- [ ] AC-3 Konfigurierbare Modell-Tiers (`lightweight` und `heavy`) in `plugins/face-detection/src/lib.rs` umsetzen.
- [ ] AC-4 SQLite-Schema-Migration um `faces` und `persons` erweitern und in `Storage::save_document` / `get_document` persistieren.
- [ ] AC-5 Integrationstests in `crates/host/tests/face_test.rs` mit Kriterien-Referenzen (`// face-detection AC-<n>: ...`) erstellen und validieren.
