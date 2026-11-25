# Issue: Dual-Language Reverse Geocoding (English Default + Local by Country Code)

- **Issue ID**: #002
- **Feature Name**: `dual-language-reversegeo`
- **Component**: `crates/models`, `plugins/reversegeo`, `crates/host`
- **Target Branch**: `feat/dual-language-reversegeo` -> `main`

---

## 1. Problem Statement & Motivation
Currently, the `reversegeo` plugin resolves GPS coordinates only in a single language (defaulting to English, e.g., "Germany", "Berlin"). When organizing media collections, users frequently need both internationalized naming for global querying/filtering and localized naming in the native tongue of the location (e.g., DE -> "Deutschland", "München"; IT -> "Italia", "Roma"; JP -> "日本", "東京").

## 2. Scope & Technical Specification (Option 1a)

### 2.1 Unified Document Model (`wasm-geo-models`)
Extend `LocationInfo` with explicit local fields:
```rust
pub struct LocationInfo {
    // Coordinates
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude: Option<f64>,

    // Standard / English (Default)
    pub country: Option<String>,
    pub country_code: Option<String>,
    pub region: Option<String>,
    pub county: Option<String>,
    pub city: Option<String>,

    // Localized (based on country code / native locale)
    pub local_country: Option<String>,
    pub local_region: Option<String>,
    pub local_city: Option<String>,
    pub local_language: Option<String>,
}
```

### 2.2 Reverse Geocoding Resolution (`plugins/reversegeo`)
- **Offline Resolution**: Update lookup table to contain dual names:
  - DE: `Germany` / `Deutschland`, `Berlin` / `Berlin`, `de`
  - FR: `France` / `France`, `Paris` / `Paris`, `fr`
  - US: `United States` / `United States`, `San Francisco`, `en`
  - JP: `Japan` / `日本`, `Tokyo` / `東京`, `ja`
  - IT: `Italy` / `Italia`, `Rome` / `Roma`, `it`
- **Online Resolution**: Dual query / header lookup via OpenStreetMap Nominatim with `accept-language=en` and `accept-language=<local_code>`.
- **KV Caching**: Keyed by `geo:<lat>:<lon>` caching the complete dual-language `LocationInfo`.

### 2.3 SQLite Storage Schema & Persistence (`crates/host`)
- Update `locations` table schema in migration:
  - `local_country TEXT`
  - `local_region TEXT`
  - `local_city TEXT`
  - `local_language TEXT`
- Update `Storage::save_document` SQL insert generation to persist the local fields.

## 3. Acceptance Criteria
- **AC-1**: `LocationInfo` serializes and deserializes the new optional fields `local_country`, `local_region`, `local_city`, `local_language` without breaking existing schemas.
- **AC-2**: `ReverseGeoPlugin` offline lookup populates both English (`country`, `city`) and localized fields (`local_country`, `local_city`, `local_language`).
- **AC-3**: SQLite database persistence inserts and updates the new local columns in `locations`.
- **AC-4**: End-to-end media pipeline run stores dual-language fields for sample photos.
