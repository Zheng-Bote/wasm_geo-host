use chrono::{DateTime, NaiveDateTime, Utc};
use flagme_sdk::{
    Plugin, PluginContext, PluginResult, TimelineInfo, UnifiedDocument,
};
use std::fs;
use std::path::Path;

/// Timeline normalization plugin with deterministic timestamp conflict resolution.
pub struct TimelinePlugin;

impl TimelinePlugin {
    pub fn new() -> Self {
        Self
    }

    fn parse_exif_date(s: &str) -> Option<DateTime<Utc>> {
        let clean = s.trim().trim_matches('"');
        // 1. Try standard EXIF format: "YYYY:MM:DD HH:MM:SS"
        if let Ok(naive) = NaiveDateTime::parse_from_str(clean, "%Y:%m:%d %H:%M:%S") {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
        }
        // 2. Try RFC 3339 / ISO 8601: "YYYY-MM-DDTHH:MM:SSZ"
        if let Ok(parsed) = DateTime::parse_from_rfc3339(clean) {
            return Some(parsed.with_timezone(&Utc));
        }
        // 3. Try filename timestamp pattern: "YYYY-MM-DD_HHMMSS"
        if clean.len() >= 17 {
            if let Ok(naive) = NaiveDateTime::parse_from_str(&clean[0..17], "%Y-%m-%d_%H%M%S") {
                return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
            }
        }
        None
    }

    fn parse_filename_date(filename: &str) -> Option<DateTime<Utc>> {
        // e.g. "2025-10-25_094918.jpg"
        let stem = Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(filename);

        if stem.len() >= 17 {
            let candidate = &stem[0..17];
            if let Ok(naive) = NaiveDateTime::parse_from_str(candidate, "%Y-%m-%d_%H%M%S") {
                return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
            }
        }
        None
    }
}

impl Plugin for TimelinePlugin {
    fn name(&self) -> &'static str {
        "timeline"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        let now = Utc::now();
        let mut taken_at = None;
        let mut modified_at = None;
        let mut resolution_source = "fallback_now".to_string();

        // 1. Priority: EXIF original timestamp
        if let Some(metadata) = &doc.metadata {
            if let Some(raw_date) = metadata.raw_tags.get("Exif.Photo.DateTimeOriginal").or_else(|| metadata.raw_tags.get("Exif.DateTimeOriginal")) {
                if let Some(parsed) = Self::parse_exif_date(raw_date) {
                    taken_at = Some(parsed);
                    resolution_source = "exif_datetime_original".to_string();
                }
            } else if let Some(raw_date) = metadata.raw_tags.get("XMP.xmp.CreateDate") {
                // 2. Priority: XMP create date
                if let Some(parsed) = Self::parse_exif_date(raw_date) {
                    taken_at = Some(parsed);
                    resolution_source = "xmp_create_date".to_string();
                }
            }
        }

        // 2. Priority: Parse timestamp encoded in filename (e.g. 2025-10-25_094918.jpg)
        if taken_at.is_none() {
            if let Some(parsed) = Self::parse_filename_date(&doc.file.filename) {
                taken_at = Some(parsed);
                resolution_source = "filename_timestamp".to_string();
            }
        }

        // 3. Priority: Filesystem modified time
        if let Ok(meta) = fs::metadata(&doc.file.path) {
            if let Ok(mod_time) = meta.modified() {
                let dt: DateTime<Utc> = mod_time.into();
                modified_at = Some(dt);
                if taken_at.is_none() {
                    taken_at = Some(dt);
                    resolution_source = "filesystem_modified_time".to_string();
                }
            }
        }

        // 4. Fallback: current time
        if taken_at.is_none() {
            taken_at = Some(now);
            resolution_source = "imported_timestamp_fallback".to_string();
        }

        ctx.capabilities.log(
            "info",
            &format!("Timeline resolved via '{}': {:?}", resolution_source, taken_at),
        );

        doc.timeline = Some(TimelineInfo {
            taken_at,
            imported_at: now,
            modified_at,
            resolution_source,
        });

        Ok(doc)
    }
}
