# Feature Spec: SQLite Full-Text Search (FTS5) Engine

- **Feature Name**: `sqlite-fts5-search`
- **Component**: `crates/host`, `crates/host/migrations`
- **Issue Reference**: `data/ideen.md` Punkt 2.1
- **Issue Type**: Platform Feature / Architecture Enhancement

---

## 1. Problem Statement & Motivation
Currently, querying the `media.db` SQLite database requires separate `LIKE` or exact equality queries across distinct tables (`ai_tags`, `metadata`, `locations`, `countries`, `photos`). As media libraries grow to tens of thousands of items, searching across all descriptive attributes simultaneously becomes slow and complex.

A unified Full-Text Search (FTS5) virtual table enables sub-millisecond, multi-token searches (e.g. `Berlin AND mountain AND Sony`) with ranking and relevance scoring.

---

## 2. Technical Specification & Schema

### 2.1 SQLite FTS5 Schema Migration
Create a unified FTS5 virtual table indexing key textual fields:

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

### 2.2 Storage Synchronization
Update `Storage::save_document` in `crates/host/src/storage.rs`:
- When a document is saved or updated, insert/replace its aggregated token string into `photos_fts`.
- Concatenate tags from `doc.ai.tags` and `doc.metadata.raw_tags`.

### 2.3 Host CLI Query Interface
Extend `flagme-host` CLI with a new subcommand:
```bash
flagme-host search "Berlin mountain" --config config.yaml
```
Output: Formatted list or JSON array of matching photos ranked by relevance (`bm25(photos_fts)`).

---

## 3. Acceptance Criteria
- [ ] **AC-1**: Database migration creates `photos_fts` virtual table using SQLite FTS5 extension.
- [ ] **AC-2**: `Storage::save_document` synchronizes metadata, location, AI tags, and filenames into `photos_fts`.
- [ ] **AC-3**: CLI subcommand `flagme-host search <QUERY>` executes BM25-ranked full-text searches.
- [ ] **AC-4**: Integration tests verify that queries matching combinations of AI tags, camera make, and city return the correct photo records.
