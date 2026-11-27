use crate::capabilities::CapabilityManager;
use crate::component::{CacheStats, ComponentEngine, HostState, MediaPluginPre};
use crate::config::AppConfig;
use flagme_sdk::{PluginError, PluginResult, UnifiedDocument};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};

/// A plugin loaded from its Wasm component.
struct LoadedPlugin {
    pre: MediaPluginPre<HostState>,
    version: String,
}

/// Plugin host runtime: loads every enabled pipeline plugin from its Wasm component
/// (`PluginConfig.path`) and runs it in a sandbox with its capability grants (ADR 0003).
pub struct PluginRuntime {
    config: AppConfig,
    engine: ComponentEngine,
    kv_store: Arc<RwLock<HashMap<String, String>>>,
    warned: Arc<Mutex<HashSet<String>>>,
    plugins: HashMap<String, LoadedPlugin>,
}

impl PluginRuntime {
    /// Loads the components of all enabled plugins named in `config.pipeline`.
    ///
    /// The file comes from [`AppConfig::component_path`]. Fails if the file cannot be loaded, it does not implement the
    /// `media-plugin` world or its `name` export differs from the configured name. There is
    /// no native fallback.
    pub fn new(config: AppConfig) -> anyhow::Result<Self> {
        let engine = ComponentEngine::new(config.component_cache_dir().as_deref())?;
        let mut plugins = HashMap::new();
        for name in &config.pipeline {
            let Some(cfg) = config.plugins.get(name) else {
                anyhow::bail!("pipeline plugin '{}' has no entry under `plugins`", name);
            };
            if !cfg.enabled || plugins.contains_key(name) {
                continue;
            }
            let path = config.component_path(name);
            let pre = engine.load(&path)?;
            let (component_name, version) = engine.component_identity(&pre, &config.resource_limits)?;
            if component_name != *name {
                anyhow::bail!(
                    "component '{}' is plugin '{}', but is configured as '{}'",
                    path.display(),
                    component_name,
                    name
                );
            }
            tracing::debug!(plugin = %name, version = %version, path = %path.display(), "Loaded plugin component");
            plugins.insert(name.clone(), LoadedPlugin { pre, version });
        }
        Ok(Self {
            config,
            engine,
            kv_store: Arc::new(RwLock::new(HashMap::new())),
            warned: Arc::new(Mutex::new(HashSet::new())),
            plugins,
        })
    }

    /// Executes a plugin by name against a document within its capability sandbox.
    /// Disabled plugins return the document unchanged.
    pub fn execute_plugin(&self, name: &str, doc: UnifiedDocument) -> PluginResult<UnifiedDocument> {
        let Some(cfg) = self.config.plugins.get(name) else {
            return Err(PluginError::ProcessingFailed(format!("Plugin '{}' not found", name)));
        };
        if !cfg.enabled {
            tracing::info!(plugin = %name, "Skipping disabled plugin");
            return Ok(doc);
        }
        let plugin = self.plugins.get(name).ok_or_else(|| {
            PluginError::ProcessingFailed(format!("Plugin '{}' not loaded", name))
        })?;

        let caps = CapabilityManager::new(name, cfg.capabilities.clone(), self.kv_store.clone());
        let start = std::time::Instant::now();
        tracing::debug!(plugin = %name, "Invoking plugin");
        let result = self.engine.process(
            &plugin.pre,
            caps,
            &self.config.resource_limits,
            self.warned.clone(),
            doc,
            &cfg.settings,
        )?;
        tracing::info!(
            plugin = %name,
            elapsed_ms = start.elapsed().as_millis(),
            "Plugin execution completed"
        );
        Ok(result)
    }

    /// Hits and misses of the component cache, or `None` if it is off (ADR 0004).
    pub fn component_cache_stats(&self) -> Option<CacheStats> {
        self.engine.cache_stats()
    }

    /// Names of the loaded plugin components.
    pub fn registered_plugin_names(&self) -> Vec<String> {
        self.plugins.keys().cloned().collect()
    }

    /// `version` export of a loaded plugin component.
    pub fn plugin_version(&self, name: &str) -> Option<&str> {
        self.plugins.get(name).map(|p| p.version.as_str())
    }
}
