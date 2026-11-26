# Timeline Plugin (`flagme-plugin-timeline`)

## Overview

The `timeline` plugin determines the effective capture time `taken_at` of a photo from competing sources (EXIF, XMP, file name, file system) and records where the value came from (`resolution_source`).

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `timeline` |
| Crate | `flagme-plugin-timeline` (`cdylib` + `rlib` + `[[bin]]`) |
| Component | `flagme_plugin_timeline.wasm` — `cargo build --release --target wasm32-wasip2 --lib --features component -p flagme-plugin-timeline`; loaded by `flagme-host` from `plugin_dir` ([ADR 0003](../../docs/adr/0003-wasm-component-execution.md)) |
| Version | `0.1.0` |
| Reads | `doc.metadata.raw_tags`, `doc.file.filename`, `doc.file.path` (file system mtime) |
| Writes | `doc.timeline` (`TimelineInfo`) |
| Capabilities | `filesystem` (declared, see §4) |
| Configurable via `settings` | **Yes** (2 parameters, see §2.2) |

---

## 1. Key Features

1. **Fixed Priority Order** for `taken_at` (first match wins):

   | Prio | Source | `raw_tags` key / origin | `resolution_source` |
   | :---: | :--- | :--- | :--- |
   | 1 | EXIF DateTimeOriginal | `Exif.Photo.DateTimeOriginal`, `Exif.DateTimeOriginal`, `Exif.Date/Time Original` | `exif_datetime_original` |
   | 2 | EXIF DateTimeDigitized | `Exif.Photo.DateTimeDigitized`, `Exif.DateTimeDigitized` | `exif_datetime_digitized` |
   | 3 | EXIF DateTime | `Exif.Image.DateTime`, `Exif.DateTime` | `exif_datetime` |
   | 4 | XMP CreateDate | `XMP.xmp.CreateDate` | `xmp_create_date` |
   | 5 | File name | `doc.file.filename` (patterns in §2.4) | `filename_timestamp` |
   | 6 | File system | mtime of `doc.file.path` | `filesystem_modified_time` |
   | — | none of the above | `taken_at` stays `None` (warning) | `none` |

   There is no fallback to the current/import time: an invented capture time would mislead weather lookup, the watcher and the timeline ([ADR 0002](../../docs/adr/0002-plugins-without-engine-no-output.md), [#44](https://github.com/Zheng-Bote/wasm_geo-host/issues/44)). The import time is always available in `imported_at`.

2. **Provenance** — `resolution_source` makes the decision traceable.
3. **UTC Normalization** — all values as `chrono::DateTime<Utc>` (ISO 8601).
4. **Dual-Mode Execution** — Wasm component (`media-plugin` world, feature `component`) run in the sandbox of `flagme-host`, or standalone CLI (`stdin` JSON → `stdout` JSON).

---

## 2. Configuration & Parameters

### 2.1 Plugin Entry (`config.yaml`)

```yaml
plugins:
  timeline:
    enabled: true
    capabilities:
      - filesystem
```

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | bool | `true` | `false` → no `doc.timeline`; `weather-enrich` then uses its default date (see its README). |
| `capabilities` | list | `[filesystem]` | See §4. |
| `settings` | map | `{}` | See §2.2. |

### 2.2 `settings`

| Key | Type / values | Default | Effect |
| :--- | :--- | :--- | :--- |
| `timezone` | `UTC`, IANA name (`Europe/Berlin`) or offset (`+02:00`, `-0530`) | `UTC` | Zone for EXIF and file-name times **without** offset; daylight saving time is applied for IANA names. RFC 3339 values and epoch file names keep their own offset. |
| `source_priority` | comma-separated list of `exif_datetime_original`, `exif_datetime_digitized`, `exif_datetime`, `xmp_create_date`, `filename_timestamp`, `filesystem_modified_time` | all six in this order | Order of the sources; sources not listed are skipped. If none matches, `taken_at` stays `None` and `resolution_source` is `none`. |

```yaml
    settings:
      timezone: "Europe/Berlin"
      source_priority: "exif_datetime_original, filename_timestamp, filesystem_modified_time"
```

All values are strings and follow the shared SDK rules (`flagme_sdk::settings`): booleans accept `true`/`yes`/`on`/`1` and `false`/`no`/`off`/`0`; numbers outside the range are clamped with a warning; invalid values and unknown keys produce a `warn` log entry `settings[<plugin>]: …` and the default applies. Settings are parsed once per configuration.

### 2.3 Accepted Date Formats in Metadata

| Format | Example |
| :--- | :--- |
| EXIF | `2025:10:25 09:49:18` |
| ISO without `T` | `2025-10-25 09:49:18` |
| RFC 3339 | `2025-10-25T09:49:18+02:00`, `2025-10-25T07:49:18Z` |
| File-name style | `2025-10-25_094918` |

Surrounding quotes and whitespace are removed.

### 2.4 File-Name Patterns (Prio 5)

| Pattern (start of file stem) | Example | Result |
| :--- | :--- | :--- |
| `YYYY-MM-DD_HHMMSS` | `2025-10-25_094918.jpg` | `2025-10-25T09:49:18Z` |
| `YYYY-MM-DD` | `2026-03-04_Stromzaehler.jpg` | `2026-03-04T12:00:00Z` (noon) |
| 13-digit Unix epoch in ms (anywhere) | `app_inventor_1554303518406.jpg` | `2019-04-03T14:58:38.406Z` |

---

## 3. Pipeline Dependencies & Ordering

1. **`metadata`** (before) → provides `raw_tags`. Without it the plugin starts at Prio 5.
2. **`timeline`**.
3. **`weather-enrich`** (after) → uses `taken_at` for date and hour.

---

## 4. Required Capabilities

| Capability | Used for | Note |
| :--- | :--- | :--- |
| `filesystem` | mtime of the file via `fs_modified` (Prio 6, `modified_at`) | `modified_at` stays `None`; Prio 6 never matches. |

---

## 5. Data Model Output (`TimelineInfo`)

```rust
pub struct TimelineInfo {
    pub taken_at: Option<DateTime<Utc>>,     // effective capture time; None if no source matched
    pub imported_at: DateTime<Utc>,          // time this plugin ran
    pub modified_at: Option<DateTime<Utc>>,  // file system mtime, if readable
    pub resolution_source: String,           // see table in §1
}
```

```json
"timeline": {
  "taken_at": "2025-10-25T09:49:18Z",
  "imported_at": "2026-11-25T12:00:52.266045Z",
  "modified_at": "2025-10-25T10:23:21.101882Z",
  "resolution_source": "exif_datetime_original"
}
```

---

## 6. Usage Examples

### 6.1 Host Pipeline

```bash
cargo run --bin flagme-host -- process Examples/2025/10/Germany/Munich/2025-10-25_094918.jpg --config config.yaml
```

### 6.2 Standalone CLI

```bash
cat doc.json | wasmtime run -S cli-exit-with-code --dir . \
  target/wasm32-wasip2/release/flagme-plugin-timeline.wasm
```

### 6.3 SQLite

```sql
SELECT photo_id, taken_at, imported_at, modified_at, resolution_source FROM timelines;
```

---

## 7. Limitations & Implementation Notes

- **Times without offset use `timezone`** (default `UTC`). `OffsetTimeOriginal` is not evaluated; with the default a photo taken at 09:49 CEST appears as `09:49Z`, with `timezone: Europe/Berlin` as `07:49Z`. Non-existent local times (DST spring-forward gap) are skipped and the next source is tried.
- `SubSecTimeOriginal` is not evaluated (seconds precision).
- Unparseable values in a higher-priority key do not stop the search; the next key is tried.
