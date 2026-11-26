//! Test-only plugin component for the sandbox tests of `flagme-host` (ADR 0003).
//!
//! It deliberately tries what a real plugin must not do (ambient file system, environment,
//! network, unbounded memory and run time), so the host tests can prove the sandbox holds.
//! The behaviour is selected with `mode` (and its parameters) from `doc.custom`, falling back to
//! the plugin settings, so one runtime can probe several cases. This crate is never part of a
//! pipeline.

use flagme_sdk::serde_json::{json, Value};
use flagme_sdk::{Plugin, PluginContext, PluginError, PluginResult, SettingsReader, UnifiedDocument};

pub struct ProbePlugin;

impl ProbePlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProbePlugin {
    fn default() -> Self {
        Self::new()
    }
}

fn outcome<T, E: std::fmt::Display>(r: Result<T, E>, ok: impl FnOnce(T) -> String) -> String {
    match r {
        Ok(v) => format!("ok:{}", ok(v)),
        Err(e) => format!("err:{}", e),
    }
}

impl Plugin for ProbePlugin {
    fn name(&self) -> &'static str {
        "probe"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        let custom = doc.custom.clone();
        let setting = |k: &str| match custom.get(k) {
            Some(Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => ctx.settings.get(k).cloned().unwrap_or_default(),
        };
        match setting("mode").as_str() {
            "echo" | "" => {}
            "ambient" => {
                let path = setting("path");
                let report = json!({
                    "std_fs_read": outcome(std::fs::read(&path), |b| b.len().to_string()),
                    "std_fs_read_dir": outcome(std::fs::read_dir("/"), |d| d.count().to_string()),
                    "env_vars": std::env::vars().count(),
                    "tcp_connect": outcome(std::net::TcpStream::connect(setting("addr")), |_| String::new()),
                    "cap_fs_read": match ctx.capabilities.fs_read(&path) {
                        Ok(b) => format!("ok:{}", b.len()),
                        Err(PluginError::CapabilityDenied(_)) => "capability-denied".to_string(),
                        Err(e) => format!("err:{}", e),
                    },
                });
                doc.custom.insert("probe".to_string(), report);
            }
            "memory" => {
                let bytes: usize = setting("bytes").parse().unwrap_or(0);
                let block = vec![1u8; bytes];
                doc.custom.insert("probe".to_string(), json!(std::hint::black_box(&block).len()));
            }
            "spin" => {
                let mut n: u64 = 0;
                loop {
                    n = std::hint::black_box(n.wrapping_add(1));
                }
            }
            "settings" => {
                let raw: Value = ctx.settings.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<flagme_sdk::serde_json::Map<_, _>>().into();
                let mut r = SettingsReader::new("probe", ctx);
                r.string("mode", &[], "");
                let parsed = json!({
                    "flag": r.bool("flag", &[], false),
                    "count": r.u32("count", &[], 0..=1000, 0),
                    "ratio": r.f32("ratio", &[], 0.0..=1.0, 0.0),
                    "sizes": r.list_u32("sizes", 1..=10_000, &[]),
                });
                r.finish();
                doc.custom.insert("raw".to_string(), raw);
                doc.custom.insert("parsed".to_string(), parsed);
            }
            "fail" => return Err(PluginError::Config("probe failure".to_string())),
            other => return Err(PluginError::Config(format!("unknown probe mode '{}'", other))),
        }
        Ok(doc)
    }
}

flagme_sdk::export_plugin!(ProbePlugin, ProbePlugin::new);
