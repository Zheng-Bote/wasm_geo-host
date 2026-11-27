//! Built-in YuNet face detector (ADR 0009) on public-domain test images.

mod common;

use flagme_host::config::PluginConfig;
use flagme_host::{AppConfig, PluginRuntime};
use flagme_plugin_face_detection::yunet::{YuNetDetector, MODEL, MODEL_NAME};
use flagme_plugin_face_detection::FaceDetectionPlugin;
use flagme_sdk::{DetectedFace, FileInfo, Plugin, PluginContext, UnifiedDocument};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

fn fixture(name: &str) -> PathBuf {
    common::repo_root().join("crates/host/tests/fixtures/faces").join(name)
}

fn doc(path: &str) -> UnifiedDocument {
    UnifiedDocument::new(
        "face-doc",
        FileInfo {
            path: path.to_string(),
            filename: std::path::Path::new(path).file_name().unwrap().to_string_lossy().to_string(),
            size_bytes: 0,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    )
}

/// Runs the plugin natively with real file access; returns the faces and the warnings.
fn detect(path: &str, settings: &[(&str, &str)]) -> (Option<Vec<DetectedFace>>, Vec<String>) {
    let caps = Arc::new(common::LogCaps::default());
    let map: HashMap<String, String> = settings.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let ctx = PluginContext::with_settings(caps.clone(), map);
    let out = FaceDetectionPlugin::new().process(doc(path), &ctx).unwrap();
    let warnings = caps.logs.lock().unwrap().iter().filter(|(l, _)| l == "warn").map(|(_, m)| m.clone()).collect();
    (out.faces.map(|f| {
        assert_eq!(f.detection_model, MODEL_NAME);
        f.faces
    }), warnings)
}

fn assert_well_formed(faces: &[DetectedFace]) {
    for f in faces {
        let b = &f.bbox;
        assert!(f.confidence >= 0.9, "{:?}", f);
        assert!(b.x >= 0.0 && b.y >= 0.0 && b.width > 0.0 && b.height > 0.0, "{:?}", b);
        assert!(b.x + b.width <= 1.0 + 1e-6 && b.y + b.height <= 1.0 + 1e-6, "{:?}", b);
        let l = f.landmarks.as_ref().expect("landmarks");
        for (x, y) in [l.left_eye, l.right_eye, l.nose, l.mouth_left, l.mouth_right] {
            assert!(x >= b.x && x <= b.x + b.width && y >= b.y && y <= b.y + b.height, "point {:?} outside {:?}", (x, y), b);
        }
        assert!(l.left_eye.0 <= l.right_eye.0 && l.mouth_left.0 <= l.mouth_right.0, "{:?}", l);
        assert!(f.embedding.is_none() && f.person_id.is_none());
    }
}

// face-detection-yunet AC-1: on the Apollo 11 crew photo face-detection finds exactly 3 faces, on the Expedition 64 portrait exactly 7, each with confidence >= 0.9, a box within 0..1 and five landmarks inside it
#[test]
fn test_ac1_faces_in_public_domain_photos() {
    for (name, expected) in [("apollo11.jpg", 3), ("expedition64.jpg", 7)] {
        let (faces, warnings) = detect(fixture(name).to_str().unwrap(), &[]);
        let faces = faces.unwrap_or_else(|| panic!("{}: no result, {:?}", name, warnings));
        assert_eq!(faces.len(), expected, "{}: {:?}", name, faces);
        assert_well_formed(&faces);
        assert!(warnings.is_empty(), "{:?}", warnings);
    }
}

// face-detection-yunet AC-1: the face-detection component in the host finds the same faces as the native plugin
#[test]
fn test_ac1_component_matches() {
    let path = fixture("apollo11.jpg").to_string_lossy().to_string();
    let native = detect(&path, &[]).0.unwrap();
    let mut config = common::with_components(AppConfig::default());
    config.component_cache = false;
    config.pipeline = vec!["face-detection".to_string()];
    let rt = PluginRuntime::new(config).unwrap();
    let out = rt.execute_plugin("face-detection", doc(&path)).unwrap();
    let faces = out.faces.expect("faces").faces;
    assert_eq!(faces.len(), native.len());
    for (a, b) in faces.iter().zip(&native) {
        assert_eq!(a.id, b.id);
        assert!((a.bbox.x - b.bbox.x).abs() < 1e-3 && (a.bbox.y - b.bbox.y).abs() < 1e-3, "{:?} vs {:?}", a.bbox, b.bbox);
        assert!((a.confidence - b.confidence).abs() < 1e-3);
    }
}

// face-detection-yunet AC-2: an image without a face gives Some with an empty list and detection_model yunet-2023mar
#[test]
fn test_ac2_no_face() {
    let path = common::repo_root().join("Examples/sample.jpg");
    let (faces, warnings) = detect(path.to_str().unwrap(), &[]);
    assert_eq!(faces, Some(vec![]), "{:?}", warnings);
}

// face-detection-yunet AC-3: an unreadable file, a truncated JPEG or an unsupported format (HEIC) leave doc.faces None and warn with path and cause
#[test]
fn test_ac3_unreadable_and_unsupported() {
    let dir = common::temp_dir("yunet_ac3");
    let full = std::fs::read(fixture("apollo11.jpg")).unwrap();
    let truncated = common::write_file(&dir, "truncated.jpg", &full[..full.len() / 3]);
    let heic = common::repo_root().join("Examples/sample.heic").to_string_lossy().to_string();
    let missing = dir.join("missing.jpg").to_string_lossy().to_string();
    for (path, cause) in [(missing.as_str(), "cannot read"), (truncated.as_str(), "cannot decode"), (heic.as_str(), "format")] {
        let (faces, warnings) = detect(path, &[]);
        assert!(faces.is_none(), "{}: {:?}", path, faces);
        assert!(
            warnings.iter().any(|w| w.starts_with("face-detection:") && w.contains(path) && w.contains(cause)),
            "{}: {:?}",
            path,
            warnings
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// face-detection-yunet AC-4: ids and order are deterministic (face-1... in reading order), the same image twice gives the same result, min_confidence still filters
#[test]
fn test_ac4_deterministic_ids_and_min_confidence() {
    let path = fixture("expedition64.jpg").to_string_lossy().to_string();
    let a = detect(&path, &[]).0.unwrap();
    let b = detect(&path, &[]).0.unwrap();
    assert_eq!(a, b);
    let ids: Vec<&str> = a.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, (1..=7).map(|n| format!("face-{}", n)).collect::<Vec<_>>().iter().map(String::as_str).collect::<Vec<_>>());
    for w in a.windows(2) {
        assert!((w[0].bbox.y, w[0].bbox.x) <= (w[1].bbox.y, w[1].bbox.x), "{:?} before {:?}", w[0].bbox, w[1].bbox);
    }
    let lowest = a.iter().map(|f| f.confidence).fold(f32::INFINITY, f32::min);
    let strict = format!("{}", lowest + 1e-4);
    let filtered = detect(&path, &[("min_confidence", &strict)]).0.unwrap();
    assert!(filtered.len() < a.len() && filtered.iter().all(|f| f.confidence > lowest));
}

// face-detection-yunet AC-5: through the host, face-detection and privacy-redact write the Apollo 11 photo with 3 redacted faces; pixels inside the boxes differ from the original, pixels far outside do not
#[test]
fn test_ac5_privacy_redact_redacts_detected_faces() {
    let dir = common::temp_dir("yunet_ac5");
    let src = fixture("apollo11.jpg").to_string_lossy().to_string();
    let mut config = common::with_components(AppConfig::default());
    config.component_cache = false;
    config.pipeline = vec!["face-detection".to_string(), "privacy-redact".to_string()];
    let pr = config.plugins.get_mut("privacy-redact").unwrap();
    pr.settings.insert("output_dir".to_string(), dir.join("redacted").to_string_lossy().to_string());
    pr.settings.insert("style".to_string(), "blackout".to_string());
    let rt = PluginRuntime::new(config).unwrap();
    let d = rt.execute_plugin("face-detection", doc(&src)).unwrap();
    let faces = d.faces.clone().unwrap().faces;
    let d = rt.execute_plugin("privacy-redact", d).unwrap();
    let privacy = d.privacy.unwrap();
    assert!(privacy.is_redacted);
    assert_eq!(privacy.faces_redacted_count, 3);
    let out = privacy.redacted_image_path.expect("redacted image");
    assert_ne!(out, src);

    let original = image::open(&src).unwrap().to_rgb8();
    let redacted = image::open(&out).unwrap().to_rgb8();
    assert_eq!(original.dimensions(), redacted.dimensions());
    let (w, h) = (original.width() as f32, original.height() as f32);
    for f in &faces {
        let cx = ((f.bbox.x + f.bbox.width / 2.0) * w) as u32;
        let cy = ((f.bbox.y + f.bbox.height / 2.0) * h) as u32;
        assert_eq!(redacted.get_pixel(cx, cy).0, [0, 0, 0], "face centre of {} is not blacked out", f.id);
        assert_ne!(original.get_pixel(cx, cy).0, [0, 0, 0]);
    }
    // A corner far away from every face keeps (almost) its original colour; JPEG re-encoding may shift it a little.
    let (o, r) = (original.get_pixel(5, 5).0, redacted.get_pixel(5, 5).0);
    assert!(o.iter().zip(r.iter()).all(|(a, b)| (*a as i32 - *b as i32).abs() <= 8), "{:?} vs {:?}", o, r);
    let _ = std::fs::remove_dir_all(&dir);
}

// face-detection-yunet AC-6: the model is embedded, its SHA-256 matches models/SOURCE.md, the licence file is next to it, and NOTICE and the README name YuNet, OpenCV Zoo and MIT
#[test]
fn test_ac6_embedded_model_and_attribution() {
    let root = common::repo_root();
    let models = root.join("plugins/face-detection/models");
    let hash = format!("{:x}", Sha256::digest(MODEL));
    let source = std::fs::read_to_string(models.join("SOURCE.md")).unwrap();
    assert!(source.contains(&hash), "SOURCE.md lacks {}", hash);
    assert_eq!(std::fs::read(models.join("face_detection_yunet_2023mar.onnx")).unwrap(), MODEL);
    assert!(std::fs::read_to_string(models.join("LICENSE")).unwrap().starts_with("MIT License"));
    let src = std::fs::read_to_string(root.join("plugins/face-detection/src/yunet.rs")).unwrap();
    assert!(src.contains("include_bytes!(\"../models/face_detection_yunet_2023mar.onnx\")"));
    assert!(YuNetDetector::new().is_ok());
    for file in ["NOTICE", "plugins/face-detection/README.md"] {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        assert!(text.contains("YuNet") && text.contains("OpenCV Zoo") && text.contains("MIT"), "{} lacks the attribution", file);
    }
}

// face-detection-yunet AC-7: a 4000 x 3000 image (generated in the test) runs through the host within the default limits (256 MB, 10 s)
#[test]
fn test_ac7_large_image_within_default_limits() {
    let dir = common::temp_dir("yunet_ac7");
    let big = image::imageops::resize(&image::open(fixture("apollo11.jpg")).unwrap().to_rgb8(), 4000, 3000, image::imageops::FilterType::Triangle);
    let path = dir.join("big.jpg");
    big.save(&path).unwrap();
    let mut config = common::with_components(AppConfig::default());
    config.component_cache = false;
    config.pipeline = vec!["face-detection".to_string()];
    assert_eq!((config.resource_limits.max_memory_bytes, config.resource_limits.timeout_ms), (256 * 1024 * 1024, 10_000));
    let rt = PluginRuntime::new(config).unwrap();
    let start = Instant::now();
    let out = rt.execute_plugin("face-detection", doc(path.to_str().unwrap())).unwrap();
    eprintln!("4000x3000 via host: {:?}", start.elapsed());
    assert_eq!(out.faces.expect("faces within the default limits").faces.len(), 3);
    let _ = std::fs::remove_dir_all(&dir);
}

// face-detection-yunet AC-8: face-detection is enabled in config.yaml and config.mac-studio.yaml without the experimental note; a process run with config.yaml no longer warns "no face detection result"
#[test]
fn test_ac8_enabled_in_shipped_configs() {
    let root = common::repo_root();
    for name in ["config.yaml", "config.mac-studio.yaml"] {
        let cfg = AppConfig::load_from_file(root.join(name)).unwrap();
        assert!(cfg.plugins["face-detection"].enabled, "{}", name);
        let text = std::fs::read_to_string(root.join(name)).unwrap();
        let section = text.split("face-detection:").nth(1).unwrap().split("privacy-redact:").next().unwrap();
        assert!(!section.contains("experimental"), "{}: {}", name, section);
    }
    assert!(AppConfig::default().plugins["face-detection"].enabled);

    let dir = common::temp_dir("yunet_ac8");
    let config: String = std::fs::read_to_string(root.join("config.yaml"))
        .unwrap()
        .lines()
        .map(|l| {
            if l.starts_with("database_path:") {
                format!("database_path: \"{}\"", dir.join("media.db").display())
            } else if l.starts_with("plugin_dir:") {
                format!("plugin_dir: \"{}\"", common::components_dir().display())
            } else if l.starts_with("component_cache:") {
                "component_cache: false".to_string()
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(dir.join("config.yaml"), config).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_flagme-host"))
        .args(["process", fixture("apollo11.jpg").to_str().unwrap(), "--config", "config.yaml"])
        .current_dir(&dir)
        .env_remove("RUST_LOG")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{}", stderr);
    assert!(!stderr.contains("no face detection result"), "{}", stderr);
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(doc["faces"]["faces"].as_array().map(|a| a.len()), Some(3));
    assert_eq!(doc["privacy"]["faces_redacted_count"], 3);
    let _ = std::fs::remove_dir_all(&dir);
}

// face-detection-yunet AC-9: .cargo/config.toml enables +simd128 for wasm32-wasip2 and all components still export the media-plugin world
#[test]
fn test_ac9_simd_for_components() {
    let text = std::fs::read_to_string(common::repo_root().join(".cargo/config.toml")).unwrap();
    let section = text.split("[target.wasm32-wasip2]").nth(1).expect("wasm32-wasip2 section");
    assert!(section.contains("target-feature=+simd128"), "{}", section);
    // Loading checks the media-plugin world of every component (ADR 0003).
    let mut config = common::with_components(AppConfig::default());
    config.component_cache = false;
    let names = common::plugin_names();
    config.pipeline = names.clone();
    for name in &names {
        config.plugins.entry(name.clone()).or_insert_with(|| PluginConfig {
            enabled: true,
            path: None,
            capabilities: vec![],
            settings: HashMap::new(),
            resource_limits: HashMap::new(),
        });
        config.plugins.get_mut(name).unwrap().enabled = true;
    }
    let rt = PluginRuntime::new(config).expect("all SIMD components load");
    assert_eq!(rt.registered_plugin_names().len(), names.len());
}

// face-detection-yunet AC-10: the fixture SOURCES.md names source URL, author and licence (public domain) and the edit (downscaled to 1600 px) for every test image
#[test]
fn test_ac10_fixture_sources() {
    let dir = common::repo_root().join("crates/host/tests/fixtures/faces");
    let sources = std::fs::read_to_string(dir.join("SOURCES.md")).unwrap();
    assert!(sources.contains("1600 px") && sources.contains("Public domain"));
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".jpg") {
            continue;
        }
        let line = sources.lines().find(|l| l.contains(&format!("`{}`", name)) && l.contains("commons.wikimedia.org")).unwrap_or_else(|| panic!("{} has no source line", name));
        assert!(line.contains("NASA") && line.contains("Public domain"), "{}", line);
        let hash = format!("{:x}", Sha256::digest(std::fs::read(entry.path()).unwrap()));
        assert!(sources.contains(&hash), "{}: SHA-256 {} not listed", name, hash);
    }
}
