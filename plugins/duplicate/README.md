# Duplicate Plugin (`flagme-plugin-duplicate`)

## Overview

The `duplicate` plugin finds exact copies (SHA-256) and visually similar images (64-bit difference hash, *dHash*) in the collection. Known hashes are kept in the host KV store, so a new photo is compared against all photos processed before it.

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `duplicate` |
| Crate | `flagme-plugin-duplicate` (`cdylib` + `rlib` + `[[bin]]`) |
| Component | `flagme_plugin_duplicate.wasm` — `cargo build --release --target wasm32-wasip2 --lib --features component -p flagme-plugin-duplicate`; loaded by `flagme-host` from `plugin_dir` ([ADR 0003](../../docs/adr/0003-wasm-component-execution.md)) |
| Version | `0.2.0` |
| Reads | `doc.id`, `doc.file.path` (file bytes) |
| Writes | `doc.duplicate` (`DuplicateInfo`), `doc.file.sha256` |
| Capabilities | `filesystem`, `kv` |
| Configurable via `settings` | **Yes** (1 parameter, see §2.2) |

---

## 1. Key Features

1. **SHA-256** — byte-exact duplicate detection (FIPS 180-4, crate `sha2`), also stored in `doc.file.sha256`.
2. **dHash (64 bit)** — the image is decoded (`image` crate: JPEG, PNG, WebP, TIFF, GIF), rotated per EXIF orientation, converted to grayscale and scaled to 9 × 8 pixels; 64 horizontal gradient comparisons → 16 hex digits. Robust against re-compression and resizing.
3. **KV Hash Index** — first-seen photo per hash: `sha256:<hex>` → `doc.id`, `phash:<hex>` → `doc.id`.
4. **Indexed SQLite Table** — `idx_duplicates_phash` on `duplicates(phash)`.
5. **Dual-Mode Execution** — Wasm component (`media-plugin` world, feature `component`) run in the sandbox of `flagme-host`, or standalone CLI (`stdin` JSON → `stdout` JSON).

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
| `settings` | map | `{}` | See §2.2. |

### 2.2 `settings`

| Key | Type / range | Default | Effect |
| :--- | :--- | :--- | :--- |
| `phash_threshold` | integer 0–64 | `0` | Maximum Hamming distance between two dHashes to count as duplicate. `0` = exact match via `phash:<hex>`; `> 0` = linear comparison against the index `phash:index` (see §2.4). |

```yaml
    settings:
      phash_threshold: "8"
```

All values are strings and follow the shared SDK rules (`flagme_sdk::settings`): booleans accept `true`/`yes`/`on`/`1` and `false`/`no`/`off`/`0`; numbers outside the range are clamped with a warning; invalid values and unknown keys produce a `warn` log entry `settings[<plugin>]: …` and the default applies. Settings are parsed once per configuration.

### 2.3 Built-in Parameters (not configurable)

| Parameter | Value | Effect |
| :--- | :--- | :--- |
| dHash grid | 9 columns × 8 rows of the decoded grayscale image (Triangle filter) | 64 bits; bit `row·8+col` set if a pixel is brighter than its right neighbour |
| Undecodable image (e.g. HEIC/AVIF) | `phash = None` | SHA-256 matching still works; no perceptual matching, `info` log entry |
| Unreadable file | no hashes at all | `doc.duplicate` and `doc.file.sha256` stay unchanged, `warn` log entry |
| KV keys | `sha256:<64 hex>`, `phash:<16 hex>` | value = `doc.id` of the first photo |
| KV index (`phash_threshold > 0`) | `phash:index`, one line `<16 hex> <doc.id>` per photo | ≈ 34 bytes per photo |
| Match rule | identical SHA-256 **or** dHash Hamming distance ≤ `phash_threshold`, different `doc.id` | IDs are added at most once |

---

## 3. Pipeline Position

No dependencies on other plugins. Should run before the host persists the document, because `doc.file.sha256` is written to the `photos` table.

---

## 4. Required Capabilities

| Capability | Used for | Without the grant |
| :--- | :--- | :--- |
| `filesystem` | `fs_read(doc.file.path)` | Warning `duplicate: cannot read '<path>': …; no hashes`; `doc.duplicate` stays `None`. |
| `kv` | `kv_get` / `kv_set` of the hash index | Hashes are still computed, but `duplicates` is always empty. |

---

## 5. Data Model Output (`DuplicateInfo`)

```rust
pub struct DuplicateInfo {
    pub sha256: String,            // 64 hex digits
    pub phash: Option<String>,     // 16 hex digits, e.g. "ba2ed74a1cf33d36"; None if not decodable (omitted in JSON)
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
SELECT photo_id, sha256, phash, duplicate_ids FROM duplicates;   -- phash '' = no perceptual hash

-- Photos with identical dHash
SELECT phash, group_concat(photo_id) FROM duplicates WHERE phash <> '' GROUP BY phash HAVING count(*) > 1;
```

---

## 7. Limitations & Implementation Notes

- **Linear index:** with `phash_threshold > 0` every new photo is compared with all entries of `phash:index`; the effort grows linearly with the library.
- **No perceptual hash for HEIC/AVIF:** these formats need decoders outside pure Rust ([#40](https://github.com/Zheng-Bote/wasm_geo-host/issues/40)).
- **The KV index lives in memory** of the host process; after a restart, earlier photos are no longer known as duplicates (the SQL query in §6.3 still works).
- Only the **first** photo per hash is remembered; later copies point to it, not to each other.
