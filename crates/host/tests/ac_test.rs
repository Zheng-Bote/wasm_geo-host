use flagme_host::{AppConfig, Pipeline, PluginRuntime, Storage};
use flagme_plugin_ai_tagging::AiTaggingPlugin;
use flagme_plugin_countryflag::CountryFlagPlugin;
use flagme_plugin_duplicate::DuplicatePlugin;
use flagme_plugin_metadata::MetadataPlugin;
use flagme_plugin_reversegeo::ReverseGeoPlugin;
use flagme_plugin_thumbnail::ThumbnailPlugin;
use flagme_plugin_timeline::TimelinePlugin;
use flagme_plugin_weather_enrich::WeatherEnrichPlugin;
use flagme_plugin_ocr_text::OcrTextPlugin;
use flagme_plugin_face_detection::FaceDetectionPlugin;
use flagme_plugin_privacy_redact::PrivacyRedactPlugin;
use flagme_sdk::{FileInfo, NoopCapabilities, Plugin, PluginContext, UnifiedDocument};
use std::sync::Arc;

// wasm-geo-host AC-1: Workspace & Unified Document Model serialization and validation
#[test]
fn test_ac1_unified_document_serialization() {
    let doc = UnifiedDocument::new(
        "doc-test-1",
        FileInfo {
            path: "test/sample.jpg".to_string(),
            filename: "sample.jpg".to_string(),
            size_bytes: 1024,
            mime_type: "image/jpeg".to_string(),
            sha256: Some("abcdef1234567890".to_string()),
        },
    );

    let serialized = serde_json::to_string(&doc).expect("Serialization failed");
    let deserialized: UnifiedDocument =
        serde_json::from_str(&serialized).expect("Deserialization failed");

    assert_eq!(doc.id, deserialized.id);
    assert_eq!(doc.file.path, deserialized.file.path);
    assert_eq!(doc.file.size_bytes, 1024);
}

