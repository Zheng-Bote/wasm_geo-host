# CountryFlag Plugin (`flagme-plugin-countryflag`)

## Overview
The `countryflag` plugin resolves ISO-3166 alpha-2 country codes into localized country names, Unicode flag emojis, continental assignments, and geopolitical regions.

---

## Key Features

1. **Embedded ISO-3166 Database**:
   - Contains a comprehensive offline static lookup table of countries, Unicode emoji representations (e.g. `🇩🇪`, `🇫🇷`, `🇯🇵`, `🇺🇸`), continents, and UN geographical sub-regions.
2. **Zero-Trust & Zero-Capability Security**:
   - Requires **no ambient authority** and **no capability grants** (`capabilities: []`).
   - Operates in strict isolation without disk, network, or environment access.
3. **Seamless Downstream Enrichment**:
   - Takes the `country_code` resolved by `reversegeo` and populates the `UnifiedDocument.country` block for fast international filtering and visual presentation in UIs.
4. **Dual-Mode Execution**:
   - **Library Component (`cdylib`)**: Loaded into `flagme-host`.
   - **Standalone CLI Binary (`[[bin]]`)**: Streams in Unix pipelines reading `stdin` JSON and outputting enriched `stdout` JSON.

---

## Data Model Output (`CountryInfo`)

```rust
pub struct CountryInfo {
    pub country_code: String,    // "DE"
    pub country_name: String,    // "Germany"
    pub flag_emoji: String,      // "🇩🇪"
    pub continent: String,       // "Europe"
    pub region: String,          // "Western Europe"
}
```

---

## Configuration (`config.yaml`)

Since this plugin operates strictly offline on in-memory tables, its capability list is empty:

```yaml
plugins:
  countryflag:
    enabled: true
    capabilities: []
```

---

## Usage Examples

### 1. In Host Pipeline
```bash
cargo run --bin flagme-host -- process Examples/sample.jpg --config config.yaml
```

### 2. Standalone WASM CLI via Unix Pipe
```bash
cat doc_with_country_code.json | wasmtime run -S cli-exit-with-code target/wasm32-wasip2/release/flagme-plugin-countryflag.wasm
```

### 3. Verification in SQLite
```sql
SELECT country_code, country_name, flag_emoji, continent, region FROM countries;
```
| country_code | country_name | flag_emoji | continent | region |
|---|---|---|---|---|
| `DE` | Germany | 🇩🇪 | Europe | Western Europe |
