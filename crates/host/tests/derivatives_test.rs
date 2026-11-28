mod common;

use chrono::{TimeZone, Utc};
use common::*;
use flagme_plugin_duplicate::{compute_dhash, hamming_distance, DuplicatePlugin};
use flagme_plugin_thumbnail::{ThumbnailPlugin, ThumbnailSettings};
use flagme_plugin_weather_enrich::WeatherEnrichPlugin;
use flagme_sdk::{FileInfo, LocationInfo, Plugin, PluginContext, TimelineInfo, UnifiedDocument};
use image::codecs::jpeg::JpegEncoder;
use image::{ImageFormat, Rgb, RgbImage};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

fn doc(path: &str) -> UnifiedDocument {
    let filename = Path::new(path).file_name().unwrap().to_string_lossy().to_string();
    UnifiedDocument::new(
        "doc-1",
        FileInfo {
            path: path.to_string(),
            filename,
            size_bytes: 0,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    )
}

fn ctx(caps: &Arc<LogCaps>, pairs: &[(&str, &str)]) -> PluginContext {
    let settings: HashMap<String, String> =
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    PluginContext::with_settings(caps.clone(), settings)
}

/// Motif with large-scale structure: diagonal gradient plus a bright disc.
fn motif(w: u32, h: u32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let (cx, cy) = (w as i32 * 2 / 3, h as i32 / 3);
        let d2 = (x as i32 - cx).pow(2) + (y as i32 - cy).pow(2);
        let base = ((x + y) * 255 / (w + h)) as u8;
        if d2 < (h as i32 / 4).pow(2) {
            Rgb([250, 240, 200])
        } else {
            Rgb([base, base / 2, 255 - base])
        }
    })
}

fn encode(img: &RgbImage, format: ImageFormat, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    match format {
        ImageFormat::Jpeg => JpegEncoder::new_with_quality(&mut out, quality).encode_image(img).unwrap(),
        _ => img.write_to(&mut std::io::Cursor::new(&mut out), format).unwrap(),
    }
    out
}

// no-synthetic-derivatives AC-1: duplicate leaves doc.duplicate and file.sha256 untouched for unreadable files and warns
#[test]
fn test_ac1_duplicate_unreadable_file() {
    let caps = Arc::new(LogCaps::default());
    let out = DuplicatePlugin::new().process(doc("/nonexistent/a.jpg"), &ctx(&caps, &[])).unwrap();
    assert!(out.duplicate.is_none());
    assert!(out.file.sha256.is_none());
    assert!(caps.warnings().iter().any(|w| w.starts_with("duplicate: cannot read '/nonexistent/a.jpg'")));
    assert!(caps.kv.lock().unwrap().is_empty(), "nothing indexed");
}

// no-synthetic-derivatives AC-2: sha256 hashes the file bytes; phash is a dHash over decoded pixels that is stable across encodings
#[test]
fn test_ac2_sha256_and_dhash() {
    let dir = temp_dir("deriv_ac2");
    let img = motif(160, 120);
    let jpeg_hi = encode(&img, ImageFormat::Jpeg, 95);
    let path = write_file(&dir, "a.jpg", &jpeg_hi);

    let caps = Arc::new(LogCaps::default());
    let out = DuplicatePlugin::new().process(doc(&path), &ctx(&caps, &[])).unwrap();
    let dup = out.duplicate.unwrap();
    let expected_sha = format!("{:x}", Sha256::digest(&jpeg_hi));
    assert_eq!(dup.sha256, expected_sha);
    assert_eq!(out.file.sha256.as_deref(), Some(expected_sha.as_str()));
    assert_eq!(dup.phash, compute_dhash(&jpeg_hi));

    let reference = compute_dhash(&jpeg_hi).unwrap();
    for (name, bytes) in [
        ("jpeg q60", encode(&img, ImageFormat::Jpeg, 60)),
        ("png", encode(&img, ImageFormat::Png, 0)),
    ] {
        let d = hamming_distance(&reference, &compute_dhash(&bytes).unwrap()).unwrap();
        assert!(d <= 4, "{}: Hamming distance {} > 4", name, d);
    }
    let other = image::imageops::flip_horizontal(&img);
    let d = hamming_distance(&reference, &compute_dhash(&encode(&other, ImageFormat::Jpeg, 95)).unwrap()).unwrap();
    assert!(d >= 16, "different motif only {} bits apart", d);
    let _ = std::fs::remove_dir_all(&dir);
}

