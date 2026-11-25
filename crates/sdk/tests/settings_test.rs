use flagme_sdk::settings::parse_bool;
use flagme_sdk::{
    HostCapabilities, PluginContext, PluginError, PluginResult, PluginSettings, SettingsCache,
    SettingsReader,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Test double that records `warn` log entries.
#[derive(Default)]
struct RecordingCaps {
    warnings: Mutex<Vec<String>>,
}

impl RecordingCaps {
    fn warnings(&self) -> Vec<String> {
        self.warnings.lock().unwrap().clone()
    }
}

impl HostCapabilities for RecordingCaps {
    fn fs_read(&self, _path: &str) -> PluginResult<Vec<u8>> {
        Err(PluginError::CapabilityDenied("filesystem".into()))
    }
    fn http_get(&self, _url: &str) -> PluginResult<String> {
        Err(PluginError::CapabilityDenied("http".into()))
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
        if level == "warn" {
            self.warnings.lock().unwrap().push(message.to_string());
        }
    }
}

fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Color {
    Red,
    Blue,
}

// plugin-settings AC-1: every reader type returns the default and warns on invalid values
#[test]
fn reader_types_fall_back_with_warning() {
    let caps = RecordingCaps::default();
    let m = map(&[
        ("flag", "maybe"),
        ("count", "abc"),
        ("ratio", "x"),
        ("color", "green"),
        ("sizes", "1,two"),
        ("dir", "../etc"),
        ("endpoint", "ftp://example.org"),
        ("name", ""),
    ]);
    let mut r = SettingsReader::from_parts("test", &m, &caps);

    assert!(r.bool("flag", &[], true));
    assert_eq!(r.u32("count", &[], 1..=10, 5), 5);
    assert_eq!(r.f32("ratio", &[], 0.0..=1.0, 0.5), 0.5);
    let colors: &[(&[&str], Color)] = &[(&["red"], Color::Red), (&["blue"], Color::Blue)];
    assert_eq!(r.choice("color", &[], colors, &[], Color::Red), Color::Red);
    assert_eq!(r.list_u32("sizes", 1..=100, &[3, 4]), vec![3, 4]);
    assert_eq!(r.path("dir", &[], "cache"), "cache");
    assert_eq!(r.url("endpoint", &[], "https://a.example"), "https://a.example");
    assert_eq!(r.string("name", &[], "fallback"), "fallback");
    r.finish();

    let w = caps.warnings();
    assert_eq!(w.len(), 8, "one warning per invalid value: {:?}", w);
    assert!(w.iter().all(|m| m.starts_with("settings[test]: invalid value")));
    assert!(w[3].contains("expected red|blue"));
}

// plugin-settings AC-1: valid values are parsed, out-of-range numbers are clamped with a warning
#[test]
fn reader_types_parse_valid_values() {
    let caps = RecordingCaps::default();
    let m = map(&[
        ("count", "500"),
        ("ratio", "0.25"),
        ("color", "BLUE"),
        ("sizes", " 512, 128,512 ,1024"),
        ("dir", "out/thumbs/"),
        ("endpoint", "http://localhost:8080/api"),
    ]);
    let mut r = SettingsReader::from_parts("test", &m, &caps);

    assert_eq!(r.u32("count", &[], 1..=128, 5), 128);
    assert_eq!(r.f32("ratio", &[], 0.0..=1.0, 0.5), 0.25);
    let colors: &[(&[&str], Color)] = &[(&["red"], Color::Red), (&["blue"], Color::Blue)];
    assert_eq!(r.choice("color", &[], colors, &[], Color::Red), Color::Blue);
    assert_eq!(r.list_u32("sizes", 1..=8192, &[1]), vec![128, 512, 1024]);
    assert_eq!(r.path("dir", &[], "cache"), "out/thumbs");
    assert_eq!(r.url("endpoint", &[], "https://a.example"), "http://localhost:8080/api");
    r.finish();

    let w = caps.warnings();
    assert_eq!(w.len(), 1, "{:?}", w);
    assert_eq!(w[0], "settings[test]: value '500' for 'count' out of range 1..=128, using '128'");
}

// plugin-settings AC-1: the canonical key wins over an alias and the ignored alias is reported
#[test]
fn canonical_key_wins_over_alias() {
    let caps = RecordingCaps::default();
    let m = map(&[("style", "blur"), ("redaction_style", "blackout")]);
    let mut r = SettingsReader::from_parts("test", &m, &caps);
    assert_eq!(r.string("style", &["redaction_style"], "pixelate"), "blur");
    r.finish();
    assert_eq!(
        caps.warnings(),
        vec!["settings[test]: alias 'redaction_style' ignored because 'style' is set".to_string()]
    );
}

// plugin-settings AC-2: true/yes/on/1 and false/no/off/0 are recognised, other values warn and use the default
#[test]
fn bool_rule() {
    for v in ["true", "TRUE", " yes ", "On", "1"] {
        assert_eq!(parse_bool(v), Some(true), "{}", v);
    }
    for v in ["false", "No", "OFF", "0"] {
        assert_eq!(parse_bool(v), Some(false), "{}", v);
    }
    for v in ["", "maybe", "2", "enabled"] {
        assert_eq!(parse_bool(v), None, "{}", v);
    }

    let caps = RecordingCaps::default();
    let m = map(&[("a", "yes"), ("b", "0"), ("c", "maybe")]);
    let mut r = SettingsReader::from_parts("test", &m, &caps);
    assert!(r.bool("a", &[], false));
    assert!(!r.bool("b", &[], true));
    assert!(r.bool("c", &[], true));
    r.finish();
    assert_eq!(caps.warnings().len(), 1);
}

#[derive(Debug, Clone, PartialEq)]
struct DemoSettings {
    enabled: bool,
}

impl PluginSettings for DemoSettings {
    const PLUGIN: &'static str = "demo";
    fn from_reader(base: &Self, r: &mut SettingsReader<'_>) -> Self {
        Self {
            enabled: r.bool("enabled", &[], base.enabled),
        }
    }
}

// plugin-settings AC-3: an unknown key produces exactly one warning naming plugin and key, also across repeated calls
#[test]
fn unknown_key_warns_once() {
    let caps = Arc::new(RecordingCaps::default());
    let ctx = PluginContext::with_settings(caps.clone(), map(&[("enabled", "false"), ("colour", "red")]));
    let cache = SettingsCache::new(DemoSettings { enabled: true });

    for _ in 0..3 {
        let s = cache.get_or_parse(&ctx);
        assert!(!s.enabled);
    }
    assert_eq!(
        caps.warnings(),
        vec!["settings[demo]: unknown key 'colour' ignored".to_string()]
    );

    // A different map is parsed again.
    let ctx2 = PluginContext::with_settings(caps.clone(), map(&[("enabled", "true")]));
    assert!(cache.get_or_parse(&ctx2).enabled);
    assert_eq!(caps.warnings().len(), 1);
}
