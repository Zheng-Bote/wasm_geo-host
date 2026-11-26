mod common;

use chrono::{TimeZone, Utc};
use common::LogCaps;
use flagme_host::MediaOrganizer;
use flagme_plugin_timeline::{TimelinePlugin, RESOLUTION_NONE};
use flagme_plugin_weather_enrich::WeatherEnrichPlugin;
use flagme_sdk::{FileInfo, LocationInfo, Plugin, PluginContext, TimelineInfo, UnifiedDocument};
use std::sync::Arc;

/// Document without EXIF/XMP, without a date in its file name and without a readable file.
fn undated_doc() -> UnifiedDocument {
    let mut doc = UnifiedDocument::new(
        "undated",
        FileInfo {
            path: "/nonexistent/holiday.jpg".to_string(),
            filename: "holiday.jpg".to_string(),
            size_bytes: 0,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    doc.location = Some(LocationInfo {
        latitude: Some(48.14),
        longitude: Some(11.58),
        country: Some("Germany".to_string()),
        city: Some("Munich".to_string()),
        ..Default::default()
    });
    doc
}

// no-invented-capture-time AC-1: without any time source taken_at stays None, resolution_source is "none", imported_at is set and a warning is logged
#[test]
fn test_ac1_no_source_no_capture_time() {
    let caps = Arc::new(LogCaps::default());
    let before = Utc::now();
    let out = TimelinePlugin::new().process(undated_doc(), &PluginContext::new(caps.clone())).unwrap();
    let t = out.timeline.expect("timeline info is still written");
    assert_eq!(t.taken_at, None);
    assert_eq!(t.resolution_source, RESOLUTION_NONE);
    assert!(t.imported_at >= before, "imported_at holds the import time");
    assert!(t.modified_at.is_none());
    assert!(
        caps.warnings().iter().any(|w| w.starts_with("timeline: no capture time found for '/nonexistent/holiday.jpg'")),
        "{:?}",
        caps.warnings()
    );
}

// no-invented-capture-time AC-2: weather-enrich sends no request for a document without capture time
#[test]
fn test_ac2_no_weather_query_without_capture_time() {
    let caps = Arc::new(LogCaps { http_body: Some("{}".to_string()), ..Default::default() });
    let ctx = PluginContext::new(caps.clone());
    let doc = TimelinePlugin::new().process(undated_doc(), &ctx).unwrap();
    let doc = WeatherEnrichPlugin::new().process(doc, &ctx).unwrap();
    assert!(doc.weather.is_none());
    assert!(caps.http_urls.lock().unwrap().is_empty(), "weather requested for an invented time");
}

// no-invented-capture-time AC-3: the watcher puts photos without taken_at into Unknown_Year/00, not into the import month
#[test]
fn test_ac3_watcher_unknown_date_folder() {
    let mut doc = undated_doc();
    doc.timeline = Some(TimelineInfo {
        taken_at: None,
        imported_at: Utc.with_ymd_and_hms(2026, 11, 26, 10, 0, 0).unwrap(),
        modified_at: None,
        resolution_source: RESOLUTION_NONE.to_string(),
    });
    let path = MediaOrganizer::format_destination_path(&doc, "{year}/{month}/{country}/{city}");
    assert_eq!(path.to_string_lossy(), "Unknown_Year/00/Germany/Munich/holiday.jpg");

    // With a capture time, that time decides (not the import time).
    doc.timeline.as_mut().unwrap().taken_at = Some(Utc.with_ymd_and_hms(2019, 5, 3, 8, 0, 0).unwrap());
    let path = MediaOrganizer::format_destination_path(&doc, "{year}/{month}/{country}/{city}");
    assert_eq!(path.to_string_lossy(), "2019/05/Germany/Munich/holiday.jpg");
}
