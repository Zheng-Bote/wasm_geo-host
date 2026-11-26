mod common;

use chrono::{TimeZone, Utc};
use flagme_host::config::PluginConfig;
use flagme_host::{AppConfig, Pipeline, PluginRuntime, Storage};
use flagme_sdk::serde_json::{json, Value};
use flagme_sdk::*;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

/// Runtime with only the test probe (`tests/probe-plugin`) in the pipeline.
fn probe_runtime(capabilities: &[&str], settings: &[(&str, &str)], limits: Option<(u64, u64)>) -> PluginRuntime {
    PluginRuntime::new(probe_config(capabilities, settings, limits)).unwrap()
}

fn probe_config(capabilities: &[&str], settings: &[(&str, &str)], limits: Option<(u64, u64)>) -> AppConfig {
    let mut config = AppConfig::default();
    config.pipeline = vec!["probe".to_string()];
    config.plugins.insert(
        "probe".to_string(),
        PluginConfig {
            enabled: true,
            path: Some(common::probe_component().to_string_lossy().to_string()),
            capabilities: capabilities.iter().map(|s| s.to_string()).collect(),
            settings: settings.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        },
    );
    if let Some((memory, timeout)) = limits {
        config.resource_limits.max_memory_bytes = memory;
        config.resource_limits.timeout_ms = timeout;
    }
    config
}

