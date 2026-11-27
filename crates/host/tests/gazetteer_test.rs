mod common;

use flagme_host::{AppConfig, PluginRuntime};
use flagme_plugin_reversegeo::gazetteer::{Gazetteer, PLACES};
use flagme_plugin_reversegeo::ReverseGeoPlugin;
use flagme_sdk::{FileInfo, LocationInfo, Plugin, PluginContext, UnifiedDocument};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn located(lat: f64, lon: f64) -> UnifiedDocument {
    let mut d = UnifiedDocument::new(
        "geo",
        FileInfo {
            path: "/photos/geo.jpg".to_string(),
            filename: "geo.jpg".to_string(),
            size_bytes: 0,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    d.location = Some(LocationInfo { latitude: Some(lat), longitude: Some(lon), ..LocationInfo::default() });
    d
}

/// Runs reversegeo offline (no online lookup, no cache) with extra settings.
fn resolve_with(lat: f64, lon: f64, extra: &[(&str, &str)]) -> (LocationInfo, Arc<common::LogCaps>) {
    let caps = Arc::new(common::LogCaps::default());
    let mut settings: HashMap<String, String> =
        [("online_lookup", "off"), ("offline_cache", "off")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    for (k, v) in extra {
        settings.insert(k.to_string(), v.to_string());
    }
    let ctx = PluginContext::with_settings(caps.clone(), settings);
    let out = ReverseGeoPlugin::new().process(located(lat, lon), &ctx).unwrap();
    (out.location.unwrap(), caps)
}

fn resolve(lat: f64, lon: f64) -> LocationInfo {
    resolve_with(lat, lon, &[]).0
}

fn names(l: &LocationInfo) -> (Option<&str>, Option<&str>, Option<&str>) {
    (l.city.as_deref(), l.region.as_deref(), l.country_code.as_deref())
}

// geonames-gazetteer AC-1: offline, photos in Freising, Moosburg an der Isar and Garmisch-Partenkirchen resolve to that town, Bavaria/Bayern, Germany/Deutschland, DE, de
#[test]
fn test_ac1_small_towns_offline() {
    for (lat, lon, town) in [
        (48.4029, 11.7489, "Freising"),
        (48.4706, 11.9381, "Moosburg"),
        (48.473, 11.933, "Moosburg"), // the user's photos 46 km from Munich
        (47.4917, 11.0955, "Garmisch-Partenkirchen"),
    ] {
        let l = resolve(lat, lon);
        assert_eq!(names(&l), (Some(town), Some("Bavaria"), Some("DE")), "{} {}", lat, lon);
        assert_eq!(l.local_city.as_deref(), Some(town));
        assert_eq!(l.local_region.as_deref(), Some("Bayern"));
        assert_eq!((l.country.as_deref(), l.local_country.as_deref()), (Some("Germany"), Some("Deutschland")));
        assert_eq!(l.local_language.as_deref(), Some("de"));
        assert_eq!((l.latitude, l.longitude), (Some(lat), Some(lon)), "coordinates unchanged");
    }
}

// geonames-gazetteer AC-2: offline, points in Salzburg, Prague, Strasbourg, Szczecin and Basel get a place in the right country; Prague is Prague/Praha
#[test]
fn test_ac2_border_points_offline() {
    for (lat, lon, city, cc) in [
        (47.8095, 13.0550, "Salzburg", "AT"),
        (48.5734, 7.7521, "Strasbourg", "FR"),
        (53.4285, 14.5528, "Szczecin", "PL"),
        (47.5596, 7.5886, "Basel", "CH"),
    ] {
        let l = resolve(lat, lon);
        assert_eq!((l.city.as_deref(), l.country_code.as_deref()), (Some(city), Some(cc)), "{} {}", lat, lon);
    }
    // Old Town Square: the city itself, not a district.
    let prague = resolve(50.0875, 14.4213);
    assert_eq!((prague.city.as_deref(), prague.local_city.as_deref()), (Some("Prague"), Some("Praha")));
    assert_eq!((prague.country.as_deref(), prague.local_country.as_deref()), (Some("Czechia"), Some("Česko")));
}

// geonames-gazetteer AC-3: offline, Chicago, Palermo and Bordeaux get their own place and region, not the capital of the country
#[test]
fn test_ac3_no_capital_fallback() {
    assert_eq!(names(&resolve(41.8781, -87.6298)), (Some("Chicago"), Some("Illinois"), Some("US")));
    assert_eq!(names(&resolve(38.1157, 13.3615)), (Some("Palermo"), Some("Sicily"), Some("IT")));
    let bordeaux = resolve(44.8378, -0.5792);
    assert_eq!((bordeaux.city.as_deref(), bordeaux.country_code.as_deref()), (Some("Bordeaux"), Some("FR")));
    assert_ne!(bordeaux.region.as_deref(), Some("Île-de-France"));
}

// geonames-gazetteer AC-4: with no place within city_radius_km the names stay empty offline (coordinates kept, warning, no cache); with online_lookup on the online geocoder is asked instead
#[test]
fn test_ac4_no_place_within_radius() {
    let caps = Arc::new(common::LogCaps::default());
    let ctx = PluginContext::with_settings(caps.clone(), [("online_lookup".to_string(), "off".to_string())].into());
    let out = ReverseGeoPlugin::new().process(located(40.0, -40.0), &ctx).unwrap();
    let l = out.location.unwrap();
    assert_eq!((l.latitude, l.longitude), (Some(40.0), Some(-40.0)));
    assert!(l.city.is_none() && l.region.is_none() && l.country.is_none() && l.local_language.is_none(), "{:?}", l);
    let logs = caps.logs.lock().unwrap().clone();
    assert!(
        logs.iter().any(|(lvl, m)| lvl == "warn" && m.contains("no place within city_radius_km (25 km)") && m.contains("online_lookup is off")),
        "{:?}",
        logs
    );
    assert!(caps.kv.lock().unwrap().is_empty(), "nothing cached");

    let caps = Arc::new(common::LogCaps::default());
    let ctx = PluginContext::new(caps.clone());
    let out = ReverseGeoPlugin::new().process(located(40.0, -40.0), &ctx).unwrap();
    assert!(out.location.unwrap().city.is_none());
    let urls = caps.http_urls.lock().unwrap().clone();
    assert_eq!(urls.len(), 1, "online geocoder asked once");
    assert!(urls[0].contains("lat=40") && urls[0].contains("lon=-40"), "{:?}", urls);
}

// geonames-gazetteer AC-5: city_radius_km is the maximum distance to the nearest place (default 25, range 1-200); a point 30 km away resolves only with a larger radius
#[test]
fn test_ac5_city_radius_km() {
    // North Sea, 30.7 km from Langeoog.
    let (lat, lon) = (54.0, 7.3);
    let g = Gazetteer::parse(PLACES).unwrap();
    let (_, km) = g.nearest(lat, lon, 200.0).unwrap();
    assert!((26.0..50.0).contains(&km), "fixture distance changed: {}", km);

    assert!(resolve(lat, lon).city.is_none(), "default 25 km must not reach");
    let (l, _) = resolve_with(lat, lon, &[("city_radius_km", "50")]);
    assert_eq!((l.city.as_deref(), l.country_code.as_deref()), (Some("Langeoog"), Some("DE")));

    let r = flagme_plugin_reversegeo::ReverseGeoSettings::default();
    assert_eq!(r.city_radius_km, 25);
    // Out of range: clamped to 200 with a settings warning.
    let (_, caps) = resolve_with(lat, lon, &[("city_radius_km", "2000")]);
    let logs = caps.logs.lock().unwrap().clone();
    assert!(logs.iter().any(|(lvl, m)| lvl == "warn" && m.contains("city_radius_km")), "{:?}", logs);
}

// geonames-gazetteer AC-6: the plugin has no built-in places or countries any more; every offline match comes from places.bin
#[test]
fn test_ac6_no_builtin_places() {
    let src = std::fs::read_to_string(common::repo_root().join("plugins/reversegeo/src/lib.rs")).unwrap();
    for needle in ["CITIES", "COUNTRY_BOXES", "CityRef", "CountryBox", "\"Munich\"", "\"Berlin\"", "\"San Francisco\"", "\"Paris\"", "\"Rome\"", "\"Tokyo\""] {
        assert!(!src.contains(needle), "reversegeo still contains {}", needle);
    }
    let gaz = std::fs::read_to_string(common::repo_root().join("plugins/reversegeo/src/gazetteer.rs")).unwrap();
    assert!(gaz.contains("include_bytes!(\"../data/places.bin\")"));
}

// geonames-gazetteer AC-7: names come from the data per ADR 0006 — Munich/München, Moosburg/Moosburg, Geneva/Genf (first country language), Czechia/Česko
#[test]
fn test_ac7_names_from_data() {
    let munich = resolve(48.1374, 11.5755);
    assert_eq!((munich.city.as_deref(), munich.local_city.as_deref()), (Some("Munich"), Some("München")));
    let moosburg = resolve(48.4706, 11.9381);
    assert_eq!((moosburg.city.as_deref(), moosburg.local_city.as_deref()), (Some("Moosburg"), Some("Moosburg")));
    let geneva = resolve(46.2044, 6.1432);
    assert_eq!(
        (geneva.city.as_deref(), geneva.local_city.as_deref(), geneva.local_language.as_deref()),
        (Some("Geneva"), Some("Genf"), Some("de"))
    );
}

/// `YYYY-MM-DD` of the embedded data, from the header text.
fn data_date() -> String {
    let meta = Gazetteer::parse(PLACES).unwrap().meta().unwrap().to_string();
    let i = meta.find("dumps of ").expect("date in meta") + "dumps of ".len();
    meta[i..i + 10].to_string()
}

// geonames-gazetteer AC-9: NOTICE, the reversegeo README and the CHANGELOG name GeoNames, CC BY 4.0 and the data date; "Natural Earth" appears neither in NOTICE nor in a log message
#[test]
fn test_ac9_attribution() {
    let root = common::repo_root();
    let date = data_date();
    for file in ["NOTICE", "plugins/reversegeo/README.md", "CHANGELOG.md"] {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        assert!(text.contains("GeoNames") && text.contains("CC BY 4.0"), "{} lacks the attribution", file);
        assert!(text.contains(&date), "{} lacks the data date {}", file, date);
    }
    assert!(!std::fs::read_to_string(root.join("NOTICE")).unwrap().contains("Natural Earth"));
    for file in ["plugins/reversegeo/src/lib.rs", "plugins/reversegeo/src/gazetteer.rs"] {
        assert!(!std::fs::read_to_string(root.join(file)).unwrap().contains("Natural Earth"), "{}", file);
    }
}

// geonames-gazetteer AC-10: places.bin is at most 8 MB, a lookup in the guest code takes at most 5 ms and a reversegeo call through the host at most 20 ms including instantiation
#[test]
fn test_ac10_size_and_time() {
    assert!(PLACES.len() <= 8 * 1024 * 1024, "places.bin is {} bytes", PLACES.len());

    let g = Gazetteer::parse(PLACES).unwrap();
    let points = [(48.4706, 11.9381), (50.0875, 14.4213), (41.8781, -87.6298), (35.6762, 139.6503), (54.0, 7.3)];
    let start = Instant::now();
    for _ in 0..20 {
        for (lat, lon) in points {
            std::hint::black_box(g.nearest(lat, lon, 25.0));
        }
    }
    let per_lookup = start.elapsed() / (20 * points.len() as u32);
    assert!(per_lookup <= Duration::from_millis(5), "lookup takes {:?}", per_lookup);

    let mut config = common::with_components(AppConfig::default());
    config.component_cache = false;
    config.pipeline = vec!["reversegeo".to_string()];
    let cfg = config.plugins.get_mut("reversegeo").unwrap();
    cfg.capabilities = vec!["kv".to_string()];
    cfg.settings = [("online_lookup".to_string(), "off".to_string()), ("offline_cache".to_string(), "off".to_string())].into();
    let rt = PluginRuntime::new(config).unwrap();
    let mut times = Vec::new();
    for (lat, lon) in points.iter().cycle().take(25) {
        let start = Instant::now();
        let out = rt.execute_plugin("reversegeo", located(*lat, *lon)).unwrap();
        times.push(start.elapsed());
        std::hint::black_box(out);
    }
    times.sort();
    let median = times[times.len() / 2];
    eprintln!("reversegeo via host: median {:?}, max {:?}; lookup {:?}", median, times.last().unwrap(), per_lookup);
    assert!(median <= Duration::from_millis(20), "reversegeo call takes {:?}", median);
    let moosburg = rt.execute_plugin("reversegeo", located(48.4706, 11.9381)).unwrap();
    assert_eq!(moosburg.location.unwrap().city.as_deref(), Some("Moosburg"), "component uses the same data");
}