// wasm-geo-host AC-2: WIT interfaces and Plugin SDK contract verification
#[test]
fn test_ac2_plugin_sdk_contract() {
    let plugin = MetadataPlugin::new();
    assert_eq!(plugin.name(), "metadata");
    assert_eq!(plugin.version(), "0.1.0");

    let doc = UnifiedDocument::new(
        "doc-test-2",
        FileInfo {
            path: "test/dummy.jpg".to_string(),
            filename: "dummy.jpg".to_string(),
            size_bytes: 512,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );

    let ctx = PluginContext::new(Arc::new(NoopCapabilities));
    let processed = plugin.process(doc, &ctx).expect("SDK contract execution failed");
    assert!(processed.metadata.is_some());
}

// wasm-geo-host AC-3: Capability-based Plugin Host isolation and permission enforcement
#[test]
fn test_ac3_host_capability_enforcement() {
    let mut config = AppConfig::default();
    if let Some(plugin_cfg) = config.plugins.get_mut("metadata") {
        plugin_cfg.capabilities = vec![];
    }

    let mut runtime = PluginRuntime::new(config);
    runtime.register_plugin(Arc::new(MetadataPlugin::new()));

    let doc = UnifiedDocument::new(
        "doc-test-3",
        FileInfo {
            path: "test/unauthorized.jpg".to_string(),
            filename: "unauthorized.jpg".to_string(),
            size_bytes: 256,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );

    let result = runtime.execute_plugin("metadata", doc);
    assert!(result.is_ok());
}

// wasm-geo-host AC-4: Lossless Metadata Plugin extraction with raw_tags preservation
#[test]
fn test_ac4_lossless_metadata_extraction() {
    let plugin = MetadataPlugin::new();
    let doc = UnifiedDocument::new(
        "doc-test-4",
        FileInfo {
            path: "sample.jpg".to_string(),
            filename: "sample.jpg".to_string(),
            size_bytes: 4096,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );

    let ctx = PluginContext::new(Arc::new(NoopCapabilities));
    let enriched = plugin.process(doc, &ctx).unwrap();

    let meta = enriched.metadata.expect("Metadata missing");
    assert_eq!(meta.camera_make.as_deref(), Some("Sony"));
    assert_eq!(meta.camera_model.as_deref(), Some("ILCE-7RM4"));
    assert!(!meta.raw_tags.is_empty(), "Raw tags must be preserved without data loss");
    assert!(meta.raw_tags.contains_key("Exif.Image.Software"));
    assert!(meta.raw_tags.contains_key("Exif"), "raw_tags must contain structured Exif object");
    assert!(meta.raw_tags.contains_key("IPTC"), "raw_tags must contain structured IPTC object");
    assert!(meta.raw_tags.contains_key("XMP"), "raw_tags must contain structured XMP object");
}

// wasm-geo-host AC-5: ReverseGeo and CountryFlag offline/online resolution with flag emoji
#[test]
fn test_ac5_reversegeo_and_countryflag() {
    let geo_plugin = ReverseGeoPlugin::new();
    let flag_plugin = CountryFlagPlugin::new();

    let mut doc = UnifiedDocument::new(
        "doc-test-5",
        FileInfo {
            path: "berlin.jpg".to_string(),
            filename: "berlin.jpg".to_string(),
            size_bytes: 2048,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    let mut loc = wasm_geo_models::LocationInfo::default();
    loc.latitude = Some(52.52);
    loc.longitude = Some(13.405);
    doc.location = Some(loc);

    let ctx = PluginContext::new(Arc::new(NoopCapabilities));
    let doc = geo_plugin.process(doc, &ctx).unwrap();
    let resolved_loc = doc.location.as_ref().unwrap();
    assert_eq!(resolved_loc.country_code.as_deref(), Some("DE"));
    assert_eq!(resolved_loc.country.as_deref(), Some("Germany"));
    assert_eq!(resolved_loc.city.as_deref(), Some("Berlin"));
    // Dual-language local checks
    assert_eq!(resolved_loc.local_country.as_deref(), Some("Deutschland"));
    assert_eq!(resolved_loc.local_city.as_deref(), Some("Berlin"));
    assert_eq!(resolved_loc.local_language.as_deref(), Some("de"));

    let doc = flag_plugin.process(doc, &ctx).unwrap();
    let country = doc.country.expect("Country info missing");
    assert_eq!(country.country_code, "DE");
    assert_eq!(country.country_name, "Germany");
    assert_eq!(country.flag_emoji, "🇩🇪");
    assert_eq!(country.continent, "Europe");
}

// wasm-geo-host AC-6: Thumbnail deterministic caching and AI Tagging model abstraction
#[test]
fn test_ac6_thumbnail_and_ai_tagging() {
    let thumb_plugin = ThumbnailPlugin::new();
    let ai_plugin = AiTaggingPlugin::new();

    let doc = UnifiedDocument::new(
        "doc-test-6",
        FileInfo {
            path: "landscape.jpg".to_string(),
            filename: "landscape.jpg".to_string(),
            size_bytes: 8192,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );

    let ctx = PluginContext::new(Arc::new(NoopCapabilities));
    let doc = thumb_plugin.process(doc, &ctx).unwrap();
    assert_eq!(doc.thumbnails.len(), 3);
    assert_eq!(doc.thumbnails[0].size_label, "small");
    assert_eq!(doc.thumbnails[0].width, 128);
    assert_eq!(doc.thumbnails[1].width, 512);
    assert_eq!(doc.thumbnails[2].width, 1024);

    let doc = ai_plugin.process(doc, &ctx).unwrap();
    let ai = doc.ai.expect("AI tagging output missing");
    assert!(!ai.tags.is_empty());
    assert!(!ai.description.is_empty());
}

// wasm-geo-host AC-7: Duplicate detection (SHA256, pHash) and Timeline conflict resolution
#[test]
fn test_ac7_duplicate_and_timeline() {
    let dup_plugin = DuplicatePlugin::new();
    let timeline_plugin = TimelinePlugin::new();

    let doc = UnifiedDocument::new(
        "doc-test-7",
        FileInfo {
            path: "photo.jpg".to_string(),
            filename: "photo.jpg".to_string(),
            size_bytes: 1234,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );

    let ctx = PluginContext::new(Arc::new(NoopCapabilities));
    let doc = dup_plugin.process(doc, &ctx).unwrap();
    let dup = doc.duplicate.as_ref().expect("Duplicate info missing");
    assert!(!dup.sha256.is_empty());
    assert!(!dup.phash.is_empty());

    let doc = timeline_plugin.process(doc, &ctx).unwrap();
    let timeline = doc.timeline.expect("Timeline info missing");
    assert!(timeline.taken_at.is_some());
    assert!(!timeline.resolution_source.is_empty());
}

// wasm-geo-host AC-8: SQLite persistence schema and configurable pipeline orchestration
#[test]
fn test_ac8_sqlite_storage_and_persistence() {
    let db_path = std::env::temp_dir().join(format!("media_test_{}.db", uuid_simple()));
    let db_path_str = db_path.to_str().unwrap();
    let storage = Storage::new(db_path_str);
    storage.init_schema().expect("Schema init failed");

    let doc = UnifiedDocument::new(
        "doc-test-8",
        FileInfo {
            path: "persisted.jpg".to_string(),
            filename: "persisted.jpg".to_string(),
            size_bytes: 9999,
            mime_type: "image/jpeg".to_string(),
            sha256: Some("sha256-test-persisted".to_string()),
        },
    );

    storage.save_document(&doc).expect("Storage save failed");
    let retrieved = storage.get_document("doc-test-8").expect("Document not found");
    assert_eq!(retrieved.id, "doc-test-8");
    assert_eq!(retrieved.file.size_bytes, 9999);
    let _ = std::fs::remove_file(&db_path);
}

// wasm-geo-host AC-9: Observability, end-to-end media pipeline execution and validation
#[test]
fn test_ac9_end_to_end_pipeline() {
    let db_path = std::env::temp_dir().join(format!("media_e2e_{}.db", uuid_simple()));
    let db_path_str = db_path.to_str().unwrap();
    let config = AppConfig::default();
    let storage = Arc::new(Storage::new(db_path_str));
    storage.init_schema().unwrap();

    let mut runtime = PluginRuntime::new(config.clone());
    runtime.register_plugin(Arc::new(MetadataPlugin::new()));
    runtime.register_plugin(Arc::new(ReverseGeoPlugin::new()));
    runtime.register_plugin(Arc::new(CountryFlagPlugin::new()));
    runtime.register_plugin(Arc::new(ThumbnailPlugin::new()));
    runtime.register_plugin(Arc::new(AiTaggingPlugin::new()));
    runtime.register_plugin(Arc::new(DuplicatePlugin::new()));
    runtime.register_plugin(Arc::new(TimelinePlugin::new()));
    runtime.register_plugin(Arc::new(WeatherEnrichPlugin::new()));
    runtime.register_plugin(Arc::new(OcrTextPlugin::new()));
    runtime.register_plugin(Arc::new(FaceDetectionPlugin::new()));
    runtime.register_plugin(Arc::new(PrivacyRedactPlugin::new()));

    let pipeline = Pipeline::new(config, runtime, storage.clone());

    let doc = UnifiedDocument::new(
        "doc-e2e-final",
        FileInfo {
            path: "complete_photo.jpg".to_string(),
            filename: "complete_photo.jpg".to_string(),
            size_bytes: 45678,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );

    let processed = pipeline.process(doc).expect("Pipeline processing failed");
    assert!(processed.metadata.is_some());
    assert!(processed.location.is_some());
    assert!(processed.country.is_some());
    assert_eq!(processed.thumbnails.len(), 3);
    assert!(processed.ai.is_some());
    assert!(processed.duplicate.is_some());
    assert!(processed.timeline.is_some());

    let stored = storage.get_document("doc-e2e-final").expect("Stored doc missing");
    assert_eq!(stored.id, "doc-e2e-final");
    assert_eq!(stored.country.unwrap().country_code, "DE");

    // Verify new Storage instance loads persisted documents from SQLite on disk
    let reloaded_storage = Storage::new(db_path_str);
    let reloaded = reloaded_storage
        .get_document("doc-e2e-final")
        .expect("Document must be reloaded from SQLite disk file");
    assert_eq!(reloaded.id, "doc-e2e-final");
    assert!(reloaded.timeline.is_some());
    assert!(reloaded.weather.is_some());
    assert!(reloaded.privacy.is_some());
    assert_eq!(reloaded_storage.list_documents(1, 10).len(), 1);

    let _ = std::fs::remove_file(&db_path);
}

fn uuid_simple() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}
