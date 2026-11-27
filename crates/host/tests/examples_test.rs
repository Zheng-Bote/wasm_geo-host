mod common;

use flagme_plugin_metadata::MetadataPlugin;
use flagme_sdk::{FileInfo, NoopCapabilities, Plugin, PluginContext, UnifiedDocument};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn sample_doc(name: &str) -> UnifiedDocument {
    let path = repo_root().join("Examples").join(name);
    UnifiedDocument::new(
        "cli-doc",
        FileInfo {
            path: path.to_string_lossy().to_string(),
            filename: name.to_string(),
            size_bytes: std::fs::metadata(&path).unwrap().len(),
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    )
}

fn ctx() -> PluginContext {
    PluginContext::new(Arc::new(NoopCapabilities))
}

// wasm-geo-host AC-11: sample.jpg, sample.png and sample.heic carry the documented EXIF values that metadata extracts
#[test]
fn test_ac11_sample_files_carry_documented_metadata() {
    for name in ["sample.jpg", "sample.png", "sample.heic"] {
        let doc = MetadataPlugin::new().process(sample_doc(name), &ctx()).unwrap();
        let m = doc.metadata.unwrap_or_else(|| panic!("{}: no metadata", name));
        assert_eq!(m.camera_make.as_deref(), Some("Sony"), "{}", name);
        assert_eq!(m.camera_model.as_deref(), Some("ILCE-7RM4"), "{}", name);
        assert_eq!(m.lens_model.as_deref(), Some("FE 24-70mm F2.8 GM"), "{}", name);
        assert_eq!((m.iso, m.focal_length, m.f_number), (Some(100), Some(35.0), Some(2.8)), "{}", name);
        assert_eq!(m.exposure_time.as_deref(), Some("1/250 s"), "{}", name);
        assert_eq!(m.raw_tags.get("XMP.photoshop.City").map(String::as_str), Some("Berlin"), "{}", name);
        let loc = doc.location.unwrap_or_else(|| panic!("{}: no GPS", name));
        assert!((loc.latitude.unwrap() - 52.520008).abs() < 1e-6, "{}", name);
        assert!((loc.longitude.unwrap() - 13.404954).abs() < 1e-6, "{}", name);
        assert_eq!(loc.altitude, Some(34.0), "{}", name);
    }
}

// wasm-geo-host AC-11: sample.jpg is a decodable JPEG
#[test]
fn test_ac11_sample_jpg_is_decodable() {
    let bytes = std::fs::read(repo_root().join("Examples/sample.jpg")).unwrap();
    let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg).expect("decodable JPEG");
    assert_eq!((img.width(), img.height()), (64, 48));
}

/// The JSON block of section 3 in docs/pipeline_host_supervisor.md.
fn documented_output() -> Value {
    let md = std::fs::read_to_string(repo_root().join("docs/pipeline_host_supervisor.md")).unwrap();
    let section = md.split("## 3. Output").nth(1).expect("section 3");
    let json = section.split("```json\n").nth(1).unwrap().split("\n```").next().unwrap();
    serde_json::from_str(json).expect("documented output is valid JSON")
}

/// Runs `flagme-host stream Examples/sample.jpg` on the plugin components in a scratch directory
/// that mirrors the repository layout (same relative path, hence same id and cache keys).
fn real_stream_run() -> UnifiedDocument {
    let dir = common::temp_dir("examples_ac12");
    std::fs::create_dir_all(dir.join("Examples")).unwrap();
    std::fs::copy(repo_root().join("Examples/sample.jpg"), dir.join("Examples/sample.jpg")).unwrap();
    let config: String = std::fs::read_to_string(repo_root().join("config.yaml"))
        .unwrap()
        .lines()
        .map(|l| {
            if l.starts_with("database_path:") {
                format!("database_path: \"{}\"", dir.join("media.db").display())
            } else if l.starts_with("plugin_dir:") {
                format!("plugin_dir: \"{}\"", common::components_dir().display())
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(dir.join("config.yaml"), config).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_flagme-host"))
        .args(["stream", "Examples/sample.jpg", "--config", "config.yaml"])
        .current_dir(&dir)
        .env_remove("RUST_LOG")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let doc = serde_json::from_slice(&out.stdout).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    doc
}

// wasm-geo-host AC-12: the documented example output matches a real flagme-host stream run of Examples/sample.jpg
#[test]
fn test_ac12_documented_output_matches_real_run() {
    let documented = documented_output();
    let expected: UnifiedDocument = serde_json::from_value(documented.clone()).expect("documented output is a UnifiedDocument");
    let actual = real_stream_run();

    assert_eq!((&expected.id, &expected.file), (&actual.id, &actual.file), "id, file");
    let (mut doc_meta, mut real_meta) = (expected.metadata.unwrap(), actual.metadata.unwrap());
    let (doc_raw, real_raw) = (std::mem::take(&mut doc_meta.raw_tags), std::mem::take(&mut real_meta.raw_tags));
    assert_eq!(doc_meta, real_meta, "metadata");
    for (key, value) in &doc_raw {
        assert_eq!(real_raw.get(key), Some(value), "raw_tags[{}]", key);
    }

    assert_eq!(expected.location, actual.location, "location");
    assert_eq!(expected.country, actual.country, "country");
    assert_eq!(expected.thumbnails, actual.thumbnails, "thumbnails");
    assert_eq!(expected.duplicate, actual.duplicate, "duplicate");
    assert_eq!(expected.privacy, actual.privacy, "privacy");
    let (et, at) = (expected.timeline.unwrap(), actual.timeline.unwrap());
    assert_eq!((et.taken_at, et.resolution_source), (at.taken_at, at.resolution_source), "timeline");
    assert!(at.modified_at.is_some(), "fs-modified through the capability");
    // `weather` depends on the network (live Open-Meteo request) and is not compared.
    assert!(documented.get("ai").is_none(), "ai must be absent (ADR 0002)");
    assert!(actual.ai.is_none());
    // ocr-text runs the built-in PaddleOCR models (ADR 0009): checked, no text in the sample.
    assert_eq!(expected.ocr, actual.ocr, "ocr");
    // face-detection runs the built-in YuNet model (ADR 0009): checked, no face in the sample.
    assert_eq!(expected.faces, actual.faces, "faces");
}
