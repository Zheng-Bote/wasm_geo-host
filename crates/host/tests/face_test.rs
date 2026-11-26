mod common;

use flagme_host::Storage;
use flagme_plugin_face_detection::{FaceDetectionPlugin, FaceDetector};
use flagme_sdk::{NoopCapabilities, Plugin, PluginContext};
use std::sync::Arc;
use wasm_geo_models::{BoundingBox, DetectedFace, FaceLandmarks, FacesInfo, FileInfo, UnifiedDocument};

// face-detection AC-1: wasm-geo-models introduces FacesInfo, DetectedFace, BoundingBox and FaceLandmarks with complete serde compatibility
#[test]
fn test_ac1_face_models_serialization() {
    let face = DetectedFace {
        id: "face-test-1".to_string(),
        bbox: BoundingBox {
            x: 0.25,
            y: 0.15,
            width: 0.50,
            height: 0.60,
        },
        confidence: 0.99,
        landmarks: Some(FaceLandmarks {
            left_eye: (0.35, 0.30),
            right_eye: (0.65, 0.30),
            nose: (0.50, 0.45),
            mouth_left: (0.38, 0.60),
            mouth_right: (0.62, 0.60),
        }),
        embedding: Some(vec![0.1f32; 128]),
        person_id: Some("person-42".to_string()),
    };

    let faces_info = FacesInfo {
        faces: vec![face],
        detection_model: "UltraFace-Slim".to_string(),
    };

    let serialized = serde_json::to_string(&faces_info).expect("Serialization failed");
    let deserialized: FacesInfo = serde_json::from_str(&serialized).expect("Deserialization failed");

    assert_eq!(faces_info, deserialized);
    assert_eq!(deserialized.faces.len(), 1);
    assert_eq!(deserialized.faces[0].landmarks.as_ref().unwrap().nose, (0.50, 0.45));
}

// face-detection AC-2: flagme-plugin-face-detection compiles as plugin crate with valid metadata
#[test]
fn test_ac2_plugin_contract_metadata() {
    let plugin = FaceDetectionPlugin::new();
    assert_eq!(plugin.name(), "face-detection");
    assert_eq!(plugin.version(), env!("CARGO_PKG_VERSION"), "all crates share the workspace version");
}

// face-detection AC-4: Relational tables faces and persons are created in media.db and populated via Storage::save_document
#[test]
fn test_ac4_sqlite_face_persistence() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join(format!("test_face_ac4_{}.db", std::process::id()));
    let db_str = db_path.to_str().unwrap();

    let storage = Storage::new(db_str);
    storage.init_schema().expect("Schema init failed");

    let mut doc = UnifiedDocument::new(
        "photo-family-sql",
        FileInfo {
            path: "/media/family.jpg".to_string(),
            filename: "family.jpg".to_string(),
            size_bytes: 4096,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );

    doc.faces = Some(FacesInfo {
        faces: vec![DetectedFace {
            id: "face-p1".to_string(),
            bbox: BoundingBox {
                x: 0.2,
                y: 0.3,
                width: 0.25,
                height: 0.35,
            },
            confidence: 0.96,
            landmarks: None,
            embedding: Some(vec![0.2f32; 128]),
            person_id: None,
        }],
        detection_model: "UltraFace-Slim".to_string(),
    });

    storage.save_document(&doc).expect("Save document failed");

    // Retrieve via Storage::get_document
    let loaded = storage.get_document("photo-family-sql").expect("Photo not found");
    assert!(loaded.faces.is_some());
    let faces = loaded.faces.unwrap();
    assert_eq!(faces.faces.len(), 1);
    assert_eq!(faces.faces[0].id, "face-p1");
    assert_eq!(faces.faces[0].confidence, 0.96);

    let _ = std::fs::remove_file(&db_path);
}

/// Test detector standing in for a real face detector (ADR 0002): two faces with landmarks.
struct TwoFaceDetector;
impl FaceDetector for TwoFaceDetector {
    fn model_name(&self) -> &str {
        "test-two-faces"
    }
    fn detect(&self, image: &[u8]) -> Result<Vec<DetectedFace>, String> {
        if image.is_empty() {
            return Err("empty image".to_string());
        }
        let face = |id: &str, x: f32| DetectedFace {
            id: id.to_string(),
            bbox: BoundingBox { x, y: 0.25, width: 0.25, height: 0.35 },
            confidence: 0.93,
            landmarks: Some(FaceLandmarks {
                left_eye: (x + 0.07, 0.35),
                right_eye: (x + 0.17, 0.35),
                nose: (x + 0.12, 0.42),
                mouth_left: (x + 0.08, 0.50),
                mouth_right: (x + 0.16, 0.50),
            }),
            embedding: None,
            person_id: None,
        };
        Ok(vec![face("face-1", 0.15), face("face-2", 0.60)])
    }
}

// face-detection AC-5: Integration tests verify that faces, bounding boxes and landmarks of a FaceDetector are taken over
#[test]
fn test_ac5_integration_face_detection() {
    let dir = common::temp_dir("face_ac5");
    let path = common::write_file(&dir, "image.jpg", &common::plain_jpeg(64, 48));
    let plugin = FaceDetectionPlugin::with_detector(Box::new(TwoFaceDetector));
    let ctx = PluginContext::new(Arc::new(NoopCapabilities));

    let doc = UnifiedDocument::new(
        "photo-group-shot",
        FileInfo {
            path,
            filename: "image.jpg".to_string(),
            size_bytes: 0,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );

    let processed = plugin.process(doc, &ctx).expect("Processing failed");
    let faces_info = processed.faces.expect("faces result");
    assert_eq!(faces_info.detection_model, "test-two-faces");
    assert_eq!(faces_info.faces.len(), 2);
    for face in &faces_info.faces {
        assert!(face.bbox.width > 0.0 && face.bbox.height > 0.0);
        assert!(face.landmarks.is_some());
    }
    let _ = std::fs::remove_dir_all(&dir);
}
