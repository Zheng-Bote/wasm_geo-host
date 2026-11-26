---
feature: face-detection
status: archived
archive-after:
touches:
  - "crates/models/**"
  - "plugins/face-detection/**"
  - "crates/host/**"
  - "crates/host/migrations/**"
  - "Cargo.toml"
release: 0.2.0
---

# Tech Spec — face-detection (`face-detection`)

Setzt das PRD `docs/prd/face-detection.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern.

## Architektur & Komponenten

Das Plugin gliedert sich als spezialisierte Analyse-Komponente in die Host-Pipeline ein:

```text
+-------------------------------------------------------------+
|                     flagme-host (Host)                      |
|  - Capabilities: filesystem                                 |
|  - SQLite Storage Engine (faces, persons)                   |
+------------------------------+------------------------------+
                               |
                               v
+-------------------------------------------------------------+
|                 flagme-plugin-face-detection                |
|  - Stage 1: Face Detection (Bounding Box, 5 Landmarks)      |
|  - Stage 2: Face Embeddings (128-D / 512-D Float32)         |
|  - Tiers: Lightweight (CPU/Docker) & Heavy (Workstation)    |
+-------------------------------------------------------------+
                               |
                               v
+-------------------------------------------------------------+
|                   SQLite (media.db)                         |
|  - Tabelle: faces (bbox, confidence, embedding BLOB)        |
|  - Tabelle: persons (name, representative_face_id)          |
+-------------------------------------------------------------+
```

### 1. Datenmodell (AC-1)
In `crates/models/src/lib.rs`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FacesInfo {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<DetectedFace>,
    pub detection_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DetectedFace {
    pub id: String,
    pub bbox: BoundingBox,
    pub confidence: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub landmarks: Option<FaceLandmarks>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaceLandmarks {
    pub left_eye: (f32, f32),
    pub right_eye: (f32, f32),
    pub nose: (f32, f32),
    pub mouth_left: (f32, f32),
    pub mouth_right: (f32, f32),
}
```
Erfüllt **AC-1**: Typisierte Modelle für Gesichter und Merkmale.

### 2. Plugin & Modell-Tiers (AC-2, AC-3)
In `plugins/face-detection/src/lib.rs`:
- Trait `FaceDetector: Send + Sync` mit Methode `detect_faces(&self, path: &str) -> Vec<DetectedFace>`.
- Unterstützte Tiers:
  - *Entfallen durch [ADR 0002](../../adr/0002-plugins-without-engine-no-output.md):* ~~`ModelTier::Lightweight`: Kompakt, CPU-freundlich, 128-D Vektoren.~~
  - ~~`ModelTier::Heavy`: Maximale Präzision, 512-D Vektoren.~~
Erfüllt **AC-2** (WASI Kompilierung). AC-3 (Modell-Tiers) ist entfallen; der Detektor wird über `FaceDetector::detect(image: &[u8])` übergeben.

### 3. SQLite Relationale Persistenz (AC-4)
In `crates/host/migrations/20261125000000_init.sql`:
```sql
CREATE TABLE IF NOT EXISTS persons (
    id TEXT PRIMARY KEY,
    name TEXT,
    representative_face_id TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS faces (
    id TEXT PRIMARY KEY,
    photo_id TEXT NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    bbox_x REAL NOT NULL,
    bbox_y REAL NOT NULL,
    bbox_w REAL NOT NULL,
    bbox_h REAL NOT NULL,
    confidence REAL NOT NULL,
    embedding BLOB,
    person_id TEXT REFERENCES persons(id)
);

CREATE INDEX IF NOT EXISTS idx_faces_photo ON faces(photo_id);
CREATE INDEX IF NOT EXISTS idx_faces_person ON faces(person_id);
```
In `crates/host/src/storage.rs`:
- Persistenz von Gesichtern und BLOB-Embeddings in `faces`.
- Wiederherstellung in `get_document`.
Erfüllt **AC-4**: Relationale Speicherung.

### 4. Integration Verification (AC-5)
In `crates/host/tests/face_test.rs`:
- Verifikation von Bounding-Boxen, Landmarks, Modell-Tiers und Persistenz.
Erfüllt **AC-5**: Vollständige Testabdeckung.
