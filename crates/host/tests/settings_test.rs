use chrono::{TimeZone, Utc};
use flagme_host::AppConfig;
use flagme_plugin_ai_tagging::{AiTaggingPlugin, AiTaggingSettings};
use flagme_plugin_countryflag::CountryFlagSettings;
use flagme_plugin_duplicate::{DuplicatePlugin, DuplicateSettings, PHASH_INDEX_KEY};
use flagme_plugin_face_detection::{FaceDetectionPlugin, FaceDetectionSettings, FaceDetector, ModelTier};
use flagme_plugin_metadata::{MetadataPlugin, MetadataSettings};
use flagme_plugin_ocr_text::{OcrEngine, OcrSettings, OcrTextPlugin};
use flagme_plugin_privacy_redact::{PrivacyRedactPlugin, PrivacySettings};
use flagme_plugin_reversegeo::{ReverseGeoPlugin, ReverseGeoSettings};
use flagme_plugin_thumbnail::{ThumbnailPlugin, ThumbnailSettings};
use flagme_plugin_timeline::{TimelinePlugin, TimelineSettings};
use flagme_plugin_weather_enrich::{WeatherEnrichPlugin, WeatherSettings};
use flagme_sdk::{
    BoundingBox, DetectedFace, FacesInfo, FileInfo, HostCapabilities, LocationInfo, MetadataInfo,
    OcrInfo, Plugin, PluginContext, PluginError, PluginResult, PluginSettings, TextRegion,
    TimelineInfo, UnifiedDocument,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Test double: records warnings, HTTP URLs and KV reads; serves canned HTTP bodies.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct RecordingCaps {
    warnings: Mutex<Vec<String>>,
    http_urls: Mutex<Vec<String>>,
    http_body: Mutex<Option<String>>,
    kv: Mutex<HashMap<String, String>>,
    kv_reads: Mutex<Vec<String>>,
}

impl RecordingCaps {
    fn with_http(body: &str) -> Self {
        let caps = Self::default();
        *caps.http_body.lock().unwrap() = Some(body.to_string());
        caps
    }
    fn warnings(&self) -> Vec<String> {
        self.warnings.lock().unwrap().clone()
    }
    fn urls(&self) -> Vec<String> {
        self.http_urls.lock().unwrap().clone()
    }
    fn kv_reads(&self) -> Vec<String> {
        self.kv_reads.lock().unwrap().clone()
    }
    fn kv_keys(&self) -> Vec<String> {
        self.kv.lock().unwrap().keys().cloned().collect()
    }
}

impl HostCapabilities for RecordingCaps {
    fn fs_read(&self, path: &str) -> PluginResult<Vec<u8>> {
        std::fs::read(path).map_err(|e| PluginError::Io(e.to_string()))
    }
    fn fs_write(&self, path: &str, data: &[u8]) -> PluginResult<()> {
        if let Some(parent) = Path::new(path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, data).map_err(|e| PluginError::Io(e.to_string()))
    }
    fn http_get(&self, url: &str) -> PluginResult<String> {
        self.http_urls.lock().unwrap().push(url.to_string());
        self.http_body
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| PluginError::CapabilityDenied("http".into()))
    }
    fn kv_get(&self, key: &str) -> PluginResult<Option<String>> {
        self.kv_reads.lock().unwrap().push(key.to_string());
        Ok(self.kv.lock().unwrap().get(key).cloned())
    }
    fn kv_set(&self, key: &str, value: &str) -> PluginResult<()> {
        self.kv.lock().unwrap().insert(key.to_string(), value.to_string());
        Ok(())
    }
    fn secret_get(&self, _name: &str) -> PluginResult<Option<String>> {
        Ok(None)
    }
    fn log(&self, level: &str, message: &str) {
        if level == "warn" {
            self.warnings.lock().unwrap().push(message.to_string());
        }
    }
}

fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn ctx(caps: &Arc<RecordingCaps>, pairs: &[(&str, &str)]) -> PluginContext {
    PluginContext::with_settings(caps.clone(), map(pairs))
}

