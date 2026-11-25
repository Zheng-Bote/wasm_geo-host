# Weather Enrichment Plugin (`flagme-plugin-weather-enrich`)

The **Weather Enrichment Plugin** enriches media items with historical environmental and meteorological conditions at the exact time and location of exposure.

---

## 1. Features
- **Historical Weather Lookup**: Integrates with the free and open Open-Meteo Historical Weather API (ERA5 reanalysis).
- **Standard WMO Codes**: Maps World Meteorological Organization weather interpretation codes (0-99) to human-readable labels and emoji icons (e.g., `Clear Sky ☀️`, `Snow Fall ❄️`, `Thunderstorm ⛈️`).
- **Capability-Based Caching**: Caches meteorological observations in the host KV-store using coordinate-hour keys (`weather:<lat>:<lon>:<date_hour>`) to minimize network bandwidth.
- **WASI 0.2 Component**: Compiles to standard WebAssembly targeting `wasm32-wasip2`.

---

## 2. Capabilities Required
- `kv`: Key-value storage for observation caching.
- `http`: Outbound HTTP access to `archive-api.open-meteo.com`.

---

## 3. Data Output (`UnifiedDocument.weather`)
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
