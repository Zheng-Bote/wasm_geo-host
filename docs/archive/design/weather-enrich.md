---
feature: weather-enrich
status: archived
archive-after:
touches:
  - "crates/models/**"
  - "plugins/weather-enrich/**"
  - "crates/host/**"
  - "crates/host/migrations/**"
  - "Cargo.toml"
release: 0.2.0
---

# Tech Spec — weather-enrich (`weather-enrich`)

Setzt das PRD `docs/prd/weather-enrich.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern.

## Architektur & Komponenten

Das Plugin fügt sich in die Pipeline stromabwärts von `metadata`/`reversegeo` (GPS-Koordinaten) und `timeline` (`taken_at` Zeitstempel) ein:

```text
+-------------------------------------------------------------+
|                     flagme-host (Host)                      |
|  - Capabilities: KV-Store & HTTP Client                     |
|  - SQLite Storage Engine (weather Tabelle)                  |
+------------------------------+------------------------------+
                               |
                               v
+-------------------------------------------------------------+
|              flagme-plugin-weather-enrich                   |
|  1. Prüfe KV-Cache (weather:<lat_2dec>:<lon_2dec>:<hour>)   |
|  2. HTTP-Abfrage an Open-Meteo Historical Weather API       |
|  3. Mappe WMO-Codes auf Bedingung + Emoji                   |
|  4. Befülle UnifiedDocument.weather                         |
+-------------------------------------------------------------+
                               |
                               v
+-------------------------------------------------------------+
|                   SQLite (media.db)                         |
|  - Tabelle: weather                                         |
|    (photo_id, temperature, condition, wmo_code,             |
|     icon_emoji, humidity_percent, precipitation_mm, is_day) |
+-------------------------------------------------------------+
```

### 1. Datenmodell (AC-1)
In `crates/models/src/lib.rs`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WeatherInfo {
    pub temperature: Option<f32>,
    pub condition: String,
    pub wmo_code: u8,
    pub icon_emoji: String,
    pub humidity_percent: Option<u8>,
    pub precipitation_mm: Option<f32>,
    pub is_day: Option<bool>,
}
```
Erfüllt **AC-1**: Typisierte und serialisierbare Repräsentation.

### 2. Plugin-Architektur & WMO Mapping (AC-2, AC-3, AC-4)
In `plugins/weather-enrich/src/lib.rs`:
- Benötigt Capabilities `kv` und `http`.
- Cache-Key: `weather:<lat_2dec>:<lon_2dec>:<date_hour>`.
- WMO-Interpretationstabelle:
  - 0: "Clear Sky", ☀️
  - 1..=3: "Partly Cloudy", ⛅
  - 45, 48: "Foggy", 🌫️
  - 51..=67: "Rain", 🌧️
  - 71..=77: "Snow", ❄️
  - 80..=82: "Rain Showers", 🌦️
  - 95..=99: "Thunderstorm", ⛈️
Erfüllt **AC-2** (WASI Kompilierung), **AC-3** (KV Caching) und **AC-4** (WMO Mapping).

### 3. SQLite Schema & Persistenz (AC-5)
In `crates/host/migrations/20261125000000_init.sql`:
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
In `crates/host/src/storage.rs`:
- Persistierung der Felder beim Aufruf von `Storage::save_document`.
- Wiederherstellung in `Storage::get_document`.
Erfüllt **AC-5**: Relationale Speicherung und Indizierung.

### 4. Integration Verification (AC-6)
In `crates/host/tests/weather_test.rs`:
- Verifikation von Cache-Hit, Cache-Miss und korrekter Attributzuweisung.
Erfüllt **AC-6**: Vollständige Testabsicherung.