fn doc(path: &str) -> UnifiedDocument {
    let filename = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    UnifiedDocument::new(
        "doc-1",
        FileInfo {
            path: path.to_string(),
            filename,
            size_bytes: 0,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    )
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("plugin-settings-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> String {
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p.to_string_lossy().to_string()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn with_location(mut d: UnifiedDocument, lat: f64, lon: f64) -> UnifiedDocument {
    d.location = Some(LocationInfo {
        latitude: Some(lat),
        longitude: Some(lon),
        ..LocationInfo::default()
    });
    d
}

const NOMINATIM_MUNICH: &str = r#"{"address":{"country":"Deutschland","country_code":"de","state":"Bayern","city":"München"}}"#;

/// Minimal JPEG with an EXIF MakerNote of `len` bytes.
fn jpeg_with_makernote(len: usize) -> Vec<u8> {
    let mut tiff: Vec<u8> = b"II\x2a\x00".to_vec();
    tiff.extend(8u32.to_le_bytes());
    // IFD0: one entry ExifIFDPointer -> 26
    tiff.extend(1u16.to_le_bytes());
    tiff.extend(0x8769u16.to_le_bytes());
    tiff.extend(4u16.to_le_bytes());
    tiff.extend(1u32.to_le_bytes());
    tiff.extend(26u32.to_le_bytes());
    tiff.extend(0u32.to_le_bytes());
    // Exif IFD at 26: one entry MakerNote (UNDEFINED) -> data at 44
    tiff.extend(1u16.to_le_bytes());
    tiff.extend(0x927Cu16.to_le_bytes());
    tiff.extend(7u16.to_le_bytes());
    tiff.extend((len as u32).to_le_bytes());
    tiff.extend(44u32.to_le_bytes());
    tiff.extend(0u32.to_le_bytes());
    tiff.extend((0..len).map(|i| (i % 251) as u8));

    let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
    out.extend(((2 + 6 + tiff.len()) as u16).to_be_bytes());
    out.extend(b"Exif\0\0");
    out.extend(tiff);
    out.extend([0xFF, 0xD9]);
    out
}

fn xmp_file(value_len: usize) -> Vec<u8> {
    let value = "A".repeat(value_len);
    format!(
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF><rdf:Description dc:longvalue=\"{}\"/></rdf:RDF></x:xmpmeta>",
        value
    )
    .into_bytes()
}

// ---------------------------------------------------------------------------
// AC-2
// ---------------------------------------------------------------------------

// plugin-settings AC-2: privacy-redact follows the shared bool rule (yes/off) and warns on other values
#[test]
fn bool_rule_in_privacy_redact() {
    let plugin = PrivacyRedactPlugin::new();
    let caps = Arc::new(RecordingCaps::default());

    let mut d = with_location(doc("missing.jpg"), 52.518624, 13.376198);
    d.metadata = Some(MetadataInfo {
        raw_tags: map(&[("CameraSerialNumber", "123")]),
        ..MetadataInfo::default()
    });

    let out = plugin
        .process(d.clone(), &ctx(&caps, &[("fuzz_gps", "yes"), ("strip_serials", "off")]))
        .unwrap();
    assert!(out.privacy.as_ref().unwrap().original_gps_fuzzed);
    assert!(out.metadata.as_ref().unwrap().raw_tags.contains_key("CameraSerialNumber"));
    assert!(caps.warnings().is_empty());

    let out = plugin.process(d, &ctx(&caps, &[("fuzz_gps", "maybe")])).unwrap();
    assert!(out.privacy.as_ref().unwrap().original_gps_fuzzed, "invalid value keeps default true");
    assert_eq!(caps.warnings().len(), 1);
}

// ---------------------------------------------------------------------------
// AC-3
// ---------------------------------------------------------------------------

// plugin-settings AC-3: unknown keys are reported once with plugin and key name; processing continues
#[test]
fn unknown_key_reported_by_plugin() {
    let plugin = flagme_plugin_countryflag::CountryFlagPlugin::new();
    let caps = Arc::new(RecordingCaps::default());
    let mut d = doc("x.jpg");
    d.location = Some(LocationInfo {
        country_code: Some("DE".into()),
        ..LocationInfo::default()
    });
    let c = ctx(&caps, &[("flag_style", "round")]);
    let out = plugin.process(d.clone(), &c).unwrap();
    let _ = plugin.process(d, &c).unwrap();
    assert_eq!(out.country.unwrap().country_code, "DE");
    assert_eq!(
        caps.warnings(),
        vec!["settings[countryflag]: unknown key 'flag_style' ignored".to_string()]
    );
}

// ---------------------------------------------------------------------------
// AC-4: one test per key
// ---------------------------------------------------------------------------

// plugin-settings AC-4: metadata makernote_max_length controls MakerNote truncation
#[test]
fn metadata_makernote_max_length() {
    let dir = temp_dir("makernote");
    let path = write_file(&dir, "mn.jpg", &jpeg_with_makernote(400));
    let plugin = MetadataPlugin::new();
    let caps = Arc::new(RecordingCaps::default());

    let default = plugin.process(doc(&path), &ctx(&caps, &[])).unwrap();
    let mn = default.metadata.unwrap().raw_tags["Exif.MakerNote"].clone();
    assert!(mn.starts_with("<MakerNote "), "{}", mn);

    let long = plugin
        .process(doc(&path), &ctx(&caps, &[("makernote_max_length", "65536")]))
        .unwrap();
    let mn = long.metadata.unwrap().raw_tags["Exif.MakerNote"].clone();
    assert!(!mn.starts_with("<MakerNote "));
    assert!(mn.len() > 512);
}

// plugin-settings AC-4: metadata xmp_max_value_length controls which XMP values are kept
#[test]
fn metadata_xmp_max_value_length() {
    let dir = temp_dir("xmp");
    let path = write_file(&dir, "x.xmp.jpg", &xmp_file(600));
    let plugin = MetadataPlugin::new();
    let caps = Arc::new(RecordingCaps::default());

    let default = plugin.process(doc(&path), &ctx(&caps, &[])).unwrap();
    assert!(!default.metadata.unwrap().raw_tags.contains_key("XMP.dc.longvalue"));

    let long = plugin
        .process(doc(&path), &ctx(&caps, &[("xmp_max_value_length", "1024")]))
        .unwrap();
    assert_eq!(long.metadata.unwrap().raw_tags["XMP.dc.longvalue"].len(), 600);
}

// plugin-settings AC-4: reversegeo offline_cache=false disables KV reads and writes
#[test]
fn reversegeo_offline_cache() {
    let plugin = ReverseGeoPlugin::new();
    let caps = Arc::new(RecordingCaps::default());
    plugin
        .process(with_location(doc("a.jpg"), 52.52, 13.405), &ctx(&caps, &[("offline_cache", "false")]))
        .unwrap();
    assert!(caps.kv_reads().is_empty());
    assert!(caps.kv_keys().is_empty());

    let caps = Arc::new(RecordingCaps::default());
    plugin.process(with_location(doc("a.jpg"), 52.52, 13.405), &ctx(&caps, &[])).unwrap();
    assert_eq!(caps.kv_reads(), vec!["geo:52.520:13.405".to_string()]);
}

// plugin-settings AC-4: reversegeo offline_lookup=false skips the city table and uses the online result
#[test]
fn reversegeo_offline_lookup() {
    let plugin = ReverseGeoPlugin::new();
    let caps = Arc::new(RecordingCaps::with_http(NOMINATIM_MUNICH));
    let out = plugin
        .process(
            with_location(doc("a.jpg"), 48.1351, 11.582),
            &ctx(&caps, &[("offline_lookup", "false"), ("offline_cache", "false")]),
        )
        .unwrap();
    let loc = out.location.unwrap();
    assert_eq!(loc.city.as_deref(), Some("München"));
    assert_eq!(caps.urls().len(), 1);
}

// plugin-settings AC-4: reversegeo online_lookup=false never issues an HTTP request
#[test]
fn reversegeo_online_lookup() {
    let plugin = ReverseGeoPlugin::new();
    let caps = Arc::new(RecordingCaps::with_http(NOMINATIM_MUNICH));
    let out = plugin
        .process(with_location(doc("a.jpg"), 0.0, 0.0), &ctx(&caps, &[("online_lookup", "false")]))
        .unwrap();
    assert!(caps.urls().is_empty());
    assert_eq!(out.location.unwrap().country_code.as_deref(), Some("XX"));
}

// plugin-settings AC-4: reversegeo city_radius_km limits the nearest-city search
#[test]
fn reversegeo_city_radius_km() {
    let plugin = ReverseGeoPlugin::new();
    let caps = Arc::new(RecordingCaps::default());
    let near_munich = || with_location(doc("a.jpg"), 48.4351, 11.582);

    let default = plugin.process(near_munich(), &ctx(&caps, &[("offline_cache", "off")])).unwrap();
    assert_eq!(default.location.unwrap().city.as_deref(), Some("Munich"));

    let small = plugin
        .process(near_munich(), &ctx(&caps, &[("offline_cache", "off"), ("city_radius_km", "10")]))
        .unwrap();
    // No reference city within 10 km -> country bounding box (capital)
    assert_eq!(small.location.unwrap().city.as_deref(), Some("Berlin"));
}

// plugin-settings AC-4: reversegeo cache_precision sets the decimals of the cache key
#[test]
fn reversegeo_cache_precision() {
    let plugin = ReverseGeoPlugin::new();
    let caps = Arc::new(RecordingCaps::default());
    plugin
        .process(with_location(doc("a.jpg"), 48.1351, 11.582), &ctx(&caps, &[("cache_precision", "1")]))
        .unwrap();
    assert_eq!(caps.kv_reads(), vec!["geo:48.1:11.6".to_string()]);
}

// plugin-settings AC-4: reversegeo nominatim_url replaces the online endpoint
#[test]
fn reversegeo_nominatim_url() {
    let plugin = ReverseGeoPlugin::new();
    let caps = Arc::new(RecordingCaps::with_http(NOMINATIM_MUNICH));
    plugin
        .process(
            with_location(doc("a.jpg"), 0.0, 0.0),
            &ctx(&caps, &[("nominatim_url", "https://geo.example.org/reverse")]),
        )
        .unwrap();
    assert!(caps.urls()[0].starts_with("https://geo.example.org/reverse?format=jsonv2&lat=0&lon=0"));
}

// plugin-settings AC-4: reversegeo accept_language is sent and stored as local_language
#[test]
fn reversegeo_accept_language() {
    let plugin = ReverseGeoPlugin::new();
    let caps = Arc::new(RecordingCaps::with_http(NOMINATIM_MUNICH));
    let out = plugin
        .process(
            with_location(doc("a.jpg"), 0.0, 0.0),
            &ctx(&caps, &[("accept_language", "de")]),
        )
        .unwrap();
    assert!(caps.urls()[0].ends_with("&accept-language=de"));
    assert_eq!(out.location.unwrap().local_language.as_deref(), Some("de"));
}

// plugin-settings AC-4: thumbnail variants defines the sizes and labels
#[test]
fn thumbnail_variants() {
    let dir = temp_dir("thumb-variants");
    let out_dir = dir.to_string_lossy().to_string();
    let plugin = ThumbnailPlugin::new();
    let caps = Arc::new(RecordingCaps::default());
    let out = plugin
        .process(
            doc("missing.jpg"),
            &ctx(&caps, &[("variants", "64,256,2048,1024"), ("output_dir", &out_dir)]),
        )
        .unwrap();
    let got: Vec<(String, u32)> = out.thumbnails.iter().map(|t| (t.size_label.clone(), t.width)).collect();
    assert_eq!(
        got,
        vec![
            ("small".to_string(), 64),
            ("medium".to_string(), 256),
            ("1024px".to_string(), 1024),
            ("large".to_string(), 2048),
        ]
    );
}

// plugin-settings AC-4: thumbnail max_dimension drops larger variants
#[test]
fn thumbnail_max_dimension() {
    let dir = temp_dir("thumb-max");
    let out_dir = dir.to_string_lossy().to_string();
    let plugin = ThumbnailPlugin::new();
    let caps = Arc::new(RecordingCaps::default());
    let out = plugin
        .process(doc("missing.jpg"), &ctx(&caps, &[("max_dimension", "600"), ("output_dir", &out_dir)]))
        .unwrap();
    let sizes: Vec<u32> = out.thumbnails.iter().map(|t| t.width).collect();
    assert_eq!(sizes, vec![128, 512]);

    let out = plugin
        .process(doc("missing.jpg"), &ctx(&caps, &[("max_dimension", "100"), ("output_dir", &out_dir)]))
        .unwrap();
    let sizes: Vec<u32> = out.thumbnails.iter().map(|t| t.width).collect();
    assert_eq!(sizes, vec![100]);
}

// plugin-settings AC-4: thumbnail output_dir sets the target directory
#[test]
fn thumbnail_output_dir() {
    let dir = temp_dir("thumb-dir");
    let out_dir = dir.to_string_lossy().to_string();
    let plugin = ThumbnailPlugin::new();
    let caps = Arc::new(RecordingCaps::default());
    let out = plugin
        .process(doc("missing.jpg"), &ctx(&caps, &[("output_dir", &out_dir)]))
        .unwrap();
    for t in &out.thumbnails {
        assert!(t.path.starts_with(&out_dir), "{}", t.path);
        assert!(Path::new(&t.path).exists());
    }
}

// plugin-settings AC-4: thumbnail quality changes the encoded size (macOS: sips is the only scaler, see #25)
#[cfg(target_os = "macos")]
#[test]
fn thumbnail_quality() {
    // Real, decodable source image: 600x600 noise BMP converted to JPEG with sips.
    let dir = temp_dir("thumb-q-src");
    let (w, h) = (600u32, 600u32);
    let row = (w * 3 + 3) & !3;
    let mut bmp = Vec::new();
    bmp.extend(b"BM");
    bmp.extend((54 + row * h).to_le_bytes());
    bmp.extend([0u8; 4]);
    bmp.extend(54u32.to_le_bytes());
    bmp.extend(40u32.to_le_bytes());
    bmp.extend((w as i32).to_le_bytes());
    bmp.extend((h as i32).to_le_bytes());
    bmp.extend(1u16.to_le_bytes());
    bmp.extend(24u16.to_le_bytes());
    bmp.extend([0u8; 24]);
    let mut seed: u32 = 12345;
    for _ in 0..h {
        for _ in 0..row {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            bmp.push((seed >> 16) as u8);
        }
    }
    let bmp_path = write_file(&dir, "src.bmp", &bmp);
    let src = dir.join("src.jpg").to_string_lossy().to_string();
    let status = std::process::Command::new("sips")
        .args(["-s", "format", "jpeg", &bmp_path, "--out", &src])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let plugin = ThumbnailPlugin::new();
    let caps = Arc::new(RecordingCaps::default());
    let size_for = |q: &str| {
        let dir = temp_dir(&format!("thumb-q{}", q));
        let out_dir = dir.to_string_lossy().to_string();
        let out = plugin
            .process(doc(&src), &ctx(&caps, &[("quality", q), ("variants", "512"), ("output_dir", &out_dir)]))
            .unwrap();
        std::fs::metadata(&out.thumbnails[0].path).unwrap().len()
    };
    assert!(size_for("5") < size_for("100"));
}

// plugin-settings AC-4: ai-tagging model selects the built-in model
#[test]
fn ai_tagging_model() {
    let plugin = AiTaggingPlugin::new();
    let caps = Arc::new(RecordingCaps::default());
    let out = plugin.process(doc("missing.jpg"), &ctx(&caps, &[("model", "MobileNetV3")])).unwrap();
    assert!(out.ai.unwrap().tags.iter().all(|t| t.model == "MobileNetV3"));
}

// plugin-settings AC-4: ai-tagging model_tier selects a model when model is not set
#[test]
fn ai_tagging_model_tier() {
    let plugin = AiTaggingPlugin::new();
    let caps = Arc::new(RecordingCaps::default());
    let heavy = plugin.process(doc("missing.jpg"), &ctx(&caps, &[("model_tier", "heavy")])).unwrap();
    assert_eq!(heavy.ai.unwrap().tags[0].model, "OpenCLIP-ConvNeXt-Large");

    let both = plugin
        .process(doc("missing.jpg"), &ctx(&caps, &[("model_tier", "heavy"), ("model", "clip")]))
        .unwrap();
    assert_eq!(both.ai.unwrap().tags[0].model, "CLIP-ViT-B/32", "model wins over model_tier");
}

// plugin-settings AC-4: ai-tagging embedding_dim sets the embedding length
#[test]
fn ai_tagging_embedding_dim() {
    let plugin = AiTaggingPlugin::new();
    let caps = Arc::new(RecordingCaps::default());
    let out = plugin.process(doc("missing.jpg"), &ctx(&caps, &[("embedding_dim", "128")])).unwrap();
    let emb = out.ai.unwrap().embedding.unwrap();
    assert_eq!(emb.len(), 128);
    let norm: f32 = emb.iter().map(|v| v * v).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 1e-4);
}

// plugin-settings AC-4: ai-tagging device accepts cpu without warning
#[test]
fn ai_tagging_device() {
    let plugin = AiTaggingPlugin::new();
    let caps = Arc::new(RecordingCaps::default());
    plugin.process(doc("missing.jpg"), &ctx(&caps, &[("device", "CPU")])).unwrap();
    assert!(caps.warnings().is_empty());
}

// plugin-settings AC-4: timeline timezone converts local EXIF times to UTC
#[test]
fn timeline_timezone() {
    let plugin = TimelinePlugin::new();
    let caps = Arc::new(RecordingCaps::default());
    let mut d = doc("missing.jpg");
    d.metadata = Some(MetadataInfo {
        raw_tags: map(&[("Exif.Photo.DateTimeOriginal", "2025:07:01 12:00:00")]),
        ..MetadataInfo::default()
    });

    let cases = [
        ("UTC", Utc.with_ymd_and_hms(2025, 7, 1, 12, 0, 0).unwrap()),
        ("Europe/Berlin", Utc.with_ymd_and_hms(2025, 7, 1, 10, 0, 0).unwrap()),
        ("+05:30", Utc.with_ymd_and_hms(2025, 7, 1, 6, 30, 0).unwrap()),
    ];
    for (tz, expected) in cases {
        let out = plugin.process(d.clone(), &ctx(&caps, &[("timezone", tz)])).unwrap();
        assert_eq!(out.timeline.unwrap().taken_at, Some(expected), "{}", tz);
    }
    assert!(caps.warnings().is_empty());
}

// plugin-settings AC-4: timeline source_priority changes the order of the sources
#[test]
fn timeline_source_priority() {
    let plugin = TimelinePlugin::new();
    let caps = Arc::new(RecordingCaps::default());
    let mut d = doc("2025-10-25_094918.jpg");
    d.metadata = Some(MetadataInfo {
        raw_tags: map(&[("Exif.Photo.DateTimeOriginal", "2020:01:01 00:00:00")]),
        ..MetadataInfo::default()
    });

    let default = plugin.process(d.clone(), &ctx(&caps, &[])).unwrap();
    assert_eq!(default.timeline.unwrap().resolution_source, "exif_datetime_original");

    let out = plugin
        .process(d, &ctx(&caps, &[("source_priority", "filename_timestamp, exif_datetime_original")]))
        .unwrap();
    let t = out.timeline.unwrap();
    assert_eq!(t.resolution_source, "filename_timestamp");
    assert_eq!(t.taken_at, Some(Utc.with_ymd_and_hms(2025, 10, 25, 9, 49, 18).unwrap()));
}

fn weather_doc() -> UnifiedDocument {
    let mut d = with_location(doc("missing.jpg"), 52.5200, 13.4050);
    d.timeline = Some(TimelineInfo {
        taken_at: Some(Utc.with_ymd_and_hms(2025, 7, 1, 12, 0, 0).unwrap()),
        imported_at: Utc::now(),
        modified_at: None,
        resolution_source: "test".into(),
    });
    d
}

// plugin-settings AC-4: weather-enrich endpoint replaces the archive URL
#[test]
fn weather_endpoint() {
    let plugin = WeatherEnrichPlugin::new();
    let caps = Arc::new(RecordingCaps::default());
    plugin
        .process(weather_doc(), &ctx(&caps, &[("endpoint", "https://weather.example.org/v1/archive")]))
        .unwrap();
    assert!(caps.urls()[0].starts_with("https://weather.example.org/v1/archive?latitude=52.52&longitude=13.40"));
}

// plugin-settings AC-4: weather-enrich coordinate_precision sets request and cache-key decimals
#[test]
fn weather_coordinate_precision() {
    let plugin = WeatherEnrichPlugin::new();
    let caps = Arc::new(RecordingCaps::default());
    plugin.process(weather_doc(), &ctx(&caps, &[("coordinate_precision", "1")])).unwrap();
    assert_eq!(caps.kv_reads(), vec!["weather:52.5:13.4:2025-07-01-12".to_string()]);
    assert!(caps.urls()[0].contains("latitude=52.5&longitude=13.4&"));
}

// plugin-settings AC-4: weather-enrich offline_fallback=false leaves weather empty when the request fails
#[test]
fn weather_offline_fallback() {
    let plugin = WeatherEnrichPlugin::new();
    let caps = Arc::new(RecordingCaps::default());
    let with_fallback = plugin.process(weather_doc(), &ctx(&caps, &[])).unwrap();
    assert!(with_fallback.weather.is_some());

    let caps = Arc::new(RecordingCaps::default());
    let without = plugin
        .process(weather_doc(), &ctx(&caps, &[("offline_fallback", "false")]))
        .unwrap();
    assert!(without.weather.is_none());
    assert!(caps.kv_keys().is_empty(), "nothing is cached");
}

/// OCR engine returning two fixed regions; used to observe settings without the default engine.
struct TwoRegionEngine;
impl OcrEngine for TwoRegionEngine {
    fn extract_text(&self, _path: &str) -> OcrInfo {
        let bbox = BoundingBox { x: 0.1, y: 0.1, width: 0.5, height: 0.1 };
        OcrInfo {
            combined_text: "high low".into(),
            regions: vec![
                TextRegion { text: "high".into(), confidence: 0.9, bbox: bbox.clone() },
                TextRegion { text: "low".into(), confidence: 0.5, bbox },
            ],
            primary_language: Some("en".into()),
        }
    }
}

// plugin-settings AC-4: ocr-text engine=default replaces the constructor engine
#[test]
fn ocr_engine() {
    let plugin = OcrTextPlugin::with_engine(Box::new(TwoRegionEngine));
    let caps = Arc::new(RecordingCaps::default());
    let custom = plugin.process(doc("plain.jpg"), &ctx(&caps, &[])).unwrap();
    assert_eq!(custom.ocr.unwrap().regions.len(), 2);

    let builtin = plugin.process(doc("plain.jpg"), &ctx(&caps, &[("engine", "default")])).unwrap();
    assert!(builtin.ocr.unwrap().regions.is_empty());
}

/// Face detector returning two faces with confidence 0.70 and 0.95.
struct TwoFaceDetector;
impl FaceDetector for TwoFaceDetector {
    fn tier(&self) -> ModelTier {
        ModelTier::Lightweight
    }
    fn model_name(&self) -> &'static str {
        "two-face-test-detector"
    }
    fn detect(&self, _path: &str) -> Vec<DetectedFace> {
        [("a", 0.70f32), ("b", 0.95f32)]
            .iter()
            .map(|(id, c)| DetectedFace {
                id: id.to_string(),
                bbox: BoundingBox { x: 0.1, y: 0.1, width: 0.2, height: 0.2 },
                confidence: *c,
                landmarks: None,
                embedding: None,
                person_id: None,
            })
            .collect()
    }
}

// plugin-settings AC-4: face-detection tier selects the built-in detector
#[test]
fn face_tier() {
    let plugin = FaceDetectionPlugin::with_detector(Box::new(TwoFaceDetector));
    let caps = Arc::new(RecordingCaps::default());
    let custom = plugin.process(doc("plain.jpg"), &ctx(&caps, &[])).unwrap();
    assert_eq!(custom.faces.unwrap().detection_model, "two-face-test-detector");
    let heavy = plugin.process(doc("plain.jpg"), &ctx(&caps, &[("tier", "heavy")])).unwrap();
    assert_eq!(heavy.faces.unwrap().detection_model, "SCRFD-ArcFace-ResNet100");
}

// plugin-settings AC-4: face-detection device accepts cpu without warning
#[test]
fn face_device() {
    let plugin = FaceDetectionPlugin::new();
    let caps = Arc::new(RecordingCaps::default());
    plugin.process(doc("plain.jpg"), &ctx(&caps, &[("device", "cpu")])).unwrap();
    assert!(caps.warnings().is_empty());
}

fn privacy_doc(dir: &Path) -> UnifiedDocument {
    let img = image::RgbImage::from_fn(64, 48, |x, y| image::Rgb([(x * 4) as u8, (y * 5) as u8, 128]));
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg).encode_image(&img).unwrap();
    let src = write_file(dir, "src.jpg", &jpeg);
    let mut d = with_location(doc(&src), 52.518624, 13.376198);
    d.metadata = Some(MetadataInfo {
        raw_tags: map(&[("CameraSerialNumber", "123"), ("Exif.Model", "X")]),
        ..MetadataInfo::default()
    });
    d.faces = Some(FacesInfo {
        faces: TwoFaceDetector.detect(""),
        detection_model: "test".into(),
    });
    d
}

