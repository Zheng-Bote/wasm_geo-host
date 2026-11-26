---
feature: sqlite-fts5-search
status: archived
archive-after:
touches:
  - 'crates/host/**'
  - 'crates/host/migrations/**'
release: 0.2.0
---

# PRD — sqlite-fts5-search (`sqlite-fts5-search`)

## Problem

Bislang erfordert die Abfrage der `media.db` SQLite-Datenbank separate `LIKE`- oder Gleichheitsabfragen über mehrere Tabellen (`ai_tags`, `metadata`, `locations`, `countries`, `photos`). Bei wachsenden Medienbibliotheken mit zehntausenden Fotos wird die gleichzeitige Suche über mehrere beschreibende Merkmale (z. B. Ort, KI-Erkennung und Kameramodell) langsam, ungenau und komplex.

## Ziel

Bereitstellung einer integrierten, extrem schnellen Volltextsuche (Full-Text Search via SQLite FTS5) im Host (`flagme-host`). Die Lösung umfasst eine virtuelle FTS5-Tabelle `photos_fts`, automatische Synchronisation bei jedem `Storage::save_document` sowie einen dedizierten CLI-Suchbefehl `flagme-host search <QUERY>` mit Relevanz-Ranking (BM25).

## Nutzer

- Endanwender und Entwickler, die Fotos schnell über beliebige kombinierte Textbegriffe durchsuchen möchten.
- Host-Pipelines und nachgelagerte Tools (z. B. Web-UI), die eine einheitliche Tokensuche über Tags, Geodaten, Kameras und Dateinamen benötigen.

## Umfang

Die Umsetzung umfasst:

- SQLite-Datenbankschema-Erweiterung: Virtuelle Tabelle `photos_fts` unter Nutzung von SQLite FTS5.
- Speicher-Synchronisation in `Storage::save_document` (`crates/host/src/storage.rs`).
- Suchfunktion `Storage::search(&self, query: &str) -> anyhow::Result<Vec<SearchResult>>`.
- CLI-Subcommand `flagme-host search <QUERY> [--config <CONFIG>]` mit formatierter Terminal-Ausgabe.
- Integrationstests je Kriterium mit Kennzeichnung.

## Nicht-Ziele

- Keine Vektoreinbettungen oder k-NN / Cosine-Distanz-Suchen (Gegenstand von Issue #8).
- Kein Web-UI Frontend (Gegenstand von Issue #10).
- Keine OCR-Texterkennung (Gegenstand von Issue #5).

## Akzeptanzkriterien

- **AC-1** Database Migration: Die SQLite-Migration erstellt die virtuelle Tabelle `photos_fts` mit SQLite FTS5 (Felder: `photo_id UNINDEXED`, `filename`, `camera_make`, `camera_model`, `country`, `local_country`, `city`, `local_city`, `ai_tags`, `description`, `raw_tags`).
- **AC-2** Storage Synchronization: `Storage::save_document` aggregiert Metadaten, Orte, KI-Tags und Dateinamen und synchronisiert diese automatisch in `photos_fts`.
- **AC-3** CLI Search Subcommand: Der CLI-Befehl `flagme-host search <QUERY>` führt Volltextsuchen aus und liefert nach BM25 gerankte Ergebnisse.
- **AC-4** Multi-Token Integration Verification: Integrationstests weisen nach, dass kombinierte Suchanfragen über KI-Tags, Kameramodell und Ortsdaten (z. B. "Berlin Sony mountain") exakt die passenden Foto-Einträge zurückgeben.