// no-synthetic-derivatives AC-2: an undecodable file gets a sha256 but no phash
#[test]
fn test_ac2_undecodable_image_has_no_phash() {
    let dir = temp_dir("deriv_ac2b");
    let bytes: Vec<u8> = (0..2048u32).map(|i| (i * 7919 % 251) as u8).collect();
    let path = write_file(&dir, "blob.heic", &bytes);
    let caps = Arc::new(LogCaps::default());
    let dup = DuplicatePlugin::new().process(doc(&path), &ctx(&caps, &[])).unwrap().duplicate.unwrap();
    assert_eq!(dup.sha256, format!("{:x}", Sha256::digest(&bytes)));
    assert!(dup.phash.is_none());
    assert!(!caps.kv.lock().unwrap().keys().any(|k| k.starts_with("phash")), "no perceptual index entry");
    let json = serde_json::to_value(&dup).unwrap();
    assert!(json.get("phash").is_none(), "None is not serialized");
    let _ = std::fs::remove_dir_all(&dir);
}

fn weather_doc(with_time: bool) -> UnifiedDocument {
    let mut d = doc("/media/photo.jpg");
    d.location = Some(LocationInfo { latitude: Some(52.52), longitude: Some(13.40), ..Default::default() });
    if with_time {
        d.timeline = Some(TimelineInfo {
            taken_at: Some(Utc.with_ymd_and_hms(2025, 7, 1, 12, 0, 0).unwrap()),
            imported_at: Utc::now(),
            modified_at: None,
            resolution_source: "exif_datetime_original".to_string(),
        });
    }
    d
}

// no-synthetic-derivatives AC-3: without measured data weather stays None, nothing is cached and a warning is logged
#[test]
fn test_ac3_weather_without_measurement() {
    for (body, reason) in [(None, "http"), (Some("not json".to_string()), "unreadable response")] {
        let caps = Arc::new(LogCaps { http_body: body, ..Default::default() });
        let out = WeatherEnrichPlugin::new().process(weather_doc(true), &ctx(&caps, &[])).unwrap();
        assert!(out.weather.is_none(), "{}", reason);
        assert!(caps.kv.lock().unwrap().is_empty(), "{}: nothing cached", reason);
        assert!(
            caps.warnings().iter().any(|w| w.starts_with("weather-enrich: no measured weather for '/media/photo.jpg'") && w.contains(reason)),
            "{}: {:?}",
            reason,
            caps.warnings()
        );
    }
}

// no-synthetic-derivatives AC-3: without a capture time nothing is queried and weather stays None
#[test]
fn test_ac3_weather_without_capture_time() {
    let caps = Arc::new(LogCaps { http_body: Some("{}".to_string()), ..Default::default() });
    let out = WeatherEnrichPlugin::new().process(weather_doc(false), &ctx(&caps, &[])).unwrap();
    assert!(out.weather.is_none());
    assert!(caps.http_urls.lock().unwrap().is_empty(), "no request without capture time");
    assert!(caps
        .warnings()
        .contains(&"weather-enrich: no capture time for '/media/photo.jpg'; weather not queried".to_string()));
}

// no-synthetic-derivatives AC-4: the setting offline_fallback is ignored with a "not supported" warning
#[test]
fn test_ac4_offline_fallback_not_supported() {
    let caps = Arc::new(LogCaps::default());
    let out = WeatherEnrichPlugin::new()
        .process(weather_doc(true), &ctx(&caps, &[("offline_fallback", "true")]))
        .unwrap();
    assert!(out.weather.is_none(), "no estimate even if requested");
    assert!(caps.warnings().contains(
        &"settings[weather-enrich]: 'true' for 'offline_fallback' is not supported yet (the climatological estimate was removed, see ADR 0002), ignored".to_string()
    ));
}

