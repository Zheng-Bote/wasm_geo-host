//! Working image for all input formats (`image-convert`, ADR 0011).

mod common;

use flagme_host::watcher::{detect_mime_type, is_supported_media_file};
use flagme_host::{AppConfig, PluginRuntime};
use flagme_plugin_ai_tagging::AiTaggingPlugin;
use flagme_plugin_duplicate::{compute_dhash, DuplicatePlugin};
use flagme_plugin_face_detection::FaceDetectionPlugin;
use flagme_plugin_image_convert::{working_file_name, ImageConvertPlugin, ImageConvertSettings};
use flagme_plugin_ocr_text::OcrTextPlugin;
use flagme_plugin_privacy_redact::PrivacyRedactPlugin;
use flagme_plugin_thumbnail::ThumbnailPlugin;
use flagme_sdk::{
    pixel_source, FileInfo, Plugin, PluginContext, PluginSettings, UnifiedDocument, WorkingImage,
};
use image::RgbImage;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

fn fx(name: &str) -> String {
    common::repo_root().join("crates/host/tests/fixtures/formats").join(name).to_string_lossy().to_string()
}

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
            mime_type: "application/octet-stream".to_string(),
            sha256: None,
        },
    )
}

fn settings(out: &Path, extra: &[(&str, &str)]) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = extra.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    map.insert("output_dir".to_string(), out.to_string_lossy().to_string());
    map
}

/// Runs `image-convert` natively with real file access.
fn convert(path: &str, out: &Path, extra: &[(&str, &str)]) -> (UnifiedDocument, Arc<common::LogCaps>) {
    let caps = Arc::new(common::LogCaps::default());
    let ctx = PluginContext::with_settings(caps.clone(), settings(out, extra));
    let d = ImageConvertPlugin::new().process(doc(path), &ctx).unwrap();
    (d, caps)
}

fn psnr(a: &RgbImage, b: &RgbImage) -> f64 {
    assert_eq!(a.dimensions(), b.dimensions());
    let mse: f64 = a
        .as_raw()
        .iter()
        .zip(b.as_raw())
        .map(|(x, y)| (*x as f64 - *y as f64).powi(2))
        .sum::<f64>()
        / a.as_raw().len() as f64;
    10.0 * (255.0f64 * 255.0 / mse.max(1e-12)).log10()
}

fn reference() -> RgbImage {
    image::open(fx("apollo11.png")).unwrap().to_rgb8()
}

fn load_jpeg(path: &str) -> RgbImage {
    let bytes = std::fs::read(path).unwrap();
    assert!(bytes.starts_with(&[0xFF, 0xD8, 0xFF]), "{} is not a JPEG", path);
    image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg).unwrap().to_rgb8()
}

/// Marker segments of a JPEG before the first scan: `(marker, payload)`.
fn jpeg_segments(bytes: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut out = Vec::new();
    let mut i = 2;
    while i + 4 <= bytes.len() && bytes[i] == 0xFF {
        let marker = bytes[i + 1];
        if marker == 0xDA {
            break;
        }
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        out.push((marker, bytes[i + 4..i + 2 + len].to_vec()));
        i += 2 + len;
    }
    out
}

/// ICC profile from the `ICC_PROFILE` APP2 segments of a JPEG.
fn jpeg_icc(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut chunks: Vec<(u8, Vec<u8>)> = jpeg_segments(bytes)
        .into_iter()
        .filter(|(m, p)| *m == 0xE2 && p.starts_with(b"ICC_PROFILE\0"))
        .map(|(_, p)| (p[12], p[14..].to_vec()))
        .collect();
    if chunks.is_empty() {
        return None;
    }
    chunks.sort_by_key(|(n, _)| *n);
    Some(chunks.into_iter().flat_map(|(_, c)| c).collect())
}

fn files_in(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).map(|r| r.filter_map(|e| e.ok()).map(|e| e.path()).collect()).unwrap_or_default()
}

fn component_config(out: &Path) -> AppConfig {
    let mut config = common::with_components(AppConfig::default());
    config.component_cache = false;
    let ic = config.plugins.get_mut("image-convert").unwrap();
    ic.settings.insert("output_dir".to_string(), out.join("working").to_string_lossy().to_string());
    let pr = config.plugins.get_mut("privacy-redact").unwrap();
    pr.settings.insert("output_dir".to_string(), out.join("redacted").to_string_lossy().to_string());
    pr.settings.insert("style".to_string(), "blackout".to_string());
    config
}

