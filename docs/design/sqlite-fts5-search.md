---
feature: sqlite-fts5-search
status: draft
archive-after:
touches:
  - "crates/host/**"
  - "crates/host/migrations/**"
---

# Tech Spec — sqlite-fts5-search (`sqlite-fts5-search`)

Setzt das PRD `docs/prd/sqlite-fts5-search.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern.

## Architektur & Komponenten

Die Volltextsuch-Erweiterung integriert sich direkt in die Host-Schicht (`crates/host`) und nutzt die in SQLite native FTS5-Erweiterung:

```text
+-------------------------------------------------------------+
|                     flagme-host (Host)                      |
|  - CLI Subcommand: search <QUERY>                          |
|  - Storage Engine (crates/host/src/storage.rs)              |
+------------------------------+------------------------------+
                               |
                               v
+-------------------------------------------------------------+
|                   SQLite (media.db)                         |
|  - Tabellen: photos, metadata, locations, ai_tags, ...      |
|  - Virtuelle FTS5-Tabelle: photos_fts                       |
|    (photo_id, filename, camera_make, camera_model,         |
|     country, local_country, city, local_city, ai_tags,      |
|     description, raw_tags)                                  |
+-------------------------------------------------------------+
```

### 1. Schema Migration (AC-1)
In `crates/host/migrations/20261125000000_init.sql` definiert:
```sql
CREATE VIRTUAL TABLE IF NOT EXISTS photos_fts USING fts5(
    photo_id UNINDEXED,
    filename,
    camera_make,
    camera_model,
    country,
    local_country,
    city,
    local_city,
    ai_tags,
    description,
    raw_tags
);
```
Erfüllt **AC-1**: Bereitstellung der FTS5-Virtual-Table mit unindizierter Photo-ID und indizierten Textspalten.

### 2. Storage Synchronisation (AC-2)
In `crates/host/src/storage.rs` beim Speichern via `Storage::save_document`:
- Extraktion und Aggregation aller Text-Tokens aus:
  - `doc.file.filename`
  - `doc.metadata.camera_make`, `camera_model`, `raw_tags`
  - `doc.location.country`, `local_country`, `city`, `local_city`
  - `doc.ai.tags` (zusammengeführt als Leerzeichen-separierte Liste)
- Ausführung eines `DELETE FROM photos_fts WHERE photo_id = ...` gefolgt von `INSERT INTO photos_fts (...)`.
- Definition der Suchfunktion:
  ```rust
  pub struct SearchResult {
      pub photo_id: String,
      pub filename: String,
      pub rank: f64,
  }
  impl Storage {
      pub fn search(&self, query: &str) -> anyhow::Result<Vec<SearchResult>>;
  }
  ```
Erfüllt **AC-2**: Automatische Indexierung und Bereitstellung der Suchschnittstelle.

### 3. CLI Search Subcommand (AC-3)
In `crates/host/src/main.rs`:
- CLI-Subcommand `flagme-host search <QUERY> [--config <CONFIG>]`
- Führt `storage.search(&query)` aus und formatiert die BM25-gerankten Suchtreffer.
Erfüllt **AC-3**: Relevanz-gerankte Terminal-Ausgabe.

### 4. Integration Verification (AC-4)
In `crates/host/tests/fts_test.rs`:
- Überprüfung von Multi-Token-Suchanfragen (z. B. Kombination aus AI-Tag, Kameramodell und Ort).
Erfüllt **AC-4**: Nachweis der Suchgenauigkeit über kombinierte Attribute.