// plugin-settings AC-4: privacy-redact keys from config.yaml change the result
#[test]
fn privacy_redact_keys() {
    let dir = temp_dir("privacy");
    let out_dir = dir.join("out").to_string_lossy().to_string();
    let plugin = PrivacyRedactPlugin::new();
    let caps = Arc::new(RecordingCaps::default());

    let out = plugin
        .process(
            privacy_doc(&dir),
            &ctx(
                &caps,
                &[
                    ("style", "blackout"),
                    ("gps_decimal_places", "1"),
                    ("strip_serial_numbers", "false"),
                    ("output_dir", &out_dir),
                ],
            ),
        )
        .unwrap();
    let p = out.privacy.unwrap();
    assert!(p.applied_rules.contains(&"redacted_2_faces_via_blackout".to_string()));
    assert!(p.applied_rules.contains(&"fuzzed_gps_precision_1_decimals".to_string()));
    assert_eq!(out.location.unwrap().latitude, Some(52.5));
    assert!(out.metadata.unwrap().raw_tags.contains_key("CameraSerialNumber"));
    assert!(p.redacted_image_path.unwrap().starts_with(&out_dir));

    let out = plugin
        .process(privacy_doc(&dir), &ctx(&caps, &[("redact_faces", "false"), ("fuzz_gps", "false")]))
        .unwrap();
    let p = out.privacy.unwrap();
    assert_eq!(p.faces_redacted_count, 0);
    assert!(!p.original_gps_fuzzed);

    // block_size changes the pixelation pattern
    let render = |bs: &str| {
        let o = dir.join(format!("bs{}", bs)).to_string_lossy().to_string();
        let out = plugin
            .process(privacy_doc(&dir), &ctx(&caps, &[("block_size", bs), ("output_dir", &o)]))
            .unwrap();
        std::fs::read(out.privacy.unwrap().redacted_image_path.unwrap()).unwrap()
    };
    assert_ne!(render("2"), render("64"));

    // blur_sigma is parsed (clamped) but has no effect yet, see #22
    plugin.process(privacy_doc(&dir), &ctx(&caps, &[("blur_sigma", "500")])).unwrap();
    assert!(caps.warnings().iter().any(|w| w.contains("'blur_sigma' out of range")));
}

