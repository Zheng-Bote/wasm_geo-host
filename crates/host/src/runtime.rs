use crate::capabilities::CapabilityManager;
use crate::config::{AppConfig, PluginConfig};
use flagme_sdk::{Plugin, PluginContext, PluginResult, UnifiedDocument};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Plugin Host Runtime managing discovery, loading, capability sandbox, and execution.
pub struct PluginRuntime {
    config: AppConfig,
    kv_store: Arc<RwLock<HashMap<String, String>>>,
    registered_plugins: HashMap<String, Arc<dyn Plugin>>,
}

impl PluginRuntime {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config,
            kv_store: Arc::new(RwLock::new(HashMap::new())),
            registered_plugins: HashMap::new(),
        }
    }

    /// Register a plugin into the host runtime.
    pub fn register_plugin(&mut self, plugin: Arc<dyn Plugin>) {
        self.registered_plugins.insert(plugin.name().to_string(), plugin);
    }

    /// Execute a plugin by name against a document within its capability sandbox.
    pub fn execute_plugin(
        &self,
        name: &str,
        doc: UnifiedDocument,
    ) -> PluginResult<UnifiedDocument> {
        let plugin = self.registered_plugins.get(name).ok_or_else(|| {
            flagme_sdk::PluginError::ProcessingFailed(format!("Plugin '{}' not found", name))
        })?;

        let plugin_cfg = self
            .config
            .plugins
            .get(name)
            .cloned()
            .unwrap_or_else(|| PluginConfig {
                enabled: true,
                path: None,
                capabilities: Vec::new(),
                settings: HashMap::new(),
            });

        if !plugin_cfg.enabled {
            tracing::info!(plugin = %name, "Skipping disabled plugin");
            return Ok(doc);
        }

        let cap_manager = Arc::new(CapabilityManager::new(
            name,
            plugin_cfg.capabilities,
            self.kv_store.clone(),
        ));

        let ctx = PluginContext::new(cap_manager);

        let start = std::time::Instant::now();
        tracing::debug!(plugin = %name, "Invoking plugin");

        let result = plugin.process(doc, &ctx)?;

        let elapsed = start.elapsed();
        tracing::info!(
            plugin = %name,
            elapsed_ms = elapsed.as_millis(),
            "Plugin execution completed"
        );

        Ok(result)
    }

    pub fn registered_plugin_names(&self) -> Vec<String> {
        self.registered_plugins.keys().cloned().collect()
    }
}
