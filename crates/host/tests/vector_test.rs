use flagme_host::Storage;
use flagme_plugin_ai_tagging::AiTaggingPlugin;
use flagme_sdk::{NoopCapabilities, Plugin, PluginContext};
use std::sync::Arc;
use wasm_geo_models::{AiInfo, AiTag, FileInfo, UnifiedDocument};

// vector-search-embeddings AC-1: AiInfo in wasm-geo-models supports optional dense vector embeddings
#[test]
fn test_ac1_model_supports_embeddings() {
    let mut ai = AiInfo::default();
    assert!(ai.embedding.is_none());

    let vector = vec![0.1f32; 512];
    ai.embedding = Some(vector.clone());

    let json = serde_json::to_string(&ai).expect("Serialization failed");
    let deserialized: AiInfo = serde_json::from_str(&json).expect("Deserialization failed");

    assert_eq!(deserialized.embedding.as_ref().map(|v| v.len()), Some(512));
    assert_eq!(deserialized.embedding.unwrap()[0], 0.1f32);
}

// vector-search-embeddings AC-2: ai-tagging plugin optionally exports normalized 512-D float vectors
#[test]
fn test_ac2_plugin_exports_normalized_embeddings() {
    let plugin = AiTaggingPlugin::new();
    let doc = UnifiedDocument::new(
        "doc-emb-1",
        FileInfo {
            path: "test/scenery.jpg".to_string(),
            filename: "scenery.jpg".to_string(),
            size_bytes: 4096,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );

    let ctx = PluginContext::new(Arc::new(NoopCapabilities));

    let processed = plugin.process(doc, &ctx).expect("Plugin process failed");
    assert!(processed.ai.is_some());
    let ai = processed.ai.unwrap();
    assert!(ai.embedding.is_some(), "Plugin must generate embedding");

    let emb = ai.embedding.unwrap();
    assert_eq!(emb.len(), 512, "Embedding vector must be 512-dimensional");

    // Check normalization (L2-norm should be close to 1.0)
    let norm_sq: f32 = emb.iter().map(|v| v * v).sum();
    assert!((norm_sq.sqrt() - 1.0).abs() < 1e-4, "Embedding must be normalized");
}

// vector-search-embeddings AC-3: Storage persists and reads binary embedding BLOBs from photo_embeddings
#[test]
fn test_ac3_storage_persists_and_reads_embeddings() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join(format!("test_vec_ac3_{}.db", std::process::id()));
    let db_str = db_path.to_str().unwrap();

    let storage = Storage::new(db_str);
    storage.init_schema().expect("Schema init failed");

    let mut doc = UnifiedDocument::new(
        "photo-emb-1",
        FileInfo {
            path: "/media/photo_emb1.jpg".to_string(),
            filename: "photo_emb1.jpg".to_string(),
            size_bytes: 1024,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );

    let mut vector = vec![0.0f32; 512];
    vector[0] = 1.0; // unit vector on axis 0
    doc.ai = Some(AiInfo {
        tags: vec![AiTag {
            label: "test".to_string(),
            confidence: 1.0,
            model: "CLIP-ViT-B/32".to_string(),
        }],
        description: "Test unit vector".to_string(),
        embedding: Some(vector.clone()),
    });

    storage.save_document(&doc).expect("Save document failed");

    // Read back embedding
    let retrieved = storage.get_embedding("photo-emb-1").expect("Embedding not retrieved");
    assert_eq!(retrieved.len(), 512);
    assert!((retrieved[0] - 1.0).abs() < 1e-6);
    assert!((retrieved[1] - 0.0).abs() < 1e-6);

    let _ = std::fs::remove_file(&db_path);
}

// vector-search-embeddings AC-4: CLI subcommand flagme-host similar finds top-K visually and semantically similar photos
#[test]
fn test_ac4_find_similar_cosine_ranking() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join(format!("test_vec_ac4_{}.db", std::process::id()));
    let db_str = db_path.to_str().unwrap();

    let storage = Storage::new(db_str);
    storage.init_schema().expect("Schema init failed");

    // Base target photo: Unit vector along axis 0
    let mut doc_target = UnifiedDocument::new(
        "target",
        FileInfo {
            path: "/media/target.jpg".to_string(),
            filename: "target.jpg".to_string(),
            size_bytes: 1000,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    let mut v_target = vec![0.0f32; 512];
    v_target[0] = 1.0;
    doc_target.ai = Some(AiInfo {
        tags: vec![],
        description: "".to_string(),
        embedding: Some(v_target),
    });
    storage.save_document(&doc_target).unwrap();

    // Close photo: Unit vector close to axis 0 (high cosine similarity)
    let mut doc_close = UnifiedDocument::new(
        "close",
        FileInfo {
            path: "/media/close.jpg".to_string(),
            filename: "close.jpg".to_string(),
            size_bytes: 1000,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    let mut v_close = vec![0.0f32; 512];
    v_close[0] = 0.95;
    v_close[1] = 0.3122; // normalized approx
    doc_close.ai = Some(AiInfo {
        tags: vec![],
        description: "".to_string(),
        embedding: Some(v_close),
    });
    storage.save_document(&doc_close).unwrap();

    // Far photo: Orthogonal vector along axis 50 (cosine similarity 0)
    let mut doc_far = UnifiedDocument::new(
        "far",
        FileInfo {
            path: "/media/far.jpg".to_string(),
            filename: "far.jpg".to_string(),
            size_bytes: 1000,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    let mut v_far = vec![0.0f32; 512];
    v_far[50] = 1.0;
    doc_far.ai = Some(AiInfo {
        tags: vec![],
        description: "".to_string(),
        embedding: Some(v_far),
    });
    storage.save_document(&doc_far).unwrap();

    let results = storage.find_similar("target", 10).expect("find_similar failed");
    assert_eq!(results.len(), 2);
    // Highest similarity must be "close"
    assert_eq!(results[0].photo_id, "close");
    assert!(results[0].score > 0.9, "Score should be > 0.9, was {}", results[0].score);

    // Second is "far"
    assert_eq!(results[1].photo_id, "far");
    assert!(results[1].score < 0.1, "Score should be approx 0, was {}", results[1].score);

    let _ = std::fs::remove_file(&db_path);
}
