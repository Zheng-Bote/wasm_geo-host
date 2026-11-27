# ReverseGeo Plugin (`flagme-plugin-reversegeo`)

## Overview

The `reversegeo` plugin resolves GPS coordinates from the [`UnifiedDocument`](../../crates/models/src/lib.rs) into administrative locations in **two languages**:

- **International / English**: `country`, `country_code`, `region`, `county`, `city`.
- **Native / localized**: `local_country`, `local_region`, `local_city`, `local_language` (e.g. `Deutschland` / `München` / `de`).

Resolution order: **KV cache → offline GeoNames gazetteer → online geocoder (Nominatim)**. If none of them resolves the coordinates, the name fields stay empty, the plugin logs `reversegeo: no location names for '<path>' (<lat>, <lon>): <cause>` and caches nothing; no placeholder location is invented ([#52](https://github.com/Zheng-Bote/wasm_geo-host/issues/52), [ADR 0002](../../docs/adr/0002-plugins-without-engine-no-output.md)).

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `reversegeo` |
| Crate | `flagme-plugin-reversegeo` (`cdylib` + `rlib` + `[[bin]]`) |
| Component | `flagme_plugin_reversegeo.wasm` — `cargo build --release --target wasm32-wasip2 --lib --features component -p flagme-plugin-reversegeo`; loaded by `flagme-host` from `plugin_dir` ([ADR 0003](../../docs/adr/0003-wasm-component-execution.md)) |
| Version | `0.4.0` |
| Reads | `doc.location.latitude`, `longitude`, `altitude` |
| Writes | `doc.location.{country, country_code, region, county, city, local_*}` (coordinates stay unchanged) |
| Capabilities | `http`, `kv` |
| Configurable via `settings` | **Yes** (7 parameters, see §2.2) |

---

## 1. Key Features

1. **Offline Nearest-Place Lookup** — embedded [GeoNames](https://www.geonames.org/) gazetteer `cities1000` (162,380 populated places worldwide, 7,280 in Germany; CC BY 4.0, data of 2026-11-27) with English and local names for place, county, region and country (§2.4, [ADR 0008](../../docs/adr/0008-short-region-names.md)). No network, no coordinates leave the machine.
2. **No invented places** — without a place within `city_radius_km` the offline lookup returns nothing; there are no reference cities or country boxes any more (#62, #63).
3. **Online Fallback** — Nominatim / OpenStreetMap (`accept-language` configurable, default `en`) for coordinates with no place within `city_radius_km`.
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
| `offline_lookup` | bool | `true` | `false` → skip the embedded GeoNames gazetteer. |
| `online_lookup` | bool | `true` | `false` → never query the online geocoder; coordinates outside the offline data keep empty names (warning). |
| `city_radius_km` | integer 1–200 | `25` | Maximum great-circle distance from the photo to the nearest gazetteer place; farther places are not used (until 0.2.0: radius around 35 reference cities, default 200). |
| `cache_precision` | integer 0–6 | `3` | Decimal places of the cache key `geo:<lat>:<lon>` (3 ≈ 111 m). |
| `nominatim_url` | `http(s)://` URL | `https://nominatim.openstreetmap.org/reverse` | Base URL; the plugin appends `?format=jsonv2&lat=…&lon=…&accept-language=…`. |
| `accept_language` | string | `en` | Language of the online request; stored as `local_language` for online results. |

```yaml
    settings:
      online_lookup: "false"      # offline only
      city_radius_km: "50"
      accept_language: "de"
```

All values are strings and follow the shared SDK rules (`flagme_sdk::settings`): booleans accept `true`/`yes`/`on`/`1` and `false`/`no`/`off`/`0`; numbers outside the range are clamped with a warning; invalid values and unknown keys produce a `warn` log entry `settings[<plugin>]: …` and the default applies. Settings are parsed once per configuration.

### 2.3 Built-in Parameters (not configurable)

| Parameter | Value | Effect |
| :--- | :--- | :--- |
| Distance measure | Great-circle distance (haversine, earth radius 6,371 km) over the 1° grid cells that cover `city_radius_km` | The nearest place within the radius wins; ties go to the lower index. |
| Online query | `<nominatim_url>?format=jsonv2&lat=…&lon=…&accept-language=<accept_language>` | Only if no gazetteer place lies within `city_radius_km` and `online_lookup = true`. |
| HTTP timeout | 2 s (host `curl --max-time 2`) | Defined by the host capability, not by the plugin. |
| No result | Name fields stay `None`, coordinates are kept, warning with the cause (`no place within city_radius_km (<n> km) … online_lookup is off`, `unreadable geocoder response` or the `http_get` error), no cache entry | When offline and online both fail. |
| Unusable data | `reversegeo: offline data unusable: <reason>` warning; treated like "no place found" | `places.bin` damaged or of another format version. |

### 2.4 Offline Gazetteer (GeoNames)

| Property | Value |
| :--- | :--- |
| Source | [GeoNames](https://www.geonames.org/) dumps `cities1000`, `admin1CodesASCII`, `admin2Codes`, `countryInfo`, `alternateNamesV2` |
| Licence | **CC BY 4.0** ([creativecommons.org/licenses/by/4.0](https://creativecommons.org/licenses/by/4.0/)); attribution in [`NOTICE`](../../NOTICE) |
| Data date | 2026-11-27 (also stored in the file header) |
| Content | 162,380 populated places (> 1,000 inhabitants or administrative seats; city sections `PPLX` excluded), 252 countries, 3,865 first-level and 47,752 second-level divisions |
| File | `data/places.bin` (≈ 7.0 MB, format version 1), embedded with `include_bytes!`; format described in `src/gazetteer.rs` |
| Refresh | `make geonames` downloads the dumps (≈ 220 MB) outside the repository, rebuilds the file with `tools/geonames-import` and leaves it for review and commit |
| Coverage | 0.1° grid over Germany and the Alps: median 3.7 km, 90 % within 10 km to the nearest place; 95.6 % of a 0.25° grid within 25 km (the rest is mostly sea) |

Name rules ([ADR 0008](../../docs/adr/0008-short-region-names.md), superseding ADR 0006):

| Field | Source |
| :--- | :--- |
| `city`, `county` | Preferred English name (`alternateNamesV2`, `en`), else the GeoNames main name |
| `local_city` | Preferred name in the country's **first** language (`countryInfo`), else the GeoNames main name |
| `region`, `local_region` | Base name as for `city`/`local_city` (preferred name, else the `admin1CodesASCII` name); if GeoNames has a **short form** of the same language that occurs in the base name as a **whole word** sequence, the short form: `State of Salzburg` → `Salzburg`, `Canton of Geneva` → `Geneva`, `Free and Hanseatic City of Hamburg` → `Hamburg`, `Land Berlin` → `Berlin`, `Kanton Basel-Stadt` → `Basel-Stadt`, `Hlavní město Praha` → `Praha`. Without such a short form the base name stays, e.g. `State of Berlin` (GeoNames has no English short form for Berlin), `Rhineland-Palatinate`, `Walloon Region`. Names are never built or cut. |
| `country` | `countryInfo` name (e.g. `Czechia`) |
| `local_country` | Country name in its first language, short form first (`Deutschland`); empty if GeoNames has none |
| `local_language` | First language of the country (`de`, `fr`, …) |

### 2.5 Field Mapping per Resolution Path

| Field | Offline gazetteer | Nominatim |
| :--- | :--- | :--- |
| `country` | `countryInfo` name | `address.country` |
| `country_code` | ISO-3166 alpha-2 | `address.country_code` (upper case) |
| `region` | first-level division (English) | `state` → `region` → city |
| `county` | second-level division (English), `None` if GeoNames has none | `county` → city |
| `city` | nearest place (English) | `city` → `town` → `village` → `municipality` |
| `local_*` | local names, see §2.4 | **copy of the English values**, `local_language = "en"` |

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
| `http` | Nominatim request | Coordinates with no gazetteer place within `city_radius_km` keep empty names; warning with the cause. |

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
    pub county: Option<String>,           // "Upper Bavaria"
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

The CLI runs with `NoopCapabilities`: no HTTP (offline gazetteer only) and no persistent cache. It embeds the same `places.bin` as the component.

### 6.3 SQLite

```sql
SELECT photo_id, country, city, local_country, local_city, local_language FROM locations;
```

---

## 7. Limitations & Implementation Notes

- **Nearest place, not municipality:** the gazetteer holds points, not boundaries. A photo gets the nearest populated place, which is usually the town it was taken in or a direct neighbour. Close to a border the nearest place can lie in the neighbouring country (e.g. a point in Rome's Trastevere resolves to `Vatican City`).
- **Districts coded as places:** GeoNames lists some city districts as ordinary places or administrative seats (e.g. `Královské Vinohrady` in Prague, `Camden` in London, `Paris 18 Buttes-Montmartre`); a photo there gets the district name, not the city.
- **Multilingual countries use their first language** for `local_*` (from `countryInfo`): Geneva is `Genf` (`de`), Brussels `Brussel` (`nl`), Bolzano `Bolzano` (`it`).
- **Data size:** `places.bin` adds ≈ 7 MB to the component; on macOS wasmtime copies it into each instance (measured median of a `reversegeo` call via the host: < 1 ms).
- **Online results use one language** (`accept_language`): `local_*` and the international fields get the same values.
- **The KV cache lives in memory** of the host process and is lost on restart.
- If the host has no network, `http_get` fails with the cause; the names stay empty and nothing is cached (#52).
