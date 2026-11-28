//! Built-in PaddleOCR engine (ADR 0009) on CC0 test images.

mod common;

use flagme_host::{AppConfig, PluginRuntime};
use flagme_plugin_ocr_text::paddle::{PaddleOcrEngine, DET_MODEL, ENGINE_NAME, REC_MODEL};
use flagme_plugin_ocr_text::{OcrEngine, OcrTextPlugin};
use flagme_sdk::{FileInfo, OcrInfo, Plugin, PluginContext, UnifiedDocument};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

fn fixture(name: &str) -> PathBuf {
    common::repo_root().join("crates/host/tests/fixtures/ocr").join(name)
}

fn doc(path: &str) -> UnifiedDocument {
    UnifiedDocument::new(
        "ocr-doc",
        FileInfo {
            path: path.to_string(),
            filename: Path::new(path).file_name().unwrap().to_string_lossy().to_string(),
            size_bytes: 0,
            mime_type: "image/png".to_string(),
            sha256: None,
        },
    )
}

/// Runs the plugin natively with real file access; returns the result and the warnings.
fn ocr(path: &str, settings: &[(&str, &str)]) -> (Option<OcrInfo>, Vec<String>) {
    let caps = Arc::new(common::LogCaps::default());
    let map: HashMap<String, String> = settings.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let ctx = PluginContext::with_settings(caps.clone(), map);
    let out = OcrTextPlugin::new().process(doc(path), &ctx).unwrap();
    let warnings = caps.logs.lock().unwrap().iter().filter(|(l, _)| l == "warn").map(|(_, m)| m.clone()).collect();
    (out.ocr, warnings)
}

fn lines(info: &OcrInfo) -> Vec<&str> {
    info.regions.iter().map(|r| r.text.as_str()).collect()
}

// ocr-text-paddle AC-1: on german-text.png ocr-text returns exactly 4 lines with umlauts and ß; combined_text holds all four separated by newlines
#[test]
fn test_ac1_german_text_lines() {
    let (info, warnings) = ocr(fixture("german-text.png").to_str().unwrap(), &[]);
    let info = info.unwrap_or_else(|| panic!("no result: {:?}", warnings));
    let l = lines(&info);
    assert_eq!(l.len(), 4, "{:?}", l);
    assert_eq!(l[0], "Grüße aus München");
    assert_eq!(l[1], "Straße 12, Größe: 3,5 m");
    assert!(l[2].starts_with("Öffnungszeiten:"), "{:?}", l[2]);
    assert_eq!(l[3], "The quick brown fox jumps 2026");
    assert_eq!(info.combined_text, l.join("\n"));
    assert!(warnings.is_empty(), "{:?}", warnings);
}

// ocr-text-paddle AC-2: on the CC0 photo strass.jpg the result contains "Straß" and "Rückholz"
#[test]
fn test_ac2_photo_with_umlaut_and_sharp_s() {
    let info = ocr(fixture("strass.jpg").to_str().unwrap(), &[]).0.expect("result");
    assert!(info.combined_text.contains("Straß"), "{:?}", info.combined_text);
    assert!(info.combined_text.contains("Rückholz"), "{:?}", info.combined_text);
}

// ocr-text-paddle AC-3: an image without text gives Some with an empty list and an empty combined_text
#[test]
fn test_ac3_no_text() {
    let path = common::repo_root().join("Examples/sample.jpg");
    let (info, warnings) = ocr(path.to_str().unwrap(), &[]);
    let info = info.unwrap_or_else(|| panic!("no result: {:?}", warnings));
    assert!(info.regions.is_empty() && info.combined_text.is_empty(), "{:?}", info);
}

