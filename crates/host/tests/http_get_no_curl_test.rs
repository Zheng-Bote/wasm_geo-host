//! Separate test binary: it empties `PATH` for the whole process, which must not affect
//! tests that start `curl` or `sqlite3`.

use flagme_host::CapabilityManager;
use flagme_sdk::{HostCapabilities, PluginError};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// honest-http-get AC-1: if curl cannot be started, http_get fails without a response body
#[test]
fn test_ac1_missing_curl_is_an_error() {
    std::env::set_var("PATH", "");
    let caps = CapabilityManager::new("test", vec!["http".to_string()], Arc::new(RwLock::new(HashMap::new())));
    let err = caps.http_get("https://example.org/").unwrap_err();
    assert!(matches!(&err, PluginError::Io(m) if m.contains("cannot run curl")), "{:?}", err);
}
