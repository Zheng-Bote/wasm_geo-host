# Weather Enrichment Plugin (`flagme-plugin-weather-enrich`)

## Overview

The `weather-enrich` plugin adds the historical weather at the place and hour a photo was taken: temperature, WMO weather code with label and emoji, humidity, precipitation and day/night. Data source is the free Open-Meteo Historical Weather API (ERA5 reanalysis). Without network access it uses a climatological estimate.

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `weather-enrich` |
| Crate | `flagme-plugin-weather-enrich` (`cdylib` + `rlib` + `[[bin]]`) |
| Component | `flagme_plugin_weather_enrich.wasm` — `cargo build --release --target wasm32-wasip2 --lib --features component -p flagme-plugin-weather-enrich`; loaded by `flagme-host` from `plugin_dir` ([ADR 0003](../../docs/adr/0003-wasm-component-execution.md)) |
| Version | `0.1.0` |
| Reads | `doc.location.latitude/longitude`, `doc.timeline.taken_at` |
| Writes | `doc.weather` (`WeatherInfo`) |
| Capabilities | `http`, `kv` |
| Configurable via `settings` | **Yes** (3 parameters, see §2.2) |

---

## 1. Key Features

1. **Historical Hourly Weather** — Open-Meteo archive, value of the capture hour (UTC).
2. **WMO Code Mapping** — codes 0–99 → label + emoji (table in §2.4).
3. **KV Caching** — per coordinate (`coordinate_precision`, default 2 decimals) and hour.
4. **Measured data only** — if no measured value is available, `doc.weather` stays empty and a warning is logged; there is no estimate ([ADR 0002](../../docs/adr/0002-plugins-without-engine-no-output.md)).
5. **Dual-Mode Execution** — Wasm component (`media-plugin` world, feature `component`) run in the sandbox of `flagme-host`, or standalone CLI (`stdin` JSON → `stdout` JSON).

---

## 2. Configuration & Parameters

### 2.1 Plugin Entry (`config.yaml`)

```yaml
plugins:
  weather-enrich:
    enabled: true
    capabilities:
      - http     # archive-api.open-meteo.com
      - kv       # cache
```

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | bool | `true` | `false` → no `doc.weather`. |
| `capabilities` | list | `[http, kv]` | See §4. |
| `settings` | map | `{}` | See §2.2. |

### 2.2 `settings`