/// JPEG whose EXIF orientation is 6 (rotate 90° clockwise for display).
fn rotated_jpeg(w: u32, h: u32) -> Vec<u8> {
    let mut tiff = b"II\x2a\x00".to_vec();
    tiff.extend(8u32.to_le_bytes());
    tiff.extend(1u16.to_le_bytes());
    tiff.extend(0x0112u16.to_le_bytes()); // Orientation
    tiff.extend(3u16.to_le_bytes()); // SHORT
    tiff.extend(1u32.to_le_bytes());
    tiff.extend([6, 0, 0, 0]);
    tiff.extend(0u32.to_le_bytes());
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend(tiff);
    let jpeg = encode(&motif(w, h), ImageFormat::Jpeg, 90);
    let mut out = jpeg[..2].to_vec();
    out.extend([0xFF, 0xE1]);
    out.extend(((payload.len() + 2) as u16).to_be_bytes());
    out.extend(payload);
    out.extend(&jpeg[2..]);
    out
}

// no-synthetic-derivatives AC-5: every thumbnail entry is an existing JPEG with the listed size, aspect ratio and orientation kept
#[test]
fn test_ac5_thumbnails_are_real_files() {
    let dir = temp_dir("deriv_ac5");
    let path = write_file(&dir, "photo.jpg", &rotated_jpeg(300, 200));
    // JPEG explicitly: this criterion checks JPEG files; AVIF is the default since ADR 0012.
    let plugin = ThumbnailPlugin::with_settings(ThumbnailSettings {
        output_dir: dir.join("thumbs").to_string_lossy().to_string(),
        format: flagme_plugin_thumbnail::ThumbnailFormat::Jpeg,
        ..Default::default()
    });
    let caps = Arc::new(LogCaps::default());
    let thumbs = plugin.process(doc(&path), &ctx(&caps, &[])).unwrap().thumbnails;

    assert_eq!(thumbs.len(), 3);
    for t in &thumbs {
        let bytes = std::fs::read(&t.path).unwrap_or_else(|_| panic!("{} missing", t.path));
        assert_eq!(image::guess_format(&bytes).unwrap(), ImageFormat::Jpeg);
        let img = image::load_from_memory(&bytes).unwrap();
        assert_eq!((img.width(), img.height()), (t.width, t.height), "{}", t.size_label);
    }
    // Displayed size is 200x300 (portrait after orientation 6).
    assert_eq!(thumbs[0].height, 128);
    assert!((85..=86).contains(&thumbs[0].width), "128px: {}x{}", thumbs[0].width, thumbs[0].height);
    for t in &thumbs[1..] {
        assert_eq!((t.width, t.height), (200, 300), "{} is not upscaled", t.size_label);
    }
    assert!(caps.warnings().is_empty(), "{:?}", caps.warnings());
    let _ = std::fs::remove_dir_all(&dir);
}

// no-synthetic-derivatives AC-6: unreadable or undecodable sources and write failures produce no entries and no placeholder files
#[test]
fn test_ac6_thumbnail_failures() {
    let dir = temp_dir("deriv_ac6");
    let out_dir = dir.join("thumbs");
    let plugin = ThumbnailPlugin::with_settings(ThumbnailSettings {
        output_dir: out_dir.to_string_lossy().to_string(),
        ..Default::default()
    });
    let garbage = write_file(&dir, "broken.jpg", &[0x42; 512]);
    let valid = write_file(&dir, "ok.jpg", &plain_jpeg(40, 30));

    let cases: [(&str, bool, &str); 3] = [
        ("/nonexistent/x.jpg", false, "cannot read"),
        (&garbage, false, "cannot decode"),
        (&valid, true, "cannot write"),
    ];
    for (path, deny_write, expect) in cases {
        let caps = Arc::new(LogCaps { deny_write, ..Default::default() });
        let thumbs = plugin.process(doc(path), &ctx(&caps, &[])).unwrap().thumbnails;
        assert!(thumbs.is_empty(), "{}", expect);
        assert!(
            caps.warnings().iter().any(|w| w.starts_with("thumbnail:") && w.contains(expect)),
            "{}: {:?}",
            expect,
            caps.warnings()
        );
    }
    assert!(!out_dir.exists(), "no placeholder files written");
    let _ = std::fs::remove_dir_all(&dir);
}
