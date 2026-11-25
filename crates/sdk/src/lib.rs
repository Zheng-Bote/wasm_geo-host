pub use wasm_geo_models::*;
pub use serde;
pub use serde_json;

use std::sync::Arc;

#[derive(Debug)]
pub enum PluginError {
    Serialization(serde_json::Error),
    CapabilityDenied(String),
    Io(String),
    ProcessingFailed(String),
    Config(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serialization(e) => write!(f, "Serialization error: {}", e),
            Self::CapabilityDenied(s) => write!(f, "Capability not granted: {}", s),
            Self::Io(s) => write!(f, "IO error: {}", s),
            Self::ProcessingFailed(s) => write!(f, "Processing failure: {}", s),
            Self::Config(s) => write!(f, "Configuration error: {}", s),
        }
    }
}

impl std::error::Error for PluginError {}

impl From<serde_json::Error> for PluginError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e)
    }
}

pub type PluginResult<T> = Result<T, PluginError>;

/// Host capability abstraction granted to plugins.
pub trait HostCapabilities: Send + Sync {
    fn fs_read(&self, path: &str) -> PluginResult<Vec<u8>>;
    fn http_get(&self, url: &str) -> PluginResult<String>;
    fn kv_get(&self, key: &str) -> PluginResult<Option<String>>;
    fn kv_set(&self, key: &str, value: &str) -> PluginResult<()>;
    fn secret_get(&self, name: &str) -> PluginResult<Option<String>>;
    fn log(&self, level: &str, message: &str);
}

/// Execution context provided to every plugin execution.
pub struct PluginContext {
    pub capabilities: Arc<dyn HostCapabilities>,
}

impl PluginContext {
    pub fn new(capabilities: Arc<dyn HostCapabilities>) -> Self {
        Self { capabilities }
    }
}

/// Trait implemented by every WASM Geo Host plugin.
pub trait Plugin: Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn process(&self, doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument>;
}

/// Dummy / Noop capabilities for testing and standalone execution.
pub struct NoopCapabilities;

impl HostCapabilities for NoopCapabilities {
    fn fs_read(&self, _path: &str) -> PluginResult<Vec<u8>> {
        Err(PluginError::CapabilityDenied("fs".to_string()))
    }
    fn http_get(&self, _url: &str) -> PluginResult<String> {
        Err(PluginError::CapabilityDenied("http".to_string()))
    }
    fn kv_get(&self, _key: &str) -> PluginResult<Option<String>> {
        Ok(None)
    }
    fn kv_set(&self, _key: &str, _value: &str) -> PluginResult<()> {
        Ok(())
    }
    fn secret_get(&self, _name: &str) -> PluginResult<Option<String>> {
        Ok(None)
    }
    fn log(&self, level: &str, message: &str) {
        match level {
            "error" => tracing::error!("{}", message),
            "warn" => tracing::warn!("{}", message),
            "info" => tracing::info!("{}", message),
            _ => tracing::debug!("{}", message),
        }
    }
}
