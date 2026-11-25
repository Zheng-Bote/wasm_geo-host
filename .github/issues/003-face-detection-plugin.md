# Feature Spec: Face Detection & Clustering Plugin (`flagme-plugin-face-detection`)

- **Feature Name**: `face-detection`
- **Component**: `plugins/face-detection`, `crates/models`, `crates/host`
- **Issue Type**: New Feature / Plugin Proposal

---

## 1. Problem Statement & Motivation
Media libraries frequently require people-based organization (e.g. "Photos with Alice", "Family Portraits", "Group Shots"). Currently, the WASM Geo Host pipeline extracts technical EXIF metadata, spatial locations, timestamps, and general scene tags, but lacks facial localization and face-clustering capabilities.

A dedicated `face-detection` plugin solves this by:
1. Detecting faces (Bounding Boxes, confidence, facial landmarks).
2. Generating face embeddings (128-D / 512-D vectors) for clustering and person identification.
3. Supporting configurable model tiers to run seamlessly on resource-constrained Docker/Podman containers (lightweight CPU) as well as high-performance workstations like Mac Studio (GPU/Metal).

---

## 2. Technical Architecture & Execution Model

### 2.1 Two-Stage Pipeline
```
Input Image 
  ──> [Stage 1: Face Detection] (UltraFace / SCRFD)
        ──> Bounding Boxes (x, y, w, h, score, 5 landmarks)
  ──> [Stage 2: Feature Extraction & Embedding] (MobileFaceNet / ArcFace)
        ──> 128-D / 512-D Embedding Vector
```

### 2.2 Configurable Model Tiers (`config.yaml`)
The plugin reads its `settings` dictionary to adjust model weight and inference backend dynamically:

```yaml
plugins:
  face-detection:
    enabled: true
    capabilities:
      - filesystem
    settings:
      model_tier: "lightweight"         # "lightweight" | "standard" | "heavy"
      min_confidence: "0.80"
      extract_embeddings: "true"
      max_faces: "20"
      model_path: ""                    # Optional path to external ONNX weights
```

- **Tier 1 (`lightweight`) - Default for Docker / Edge**:
  - Detection: UltraFace (320x240, ~1.2 MB).
  - Recognition: MobileFaceNet (128-D, ~4 MB).
  - Backend: Pure WASM-CPU, memory usage < 120 MB.
- **Tier 2 (`heavy`) - For Workstations / Mac Studio**:
  - Detection: SCRFD-10G (higher recall for small/profile faces).
  - Recognition: ArcFace ResNet-100 (512-D high-precision embeddings).
  - Backend: External weights / WASI-NN delegation (CoreML / Metal / CUDA).

---

## 3. Data Model Specification (`wasm-geo-models`)

Add `FacesInfo` to `UnifiedDocument`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FacesInfo {
    pub count: usize,
    pub faces: Vec<DetectedFace>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DetectedFace {
    pub face_id: String,
    pub bounding_box: BoundingBox,     // Normalized coordinates (0.0 - 1.0)
    pub confidence: f32,
    pub landmarks: Option<FaceLandmarks>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
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

---

## 4. SQLite Schema & Persistence (`media.db`)

Add relational tables for face instances and recognized persons:

```sql
CREATE TABLE IF NOT EXISTS faces (
    id TEXT PRIMARY KEY,
    photo_id TEXT NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    bbox_x REAL NOT NULL,
    bbox_y REAL NOT NULL,
    bbox_w REAL NOT NULL,
    bbox_h REAL NOT NULL,
    confidence REAL NOT NULL,
    embedding BLOB,             -- Packed 128*4 or 512*4 Float32 bytes
    person_id TEXT REFERENCES persons(id)
);

CREATE TABLE IF NOT EXISTS persons (
    id TEXT PRIMARY KEY,
    name TEXT,
    representative_face_id TEXT REFERENCES faces(id),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_faces_photo ON faces(photo_id);
CREATE INDEX IF NOT EXISTS idx_faces_person ON faces(person_id);
```

---

## 5. Acceptance Criteria
- [ ] **AC-1**: `wasm-geo-models` introduces `FacesInfo`, `DetectedFace`, `BoundingBox` and `FaceLandmarks` with complete serde compatibility.
- [ ] **AC-2**: `flagme-plugin-face-detection` compiles to both `cdylib` and standalone `[[bin]]` CLI under `wasm32-wasip2`.
- [ ] **AC-3**: Configurable `model_tier` setting selects between lightweight embedded weights and heavy external weights.
- [ ] **AC-4**: Relational tables `faces` and `persons` are created in `media.db` and populated via `Storage::save_document`.
- [ ] **AC-5**: Integration tests verify face bounding box detection on sample test images.
