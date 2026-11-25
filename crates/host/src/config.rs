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
        plugins.insert(
            "weather-enrich".to_string(),
            PluginConfig {
                enabled: true,
                path: Some("plugins/weather-enrich.wasm".to_string()),
                capabilities: vec!["http".to_string(), "kv".to_string()],
                settings: HashMap::new(),
            },
        );
        plugins.insert(
            "ocr-text".to_string(),
            PluginConfig {
                enabled: true,
                path: Some("plugins/ocr-text.wasm".to_string()),
                capabilities: vec!["filesystem".to_string()],
                settings: HashMap::new(),
            },
        );
        plugins.insert(
            "face-detection".to_string(),
            PluginConfig {
                enabled: true,
                path: Some("plugins/face-detection.wasm".to_string()),
                capabilities: vec!["filesystem".to_string()],
                settings: HashMap::new(),
            },
        );
        plugins.insert(
            "privacy-redact".to_string(),
            PluginConfig {
                enabled: true,
                path: Some("plugins/privacy-redact.wasm".to_string()),
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
                "weather-enrich".to_string(),
                "ocr-text".to_string(),
                "face-detection".to_string(),
                "privacy-redact".to_string(),
            ],
            plugins,
        }
    }
}

impl AppConfig {
    pub fn load_from_str(s: &str) -> anyhow::Result<Self> {
        if let Ok(cfg) = serde_json::from_str::<AppConfig>(s) {
            return Ok(cfg);
        }

        let mut cfg = AppConfig::default();
        let mut has_custom_pipeline = false;
        let mut custom_pipeline = Vec::new();

        #[derive(PartialEq)]
        enum Section {
            Root,
            ResourceLimits,
            Pipeline,
            Plugins,
        }

        let mut section = Section::Root;
        let mut current_plugin: Option<String> = None;
        let mut in_capabilities = false;
        let mut in_settings = false;

        for raw_line in s.lines() {
            let line_no_comment = match raw_line.find('#') {
                Some(idx) => &raw_line[..idx],
                None => raw_line,
            };
            if line_no_comment.trim().is_empty() {
                continue;
            }

            let indent = line_no_comment.chars().take_while(|c| *c == ' ').count();
            let trimmed = line_no_comment.trim();

            if indent == 0 {
                current_plugin = None;
                in_capabilities = false;
                in_settings = false;

                if let Some((k, v)) = trimmed.split_once(':') {
                    let k = k.trim();
                    let v = clean_yaml_scalar(v);
                    match k {
                        "database_path" => {
                            section = Section::Root;
                            if !v.is_empty() {
                                cfg.database_path = v;
                            }
                        }
                        "resource_limits" => {
                            section = Section::ResourceLimits;
                        }
                        "pipeline" => {
                            section = Section::Pipeline;
                            has_custom_pipeline = true;
                        }
                        "plugins" => {
                            section = Section::Plugins;
                        }
                        _ => {
                            section = Section::Root;
                        }
                    }
                }
                continue;
            }

            match section {
                Section::ResourceLimits => {
                    if let Some((k, v)) = trimmed.split_once(':') {
                        let k = k.trim();
                        let v = clean_yaml_scalar(v);
                        if k == "max_memory_bytes" {
                            if let Ok(val) = v.parse::<u64>() {
                                cfg.resource_limits.max_memory_bytes = val;
                            }
                        } else if k == "timeout_ms" {
                            if let Ok(val) = v.parse::<u64>() {
                                cfg.resource_limits.timeout_ms = val;
                            }
                        }
                    }
                }
                Section::Pipeline => {
                    if let Some(item) = trimmed.strip_prefix('-') {
                        let plugin_name = clean_yaml_scalar(item);
                        if !plugin_name.is_empty() {
                            custom_pipeline.push(plugin_name);
                        }
                    }
                }
                Section::Plugins => {
                    if indent == 2 && trimmed.ends_with(':') {
                        let pname = trimmed.trim_end_matches(':').trim().to_string();
                        cfg.plugins.entry(pname.clone()).or_insert_with(|| PluginConfig {
                            enabled: true,
                            path: Some(format!("plugins/{}.wasm", pname)),
                            capabilities: Vec::new(),
                            settings: HashMap::new(),
                        });
                        current_plugin = Some(pname);
                        in_capabilities = false;
                        in_settings = false;
                    } else if let Some(ref pname) = current_plugin {
                        if indent == 4 {
                            if let Some((k, v)) = trimmed.split_once(':') {
                                let k = k.trim();
                                let v_raw = v.trim();
                                let v_clean = clean_yaml_scalar(v_raw);
                                in_capabilities = false;
                                in_settings = false;

                                if let Some(pcfg) = cfg.plugins.get_mut(pname) {
                                    match k {
                                        "enabled" => {
                                            pcfg.enabled = v_clean.eq_ignore_ascii_case("true");
                                        }
                                        "path" => {
                                            if !v_clean.is_empty() {
                                                pcfg.path = Some(v_clean);
                                            }
                                        }
                                        "capabilities" => {
                                            pcfg.capabilities.clear();
                                            if v_raw.starts_with('[') && v_raw.ends_with(']') {
                                                let inner = &v_raw[1..v_raw.len() - 1];
                                                for item in inner.split(',') {
                                                    let c = clean_yaml_scalar(item);
                                                    if !c.is_empty() {
                                                        pcfg.capabilities.push(c);
                                                    }
                                                }
                                            } else {
                                                in_capabilities = true;
                                            }
                                        }
                                        "settings" => {
                                            in_settings = true;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        } else if indent >= 6 {
                            if in_capabilities {
                                if let Some(item) = trimmed.strip_prefix('-') {
                                    let cap = clean_yaml_scalar(item);
                                    if !cap.is_empty() {
                                        if let Some(pcfg) = cfg.plugins.get_mut(pname) {
                                            pcfg.capabilities.push(cap);
                                        }
                                    }
                                }
                            } else if in_settings {
                                if let Some((sk, sv)) = trimmed.split_once(':') {
                                    let sk = clean_yaml_scalar(sk);
                                    let sv = clean_yaml_scalar(sv);
                                    if let Some(pcfg) = cfg.plugins.get_mut(pname) {
                                        pcfg.settings.insert(sk, sv);
                                    }
                                }
                            }
                        }
                    }
                }
                Section::Root => {}
            }
        }

        if has_custom_pipeline && !custom_pipeline.is_empty() {
            cfg.pipeline = custom_pipeline;
        }

        Ok(cfg)
    }

    pub fn load_from_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::load_from_str(&content)
    }
}

fn clean_yaml_scalar(s: &str) -> String {
    s.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}
