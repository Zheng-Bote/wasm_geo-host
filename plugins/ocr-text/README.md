# OCR Text Recognition Plugin (`flagme-plugin-ocr-text`)

The **OCR Text Recognition Plugin** transcribes visible text from media items (street signs, documents, brand logos, posters) into structured text regions with bounding boxes and feeds them into the host Full-Text Search (FTS5) index.

---

## 1. Features
- **Two-Stage Architecture**: Text detection (bounding coordinates) followed by text recognition (CRNN UTF-8 transcription).
- **Relational Persistence**: Stores structured bounding boxes in `ocr_regions` and aggregated text in `ocr_texts`.
- **FTS5 Integration**: Synchronizes transcribed text into the SQLite `photos_fts` virtual table for sub-millisecond keyword searches.
- **WASI 0.2 Component**: Targets `wasm32-wasip2` for isolated execution under Wasmtime.

---

## 2. Capabilities Required
- `filesystem`: Required for loading image data.

---

## 3. Data Output (`UnifiedDocument.ocr`)
```json
{
  "combined_text": "Brandenburger Tor Unter den Linden",
  "regions": [
    {
      "text": "Brandenburger Tor",
      "confidence": 0.96,
      "bbox": {
        "x": 0.15,
        "y": 0.20,
        "width": 0.70,
        "height": 0.15
      }
    }
  ],
  "primary_language": "de"
}
```