// image-convert AC-1: PNG, TIFF, BMP, WebP, HEIC, AVIF and 10-bit AVIF become a converted JPEG working image with the source size and a PSNR >= 35 dB to apollo11.png
#[test]
fn test_ac1_all_formats_to_jpeg() {
    let out = common::temp_dir("ic_ac1");
    let reference = reference();
    for (name, format) in [
        ("apollo11.png", "png"),
        ("apollo11.tiff", "tiff"),
        ("apollo11.bmp", "bmp"),
        ("apollo11.webp", "webp"),
        ("apollo11.heic", "heic"),
        ("apollo11.avif", "avif"),
        ("apollo11-10bit.avif", "avif"),
    ] {
        let start = Instant::now();
        let (d, caps) = convert(&fx(name), &out, &[]);
        let w = d.working_image.unwrap_or_else(|| panic!("{}: no working image, {:?}", name, caps.warnings()));
        eprintln!("{}: {:?}", name, start.elapsed());
        assert!(w.converted, "{}", name);
        assert_eq!(w.source_format, format, "{}", name);
        assert_eq!((w.width, w.height), reference.dimensions(), "{}", name);
        assert!(w.path.ends_with(".working.jpg"), "{}", w.path);
        let img = load_jpeg(&w.path);
        assert_eq!(img.dimensions(), reference.dimensions(), "{}", name);
        let p = psnr(&img, &reference);
        assert!(p >= 35.0, "{}: PSNR {:.1} dB", name, p);
        assert!(caps.warnings().is_empty(), "{}: {:?}", name, caps.warnings());
    }
    let _ = std::fs::remove_dir_all(&out);
}

// image-convert AC-2: a complete JPEG without rotation is its own working image (converted = false, source_format = jpeg) and nothing is written
#[test]
fn test_ac2_plain_jpeg_passthrough() {
    let out = common::temp_dir("ic_ac2");
    let src = face_fx("apollo11.jpg");
    let (d, caps) = convert(&src, &out, &[]);
    let w = d.working_image.clone().expect("working image");
    assert_eq!(w, WorkingImage { path: src.clone(), source_format: "jpeg".to_string(), width: 1600, height: 1257, converted: false });
    assert_eq!(pixel_source(&d), src);
    assert!(caps.writes.lock().unwrap().is_empty());
    assert!(files_in(&out).is_empty());
    let _ = std::fs::remove_dir_all(&out);
}

// image-convert AC-3: a JPEG with EXIF orientation 6 and HEIC/AVIF with irot become an upright working image with swapped size and a PSNR >= 35 dB to apollo11.png rotated 90 degrees clockwise
#[test]
fn test_ac3_rotation_exif_and_irot() {
    let out = common::temp_dir("ic_ac3");
    let rotated = image::imageops::rotate90(&reference());
    for (name, format) in [("apollo11-rot90.jpg", "jpeg"), ("apollo11-rot90.heic", "heic"), ("apollo11-rot90.avif", "avif")] {
        let (d, caps) = convert(&fx(name), &out, &[]);
        let w = d.working_image.unwrap_or_else(|| panic!("{}: {:?}", name, caps.warnings()));
        assert!(w.converted, "{}", name);
        assert_eq!(w.source_format, format);
        assert_eq!((w.width, w.height), (503, 640), "{}", name);
        let p = psnr(&load_jpeg(&w.path), &rotated);
        assert!(p >= 35.0, "{}: PSNR {:.1} dB", name, p);
    }
    let _ = std::fs::remove_dir_all(&out);
}

