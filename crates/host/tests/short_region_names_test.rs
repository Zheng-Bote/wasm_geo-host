mod common;

use flagme_plugin_reversegeo::gazetteer::{Gazetteer, FORMAT_VERSION, PLACES};
use flagme_plugin_reversegeo::ReverseGeoPlugin;
use flagme_sdk::{FileInfo, LocationInfo, Plugin, PluginContext, UnifiedDocument};
use std::collections::HashMap;
use std::sync::Arc;

/// reversegeo offline (no online lookup, no cache).
fn resolve(lat: f64, lon: f64) -> LocationInfo {
    let mut d = UnifiedDocument::new(
        "r",
        FileInfo {
            path: "/photos/r.jpg".to_string(),
            filename: "r.jpg".to_string(),
            size_bytes: 0,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    d.location = Some(LocationInfo { latitude: Some(lat), longitude: Some(lon), ..LocationInfo::default() });
    let settings: HashMap<String, String> =
        [("online_lookup", "off"), ("offline_cache", "off")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let ctx = PluginContext::with_settings(Arc::new(common::LogCaps::default()), settings);
    ReverseGeoPlugin::new().process(d, &ctx).unwrap().location.unwrap()
}

fn regions(l: &LocationInfo) -> (Option<&str>, Option<&str>) {
    (l.region.as_deref(), l.local_region.as_deref())
}

// short-region-names AC-1: offline, Salzburg, Vienna, Geneva, Basel, Hamburg, Paris and Prague get the short region names (English / local)
#[test]
fn test_ac1_short_region_names() {
    for (lat, lon, en, local) in [
        (47.8095, 13.0550, "Salzburg", "Salzburg"),
        (48.2082, 16.3738, "Vienna", "Wien"),
        (46.2044, 6.1432, "Geneva", "Genf"),
        (47.5596, 7.5886, "Basel-City", "Basel-Stadt"),
        (53.5511, 9.9937, "Hamburg", "Hamburg"),
        (48.8566, 2.3522, "Île-de-France", "Île-de-France"),
        (50.0875, 14.4213, "Prague", "Praha"),
    ] {
        assert_eq!(regions(&resolve(lat, lon)), (Some(en), Some(local)), "{} {}", lat, lon);
    }
}

// short-region-names AC-2: without a matching short form the base name stays — Berlin State of Berlin / Berlin, Mainz Rhineland-Palatinate / Rheinland-Pfalz, Bavaria / Bayern
#[test]
fn test_ac2_base_name_without_matching_short_form() {
    assert_eq!(regions(&resolve(52.5200, 13.4050)), (Some("State of Berlin"), Some("Berlin")));
    assert_eq!(regions(&resolve(49.9929, 8.2473)), (Some("Rhineland-Palatinate"), Some("Rheinland-Pfalz")));
    assert_eq!(regions(&resolve(48.4706, 11.9381)), (Some("Bavaria"), Some("Bayern")));
}

// short-region-names AC-3: counties are unchanged — Brighton has Brighton and Hove, Munich Upper Bavaria
#[test]
fn test_ac3_counties_unchanged() {
    assert_eq!(resolve(50.8225, -0.1372).county.as_deref(), Some("Brighton and Hove"));
    assert_eq!(resolve(48.1374, 11.5755).county.as_deref(), Some("Upper Bavaria"));
}

// short-region-names AC-5: places.bin is regenerated with format version 1 and the data date 2026-11-27 and stays within 8 MiB
#[test]
fn test_ac5_regenerated_data() {
    assert_eq!(u16::from_le_bytes([PLACES[4], PLACES[5]]), FORMAT_VERSION);
    assert_eq!(FORMAT_VERSION, 1);
    let meta = Gazetteer::parse(PLACES).unwrap().meta().unwrap().to_string();
    assert!(meta.contains("dumps of 2026-11-27"), "{}", meta);
    assert!(PLACES.len() <= 8 * 1024 * 1024, "{}", PLACES.len());
}

// short-region-names AC-6: the reversegeo README describes the rule with examples and names Berlin as a case without an English short form; the CHANGELOG names the change
#[test]
fn test_ac6_documentation() {
    let root = common::repo_root();
    let readme = std::fs::read_to_string(root.join("plugins/reversegeo/README.md")).unwrap();
    for needle in ["ADR 0008", "State of Salzburg", "Salzburg", "State of Berlin", "whole word"] {
        assert!(readme.contains(needle), "README lacks {}", needle);
    }
    let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    assert!(changelog.contains("ADR 0008") && changelog.contains("#67"), "CHANGELOG lacks the change");
}
