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
- [x] AC-3 Bildverarbeitungs- und Obfuskationslogik (Pixelate, Blackout) implementieren (pixelgenau überarbeitet in AC-7 bis AC-11, Issue #22).
- [x] AC-4 GPS-Fuzzing-Logik für Standort-Koordinaten umsetzen.
- [x] AC-5 Bereinigung von Kamera- und Objektiv-Seriennummern aus Metadaten umsetzen.
- [x] AC-6 SQLite-Schema und `Storage` um Anonymisierungs-Felder erweitern und Integrationstests in `crates/host/tests/privacy_test.rs` verifizieren.
- [x] AC-7 Quelle dekodieren (`image`, nur JPEG), `bbox` in Pixelrechteck umrechnen und nur dieses bearbeiten (`redact_region`).
- [x] AC-8 `blur` als Gauß-Weichzeichnung mit `blur_sigma` umsetzen.
- [x] AC-9 `pixelate` als Mosaik mit `block_size`-Pixel-Blöcken (Mittelwert je Block) umsetzen.
- [x] AC-10 Ergebnis als JPEG (Qualität 90) neu kodieren und ausschließlich über `fs_write` schreiben (std::fs-Fallback entfällt, Verfassung Security 2).
- [x] AC-11 Fehlerpfade (Lesen, Dekodieren, keine gültige Box, Kodieren, Schreiben) warnen und keine Redaktionsregel melden; Tests in `crates/host/tests/privacy_test.rs` je Kriterium.