// image-convert AC-4: apollo11-alpha.png gives a working image without alpha channel; transparent areas are white
#[test]
fn test_ac4_alpha_on_white() {
    let out = common::temp_dir("ic_ac4");
    let (d, _) = convert(&fx("apollo11-alpha.png"), &out, &[]);
    let w = d.working_image.expect("working image");
    let decoded = image::open(&w.path).unwrap();
    assert!(!decoded.color().has_alpha());
    let img = decoded.to_rgb8();
    assert_eq!(img.dimensions(), (160, 126));
    // Columns 0..40 were transparent; leave a margin for JPEG ringing at the edge.
    for y in 0..126 {
        for x in 0..36 {
            assert!(img.get_pixel(x, y).0.iter().all(|c| *c >= 245), "pixel {},{} = {:?}", x, y, img.get_pixel(x, y));
        }
    }
    // The opaque part keeps the photo (mean clearly below white).
    let mean: f64 = (48..160).flat_map(|x| (0..126).map(move |y| (x, y))).map(|(x, y)| img.get_pixel(x, y).0[0] as f64).sum::<f64>() / (112.0 * 126.0);
    assert!(mean < 200.0, "mean {}", mean);
    let _ = std::fs::remove_dir_all(&out);
}

// image-convert AC-5: the working image has no EXIF, GPS or XMP; an ICC profile of the source is embedded
#[test]
fn test_ac5_no_metadata_but_icc() {
    let out = common::temp_dir("ic_ac5");
    // Sources with EXIF: the rotated JPEG (orientation), PNG and HEIC derived from the NASA JPEG.
    for name in ["apollo11-rot90.jpg", "apollo11.png", "apollo11.heic"] {
        let (d, _) = convert(&fx(name), &out, &[]);
        let bytes = std::fs::read(d.working_image.unwrap().path).unwrap();
        let segments = jpeg_segments(&bytes);
        assert!(!segments.iter().any(|(m, _)| *m == 0xE1), "{}: APP1 (EXIF/XMP) present", name);
        assert!(!segments.iter().any(|(m, _)| *m == 0xED), "{}: APP13 (IPTC) present", name);
    }
    let icc = std::fs::read(fx("test-gamma22.icc")).unwrap();
    for name in ["apollo11-icc.png", "apollo11-icc.heic"] {
        let (d, caps) = convert(&fx(name), &out, &[]);
        let w = d.working_image.unwrap_or_else(|| panic!("{}: {:?}", name, caps.warnings()));
        let bytes = std::fs::read(&w.path).unwrap();
        assert_eq!(jpeg_icc(&bytes).as_deref(), Some(icc.as_slice()), "{}: ICC profile not embedded", name);
        assert!(!jpeg_segments(&bytes).iter().any(|(m, _)| *m == 0xE1), "{}", name);
    }
    let _ = std::fs::remove_dir_all(&out);
}

// image-convert AC-6: unreadable files, truncated HEIC/AVIF/JPEG, unsupported formats (GIF) and images above max_pixels leave working_image = None, write nothing and warn with path and cause
#[test]
fn test_ac6_failures_leave_none() {
    let out = common::temp_dir("ic_ac6");
    let truncated_jpeg = {
        let bytes = std::fs::read(face_fx("apollo11.jpg")).unwrap();
        common::write_file(&out, "trunc.jpg", &bytes[..bytes.len() / 2])
    };
    let cases: Vec<(String, Vec<(&str, &str)>, &str)> = vec![
        (out.join("missing.heic").to_string_lossy().to_string(), vec![], "cannot read"),
        (fx("apollo11-trunc.heic"), vec![], "HEIC"),
        (fx("apollo11-trunc.avif"), vec![], "AVIF"),
        (truncated_jpeg, vec![], "truncated JPEG"),
        (fx("apollo11.gif"), vec![], "unsupported format"),
        (fx("gradient-48mp.heic"), vec![("max_pixels", "40000000")], "exceed max_pixels 40000000"),
    ];
    let work = out.join("working");
    for (path, extra, cause) in cases {
        let (d, caps) = convert(&path, &work, &extra);
        assert!(d.working_image.is_none(), "{}", path);
        let warnings = caps.warnings();
        assert!(
            warnings.iter().any(|w| w.starts_with("image-convert:") && w.contains(&path) && w.contains(cause)),
            "{}: {:?}",
            path,
            warnings
        );
        assert!(caps.writes.lock().unwrap().is_empty(), "{}", path);
    }
    assert!(files_in(&work).is_empty());
    let _ = std::fs::remove_dir_all(&out);
}

