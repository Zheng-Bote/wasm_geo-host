//! Thumbnails only from redacted images, as AVIF (`redacted-avif-thumbnails`, ADR 0012).

mod common;

use axum::http::{header, StatusCode};
use flagme_host::{build_router, AppConfig, Pipeline, PluginRuntime, ServerState, Storage};
use flagme_plugin_privacy_redact::{PrivacyRedactPlugin, PrivacySettings};
use flagme_plugin_thumbnail::{
    render_thumbnail, ThumbnailFormat, ThumbnailPlugin, ThumbnailSettings, DEFAULT_AVIF_QUALITY,
    DEFAULT_JPEG_QUALITY,
};
use flagme_sdk::{
    display_source, pixel_source, BoundingBox, DetectedFace, FaceRedaction, FacesInfo, FileInfo,
    Plugin, PluginContext, PluginSettings, PrivacyInfo, ThumbnailEntry, UnifiedDocument, WorkingImage,
};
use image::RgbImage;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tower::ServiceExt;

fn face_fx(name: &str) -> String {
    common::repo_root().join("crates/host/tests/fixtures/faces").join(name).to_string_lossy().to_string()
}

fn doc(path: &str) -> UnifiedDocument {
    UnifiedDocument::new(
        format!("doc-{}", Path::new(path).file_name().unwrap().to_string_lossy()),
        FileInfo {
            path: path.to_string(),
            filename: Path::new(path).file_name().unwrap().to_string_lossy().to_string(),
            size_bytes: 0,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    )
}

fn one_face() -> FacesInfo {
    FacesInfo {
        faces: vec![DetectedFace {
            id: "face-1".to_string(),
            bbox: BoundingBox { x: 0.3, y: 0.2, width: 0.2, height: 0.3 },
            confidence: 0.99,
            landmarks: None,
            embedding: None,
            person_id: None,
        }],
        detection_model: "test".to_string(),
    }
}

/// Runs a plugin natively; returns the document, the warnings, the read and the written paths.
fn run(
    plugin: &dyn Plugin,
    d: UnifiedDocument,
    settings: &[(&str, &str)],
    deny_write: bool,
) -> (UnifiedDocument, Vec<String>, Vec<String>, Vec<String>) {
    let caps = Arc::new(common::LogCaps { deny_write, ..Default::default() });
    let map: HashMap<String, String> = settings.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let ctx = PluginContext::with_settings(caps.clone(), map);
    let d = plugin.process(d, &ctx).unwrap();
    let reads = caps.reads.lock().unwrap().clone();
    let writes = caps.writes.lock().unwrap().clone();
    (d, caps.warnings(), reads, writes)
}

/// Decodes an AVIF file; panics unless the `ftyp` box carries the brand `avif`.
fn load_avif(path: &str) -> RgbImage {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(&bytes[4..8], b"ftyp", "{}", path);
    let size = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    let brands: Vec<&[u8]> = std::iter::once(&bytes[8..12]).chain(bytes[16..size].chunks_exact(4)).collect();
    assert!(brands.contains(&&b"avif"[..]), "{}: no avif brand", path);
    let img = oxideav_avif::decode_rgba8(&bytes).unwrap();
    let rgb: Vec<u8> = img.data.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    RgbImage::from_raw(img.width, img.height, rgb).unwrap()
}

fn brightness(img: &RgbImage, x: u32, y: u32) -> u32 {
    img.get_pixel(x, y).0.iter().map(|c| *c as u32).sum::<u32>() / 3
}

fn component_config(dir: &Path) -> AppConfig {
    let mut config = common::with_components(AppConfig::default());
    config.component_cache = false;
    config.database_path = dir.join("media.db").to_string_lossy().to_string();
    let set = |config: &mut AppConfig, plugin: &str, key: &str, value: String| {
        config.plugins.get_mut(plugin).unwrap().settings.insert(key.to_string(), value);
    };
    set(&mut config, "image-convert", "output_dir", dir.join("working").to_string_lossy().to_string());
    set(&mut config, "privacy-redact", "output_dir", dir.join("thumbs").to_string_lossy().to_string());
    set(&mut config, "privacy-redact", "style", "blackout".to_string());
    set(&mut config, "thumbnail", "output_dir", dir.join("thumbs").to_string_lossy().to_string());
    config
}

/// Runs the whole default pipeline through the host (components) and returns the document.
fn run_pipeline(config: AppConfig, path: &str) -> UnifiedDocument {
    let storage = Arc::new(Storage::new(config.database_path.clone()));
    let runtime = PluginRuntime::new(config.clone()).unwrap();
    Pipeline::new(config, runtime, storage).process(doc(path)).unwrap()
}

// redacted-avif-thumbnails AC-1: with AppConfig::default(), every thumbnail size of apollo11.jpg shows the face centres redacted, while the thumbnail of the original does not
// redacted-avif-thumbnails AC-2: nothing is written to cache/redacted; the redacted image and all thumbnails are below the thumbnail output_dir; face_redaction is applied
#[test]
fn test_ac1_thumbnails_are_redacted() {
    let dir = common::temp_dir("rt_ac1");
    let src = face_fx("apollo11.jpg");
    let config = component_config(&dir);
    assert_eq!(config.pipeline.last().map(String::as_str), Some("thumbnail"));
    let d = run_pipeline(config, &src);

    let privacy = d.privacy.clone().unwrap();
    assert_eq!(privacy.face_redaction, Some(FaceRedaction::Applied));
    let thumbs_dir = dir.join("thumbs").to_string_lossy().to_string();
    let redacted = privacy.redacted_image_path.clone().unwrap();
    assert!(redacted.starts_with(&thumbs_dir), "{}", redacted);
    let faces = d.faces.clone().unwrap().faces;
    assert_eq!(faces.len(), 3);
    assert_eq!(d.thumbnails.len(), 3);

    // Thumbnails of the unredacted original for comparison (privacy-redact not run).
    let (plain, _, _, _) = run(&ThumbnailPlugin::new(), doc(&src), &[("output_dir", &dir.join("plain").to_string_lossy())], false);
    for (t, p) in d.thumbnails.iter().zip(&plain.thumbnails) {
        assert!(t.path.starts_with(&thumbs_dir) && t.path.ends_with(".avif"), "{}", t.path);
        let img = load_avif(&t.path);
        let orig = load_avif(&p.path);
        assert_eq!(img.dimensions(), (t.width, t.height));
        for f in &faces {
            let cx = ((f.bbox.x + f.bbox.width / 2.0) * t.width as f32) as u32;
            let cy = ((f.bbox.y + f.bbox.height / 2.0) * t.height as f32) as u32;
            assert!(brightness(&img, cx, cy) < 40, "{} {}: face centre not redacted ({})", t.size_label, f.id, brightness(&img, cx, cy));
            assert!(brightness(&orig, cx, cy) > 60, "{} {}: original unexpectedly dark", t.size_label, f.id);
        }
    }
    // AC-2: no separate redacted folder, and the defaults point to the thumbnail folder.
    assert!(!dir.join("redacted").exists());
    assert!(std::fs::read_dir(&dir).unwrap().all(|e| e.unwrap().file_name() != "redacted"));
    assert_eq!(PrivacySettings::default().output_dir, "cache/thumbnails");
    assert_eq!(PrivacySettings::default().output_dir, ThumbnailSettings::default().output_dir);
    let _ = std::fs::remove_dir_all(&dir);
}

// redacted-avif-thumbnails AC-2: privacy-redact writes the redacted image below cache/thumbnails by default and sets face_redaction = applied
#[test]
fn test_ac2_no_cache_redacted() {
    let dir = common::temp_dir("rt_ac2");
    let mut d = doc(&face_fx("apollo11.jpg"));
    d.faces = Some(one_face());
    let out = dir.join("cache/thumbnails").to_string_lossy().to_string();
    let (d, _, _, writes) = run(&PrivacyRedactPlugin::new(), d, &[("output_dir", &out)], false);
    let privacy = d.privacy.unwrap();
    assert_eq!(privacy.face_redaction, Some(FaceRedaction::Applied));
    assert_eq!(writes, vec![format!("{}/doc-apollo11.jpg.redacted.jpg", out)]);
    assert_eq!(privacy.redacted_image_path.as_deref(), Some(writes[0].as_str()));
    assert!(!dir.join("cache/redacted").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

// redacted-avif-thumbnails AC-3: with redact_faces on, a missing face result, an unreadable image or a failed write give face_redaction = failed; thumbnail then writes nothing, keeps doc.thumbnails empty and warns with path and reason
#[test]
fn test_ac3_failed_redaction_no_thumbnails() {
    let dir = common::temp_dir("rt_ac3");
    let src = face_fx("apollo11.jpg");
    let thumbs = dir.join("thumbs").to_string_lossy().to_string();
    let out = [("output_dir", thumbs.as_str())];

    let missing_faces = doc(&src);
    let mut unreadable = doc(&src);
    unreadable.faces = Some(one_face());
    unreadable.working_image = Some(WorkingImage {
        path: dir.join("gone.working.jpg").to_string_lossy().to_string(),
        source_format: "heic".to_string(),
        width: 10,
        height: 10,
        converted: true,
    });
    let mut not_writable = doc(&src);
    not_writable.faces = Some(one_face());

    for (case, d, deny) in [("no face result", missing_faces, false), ("unreadable", unreadable, false), ("write denied", not_writable, true)] {
        let (d, warnings, _, _) = run(&PrivacyRedactPlugin::new(), d, &out, deny);
        let privacy = d.privacy.clone().unwrap();
        assert_eq!(privacy.face_redaction, Some(FaceRedaction::Failed), "{}", case);
        assert!(privacy.redacted_image_path.is_none(), "{}", case);
        assert!(warnings.iter().any(|w| w.contains("NOT redacted")), "{}: {:?}", case, warnings);
        assert_eq!(display_source(&d), None, "{}", case);

        let mut d = d;
        d.thumbnails = vec![ThumbnailEntry { size_label: "old".into(), width: 1, height: 1, path: "/x".into(), cache_key: "k".into() }];
        let (d, warnings, reads, writes) = run(&ThumbnailPlugin::new(), d, &out, false);
        assert!(d.thumbnails.is_empty(), "{}", case);
        assert!(reads.is_empty() && writes.is_empty(), "{}: {:?} {:?}", case, reads, writes);
        assert!(
            warnings.iter().any(|w| w.starts_with("thumbnail:") && w.contains(&src) && w.contains("faces could not be redacted")),
            "{}: {:?}",
            case,
            warnings
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// redacted-avif-thumbnails AC-4: with redact_faces off (disabled), without faces (no_faces) or without doc.privacy, thumbnails come from the working image
#[test]
fn test_ac4_disabled_and_no_faces_use_working_image() {
    let dir = common::temp_dir("rt_ac4");
    let src = face_fx("apollo11.jpg");
    let thumbs = dir.join("thumbs").to_string_lossy().to_string();

    let (disabled, _, _, _) = run(&PrivacyRedactPlugin::new(), doc(&src), &[("redact_faces", "false")], false);
    assert_eq!(disabled.privacy.as_ref().unwrap().face_redaction, Some(FaceRedaction::Disabled));
    let mut empty = doc(&src);
    empty.faces = Some(FacesInfo { faces: vec![], detection_model: "test".into() });
    let (no_faces, _, _, writes) = run(&PrivacyRedactPlugin::new(), empty, &[], false);
    assert_eq!(no_faces.privacy.as_ref().unwrap().face_redaction, Some(FaceRedaction::NoFaces));
    assert!(writes.is_empty());

    for (case, d) in [("disabled", disabled), ("no_faces", no_faces), ("no privacy", doc(&src))] {
        assert_eq!(display_source(&d), Some(pixel_source(&d)), "{}", case);
        let (d, warnings, reads, _) = run(&ThumbnailPlugin::new(), d, &[("output_dir", &thumbs)], false);
        assert_eq!(reads, vec![src.clone()], "{}", case);
        assert_eq!(d.thumbnails.len(), 3, "{}: {:?}", case, warnings);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

async fn get(app: axum::Router, uri: &str) -> axum::response::Response {
    let req = axum::http::Request::builder().uri(uri).body(axum::body::Body::empty()).unwrap();
    app.oneshot(req).await.unwrap()
}

// redacted-avif-thumbnails AC-5: the thumbnail endpoint answers 404 without thumbnail file, never serves the original or a placeholder, and sends image/avif or image/jpeg with Cache-Control private, max-age=86400
#[test]
fn test_ac5_thumbnail_endpoint_404_and_headers() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        let dir = common::temp_dir("rt_ac5");
        let storage = Arc::new(Storage::new(dir.join("media.db").to_string_lossy().to_string()));
        storage.init_schema().unwrap();
        // The original exists, so a fallback would be possible; it must not be used.
        let original = common::write_file(&dir, "original.jpg", &common::plain_jpeg(64, 48));
        let avif = common::write_file(&dir, "t_128.avif", b"avif-bytes");
        let jpeg = common::write_file(&dir, "t_512.jpg", b"jpeg-bytes");
        let entry = |label: &str, path: &str| ThumbnailEntry { size_label: label.into(), width: 1, height: 1, path: path.into(), cache_key: format!("k-{}", label) };

        let mut none = doc(&original);
        none.id = "none".into();
        let mut missing = doc(&original);
        missing.id = "missing".into();
        missing.thumbnails = vec![entry("small", &dir.join("gone.avif").to_string_lossy())];
        let mut both = doc(&original);
        both.id = "both".into();
        both.thumbnails = vec![entry("small", &avif), entry("medium", &jpeg)];
        for d in [&none, &missing, &both] {
            storage.save_document(d).unwrap();
        }
        let app = build_router(ServerState { storage });

        for uri in ["/api/photos/none/thumbnail/small", "/api/photos/missing/thumbnail/small", "/api/photos/unknown/thumbnail/small"] {
            let res = get(app.clone(), uri).await;
            assert_eq!(res.status(), StatusCode::NOT_FOUND, "{}", uri);
        }
        for (uri, mime, body) in [
            ("/api/photos/both/thumbnail/small", "image/avif", &b"avif-bytes"[..]),
            ("/api/photos/both/thumbnail/medium", "image/jpeg", &b"jpeg-bytes"[..]),
        ] {
            let res = get(app.clone(), uri).await;
            assert_eq!(res.status(), StatusCode::OK, "{}", uri);
            assert_eq!(res.headers()[header::CONTENT_TYPE], mime);
            assert_eq!(res.headers()[header::CACHE_CONTROL], "private, max-age=86400");
            let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
            assert_eq!(&bytes[..], body);
        }
        let src = std::fs::read_to_string(common::repo_root().join("crates/host/src/server.rs")).unwrap();
        assert!(!src.contains("FALLBACK_JPEG_1X1"));
        let _ = std::fs::remove_dir_all(&dir);
    });
}

// redacted-avif-thumbnails AC-6: the host rejects a pipeline with thumbnail before privacy-redact, face-detection after it or face-detection missing/disabled while redact_faces is on; no condition with redact_faces false or privacy-redact disabled; the default and all profiles satisfy the rule
#[test]
fn test_ac6_pipeline_order_is_enforced() {
    let yaml = |pipeline: &[&str], plugins: &str| {
        let mut s = String::from("pipeline:\n");
        for p in pipeline {
            s.push_str(&format!("  - {}\n", p));
        }
        s.push_str("plugins:\n");
        s.push_str(plugins);
        s
    };
    let wrong_thumb = ["metadata", "thumbnail", "face-detection", "privacy-redact"];
    let wrong_face = ["metadata", "privacy-redact", "face-detection", "thumbnail"];
    let no_face = ["metadata", "privacy-redact", "thumbnail"];
    let ok = ["metadata", "face-detection", "privacy-redact", "thumbnail"];

    let err = AppConfig::load_from_str(&yaml(&wrong_thumb, "")).unwrap_err().to_string();
    assert!(err.contains("thumbnail (position 2)") && err.contains("privacy-redact (position 4)") && err.contains("ADR 0012"), "{}", err);
    for (pipeline, plugins) in [
        (&wrong_face[..], ""),
        (&no_face[..], ""),
        (&ok[..], "  face-detection:\n    enabled: false\n"),
    ] {
        let err = AppConfig::load_from_str(&yaml(pipeline, plugins)).unwrap_err().to_string();
        assert!(err.contains("face-detection") && err.contains("privacy-redact"), "{:?}: {}", pipeline, err);
    }
    // JSON configs are checked as well.
    let mut cfg = AppConfig::default();
    cfg.pipeline = wrong_thumb.iter().map(|s| s.to_string()).collect();
    assert!(AppConfig::load_from_str(&serde_json::to_string(&cfg).unwrap()).is_err());

    // No condition without face redaction.
    let off = "  privacy-redact:\n    enabled: true\n    settings:\n      redact_faces: 'false'\n";
    assert!(AppConfig::load_from_str(&yaml(&wrong_thumb, off)).is_ok());
    let disabled = "  privacy-redact:\n    enabled: false\n";
    assert!(AppConfig::load_from_str(&yaml(&wrong_thumb, disabled)).is_ok());
    assert!(AppConfig::load_from_str(&yaml(&["metadata", "thumbnail"], "")).is_ok());
    assert!(AppConfig::load_from_str(&yaml(&ok, "")).is_ok());

    AppConfig::default().validate_privacy_order().unwrap();
    for name in common::PROFILES {
        let cfg = AppConfig::load_from_file(common::repo_root().join(name)).unwrap_or_else(|e| panic!("{}: {}", name, e));
        assert_eq!(cfg.pipeline.last().map(String::as_str), Some("thumbnail"), "{}", name);
        assert_eq!(cfg.plugins["thumbnail"].settings.get("format").map(String::as_str), Some("avif"), "{}", name);
    }
}

// redacted-avif-thumbnails AC-7: with the default format every thumbnail is a decodable AVIF (brand avif) with the size of its entry and the extension .avif; with format jpeg they are JPEGs on .jpg
#[test]
fn test_ac7_avif_thumbnails() {
    let dir = common::temp_dir("rt_ac7");
    let src = face_fx("apollo11.jpg");
    let thumbs = dir.join("thumbs").to_string_lossy().to_string();
    let (d, warnings, _, _) = run(&ThumbnailPlugin::new(), doc(&src), &[("output_dir", &thumbs)], false);
    assert!(warnings.is_empty(), "{:?}", warnings);
    assert_eq!(d.thumbnails.iter().map(|t| (t.width, t.height)).collect::<Vec<_>>(), vec![(128, 101), (512, 402), (1024, 804)]);
    for t in &d.thumbnails {
        assert!(t.path.ends_with(&format!("_{}.avif", t.width.max(t.height))), "{}", t.path);
        assert_eq!(load_avif(&t.path).dimensions(), (t.width, t.height));
    }
    let (d, _, _, _) = run(&ThumbnailPlugin::new(), doc(&src), &[("output_dir", &thumbs), ("format", "jpeg")], false);
    for t in &d.thumbnails {
        assert!(t.path.ends_with(".jpg"), "{}", t.path);
        let bytes = std::fs::read(&t.path).unwrap();
        assert!(bytes.starts_with(&[0xFF, 0xD8, 0xFF]));
        assert_eq!(image::load_from_memory(&bytes).unwrap().to_rgb8().dimensions(), (t.width, t.height));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// redacted-avif-thumbnails AC-8: an unknown format warns and uses avif; without quality AVIF uses 70 and JPEG 75; a 1024 px AVIF of apollo11.jpg is smaller than the JPEG with its default quality
#[test]
fn test_ac8_format_and_quality_defaults() {
    let defaults = ThumbnailSettings::default();
    assert_eq!((defaults.format, defaults.quality), (ThumbnailFormat::Avif, None));
    assert_eq!((DEFAULT_AVIF_QUALITY, DEFAULT_JPEG_QUALITY), (70, 75));
    assert_eq!(ThumbnailFormat::Avif.default_quality(), 70);
    assert_eq!(ThumbnailFormat::Jpeg.default_quality(), 75);

    let caps = common::LogCaps::default();
    let s = ThumbnailSettings::parse(&defaults, &HashMap::from([("format".to_string(), "webp".to_string())]), &caps);
    assert_eq!(s.format, ThumbnailFormat::Avif);
    assert!(caps.warnings().iter().any(|w| w.contains("thumbnail") && w.contains("format") && w.contains("webp")), "{:?}", caps.warnings());
    let caps = common::LogCaps::default();
    let s = ThumbnailSettings::parse(&defaults, &HashMap::from([("format".to_string(), "JPG".to_string())]), &caps);
    assert_eq!(s.format, ThumbnailFormat::Jpeg);

    let img = image::open(face_fx("apollo11.jpg")).unwrap();
    let (avif, w, h) = render_thumbnail(&img, 1024, ThumbnailFormat::Avif, DEFAULT_AVIF_QUALITY).unwrap();
    let (jpeg, w2, h2) = render_thumbnail(&img, 1024, ThumbnailFormat::Jpeg, DEFAULT_JPEG_QUALITY).unwrap();
    assert_eq!((w, h), (w2, h2));
    eprintln!("1024 px: AVIF {} B, JPEG {} B", avif.len(), jpeg.len());
    assert!(avif.len() < jpeg.len(), "AVIF {} B >= JPEG {} B", avif.len(), jpeg.len());
}

// redacted-avif-thumbnails AC-9: face_redaction survives host -> component -> host and JSON; an older document without the field reads as None and display_source gives the working image
#[test]
fn test_ac9_face_redaction_roundtrip() {
    let wit = std::fs::read_to_string(common::repo_root().join("wit/media-plugin.wit")).unwrap();
    assert!(wit.contains("enum face-redaction") && wit.contains("face-redaction: option<face-redaction>"));
    let mut config = common::with_components(AppConfig::default());
    config.component_cache = false;
    let rt = PluginRuntime::new(config).unwrap();
    for value in [Some(FaceRedaction::Applied), Some(FaceRedaction::NoFaces), Some(FaceRedaction::Disabled), Some(FaceRedaction::Failed), None] {
        let mut d = doc("/a/b.jpg");
        d.privacy = Some(PrivacyInfo { face_redaction: value, redacted_image_path: Some("/r.jpg".into()), ..Default::default() });
        let back = rt.execute_plugin("countryflag", d.clone()).unwrap();
        assert_eq!(back.privacy.unwrap().face_redaction, value);
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(serde_json::from_str::<UnifiedDocument>(&json).unwrap().privacy.unwrap().face_redaction, value);
    }
    let json = serde_json::to_string(&PrivacyInfo { face_redaction: Some(FaceRedaction::NoFaces), ..Default::default() }).unwrap();
    assert!(json.contains("\"face_redaction\":\"no_faces\""), "{}", json);

    let old = r#"{"id":"x","file":{"path":"/p.jpg","filename":"p.jpg","size_bytes":1,"mime_type":"image/jpeg"},"privacy":{"is_redacted":false,"faces_redacted_count":0,"license_plates_redacted_count":0,"original_gps_fuzzed":false},"custom":{}}"#;
    let d: UnifiedDocument = serde_json::from_str(old).unwrap();
    assert_eq!(d.privacy.as_ref().unwrap().face_redaction, None);
    assert_eq!(display_source(&d), Some("/p.jpg"));
}

// redacted-avif-thumbnails AC-10: a generated 4000 x 3000 image gets AVIF thumbnails of 128, 512 and 1024 px through the host within the limits of thumbnail in AppConfig::default()
#[test]
fn test_ac10_large_image_avif_within_limits() {
    let dir = common::temp_dir("rt_ac10");
    let big = image::imageops::resize(&image::open(face_fx("apollo11.jpg")).unwrap().to_rgb8(), 4000, 3000, image::imageops::FilterType::Triangle);
    let path = dir.join("big.jpg");
    big.save(&path).unwrap();
    let config = component_config(&dir);
    let rt = PluginRuntime::new(config).unwrap();
    let mut d = doc(path.to_str().unwrap());
    for stage in ["image-convert", "face-detection", "privacy-redact"] {
        d = rt.execute_plugin(stage, d).unwrap();
    }
    let start = Instant::now();
    let d = rt.execute_plugin("thumbnail", d).unwrap();
    eprintln!("thumbnail stage, 4000 x 3000, AVIF: {:?}", start.elapsed());
    assert_eq!(d.thumbnails.iter().map(|t| t.width).collect::<Vec<_>>(), vec![128, 512, 1024]);
    for t in &d.thumbnails {
        assert_eq!(load_avif(&t.path).dimensions(), (t.width, t.height));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// redacted-avif-thumbnails AC-11: the READMEs of thumbnail and privacy-redact, the main README and the plugin guide describe format, the order, face_redaction and fail closed; NOTICE names oxideav-avif for thumbnail
#[test]
fn test_ac11_docs_and_notice() {
    let root = common::repo_root();
    let read = |p: &str| std::fs::read_to_string(root.join(p)).unwrap();
    let thumb = read("plugins/thumbnail/README.md");
    for needle in ["`format`", "avif", "face_redaction", "fail closed", "ADR 0012"] {
        assert!(thumb.contains(needle), "thumbnail README lacks {}", needle);
    }
    let privacy = read("plugins/privacy-redact/README.md");
    for needle in ["face_redaction", "cache/thumbnails", "ADR 0012"] {
        assert!(privacy.contains(needle), "privacy-redact README lacks {}", needle);
    }
    let main = read("README.md");
    assert!(main.contains("AVIF") && main.contains("ADR 0012"), "README");
    let guide = read("docs/plugin-development-guide.md");
    assert!(guide.contains("display_source") && guide.contains("fail closed"), "plugin guide");
    let notice = read("NOTICE");
    assert!(notice.contains("oxideav-avif") && notice.contains("thumbnail"), "NOTICE");
}
