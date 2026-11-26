//! Shared parsing of plugin `settings` from `config.yaml`.
//!
//! Every plugin reads its parameters through a [`SettingsReader`]. The reader applies the
//! same rules for all plugins (booleans, ranges, choices, lists, paths, URLs), reports
//! problems as `warn` log entries via the host capabilities and falls back to the default.
//! [`SettingsCache`] parses a settings map only once, so warnings appear once per
//! configuration instead of once per document.

use crate::{HostCapabilities, PluginContext};
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::ops::RangeInclusive;
use std::sync::{Arc, Mutex};

/// Values accepted as `true` (case-insensitive, surrounding whitespace ignored).
pub const TRUE_VALUES: &[&str] = &["true", "yes", "on", "1"];
/// Values accepted as `false` (case-insensitive, surrounding whitespace ignored).
pub const FALSE_VALUES: &[&str] = &["false", "no", "off", "0"];

/// Parses a boolean according to the shared rule; `None` if the value is not recognised.
pub fn parse_bool(value: &str) -> Option<bool> {
    let v = value.trim().to_ascii_lowercase();
    if TRUE_VALUES.contains(&v.as_str()) {
        Some(true)
    } else if FALSE_VALUES.contains(&v.as_str()) {
        Some(false)
    } else {
        None
    }
}

/// Reads typed values from a settings map and tracks which keys were consumed.
pub struct SettingsReader<'a> {
    plugin: &'static str,
    map: &'a HashMap<String, String>,
    capabilities: &'a dyn HostCapabilities,
    seen: HashSet<String>,
}

impl<'a> SettingsReader<'a> {
    pub fn new(plugin: &'static str, ctx: &'a PluginContext) -> Self {
        Self::from_parts(plugin, &ctx.settings, ctx.capabilities.as_ref())
    }

    pub fn from_parts(
        plugin: &'static str,
        map: &'a HashMap<String, String>,
        capabilities: &'a dyn HostCapabilities,
    ) -> Self {
        Self {
            plugin,
            map,
            capabilities,
            seen: HashSet::new(),
        }
    }

    fn warn(&self, message: String) {
        self.capabilities
            .log("warn", &format!("settings[{}]: {}", self.plugin, message));
    }

    /// Emits the standard "invalid value" warning.
    pub fn warn_invalid(&self, key: &str, value: &str, expected: &str, default: &dyn Display) {
        self.warn(format!(
            "invalid value '{}' for '{}' (expected {}), using default '{}'",
            value, key, expected, default
        ));
    }

    /// Emits the standard "not supported yet" warning.
    pub fn warn_unsupported(&self, key: &str, value: &str, supported: &[&str], default: &dyn Display) {
        self.warn(format!(
            "'{}' for '{}' is not supported yet (supported: {}), using default '{}'",
            value,
            key,
            supported.join(", "),
            default
        ));
    }

    /// Consumes `key`; any value is reported as "not supported yet" and ignored.
    ///
    /// Used for settings that select an implementation which does not exist (ADR 0002).
    pub fn not_supported(&mut self, key: &str, aliases: &[&str], reason: &str) {
        if let Some((k, v)) = self.lookup(key, aliases) {
            self.warn(format!(
                "'{}' for '{}' is not supported yet ({}), ignored",
                v, k, reason
            ));
        }
    }

    /// Looks up the canonical key first, then the aliases. Marks all of them as seen.
    /// Returns the key that was used and its trimmed value.
    fn lookup(&mut self, key: &str, aliases: &[&str]) -> Option<(String, String)> {
        self.seen.insert(key.to_string());
        for a in aliases {
            self.seen.insert((*a).to_string());
        }
        let canonical = self.map.get(key).map(|v| (key.to_string(), v.trim().to_string()));
        let present_aliases: Vec<&str> = aliases
            .iter()
            .copied()
            .filter(|a| self.map.contains_key(*a))
            .collect();
        match canonical {
            Some(found) => {
                for a in present_aliases {
                    self.warn(format!("alias '{}' ignored because '{}' is set", a, key));
                }
                Some(found)
            }
            None => present_aliases.first().map(|a| {
                let v = self.map.get(*a).map(|v| v.trim().to_string()).unwrap_or_default();
                ((*a).to_string(), v)
            }),
        }
    }

    pub fn bool(&mut self, key: &str, aliases: &[&str], default: bool) -> bool {
        let Some((k, v)) = self.lookup(key, aliases) else {
            return default;
        };
        match parse_bool(&v) {
            Some(b) => b,
            None => {
                self.warn_invalid(&k, &v, "true|false|yes|no|on|off|1|0", &default);
                default
            }
        }
    }

    pub fn u32(&mut self, key: &str, aliases: &[&str], range: RangeInclusive<u32>, default: u32) -> u32 {
        self.opt_u32(key, aliases, range).unwrap_or(default)
    }

