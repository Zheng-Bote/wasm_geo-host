# ReverseGeo Plugin (`flagme-plugin-reversegeo`)

## Overview

The `reversegeo` plugin resolves GPS coordinates from the [`UnifiedDocument`](../../crates/models/src/lib.rs) into administrative locations in **two languages**:

- **International / English**: `country`, `country_code`, `region`, `county`, `city`.
- **Native / localized**: `local_country`, `local_region`, `local_city`, `local_language` (e.g. `Deutschland` / `München` / `de`).

Resolution order: **KV cache → offline table → online geocoder (Nominatim) → `Unknown Country` fallback**.

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `reversegeo` |
| Crate | `flagme-plugin-reversegeo` (`cdylib` + `rlib` + `[[bin]]`) |
| Component | `flagme_plugin_reversegeo.wasm` — `cargo build --release --target wasm32-wasip2 --lib --features component -p flagme-plugin-reversegeo`; loaded by `flagme-host` from `plugin_dir` ([ADR 0003](../../docs/adr/0003-wasm-component-execution.md)) |
| Version | `0.1.0` |
| Reads | `doc.location.latitude`, `longitude`, `altitude` |
| Writes | `doc.location.{country, country_code, region, county, city, local_*}` (coordinates stay unchanged) |
| Capabilities | `http`, `kv` |
| Configurable via `settings` | **Yes** (7 parameters, see §2.2) |

---

## 1. Key Features

1. **Offline Nearest-City Lookup** — 34 reference cities in 16 countries with native names (table in §2.4). Zero network overhead.
2. **Country Bounding-Box Fallback** — 7 country boxes (DE, FR, IT, US, JP, ES, GB) when no reference city is close enough.
3. **Online Fallback** — Nominatim / OpenStreetMap (`accept-language` configurable, default `en`) for coordinates outside the offline data.
4. **KV Caching** — results are stored per coordinate bucket (`geo:<lat>:<lon>`, default 3 decimals).
5. **Dual-Mode Execution** — Wasm component (`media-plugin` world, feature `component`) run in the sandbox of `flagme-host`, or standalone CLI (`stdin` JSON → `stdout` JSON).

---

## 2. Configuration & Parameters

### 2.1 Plugin Entry (`config.yaml`)

```yaml
plugins:
  reversegeo:
    enabled: true
    capabilities:
      - http     # online fallback (Nominatim)
      - kv       # result cache
```

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | bool | `true` | `false` → host skips the plugin; `countryflag` then finds no `country_code`. |
| `capabilities` | list | `[http, kv]` | See §4. |
| `settings` | map | `{}` | See §2.2. |

### 2.2 `settings`

| Key | Type / range | Default | Effect |
| :--- | :--- | :--- | :--- |
| `offline_cache` | bool | `true` | `false` → no KV cache reads or writes. |
| `offline_lookup` | bool | `true` | `false` → skip the embedded city table and country boxes. |
| `online_lookup` | bool | `true` | `false` → never query the online geocoder; unresolved coordinates become `Unknown Country` / `XX`. |
| `city_radius_km` | integer 1–2000 | `200` | Search radius around the reference cities (200 km = the former fixed 1.8°). |
| `cache_precision` | integer 0–6 | `3` | Decimal places of the cache key `geo:<lat>:<lon>` (3 ≈ 111 m). |
| `nominatim_url` | `http(s)://` URL | `https://nominatim.openstreetmap.org/reverse` | Base URL; the plugin appends `?format=jsonv2&lat=…&lon=…&accept-language=…`. |
| `accept_language` | string | `en` | Language of the online request; stored as `local_language` for online results. |

```yaml
    settings:
      online_lookup: "false"      # offline only
      city_radius_km: "100"
      accept_language: "de"
```

All values are strings and follow the shared SDK rules (`flagme_sdk::settings`): booleans accept `true`/`yes`/`on`/`1` and `false`/`no`/`off`/`0`; numbers outside the range are clamped with a warning; invalid values and unknown keys produce a `warn` log entry `settings[<plugin>]: …` and the default applies. Settings are parsed once per configuration.

### 2.3 Built-in Parameters (not configurable)

