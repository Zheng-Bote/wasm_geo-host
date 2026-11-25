---
feature: ocr-text
status: draft
archive-after:
---

# Plan — ocr-text (`ocr-text`)

Reihenfolge der Umsetzung. Jede Task nennt das Akzeptanzkriterium, auf das sie einzahlt.

## Tasks

- [x] AC-1 `OcrInfo`, `TextRegion` und `BoundingBox` in `crates/models/src/lib.rs` definieren und in `UnifiedDocument` einbinden.
- [x] AC-2 Crate `plugins/ocr-text` mit `Cargo.toml` (`cdylib` und `[[bin]]`) anlegen und im Root-Workspace registrieren.
- [x] AC-3 Textdetektions- und Erkennungs-Pipeline in `plugins/ocr-text/src/lib.rs` implementieren.
- [x] AC-4 SQLite-Schema-Migration um `ocr_texts` und `ocr_regions` erweitern, FTS5 um `ocr_text` ergänzen und in `Storage::save_document` synchronisieren.
- [x] AC-5 Integrationstests in `crates/host/tests/ocr_test.rs` mit Kriterien-Referenzen (`// ocr-text AC-<n>: ...`) erstellen und validieren.
