# ReverseGeo Plugin (`flagme-plugin-reversegeo`)

## Overview
The `reversegeo` plugin resolves GPS coordinates (`latitude`, `longitude`) in the [UnifiedDocument](../../crates/models/src/lib.rs) into structured administrative locations with **dual-language support**:
- **Default International / English**: Standardized naming for global querying and indexing (`country`, `region`, `city`).
- **Native Localized**: Names in the official language of the detected country code (`local_country`, `local_region`, `local_city`, `local_language`).

---

## Key Features

1. **Dual-Language Geocoding**:
   - Resolves international English names as well as the native language based on ISO-3166 country codes.
   - Examples:
     - `DE`: Germany / Deutschland, Berlin / Berlin (`de`)
     - `IT`: Italy / Italia, Rome / Roma (`it`)
     - `JP`: Japan / 日本, Tokyo / 東京 (`ja`)
     - `FR`: France / France, Paris / Paris (`fr`)
     - `US`: United States / United States, San Francisco (`en`)
2. **Dual Resolution Strategy**:
   - **Offline Spatial Lookup**: High-performance in-memory bounding-box spatial table derived from Natural Earth and OpenStreetMap datasets. Operates with zero network overhead.
   - **Online Provider Fallback**: Outbound HTTP requests to Nominatim / OSM with `accept-language` query parameters when coordinates fall outside offline bounds.
3. **High-Performance KV Caching**:
   - Results are keyed by coordinate buckets (`geo:<lat>:<lon>`) and cached in the host's KV capability to eliminate redundant computations and API calls.
4. **Dual-Mode Execution**:
   - **Library Component (`cdylib`)**: Dynamically loaded into `flagme-host` sandbox.
   - **Standalone CLI Binary (`[[bin]]`)**: Usable in Unix pipelines (`stdin` $\to$ `stdout`).

---

## Data Model Output (`LocationInfo`)

```rust
pub struct LocationInfo {
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude: Option<f64>,

    // English / Standard
    pub country: Option<String>,          // "Germany"
    pub country_code: Option<String>,     // "DE"
    pub region: Option<String>,           // "Berlin"
    pub county: Option<String>,           // "Berlin"
    pub city: Option<String>,             // "Berlin"

    // Localized (based on country code)
    pub local_country: Option<String>,    // "Deutschland"
    pub local_region: Option<String>,     // "Berlin"
    pub local_city: Option<String>,       // "Berlin"
    pub local_language: Option<String>,   // "de"
}
```

---

## Required Capabilities

When executing in sandboxed environments (`config.yaml`), the plugin requires:
- `http`: Outbound network access for online geocoder fallback.
- `kv`: Key-value cache storage for memoizing geocoding results.

```yaml
plugins:
  reversegeo:
    enabled: true
    capabilities:
      - http
      - kv
```

---

## Usage Examples

### 1. In Host Pipeline (`flagme-host`)
```bash
cargo run --bin flagme-host -- process Examples/sample.jpg --config config.yaml
```

### 2. Standalone WASM CLI via Unix Pipe
```bash
cat doc_with_coords.json | wasmtime run -S cli-exit-with-code target/wasm32-wasip2/release/flagme-plugin-reversegeo.wasm
```

### 3. Containerized Execution (Docker / Podman)
```bash
podman run --rm -v $(pwd)/Examples:/data:Z wasm-geo-host process /data/sample.jpg
```
