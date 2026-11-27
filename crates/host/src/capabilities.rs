use flagme_sdk::{HostCapabilities, PluginError, PluginResult};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Total time limit of an outbound `http_get` request, including redirects (ADR 0007).
pub const HTTP_TIMEOUT_SECS: u64 = 2;
/// Largest accepted response body; the body is never read beyond it (ADR 0007).
pub const HTTP_MAX_RESPONSE_BYTES: u64 = 1_048_576;
/// Largest number of followed redirects (ADR 0007).
pub const HTTP_MAX_REDIRECTS: u32 = 5;
/// `User-Agent` of every request (required by Nominatim's usage policy).
pub const HTTP_USER_AGENT: &str = concat!("flagme-host/", env!("CARGO_PKG_VERSION"));

/// The HTTP client of the `http` capability (ADR 0007): `ureq` with rustls and the system
/// certificate store, fixed limits and the proxy settings of the environment
/// (`HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`, `NO_PROXY`). Cloning shares the connection pool.
pub fn http_agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(HTTP_TIMEOUT_SECS)))
        .max_redirects(HTTP_MAX_REDIRECTS)
        .max_redirects_will_error(true)
        .http_status_as_error(true)
        .user_agent(HTTP_USER_AGENT)
        .tls_config(tls)
        .build()
        .into()
}

/// Short cause of a failed request, without client-specific prefixes.
fn http_cause(e: &ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(s) => format!("HTTP status {}", s),
        ureq::Error::Timeout(_) => format!("timed out after {} s", HTTP_TIMEOUT_SECS),
        ureq::Error::TooManyRedirects => format!("more than {} redirects", HTTP_MAX_REDIRECTS),
        ureq::Error::BodyExceedsLimit(_) => format!("response larger than {} bytes", HTTP_MAX_RESPONSE_BYTES),
        ureq::Error::Tls(_) | ureq::Error::Rustls(_) => format!("TLS: {}", e),
        other => other.to_string(),
    }
}

/// Capability enforcement manager for a specific plugin.
pub struct CapabilityManager {
    plugin_name: String,
    granted_capabilities: HashSet<String>,
    kv_store: Arc<RwLock<HashMap<String, String>>>,
    secrets: HashMap<String, String>,
    http: ureq::Agent,
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
            http: http_agent(),
        }
    }

    /// Uses a shared HTTP client (and its connection pool) instead of an own one.
    pub fn with_http_agent(mut self, agent: ureq::Agent) -> Self {
        self.http = agent;
        self
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

        // In-process request (ADR 0007). Every failure is reported as an error; the host never
        // builds a response body itself (#52, ADR 0002).
        let fail = |cause: String| PluginError::Io(format!("http_get {}: {}", url, cause));
        let mut response = self.http.get(url).call().map_err(|e| fail(http_cause(&e)))?;
        let bytes = response
            .body_mut()
            .with_config()
            .limit(HTTP_MAX_RESPONSE_BYTES)
            .read_to_vec()
            .map_err(|e| fail(http_cause(&e)))?;
        let body = String::from_utf8(bytes).map_err(|_| fail("response is not UTF-8".to_string()))?;
        if body.trim().is_empty() {
            return Err(fail("empty response".to_string()));
        }
        Ok(body)
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
