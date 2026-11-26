use chrono::{TimeZone, Utc};
use flagme_host::Storage;
use flagme_plugin_weather_enrich::WeatherEnrichPlugin;
use flagme_sdk::{HostCapabilities, NoopCapabilities, Plugin, PluginContext, PluginResult};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wasm_geo_models::{FileInfo, LocationInfo, TimelineInfo, UnifiedDocument, WeatherInfo};

// weather-enrich AC-1: wasm-geo-models introduces WeatherInfo with complete serde serialization
#[test]
fn test_ac1_weather_info_serialization() {
    let weather = WeatherInfo {
        temperature: Some(22.5),
        condition: "Clear Sky".to_string(),
        wmo_code: 0,
        icon_emoji: "☀️".to_string(),
        humidity_percent: Some(55),
        precipitation_mm: Some(0.0),
        is_day: Some(true),
    };

    let serialized = serde_json::to_string(&weather).expect("Serialization failed");
    let deserialized: WeatherInfo = serde_json::from_str(&serialized).expect("Deserialization failed");

    assert_eq!(weather, deserialized);
    assert_eq!(deserialized.temperature, Some(22.5));
    assert_eq!(deserialized.condition, "Clear Sky");
    assert_eq!(deserialized.icon_emoji, "☀️");
}

// weather-enrich AC-2: flagme-plugin-weather-enrich compiles as plugin crate with valid metadata
#[test]
fn test_ac2_plugin_contract_metadata() {
    let plugin = WeatherEnrichPlugin::new();
    assert_eq!(plugin.name(), "weather-enrich");
    assert_eq!(plugin.version(), env!("CARGO_PKG_VERSION"), "all crates share the workspace version");
}

// Mock capabilities for KV caching and HTTP observation
/// Open-Meteo archive response for one day (24 hourly values; 12:00 = 18.5 °C, code 2).
fn open_meteo_fixture() -> String {
    let temps: Vec<String> = (0..24).map(|h| if h == 12 { "18.5".to_string() } else { "10.0".to_string() }).collect();
    let codes: Vec<String> = (0..24).map(|h| if h == 12 { "2".to_string() } else { "0".to_string() }).collect();
    format!(
        r#"{{"hourly":{{"time":[],"temperature_2m":[{}],"weather_code":[{}],"relative_humidity_2m":[{}],"precipitation":[{}],"is_day":[{}]}}}}"#,
        temps.join(","),
        codes.join(","),
        vec!["60"; 24].join(","),
        vec!["0.0"; 24].join(","),
        vec!["1"; 24].join(",")
    )
}

struct MockKvCapabilities {
    kv_store: Mutex<HashMap<String, String>>,
    http_calls: Mutex<Vec<String>>,
}

impl MockKvCapabilities {
    fn new() -> Self {
        Self {
            kv_store: Mutex::new(HashMap::new()),
            http_calls: Mutex::new(Vec::new()),
        }
    }
}

impl HostCapabilities for MockKvCapabilities {
    fn fs_read(&self, _path: &str) -> PluginResult<Vec<u8>> {
        Ok(Vec::new())
    }
    fn secret_get(&self, _name: &str) -> PluginResult<Option<String>> {
        Ok(None)
    }
    fn log(&self, _level: &str, _message: &str) {}
    fn http_get(&self, url: &str) -> PluginResult<String> {
        self.http_calls.lock().unwrap().push(url.to_string());
        Ok(open_meteo_fixture())
    }
    fn kv_get(&self, key: &str) -> PluginResult<Option<String>> {
        Ok(self.kv_store.lock().unwrap().get(key).cloned())
    }
    fn kv_set(&self, key: &str, value: &str) -> PluginResult<()> {
        self.kv_store.lock().unwrap().insert(key.to_string(), value.to_string());
        Ok(())
    }
}

