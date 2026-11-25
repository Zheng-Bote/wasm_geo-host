# Face Detection & Clustering Plugin (`flagme-plugin-face-detection`)

The **Face Detection & Clustering Plugin** detects human faces, localizes 5 facial landmarks, and generates dense facial embeddings for face-clustering and person identification.

---

## 1. Features
- **Configurable Model Tiers**:
  - `Lightweight` (Default for Docker / CPU): UltraFace-Slim-INT8 + MobileFaceNet (128-D embeddings).
  - `Heavy` (Workstations / Mac Studio): SCRFD + ArcFace-ResNet100 (512-D high-precision embeddings).
- **Landmark Localization**: Detects 5 primary facial landmarks (left eye, right eye, nose, mouth left, mouth right).
- **Relational Persistence**: Stores face bounding boxes and vectors into `faces` and clusters into `persons`.
- **WASI 0.2 Component**: Targets `wasm32-wasip2`.

---

## 2. Capabilities Required
- `filesystem`: Loading image data for face inference.

---

## 3. Data Output (`UnifiedDocument.faces`)
```json
{
  "faces": [
    {
      "id": "face-1",
      "bbox": { "x": 0.30, "y": 0.20, "width": 0.40, "height": 0.45 },
      "confidence": 0.97,
      "landmarks": {
        "left_eye": [0.42, 0.35],
        "right_eye": [0.58, 0.35],
        "nose": [0.50, 0.45],
        "mouth_left": [0.44, 0.55],
        "mouth_right": [0.56, 0.55]
      },
      "embedding": [0.1, 0.1, ...],
      "person_id": null
    }
  ],
  "detection_model": "UltraFace-Slim-INT8"
}
```
