# CountryFlag Plugin (`flagme-plugin-countryflag`)

## Overview

The `countryflag` plugin turns an ISO-3166-1 alpha-2 country code into an English country name, a Unicode flag emoji, a continent and a UN geographic sub-region. It works entirely offline on an embedded table and needs **no capabilities**.

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `countryflag` |
| Crate | `flagme-plugin-countryflag` (`cdylib` + `rlib` + `[[bin]]`) |
| Version | `0.1.0` |
| Reads | `doc.location.country_code` |
| Writes | `doc.country` (`CountryInfo`) |
| Capabilities | none (`[]`) |
| Configurable via `settings` | **No** |

---

## 1. Key Features

1. **Embedded Country Table** — 62 countries (Europe, Asia, Americas, Oceania, Africa) plus `XX` for "unknown".
2. **Computed Flag Emojis** — any two-letter code A–Z is converted to the matching pair of Unicode *Regional Indicator Symbols* (`DE` → 🇩🇪), even if the code is not in the table.
3. **Zero Capabilities** — no disk, network or KV access.
4. **Dual-Mode Execution** — library component for `flagme-host`, or standalone CLI (`stdin` JSON → `stdout` JSON).

---

## 2. Configuration & Parameters

### 2.1 Plugin Entry (`config.yaml`)

```yaml
plugins:
  countryflag:
    enabled: true
    capabilities: []     # no grants required
```

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | bool | `true` | `false` → host skips the plugin; `doc.country` stays empty. |
| `capabilities` | list | `[]` | Not needed. |
| `settings` | map | `{}` | **Not evaluated** by this plugin. |

### 2.2 `settings`

This plugin has **no configurable parameters**. Settings support for all plugins is planned in [#26](https://github.com/Zheng-Bote/wasm_geo-host/issues/26).

### 2.3 Lookup Rules (fixed in code)

| Input `country_code` | `country_code` (output) | `country_name` | `flag_emoji` | `continent` / `region` |
| :--- | :--- | :--- | :--- | :--- |
| Code in the table (e.g. `de`, ` DE `) | trimmed, upper case (`DE`) | from table (`Germany`) | 🇩🇪 | from table |
| `UK` | `UK` | `United Kingdom` | regional indicators `U`+`K` (not an official flag) | `Europe` / `Northern Europe` |
| `XX` | `XX` | `Global / Unknown` | 🌐 | `Unknown` / `Unknown` |
| Unknown two-letter code (e.g. `MC`) | `MC` | `Global / Unknown` | computed (🇲🇨) | `Unknown` / `Unknown` |
| Not exactly two letters A–Z | as given (upper case) | `Global / Unknown` | 🌐 | `Unknown` / `Unknown` |
| Empty string | — | — | — | `doc.country` is **not** set |

### 2.4 Covered Countries

| Continent | Codes |
| :--- | :--- |
| Europe | AT, BE, BG, CH, CZ, DE, DK, EE, ES, FI, FR, GB/UK, GR, HR, HU, IE, IS, IT, LT, LU, LV, NL, NO, PL, PT, RO, RS, SE, SI, SK, UA |
| Asia | AE, CN, HK, ID, IL, IN, JP, KR, MY, PH, SA, SG, TH, TR, TW, VN |
| North America | CA, MX, US |
| South America | AR, BR, CL, CO, PE |
| Oceania | AU, NZ |
| Africa | EG, KE, MA, NG, ZA |

---

## 3. Pipeline Dependencies & Ordering

1. **`reversegeo`** (before) → sets `doc.location.country_code`. Without a code the plugin logs `No country code present…` and returns the document unchanged.
2. **`countryflag`**.

---

## 4. Required Capabilities

None. The plugin works in a fully isolated sandbox (`capabilities: []`).

---

## 5. Data Model Output (`CountryInfo`)

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

## 6. Usage Examples

### 6.1 Host Pipeline

```bash
cargo run --bin flagme-host -- process Examples/sample.jpg --config config.yaml
```

### 6.2 Standalone CLI

```bash
cat doc_with_country_code.json | wasmtime run -S cli-exit-with-code \
  target/wasm32-wasip2/release/flagme-plugin-countryflag.wasm
```

The CLI expects a valid `UnifiedDocument` JSON on `stdin`; empty input causes a parse error.

### 6.3 SQLite

```sql
SELECT country_code, country_name, flag_emoji, continent, region FROM countries;
```

| country_code | country_name | flag_emoji | continent | region |
|---|---|---|---|---|
| `DE` | Germany | 🇩🇪 | Europe | Western Europe |

---

## 7. Limitations

- Country names are English only; native names come from `reversegeo` (`local_country`).
- The `countries` table is keyed by `country_code` (one row per country, not per photo).