// image-convert AC-7: a second run with the same source does not write again and yields the same working_image; the file name is <output_dir>/<sha256 of the source bytes>.working.jpg
#[test]
fn test_ac7_cache_reuse() {
    let out = common::temp_dir("ic_ac7");
    let src = fx("apollo11.heic");
    let hex = format!("{:x}", Sha256::digest(std::fs::read(&src).unwrap()));
    assert_eq!(working_file_name(&std::fs::read(&src).unwrap()), format!("{}.working.jpg", hex));

    let (first, caps1) = convert(&src, &out, &[]);
    let first = first.working_image.unwrap();
    assert_eq!(first.path, format!("{}/{}.working.jpg", out.to_string_lossy(), hex));
    assert_eq!(caps1.writes.lock().unwrap().len(), 1);
    let content = std::fs::read(&first.path).unwrap();
    let modified = std::fs::metadata(&first.path).unwrap().modified().unwrap();

    let (second, caps2) = convert(&src, &out, &[]);
    assert_eq!(second.working_image.unwrap(), first);
    assert!(caps2.writes.lock().unwrap().is_empty());
    assert_eq!(std::fs::read(&first.path).unwrap(), content);
    assert_eq!(std::fs::metadata(&first.path).unwrap().modified().unwrap(), modified);
    let _ = std::fs::remove_dir_all(&out);
}

// image-convert AC-8: through the host, face-detection finds the 3 faces of apollo11 in the HEIC, AVIF and WebP test images, and privacy-redact writes a redacted JPEG with faces_redacted_count = 3
#[test]
fn test_ac8_faces_redacted_for_heic_avif_webp() {
    let out = common::temp_dir("ic_ac8");
    let rt = PluginRuntime::new(component_config(&out)).unwrap();
    for name in ["apollo11.heic", "apollo11.avif", "apollo11.webp"] {
        let d = rt.execute_plugin("image-convert", doc(&fx(name))).unwrap();
        let w = d.working_image.clone().unwrap_or_else(|| panic!("{}: no working image", name));
        assert!(w.converted);
        let d = rt.execute_plugin("face-detection", d).unwrap();
        assert_eq!(d.faces.as_ref().map(|f| f.faces.len()), Some(3), "{}", name);
        let d = rt.execute_plugin("privacy-redact", d).unwrap();
        let privacy = d.privacy.unwrap();
        assert_eq!(privacy.faces_redacted_count, 3, "{}", name);
        let redacted = privacy.redacted_image_path.expect("redacted image");
        assert_eq!(load_jpeg(&redacted).dimensions(), (w.width, w.height));
    }
    let _ = std::fs::remove_dir_all(&out);
}

