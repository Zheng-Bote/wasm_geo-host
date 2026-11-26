---
feature: sqlite-fts5-search
status: archived
archive-after:
release: 0.2.0
---

# Plan — sqlite-fts5-search (`sqlite-fts5-search`)

Reihenfolge der Umsetzung. Jede Task nennt das Akzeptanzkriterium, auf das sie einzahlt.

## Tasks

- [x] AC-1 Schema-Migration um `photos_fts` (SQLite FTS5 virtual table) erweitern.
- [x] AC-2 `Storage::save_document` erweitern, um FTS5-Indexierung von Metadaten, Location, AI-Tags und Filename durchzuführen.
- [x] AC-2 `Storage::search` Methode implementieren, die BM25-gerankte Ergebnisse aus SQLite abfragt.
- [x] AC-3 CLI-Subcommand `flagme-host search <QUERY>` in `crates/host/src/main.rs` integrieren.
- [x] AC-4 Integrationstests in `crates/host/tests/fts_test.rs` mit Kriterien-Referenzen (`// sqlite-fts5-search AC-<n>: ...`) erstellen und verifizieren.