// ---------------------------------------------------------------------------
// AC-5
// ---------------------------------------------------------------------------

// plugin-settings AC-5: without settings all plugins keep their previous behaviour
#[test]
fn defaults_match_previous_behaviour() {
    assert_eq!(
        MetadataSettings::default(),
        MetadataSettings { makernote_max_length: 512, xmp_max_value_length: 512 }
    );

    let t = ThumbnailSettings::default();
    assert_eq!(t.effective_sizes(), vec![128, 512, 1024]);
    assert_eq!(flagme_plugin_thumbnail::size_labels(&t.effective_sizes()), vec!["small", "medium", "large"]);
    assert_eq!(t.output_dir, "cache/thumbnails");
    assert_eq!(t.quality, None);

    let r = ReverseGeoSettings::default();
    assert!(r.offline_cache && r.offline_lookup && r.online_lookup);
    assert_eq!((r.city_radius_km, r.cache_precision), (200, 3));
    assert_eq!(r.nominatim_url, "https://nominatim.openstreetmap.org/reverse");
    assert_eq!(r.accept_language, "en");

    // Previous radius was 1.8 degrees: a point 1.79° (scaled) east of Berlin, outside every
    // country box, still resolves to Berlin; with a smaller radius it would be unresolved.
    let caps = Arc::new(RecordingCaps::default());
    let lon = 13.405 + 1.79 / 52.52f64.to_radians().cos();
    let out = ReverseGeoPlugin::new()
        .process(
            with_location(doc("a.jpg"), 52.52, lon),
            &ctx(&caps, &[("offline_cache", "off"), ("online_lookup", "off")]),
        )
        .unwrap();
    assert_eq!(out.location.unwrap().city.as_deref(), Some("Berlin"));

    let caps = Arc::new(RecordingCaps::default());
    let w = WeatherEnrichPlugin::new().process(weather_doc(), &ctx(&caps, &[])).unwrap();
    assert!(w.weather.is_some());
    assert_eq!(caps.kv_reads(), vec!["weather:52.52:13.40:2025-07-01-12".to_string()]);
    assert!(caps.urls()[0].starts_with("https://archive-api.open-meteo.com/v1/archive?latitude=52.52&longitude=13.40&"));

    assert_eq!(DuplicateSettings::default().phash_threshold, 0);
    let caps = Arc::new(RecordingCaps::default());
    DuplicatePlugin::new().process(doc("missing.jpg"), &ctx(&caps, &[])).unwrap();
    assert!(!caps.kv_keys().contains(&PHASH_INDEX_KEY.to_string()));

    assert_eq!(TimelineSettings::default().source_priority.len(), 6);
    assert_eq!(AiTaggingSettings::default().embedding_dim, 512);
    assert_eq!(OcrSettings::default().min_confidence, 0.0);
    assert_eq!(FaceDetectionSettings::default().min_confidence, 0.0);
    let p = PrivacySettings::default();
    assert_eq!((p.block_size, p.gps_decimal_places, p.output_dir.as_str()), (16, 2, "cache/redacted"));

    let caps = Arc::new(RecordingCaps::default());
    let ai = AiTaggingPlugin::new().process(doc("missing.jpg"), &ctx(&caps, &[])).unwrap();
    assert_eq!(ai.ai.unwrap().tags[0].model, "CLIP-ViT-B/32");
    let faces = FaceDetectionPlugin::new().process(doc("missing.jpg"), &ctx(&caps, &[])).unwrap();
    assert_eq!(faces.faces.unwrap().detection_model, "UltraFace-Slim-INT8");
    assert!(caps.warnings().is_empty());
}