| Key | Type / range | Default | Effect |
| :--- | :--- | :--- | :--- |
| `endpoint` | `http(s)://` URL | `https://archive-api.open-meteo.com/v1/archive` | Base URL of the archive API (e.g. an own Open-Meteo instance). |
| `coordinate_precision` | integer 0–4 | `2` | Decimal places of the coordinates in the request and the cache key. |
| `offline_fallback` | any | — | **Not supported** (the climatological estimate was removed, [#40](https://github.com/Zheng-Bote/wasm_geo-host/issues/40)) → warning, ignored. |

```yaml
    settings:
      coordinate_precision: "2"
```

All values are strings and follow the shared SDK rules (`flagme_sdk::settings`): booleans accept `true`/`yes`/`on`/`1` and `false`/`no`/`off`/`0`; numbers outside the range are clamped with a warning; invalid values and unknown keys produce a `warn` log entry `settings[<plugin>]: …` and the default applies. Settings are parsed once per configuration.

### 2.3 Built-in Parameters (not configurable)

| Parameter | Value | Effect |
| :--- | :--- | :--- |
| Query | `<endpoint>?latitude=…&longitude=…` (`coordinate_precision` decimals), `start_date` = `end_date` = capture date, `hourly=temperature_2m,relative_humidity_2m,precipitation,weather_code,is_day` | one request per photo and day |
| Cache key | `weather:<lat>:<lon>:<YYYY-MM-DD-HH>` | coordinates with `coordinate_precision` decimals |
| Hour selection | hour of `taken_at` (0–23), index into the `hourly` arrays | |
| Missing `taken_at` | no request; warning `weather-enrich: no capture time for '<path>'; weather not queried` | |
| HTTP timeout | 2 s (host capability) | |
| Units | °C, %, mm | Open-Meteo defaults |

### 2.4 WMO Code Mapping

| WMO code | `condition` | `icon_emoji` |
| :--- | :--- | :---: |
| 0 | Clear Sky | ☀️ |
| 1 | Mainly Clear | 🌤️ |
| 2 | Partly Cloudy | ⛅ |
| 3 | Overcast | ☁️ |
| 45, 48 | Foggy | 🌫️ |
| 51, 53, 55 | Drizzle | 🌦️ |
| 56, 57 | Freezing Drizzle | 🌧️ |
| 61, 63, 65 | Rain | 🌧️ |
| 66, 67 | Freezing Rain | 🌧️ |
| 71, 73, 75 | Snow Fall | ❄️ |
| 77 | Snow Grains | ❄️ |
| 80, 81, 82 | Rain Showers | 🌦️ |
| 85, 86 | Snow Showers | 🌨️ |
| 95 | Thunderstorm | ⛈️ |
| 96, 99 | Thunderstorm with Hail | ⛈️ |
| other | Variable Weather | 🌡️ |

### 2.5 No Measurement

If the HTTP request fails or the response cannot be parsed, the plugin logs `weather-enrich: no measured weather for '<path>': …`, leaves `doc.weather` empty and caches nothing.

---

## 3. Pipeline Dependencies & Ordering

1. **`metadata`** (before) → coordinates. Without `latitude` **and** `longitude` the plugin logs `No GPS coordinates…` and returns the document unchanged.
2. **`timeline`** (before) → `taken_at`.
3. **`weather-enrich`**.
4. **`privacy-redact`** (after) → otherwise the weather would be queried for rounded coordinates.

---

## 4. Required Capabilities

| Capability | Used for | Without the grant |
| :--- | :--- | :--- |
| `http` | Open-Meteo request | Warning, `doc.weather` stays empty (unless the KV cache has a measured value). |
| `kv` | cache | Every photo triggers a new request. |

---

## 5. Data Model Output (`WeatherInfo`)

```rust
pub struct WeatherInfo {
    pub temperature: Option<f32>,       // °C
    pub condition: String,              // "Clear Sky"
    pub wmo_code: u8,                   // 0
    pub icon_emoji: String,             // "☀️"
    pub humidity_percent: Option<u8>,   // 0..100
    pub precipitation_mm: Option<f32>,
    pub is_day: Option<bool>,
}
```

```json
{
  "temperature": 21.5,
  "condition": "Clear Sky",
  "wmo_code": 0,
  "icon_emoji": "☀️",
  "humidity_percent": 60,
  "precipitation_mm": 0.0,
  "is_day": true
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
cat doc_with_location_and_timeline.json | wasmtime run -S cli-exit-with-code \
  target/wasm32-wasip2/release/flagme-plugin-weather-enrich.wasm
```

The CLI has no HTTP capability → `doc.weather` stays empty. Empty input prints `Plugin weather-enrich v0.1.0`.

### 6.3 SQLite

```sql
SELECT photo_id, temperature, condition, wmo_code, icon_emoji, humidity_percent, precipitation_mm, is_day FROM weather;
```

---

## 7. Limitations & Implementation Notes

- **Only measured values.** The former climatological fallback (estimated values that looked like measurements) and the fixed substitute date for photos without capture time were removed ([#40](https://github.com/Zheng-Bote/wasm_geo-host/issues/40), [ADR 0002](../../docs/adr/0002-plugins-without-engine-no-output.md)).
- Because `timeline` stores local times as UTC (see its README), the selected hour can be off by the time-zone offset.
- The ERA5 archive has a delay of a few days; very recent photos may get no data (warning, no weather).
- The KV cache lives in memory of the host process.
