//! Built-in CLIP model of ai-tagging (ADR 0010). Needs `make models`.

mod common;

use flagme_host::{AppConfig, PluginRuntime};
use flagme_plugin_ai_tagging::clip::{ClipModel, Labels, TagParams, LABELS, MODEL_NAME, MODEL_SHA256};
use flagme_plugin_ai_tagging::{AiTaggingPlugin, AiTaggingSettings};
use flagme_sdk::{AiInfo, FileInfo, Plugin, PluginContext, UnifiedDocument};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

const SPACE: &[&str] = &["a space suit", "a spacecraft", "outer space", "a rocket", "the earth from space", "the moon"];

fn fixture(rel: &str) -> PathBuf {
    common::repo_root().join("crates/host/tests/fixtures").join(rel)
}

fn doc(path: &str) -> UnifiedDocument {
    UnifiedDocument::new(
        "ai-doc",
        FileInfo {
            path: path.to_string(),
            filename: Path::new(path).file_name().unwrap().to_string_lossy().to_string(),
            size_bytes: 0,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    )
}

/// Runs the plugin natively (built-in CLIP, absolute model path); returns the result and the warnings.
fn tag(path: &str, extra: &[(&str, &str)]) -> (Option<AiInfo>, Vec<String>) {
    let caps = Arc::new(common::LogCaps::default());
    let mut map: HashMap<String, String> = HashMap::new();
    map.insert("model_path".to_string(), common::clip_model_path().to_string_lossy().to_string());
    for (k, v) in extra {
        map.insert(k.to_string(), v.to_string());
    }
    let ctx = PluginContext::with_settings(caps.clone(), map);
    let out = AiTaggingPlugin::new().process(doc(path), &ctx).unwrap();
    let warnings = caps.logs.lock().unwrap().iter().filter(|(l, _)| l == "warn").map(|(_, m)| m.clone()).collect();
    (out.ai, warnings)
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn embedding(rel: &str) -> Vec<f32> {
    tag(fixture(rel).to_str().unwrap(), &[]).0.expect("ai").embedding.expect("embedding")
}

// ai-tagging-clip AC-1: for the Apollo 11 and Expedition 64 photos ai-tagging returns a normalised 512-d embedding and tags; the best tag is from the space topic of the label list and reaches min_confidence
#[test]
fn test_ac1_space_photos() {
    for rel in ["faces/apollo11.jpg", "faces/expedition64.jpg"] {
        let (ai, warnings) = tag(fixture(rel).to_str().unwrap(), &[]);
        let ai = ai.unwrap_or_else(|| panic!("{}: {:?}", rel, warnings));
        let e = ai.embedding.as_ref().unwrap();
        assert_eq!(e.len(), 512);
        assert!((cosine(e, e) - 1.0).abs() < 1e-4, "not normalised");
        let best = ai.tags.first().unwrap_or_else(|| panic!("{}: no tags", rel));
        assert!(SPACE.contains(&best.label.as_str()), "{}: best tag {:?}", rel, ai.tags);
        assert!(best.confidence >= AiTaggingSettings::default().min_confidence);
        assert!(warnings.is_empty(), "{:?}", warnings);
    }
}

// ai-tagging-clip AC-2: the two NASA photos are more similar to each other than either is to strass.jpg and german-text.png
#[test]
fn test_ac2_similarity() {
    let (a, e) = (embedding("faces/apollo11.jpg"), embedding("faces/expedition64.jpg"));
    let nasa = cosine(&a, &e);
    for other in ["ocr/strass.jpg", "ocr/german-text.png"] {
        let o = embedding(other);
        assert!(nasa > cosine(&a, &o) && nasa > cosine(&e, &o), "{}: nasa {} vs {} / {}", other, nasa, cosine(&a, &o), cosine(&e, &o));
    }
}

// ai-tagging-clip AC-2: after processing all four images, flagme-host similar returns the Expedition 64 photo as the first match for the Apollo 11 photo
#[test]
fn test_ac2_similar_command() {
    let dir = common::temp_dir("clip_ac2");
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
    // The relative default model_path resolves against the working directory.
    std::fs::create_dir_all(dir.join("models")).unwrap();
    std::fs::copy(common::clip_model_path(), dir.join("models/clip-vit-b32-vision-int8.onnx")).unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_flagme-host")).args(args).current_dir(&dir).env_remove("RUST_LOG").output().unwrap()
    };
    let mut ids = HashMap::new();
    for rel in ["faces/apollo11.jpg", "faces/expedition64.jpg", "ocr/strass.jpg", "ocr/german-text.png"] {
        let out = run(&["process", fixture(rel).to_str().unwrap(), "--config", "config.yaml"]);
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(v["ai"]["embedding"].as_array().map(|a| a.len()) == Some(512), "{}: {}", rel, v["ai"]);
        ids.insert(rel, v["id"].as_str().unwrap().to_string());
    }
    let out = run(&["similar", &ids["faces/apollo11.jpg"], "--config", "config.yaml"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let first = stdout
        .lines()
        .filter_map(|l| l.split("photo_id: ").nth(1).and_then(|r| r.split(' ').next()))
        .find(|id| *id != ids["faces/apollo11.jpg"])
        .unwrap_or_else(|| panic!("no match: {}", stdout));
    assert_eq!(first, ids["faces/expedition64.jpg"], "{}", stdout);
    let _ = std::fs::remove_dir_all(&dir);
}

// ai-tagging-clip AC-3: at most max_tags tags, all >= min_confidence, sorted descending, model clip-vit-b32; description is empty
#[test]
fn test_ac3_tag_rules() {
    let path = fixture("faces/apollo11.jpg").to_string_lossy().to_string();
    for (max, min) in [(5usize, 0.10f32), (2, 0.0), (20, 0.05)] {
        let (ai, _) = tag(&path, &[("max_tags", &max.to_string()), ("min_confidence", &min.to_string())]);
        let ai = ai.unwrap();
        assert!(ai.tags.len() <= max && !ai.tags.is_empty(), "{:?}", ai.tags);
        for w in ai.tags.windows(2) {
            assert!(w[0].confidence >= w[1].confidence);
        }
        assert!(ai.tags.iter().all(|t| t.confidence >= min && t.model == MODEL_NAME), "{:?}", ai.tags);
        assert_eq!(ai.description, "");
    }
    // A synthetic pattern has no label above the default threshold: no invented tags.
    let (ai, _) = tag(common::repo_root().join("Examples/sample.jpg").to_str().unwrap(), &[]);
    let ai = ai.unwrap();
    assert!(ai.tags.iter().all(|t| t.confidence >= 0.10), "{:?}", ai.tags);
}

// ai-tagging-clip AC-4: a missing model file or a wrong SHA-256 leaves doc.ai None and warns with path, cause and `make models`; an unreadable image, a truncated JPEG or HEIC also give None with a warning
#[test]
fn test_ac4_missing_or_wrong_model_and_bad_images() {
    let dir = common::temp_dir("clip_ac4");
    let photo = fixture("faces/apollo11.jpg").to_string_lossy().to_string();
    let wrong = common::write_file(&dir, "wrong.onnx", b"not a model");
    let missing = dir.join("missing.onnx").to_string_lossy().to_string();
    for (model, cause) in [(missing.as_str(), "No such file"), (wrong.as_str(), "SHA-256")] {
        let caps = Arc::new(common::LogCaps::default());
        let ctx = PluginContext::with_settings(caps.clone(), [("model_path".to_string(), model.to_string())].into());
        let out = AiTaggingPlugin::new().process(doc(&photo), &ctx).unwrap();
        assert!(out.ai.is_none());
        let logs = caps.logs.lock().unwrap().clone();
        assert!(
            logs.iter().any(|(l, m)| l == "warn" && m.contains(model) && m.contains(cause) && m.contains("make models")),
            "{}: {:?}",
            model,
            logs
        );
    }
    let full = std::fs::read(fixture("faces/apollo11.jpg")).unwrap();
    let truncated = common::write_file(&dir, "truncated.jpg", &full[..full.len() / 3]);
    let heic = common::repo_root().join("Examples/sample.heic").to_string_lossy().to_string();
    let unreadable = dir.join("nope.jpg").to_string_lossy().to_string();
    for (path, cause) in [(unreadable.as_str(), "cannot read"), (truncated.as_str(), "truncated"), (heic.as_str(), "format")] {
        let (ai, warnings) = tag(path, &[]);
        assert!(ai.is_none(), "{}", path);
        assert!(warnings.iter().any(|w| w.contains(path) && w.contains(cause)), "{}: {:?}", path, warnings);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ai-tagging-clip AC-5: make models downloads from the fixed revision, checks the SHA-256 and removes a mismatching file; models/ is ignored by git except its .gitignore and SHA256SUMS
#[test]
fn test_ac5_models_target() {
    let root = common::repo_root();
    let makefile = std::fs::read_to_string(root.join("Makefile")).unwrap();
    assert!(makefile.contains("resolve/6b7c523703ae26e39be4b03774966f8571c7f523"), "fixed revision");
    assert!(makefile.contains("shasum -a 256 -c") && makefile.contains("SHA-256 mismatch, removed") && makefile.contains("rm -f \"models/$$name\""));
    assert!(makefile.contains("test: models"));
    let sums = std::fs::read_to_string(root.join("models/SHA256SUMS")).unwrap();
    assert!(sums.contains(&format!("{}  clip-vit-b32-vision-int8.onnx", MODEL_SHA256)));
    let ignore = std::fs::read_to_string(root.join("models/.gitignore")).unwrap();
    assert!(ignore.lines().any(|l| l == "*") && ignore.contains("!SHA256SUMS") && ignore.contains("!.gitignore"));
    let hash = format!("{:x}", Sha256::digest(std::fs::read(common::clip_model_path()).unwrap()));
    assert_eq!(hash, MODEL_SHA256, "downloaded model matches SHA256SUMS");
}

// ai-tagging-clip AC-6: labels.txt contains no label from the block list of personal attributes and no duplicates
#[test]
fn test_ac6_labels_without_personal_attributes() {
    const BLOCKED: &[&str] = &[
        "man", "men", "woman", "women", "girl", "girls", "boy", "boys", "child", "children", "kid", "kids", "baby", "toddler", "teenager",
        "old", "young", "elderly", "senior", "male", "female", "gay", "lesbian", "black", "white", "asian", "african", "european", "latino",
        "arab", "indian", "muslim", "christian", "jewish", "hindu", "buddhist", "beautiful", "ugly", "pretty", "handsome", "fat", "thin",
        "skinny", "disabled", "sick", "ill", "nurse", "doctor", "astronaut", "astronauts", "police", "policeman", "soldier", "teacher",
        "worker", "farmer", "chef", "bride", "groom", "mother", "father",
    ];
    let text = std::fs::read_to_string(common::repo_root().join("plugins/ai-tagging/labels/labels.txt")).unwrap();
    let mut seen = std::collections::HashSet::new();
    for line in text.lines() {
        let label = line.split('#').next().unwrap().trim();
        if label.is_empty() {
            continue;
        }
        assert!(seen.insert(label.to_string()), "duplicate {}", label);
        for word in label.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
            assert!(!BLOCKED.contains(&word.to_lowercase().as_str()), "'{}' contains the personal attribute '{}'", label, word);
        }
    }
    assert!(seen.len() >= 200, "{}", seen.len());
}

// ai-tagging-clip AC-7: labels.bin matches labels.txt (SHA-256 in the header, same count and labels); the plugin rejects a labels.bin with a wrong version or dimension
#[test]
fn test_ac7_labels_bin_matches_txt() {
    let text = std::fs::read(common::repo_root().join("plugins/ai-tagging/labels/labels.txt")).unwrap();
    let labels = Labels::parse(LABELS).unwrap();
    let sha: [u8; 32] = Sha256::digest(&text).into();
    assert_eq!(labels.labels_sha256, sha, "labels.bin is stale: run `make labels`");
    let txt: Vec<String> = String::from_utf8(text).unwrap().lines().map(|l| l.split('#').next().unwrap().trim().to_string()).filter(|l| !l.is_empty()).collect();
    assert_eq!(labels.names, txt);
    let text_model = std::fs::read(common::repo_root().join("models/clip-vit-b32-text-int8.onnx")).expect("run `make models`");
    let model_sha: [u8; 32] = Sha256::digest(&text_model).into();
    assert_eq!(labels.text_model_sha256, model_sha);
    let mut v2 = LABELS.to_vec();
    v2[4] = 2;
    assert!(Labels::parse(&v2).unwrap_err().contains("version"));
    let mut dim = LABELS.to_vec();
    dim[12] = 1;
    assert!(Labels::parse(&dim).unwrap_err().contains("dimension"));
}

// ai-tagging-clip AC-8: the component in the host returns nearly the same result as the native plugin (cosine >= 0.99, same best tag, shared tag probabilities within 0.05), within 512 MB and 30 s, also for a 4000 x 3000 image
#[test]
fn test_ac8_component_matches_within_limits() {
    let path = fixture("faces/apollo11.jpg").to_string_lossy().to_string();
    let native = tag(&path, &[]).0.unwrap();
    let mut config = common::with_components(AppConfig::default());
    config.component_cache = false;
    config.pipeline = vec!["ai-tagging".to_string()];
    config.plugins.get_mut("ai-tagging").unwrap().settings.insert("model_path".to_string(), common::clip_model_path().to_string_lossy().to_string());
    let (limits, _) = config.plugin_limits("ai-tagging");
    assert_eq!((limits.max_memory_bytes, limits.timeout_ms), (536_870_912, 30_000));
    let rt = PluginRuntime::new(config).unwrap();
    let out = rt.execute_plugin("ai-tagging", doc(&path)).unwrap().ai.expect("ai via host");
    // int8 kernels round differently natively (NEON) and in Wasm SIMD; production uses Wasm.
    let c = cosine(out.embedding.as_ref().unwrap(), native.embedding.as_ref().unwrap());
    assert!(c >= 0.99, "cosine {}", c);
    assert_eq!(out.tags.first().map(|t| &t.label), native.tags.first().map(|t| &t.label));
    for t in &out.tags {
        if let Some(n) = native.tags.iter().find(|n| n.label == t.label) {
            assert!((t.confidence - n.confidence).abs() <= 0.05, "{}: {} vs {}", t.label, t.confidence, n.confidence);
        }
    }

    let dir = common::temp_dir("clip_ac8");
    let big = image::imageops::resize(&image::open(&path).unwrap().to_rgb8(), 4000, 3000, image::imageops::FilterType::Triangle);
    let big_path = dir.join("big.jpg");
    big.save(&big_path).unwrap();
    let start = Instant::now();
    let out = rt.execute_plugin("ai-tagging", doc(big_path.to_str().unwrap())).unwrap();
    eprintln!("4000x3000 via host: {:?}", start.elapsed());
    assert!(out.ai.expect("ai within the limits").embedding.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

// ai-tagging-clip AC-9: ai-tagging is enabled in config.yaml, config_mac-studio_32GB.yaml and AppConfig::default() with 512 MB / 30 s and no experimental note; embedding_dim other than 512 warns and 512 is used
#[test]
fn test_ac9_enabled_with_limits() {
    let root = common::repo_root();
    let mut configs = vec![AppConfig::default()];
    for name in ["config.yaml", "config_mac-studio_32GB.yaml"] {
        configs.push(AppConfig::load_from_file(root.join(name)).unwrap());
        let text = std::fs::read_to_string(root.join(name)).unwrap();
        let section = text.split("ai-tagging:").nth(1).unwrap().split("duplicate:").next().unwrap();
        assert!(!section.contains("experimental"), "{}: {}", name, section);
    }
    for cfg in &configs {
        assert!(cfg.plugins["ai-tagging"].enabled);
        let (l, w) = cfg.plugin_limits("ai-tagging");
        assert!(w.is_empty(), "{:?}", w);
        assert_eq!((l.max_memory_bytes, l.timeout_ms), (536_870_912, 30_000));
    }
    let (ai, warnings) = tag(fixture("faces/apollo11.jpg").to_str().unwrap(), &[("embedding_dim", "256")]);
    assert_eq!(ai.unwrap().embedding.unwrap().len(), 512);
    assert!(warnings.iter().any(|w| w.contains("embedding_dim 256 ignored")), "{:?}", warnings);
}

// ai-tagging-clip AC-10: make dist puts the model file and its licence into the complete bundle and the platform package
#[test]
fn test_ac10_dist_contains_model() {
    let makefile = std::fs::read_to_string(common::repo_root().join("Makefile")).unwrap();
    let dist = makefile.split("\ndist:").nth(1).unwrap().split("\nclean-dist:").next().unwrap();
    assert!(dist.contains("$(CLIP_MODEL) models/CLIP-LICENSE $(STAGE)/$(COMPLETE)/models/"), "{}", dist);
    assert!(dist.contains("$(STAGE)/$(COMPLETE)/models $(STAGE)/$(PLATFORM)/"), "{}", dist);
    assert!(makefile.contains("release: check models test"));
}

// ai-tagging-clip AC-11: NOTICE and the README name CLIP, OpenAI, MIT and the usage note of the model card
#[test]
fn test_ac11_attribution() {
    let root = common::repo_root();
    for file in ["NOTICE", "plugins/ai-tagging/README.md"] {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        for needle in ["CLIP", "OpenAI", "MIT", "model card"] {
            assert!(text.contains(needle), "{} lacks {}", file, needle);
        }
    }
    assert!(ClipModel::from_model_bytes(std::fs::read(common::clip_model_path()).unwrap()).is_ok());
    let _ = TagParams::default();
}
