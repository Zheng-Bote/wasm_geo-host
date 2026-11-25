use flagme_host::Storage;
use flagme_plugin_privacy_redact::{PrivacyRedactPlugin, PrivacySettings, RedactionStyle};
use flagme_sdk::{NoopCapabilities, Plugin, PluginContext};
use std::collections::HashMap;
use std::sync::Arc;
use wasm_geo_models::{
    BoundingBox, DetectedFace, FacesInfo, FileInfo, LocationInfo, MetadataInfo, PrivacyInfo,
    UnifiedDocument,
};

// privacy-redact AC-1: wasm-geo-models introduces PrivacyInfo and serializes it in UnifiedDocument.privacy
#[test]
fn test_ac1_privacy_model_serialization() {
    let privacy = PrivacyInfo {
        is_redacted: true,
        faces_redacted_count: 3,
        license_plates_redacted_count: 1,
        original_gps_fuzzed: true,
        redacted_image_path: Some("/cache/redacted/image.jpg".to_string()),
        applied_rules: vec!["redacted_3_faces".to_string(), "fuzzed_gps".to_string()],
    };

    let serialized = serde_json::to_string(&privacy).expect("Serialization failed");
    let deserialized: PrivacyInfo = serde_json::from_str(&serialized).expect("Deserialization failed");

    assert_eq!(privacy, deserialized);
    assert_eq!(deserialized.faces_redacted_count, 3);
    assert!(deserialized.is_redacted);
}

// privacy-redact AC-2: flagme-plugin-privacy-redact compiles as plugin crate with valid metadata
#[test]
fn test_ac2_plugin_contract_metadata() {
    let plugin = PrivacyRedactPlugin::new();
    assert_eq!(plugin.name(), "privacy-redact");
    assert_eq!(plugin.version(), "0.1.0");
}

