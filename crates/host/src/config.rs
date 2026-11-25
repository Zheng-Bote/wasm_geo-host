use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_database_path")]
    pub database_path: String,
    #[serde(default)]
    pub resource_limits: ResourceLimits,
    #[serde(default)]
    pub pipeline: Vec<String>,
    #[serde(default)]
    pub plugins: HashMap<String, PluginConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    #[serde(default = "default_max_memory_bytes")]
    pub max_memory_bytes: u64,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: default_max_memory_bytes(),
            timeout_ms: default_timeout_ms(),
        }
    }
}

fn default_database_path() -> String {
    "media.db".to_string()
}

fn default_max_memory_bytes() -> u64 {
    256 * 1024 * 1024 // 256 MB
}

fn default_timeout_ms() -> u64 {
    10_000 // 10s
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub settings: HashMap<String, String>,
}

fn default_true() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut plugins = HashMap::new();
        plugins.insert(
            "metadata".to_string(),
            PluginConfig {
                enabled: true,
                path: Some("plugins/metadata.wasm".to_string()),
                capabilities: vec!["filesystem".to_string()],
                settings: HashMap::new(),
            },
        );
        plugins.insert(
            "reversegeo".to_string(),
            PluginConfig {
                enabled: true,
                path: Some("plugins/reversegeo.wasm".to_string()),
                capabilities: vec!["http".to_string(), "kv".to_string()],
                settings: HashMap::new(),
            },
        );
        plugins.insert(
            "countryflag".to_string(),
            PluginConfig {
                enabled: true,
                path: Some("plugins/countryflag.wasm".to_string()),
                capabilities: vec![],
                settings: HashMap::new(),
            },
        );
        plugins.insert(
            "thumbnail".to_string(),
            PluginConfig {
                enabled: true,
                path: Some("plugins/thumbnail.wasm".to_string()),
                capabilities: vec!["filesystem".to_string()],
                settings: HashMap::new(),
            },
        );
        plugins.insert(
            "ai-tagging".to_string(),
            PluginConfig {
                enabled: true,
                path: Some("plugins/ai-tagging.wasm".to_string()),
                capabilities: vec!["filesystem".to_string()],
                settings: HashMap::new(),
            },
        );
        plugins.insert(
            "duplicate".to_string(),
            PluginConfig {
                enabled: true,
                path: Some("plugins/duplicate.wasm".to_string()),
                capabilities: vec!["filesystem".to_string(), "kv".to_string()],
                settings: HashMap::new(),
            },
        );
        plugins.insert(
            "timeline".to_string(),
            PluginConfig {
                enabled: true,
                path: Some("plugins/timeline.wasm".to_string()),
                capabilities: vec!["filesystem".to_string()],
                settings: HashMap::new(),
            },
        );

        Self {
            database_path: default_database_path(),
            resource_limits: ResourceLimits::default(),
            pipeline: vec![
                "metadata".to_string(),
                "reversegeo".to_string(),
                "countryflag".to_string(),
                "thumbnail".to_string(),
                "ai-tagging".to_string(),
                "duplicate".to_string(),
                "timeline".to_string(),
            ],
            plugins,
        }
    }
}

impl AppConfig {
    pub fn load_from_str(s: &str) -> anyhow::Result<Self> {
        // Simple YAML-like / JSON parser support
        if let Ok(cfg) = serde_json::from_str::<AppConfig>(s) {
            return Ok(cfg);
        }
        // Fallback to default if empty or parse standard config format
        let mut cfg = AppConfig::default();
        for line in s.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            if let Some((k, v)) = trimmed.split_once(':') {
                let k = k.trim();
                let v = v.trim().trim_matches('"').trim_matches('\'').trim();
                if k == "database_path" {
                    cfg.database_path = v.to_string();
                }
            }
        }
        Ok(cfg)
    }

    pub fn load_from_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::load_from_str(&content)
    }
}
