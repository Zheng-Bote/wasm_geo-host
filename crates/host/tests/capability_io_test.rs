mod common;

use flagme_host::config::PluginConfig;
use flagme_host::{AppConfig, CapabilityManager, PluginRuntime};
use flagme_plugin_ai_tagging::AiTaggingPlugin;
use flagme_plugin_duplicate::DuplicatePlugin;
use flagme_plugin_face_detection::FaceDetectionPlugin;
use flagme_plugin_metadata::MetadataPlugin;
use flagme_plugin_ocr_text::OcrTextPlugin;
use flagme_plugin_privacy_redact::PrivacyRedactPlugin;
use flagme_plugin_thumbnail::ThumbnailPlugin;
use flagme_plugin_timeline::TimelinePlugin;
use flagme_sdk::{
    BoundingBox, DetectedFace, FacesInfo, FileInfo, HostCapabilities, PluginContext, PluginError,
    Plugin, UnifiedDocument,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

// capability-only-io AC-1: no source file under plugins/*/src uses std::fs, std::process, Command::new or File
#[test]
fn test_ac1_no_direct_io_in_plugins() {
    let mut files = Vec::new();
    for plugin in std::fs::read_dir(repo_root().join("plugins")).unwrap() {
        let src = plugin.unwrap().path().join("src");
        if src.is_dir() {
            rs_files(&src, &mut files);
        }
    }
    assert!(files.len() >= 22, "expected lib.rs + main.rs of 11 plugins, found {}", files.len());

    let forbidden = ["std::fs", "std::process", "Command::new", "File::open", "File::create", "use std::fs"];
    let mut hits = Vec::new();
    for file in &files {
        for (no, line) in std::fs::read_to_string(file).unwrap().lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            if forbidden.iter().any(|f| code.contains(f)) {
                hits.push(format!("{}:{}: {}", file.display(), no + 1, line.trim()));
            }
        }
    }
    assert!(hits.is_empty(), "direct I/O in plugins:\n{}", hits.join("\n"));
}

/// Runtime with every plugin enabled; `filesystem` granted only if `grant` is true.
fn runtime(grant: bool, out_dir: &Path) -> PluginRuntime {
    let mut config = AppConfig::default();
    let out = |sub: &str| out_dir.join(sub).to_string_lossy().to_string();
    let caps = if grant { vec!["filesystem".to_string()] } else { vec![] };
    for name in [
        "metadata", "thumbnail", "duplicate", "timeline", "privacy-redact", "ocr-text", "ai-tagging", "face-detection",
    ] {
        let mut settings = HashMap::new();
        match name {
            "thumbnail" => drop(settings.insert("output_dir".to_string(), out("thumbs"))),
            "privacy-redact" => drop(settings.insert("output_dir".to_string(), out("redacted"))),
            "timeline" => drop(settings.insert("source_priority".to_string(), "filesystem_modified_time".to_string())),
            _ => {}
        }
        config.plugins.insert(
            name.to_string(),
            PluginConfig { enabled: true, path: None, capabilities: caps.clone(), settings },
        );
    }
    let mut rt = PluginRuntime::new(config);
    rt.register_plugin(Arc::new(MetadataPlugin::new()));
    rt.register_plugin(Arc::new(ThumbnailPlugin::new()));
    rt.register_plugin(Arc::new(DuplicatePlugin::new()));
    rt.register_plugin(Arc::new(TimelinePlugin::new()));
    rt.register_plugin(Arc::new(PrivacyRedactPlugin::new()));
    rt.register_plugin(Arc::new(OcrTextPlugin::with_engine(Box::new(common::ByteLenOcrEngine))));
    rt.register_plugin(Arc::new(AiTaggingPlugin::with_model(Box::new(common::ByteLenVisionModel))));
    rt.register_plugin(Arc::new(FaceDetectionPlugin::with_detector(Box::new(common::OneFaceDetector))));
    rt
}