| Parameter | Value | Effect |
| :--- | :--- | :--- |
| Distance measure | `dist² = Δlat² + (Δlon · max(cos(lat), 0.2))²` in degrees, radius `city_radius_km / 111.1` | The nearest city within the radius wins. |
| Online query | `<nominatim_url>?format=jsonv2&lat=…&lon=…&accept-language=<accept_language>` | Only if the offline lookup finds nothing and `online_lookup = true`. |
| HTTP timeout | 2 s (host `curl --max-time 2`) | Defined by the host capability, not by the plugin. |
| Fallback result | `country = "Unknown Country"`, `country_code = "XX"`, `region = "Global"`, `local_language = "en"` | When offline and online both fail. |

### 2.4 Offline Reference Data

| Country | Cities (English name) |
| :--- | :--- |
| DE | Berlin, Munich, Hamburg, Cologne, Frankfurt, Stuttgart, Düsseldorf, Leipzig, Dresden |
| AT | Vienna, Innsbruck |
| CH | Zurich, Geneva |
| FR | Paris, Lyon, Marseille |
| IT | Rome, Milan, Venice |
| ES | Madrid, Barcelona |
| PT | Lisbon |
| GB | London |
| NL | Amsterdam |
| SE / NO / DK | Stockholm / Oslo / Copenhagen |
| US | San Francisco, Los Angeles, New York |
| CA | Toronto |
| JP | Tokyo, Osaka |
| AU | Sydney |

### 2.5 Field Mapping per Resolution Path

| Field | Offline city | Country box | Nominatim |
| :--- | :--- | :--- | :--- |
| `country` | English name | English name | `address.country` |
| `country_code` | ISO-3166 alpha-2 | ISO-3166 alpha-2 | `address.country_code` (upper case) |
| `region` | English region | capital region of the box | `state` → `region` → city |
| `county` | from table | capital city of the box | `county` → city |
| `city` | English city | capital city of the box | `city` → `town` → `village` → `municipality` |
| `local_*` | native names | native names | **copy of the English values**, `local_language = "en"` |

---

## 3. Pipeline Dependencies & Ordering

1. **`metadata`** (before) → provides the coordinates. Without `latitude` **and** `longitude` the plugin logs `No GPS coordinates…` and returns the document unchanged.
2. **`reversegeo`**.
3. **`countryflag`** (after) → uses `doc.location.country_code`.
4. **`privacy-redact`** (later) → rounds the coordinates; `reversegeo` must run **before** it to geocode the exact position.

---

## 4. Required Capabilities

| Capability | Used for | Without the grant |
| :--- | :--- | :--- |
| `kv` | `kv_get` / `kv_set` with key `geo:<lat>:<lon>` | No caching; every document is resolved again. |
| `http` | Nominatim request | Coordinates outside the offline data end up as `Unknown Country` / `XX`. |

---

## 5. Data Model Output (`LocationInfo`)

```rust
pub struct LocationInfo {
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude: Option<f64>,

    // English / standard
    pub country: Option<String>,          // "Germany"
    pub country_code: Option<String>,     // "DE"
    pub region: Option<String>,           // "Bavaria"
    pub county: Option<String>,           // "Munich"
    pub city: Option<String>,             // "Munich"

    // Localized (native language of the country)
    pub local_country: Option<String>,    // "Deutschland"
    pub local_region: Option<String>,     // "Bayern"
    pub local_city: Option<String>,       // "München"
    pub local_language: Option<String>,   // "de"
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
cat doc_with_coords.json | wasmtime run -S cli-exit-with-code \
  target/wasm32-wasip2/release/flagme-plugin-reversegeo.wasm
```

The CLI runs with `NoopCapabilities`: no HTTP (offline lookup only) and no persistent cache.

### 6.3 SQLite

```sql
SELECT photo_id, country, city, local_country, local_city, local_language FROM locations;
```

---

## 7. Limitations & Implementation Notes

- **Country boxes are coarse:** a coordinate in Germany more than `city_radius_km` (default 200 km) away from every reference city is resolved as `Berlin` (box capital). The boxes overlap; the first matching entry (order DE, FR, IT, US, JP, ES, GB) wins.
- **Online results use one language** (`accept_language`): `local_*` and the international fields get the same values.
- **The KV cache lives in memory** of the host process and is lost on restart.
- If the host has no network, `http_get` returns `{"status": "offline", …}`; this is treated like a failed request.
