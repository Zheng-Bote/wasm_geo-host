mod common;

use chrono::{TimeZone, Utc};
use flagme_host::CapabilityManager;
use flagme_plugin_reversegeo::ReverseGeoPlugin;
use flagme_plugin_weather_enrich::WeatherEnrichPlugin;
use flagme_sdk::*;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex, RwLock};

/// Minimal local HTTP server: `/ok` → 200 with a body, `/missing` → 404, `/broken` → 500,
/// `/empty` → 200 without body, `/garbage` → 200 with a non-JSON body. Returns the base URL.
fn serve() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request_line = String::new();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let _ = reader.read_line(&mut request_line);
            let mut line = String::new();
            while reader.read_line(&mut line).map(|n| n > 2).unwrap_or(false) {
                line.clear();
            }
            let path = request_line.split_whitespace().nth(1).unwrap_or("/");
            let (status, body) = match path.split('?').next().unwrap_or("/") {
                "/ok" => ("200 OK", "real body"),
                "/missing" => ("404 Not Found", "no such page"),
                "/broken" => ("500 Internal Server Error", "server error"),
                "/empty" => ("200 OK", ""),
                _ => ("200 OK", "<html>not json</html>"),
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status,
                body.len(),
                body
            );
        }
    });
    base
}

/// URL of a local port nobody listens on.
fn unreachable_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    format!("http://{}/nothing", addr)
}

fn manager() -> CapabilityManager {
    CapabilityManager::new("test", vec!["http".to_string(), "kv".to_string()], Arc::new(RwLock::new(HashMap::new())))
}

/// The real `CapabilityManager` for HTTP and KV, recording warnings and KV writes.
struct Recording {
    inner: CapabilityManager,
    warnings: Mutex<Vec<String>>,
    kv_writes: Mutex<Vec<String>>,
}

impl Recording {
    fn new() -> Self {
        Self { inner: manager(), warnings: Mutex::default(), kv_writes: Mutex::default() }
    }
}

impl HostCapabilities for Recording {
    fn fs_read(&self, path: &str) -> PluginResult<Vec<u8>> {
        self.inner.fs_read(path)
    }
    fn http_get(&self, url: &str) -> PluginResult<String> {
        self.inner.http_get(url)
    }
    fn kv_get(&self, key: &str) -> PluginResult<Option<String>> {
        self.inner.kv_get(key)
    }
    fn kv_set(&self, key: &str, value: &str) -> PluginResult<()> {
        self.kv_writes.lock().unwrap().push(key.to_string());
        self.inner.kv_set(key, value)
    }
    fn secret_get(&self, name: &str) -> PluginResult<Option<String>> {
        self.inner.secret_get(name)
    }
    fn log(&self, level: &str, message: &str) {
        if level == "warn" {
            self.warnings.lock().unwrap().push(message.to_string());
        }
    }
}

fn ctx(caps: &Arc<Recording>, settings: &[(&str, &str)]) -> PluginContext {
    PluginContext::with_settings(
        caps.clone(),
        settings.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
    )
}