// ocr-text-paddle AC-4: an unreadable file, a truncated JPEG or an unsupported format (HEIC) leave doc.ocr None and warn with path and cause
#[test]
fn test_ac4_unreadable_and_unsupported() {
    let dir = common::temp_dir("paddle_ac4");
    let full = std::fs::read(fixture("strass.jpg")).unwrap();
    let truncated = common::write_file(&dir, "truncated.jpg", &full[..full.len() / 3]);
    let heic = common::repo_root().join("Examples/sample.heic").to_string_lossy().to_string();
    let missing = dir.join("missing.jpg").to_string_lossy().to_string();
    for (path, cause) in [(missing.as_str(), "cannot read"), (truncated.as_str(), "truncated"), (heic.as_str(), "format")] {
        let (info, warnings) = ocr(path, &[]);
        assert!(info.is_none(), "{}: {:?}", path, info);
        assert!(warnings.iter().any(|w| w.starts_with("ocr-text:") && w.contains(path) && w.contains(cause)), "{}: {:?}", path, warnings);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ocr-text-paddle AC-5: every line has a box within 0..1 and a confidence within 0..1, the order is reading order, the same image twice gives the same result
#[test]
fn test_ac5_normalised_ordered_deterministic() {
    let path = fixture("german-text.png").to_string_lossy().to_string();
    let a = ocr(&path, &[]).0.unwrap();
    let b = ocr(&path, &[]).0.unwrap();
    assert_eq!(a, b);
    for r in &a.regions {
        let bb = &r.bbox;
        assert!(bb.x >= 0.0 && bb.y >= 0.0 && bb.width > 0.0 && bb.height > 0.0, "{:?}", bb);
        assert!(bb.x + bb.width <= 1.0 + 1e-6 && bb.y + bb.height <= 1.0 + 1e-6, "{:?}", bb);
        assert!((0.0..=1.0).contains(&r.confidence), "{}", r.confidence);
    }
    for w in a.regions.windows(2) {
        assert!(w[0].bbox.y < w[1].bbox.y, "{:?} before {:?}", w[0].text, w[1].text);
    }
}

// ocr-text-paddle AC-5: the ocr-text component in the host returns the same lines as the native plugin
#[test]
fn test_ac5_component_matches() {
    let path = fixture("german-text.png").to_string_lossy().to_string();
    let native = ocr(&path, &[]).0.unwrap();
    let mut config = common::with_components(AppConfig::default());
    config.component_cache = false;
    config.pipeline = vec!["ocr-text".to_string()];
    let rt = PluginRuntime::new(config).unwrap();
    let out = rt.execute_plugin("ocr-text", doc(&path)).unwrap().ocr.expect("ocr");
    assert_eq!(lines(&out), lines(&native));
}

// ocr-text-paddle AC-6: min_confidence defaults to 0.5 and still filters; max_regions limits the number of lines read and warns when reached
#[test]
fn test_ac6_min_confidence_and_max_regions() {
    let settings = flagme_plugin_ocr_text::OcrSettings::default();
    assert_eq!((settings.min_confidence, settings.max_regions), (0.5, 200));
    let path = fixture("german-text.png").to_string_lossy().to_string();
    let strict = ocr(&path, &[("min_confidence", "1.0")]).0.unwrap();
    assert!(strict.regions.iter().all(|r| r.confidence >= 1.0));
    let (limited, warnings) = ocr(&path, &[("max_regions", "2")]);
    assert_eq!(limited.unwrap().regions.len(), 2);
    assert!(warnings.iter().any(|w| w.contains("4 text lines found, read the 2 most confident (max_regions)")), "{:?}", warnings);
}

// ocr-text-paddle AC-7: primary_language is always None
#[test]
fn test_ac7_no_primary_language() {
    for name in ["german-text.png", "strass.jpg"] {
        assert_eq!(ocr(fixture(name).to_str().unwrap(), &[]).0.unwrap().primary_language, None, "{}", name);
    }
}

// ocr-text-paddle AC-8: both models are embedded, their SHA-256 match models/SOURCE.md, the Apache-2.0 licence is next to them, and NOTICE and the README name PaddleOCR, PP-OCRv5 and Apache-2.0
#[test]
fn test_ac8_embedded_models_and_attribution() {
    let root = common::repo_root();
    let models = root.join("plugins/ocr-text/models");
    let source = std::fs::read_to_string(models.join("SOURCE.md")).unwrap();
    for (bytes, file) in [(DET_MODEL, "PP-OCRv5_mobile_det.onnx"), (REC_MODEL, "latin_PP-OCRv5_mobile_rec.onnx")] {
        assert_eq!(std::fs::read(models.join(file)).unwrap(), bytes);
        let hash = format!("{:x}", Sha256::digest(bytes));
        assert!(source.contains(&hash), "SOURCE.md lacks {} for {}", hash, file);
    }
    assert!(std::fs::read_to_string(models.join("LICENSE")).unwrap().contains("Apache License"));
    let engine = PaddleOcrEngine::new().unwrap();
    assert_eq!(engine.name(), ENGINE_NAME);
    for file in ["NOTICE", "plugins/ocr-text/README.md"] {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        assert!(text.contains("PaddleOCR") && text.contains("PP-OCRv5") && text.contains("Apache-2.0"), "{} lacks the attribution", file);
    }
}

// ocr-text-paddle AC-9: ocr-text is enabled in config.yaml, config_mac-studio_32GB.yaml and AppConfig::default() with timeout_ms 30000 per plugin and no experimental note; a generated 4000 x 3000 image runs through the host within these limits
#[test]
fn test_ac9_enabled_with_limits_and_large_image() {
    let root = common::repo_root();
    let mut configs = vec![AppConfig::default()];
    for name in ["config.yaml", "config_mac-studio_32GB.yaml"] {
        configs.push(AppConfig::load_from_file(root.join(name)).unwrap());
        let text = std::fs::read_to_string(root.join(name)).unwrap();
        let section = text.split("ocr-text:").nth(1).unwrap().split("face-detection:").next().unwrap();
        assert!(!section.contains("experimental"), "{}: {}", name, section);
    }
    for cfg in &configs {
        assert!(cfg.plugins["ocr-text"].enabled);
        assert_eq!(cfg.plugin_limits("ocr-text").0.timeout_ms, 30_000);
    }

    let dir = common::temp_dir("paddle_ac9");
    let big = image::imageops::resize(&image::open(fixture("strass.jpg")).unwrap().to_rgb8(), 3000, 4000, image::imageops::FilterType::Triangle);
    let path = dir.join("big.jpg");
    big.save(&path).unwrap();
    let mut config = common::with_components(AppConfig::default());
    config.component_cache = false;
    config.pipeline = vec!["ocr-text".to_string()];
    let rt = PluginRuntime::new(config).unwrap();
    let start = Instant::now();
    let out = rt.execute_plugin("ocr-text", doc(path.to_str().unwrap())).unwrap();
    eprintln!("3000x4000 via host: {:?}", start.elapsed());
    assert!(out.ocr.expect("ocr within the limits").combined_text.contains("Straß"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ocr-text-paddle AC-10: text from ocr-text is found by the full-text search: after process of german-text.png, search München finds the document
#[test]
fn test_ac10_full_text_search() {
    let dir = common::temp_dir("paddle_ac10");
    let config: String = std::fs::read_to_string(common::repo_root().join("config.yaml"))
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
    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_flagme-host"))
            .args(args)
            .current_dir(&dir)
            .env_remove("RUST_LOG")
            .output()
            .unwrap()
    };
    let img = fixture("german-text.png").to_string_lossy().to_string();
    let out = run(&["process", &img, "--config", "config.yaml"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let processed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(processed["ocr"]["combined_text"].as_str().unwrap().contains("München"));
    let found = run(&["search", "München", "--config", "config.yaml"]);
    let stdout = String::from_utf8_lossy(&found.stdout);
    assert!(stdout.contains("1 match(es)") && stdout.contains("german-text.png"), "{}", stdout);
    let _ = std::fs::remove_dir_all(&dir);
}

// ocr-text-paddle AC-11: the fixture SOURCES.md names origin, author, licence, edit and SHA-256 for every test image
#[test]
fn test_ac11_fixture_sources() {
    let dir = common::repo_root().join("crates/host/tests/fixtures/ocr");
    let sources = std::fs::read_to_string(dir.join("SOURCES.md")).unwrap();
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "SOURCES.md" {
            continue;
        }
        let line = sources.lines().find(|l| l.starts_with(&format!("| `{}`", name))).unwrap_or_else(|| panic!("{} has no source line", name));
        assert!(line.contains("CC0"), "{}", line);
        let hash = format!("{:x}", Sha256::digest(std::fs::read(entry.path()).unwrap()));
        assert!(sources.contains(&hash), "{}: SHA-256 {} not listed", name, hash);
    }
    assert!(sources.contains("commons.wikimedia.org") && sources.contains("Bruno413") && sources.contains("1600 px"));
}