    /// Like [`Self::u32`], but without a default: missing or invalid values yield `None`.
    pub fn opt_u32(&mut self, key: &str, aliases: &[&str], range: RangeInclusive<u32>) -> Option<u32> {
        let (k, v) = self.lookup(key, aliases)?;
        match v.parse::<i64>() {
            Ok(n) => Some(self.clamp_u32(&k, &v, n, &range)),
            Err(_) => {
                self.warn_invalid(
                    &k,
                    &v,
                    &format!("integer {}..={}", range.start(), range.end()),
                    &"unset",
                );
                None
            }
        }
    }

    fn clamp_u32(&self, key: &str, raw: &str, n: i64, range: &RangeInclusive<u32>) -> u32 {
        let min = *range.start() as i64;
        let max = *range.end() as i64;
        let clamped = n.clamp(min, max);
        if clamped != n {
            self.warn(format!(
                "value '{}' for '{}' out of range {}..={}, using '{}'",
                raw, key, min, max, clamped
            ));
        }
        clamped as u32
    }

    pub fn f32(&mut self, key: &str, aliases: &[&str], range: RangeInclusive<f32>, default: f32) -> f32 {
        let Some((k, v)) = self.lookup(key, aliases) else {
            return default;
        };
        match v.parse::<f32>() {
            Ok(n) if n.is_finite() => {
                let clamped = n.clamp(*range.start(), *range.end());
                if clamped != n {
                    self.warn(format!(
                        "value '{}' for '{}' out of range {}..={}, using '{}'",
                        v,
                        k,
                        range.start(),
                        range.end(),
                        clamped
                    ));
                }
                clamped
            }
            _ => {
                self.warn_invalid(
                    &k,
                    &v,
                    &format!("number {}..={}", range.start(), range.end()),
                    &default,
                );
                default
            }
        }
    }

    /// Selects one of `options` (each with its accepted spellings). Values listed in
    /// `unsupported` are known but not implemented yet and produce the "not supported" warning.
    pub fn choice<T: Copy + PartialEq>(
        &mut self,
        key: &str,
        aliases: &[&str],
        options: &[(&[&str], T)],
        unsupported: &[&str],
        default: T,
    ) -> T {
        self.opt_choice(key, aliases, options, unsupported, default)
            .unwrap_or(default)
    }

    /// Like [`Self::choice`], but returns `None` if the key is missing or invalid.
    /// `default` is only used for the warning text.
    pub fn opt_choice<T: Copy + PartialEq>(
        &mut self,
        key: &str,
        aliases: &[&str],
        options: &[(&[&str], T)],
        unsupported: &[&str],
        default: T,
    ) -> Option<T> {
        let (k, v) = self.lookup(key, aliases)?;
        if let Some(t) = match_choice(&v, options) {
            return Some(t);
        }
        let default_name = choice_name(options, default);
        let supported: Vec<&str> = options.iter().filter_map(|(names, _)| names.first().copied()).collect();
        if unsupported.iter().any(|u| u.eq_ignore_ascii_case(&v)) {
            self.warn_unsupported(&k, &v, &supported, &default_name);
        } else {
            self.warn_invalid(&k, &v, &supported.join("|"), &default_name);
        }
        None
    }

    /// Comma-separated list of integers; sorted ascending, duplicates removed.
    pub fn list_u32(&mut self, key: &str, range: RangeInclusive<u32>, default: &[u32]) -> Vec<u32> {
        let Some((k, v)) = self.lookup(key, &[]) else {
            return default.to_vec();
        };
        let mut out = Vec::new();
        for part in v.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            match part.parse::<i64>() {
                Ok(n) => out.push(self.clamp_u32(&k, part, n, &range)),
                Err(_) => {
                    self.warn_invalid(&k, &v, "comma-separated integers", &join_display(default));
                    return default.to_vec();
                }
            }
        }
        if out.is_empty() {
            self.warn_invalid(&k, &v, "comma-separated integers", &join_display(default));
            return default.to_vec();
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Comma-separated list of choices, order preserved, duplicates removed.
    pub fn list_choice<T: Copy + PartialEq>(
        &mut self,
        key: &str,
        options: &[(&[&str], T)],
        default: &[T],
    ) -> Vec<T> {
        let Some((k, v)) = self.lookup(key, &[]) else {
            return default.to_vec();
        };
        let supported: Vec<&str> = options.iter().filter_map(|(names, _)| names.first().copied()).collect();
        let default_names: Vec<String> = default.iter().map(|d| choice_name(options, *d)).collect();
        let mut out: Vec<T> = Vec::new();
        for part in v.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            match match_choice(part, options) {
                Some(t) => {
                    if !out.contains(&t) {
                        out.push(t);
                    }
                }
                None => {
                    self.warn_invalid(&k, &v, &format!("list of {}", supported.join("|")), &default_names.join(","));
                    return default.to_vec();
                }
            }
        }
        if out.is_empty() {
            self.warn_invalid(&k, &v, &format!("list of {}", supported.join("|")), &default_names.join(","));
            return default.to_vec();
        }
        out
    }