fn located(lat: f64, lon: f64) -> UnifiedDocument {
    let mut doc = UnifiedDocument::new(
        "http",
        FileInfo {
            path: "/media/photo.jpg".to_string(),
            filename: "photo.jpg".to_string(),
            size_bytes: 1,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    doc.location = Some(LocationInfo { latitude: Some(lat), longitude: Some(lon), ..Default::default() });
    doc.timeline = Some(TimelineInfo {
        taken_at: Some(Utc.with_ymd_and_hms(2025, 7, 1, 12, 0, 0).unwrap()),
        imported_at: Utc::now(),
        modified_at: None,
        resolution_source: "exif_datetime_original".to_string(),
    });
    doc
}

// honest-http-get AC-1: an unreachable server makes http_get fail without a response body
#[test]
fn test_ac1_unreachable_server_is_an_error() {
    let url = unreachable_url();
    let err = manager().http_get(&url).unwrap_err();
    assert!(matches!(&err, PluginError::Io(m) if m.contains(&url) && m.contains("curl exit")), "{:?}", err);
    assert!(!err.to_string().contains("offline"), "{}", err);
}

// honest-http-get AC-2: HTTP 404 and 500 fail with the status named, an empty response fails, 200 returns the real body
#[test]
fn test_ac2_http_status_and_empty_body() {
    let base = serve();
    assert_eq!(manager().http_get(&format!("{}/ok", base)).unwrap(), "real body");
    for (path, status) in [("/missing", "404"), ("/broken", "500")] {
        let err = manager().http_get(&format!("{}{}", base, path)).unwrap_err();
        assert!(matches!(&err, PluginError::Io(m) if m.contains(status)), "{}: {:?}", path, err);
    }
    let err = manager().http_get(&format!("{}/empty", base)).unwrap_err();
    assert!(matches!(&err, PluginError::Io(m) if m.contains("empty response")), "{:?}", err);
}

// honest-http-get AC-3: when http_get fails, weather-enrich and reversegeo leave their fields empty, warn with the cause and cache nothing
#[test]
fn test_ac3_plugins_on_http_failure() {
    let base = serve();
    for endpoint in [unreachable_url(), format!("{}/broken", base)] {
        let caps = Arc::new(Recording::new());
        let out = WeatherEnrichPlugin::new().process(located(52.52, 13.40), &ctx(&caps, &[("endpoint", &endpoint)])).unwrap();
        assert!(out.weather.is_none());
        let warnings = caps.warnings.lock().unwrap().clone();
        assert!(warnings.iter().any(|w| w.contains("no measured weather") && w.contains("curl exit")), "{:?}", warnings);
        assert!(caps.kv_writes.lock().unwrap().is_empty(), "weather cached after a failed request");

        // (0, 0) lies outside the offline data, so reversegeo asks the online geocoder.
        let caps = Arc::new(Recording::new());
        let out = ReverseGeoPlugin::new().process(located(0.0, 0.0), &ctx(&caps, &[("nominatim_url", &endpoint)])).unwrap();
        let loc = out.location.unwrap();
        assert_eq!((loc.latitude, loc.longitude), (Some(0.0), Some(0.0)));
        assert!(loc.country.is_none() && loc.country_code.is_none() && loc.city.is_none(), "{:?}", loc);
        let warnings = caps.warnings.lock().unwrap().clone();
        assert!(warnings.iter().any(|w| w.contains("reversegeo: no location names") && w.contains("curl exit")), "{:?}", warnings);
        assert!(caps.kv_writes.lock().unwrap().is_empty(), "location cached after a failed request");
    }
}

/// Body of a function in a source file, from its `fn` line to the closing brace at the same indent.
fn function_body(source: &str, signature: &str) -> String {
    let start = source.find(signature).unwrap_or_else(|| panic!("{} not found", signature));
    let indent = source[..start].rsplit('\n').next().unwrap().len();
    let closing = format!("\n{}}}", " ".repeat(indent));
    let end = source[start..].find(&closing).unwrap() + start;
    source[start..end].to_string()
}

// honest-http-get AC-4: no code path of http_get builds a response body; reversegeo has no placeholder location
#[test]
fn test_ac4_no_fabricated_responses_in_source() {
    let root = common::repo_root();
    let caps = std::fs::read_to_string(root.join("crates/host/src/capabilities.rs")).unwrap();
    let body = function_body(&caps, "fn http_get(");
    let returns: Vec<&str> = body.lines().filter(|l| l.contains("Ok(")).map(str::trim).collect();
    assert_eq!(returns, vec!["Ok(body)"], "http_get may only return the received body");
    assert!(!body.contains("offline"), "{}", body);

    let geo = std::fs::read_to_string(root.join("plugins/reversegeo/src/lib.rs")).unwrap();
    for placeholder in ["Unknown Country", "\"XX\"", "\"Global\""] {
        assert!(!geo.contains(placeholder), "reversegeo still contains {}", placeholder);
    }
}

// honest-http-get AC-5: unresolved coordinates (online_lookup off or unreadable answer) keep empty names, keep the coordinates, warn and are not cached
#[test]
fn test_ac5_reversegeo_unresolved_coordinates() {
    let base = serve();
    let cases = [
        (vec![("online_lookup", "off".to_string())], "online_lookup is off"),
        (vec![("nominatim_url", format!("{}/garbage", base))], "unreadable geocoder response"),
    ];
    for (settings, cause) in cases {
        let settings: Vec<(&str, &str)> = settings.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let caps = Arc::new(Recording::new());
        let out = ReverseGeoPlugin::new().process(located(0.0, 0.0), &ctx(&caps, &settings)).unwrap();
        let loc = out.location.unwrap();
        assert_eq!((loc.latitude, loc.longitude), (Some(0.0), Some(0.0)));
        assert!(loc.country.is_none() && loc.region.is_none() && loc.local_language.is_none(), "{:?}", loc);
        let warnings = caps.warnings.lock().unwrap().clone();
        assert!(warnings.iter().any(|w| w.contains("reversegeo: no location names") && w.contains(cause)), "{:?}", warnings);
        assert!(caps.kv_writes.lock().unwrap().is_empty());
    }
}
