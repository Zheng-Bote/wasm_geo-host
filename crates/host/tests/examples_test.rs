use flagme_plugin_countryflag::CountryFlagPlugin;
use flagme_plugin_metadata::MetadataPlugin;
use flagme_plugin_privacy_redact::{PrivacyRedactPlugin, PrivacySettings};
use flagme_plugin_reversegeo::ReverseGeoPlugin;
use flagme_plugin_timeline::TimelinePlugin;
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

// wasm-geo-host AC-12: the documented example output matches what the plugins determine from Examples/sample.jpg
#[test]
fn test_ac12_documented_output_matches_real_run() {
    let documented = documented_output();
    let out_dir = std::env::temp_dir().join(format!("examples_ac12_{}", std::process::id()));
    let privacy = PrivacyRedactPlugin::with_settings(PrivacySettings {
        output_dir: out_dir.to_string_lossy().to_string(),
        ..Default::default()
    });

    let c = ctx();
    let doc = MetadataPlugin::new().process(sample_doc("sample.jpg"), &c).unwrap();
    let doc = ReverseGeoPlugin::new().process(doc, &c).unwrap();
    let doc = CountryFlagPlugin::new().process(doc, &c).unwrap();
    let doc = TimelinePlugin::new().process(doc, &c).unwrap();
    let doc = privacy.process(doc, &c).unwrap();
    let actual = doc;
    let expected: UnifiedDocument = serde_json::from_value(documented.clone()).expect("documented output is a UnifiedDocument");

    let (mut doc_meta, mut real_meta) = (expected.metadata.unwrap(), actual.metadata.unwrap());
    let (doc_raw, real_raw) = (std::mem::take(&mut doc_meta.raw_tags), std::mem::take(&mut real_meta.raw_tags));
    assert_eq!(doc_meta, real_meta, "metadata");
    for (key, value) in &doc_raw {
        assert_eq!(real_raw.get(key), Some(value), "raw_tags[{}]", key);
    }

    assert_eq!(expected.location, actual.location, "location");
    assert_eq!(expected.country, actual.country, "country");
    assert_eq!(expected.privacy, actual.privacy, "privacy");
    let (et, at) = (expected.timeline.unwrap(), actual.timeline.unwrap());
    assert_eq!((et.taken_at, et.resolution_source), (at.taken_at, at.resolution_source), "timeline");
    for field in ["ai", "ocr", "faces"] {
        assert!(documented.get(field).is_none(), "{} must be absent (ADR 0002)", field);
    }
    let _ = std::fs::remove_dir_all(&out_dir);
}
