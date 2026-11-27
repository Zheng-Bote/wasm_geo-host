mod common;

use flagme_host::config::{PluginConfig, ResourceLimits};
use flagme_host::{AppConfig, PluginRuntime};
use flagme_sdk::serde_json::{json, Value};
use flagme_sdk::{FileInfo, UnifiedDocument};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

const MIB: u64 = 1024 * 1024;

/// Config with only the test probe in the pipeline, the given global limits and the given
/// per-plugin `resource_limits` (raw values as in the config file). No component cache.
fn probe_config(global: (u64, u64), plugin: &[(&str, Value)]) -> AppConfig {
    let mut config = AppConfig {
        component_cache: false,
        resource_limits: ResourceLimits { max_memory_bytes: global.0, timeout_ms: global.1 },
        pipeline: vec!["probe".to_string()],
        ..AppConfig::default()
    };
    config.plugins.insert(
        "probe".to_string(),
        PluginConfig {
            enabled: true,
            path: Some(common::probe_component().to_string_lossy().to_string()),
            capabilities: Vec::new(),
            settings: HashMap::new(),
            resource_limits: plugin.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
        },
    );
    config
}

fn probe_doc(custom: &[(&str, Value)]) -> UnifiedDocument {
    let mut d = UnifiedDocument::new(
        "limits",
        FileInfo {
            path: "/none".to_string(),
            filename: "none.jpg".to_string(),
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

fn allocate(bytes: u64) -> UnifiedDocument {
    probe_doc(&[("mode", json!("memory")), ("bytes", json!(bytes.to_string()))])
}

// plugin-resource-limits AC-1: a per-plugin max_memory_bytes below the global value fails the plugin at its own limit, naming it; the same allocation succeeds under the global limits
#[test]
fn test_ac1_lower_memory_limit_per_plugin() {
    let global = (256 * MIB, 10_000);

    let unlimited = PluginRuntime::new(probe_config(global, &[])).unwrap();
    let ok = unlimited.execute_plugin("probe", allocate(64 * MIB)).expect("64 MiB fit the global limit");
    assert_eq!(ok.custom["probe"], json!(64 * MIB));

    let limited = PluginRuntime::new(probe_config(global, &[("max_memory_bytes", json!(32 * MIB))])).unwrap();
    assert_eq!(limited.plugin_limits("probe"), Some(&ResourceLimits { max_memory_bytes: 32 * MIB, timeout_ms: 10_000 }));
    let err = limited.execute_plugin("probe", allocate(64 * MIB)).unwrap_err().to_string();
    assert!(err.contains(&format!("max_memory_bytes ({} bytes)", 32 * MIB)), "{}", err);

    // The plugin keeps working within its limit.
    let small = limited.execute_plugin("probe", allocate(MIB)).expect("1 MiB fits");
    assert_eq!(small.custom["probe"], json!(MIB));
}

// plugin-resource-limits AC-2: a per-plugin timeout_ms above the global value lets the plugin run past the global limit; it fails at its own limit, naming it
#[test]
fn test_ac2_higher_timeout_per_plugin() {
    let global = (256 * MIB, 200);

    let global_only = PluginRuntime::new(probe_config(global, &[])).unwrap();
    let err = global_only.execute_plugin("probe", probe_doc(&[("mode", json!("spin"))])).unwrap_err().to_string();
    assert!(err.contains("timeout_ms (200 ms)"), "{}", err);

    let rt = PluginRuntime::new(probe_config(global, &[("timeout_ms", json!(900))])).unwrap();
    let start = Instant::now();
    let err = rt.execute_plugin("probe", probe_doc(&[("mode", json!("spin"))])).unwrap_err().to_string();
    let elapsed = start.elapsed();
    assert!(err.contains("timeout_ms (900 ms)"), "{}", err);
    assert!(elapsed >= Duration::from_millis(850), "stopped at the global limit: {:?}", elapsed);
    assert!(elapsed < Duration::from_secs(5), "timeout not enforced: {:?}", elapsed);
}

// plugin-resource-limits AC-3: a plugin that sets only one field inherits the other from the global resource_limits
#[test]
fn test_ac3_missing_field_inherits_global() {
    let global = (128 * MIB, 4_000);
    let cfg = probe_config(global, &[("timeout_ms", json!(500))]);
    let (limits, warnings) = cfg.plugin_limits("probe");
    assert_eq!(limits, ResourceLimits { max_memory_bytes: 128 * MIB, timeout_ms: 500 });
    assert!(warnings.is_empty(), "{:?}", warnings);

    let cfg = probe_config(global, &[("max_memory_bytes", json!("67108864"))]);
    let rt = PluginRuntime::new(cfg).unwrap();
    assert_eq!(rt.plugin_limits("probe"), Some(&ResourceLimits { max_memory_bytes: 64 * MIB, timeout_ms: 4_000 }));
}

// plugin-resource-limits AC-4: 0, a non-integer value or an unknown key keeps the host starting, warns with plugin, key and value and uses the global value for that field
#[test]
fn test_ac4_invalid_values_warn_and_fall_back() {
    let global = (128 * MIB, 4_000);
    for (plugin, warning) in [
        (vec![("max_memory_bytes", json!(0))], "resource_limits.max_memory_bytes = 0 is not a positive integer; using the global value 134217728"),
        (vec![("timeout_ms", json!("abc"))], "resource_limits.timeout_ms = \"abc\" is not a positive integer; using the global value 4000"),
        (vec![("timeout_ms", json!(-5))], "resource_limits.timeout_ms = -5 is not a positive integer"),
        (vec![("timeout_ms", json!(1.5))], "resource_limits.timeout_ms = 1.5 is not a positive integer"),
        (vec![("max_fuel", json!(10))], "unknown resource_limits key 'max_fuel'; ignored"),
    ] {
        let cfg = probe_config(global, &plugin);
        let (limits, warnings) = cfg.plugin_limits("probe");
        assert_eq!(limits, ResourceLimits { max_memory_bytes: 128 * MIB, timeout_ms: 4_000 }, "{:?}", plugin);
        assert_eq!(warnings.len(), 1, "{:?}", warnings);
        assert!(warnings[0].starts_with("plugin 'probe': "), "{}", warnings[0]);
        assert!(warnings[0].contains(warning), "{}", warnings[0]);

        let rt = PluginRuntime::new(cfg).expect("host starts");
        assert_eq!(rt.plugin_limits("probe"), Some(&limits));
    }

    // A valid field next to an invalid one still applies.
    let cfg = probe_config(global, &[("max_memory_bytes", json!(0)), ("timeout_ms", json!(700))]);
    let (limits, warnings) = cfg.plugin_limits("probe");
    assert_eq!(limits, ResourceLimits { max_memory_bytes: 128 * MIB, timeout_ms: 700 });
    assert_eq!(warnings.len(), 1, "{:?}", warnings);
}

// plugin-resource-limits AC-4: the warning for an invalid per-plugin limit appears on stderr at start and the host still processes the file
#[test]
fn test_ac4_warning_on_stderr() {
    let dir = common::temp_dir("plugin_limits_ac4");
    let file = common::write_file(&dir, "photo.jpg", &common::plain_jpeg(16, 16));
    let config = format!(
        "database_path: \"{}\"\nplugin_dir: \"{}\"\ncomponent_cache: false\npipeline:\n  - countryflag\nplugins:\n  countryflag:\n    enabled: true\n    resource_limits:\n      timeout_ms: soon\n",
        dir.join("media.db").display(),
        common::components_dir().display(),
    );
    let config_path = dir.join("config.yaml");
    std::fs::write(&config_path, config).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_flagme-host"))
        .args(["process", &file, "--config", config_path.to_str().unwrap()])
        .current_dir(&dir)
        .env_remove("RUST_LOG")
        .output()
        .expect("flagme-host runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{}", stderr);
    let line = stderr
        .lines()
        .find(|l| l.contains("resource_limits.timeout_ms"))
        .unwrap_or_else(|| panic!("warning missing on stderr:\n{}", stderr));
    assert!(line.contains("WARN") && line.contains("plugin=countryflag"), "{}", line);
    assert!(line.contains("\"soon\" is not a positive integer; using the global value 10000"), "{}", line);
    let _ = std::fs::remove_dir_all(&dir);
}

// plugin-resource-limits AC-5: plugins.<name>.resource_limits is read from YAML (block under the plugin) and JSON; without it the effective limits equal the global ones
#[test]
fn test_ac5_config_yaml_and_json() {
    let yaml = "\
resource_limits:
  max_memory_bytes: 268435456
  timeout_ms: 10000
plugins:
  thumbnail:
    enabled: true
    capabilities:
      - filesystem
    resource_limits:
      max_memory_bytes: 1073741824 # 1 GB
      timeout_ms: '30000'
    settings:
      sizes: '128, 512'
  countryflag:
    enabled: true
";
    let cfg = AppConfig::load_from_str(yaml).unwrap();
    let thumb = &cfg.plugins["thumbnail"];
    assert_eq!(thumb.capabilities, vec!["filesystem"]);
    assert_eq!(thumb.settings.get("sizes").map(String::as_str), Some("128, 512"));
    assert_eq!(thumb.resource_limits.len(), 2);
    assert_eq!(cfg.plugin_limits("thumbnail"), (ResourceLimits { max_memory_bytes: 1024 * MIB, timeout_ms: 30_000 }, vec![]));
    assert_eq!(cfg.plugin_limits("countryflag"), (ResourceLimits { max_memory_bytes: 256 * MIB, timeout_ms: 10_000 }, vec![]));
    assert_eq!(cfg.plugin_limits("not-configured").0, cfg.resource_limits);

    let json = r#"{
        "resource_limits": {"max_memory_bytes": 268435456, "timeout_ms": 10000},
        "plugins": {
            "thumbnail": {"resource_limits": {"max_memory_bytes": 1073741824, "timeout_ms": "30000"}},
            "countryflag": {}
        }
    }"#;
    let cfg = AppConfig::load_from_str(json).unwrap();
    assert_eq!(cfg.plugin_limits("thumbnail"), (ResourceLimits { max_memory_bytes: 1024 * MIB, timeout_ms: 30_000 }, vec![]));
    assert_eq!(cfg.plugin_limits("countryflag").0, cfg.resource_limits);

    // A wrong type stays a per-plugin warning and does not break the JSON config.
    let cfg = AppConfig::load_from_str(r#"{"plugins": {"thumbnail": {"resource_limits": {"timeout_ms": [1]}}}}"#).unwrap();
    let (limits, warnings) = cfg.plugin_limits("thumbnail");
    assert_eq!(limits, cfg.resource_limits);
    assert_eq!(warnings.len(), 1, "{:?}", warnings);

    // The example configs load without warnings; only ocr-text (ADR 0009) and ai-tagging
    // (ADR 0010) set their own limits, every other plugin keeps the global limits.
    let root = common::repo_root();
    for name in ["config.yaml", "config.mac-studio.yaml"] {
        let cfg = AppConfig::load_from_file(Path::new(&root).join(name)).unwrap();
        for plugin in cfg.plugins.keys() {
            let (limits, warnings) = cfg.plugin_limits(plugin);
            assert!(warnings.is_empty(), "{} {}: {:?}", name, plugin, warnings);
            if plugin == "ocr-text" {
                let expected = ResourceLimits { timeout_ms: 30_000, ..cfg.resource_limits.clone() };
                assert_eq!(limits, expected, "{} {}", name, plugin);
            } else if plugin == "ai-tagging" {
                assert_eq!(limits, ResourceLimits { max_memory_bytes: 536_870_912, timeout_ms: 30_000 }, "{} {}", name, plugin);
            } else {
                assert_eq!(limits, cfg.resource_limits, "{} {}", name, plugin);
            }
        }
    }
}

// plugin-resource-limits AC-6: the per-plugin limits already apply when the host queries name and version at load time
#[test]
fn test_ac6_limits_apply_when_loading() {
    // Too little memory for the component's initial linear memory: loading fails for this plugin.
    let err = PluginRuntime::new(probe_config((256 * MIB, 10_000), &[("max_memory_bytes", json!(65_536))]))
        .err()
        .expect("load must fail under the plugin limit")
        .to_string();
    assert!(err.contains("plugin 'probe'") && err.contains("max_memory_bytes"), "{}", err);

    // The same tiny global limit is lifted for the plugin by its own value.
    let rt = PluginRuntime::new(probe_config((65_536, 10_000), &[("max_memory_bytes", json!(256 * MIB))]))
        .expect("plugin limit replaces the global one when loading");
    assert_eq!(rt.plugin_version("probe").map(str::is_empty), Some(false));
    let ok = rt.execute_plugin("probe", allocate(MIB)).expect("runs with its own limit");
    assert_eq!(ok.custom["probe"], json!(MIB));
}
