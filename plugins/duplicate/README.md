# Duplicate Plugin (`flagme-plugin-duplicate`)

## Overview
The `duplicate` plugin identifies exact binary duplicates and visual perceptual similarities across the media collection using cryptographic digests and difference hashing.

---

## Key Features

1. **Dual Hashing Strategy**:
   - **Cryptographic SHA-256**: Generates a 256-bit hash for exact, byte-for-byte duplicate detection.
   - **Perceptual Hash (`phash` / `dHash`)**: Generates an invariant 64-bit hexadecimal visual hash that remains stable across image resizing, compression artifacts, and minor color adjustments.
2. **Persistent Hash Index via KV**:
   - Integrates with the host's KV-store capability to maintain an index of seen hashes, immediately identifying duplicates (`duplicate_ids`) without scanning entire directories.
3. **Database Indexing**:
   - Relational schema in SQLite indexes both `sha256` and `phash` (`CREATE INDEX idx_duplicates_phash ON duplicates(phash)`), enabling instant similarity queries.
4. **Dual-Mode Execution**:
   - **Library Component (`cdylib`)**: Loaded into `flagme-host`.
   - **Standalone CLI Binary (`[[bin]]`)**: Streams in Unix pipelines reading `stdin` JSON and outputting duplicate analysis.

---

## Data Model Output (`DuplicateInfo`)

```rust
pub struct DuplicateInfo {
    pub sha256: String,            // "0000000000000000842fca2aec818f32..."
    pub phash: String,             // "ba2ed74a1cf33d36"
    pub duplicates: Vec<String>,   // List of duplicate photo IDs
}
```

---

## Required Capabilities

When executing under sandboxed host control (`config.yaml`), the plugin requires:
- `filesystem`: Access to read file bytes for hashing.
- `kv`: Key-value storage access to look up and store existing image hashes.

```yaml
plugins:
  duplicate:
    enabled: true
    capabilities:
      - filesystem
      - kv
```

---

## Usage Examples

### 1. In Host Pipeline
```bash
cargo run --bin flagme-host -- process Examples/sample.jpg --config config.yaml
```

### 2. Standalone WASM CLI via Unix Pipe
```bash
cat doc.json | wasmtime run -S cli-exit-with-code target/wasm32-wasip2/release/flagme-plugin-duplicate.wasm
```

### 3. Verification in SQLite
```sql
SELECT photo_id, sha256, phash, duplicate_ids FROM duplicates;
```
| photo_id | sha256 | phash | duplicate_ids |
|---|---|---|---|
| `3f12502733adb127` | 00000000...842fca2aec818f32 | ba2ed74a1cf33d36 | [] |