    pub fn string(&mut self, key: &str, aliases: &[&str], default: &str) -> String {
        match self.lookup(key, aliases) {
            Some((k, v)) if v.is_empty() => {
                self.warn_invalid(&k, &v, "non-empty string", &default);
                default.to_string()
            }
            Some((_, v)) => v,
            None => default.to_string(),
        }
    }

    /// Directory path without `..`; a trailing `/` is removed.
    pub fn path(&mut self, key: &str, aliases: &[&str], default: &str) -> String {
        let Some((k, v)) = self.lookup(key, aliases) else {
            return default.to_string();
        };
        let trimmed = v.trim_end_matches('/');
        if trimmed.is_empty() || trimmed.contains("..") {
            self.warn_invalid(&k, &v, "path without '..'", &default);
            return default.to_string();
        }
        trimmed.to_string()
    }

    /// URL starting with `https://` or `http://`.
    pub fn url(&mut self, key: &str, aliases: &[&str], default: &str) -> String {
        let Some((k, v)) = self.lookup(key, aliases) else {
            return default.to_string();
        };
        if (v.starts_with("https://") || v.starts_with("http://")) && v.len() > "https://".len() {
            v
        } else {
            self.warn_invalid(&k, &v, "http(s):// URL", &default);
            default.to_string()
        }
    }

    /// Reads a value with a custom parser. Invalid values produce the standard warning.
    pub fn parse_with<T>(
        &mut self,
        key: &str,
        aliases: &[&str],
        expected: &str,
        default: T,
        default_display: &str,
        parse: impl Fn(&str) -> Option<T>,
    ) -> T {
        let Some((k, v)) = self.lookup(key, aliases) else {
            return default;
        };
        match parse(&v) {
            Some(t) => t,
            None => {
                self.warn_invalid(&k, &v, expected, &default_display);
                default
            }
        }
    }

    /// Warns once for every key in the map that no read call has consumed.
    pub fn finish(self) {
        let mut unknown: Vec<&String> = self.map.keys().filter(|k| !self.seen.contains(*k)).collect();
        unknown.sort();
        for k in unknown {
            self.warn(format!("unknown key '{}' ignored", k));
        }
    }
}

fn match_choice<T: Copy>(value: &str, options: &[(&[&str], T)]) -> Option<T> {
    options
        .iter()
        .find(|(names, _)| names.iter().any(|n| n.eq_ignore_ascii_case(value.trim())))
        .map(|(_, t)| *t)
}

fn choice_name<T: Copy + PartialEq>(options: &[(&[&str], T)], value: T) -> String {
    options
        .iter()
        .find(|(_, t)| *t == value)
        .and_then(|(names, _)| names.first())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn join_display(values: &[u32]) -> String {
    values.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",")
}

/// Typed settings of one plugin.
pub trait PluginSettings: Sized + Clone + Send + Sync {
    /// Plugin name used in warnings.
    const PLUGIN: &'static str;

    /// Builds the effective settings: `base` holds the defaults (or the values passed to the
    /// plugin's constructor); every key present in the reader overrides them.
    fn from_reader(base: &Self, reader: &mut SettingsReader<'_>) -> Self;

    /// Parses `map` on top of `base` and reports unknown keys.
    fn parse(base: &Self, map: &HashMap<String, String>, capabilities: &dyn HostCapabilities) -> Self {
        let mut reader = SettingsReader::from_parts(Self::PLUGIN, map, capabilities);
        let parsed = Self::from_reader(base, &mut reader);
        reader.finish();
        parsed
    }
}

/// Caches the parsed settings per distinct settings map.
pub struct SettingsCache<T: PluginSettings> {
    base: T,
    inner: Mutex<Option<(HashMap<String, String>, Arc<T>)>>,
}

impl<T: PluginSettings> SettingsCache<T> {
    pub fn new(base: T) -> Self {
        Self {
            base,
            inner: Mutex::new(None),
        }
    }

    /// The constructor values (before `ctx.settings` is applied).
    pub fn base(&self) -> &T {
        &self.base
    }

    /// Returns the effective settings for `ctx`; parses (and warns) only if the map changed.
    pub fn get_or_parse(&self, ctx: &PluginContext) -> Arc<T> {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some((map, parsed)) = guard.as_ref() {
            if *map == ctx.settings {
                return parsed.clone();
            }
        }
        let parsed = Arc::new(T::parse(&self.base, &ctx.settings, ctx.capabilities.as_ref()));
        *guard = Some((ctx.settings.clone(), parsed.clone()));
        parsed
    }
}