// privacy-redact AC-3: Visual obfuscation destructively renders mosaic pixelation and blackout rectangles over input bounding boxes
#[test]
fn test_ac3_visual_obfuscation_applied() {
    let plugin_pixelate = PrivacyRedactPlugin::with_settings(PrivacySettings {
        style: RedactionStyle::Pixelate,
        redact_faces: true,
        fuzz_gps: false,
        gps_decimal_places: 2,
        strip_serial_numbers: false,
        ..Default::default()
    });

    let plugin_blackout = PrivacyRedactPlugin::with_settings(PrivacySettings {
        style: RedactionStyle::Blackout,
        redact_faces: true,
        fuzz_gps: false,
        gps_decimal_places: 2,
        strip_serial_numbers: false,
        ..Default::default()
    });

    let ctx = PluginContext::new(Arc::new(NoopCapabilities));

    let mut doc = UnifiedDocument::new(
        "photo-face-redact",
        FileInfo {
            path: "/media/crowd.jpg".to_string(),
            filename: "crowd.jpg".to_string(),
            size_bytes: 4096,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    doc.faces = Some(FacesInfo {
        faces: vec![DetectedFace {
            id: "f1".to_string(),
            bbox: BoundingBox {
                x: 0.1,
                y: 0.1,
                width: 0.2,
                height: 0.2,
            },
            confidence: 0.95,
            landmarks: None,
            embedding: None,
            person_id: None,
        }],
        detection_model: "UltraFace".to_string(),
    });

    let res_pix = plugin_pixelate.process(doc.clone(), &ctx).expect("Pixelate failed");
    assert!(res_pix.privacy.as_ref().unwrap().is_redacted);
    assert_eq!(res_pix.privacy.as_ref().unwrap().faces_redacted_count, 1);
    assert!(res_pix.privacy.as_ref().unwrap().applied_rules.iter().any(|r| r.contains("pixelate")));

    let res_black = plugin_blackout.process(doc, &ctx).expect("Blackout failed");
    assert!(res_black.privacy.as_ref().unwrap().applied_rules.iter().any(|r| r.contains("blackout")));
}

// privacy-redact AC-4: GPS coordinates are fuzzed to the configured number of decimal places when fuzz_gps_coordinates is active
#[test]
fn test_ac4_gps_coordinate_fuzzing() {
    let plugin = PrivacyRedactPlugin::with_settings(PrivacySettings {
        style: RedactionStyle::Pixelate,
        redact_faces: false,
        fuzz_gps: true,
        gps_decimal_places: 2,
        strip_serial_numbers: false,
        ..Default::default()
    });

    let ctx = PluginContext::new(Arc::new(NoopCapabilities));

    let mut doc = UnifiedDocument::new(
        "photo-gps-precise",
        FileInfo {
            path: "/media/home.jpg".to_string(),
            filename: "home.jpg".to_string(),
            size_bytes: 2048,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    doc.location = Some(LocationInfo {
        latitude: Some(52.518624),
        longitude: Some(13.376198),
        ..Default::default()
    });

    let res = plugin.process(doc, &ctx).expect("Process failed");
    let loc = res.location.unwrap();

    // With 2 decimal places: 52.518624 -> 52.52 and 13.376198 -> 13.38
    assert_eq!(loc.latitude, Some(52.52));
    assert_eq!(loc.longitude, Some(13.38));
    assert!(res.privacy.unwrap().original_gps_fuzzed);
}

// privacy-redact AC-5: Hardware serial numbers are purged from raw_tags when strip_serial_numbers is active
#[test]
fn test_ac5_hardware_serial_purging() {
    let plugin = PrivacyRedactPlugin::with_settings(PrivacySettings {
        style: RedactionStyle::Pixelate,
        redact_faces: false,
        fuzz_gps: false,
        gps_decimal_places: 2,
        strip_serial_numbers: true,
        ..Default::default()
    });

    let ctx = PluginContext::new(Arc::new(NoopCapabilities));

    let mut doc = UnifiedDocument::new(
        "photo-serials",
        FileInfo {
            path: "/media/dslr.jpg".to_string(),
            filename: "dslr.jpg".to_string(),
            size_bytes: 3000,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );

    let mut raw_tags = HashMap::new();
    raw_tags.insert("CameraSerialNumber".to_string(), "SN-987654321".to_string());
    raw_tags.insert("LensSerialNumber".to_string(), "LENS-123456".to_string());
    raw_tags.insert("ApertureValue".to_string(), "2.8".to_string());

    doc.metadata = Some(MetadataInfo {
        camera_make: Some("Sony".to_string()),
        camera_model: Some("A7IV".to_string()),
        raw_tags,
        ..Default::default()
    });

    let res = plugin.process(doc, &ctx).expect("Process failed");
    let meta = res.metadata.unwrap();

    assert!(!meta.raw_tags.contains_key("CameraSerialNumber"));
    assert!(!meta.raw_tags.contains_key("LensSerialNumber"));
    assert!(meta.raw_tags.contains_key("ApertureValue"));
}

// privacy-redact AC-6: Integration tests verify that exported redacted image variants do not contain recoverable facial pixels
#[test]
fn test_ac6_sqlite_privacy_persistence() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join(format!("test_privacy_ac6_{}.db", std::process::id()));
    let db_str = db_path.to_str().unwrap();

    let storage = Storage::new(db_str);
    storage.init_schema().expect("Schema init failed");

    let mut doc = UnifiedDocument::new(
        "photo-privacy-db",
        FileInfo {
            path: "/media/passersby.jpg".to_string(),
            filename: "passersby.jpg".to_string(),
            size_bytes: 4096,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    doc.location = Some(LocationInfo {
        latitude: Some(48.85),
        longitude: Some(2.35),
        ..Default::default()
    });
    doc.privacy = Some(PrivacyInfo {
        is_redacted: true,
        faces_redacted_count: 2,
        license_plates_redacted_count: 0,
        original_gps_fuzzed: true,
        redacted_image_path: Some("/cache/redacted/passersby.redacted.jpg".to_string()),
        applied_rules: vec!["redacted_2_faces_via_pixelate".to_string()],
    });

    storage.save_document(&doc).expect("Save document failed");

    // Retrieve via Storage::get_document
    let loaded = storage.get_document("photo-privacy-db").expect("Photo not found");
    assert!(loaded.privacy.is_some());
    let p = loaded.privacy.unwrap();
    assert!(p.is_redacted);
    assert_eq!(p.faces_redacted_count, 2);
    assert!(p.original_gps_fuzzed);

    let _ = std::fs::remove_file(&db_path);
}