// weather-enrich AC-3: Plugin checks the host KV cache before issuing outbound HTTP requests
#[test]
fn test_ac3_kv_caching_and_http_avoidance() {
    let plugin = WeatherEnrichPlugin::new();
    let caps = Arc::new(MockKvCapabilities::new());
    let ctx = PluginContext::new(caps.clone());

    let mut doc = UnifiedDocument::new(
        "photo-w1",
        FileInfo {
            path: "/media/photo1.jpg".to_string(),
            filename: "photo1.jpg".to_string(),
            size_bytes: 1024,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    doc.location = Some(LocationInfo {
        latitude: Some(52.52),
        longitude: Some(13.405),
        ..Default::default()
    });
    doc.timeline = Some(TimelineInfo {
        taken_at: Some(Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap()),
        imported_at: Utc::now(),
        modified_at: None,
        resolution_source: "exif".to_string(),
    });

    // 1. First run: cache is empty, HTTP should be called and KV set
    let res1 = plugin.process(doc.clone(), &ctx).expect("Process 1 failed");
    assert_eq!(res1.weather.as_ref().and_then(|w| w.temperature), Some(18.5));
    assert_eq!(caps.http_calls.lock().unwrap().len(), 1);

    // 2. Second run: cache is present, HTTP count should remain 1
    let res2 = plugin.process(doc, &ctx).expect("Process 2 failed");
    assert!(res2.weather.is_some());
    assert_eq!(caps.http_calls.lock().unwrap().len(), 1, "HTTP must not be called when KV cache hits");
}

// weather-enrich AC-4: Open-Meteo response maps standard WMO codes to human-readable condition strings and emoji icons
#[test]
fn test_ac4_wmo_code_mapping() {
    assert_eq!(WeatherEnrichPlugin::map_wmo_code(0), ("Clear Sky", "☀️"));
    assert_eq!(WeatherEnrichPlugin::map_wmo_code(2), ("Partly Cloudy", "⛅"));
    assert_eq!(WeatherEnrichPlugin::map_wmo_code(45), ("Foggy", "🌫️"));
    assert_eq!(WeatherEnrichPlugin::map_wmo_code(61), ("Rain", "🌧️"));
    assert_eq!(WeatherEnrichPlugin::map_wmo_code(71), ("Snow Fall", "❄️"));
    assert_eq!(WeatherEnrichPlugin::map_wmo_code(95), ("Thunderstorm", "⛈️"));
}

// weather-enrich AC-5: SQLite persistence stores weather metrics in the weather table
#[test]
fn test_ac5_sqlite_weather_persistence() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join(format!("test_weather_ac5_{}.db", std::process::id()));
    let db_str = db_path.to_str().unwrap();

    let storage = Storage::new(db_str);
    storage.init_schema().expect("Schema init failed");

    let mut doc = UnifiedDocument::new(
        "photo-w-sql",
        FileInfo {
            path: "/media/winter.jpg".to_string(),
            filename: "winter.jpg".to_string(),
            size_bytes: 2048,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );

    doc.weather = Some(WeatherInfo {
        temperature: Some(-4.0),
        condition: "Snow Fall".to_string(),
        wmo_code: 71,
        icon_emoji: "❄️".to_string(),
        humidity_percent: Some(85),
        precipitation_mm: Some(5.2),
        is_day: Some(false),
    });

    storage.save_document(&doc).expect("Save document failed");

    // Retrieve via Storage::get_document
    let loaded = storage.get_document("photo-w-sql").expect("Photo not found");
    assert!(loaded.weather.is_some());
    let w = loaded.weather.unwrap();
    assert_eq!(w.condition, "Snow Fall");
    assert_eq!(w.wmo_code, 71);
    assert_eq!(w.icon_emoji, "❄️");
    assert_eq!(w.temperature, Some(-4.0));
    assert_eq!(w.is_day, Some(false));

    let _ = std::fs::remove_file(&db_path);
}

// weather-enrich AC-6: Integration tests verify caching and correct resolution with mock coordinate/timestamp fixtures
#[test]
fn test_ac6_end_to_end_weather_enrichment() {
    let plugin = WeatherEnrichPlugin::new();
    let caps = Arc::new(MockKvCapabilities::new());
    let ctx = PluginContext::new(caps.clone());

    let mut doc = UnifiedDocument::new(
        "photo-e2e-w",
        FileInfo {
            path: "/media/photo.jpg".to_string(),
            filename: "photo.jpg".to_string(),
            size_bytes: 1024,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    doc.location = Some(LocationInfo {
        latitude: Some(35.68),
        longitude: Some(139.76),
        ..Default::default()
    });
    doc.timeline = Some(TimelineInfo {
        taken_at: Some(Utc.with_ymd_and_hms(2025, 4, 1, 12, 0, 0).unwrap()),
        imported_at: Utc::now(),
        modified_at: None,
        resolution_source: "exif".to_string(),
    });

    let enriched = plugin.process(doc, &ctx).expect("Enrichment failed");
    let w = enriched.weather.expect("measured weather from the response");
    assert_eq!(w.temperature, Some(18.5));
    assert_eq!((w.wmo_code, w.condition.as_str(), w.icon_emoji.as_str()), (2, "Partly Cloudy", "⛅"));
    assert_eq!(w.humidity_percent, Some(60));
    let calls = caps.http_calls.lock().unwrap();
    assert!(calls[0].contains("latitude=35.68&longitude=139.76&start_date=2025-04-01"), "{}", calls[0]);
    assert!(caps.kv_store.lock().unwrap().contains_key("weather:35.68:139.76:2025-04-01-12"));

    // Without coordinates or HTTP there is no weather (ADR 0002, no estimate).
    let offline = WeatherEnrichPlugin::new()
        .process(enriched_without_weather(), &PluginContext::new(Arc::new(NoopCapabilities)))
        .unwrap();
    assert!(offline.weather.is_none());
}

fn enriched_without_weather() -> UnifiedDocument {
    let mut doc = UnifiedDocument::new(
        "photo-offline",
        FileInfo {
            path: "/media/offline.jpg".to_string(),
            filename: "offline.jpg".to_string(),
            size_bytes: 1,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    doc.location = Some(LocationInfo { latitude: Some(35.68), longitude: Some(139.76), ..Default::default() });
    doc.timeline = Some(TimelineInfo {
        taken_at: Some(Utc.with_ymd_and_hms(2025, 4, 1, 12, 0, 0).unwrap()),
        imported_at: Utc::now(),
        modified_at: None,
        resolution_source: "exif".to_string(),
    });
    doc
}
