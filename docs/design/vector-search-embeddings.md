---
feature: vector-search-embeddings
status: draft
archive-after:
touches:
  - "crates/models/**"
  - "plugins/ai-tagging/**"
  - "crates/host/**"
  - "crates/host/migrations/**"
---

# Tech Spec — vector-search-embeddings (`vector-search-embeddings`)

Setzt das PRD `docs/prd/vector-search-embeddings.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern.

## Architektur & Komponenten

Die Vektorsuche kombiniert das einheitliche Dokumentenmodell, das KI-Tagging-Plugin und die SQLite-Speicher-Engine:

```text
+-------------------------------------------------------------+
|                     flagme-host (Host)                      |
|  - CLI Subcommand: similar <PHOTO_ID>                      |
|  - In-Memory Cosine Similarity Ranking Engine               |
|  - Storage Engine (photo_embeddings BLOB Table)             |
+------------------------------+------------------------------+
                               |
                               v
+-------------------------------------------------------------+
|                   ai-tagging (Plugin)                       |
|  - VisionModel::predict_embedding(path) -> Option<Vec<f32>> |
|  - Liefert normierte 512-D Vektoren in AiInfo               |
+-------------------------------------------------------------+
                               |
                               v
+-------------------------------------------------------------+
|                   SQLite (media.db)                         |
|  - Tabelle: photo_embeddings                                |
|    (photo_id, embedding BLOB, model TEXT)                   |
+-------------------------------------------------------------+
```

### 1. Datenmodell-Erweiterung (AC-1)
In `crates/models/src/lib.rs`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AiInfo {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<AiTag>,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
}
```
Erfüllt **AC-1**: Dichte Vektoreinbettungen im Datenmodell.

### 2. Plugin & VisionModel Embedding-Generierung (AC-2)
In `plugins/ai-tagging/src/lib.rs`:
- Trait-Erweiterung `VisionModel::predict_with_embedding(&self, path: &str) -> (Vec<AiTag>, String, Option<Vec<f32>>)`.
- Standard-Implementierungen liefern normierte Vektoren (z. B. 512 Dimensionen mit euklidischer Norm = 1.0).
- `AiTaggingPlugin::process` befüllt `doc.ai.embedding`.
Erfüllt **AC-2**: Export normierter 512-D Vektoren aus dem Plugin.

### 3. SQLite BLOB Speicherung & Kosinus-Ähnlichkeit (AC-3)
In `crates/host/migrations/20261125000000_init.sql`:
```sql
CREATE TABLE IF NOT EXISTS photo_embeddings (
    photo_id TEXT PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
    embedding BLOB NOT NULL,
    model TEXT NOT NULL
);
```
In `crates/host/src/storage.rs`:
- Konvertierung von `Vec<f32>` in Byte-BLOBs (`f32::to_le_bytes`) und Hex-kodierte SQLite-Inserts (`X'...'`).
- Methode `Storage::get_embedding(&self, photo_id: &str) -> Option<Vec<f32>>`.
- Methode `Storage::find_similar(&self, target_id: &str, limit: usize) -> anyhow::Result<Vec<SimilarityResult>>`.
- Kosinus-Ähnlichkeit: $\text{sim}(u, v) = \frac{u \cdot v}{\|u\|_2 \|v\|_2}$.
Erfüllt **AC-3**: Effiziente BLOB-Persistenz und Vektordistanzberechnung.

### 4. CLI Subcommand "similar" & Verifikation (AC-4)
In `crates/host/src/main.rs`:
- CLI-Subcommand `flagme-host similar <PHOTO_ID> [--limit <N>] [--config <CONFIG>]`.
- Listet die ähnlichsten Fotos mit Ähnlichkeits-Score (0.0 bis 1.0) auf.
In `crates/host/tests/vector_test.rs`:
- Integrationstests überprüfen Speicherung, Ähnlichkeitsabfrage und CLI-Befehl.
Erfüllt **AC-4**: Benutzerfreundliches Subcommand und Testabsicherung.
