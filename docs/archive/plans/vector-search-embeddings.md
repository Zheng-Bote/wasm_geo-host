---
feature: vector-search-embeddings
status: archived
archive-after:
release: 0.2.0
---

# Plan — vector-search-embeddings (`vector-search-embeddings`)

Reihenfolge der Umsetzung. Jede Task nennt das Akzeptanzkriterium, auf das sie einzahlt.

## Tasks

- [x] AC-1 `AiInfo` in `crates/models/src/lib.rs` um `embedding: Option<Vec<f32>>` erweitern.
- [x] AC-2 `VisionModel` und `AiTaggingPlugin` in `plugins/ai-tagging/src/lib.rs` erweitern, um 512-D Einbettungsvektoren zu berechnen und zu exportieren.
- [x] AC-3 SQLite-Migration um Tabelle `photo_embeddings` erweitern und BLOB-Speicherung / Laden in `Storage` implementieren.
- [x] AC-3 Kosinus-Ähnlichkeits-Suchmethode `Storage::find_similar` in `crates/host/src/storage.rs` implementieren.
- [x] AC-4 CLI-Subcommand `flagme-host similar <PHOTO_ID>` in `crates/host/src/main.rs` integrieren.
- [x] AC-4 Integrationstests in `crates/host/tests/vector_test.rs` mit Kriterien-Referenzen (`// vector-search-embeddings AC-<n>: ...`) erstellen und validieren.
