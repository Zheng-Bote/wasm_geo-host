mod common;

use flagme_host::Storage;
use flagme_plugin_ocr_text::{OcrEngine, OcrTextPlugin};
use flagme_sdk::{NoopCapabilities, Plugin, PluginContext};
use std::sync::Arc;
use wasm_geo_models::{BoundingBox, FileInfo, OcrInfo, TextRegion, UnifiedDocument};

/// Test engine standing in for a real OCR engine (ADR 0002): two signage lines.
struct SignageEngine;
impl OcrEngine for SignageEngine {
    fn name(&self) -> &str {
        "test-signage"
    }
    fn recognize(&self, image: &[u8]) -> Result<OcrInfo, String> {
        if image.is_empty() {
            return Err("empty image".to_string());
        }
        let region = |text: &str, y: f32, confidence: f32| TextRegion {
            text: text.to_string(),
            confidence,
            bbox: BoundingBox { x: 0.15, y, width: 0.7, height: 0.12 },
        };
        Ok(OcrInfo {
            combined_text: "Brandenburger Tor Unter den Linden".to_string(),
            regions: vec![region("Brandenburger Tor", 0.2, 0.96), region("Unter den Linden", 0.38, 0.92)],
            primary_language: Some("de".to_string()),
        })
    }
}

fn image_doc(id: &str, dir: &std::path::Path) -> UnifiedDocument {
    let path = common::write_file(dir, "image.jpg", &common::plain_jpeg(64, 48));
    UnifiedDocument::new(
        id,
        FileInfo {
            path,
            filename: "image.jpg".to_string(),
            size_bytes: 0,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    )
}

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

// ocr-text AC-3: the plugin takes text lines with bounding boxes and confidence scores from an exchangeable OcrEngine
#[test]
fn test_ac3_text_detection_and_recognition_pipeline() {
    let dir = common::temp_dir("ocr_ac3");
    let plugin = OcrTextPlugin::with_engine(Box::new(SignageEngine));
    let ctx = PluginContext::new(Arc::new(NoopCapabilities));
    let ocr_info = plugin.process(image_doc("photo-ocr-3", &dir), &ctx).unwrap().ocr.expect("ocr result");

    assert!(!ocr_info.combined_text.is_empty());
    assert_eq!(ocr_info.regions.len(), 2);
    for region in &ocr_info.regions {
        assert!(!region.text.is_empty());
        assert!(region.confidence > 0.8);
        assert!(region.bbox.width > 0.0);
        assert!(region.bbox.height > 0.0);
    }
    let _ = std::fs::remove_dir_all(&dir);
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

// ocr-text AC-5: Integration tests verify that recognised text is persisted and found via full-text search
#[test]
fn test_ac5_integration_signage_transcription() {
    let dir = common::temp_dir("ocr_ac5");
    let plugin = OcrTextPlugin::with_engine(Box::new(SignageEngine));
    let ctx = PluginContext::new(Arc::new(NoopCapabilities));

    let processed = plugin.process(image_doc("photo-ocr-berlin", &dir), &ctx).expect("Plugin process failed");
    let ocr = processed.ocr.clone().expect("ocr result");
    assert!(ocr.combined_text.contains("Brandenburger Tor"));
    assert_eq!(ocr.regions[0].text, "Brandenburger Tor");

    let db_path = dir.join("ocr.db");
    let storage = Storage::new(db_path.to_str().unwrap());
    storage.init_schema().expect("Schema init failed");
    storage.save_document(&processed).expect("Save failed");
    let hits = storage.search("Brandenburger").expect("Search failed");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].photo_id, "photo-ocr-berlin");
    let _ = std::fs::remove_dir_all(&dir);
}
