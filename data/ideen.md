# Roadmap & Vorschläge für Verbesserungen und neue Plugins

Dieses Dokument sammelt architektonische Verbesserungsvorschläge und neue Plugin-Konzepte für die Plattform **WASM Geo Host (`flagme-host`)**.

---

## 1. Neue Plugin-Konzepte

| Plugin-Name | Kernfunktion & Nutzen | Technische Umsetzung | Benötigte Capabilities |
|---|---|---|---|
| **`color-palette`** | Extrahiert dominante Farben und Stimmungen (HEX-Codes, Farbanteile). | K-Means-Clustering auf Bildpixeln zur Ermittlung von 5 dominanten Farben (z. B. `#1A365D`, `#E2E8F0`). Ermöglicht Suche und Sortierung nach Farbstimmung und Bildharmonie. | `filesystem` |
| **`blur-quality`** | Erkennt Bildqualität, Schärfe, Verwacklungen und Belichtungsfehler. | Berechnung des Laplace-Varianzwertes (Schärfe-Score) und Histogramm-Dynamik; markiert Fotos automatisch als *Highlight*, *Akzeptabel* oder *Ausschuss*. | `filesystem` |
| **`face-detection`** | Erkennt Gesichter, markiert Bounding Boxes und gruppiert Personen. | Leichtgewichtiges WASM-fähiges Vision-Modell (z. B. UltraFace / SCRFD) zur Generierung von Gesichts-Embeddings; ermöglicht automatische Alben nach Personen. | `filesystem` |
| **`ocr-text`** | Volltext-Erkennung für abgebildete Straßenschilder, Plakate, Notizen. | WASM-Build von Tesseract oder leichtes CRNN-Modell; macht Bilder über den abgebildeten Text volltext-durchsuchbar. | `filesystem` |
| **`privacy-redact`** | Automatische Anonymisierung für Web-Publishing und DSGVO. | Erkennt Gesichter und Autokennzeichen und schwärzt oder verpixelt diese in den generierten Thumbnails oder Exporten. | `filesystem` |
| **`weather-enrich`** | Historisches Wetter zum genauen Aufnahmezeitpunkt. | Verknüpft `taken_at` und GPS-Koordinaten über Open-Meteo Historical API (z. B. "Sonnig, 24°C, goldene Stunde"); speichert Wetter-Tags im Dokument. | `http`, `kv` |

---

## 2. Plattform- & Architektur-Verbesserungen

### 2.1 Full-Text Search (FTS5) in SQLite
- **Ziel**: Sub-Millisekunden Volltextsuche über alle Bildattribute.
- **Konzept**: Eine virtuelle FTS5-Tabelle `photos_fts` in `media.db`, die Tags aus `ai_tags`, `metadata.raw_tags`, `locations` (Stadt, Land, lokal) und `file.filename` indexiert.
- **Beispiel-Query**:
  ```sql
  SELECT p.* FROM photos p 
  JOIN photos_fts fts ON p.id = fts.photo_id 
  WHERE photos_fts MATCH 'Berlin AND mountain AND Sony';
  ```

### 2.2 Embedding-basierte semantische Vektorsuche (Vector Search)
- **Ziel**: Semantische Bildsuche anhand natürlicher Sprache ("Ein rotes Auto vor einer alten Scheune"), ohne dass exakte Tags vorhanden sein müssen.
- **Konzept**: 
  - Das `ai-tagging`-Plugin extrahiert den 512-dimensionalen CLIP-Feature-Vektor.
  - Speicherung als BLOB in SQLite mit Cosinus-Distanzberechnung (oder via `sqlite-vec`-Extension).

### 2.3 Verzeichnis- & Batch-Watcher (`flagme-host watch`)
- **Ziel**: Automatischer Import-Dienst für Hintergrundverarbeitung.
- **Konzept**:
  ```bash
  flagme-host watch /media/import/ --config config.yaml --target-dir /media/library/
  ```
  - Nutzt das `notify`-Crate zur Überwachung von Ingest-Ordnern.
  - Verarbeitet neue Medien parallel durch die Pipeline.
  - Automatische Einsortierung in Datums- & Ortsordner (`/data/2026/11/Deutschland/Berlin/...`).

### 2.4 Web-UI & REST/GraphQL-API (`flagme-host serve`)
- **Ziel**: Sofortige visuelle Darstellung der verarbeiteten Fotos ohne externe Tools.
- **Konzept**: Ein leichtgewichtiger, eingebetteter Axum-Webserver mit folgenden Endpunkten:
  - `GET /api/photos`: Liste aller Fotos mit Paging und Filtern.
  - `GET /api/photos/{id}/thumbnail/{size}`: Auslieferung der generierten Thumbnails.
  - `GET /api/map/markers`: GeoJSON-Export aller Fotos für interaktive Kartenansichten (Leaflet / MapLibre).
  - Minimales Web-Frontend mit Galerieansicht und Weltkarte.

### 2.5 WASI-NN (Neural Network) Host-Unterstützung
- **Ziel**: Direkte Hardware-Beschleunigung (GPU/NPU) für KI-Plugins.
- **Konzept**:
  - Bereitstellung der offiziellen `wasi-nn`-Imports im Wasmtime-Host.
  - Plugins können große ONNX-, OpenVINO- oder PyTorch-Modelle direkt an die Host-GPU delegieren, während die Speichersicherheit und Isolierung von WASI erhalten bleibt.

---

## 3. Priorisierungsempfehlung

1. **Kurzfristig (Quick Wins)**:
   - `color-palette` Plugin (hoher visueller Mehrwert, reine CPU-Berechnung).
   - `photos_fts` (SQLite Full-Text Search für sofortige Suche).
2. **Mittelfristig**:
   - `flagme-host watch` (Batch- und Verzeichnisüberwachung für echten Dauerbetrieb).
   - `flagme-host serve` (Minimales Karten-Dashboard).
3. **Langfristig**:
   - WASI-NN Integration und CLIP-Vektorsuche.
