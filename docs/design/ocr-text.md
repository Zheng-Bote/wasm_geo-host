---
feature: ocr-text
status: draft
archive-after:
touches:
  - "crates/models/**"
  - "plugins/ocr-text/**"
  - "crates/host/**"
  - "crates/host/migrations/**"
  - "Cargo.toml"
---

# Tech Spec — ocr-text (`ocr-text`)

Setzt das PRD `docs/prd/ocr-text.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern.

## Architektur & Komponenten

Das Plugin fügt sich als Analyse-Komponente in die Pipeline ein und benötigt die `filesystem`-Capability zum Lesen von Bilddaten:

```text
+-------------------------------------------------------------+
|                     flagme-host (Host)                      |
|  - Pipeline Orchestrator                                    |
|  - FTS5 Full-Text Search Engine                             |
+------------------------------+------------------------------+
                               |
                               v
+-------------------------------------------------------------+
|                   flagme-plugin-ocr-text                    |
|  - Text Detection (CRAFT / DBNet Bounding Boxes)            |
|  - Text Recognition (CRNN Transkription)                    |
|  - Liefert OcrInfo mit TextRegionen & BoundingBoxes         |
+-------------------------------------------------------------+
                               |
                               v
+-------------------------------------------------------------+
|                   SQLite (media.db)                         |
|  - Tabelle: ocr_texts (combined_text, primary_language)     |
|  - Tabelle: ocr_regions (text, confidence, bbox)            |
|  - Virtuelle FTS5-Tabelle: photos_fts (ocr_text Indexierung)|
+-------------------------------------------------------------+
```

### 1. Datenmodell (AC-1)
In `crates/models/src/lib.rs`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OcrInfo {
    pub combined_text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<TextRegion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextRegion {
    pub text: String,
    pub confidence: f32,
    pub bbox: BoundingBox,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
```
Erfüllt **AC-1**: Typisierte Modelle für OCR-Informationen.

### 2. Plugin & OCR-Pipeline (AC-2, AC-3)
In `plugins/ocr-text/src/lib.rs`:
- Trait `OcrEngine: Send + Sync` mit Methode `extract_text(&self, path: &str) -> OcrInfo`.
- Keine eingebaute Engine: `OcrEngine::recognize(image: &[u8])` erhält nur die Bildbytes; ohne Engine bleibt `doc.ocr` leer ([ADR 0002](../adr/0002-plugins-without-engine-no-output.md)).
- Erzeugt Bounding-Boxen und Konfidenzwerte für jede erkannte Zeile.
Erfüllt **AC-2** (WASI Kompilierung) und **AC-3** (Erkennungs-Pipeline).

### 3. SQLite Persistenz & FTS5-Synchronisation (AC-4)
In `crates/host/migrations/20261125000000_init.sql`:
```sql
CREATE TABLE IF NOT EXISTS ocr_texts (
    photo_id TEXT PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
    combined_text TEXT NOT NULL,
    primary_language TEXT
);

CREATE TABLE IF NOT EXISTS ocr_regions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    photo_id TEXT NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    text TEXT NOT NULL,
    confidence REAL NOT NULL,
    bbox_x REAL NOT NULL,
    bbox_y REAL NOT NULL,
    bbox_w REAL NOT NULL,
    bbox_h REAL NOT NULL
);
```
In `crates/host/src/storage.rs`:
- Speichern in `ocr_texts` und `ocr_regions`.
- FTS5-Indexierung: `photos_fts` enthält Spalte `ocr_text` (oder synchronisiert `combined_text` in das FTS5-Dokument).
Erfüllt **AC-4**: Relationale Speicherung und FTS5-Suchindexierung.

### 4. Integration Verification (AC-5)
In `crates/host/tests/ocr_test.rs`:
- Verifikation von Texterkennung, Speicherung und Volltextsuche über `Storage::search`.
Erfüllt **AC-5**: Vollständige Testabsicherung.