// image-convert AC-9: thumbnail, duplicate (dHash), ai-tagging, ocr-text, face-detection and privacy-redact read pixel_source(&doc); without working_image they behave as before, metadata and the duplicate SHA-256 keep reading file.path
#[test]
fn test_ac9_pixel_plugins_read_working_image() {
    let out = common::temp_dir("ic_ac9");
    // Original: a face-free square JPEG; working image: the Apollo 11 photo (3 faces, 1600 x 1257).
    let original = common::write_file(&out, "plain.jpg", &common::plain_jpeg(300, 300));
    let working = face_fx("apollo11.jpg");
    let working_len = std::fs::read(&working).unwrap().len();
    let with_working = |mut d: UnifiedDocument| {
        d.working_image = Some(WorkingImage {
            path: working.clone(),
            source_format: "jpeg".to_string(),
            width: 1600,
            height: 1257,
            converted: true,
        });
        d
    };
    let run = |plugin: &dyn Plugin, d: UnifiedDocument, extra: &[(&str, &str)]| {
        let caps = Arc::new(common::LogCaps::default());
        let map: HashMap<String, String> = extra.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let ctx = PluginContext::with_settings(caps.clone(), map);
        let d = plugin.process(d, &ctx).unwrap();
        let reads = caps.reads.lock().unwrap().clone();
        (d, reads)
    };
    let thumbs = out.join("thumbs").to_string_lossy().to_string();
    let redacted = out.join("redacted").to_string_lossy().to_string();

    for use_working in [true, false] {
        let base = if use_working { with_working(doc(&original)) } else { doc(&original) };
        let expected = if use_working { working.clone() } else { original.clone() };

        // face-detection (real YuNet): 3 faces only in the working image.
        let (d, reads) = run(&FaceDetectionPlugin::new(), base.clone(), &[]);
        assert_eq!(reads, vec![expected.clone()]);
        assert_eq!(d.faces.as_ref().unwrap().faces.len(), if use_working { 3 } else { 0 });

        // privacy-redact: the redacted copy has the size of the image it read.
        let (d, reads) = run(&PrivacyRedactPlugin::new(), d, &[("output_dir", &redacted)]);
        if use_working {
            assert_eq!(reads, vec![expected.clone()]);
            let img = load_jpeg(d.privacy.unwrap().redacted_image_path.as_deref().unwrap());
            assert_eq!(img.dimensions(), (1600, 1257));
        } else {
            assert!(reads.is_empty(), "no faces, nothing to read: {:?}", reads);
        }

        // ocr-text and ai-tagging (test engines whose output is the byte length they read).
        let (d, reads) = run(&OcrTextPlugin::with_engine(Box::new(common::ByteLenOcrEngine)), base.clone(), &[]);
        assert_eq!(reads, vec![expected.clone()]);
        let len = std::fs::read(&expected).unwrap().len();
        assert_eq!(d.ocr.unwrap().combined_text, format!("bytes {}", len));
        let (d, reads) = run(&AiTaggingPlugin::with_model(Box::new(common::ByteLenVisionModel)), base.clone(), &[]);
        assert_eq!(reads, vec![expected.clone()]);
        assert_eq!(d.ai.unwrap().tags[0].label, format!("bytes {}", len));

        // thumbnail: aspect ratio of the image read.
        let (d, reads) = run(&ThumbnailPlugin::new(), base.clone(), &[("output_dir", &thumbs), ("variants", "256")]);
        assert_eq!(reads, vec![expected.clone()]);
        let t = &d.thumbnails[0];
        assert_eq!((t.width, t.height), if use_working { (256, 201) } else { (256, 256) });

        // duplicate: SHA-256 of the original file, dHash of the image read.
        let (d, reads) = run(&DuplicatePlugin::new(), base.clone(), &[]);
        let dup = d.duplicate.unwrap();
        assert_eq!(dup.sha256, format!("{:x}", Sha256::digest(std::fs::read(&original).unwrap())));
        assert_eq!(dup.phash, compute_dhash(&std::fs::read(&expected).unwrap()));
        if use_working {
            assert_eq!(reads, vec![original.clone(), working.clone()]);
        } else {
            assert_eq!(reads, vec![original.clone()]);
        }
    }
    assert!(working_len > 0);
    let _ = std::fs::remove_dir_all(&out);
}

// image-convert AC-10: for apollo11-rot90.jpg the face boxes lie in the upright working image and the redaction of privacy-redact hits the faces there
#[test]
fn test_ac10_rotated_jpeg_faces_and_redaction() {
    let out = common::temp_dir("ic_ac10");
    let rt = PluginRuntime::new(component_config(&out)).unwrap();
    let d = rt.execute_plugin("image-convert", doc(&fx("apollo11-rot90.jpg"))).unwrap();
    let w = d.working_image.clone().expect("working image");
    assert_eq!((w.width, w.height), (503, 640));
    let d = rt.execute_plugin("face-detection", d).unwrap();
    let faces = d.faces.clone().unwrap().faces;
    assert_eq!(faces.len(), 3);
    let d = rt.execute_plugin("privacy-redact", d).unwrap();
    let working = load_jpeg(&w.path);
    let redacted = load_jpeg(d.privacy.unwrap().redacted_image_path.as_deref().unwrap());
    assert_eq!(redacted.dimensions(), (503, 640));
    for f in &faces {
        assert!(f.bbox.x >= 0.0 && f.bbox.y >= 0.0 && f.bbox.x + f.bbox.width <= 1.0 && f.bbox.y + f.bbox.height <= 1.0);
        let cx = ((f.bbox.x + f.bbox.width / 2.0) * 503.0) as u32;
        let cy = ((f.bbox.y + f.bbox.height / 2.0) * 640.0) as u32;
        assert_eq!(redacted.get_pixel(cx, cy).0, [0, 0, 0], "face centre of {} is not blacked out", f.id);
        assert_ne!(working.get_pixel(cx, cy).0, [0, 0, 0]);
    }
    let (o, r) = (working.get_pixel(3, 3).0, redacted.get_pixel(3, 3).0);
    assert!(o.iter().zip(r.iter()).all(|(a, b)| (*a as i32 - *b as i32).abs() <= 8), "{:?} vs {:?}", o, r);
    let _ = std::fs::remove_dir_all(&out);
}

