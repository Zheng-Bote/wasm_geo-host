use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_database_path")]
    pub database_path: String,
    /// Directory with the plugin components; used for plugins without an explicit `path`.
    #[serde(default = "default_plugin_dir")]
    pub plugin_dir: String,
    /// Cache for compiled components (ADR 0004); `false` compiles on every start.
    #[serde(default = "default_true")]
    pub component_cache: bool,
    /// Directory of the component cache; relative paths are resolved against the working
    /// directory, like `plugin_dir`.
    #[serde(default = "default_component_cache_dir")]
    pub component_cache_dir: String,
    #[serde(default)]
    pub resource_limits: ResourceLimits,
    #[serde(default)]
    pub pipeline: Vec<String>,
    #[serde(default)]
    pub plugins: HashMap<String, PluginConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Output directory of `cargo build --release --target wasm32-wasip2 --lib --features component`.
pub const DEFAULT_PLUGIN_DIR: &str = "target/wasm32-wasip2/release";

fn default_plugin_dir() -> String {
    DEFAULT_PLUGIN_DIR.to_string()
}

/// Default directory of the component cache (ADR 0004).
pub const DEFAULT_COMPONENT_CACHE_DIR: &str = "cache/components";

fn default_component_cache_dir() -> String {
    DEFAULT_COMPONENT_CACHE_DIR.to_string()
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
    /// Limits of this plugin (`max_memory_bytes`, `timeout_ms`) as raw values; resolved
    /// against the global `resource_limits` by [`AppConfig::plugin_limits`].
    #[serde(default)]
    pub resource_limits: HashMap<String, serde_json::Value>,
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
                path: None,
                capabilities: vec!["filesystem".to_string()],
                settings: HashMap::new(),
                resource_limits: HashMap::new(),
            },
        );
        plugins.insert(
            "reversegeo".to_string(),
            PluginConfig {
                enabled: true,
                path: None,
                capabilities: vec!["http".to_string(), "kv".to_string()],
                settings: HashMap::new(),
                resource_limits: HashMap::new(),
            },
        );
        plugins.insert(
            "countryflag".to_string(),
            PluginConfig {
                enabled: true,
                path: None,
                capabilities: vec![],
                settings: HashMap::new(),
                resource_limits: HashMap::new(),
            },
        );
        plugins.insert(
            "thumbnail".to_string(),
            PluginConfig {
                enabled: true,
                path: None,
                capabilities: vec!["filesystem".to_string()],
                settings: HashMap::new(),
                resource_limits: HashMap::new(),
            },
        );
        plugins.insert(
            "ai-tagging".to_string(),
            PluginConfig {
                // experimental: no engine built in, produces no results (ADR 0002)
                enabled: false,
                path: None,
                capabilities: vec!["filesystem".to_string()],
                settings: HashMap::new(),
                resource_limits: HashMap::new(),
            },
        );
        plugins.insert(
            "duplicate".to_string(),
            PluginConfig {
                enabled: true,
                path: None,
                capabilities: vec!["filesystem".to_string(), "kv".to_string()],
                settings: HashMap::new(),
                resource_limits: HashMap::new(),
            },
        );
        plugins.insert(
            "timeline".to_string(),
            PluginConfig {
                enabled: true,
                path: None,
                capabilities: vec!["filesystem".to_string()],
                settings: HashMap::new(),
                resource_limits: HashMap::new(),
            },
        );
        plugins.insert(
            "weather-enrich".to_string(),
            PluginConfig {
                enabled: true,
                path: None,
                capabilities: vec!["http".to_string(), "kv".to_string()],
                settings: HashMap::new(),
                resource_limits: HashMap::new(),
            },
        );
        plugins.insert(
            "ocr-text".to_string(),
            PluginConfig {
                // experimental: no engine built in, produces no results (ADR 0002)
                enabled: false,
                path: None,
                capabilities: vec!["filesystem".to_string()],
                settings: HashMap::new(),
                resource_limits: HashMap::new(),
            },
        );
        plugins.insert(
            "face-detection".to_string(),
            PluginConfig {
                // built-in YuNet detector (ADR 0009)
                enabled: true,
                path: None,
                capabilities: vec!["filesystem".to_string()],
                settings: HashMap::new(),
                resource_limits: HashMap::new(),
            },
        );
        plugins.insert(
            "privacy-redact".to_string(),
            PluginConfig {
                enabled: true,
                path: None,
                capabilities: vec!["filesystem".to_string()],
                settings: HashMap::new(),
                resource_limits: HashMap::new(),
            },
        );

        Self {
            database_path: default_database_path(),
            plugin_dir: default_plugin_dir(),
            component_cache: true,
            component_cache_dir: default_component_cache_dir(),
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
    /// Directory of the component cache, or `None` if `component_cache` is off.
    pub fn component_cache_dir(&self) -> Option<std::path::PathBuf> {
        self.component_cache.then(|| std::path::PathBuf::from(&self.component_cache_dir))
    }

    /// Effective limits of plugin `name`: each field of `plugins.<name>.resource_limits` that is
    /// a positive integer replaces the global value, a missing or invalid field inherits it.
    /// Returns the limits and one warning per invalid value or unknown key (sorted by key).
    pub fn plugin_limits(&self, name: &str) -> (ResourceLimits, Vec<String>) {
        let mut limits = self.resource_limits.clone();
        let mut warnings = Vec::new();
        let Some(raw) = self.plugins.get(name).map(|p| &p.resource_limits) else {
            return (limits, warnings);
        };
        let mut keys: Vec<&String> = raw.keys().collect();
        keys.sort();
        for key in keys {
            let value = &raw[key];
            let field = match key.as_str() {
                "max_memory_bytes" => &mut limits.max_memory_bytes,
                "timeout_ms" => &mut limits.timeout_ms,
                _ => {
                    warnings.push(format!("plugin '{}': unknown resource_limits key '{}'; ignored", name, key));
                    continue;
                }
            };
            let parsed = match value {
                serde_json::Value::Number(n) => n.as_u64(),
                serde_json::Value::String(s) => s.trim().parse::<u64>().ok(),
                _ => None,
            };
            match parsed {
                Some(v) if v > 0 => *field = v,
                _ => warnings.push(format!(
                    "plugin '{}': resource_limits.{} = {} is not a positive integer; using the global value {}",
                    name, key, value, field
                )),
            }
        }
        (limits, warnings)
    }

    /// Component file of a plugin: its `path`, else `<plugin_dir>/flagme_plugin_<name>.wasm`
    /// (`-` in the name becomes `_`, as in the Cargo artifact name).
    pub fn component_path(&self, name: &str) -> std::path::PathBuf {
        match self.plugins.get(name).and_then(|p| p.path.as_deref()) {
            Some(path) => std::path::PathBuf::from(path),
            None => Path::new(&self.plugin_dir).join(format!("flagme_plugin_{}.wasm", name.replace('-', "_"))),
        }
    }

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
        let mut in_limits = false;

        for raw_line in s.lines() {
            let line_no_comment = strip_comment(raw_line);
            if line_no_comment.trim().is_empty() {
                continue;
            }

            let indent = line_no_comment.chars().take_while(|c| *c == ' ').count();
            let trimmed = line_no_comment.trim();

            if indent == 0 {
                current_plugin = None;
                in_capabilities = false;
                in_settings = false;
                in_limits = false;

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
                        "plugin_dir" => {
                            section = Section::Root;
                            if !v.is_empty() {
                                cfg.plugin_dir = v;
                            }
                        }
                        "component_cache" => {
                            section = Section::Root;
                            match v.to_ascii_lowercase().as_str() {
                                "true" => cfg.component_cache = true,
                                "false" => cfg.component_cache = false,
                                _ => tracing::warn!(
                                    value = %v,
                                    "component_cache must be true or false; keeping {}",
                                    cfg.component_cache
                                ),
                            }
                        }
                        "component_cache_dir" => {
                            section = Section::Root;
                            if !v.is_empty() {
                                cfg.component_cache_dir = v;
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
                            path: None,
                            capabilities: Vec::new(),
                            settings: HashMap::new(),
                            resource_limits: HashMap::new(),
                        });
                        current_plugin = Some(pname);
                        in_capabilities = false;
                        in_settings = false;
                        in_limits = false;
                    } else if let Some(ref pname) = current_plugin {
                        if indent == 4 {
                            if let Some((k, v)) = trimmed.split_once(':') {
                                let k = k.trim();
                                let v_raw = v.trim();
                                let v_clean = clean_yaml_scalar(v_raw);
                                in_capabilities = false;
                                in_settings = false;
                                in_limits = false;

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
                                        "resource_limits" => {
                                            in_limits = true;
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
                            } else if in_limits {
                                if let Some((lk, lv)) = trimmed.split_once(':') {
                                    let lk = clean_yaml_scalar(lk);
                                    let lv = clean_yaml_scalar(lv);
                                    if let Some(pcfg) = cfg.plugins.get_mut(pname) {
                                        pcfg.resource_limits.insert(lk, serde_json::Value::String(lv));
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

/// Removes a trailing `# comment`. A `#` inside a quoted scalar (`"…"` or `'…'`) is part of
/// the value. A quote only opens a scalar at the start of a value (after `:`, `-`, `[`, `,`
/// or at the beginning of the line), so apostrophes in plain text stay plain.
fn strip_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut prev_significant: Option<char> = None;
    for (idx, c) in line.char_indices() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => {
                if c == '#' {
                    return &line[..idx];
                }
                if (c == '"' || c == '\'')
                    && matches!(prev_significant, None | Some(':') | Some('-') | Some('[') | Some(','))
                {
                    quote = Some(c);
                }
            }
        }
        if !c.is_whitespace() {
            prev_significant = Some(c);
        }
    }
    line
}

fn clean_yaml_scalar(s: &str) -> String {
    s.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}