// ---------------------------------------------------------------------------
// AC-6
// ---------------------------------------------------------------------------

// plugin-settings AC-6: values that need a missing implementation warn with the supported values and use the default
#[test]
fn unsupported_value_warns() {
    let caps = Arc::new(RecordingCaps::default());

    let ai = AiTaggingPlugin::new().process(doc("missing.jpg"), &ctx(&caps, &[("device", "mps")])).unwrap();
    assert!(ai.ai.is_some());
    FaceDetectionPlugin::new().process(doc("missing.jpg"), &ctx(&caps, &[("device", "cuda")])).unwrap();
    let ocr = OcrTextPlugin::new()
        .process(doc("plain.jpg"), &ctx(&caps, &[("engine", "PaddleOCR-v4-Server")]))
        .unwrap();
    assert!(ocr.ocr.is_some());

    assert_eq!(
        caps.warnings(),
        vec![
            "settings[ai-tagging]: 'mps' for 'device' is not supported yet (supported: cpu), using default 'cpu'".to_string(),
            "settings[face-detection]: 'cuda' for 'device' is not supported yet (supported: cpu), using default 'cpu'".to_string(),
            "settings[ocr-text]: 'PaddleOCR-v4-Server' for 'engine' is not supported yet (supported: default), using default 'default'".to_string(),
        ]
    );
}