fn run_all(rt: &PluginRuntime, path: &str) -> UnifiedDocument {
    let mut doc = UnifiedDocument::new(
        "cap-io",
        FileInfo {
            path: path.to_string(),
            filename: "photo.jpg".to_string(),
            size_bytes: 0,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    // A face result from upstream, so privacy-redact would have something to redact.
    doc.faces = Some(FacesInfo {
        faces: vec![DetectedFace {
            id: "f".into(),
            bbox: BoundingBox { x: 0.2, y: 0.2, width: 0.4, height: 0.4 },
            confidence: 0.9,
            landmarks: None,
            embedding: None,
            person_id: None,
        }],
        detection_model: "upstream".into(),
    });
    for name in ["metadata", "thumbnail", "duplicate", "timeline", "ocr-text", "ai-tagging"] {
        doc = rt.execute_plugin(name, doc).unwrap();
    }
    doc = rt.execute_plugin("privacy-redact", doc).unwrap();
    let mut probe = doc.clone();
    probe.faces = None;
    let probe = rt.execute_plugin("face-detection", probe).unwrap();
    doc.faces = probe.faces;
    doc
}

// capability-only-io AC-2: without the filesystem grant no plugin reads or writes a file
#[test]
fn test_ac2_no_file_access_without_grant() {
    let dir = common::temp_dir("cap_io_ac2");
    let path = common::write_file(&dir, "photo.jpg", &common::jpeg_with_exif("Make", "Model", Some((48.1, 11.5))));

    // Positive control: with the grant every plugin uses the file.
    let granted = run_all(&runtime(true, &dir.join("granted")), &path);
    assert!(granted.metadata.is_some() && granted.duplicate.is_some());
    assert_eq!(granted.thumbnails.len(), 3);
    assert!(granted.timeline.as_ref().unwrap().modified_at.is_some());
    assert_eq!(granted.privacy.as_ref().unwrap().faces_redacted_count, 1);
    assert!(granted.ocr.is_some() && granted.ai.is_some() && granted.faces.is_some());

    let denied = run_all(&runtime(false, &dir.join("denied")), &path);
    assert!(denied.metadata.is_none(), "metadata read the file");
    assert!(denied.location.is_none());
    assert!(denied.thumbnails.is_empty(), "thumbnail wrote files");
    assert!(denied.duplicate.is_none() && denied.file.sha256.is_none(), "duplicate read the file");
    let timeline = denied.timeline.unwrap();
    assert!(timeline.modified_at.is_none(), "timeline read the modification time");
    assert_ne!(timeline.resolution_source, "filesystem_modified_time");
    let privacy = denied.privacy.unwrap();
    assert_eq!(privacy.faces_redacted_count, 0);
    assert!(privacy.redacted_image_path.is_none());
    assert!(denied.ocr.is_none() && denied.ai.is_none(), "engines received file bytes");
    assert!(denied.faces.is_none(), "face detector received file bytes");
    assert!(!dir.join("denied").exists(), "a file was written without the grant");
    let _ = std::fs::remove_dir_all(&dir);
}

fn manager(granted: &[&str]) -> CapabilityManager {
    CapabilityManager::new(
        "test-plugin",
        granted.iter().map(|s| s.to_string()).collect(),
        Arc::new(RwLock::new(HashMap::new())),
    )
}

// capability-only-io AC-3: fs_modified returns the mtime with the grant, CapabilityDenied without, rejects '..' and is declared in WIT
#[test]
fn test_ac3_fs_modified_capability() {
    let dir = common::temp_dir("cap_io_ac3");
    let path = common::write_file(&dir, "f.jpg", b"data");
    let expected = std::fs::metadata(&path)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    assert_eq!(manager(&["filesystem"]).fs_modified(&path).unwrap(), expected);
    assert!(matches!(manager(&[]).fs_modified(&path), Err(PluginError::CapabilityDenied(_))));
    assert!(matches!(
        manager(&["filesystem"]).fs_modified(&format!("{}/../f.jpg", dir.display())),
        Err(PluginError::Io(_))
    ));
    assert_eq!(flagme_sdk::NoopCapabilities.fs_modified(&path).unwrap(), expected);

    let wit = std::fs::read_to_string(repo_root().join("wit/media-plugin.wit")).unwrap();
    assert!(wit.contains("fs-modified: func(path: string) -> result<u64, string>;"));
    let _ = std::fs::remove_dir_all(&dir);
}

// capability-only-io AC-4: timeline takes modified_at and filesystem_modified_time only from fs_modified
#[test]
fn test_ac4_timeline_uses_fs_modified() {
    let dir = common::temp_dir("cap_io_ac4");
    let path = common::write_file(&dir, "f.jpg", b"data");
    let mtime = manager(&["filesystem"]).fs_modified(&path).unwrap() as i64;
    let settings: HashMap<String, String> =
        [("source_priority".to_string(), "filesystem_modified_time".to_string())].into();
    let doc = UnifiedDocument::new(
        "t",
        FileInfo {
            path: path.clone(),
            filename: "f.jpg".into(),
            size_bytes: 4,
            mime_type: "image/jpeg".into(),
            sha256: None,
        },
    );

    let ctx = PluginContext::with_settings(Arc::new(manager(&["filesystem"])), settings.clone());
    let t = TimelinePlugin::new().process(doc.clone(), &ctx).unwrap().timeline.unwrap();
    assert_eq!(t.modified_at.map(|d| d.timestamp()), Some(mtime));
    assert_eq!(t.taken_at.map(|d| d.timestamp()), Some(mtime));
    assert_eq!(t.resolution_source, "filesystem_modified_time");

    let ctx = PluginContext::with_settings(Arc::new(manager(&[])), settings);
    let t = TimelinePlugin::new().process(doc, &ctx).unwrap().timeline.unwrap();
    assert!(t.modified_at.is_none());
    assert_ne!(t.resolution_source, "filesystem_modified_time");
    let _ = std::fs::remove_dir_all(&dir);
}
