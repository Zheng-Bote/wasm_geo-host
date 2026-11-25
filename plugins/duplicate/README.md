# Duplicate Plugin (`flagme-plugin-duplicate`)

## Overview

The `duplicate` plugin finds exact copies (SHA-256) and visually similar images (64-bit difference hash, *dHash*) in the collection. Known hashes are kept in the host KV store, so a new photo is compared against all photos processed before it.

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `duplicate` |
| Crate | `flagme-plugin-duplicate` (`cdylib` + `rlib` + `[[bin]]`) |
| Version | `0.1.0` |
| Reads | `doc.id`, `doc.file.path` (file bytes) |
| Writes | `doc.duplicate` (`DuplicateInfo`), `doc.file.sha256` |
| Capabilities | `filesystem`, `kv` |
| Configurable via `settings` | **No** |

---

## 1. Key Features

1. **SHA-256** — byte-exact duplicate detection (FIPS 180-4, crate `sha2`), also stored in `doc.file.sha256`.
2. **dHash (64 bit)** — 9 × 8 grid of mean values over the JPEG scan data (after the SOS marker), 64 horizontal gradient comparisons → 16 hex digits.
3. **KV Hash Index** — first-seen photo per hash: `sha256:<hex>` → `doc.id`, `phash:<hex>` → `doc.id`.
4. **Indexed SQLite Table** — `idx_duplicates_phash` on `duplicates(phash)`.
5. **Dual-Mode Execution** — library component for `flagme-host`, or standalone CLI (`stdin` JSON → `stdout` JSON).

---

## 2. Configuration & Parameters

### 2.1 Plugin Entry (`config.yaml`)

```yaml
plugins:
  duplicate:
    enabled: true
    capabilities:
      - filesystem     # read file bytes
      - kv             # hash index
```

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | bool | `true` | `false` → no hashes; `doc.file.sha256` stays empty. |
| `capabilities` | list | `[filesystem, kv]` | See §4. |
| `settings` | map | `{}` | **Not evaluated** by this plugin. |

### 2.2 `settings`

This plugin has **no configurable parameters**. Settings support for all plugins is planned in [#26](https://github.com/Zheng-Bote/wasm_geo-host/issues/26).

> **Note:** `config.docker.yaml` (`phash_threshold: "10"`) and `config.mac-studio.yaml` (`phash_threshold: "8"`) set a Hamming threshold that is **ignored**. Matching is always **exact** (Hamming distance 0, see §7).

### 2.3 Built-in Parameters (fixed in code)

| Parameter | Value | Effect |
| :--- | :--- | :--- |
| dHash grid | 9 columns × 8 rows | 64 bits |
| Sampled region | JPEG: data after SOS (`0xFFDA`); other formats: whole file | skips EXIF/APP segments |
| Empty input | `phash = "0000000000000000"` | |
| KV keys | `sha256:<64 hex>`, `phash:<16 hex>` | value = `doc.id` of the first photo |
| Match rule | identical SHA-256 **or** identical dHash, different `doc.id` | IDs are added at most once |

---

## 3. Pipeline Position

No dependencies on other plugins. Should run before the host persists the document, because `doc.file.sha256` is written to the `photos` table.

---

## 4. Required Capabilities

| Capability | Used for | Without the grant |
| :--- | :--- | :--- |
| `filesystem` | `fs_read(doc.file.path)` | The hashes are computed over the **file path string** instead of the content — no meaningful duplicates. |
| `kv` | `kv_get` / `kv_set` of the hash index | Hashes are still computed, but `duplicates` is always empty. |

---

## 5. Data Model Output (`DuplicateInfo`)

```rust
pub struct DuplicateInfo {
    pub sha256: String,            // 64 hex digits
    pub phash: String,             // 16 hex digits, e.g. "ba2ed74a1cf33d36"
    pub duplicates: Vec<String>,   // doc.id of earlier matches (omitted in JSON if empty)
}
```

---

## 6. Usage Examples

### 6.1 Host Pipeline

```bash
cargo run --bin flagme-host -- process Examples/sample.jpg --config config.yaml
```

### 6.2 Standalone CLI

```bash
cat doc.json | wasmtime run -S cli-exit-with-code --dir . \
  target/wasm32-wasip2/release/flagme-plugin-duplicate.wasm
```

The CLI uses `NoopCapabilities` (KV without memory) → `duplicates` is always empty.

### 6.3 SQLite

```sql
SELECT photo_id, sha256, phash, duplicate_ids FROM duplicates;

-- Photos with identical dHash
SELECT phash, group_concat(photo_id) FROM duplicates GROUP BY phash HAVING count(*) > 1;
```

---

## 7. Limitations & Implementation Notes

- **No similarity threshold:** dHash matches only if all 64 bits are identical. A Hamming comparison (`phash_threshold`) is not implemented.
- **dHash over compressed bytes:** the grid is computed over the encoded JPEG stream, not decoded pixels. It is therefore not robust against re-compression or resizing.
- **The KV index lives in memory** of the host process; after a restart, earlier photos are no longer known as duplicates (the SQL query in §6.3 still works).
- Only the **first** photo per hash is remembered; later copies point to it, not to each other.