// ---------------------------------------------------------------------------
// AC-7
// ---------------------------------------------------------------------------

// plugin-settings AC-7: duplicate treats images within phash_threshold Hamming bits as duplicates
#[test]
fn duplicate_phash_threshold() {
    let dir = temp_dir("dup");
    let b1: Vec<u8> = (0..4096u32).map(|i| ((i * 7919) % 251) as u8).collect();
    let mut b2 = b1.clone();
    for b in b2.iter_mut().take(200) {
        *b = 0;
    }
    let p1 = write_file(&dir, "one.bin", &b1);
    let p2 = write_file(&dir, "two.bin", &b2);

    let plugin = DuplicatePlugin::new();
    let distance = flagme_plugin_duplicate::hamming_distance(&plugin.compute_phash(&b1), &plugin.compute_phash(&b2)).unwrap();
    assert!(distance > 0 && distance < 64, "distance {}", distance);

    let run = |threshold: u32| {
        let caps = Arc::new(RecordingCaps::default());
        let t = threshold.to_string();
        let c = ctx(&caps, &[("phash_threshold", &t)]);
        let mut first = doc(&p1);
        first.id = "first".into();
        let mut second = doc(&p2);
        second.id = "second".into();
        plugin.process(first, &c).unwrap();
        plugin.process(second, &c).unwrap().duplicate.unwrap().duplicates
    };

    assert_eq!(run(distance), vec!["first".to_string()]);
    assert!(run(distance - 1).is_empty());
    assert!(run(0).is_empty(), "exact match only");
}

