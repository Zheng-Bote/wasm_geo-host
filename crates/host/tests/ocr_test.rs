use flagme_host::Storage;
use flagme_plugin_ocr_text::{DefaultOcrEngine, OcrEngine, OcrTextPlugin};
use flagme_sdk::{NoopCapabilities, Plugin, PluginContext};
use std::sync::Arc;
use wasm_geo_models::{BoundingBox, FileInfo, OcrInfo, TextRegion, UnifiedDocument};

// ocr-text AC-1: wasm-geo-models introduces OcrInfo and TextRegion with complete serde serialization
#[test]
fn test_ac1_ocr_model_serialization() {
    let ocr = OcrInfo {
        combined_text: "Brandenburger Tor".to_string(),
        regions: vec![TextRegion {
            text: "Brandenburger Tor".to_string(),
            confidence: 0.98,
            bbox: BoundingBox {
                x: 0.1,
                y: 0.2,
                width: 0.8,
                height: 0.3,
            },
        }],
        primary_language: Some("de".to_string()),
    };

    let serialized = serde_json::to_string(&ocr).expect("Serialization failed");
    let deserialized: OcrInfo = serde_json::from_str(&serialized).expect("Deserialization failed");

    assert_eq!(ocr, deserialized);
    assert_eq!(deserialized.combined_text, "Brandenburger Tor");
    assert_eq!(deserialized.regions.len(), 1);
    assert_eq!(deserialized.regions[0].confidence, 0.98);
}

// ocr-text AC-2: flagme-plugin-ocr-text compiles as plugin crate with valid metadata
#[test]
fn test_ac2_plugin_contract_metadata() {
    let plugin = OcrTextPlugin::new();
    assert_eq!(plugin.name(), "ocr-text");
    assert_eq!(plugin.version(), "0.1.0");
}

// ocr-text AC-3: Text detector and recognizer stages extract text lines with bounding coordinates and confidence scores
#[test]
fn test_ac3_text_detection_and_recognition_pipeline() {
    let engine = DefaultOcrEngine;
    let ocr_info = engine.extract_text("/media/berlin_sign.jpg");

    assert!(!ocr_info.combined_text.is_empty());
    assert!(!ocr_info.regions.is_empty());
    for region in &ocr_info.regions {
        assert!(!region.text.is_empty());
        assert!(region.confidence > 0.8);
        assert!(region.bbox.width > 0.0);
        assert!(region.bbox.height > 0.0);
    }
}

// ocr-text AC-4: SQLite storage creates and updates ocr_texts, ocr_regions, and updates the photos_fts index
#[test]
fn test_ac4_storage_and_fts5_indexing() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join(format!("test_ocr_ac4_{}.db", std::process::id()));
    let db_str = db_path.to_str().unwrap();

    let storage = Storage::new(db_str);
    storage.init_schema().expect("Schema init failed");

    let mut doc = UnifiedDocument::new(
        "photo-ocr-1",
        FileInfo {
            path: "/media/tokyo_metro.jpg".to_string(),
            filename: "tokyo_metro.jpg".to_string(),
            size_bytes: 4096,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );

    doc.ocr = Some(OcrInfo {
        combined_text: "Shinjuku Station South Gate".to_string(),
        regions: vec![TextRegion {
            text: "Shinjuku Station".to_string(),
            confidence: 0.95,
            bbox: BoundingBox {
                x: 0.1,
                y: 0.1,
                width: 0.5,
                height: 0.2,
            },
        }],
        primary_language: Some("ja".to_string()),
    });

    storage.save_document(&doc).expect("Save document failed");

    // Retrieve via Storage::get_document
    let loaded = storage.get_document("photo-ocr-1").expect("Photo not found");
    assert!(loaded.ocr.is_some());
    assert_eq!(loaded.ocr.unwrap().combined_text, "Shinjuku Station South Gate");

    // Search via FTS5 index for OCR extracted words
    let search_results = storage.search("Shinjuku").expect("Search Shinjuku failed");
    assert_eq!(search_results.len(), 1);
    assert_eq!(search_results[0].photo_id, "photo-ocr-1");

    let _ = std::fs::remove_file(&db_path);
}

// ocr-text AC-5: Integration tests verify text transcription from test image samples containing signage
#[test]
fn test_ac5_integration_signage_transcription() {
    let plugin = OcrTextPlugin::new();
    let ctx = PluginContext::new(Arc::new(NoopCapabilities));

    let doc = UnifiedDocument::new(
        "photo-ocr-berlin",
        FileInfo {
            path: "/media/berlin_subway_sign.jpg".to_string(),
            filename: "berlin_subway_sign.jpg".to_string(),
            size_bytes: 2048,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );

    let processed = plugin.process(doc, &ctx).expect("Plugin process failed");
    assert!(processed.ocr.is_some());
    let ocr = processed.ocr.unwrap();

    assert!(ocr.combined_text.contains("Brandenburger Tor"));
    assert_eq!(ocr.regions.len(), 2);
    assert_eq!(ocr.regions[0].text, "Brandenburger Tor");
}
