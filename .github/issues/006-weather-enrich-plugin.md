# Feature Spec: Historical Weather Enrichment Plugin (`flagme-plugin-weather-enrich`)

- **Feature Name**: `weather-enrich`
- **Component**: `plugins/weather-enrich`, `crates/models`, `crates/host`
- **Issue Type**: New Feature / Plugin Proposal

---

## 1. Problem Statement & Motivation
Photographs tell richer stories when contextualized with environmental conditions at the moment of capture:
- Was it sunny, raining, foggy, or snowing?
- What was the ambient temperature and cloud coverage?
- Did the photo capture the golden hour or a dramatic thunderstorm?

While modern cameras record precise timestamps (`taken_at`) and GPS coordinates (`latitude`, `longitude`), they do not record ambient meteorological conditions. The `weather-enrich` plugin queries historical reanalysis weather archives to enrich photos with atmospheric conditions at the exact time and location of exposure.

---

## 2. Technical Architecture & Data Sources

The plugin executes downstream of `timeline` (which provides normalized UTC `taken_at`) and `metadata` / `reversegeo` (which provide GPS coordinates):

```
[timeline: taken_at] + [metadata/reversegeo: GPS] 
                        │
                        ▼
       [flagme-plugin-weather-enrich]
         ├── 1. Check local KV cache (key: weather:<lat_2dec>:<lon_2dec>:<date_hour>)
         └── 2. Outbound HTTP Query (Open-Meteo Historical Weather API)
                 ├── Temperature (°C)
                 ├── WMO Weather Condition Code (Sunny, Overcast, Rain, Snow)
                 ├── Precipitation & Relative Humidity
                 └── Solar Position / Day-Night Phase
                        │
                        ▼
             [UnifiedDocument.weather]
```

### Free & Open API Provider
- **Open-Meteo Historical Weather API**: Free for non-commercial and open-source use, requires **no API key**, based on national meteorological reanalysis datasets (ERA5, DWD ICON, NOAA GFS).

---

## 3. Configuration Specification (`config.yaml`)

```yaml
plugins:
  weather-enrich:
    enabled: true
    capabilities:
      - http
      - kv
    settings:
      provider: "open-meteo"            # Default open provider
      temperature_unit: "celsius"       # "celsius" | "fahrenheit"
      cache_ttl_seconds: "2592000"      # 30 days (historical weather is immutable)
      skip_if_older_than_year: "1940"   # Reanalysis data availability boundary
```

---

## 4. Data Model Output (`wasm-geo-models`)

Add `WeatherInfo` to `UnifiedDocument`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WeatherInfo {
    pub temperature: Option<f32>,       // e.g. 21.5 °C
    pub condition: String,              // e.g. "Clear Sky", "Moderate Rain"
    pub wmo_code: u8,                   // Standard WMO weather interpretation code (0-99)
    pub icon_emoji: String,             // e.g. "☀️", "🌧️", "⛅", "❄️"
    pub humidity_percent: Option<u8>,   // e.g. 65%
    pub precipitation_mm: Option<f32>,  // e.g. 0.0 mm
    pub is_day: Option<bool>,           // Day/Night indicator
}
```

---

## 5. SQLite Persistence (`media.db`)

Add relational persistence for photo meteorological conditions:

```sql
CREATE TABLE IF NOT EXISTS weather (
    photo_id TEXT PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
    temperature REAL,
    condition TEXT NOT NULL,
    wmo_code INTEGER NOT NULL,
    icon_emoji TEXT NOT NULL,
    humidity_percent INTEGER,
    precipitation_mm REAL,
    is_day INTEGER
);

CREATE INDEX IF NOT EXISTS idx_weather_condition ON weather(condition);
CREATE INDEX IF NOT EXISTS idx_weather_temp ON weather(temperature);
```

### Example Search Queries
- Find all photos taken in the snow:
  ```sql
  SELECT p.* FROM photos p JOIN weather w ON p.id = w.photo_id WHERE w.condition LIKE '%Snow%';
  ```
- Find photos taken above 30°C:
  ```sql
  SELECT p.* FROM photos p JOIN weather w ON p.id = w.photo_id WHERE w.temperature > 30.0;
  ```

---

## 6. Acceptance Criteria
- [ ] **AC-1**: `wasm-geo-models` introduces `WeatherInfo` with complete serde serialization.
- [ ] **AC-2**: `flagme-plugin-weather-enrich` compiles to both `cdylib` and standalone `[[bin]]` under `wasm32-wasip2`.
- [ ] **AC-3**: Plugin checks the host KV cache before issuing outbound HTTP requests.
- [ ] **AC-4**: Open-Meteo response maps standard WMO codes to human-readable condition strings and emoji icons.
- [ ] **AC-5**: SQLite persistence stores weather metrics in the `weather` table.
- [ ] **AC-6**: Integration tests verify caching and correct resolution with mock coordinate/timestamp fixtures.