// ---------------------------------------------------------------------------
// AC-8
// ---------------------------------------------------------------------------

// plugin-settings AC-8: face-detection and ocr-text drop entries below min_confidence
#[test]
fn min_confidence_filters() {
    let caps = Arc::new(RecordingCaps::default());

    let faces = FaceDetectionPlugin::with_detector(Box::new(TwoFaceDetector))
        .process(doc("plain.jpg"), &ctx(&caps, &[("min_confidence", "0.8")]))
        .unwrap()
        .faces
        .unwrap()
        .faces;
    assert_eq!(faces.len(), 1);
    assert!(faces.iter().all(|f| f.confidence >= 0.8));

    let ocr = OcrTextPlugin::with_engine(Box::new(TwoRegionEngine))
        .process(doc("plain.jpg"), &ctx(&caps, &[("min_confidence", "0.6")]))
        .unwrap()
        .ocr
        .unwrap();
    assert_eq!(ocr.regions.len(), 1);
    assert_eq!(ocr.combined_text, "high");

    let none = OcrTextPlugin::with_engine(Box::new(TwoRegionEngine))
        .process(doc("plain.jpg"), &ctx(&caps, &[("min_confidence", "1.0")]))
        .unwrap()
        .ocr
        .unwrap();
    assert!(none.regions.is_empty());
    assert_eq!(none.primary_language, None);
}

// ---------------------------------------------------------------------------
// AC-9
// ---------------------------------------------------------------------------

// plugin-settings AC-9: one plugin instance selects model and tier per call from ctx.settings
#[test]
fn model_and_tier_selected_per_call() {
    let caps = Arc::new(RecordingCaps::default());
    let ai = AiTaggingPlugin::new();
    let a = ai.process(doc("missing.jpg"), &ctx(&caps, &[("model", "openclip")])).unwrap();
    let b = ai.process(doc("missing.jpg"), &ctx(&caps, &[("model", "mobilenet")])).unwrap();
    assert_eq!(a.ai.unwrap().tags[0].model, "OpenCLIP-ConvNeXt-Large");
    assert_eq!(b.ai.unwrap().tags[0].model, "MobileNetV3");

    let face = FaceDetectionPlugin::new();
    let h = face.process(doc("missing.jpg"), &ctx(&caps, &[("tier", "heavy")])).unwrap();
    let l = face.process(doc("missing.jpg"), &ctx(&caps, &[("tier", "lightweight")])).unwrap();
    assert_eq!(h.faces.unwrap().detection_model, "SCRFD-ArcFace-ResNet100");
    assert_eq!(l.faces.unwrap().detection_model, "UltraFace-Slim-INT8");
}

