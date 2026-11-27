mod common;

use flagme_host::component::{CacheStats, ComponentEngine};
use flagme_host::config::ResourceLimits;
use flagme_host::{AppConfig, PluginRuntime};
use flagme_sdk::{FileInfo, UnifiedDocument};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Config with only the given plugins (built components) in the pipeline and the component
/// cache in `cache_dir` (`None`: cache off).
fn config(plugins: &[&str], cache_dir: Option<&Path>) -> AppConfig {
    let mut config = common::with_components(AppConfig::default());
    config.pipeline = plugins.iter().map(|s| s.to_string()).collect();
    for name in plugins {
        config.plugins.get_mut(*name).unwrap().enabled = true;
    }
    match cache_dir {
        Some(dir) => config.component_cache_dir = dir.to_string_lossy().to_string(),
        None => config.component_cache = false,
    }
    config
}

/// All regular files below `dir` (empty if `dir` does not exist).
fn files_below(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(files_below(&path));
        } else {
            out.push(path);
        }
    }
    out
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

// component-cache AC-1: the first start compiles and writes cache entries, a second start loads every component from the cache
#[test]
fn test_ac1_second_start_hits_cache() {
    let dir = common::temp_dir("cache_ac1");
    let cache = dir.join("cc");
    let plugins = ["metadata", "countryflag", "timeline"];

    let first = PluginRuntime::new(config(&plugins, Some(&cache))).expect("first start");
    assert_eq!(first.component_cache_stats(), Some(CacheStats { hits: 0, misses: 3 }));
    assert!(!files_below(&cache.join("modules")).is_empty(), "no cache entries written");
    drop(first);

    let second = PluginRuntime::new(config(&plugins, Some(&cache))).expect("second start");
    assert_eq!(second.component_cache_stats(), Some(CacheStats { hits: 3, misses: 0 }));
    let mut loaded = second.registered_plugin_names();
    loaded.sort();
    assert_eq!(loaded, vec!["countryflag", "metadata", "timeline"]);
    let _ = std::fs::remove_dir_all(&dir);
}