fn doc(id: &str, path: &str, custom: &[(&str, Value)]) -> UnifiedDocument {
    let mut d = UnifiedDocument::new(
        id,
        FileInfo {
            path: path.to_string(),
            filename: "photo.jpg".to_string(),
            size_bytes: 0,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    for (k, v) in custom {
        d.custom.insert(k.to_string(), v.clone());
    }
    d
}

/// `version = "…"` of a crate manifest.
fn crate_version(manifest: &Path) -> String {
    std::fs::read_to_string(manifest)
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("version = \"").map(|v| v.trim_end_matches('"').to_string()))
        .unwrap()
}

/// Repo `config.yaml` with database and plugin directory redirected.
fn write_config(dir: &Path, plugin_dir: &Path) -> PathBuf {
    let original = std::fs::read_to_string(common::repo_root().join("config.yaml")).unwrap();
    let config: String = original
        .lines()
        .map(|l| {
            if l.starts_with("database_path:") {
                format!("database_path: \"{}\"", dir.join("media.db").display())
            } else if l.starts_with("plugin_dir:") {
                format!("plugin_dir: \"{}\"", plugin_dir.display())
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let path = dir.join("config.yaml");
    std::fs::write(&path, config).unwrap();
    path
}

fn run_host(dir: &Path, args: &[&str], path_env: Option<&Path>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_flagme-host"));
    cmd.args(args).current_dir(dir).env_remove("RUST_LOG");
    if let Some(p) = path_env {
        cmd.env("PATH", p);
    }
    cmd.output().expect("flagme-host runs")
}

// wasm-component-runtime AC-1: every plugin builds to a media-plugin component that the host loads under its configured name
#[test]
fn test_ac1_all_plugins_are_components() {
    let mut config = common::with_components(AppConfig::default());
    for cfg in config.plugins.values_mut() {
        cfg.enabled = true;
    }
    let names = common::plugin_names();
    assert_eq!(names.len(), 11);
    config.pipeline = names.clone();
    let rt = PluginRuntime::new(config.clone()).expect("all components load");
    let mut loaded = rt.registered_plugin_names();
    loaded.sort();
    assert_eq!(loaded, names);
    for name in &names {
        let manifest = common::repo_root().join("plugins").join(name).join("Cargo.toml");
        assert_eq!(rt.plugin_version(name), Some(crate_version(&manifest).as_str()), "{}", name);
    }

    // A component configured under the wrong name is rejected.
    let mut wrong = config;
    wrong.pipeline = vec!["timeline".to_string()];
    wrong.plugins.get_mut("timeline").unwrap().path =
        Some(common::components_dir().join("flagme_plugin_countryflag.wasm").to_string_lossy().to_string());
    let err = PluginRuntime::new(wrong).err().expect("name mismatch").to_string();
    assert!(err.contains("is plugin 'countryflag'"), "{}", err);
}

// wasm-component-runtime AC-2: process runs plugins only from their components; a missing component stops the host with the file name
#[test]
fn test_ac2_no_native_fallback() {
    let manifest = std::fs::read_to_string(common::repo_root().join("crates/host/Cargo.toml")).unwrap();
    let deps = manifest.split("[dependencies]").nth(1).unwrap().split("\n[").next().unwrap();
    assert!(!deps.contains("flagme-plugin-"), "host links a plugin natively:\n{}", deps);

    let dir = common::temp_dir("component_ac2");
    let photo = common::write_file(&dir, "photo.jpg", &common::jpeg_with_exif("Sony", "A7", Some((52.52, 13.4))));

    let empty = dir.join("no-components");
    std::fs::create_dir_all(&empty).unwrap();
    let config = write_config(&dir, &empty);
    let out = run_host(&dir, &["process", &photo, "--config", config.to_str().unwrap()], None);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains(&empty.join("flagme_plugin_metadata.wasm").display().to_string()), "{}", stderr);
    assert!(out.stdout.is_empty());

    let copied = dir.join("components");
    std::fs::create_dir_all(&copied).unwrap();
    for name in common::plugin_names() {
        let file = format!("flagme_plugin_{}.wasm", name.replace('-', "_"));
        std::fs::copy(common::components_dir().join(&file), copied.join(&file)).unwrap();
    }
    let config = write_config(&dir, &copied);
    let out = run_host(&dir, &["process", &photo, "--config", config.to_str().unwrap()], None);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let processed: UnifiedDocument = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(processed.metadata.unwrap().camera_make.as_deref(), Some("Sony"));
    assert_eq!(processed.country.unwrap().country_code, "DE");
    let _ = std::fs::remove_dir_all(&dir);
}

// wasm-component-runtime AC-3: a guest has no ambient file system, environment or network; fs-read follows the filesystem grant
#[test]
fn test_ac3_no_ambient_authority() {
    let dir = common::temp_dir("component_ac3");
    let path = common::write_file(&dir, "secret.txt", b"twelve bytes");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    std::env::set_var("FLAGME_PROBE_SECRET", "must-not-leak");
    let input = doc("ac3", &path, &[("mode", json!("ambient")), ("path", json!(path)), ("addr", json!(addr))]);

    let report = |grants: &[&str]| -> Value {
        let out = probe_runtime(grants, &[], None).execute_plugin("probe", input.clone()).unwrap();
        out.custom["probe"].clone()
    };

    let denied = report(&[]);
    assert!(denied["std_fs_read"].as_str().unwrap().starts_with("err:"), "{}", denied);
    assert!(denied["std_fs_read_dir"].as_str().unwrap().starts_with("err:"), "{}", denied);
    assert_eq!(denied["env_vars"], json!(0), "{}", denied);
    assert!(denied["tcp_connect"].as_str().unwrap().starts_with("err:"), "{}", denied);
    assert_eq!(denied["cap_fs_read"], json!("capability-denied"));

    let granted = report(&["filesystem"]);
    assert_eq!(granted["cap_fs_read"], json!("ok:12"));
    assert!(granted["std_fs_read"].as_str().unwrap().starts_with("err:"), "grant must not open WASI preopens");
    drop(listener);
    let _ = std::fs::remove_dir_all(&dir);
}

// wasm-component-runtime AC-4: exceeding max_memory_bytes or timeout_ms fails the call with the limit named; the next document runs normally
#[test]
fn test_ac4_resource_limits() {
    let dir = common::temp_dir("component_ac4");
    let storage = Arc::new(Storage::new(dir.join("media.db").to_str().unwrap()));
    storage.init_schema().unwrap();
    let config = probe_config(&[], &[], Some((64 * 1024 * 1024, 300)));
    let pipeline = Pipeline::new(config.clone(), PluginRuntime::new(config).unwrap(), storage.clone());

    let memory = doc("ac4-memory", "/none", &[("mode", json!("memory")), ("bytes", json!("268435456"))]);
    let err = pipeline.process(memory).unwrap_err().to_string();
    assert!(err.contains("max_memory_bytes"), "{}", err);

    let start = std::time::Instant::now();
    let err = pipeline.process(doc("ac4-spin", "/none", &[("mode", json!("spin"))])).unwrap_err().to_string();
    assert!(err.contains("timeout_ms (300 ms)"), "{}", err);
    assert!(start.elapsed() < std::time::Duration::from_secs(5), "timeout not enforced: {:?}", start.elapsed());

    let small = doc("ac4-small", "/none", &[("mode", json!("memory")), ("bytes", json!("1048576"))]);
    let ok = pipeline.process(small).expect("next document runs");
    assert_eq!(ok.custom["probe"], json!(1048576));
    assert!(storage.get_document("ac4-small").is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

// wasm-component-runtime AC-5: settings from config.yaml reach the guest unchanged and SettingsReader parses bool, integer, float and list values
#[test]
fn test_ac5_settings_reach_guest() {
    let settings = [
        ("mode", "settings"),
        ("flag", "yes"),
        ("count", "42"),
        ("ratio", "0.25"),
        ("sizes", "64, 256"),
        ("text", "Grüße, \"quoted\" & spaced "),
    ];
    let out = probe_runtime(&[], &settings, None).execute_plugin("probe", doc("ac5", "/none", &[])).unwrap();
    let raw: HashMap<String, String> = serde_json::from_value(out.custom["raw"].clone()).unwrap();
    let expected: HashMap<String, String> = settings.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    assert_eq!(raw, expected);
    assert_eq!(out.custom["parsed"], json!({"flag": true, "count": 42, "ratio": 0.25, "sizes": [64, 256]}));
}

fn full_document() -> UnifiedDocument {
    let bbox = BoundingBox { x: 0.1, y: 0.2, width: 0.3, height: 0.4 };
    let mut d = doc("ac6-full", "/media/full.jpg", &[]);
    d.file.size_bytes = 123_456_789_012;
    d.file.sha256 = Some("ab".repeat(32));
    d.metadata = Some(MetadataInfo {
        camera_make: Some("Sony".into()),
        camera_model: Some("ILCE-7RM4".into()),
        lens_model: Some("FE 24-70mm".into()),
        iso: Some(6400),
        focal_length: Some(35.5),
        f_number: Some(2.8),
        exposure_time: Some("1/250 s".into()),
        raw_tags: [("Exif.Make".to_string(), "Sony".to_string()), ("XMP.dc.title".to_string(), "Ümlaut 🇩🇪".to_string())].into(),
    });
    d.location = Some(LocationInfo {
        latitude: Some(52.520008),
        longitude: Some(-13.404954),
        altitude: Some(34.5),
        country: Some("Germany".into()),
        country_code: Some("DE".into()),
        region: Some("Berlin".into()),
        county: Some("Mitte".into()),
        city: Some("Berlin".into()),
        local_country: Some("Deutschland".into()),
        local_region: Some("Berlin".into()),
        local_city: Some("Berlin".into()),
        local_language: Some("de".into()),
    });
    d.country = Some(CountryInfo {
        country_code: "DE".into(),
        country_name: "Germany".into(),
        flag_emoji: "🇩🇪".into(),
        continent: "Europe".into(),
        region: "Western Europe".into(),
    });
    d.thumbnails = vec![ThumbnailEntry { size_label: "small".into(), width: 64, height: 48, path: "/t/s.jpg".into(), cache_key: "k".into() }];
    d.ai = Some(AiInfo {
        tags: vec![AiTag { label: "city".into(), confidence: 0.75, model: "test".into() }],
        description: "a street".into(),
        embedding: Some(vec![0.5, -0.25, 1e-7]),
    });
    d.duplicate = Some(DuplicateInfo { sha256: "cd".repeat(32), phash: Some("ffeeddccbbaa9988".into()), duplicates: vec!["other".into()] });
    d.timeline = Some(TimelineInfo {
        taken_at: Some(Utc.with_ymd_and_hms(2026, 11, 25, 10, 0, 0).unwrap() + chrono::Duration::nanoseconds(123_456_789)),
        imported_at: Utc.with_ymd_and_hms(2026, 11, 26, 8, 30, 15).unwrap(),
        modified_at: Some(Utc.with_ymd_and_hms(1999, 12, 31, 23, 59, 59).unwrap()),
        resolution_source: "exif_datetime_original".into(),
    });
    d.weather = Some(WeatherInfo {
        temperature: Some(-3.5),
        condition: "Snow".into(),
        wmo_code: 71,
        icon_emoji: "🌨".into(),
        humidity_percent: Some(88),
        precipitation_mm: Some(1.25),
        is_day: Some(false),
    });
    d.ocr = Some(OcrInfo {
        combined_text: "Unter den Linden".into(),
        regions: vec![TextRegion { text: "Unter den Linden".into(), confidence: 0.9, bbox: bbox.clone() }],
        primary_language: Some("de".into()),
    });
    d.faces = Some(FacesInfo {
        faces: vec![DetectedFace {
            id: "face-1".into(),
            bbox,
            confidence: 0.99,
            landmarks: Some(FaceLandmarks {
                left_eye: (0.1, 0.2),
                right_eye: (0.3, 0.2),
                nose: (0.2, 0.3),
                mouth_left: (0.15, 0.4),
                mouth_right: (0.25, 0.4),
            }),
            embedding: Some(vec![0.125; 8]),
            person_id: Some("anna".into()),
        }],
        detection_model: "test".into(),
    });
    d.privacy = Some(PrivacyInfo {
        is_redacted: true,
        faces_redacted_count: 1,
        license_plates_redacted_count: 2,
        original_gps_fuzzed: true,
        redacted_image_path: Some("/r/full.jpg".into()),
        applied_rules: vec!["fuzzed_gps_precision_2_decimals".into()],
    });
    d.custom.insert("mode".into(), json!("echo"));
    d.custom.insert("nested".into(), json!({"a": [1, 2.5, null, true], "b": {"c": "ü"}}));
    d
}

// wasm-component-runtime AC-6: a fully populated UnifiedDocument crosses the WIT boundary both ways without loss; PluginError kinds survive
#[test]
fn test_ac6_document_round_trip() {
    let rt = probe_runtime(&[], &[], None);
    let input = full_document();
    let output = rt.execute_plugin("probe", input.clone()).unwrap();
    assert_eq!(output, input);

    let err = rt.execute_plugin("probe", doc("ac6-fail", "/none", &[("mode", json!("fail"))])).unwrap_err();
    assert!(matches!(&err, PluginError::Config(m) if m == "probe failure"), "{:?}", err);
}

// wasm-component-runtime AC-7: stream runs on the embedded runtime, gives the same result as process and needs no wasmtime CLI
#[test]
fn test_ac7_stream_uses_embedded_runtime() {
    let dir = common::temp_dir("component_ac7");
    let photo = common::write_file(&dir, "photo.jpg", &common::jpeg_with_exif("Canon", "R5", Some((48.137, 11.575))));
    let config = write_config(&dir, &common::components_dir());

    // PATH with sqlite3 only (needed by the storage), so a `wasmtime` CLI cannot be found.
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let sqlite = String::from_utf8(Command::new("which").arg("sqlite3").output().unwrap().stdout).unwrap();
    std::os::unix::fs::symlink(sqlite.trim(), bin.join("sqlite3")).unwrap();

    let normalize = |out: Output| -> Value {
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let mut v: Value = serde_json::from_slice(&out.stdout).unwrap();
        v["timeline"]["imported_at"] = Value::Null;
        v
    };
    let args = |cmd: &'static str| [cmd, photo.as_str(), "--config", config.to_str().unwrap()].map(str::to_string);
    let stream = normalize(run_host(&dir, &args("stream").each_ref().map(String::as_str), Some(&bin)));
    let process = normalize(run_host(&dir, &args("process").each_ref().map(String::as_str), Some(&bin)));
    assert_eq!(stream, process);
    assert_eq!(stream["metadata"]["camera_make"], json!("Canon"));
    assert!(stream["duplicate"]["sha256"].is_string(), "later stages read the file through capabilities");
    let _ = std::fs::remove_dir_all(&dir);
}

// wasm-component-runtime AC-8: a plugin's CLI binary built without the component feature does not import host-capabilities
#[test]
fn test_ac8_cli_binary_stays_plain_command() {
    let root = common::repo_root();
    let target = root.join("target/test-cli-binaries");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(cargo)
        .current_dir(&root)
        .args(["build", "--release", "--target", "wasm32-wasip2", "-p", "flagme-plugin-countryflag", "--bin", "flagme-plugin-countryflag"])
        .arg("--target-dir")
        .arg(&target)
        .status()
        .unwrap();
    assert!(status.success());
    let contains = |bytes: &[u8], needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);

    let cli = std::fs::read(target.join("wasm32-wasip2/release/flagme-plugin-countryflag.wasm")).unwrap();
    assert!(contains(&cli, b"wasi:cli/run"));
    assert!(!contains(&cli, b"wasmgeohost:media/host-capabilities"));

    let component = std::fs::read(common::components_dir().join("flagme_plugin_countryflag.wasm")).unwrap();
    assert!(contains(&component, b"wasmgeohost:media/host-capabilities"));
}

/// Collects the formatted log output of `tracing` on the current thread.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// wasm-component-runtime AC-9: a settings warning appears once per plugin and message although every document gets a fresh instance
#[test]
fn test_ac9_settings_warning_once() {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .finish();
    let rt = probe_runtime(&[], &[("mode", "settings"), ("count", "many")], None);
    tracing::subscriber::with_default(subscriber, || {
        for i in 0..3 {
            rt.execute_plugin("probe", doc(&format!("ac9-{}", i), "/none", &[])).unwrap();
        }
    });
    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    let warnings: Vec<&str> = log.lines().filter(|l| l.contains("settings[probe]") && l.contains("count")).collect();
    assert_eq!(warnings.len(), 1, "{}", log);
}