// ---------------------------------------------------------------------------
// AC-10
// ---------------------------------------------------------------------------

// plugin-settings AC-10: the host YAML reader keeps '#' inside quotes and still strips comments
#[test]
fn hash_inside_quotes_kept() {
    let yaml = r#"
plugins:
  weather-enrich:
    enabled: true
    settings:
      endpoint: "https://weather.example.org/archive#v1" # own instance
      note: 'a # b'
      plain: value # comment
      label: it's fine # comment
"#;
    let cfg = AppConfig::load_from_str(yaml).unwrap();
    let s = &cfg.plugins["weather-enrich"].settings;
    assert_eq!(s["endpoint"], "https://weather.example.org/archive#v1");
    assert_eq!(s["note"], "a # b");
    assert_eq!(s["plain"], "value");
    assert_eq!(s["label"], "it's fine");
}

// ---------------------------------------------------------------------------
// AC-11 / AC-12
// ---------------------------------------------------------------------------

/// Parses `settings` of `plugin` with its settings type; returns the warnings.
fn parse_warnings(plugin: &str, settings: &HashMap<String, String>) -> Vec<String> {
    let caps = RecordingCaps::default();
    match plugin {
        "metadata" => drop(MetadataSettings::parse(&Default::default(), settings, &caps)),
        "reversegeo" => drop(ReverseGeoSettings::parse(&Default::default(), settings, &caps)),
        "countryflag" => drop(CountryFlagSettings::parse(&Default::default(), settings, &caps)),
        "thumbnail" => drop(ThumbnailSettings::parse(&Default::default(), settings, &caps)),
        "ai-tagging" => drop(AiTaggingSettings::parse(&Default::default(), settings, &caps)),
        "duplicate" => drop(DuplicateSettings::parse(&Default::default(), settings, &caps)),
        "timeline" => drop(TimelineSettings::parse(&Default::default(), settings, &caps)),
        "weather-enrich" => drop(WeatherSettings::parse(&Default::default(), settings, &caps)),
        "ocr-text" => drop(OcrSettings::parse(&Default::default(), settings, &caps)),
        "face-detection" => drop(FaceDetectionSettings::parse(&Default::default(), settings, &caps)),
        "privacy-redact" => drop(PrivacySettings::parse(&Default::default(), settings, &caps)),
        other => panic!("unknown plugin {}", other),
    }
    caps.warnings()
}

// plugin-settings AC-11: the shipped profiles contain only evaluated keys and load without warnings
#[test]
fn profiles_have_no_unknown_keys() {
    for profile in ["config.yaml", "config.docker.yaml", "config.mac-studio.yaml"] {
        let cfg = AppConfig::load_from_file(repo_root().join(profile)).unwrap();
        for (name, pcfg) in &cfg.plugins {
            let w = parse_warnings(name, &pcfg.settings);
            assert!(w.is_empty(), "{} / {}: {:?}", profile, name, w);
        }
    }
}

/// Every settings key per plugin with a valid sample value.
const ALL_KEYS: &[(&str, &[(&str, &str)])] = &[
    ("metadata", &[("makernote_max_length", "1024"), ("xmp_max_value_length", "1024")]),
    (
        "reversegeo",
        &[
            ("offline_cache", "true"),
            ("offline_lookup", "true"),
            ("online_lookup", "false"),
            ("city_radius_km", "50"),
            ("cache_precision", "2"),
            ("nominatim_url", "https://geo.example.org/reverse"),
            ("accept_language", "de"),
        ],
    ),
    ("countryflag", &[]),
    (
        "thumbnail",
        &[("variants", "128,256"), ("max_dimension", "1024"), ("quality", "80"), ("output_dir", "cache/t")],
    ),
    (
        "ai-tagging",
        &[("model", "clip"), ("model_tier", "heavy"), ("embedding_dim", "256"), ("device", "cpu")],
    ),
    ("duplicate", &[("phash_threshold", "8")]),
    ("timeline", &[("timezone", "Europe/Berlin"), ("source_priority", "exif_datetime_original")]),
    (
        "weather-enrich",
        &[("endpoint", "https://w.example.org/a"), ("coordinate_precision", "3"), ("offline_fallback", "false")],
    ),
    ("ocr-text", &[("engine", "default"), ("min_confidence", "0.5")]),
    ("face-detection", &[("tier", "heavy"), ("min_confidence", "0.5"), ("device", "cpu")]),
    (
        "privacy-redact",
        &[
            ("style", "blur"),
            ("block_size", "8"),
            ("blur_sigma", "10"),
            ("redact_faces", "true"),
            ("fuzz_gps", "true"),
            ("gps_decimal_places", "3"),
            ("strip_serial_numbers", "true"),
            ("output_dir", "cache/r"),
        ],
    ),
];

// plugin-settings AC-12: every settings key is known to its plugin and documented in the plugin README
#[test]
fn readmes_list_all_keys() {
    for (plugin, keys) in ALL_KEYS {
        let w = parse_warnings(plugin, &map(keys));
        assert!(w.is_empty(), "{}: {:?}", plugin, w);

        let readme = std::fs::read_to_string(repo_root().join("plugins").join(plugin).join("README.md")).unwrap();
        let section = readme
            .split("### 2.2 `settings`")
            .nth(1)
            .unwrap_or_else(|| panic!("{}: README has no settings section", plugin));
        for (key, _) in *keys {
            assert!(section.contains(&format!("`{}`", key)), "{}: key `{}` missing in README §2.2", plugin, key);
        }
    }
}
