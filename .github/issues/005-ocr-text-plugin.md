# Feature Spec: OCR Text Recognition Plugin (`flagme-plugin-ocr-text`)

- **Feature Name**: `ocr-text`
- **Component**: `plugins/ocr-text`, `crates/models`, `crates/host`
- **Issue Type**: New Feature / Plugin Proposal

---

## 1. Problem Statement & Motivation
Photographs in media libraries frequently contain rich textual context that is not captured in standard EXIF/IPTC metadata:
- **Street signs & location markers**: Street names, metro stations, venue signs.
- **Documents & receipts**: Whiteboards, book covers, tickets, business cards.
- **Products & brand names**: Billboards, storefronts, car models.

Currently, searching for photos containing specific textual terms is impossible unless manually tagged. A dedicated `ocr-text` plugin extracts visible text, associates it with bounding box regions, and integrates with the host database to enable instant full-text search.

---

## 2. Technical Architecture & Inference Pipeline

The plugin executes as a two-stage optical character recognition pipeline under WebAssembly:

```
Input Image 
  ──> [Stage 1: Text Detection (DBNet / CRAFT)]
        ──> Text Line Bounding Polygons / Boxes
  ──> [Stage 2: Text Recognition (CRNN / SVTR)]
        ──> Transcribed UTF-8 String + Confidence Score
```

### Supported Runtime Tiers
1. **Lightweight (Default for Docker / CPU)**:
   - Quantized INT8 / FP16 CRNN text model (~3.5 MB).
   - Fast CPU inference (< 80 ms per standard image).
   - Multi-language dictionary support (Latin, Cyrillic, CJK basics).
2. **Heavy (Workstations / Mac Studio)**:
   - External PaddleOCR / Tesseract-WASM model path via `config.yaml`.
   - Higher accuracy on rotated text, artistic fonts, and complex documents.

---

## 3. Configuration Specification (`config.yaml`)

```yaml
plugins:
  ocr-text:
    enabled: true
    capabilities:
      - filesystem
    settings:
      model_tier: "lightweight"         # "lightweight" | "standard" | "heavy"
      min_confidence: "0.75"            # Filters low-confidence noise
      max_text_regions: "50"            # Maximum text segments extracted
      target_languages: "en,de,fr,es"   # Primary character sets
      enable_fulltext_index: "true"     # Feeds SQLite FTS5 table
```

---

## 4. Data Model Output (`wasm-geo-models`)

Add `OcrInfo` and `TextRegion` to `UnifiedDocument`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OcrInfo {
    pub combined_text: String,          // All recognized text concatenated for FTS
    pub text_regions: Vec<TextRegion>,
    pub primary_language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextRegion {
    pub text: String,
    pub confidence: f32,                // e.g. 0.94
    pub bounding_box: BoundingBox,      // Normalized coordinates (0.0 - 1.0)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
```

---

## 5. SQLite Persistence & Full-Text Search (FTS5)

Add relational persistence and integration with SQLite FTS5:

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

-- Virtual Full-Text Search Table
CREATE VIRTUAL TABLE IF NOT EXISTS photos_fts USING fts5(
    photo_id UNINDEXED,
    ocr_text,
    content='ocr_texts',
    content_rowid='rowid'
);
```

### Example Search Query
```sql
SELECT p.* FROM photos p
JOIN photos_fts fts ON p.id = fts.photo_id
WHERE photos_fts MATCH 'Brandenburger Tor';
```

---

## 6. Acceptance Criteria
- [ ] **AC-1**: `wasm-geo-models` introduces `OcrInfo` and `TextRegion` with complete serde serialization.
- [ ] **AC-2**: `flagme-plugin-ocr-text` compiles to both `cdylib` and standalone `[[bin]]` under `wasm32-wasip2`.
- [ ] **AC-3**: Text detector and recognizer stages extract text lines with bounding coordinates and confidence scores.
- [ ] **AC-4**: SQLite storage creates and updates `ocr_texts`, `ocr_regions`, and updates the `photos_fts` index.
- [ ] **AC-5**: Integration tests verify text transcription from test image samples containing signage.