// component-cache AC-2: a changed component file is compiled again and reports its new identity, not the cached one
#[test]
fn test_ac2_changed_component_recompiled() {
    let dir = common::temp_dir("cache_ac2");
    let cache = dir.join("cc");
    let file = dir.join("plugin.wasm");
    let limits = ResourceLimits::default();

    std::fs::copy(common::probe_component(), &file).unwrap();
    let engine = ComponentEngine::new(Some(&cache)).unwrap();
    let pre = engine.load(&file).unwrap();
    assert_eq!(engine.component_identity(&pre, &limits).unwrap().0, "probe");
    assert_eq!(engine.cache_stats(), Some(CacheStats { hits: 0, misses: 1 }));
    drop(engine);

    // Same path, different component.
    std::fs::copy(common::components_dir().join("flagme_plugin_countryflag.wasm"), &file).unwrap();
    let engine = ComponentEngine::new(Some(&cache)).unwrap();
    let pre = engine.load(&file).unwrap();
    assert_eq!(engine.component_identity(&pre, &limits).unwrap().0, "countryflag");
    assert_eq!(engine.cache_stats(), Some(CacheStats { hits: 0, misses: 1 }));

    // The original component is still served from its own entry.
    std::fs::copy(common::probe_component(), &file).unwrap();
    let pre = engine.load(&file).unwrap();
    assert_eq!(engine.component_identity(&pre, &limits).unwrap().0, "probe");
    assert_eq!(engine.cache_stats(), Some(CacheStats { hits: 1, misses: 1 }));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Runs `plugins` in order on a JPEG with EXIF and GPS. `timeline.imported_at` is the wall
/// clock of the run, so it is checked for presence and then left out of the comparison.
fn run_pipeline(rt: &PluginRuntime, plugins: &[&str], file: &str) -> serde_json::Value {
    let mut doc = UnifiedDocument::new(
        "photo-1",
        FileInfo {
            path: file.to_string(),
            filename: "photo.jpg".to_string(),
            size_bytes: std::fs::metadata(file).unwrap().len(),
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    for name in plugins {
        doc = rt.execute_plugin(name, doc).unwrap_or_else(|e| panic!("{}: {}", name, e));
    }
    let mut value = serde_json::to_value(&doc).unwrap();
    let imported_at = value["timeline"].as_object_mut().and_then(|t| t.remove("imported_at"));
    assert!(imported_at.is_some_and(|v| v.is_string()), "timeline.imported_at missing");
    value
}

// component-cache AC-3: a document passes the pipeline with the same result with and without the cache
#[test]
fn test_ac3_same_result_with_and_without_cache() {
    let dir = common::temp_dir("cache_ac3");
    let cache = dir.join("cc");
    let file = common::write_file(&dir, "photo.jpg", &common::jpeg_with_exif("Canon", "EOS R5", Some((48.137, 11.575))));
    let plugins = ["metadata", "countryflag", "timeline"];

    let uncached = PluginRuntime::new(config(&plugins, None)).unwrap();
    assert_eq!(uncached.component_cache_stats(), None);
    let expected = run_pipeline(&uncached, &plugins, &file);
    assert!(expected["metadata"].is_object(), "{}", expected);

    let filling = PluginRuntime::new(config(&plugins, Some(&cache))).unwrap();
    assert_eq!(run_pipeline(&filling, &plugins, &file), expected);
    drop(filling);

    let cached = PluginRuntime::new(config(&plugins, Some(&cache))).unwrap();
    assert_eq!(cached.component_cache_stats(), Some(CacheStats { hits: 3, misses: 0 }));
    assert_eq!(run_pipeline(&cached, &plugins, &file), expected);
    let _ = std::fs::remove_dir_all(&dir);
}

// component-cache AC-4: with component_cache false no cache directory is created and no cache file is written
#[test]
fn test_ac4_disabled_writes_nothing() {
    let dir = common::temp_dir("cache_ac4");
    let cache = dir.join("cc");
    let mut cfg = config(&["countryflag"], Some(&cache));
    cfg.component_cache = false;
    assert_eq!(cfg.component_cache_dir(), None);

    let rt = PluginRuntime::new(cfg).unwrap();
    assert_eq!(rt.component_cache_stats(), None);
    assert!(!cache.exists(), "cache directory was created");
    let _ = std::fs::remove_dir_all(&dir);
}

// component-cache AC-5: a missing cache directory (and its parents) is created, the directory itself with mode 0700
#[test]
fn test_ac5_missing_dir_created_0700() {
    let dir = common::temp_dir("cache_ac5");
    let cache = dir.join("nested").join("cc");
    let rt = PluginRuntime::new(config(&["countryflag"], Some(&cache))).unwrap();
    assert!(rt.component_cache_stats().is_some(), "cache not enabled");
    assert!(cache.is_dir());
    assert_eq!(mode(&cache), 0o700);
    let _ = std::fs::remove_dir_all(&dir);
}

// component-cache AC-6: a cache directory writable by group or others, a symbolic link, a file or an uncreatable path disables the cache without failing the start and nothing is written there
#[test]
fn test_ac6_unsafe_dir_falls_back() {
    let dir = common::temp_dir("cache_ac6");

    let group_writable = dir.join("group");
    std::fs::create_dir(&group_writable).unwrap();
    std::fs::set_permissions(&group_writable, std::fs::Permissions::from_mode(0o770)).unwrap();

    let world_writable = dir.join("world");
    std::fs::create_dir(&world_writable).unwrap();
    std::fs::set_permissions(&world_writable, std::fs::Permissions::from_mode(0o777)).unwrap();

    let target = dir.join("target");
    std::fs::create_dir(&target).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700)).unwrap();
    let link = dir.join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let file = dir.join("file");
    std::fs::write(&file, b"x").unwrap();
    let below_file = file.join("cc");

    for (cache, watched) in [
        (&group_writable, &group_writable),
        (&world_writable, &world_writable),
        (&link, &target),
        (&file, &dir.join("none")),
        (&below_file, &dir.join("none")),
    ] {
        let rt = PluginRuntime::new(config(&["countryflag"], Some(cache)))
            .unwrap_or_else(|e| panic!("start failed for {}: {}", cache.display(), e));
        assert_eq!(rt.component_cache_stats(), None, "{}", cache.display());
        assert_eq!(rt.registered_plugin_names(), vec!["countryflag"]);
        assert!(files_below(watched).is_empty(), "{} was written", watched.display());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// component-cache AC-6: the fallback is logged as a warning on stderr with path and reason, and the host still processes the file
#[test]
fn test_ac6_warning_on_stderr() {
    let dir = common::temp_dir("cache_ac6_log");
    let cache = dir.join("shared");
    std::fs::create_dir(&cache).unwrap();
    std::fs::set_permissions(&cache, std::fs::Permissions::from_mode(0o777)).unwrap();
    let file = common::write_file(&dir, "photo.jpg", &common::plain_jpeg(16, 16));

    let config = format!(
        "database_path: \"{}\"\nplugin_dir: \"{}\"\ncomponent_cache_dir: \"{}\"\npipeline:\n  - countryflag\nplugins:\n  countryflag:\n    enabled: true\n",
        dir.join("media.db").display(),
        common::components_dir().display(),
        cache.display()
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
        .find(|l| l.contains("component cache disabled"))
        .unwrap_or_else(|| panic!("warning missing on stderr:\n{}", stderr));
    assert!(line.contains("WARN"), "{}", line);
    assert!(line.contains(&cache.display().to_string()), "{}", line);
    assert!(line.contains("writable by group or others (mode 777)"), "{}", line);
    assert!(files_below(&cache).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

// component-cache AC-7: component_cache and component_cache_dir are read from YAML and JSON; missing keys default to true and cache/components
#[test]
fn test_ac7_config_keys() {
    let defaults = AppConfig::load_from_str("database_path: \"x.db\"\n").unwrap();
    assert!(defaults.component_cache);
    assert_eq!(defaults.component_cache_dir, "cache/components");
    assert_eq!(defaults.component_cache_dir(), Some(PathBuf::from("cache/components")));
    let json_defaults = AppConfig::load_from_str("{}").unwrap();
    assert!(json_defaults.component_cache);
    assert_eq!(json_defaults.component_cache_dir, "cache/components");

    let yaml = AppConfig::load_from_str(
        "component_cache: false # off\ncomponent_cache_dir: '/var/tmp/flagme cc'\nplugin_dir: \"p\"\n",
    )
    .unwrap();
    assert!(!yaml.component_cache);
    assert_eq!(yaml.component_cache_dir, "/var/tmp/flagme cc");
    assert_eq!(yaml.component_cache_dir(), None);
    assert_eq!(yaml.plugin_dir, "p");

    let on = AppConfig::load_from_str("component_cache: true\ncomponent_cache_dir: other\n").unwrap();
    assert_eq!(on.component_cache_dir(), Some(PathBuf::from("other")));

    let invalid = AppConfig::load_from_str("component_cache: maybe\n").unwrap();
    assert!(invalid.component_cache, "an invalid value keeps the default");

    let json = AppConfig::load_from_str(r#"{"component_cache": false, "component_cache_dir": "/c"}"#).unwrap();
    assert!(!json.component_cache);
    assert_eq!(json.component_cache_dir, "/c");
}

/// Rust sources below `dir`.
fn rust_sources(dir: &Path) -> Vec<PathBuf> {
    files_below(dir).into_iter().filter(|p| p.extension().is_some_and(|e| e == "rs")).collect()
}

// component-cache AC-8: the host code contains no unsafe and never calls Component::deserialize or deserialize_file
#[test]
fn test_ac8_no_unsafe_no_deserialize() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let sources = rust_sources(&src);
    assert!(sources.iter().any(|p| p.ends_with("component.rs")));
    for path in sources {
        let text = std::fs::read_to_string(&path).unwrap();
        for (no, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            assert!(
                !code.split(|c: char| !c.is_alphanumeric() && c != '_').any(|w| w == "unsafe"),
                "{}:{}: unsafe",
                path.display(),
                no + 1
            );
            assert!(
                !code.contains("deserialize_file") && !code.contains("Component::deserialize"),
                "{}:{}: loads precompiled code",
                path.display(),
                no + 1
            );
        }
    }
}