// image-convert AC-11: image-convert follows metadata in every shipped profile and AppConfig::default(), is enabled with grant filesystem and own resource_limits
#[test]
fn test_ac11_enabled_first_after_metadata() {
    let root = common::repo_root();
    let mut configs = vec![("default".to_string(), AppConfig::default())];
    for name in common::PROFILES {
        configs.push((name.to_string(), AppConfig::load_from_file(root.join(name)).unwrap()));
    }
    for (name, cfg) in configs {
        assert_eq!(&cfg.pipeline[..2], &["metadata".to_string(), "image-convert".to_string()], "{}", name);
        let ic = &cfg.plugins["image-convert"];
        assert!(ic.enabled, "{}", name);
        assert_eq!(ic.capabilities, vec!["filesystem".to_string()], "{}", name);
        let (limits, warnings) = cfg.plugin_limits("image-convert");
        assert!(warnings.is_empty(), "{}: {:?}", name, warnings);
        assert!(limits.max_memory_bytes >= 1_073_741_824 && limits.timeout_ms >= 30_000, "{}: {:?}", name, limits);
    }
    let makefile = std::fs::read_to_string(root.join("Makefile")).unwrap();
    assert!(makefile.contains("PLUGINS := metadata image-convert "), "Makefile PLUGINS");
}

// image-convert AC-11: a 48 MP HEIC (8064 x 6048) runs through the host within the limits of AppConfig::default()
#[test]
fn test_ac11_48mp_heic_within_limits() {
    let out = common::temp_dir("ic_ac11");
    let config = component_config(&out);
    let rt = PluginRuntime::new(config).unwrap();
    let start = Instant::now();
    let d = rt.execute_plugin("image-convert", doc(&fx("gradient-48mp.heic"))).unwrap();
    eprintln!("48 MP HEIC via host: {:?}", start.elapsed());
    let w = d.working_image.expect("working image within the default limits");
    assert_eq!((w.width, w.height, w.converted), (8064, 6048, true));
    assert_eq!(image::image_dimensions(&w.path).unwrap(), (8064, 6048));
    let _ = std::fs::remove_dir_all(&out);
}

// image-convert AC-12: working_image survives host -> component -> host (WIT 0.3.0) and the JSON of the document unchanged
#[test]
fn test_ac12_working_image_roundtrip() {
    let wit = std::fs::read_to_string(common::repo_root().join("wit/media-plugin.wit")).unwrap();
    assert!(wit.starts_with("package wasmgeohost:media@0.3.0;"));
    assert!(wit.contains("record working-image") && wit.contains("working-image: option<working-image>"));

    let out = common::temp_dir("ic_ac12");
    let mut d = doc(&face_fx("apollo11.jpg"));
    let w = WorkingImage { path: "/x/ü ß.working.jpg".to_string(), source_format: "heic".to_string(), width: 4032, height: 3024, converted: true };
    d.working_image = Some(w.clone());
    // countryflag does not touch working_image; the document passes the WIT boundary twice.
    let mut config = common::with_components(AppConfig::default());
    config.component_cache = false;
    let rt = PluginRuntime::new(config).unwrap();
    let back = rt.execute_plugin("countryflag", d.clone()).unwrap();
    assert_eq!(back.working_image, Some(w.clone()));

    let json = serde_json::to_string(&d).unwrap();
    assert!(json.contains("\"working_image\":{\"path\":"));
    let parsed: UnifiedDocument = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.working_image, Some(w));
    // Older documents without the field stay readable.
    let old = serde_json::to_string(&doc("/a.jpg")).unwrap();
    assert!(!old.contains("working_image"));
    assert_eq!(serde_json::from_str::<UnifiedDocument>(&old).unwrap().working_image, None);
    let _ = std::fs::remove_dir_all(&out);
}

