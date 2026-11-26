use flagme_sdk::{HostCapabilities, PluginError, PluginResult};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, RwLock};

/// Capability enforcement manager for a specific plugin.
pub struct CapabilityManager {
    plugin_name: String,
    granted_capabilities: HashSet<String>,
    kv_store: Arc<RwLock<HashMap<String, String>>>,
    secrets: HashMap<String, String>,
}

impl CapabilityManager {
    pub fn new(
        plugin_name: impl Into<String>,
        granted: Vec<String>,
        kv_store: Arc<RwLock<HashMap<String, String>>>,
    ) -> Self {
        let mut set = HashSet::new();
        for cap in granted {
            set.insert(cap.to_lowercase());
        }
        Self {
            plugin_name: plugin_name.into(),
            granted_capabilities: set,
            kv_store,
            secrets: HashMap::new(),
        }
    }

    pub fn set_secret(&mut self, key: impl Into<String>, val: impl Into<String>) {
        self.secrets.insert(key.into(), val.into());
    }

    pub fn has_capability(&self, cap: &str) -> bool {
        self.granted_capabilities.contains(cap)
    }

    fn check_capability(&self, cap: &str) -> PluginResult<()> {
        if self.has_capability(cap) {
            Ok(())
        } else {
            Err(PluginError::CapabilityDenied(format!(
                "Plugin '{}' lacks capability '{}'",
                self.plugin_name, cap
            )))
        }
    }
}

impl HostCapabilities for CapabilityManager {
    fn fs_read(&self, path: &str) -> PluginResult<Vec<u8>> {
        self.check_capability("filesystem")?;

        let p = Path::new(path);
        if path.contains("..") {
            return Err(PluginError::Io("Path traversal forbidden".to_string()));
        }

        std::fs::read(p).map_err(|e| PluginError::Io(format!("fs_read failed for {}: {}", path, e)))
    }

    fn fs_write(&self, path: &str, data: &[u8]) -> PluginResult<()> {
        self.check_capability("filesystem")?;

        if path.contains("..") {
            return Err(PluginError::Io("Path traversal forbidden".to_string()));
        }

        let p = Path::new(path);
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    PluginError::Io(format!("fs_write create_dir_all failed for {}: {}", path, e))
                })?;
            }
        }

        std::fs::write(p, data)
            .map_err(|e| PluginError::Io(format!("fs_write failed for {}: {}", path, e)))
    }

    fn fs_modified(&self, path: &str) -> PluginResult<u64> {
        self.check_capability("filesystem")?;

        if path.contains("..") {
            return Err(PluginError::Io("Path traversal forbidden".to_string()));
        }

        flagme_sdk::modified_unix_seconds(Path::new(path))
    }

    fn http_get(&self, url: &str) -> PluginResult<String> {
        self.check_capability("http")?;

        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(PluginError::ProcessingFailed("Invalid HTTP scheme".to_string()));
        }

        tracing::info!(plugin = %self.plugin_name, url = %url, "Executing HTTP GET");

        // Attempt outbound HTTP fetch with strict 2-second timeout via system curl if network is available
        if let Ok(output) = Command::new("curl")
            .args([
                "-sS",
                "-L",
                "--max-time",
                "2",
                "-H",
                "User-Agent: flagme-host/0.1.0",
                url,
            ])
            .output()
        {
            if output.status.success() && !output.stdout.is_empty() {
                if let Ok(body) = String::from_utf8(output.stdout) {
                    if !body.trim().is_empty() {
                        return Ok(body);
                    }
                }
            }
        }

        Ok(format!(r#"{{"status": "offline", "url": "{}"}}"#, url))
    }

    fn kv_get(&self, key: &str) -> PluginResult<Option<String>> {
        self.check_capability("kv")?;
        let store = self.kv_store.read().map_err(|_| {
            PluginError::ProcessingFailed("KV lock poisoned".to_string())
        })?;
        Ok(store.get(key).cloned())
    }

    fn kv_set(&self, key: &str, value: &str) -> PluginResult<()> {
        self.check_capability("kv")?;
        let mut store = self.kv_store.write().map_err(|_| {
            PluginError::ProcessingFailed("KV lock poisoned".to_string())
        })?;
        store.insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn secret_get(&self, name: &str) -> PluginResult<Option<String>> {
        self.check_capability("secrets")?;
        Ok(self.secrets.get(name).cloned())
    }

    fn log(&self, level: &str, message: &str) {
        match level {
            "error" => tracing::error!(plugin = %self.plugin_name, "{}", message),
            "warn" => tracing::warn!(plugin = %self.plugin_name, "{}", message),
            "info" => tracing::info!(plugin = %self.plugin_name, "{}", message),
            _ => tracing::debug!(plugin = %self.plugin_name, "{}", message),
        }
    }
}
