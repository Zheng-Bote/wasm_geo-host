---
feature: privacy-redact
status: draft
archive-after:
---

# Plan — privacy-redact (`privacy-redact`)

Reihenfolge der Umsetzung. Jede Task nennt das Akzeptanzkriterium, auf das sie einzahlt.

## Tasks

- [x] AC-1 `PrivacyInfo` in `crates/models/src/lib.rs` definieren und in `UnifiedDocument` einbinden.
- [x] AC-2 Crate `plugins/privacy-redact` mit `Cargo.toml` (`cdylib` und `[[bin]]`) anlegen und im Root-Workspace registrieren.
- [x] AC-3 Bildverarbeitungs- und Obfuskationslogik (Pixelate, Blackout) implementieren.
- [x] AC-4 GPS-Fuzzing-Logik für Standort-Koordinaten umsetzen.
- [x] AC-5 Bereinigung von Kamera- und Objektiv-Seriennummern aus Metadaten umsetzen.
- [x] AC-6 SQLite-Schema und `Storage` um Anonymisierungs-Felder erweitern und Integrationstests in `crates/host/tests/privacy_test.rs` verifizieren.
