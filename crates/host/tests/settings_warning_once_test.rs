//! wasm-component-runtime AC-9 in a test binary of its own (#79).
//!
//! The test captures `tracing` output with a thread-local subscriber
//! (`tracing::subscriber::with_default`). In a binary with other tests running in parallel,
//! their events registered the callsites on other threads first, and the scoped subscriber
//! sometimes missed the warning. Alone in its process, nothing else emits events concurrently.

mod common;

use flagme_host::config::PluginConfig;
use flagme_host::{AppConfig, PluginRuntime};
use flagme_sdk::{FileInfo, UnifiedDocument};
use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

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

fn probe_runtime(settings: &[(&str, &str)]) -> PluginRuntime {
    let mut config = AppConfig::default();
    config.pipeline = vec!["probe".to_string()];
    config.plugins.insert(
        "probe".to_string(),
        PluginConfig {
            enabled: true,
            path: Some(common::probe_component().to_string_lossy().to_string()),
            capabilities: vec![],
            settings: settings.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            resource_limits: HashMap::new(),
        },
    );
    PluginRuntime::new(config).unwrap()
}

fn doc(id: &str) -> UnifiedDocument {
    UnifiedDocument::new(
        id,
        FileInfo {
            path: "/none".to_string(),
            filename: "photo.jpg".to_string(),
            size_bytes: 0,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    )
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
    let rt = probe_runtime(&[("mode", "settings"), ("count", "many")]);
    tracing::subscriber::with_default(subscriber, || {
        for i in 0..3 {
            rt.execute_plugin("probe", doc(&format!("ac9-{}", i))).unwrap();
        }
    });
    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    let warnings: Vec<&str> = log.lines().filter(|l| l.contains("settings[probe]") && l.contains("count")).collect();
    assert_eq!(warnings.len(), 1, "{}", log);
}
