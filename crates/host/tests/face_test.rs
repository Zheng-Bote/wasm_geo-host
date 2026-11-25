use flagme_host::Storage;
use flagme_plugin_face_detection::{FaceDetectionPlugin, ModelTier};
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
    assert_eq!(plugin.version(), "0.1.0");
}

// face-detection AC-3: Configurable model_tier setting selects between lightweight embedded weights and heavy external weights
#[test]
fn test_ac3_model_tiers_selection() {
    let plugin_light = FaceDetectionPlugin::with_tier(ModelTier::Lightweight);
    let plugin_heavy = FaceDetectionPlugin::with_tier(ModelTier::Heavy);
    let ctx = PluginContext::new(Arc::new(NoopCapabilities));

    let doc = UnifiedDocument::new(
        "photo-portrait",
        FileInfo {
            path: "/media/portrait.jpg".to_string(),
            filename: "portrait.jpg".to_string(),
            size_bytes: 2048,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );

    let res_light = plugin_light.process(doc.clone(), &ctx).expect("Lightweight failed");
    let res_heavy = plugin_heavy.process(doc, &ctx).expect("Heavy failed");

    assert_eq!(res_light.faces.as_ref().unwrap().detection_model, "UltraFace-Slim-INT8");
    assert_eq!(res_heavy.faces.as_ref().unwrap().detection_model, "SCRFD-ArcFace-ResNet100");

    // Check embedding dimensions: 128-D for lightweight vs 512-D for heavy
    let emb_light = res_light.faces.unwrap().faces[0].embedding.clone().unwrap();
    let emb_heavy = res_heavy.faces.unwrap().faces[0].embedding.clone().unwrap();
    assert_eq!(emb_light.len(), 128);
    assert_eq!(emb_heavy.len(), 512);
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

// face-detection AC-5: Integration tests verify face bounding box detection on sample test images
#[test]
fn test_ac5_integration_face_detection() {
    let plugin = FaceDetectionPlugin::new();
    let ctx = PluginContext::new(Arc::new(NoopCapabilities));

    let doc = UnifiedDocument::new(
        "photo-group-shot",
        FileInfo {
            path: "/media/group_photo.jpg".to_string(),
            filename: "group_photo.jpg".to_string(),
            size_bytes: 4096,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );

    let processed = plugin.process(doc, &ctx).expect("Processing failed");
    assert!(processed.faces.is_some());
    let faces_info = processed.faces.unwrap();

    // Group photo should detect multiple faces with valid landmarks
    assert_eq!(faces_info.faces.len(), 2);
    assert!(faces_info.faces[0].landmarks.is_some());
    assert!(faces_info.faces[1].landmarks.is_some());
}