// image-convert AC-13: the watcher accepts .bmp (MIME image/bmp) and skips files ending in .working.jpg
#[test]
fn test_ac13_bmp_and_working_jpg() {
    assert!(is_supported_media_file(Path::new("/photos/scan.bmp")));
    assert!(is_supported_media_file(Path::new("/photos/scan.BMP")));
    assert_eq!(detect_mime_type(Path::new("/photos/scan.bmp")), "image/bmp");
    assert!(is_supported_media_file(Path::new("/photos/a.heif")));
    assert_eq!(detect_mime_type(Path::new("/photos/a.heif")), "image/heif");
    assert!(!is_supported_media_file(Path::new("/photos/abc.working.jpg")));
    assert!(!is_supported_media_file(Path::new("/photos/abc.redacted.jpg")));
    assert!(is_supported_media_file(Path::new("/photos/working.jpg")));
}

// image-convert AC-14: settings out of range are clamped to the range with a warning, unreadable values fall back to the default with a warning
#[test]
fn test_ac14_invalid_settings() {
    let defaults = ImageConvertSettings::default();
    assert_eq!((defaults.output_dir.as_str(), defaults.quality, defaults.max_pixels), ("cache/working", 95, 50_000_000));
    for (key, value, expected) in [
        ("quality", "0", (1, 50_000_000)),
        ("quality", "101", (100, 50_000_000)),
        ("max_pixels", "999999", (95, 1_000_000)),
        ("max_pixels", "100000001", (95, 100_000_000)),
        ("quality", "high", (95, 50_000_000)),
    ] {
        let caps = common::LogCaps::default();
        let map = HashMap::from([(key.to_string(), value.to_string())]);
        let s = ImageConvertSettings::parse(&defaults, &map, &caps);
        assert_eq!((s.quality, s.max_pixels), expected, "{}={}", key, value);
        let warnings = caps.warnings();
        assert!(warnings.iter().any(|w| w.contains("image-convert") && w.contains(key) && w.contains(value)), "{}={}: {:?}", key, value, warnings);
    }
    let caps = common::LogCaps::default();
    let map = HashMap::from([("quality".to_string(), "80".to_string()), ("max_pixels".to_string(), "1000000".to_string())]);
    let s = ImageConvertSettings::parse(&defaults, &map, &caps);
    assert_eq!((s.quality, s.max_pixels), (80, 1_000_000));
    assert!(caps.warnings().is_empty());
}

// image-convert AC-15: SOURCES.md names origin, licence, generation command and SHA-256 of every test image; NOTICE and README name heic-rs and oxideav-avif with licence; no C dependency
#[test]
fn test_ac15_sources_and_notice() {
    let root = common::repo_root();
    let dir = root.join("crates/host/tests/fixtures/formats");
    let sources = std::fs::read_to_string(dir.join("SOURCES.md")).unwrap();
    assert!(sources.contains("apollo11.jpg") && sources.contains("Public domain") && sources.contains("generate.sh"));
    for path in files_in(&dir) {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if name == "SOURCES.md" {
            continue;
        }
        let hash = format!("{:x}", Sha256::digest(std::fs::read(&path).unwrap()));
        assert!(sources.contains(&format!("`{}`: `{}`", name, hash)), "SOURCES.md lacks the SHA-256 of {}", name);
    }
    for file in ["NOTICE", "plugins/image-convert/README.md"] {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        for needle in ["heic-rs", "oxideav-avif", "MIT"] {
            assert!(text.contains(needle), "{} lacks {}", file, needle);
        }
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let tree = std::process::Command::new(cargo)
        .current_dir(&root)
        .args(["tree", "-p", "flagme-plugin-image-convert", "--target", "wasm32-wasip2", "-e", "normal,build", "--prefix", "none"])
        .output()
        .unwrap();
    assert!(tree.status.success());
    let tree = String::from_utf8(tree.stdout).unwrap();
    for line in tree.lines() {
        let krate = line.split_whitespace().next().unwrap_or("");
        assert!(!krate.ends_with("-sys") && krate != "cc" && krate != "cmake", "C dependency: {}", line);
    }
    assert!(tree.lines().any(|l| l.starts_with("heic-rs v0.1.5")) && tree.lines().any(|l| l.starts_with("oxideav-avif v0.0.18")));
}
