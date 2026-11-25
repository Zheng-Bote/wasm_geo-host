# Feature Spec: Vector Search & Embedding Storage via CLIP

- **Feature Name**: `vector-search-embeddings`
- **Component**: `crates/host`, `plugins/ai-tagging`, `crates/models`
- **Issue Reference**: `data/ideen.md` Punkt 2.2
- **Issue Type**: AI / Search Architecture Enhancement

---

## 1. Problem Statement & Motivation
Traditional keyword matching and tag lookup only find photos with exact label matches. Users often search using natural language phrases ("a red vintage car in front of a barn", "foggy autumn morning in the forest") where exact tags might not exist.

By computing and storing dense 512-dimensional CLIP image embeddings, the platform enables semantic similarity search using cosine distance without requiring external vector databases.

---

## 2. Technical Specification

### 2.1 Unified Document & Model Extension
Extend `AiInfo` in `crates/models`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AiInfo {
    pub tags: Vec<AiTag>,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>, // 512-dim Float32 vector from CLIP
}
```

### 2.2 SQLite Storage
Store embeddings compactly in SQLite as binary BLOBs (512 * 4 bytes = 2048 bytes per photo):
```sql
CREATE TABLE IF NOT EXISTS photo_embeddings (
    photo_id TEXT PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
    embedding BLOB NOT NULL,
    model TEXT NOT NULL
);
```

### 2.3 Cosine Distance Search
Implement in-memory Cosine Similarity matching in `flagme-host` (or via `sqlite-vec` extension):
$$\text{similarity} = \frac{A \cdot B}{\|A\| \|B\|}$$

---

## 3. Acceptance Criteria
- [ ] **AC-1**: `AiInfo` in `wasm-geo-models` supports optional dense vector embeddings.
- [ ] **AC-2**: `ai-tagging` plugin optionally exports normalized 512-D float vectors.
- [ ] **AC-3**: `Storage` persists and reads binary embedding BLOBs from `photo_embeddings`.
- [ ] **AC-4**: CLI subcommand `flagme-host similar <PHOTO_ID>` finds top-K visually and semantically similar photos.
