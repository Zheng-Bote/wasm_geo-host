use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
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

    pub fn parse_exif_date(s: &str) -> Option<DateTime<Utc>> {
        let clean = s.trim().trim_matches('"').trim();
        // 1. Standard EXIF format: "YYYY:MM:DD HH:MM:SS"
        if let Ok(naive) = NaiveDateTime::parse_from_str(clean, "%Y:%m:%d %H:%M:%S") {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
        }
        // 2. Standard ISO without T: "YYYY-MM-DD HH:MM:SS"
        if let Ok(naive) = NaiveDateTime::parse_from_str(clean, "%Y-%m-%d %H:%M:%S") {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
        }
        // 3. RFC 3339 / ISO 8601: "YYYY-MM-DDTHH:MM:SSZ"
        if let Ok(parsed) = DateTime::parse_from_rfc3339(clean) {
            return Some(parsed.with_timezone(&Utc));
        }
        // 4. Filename timestamp pattern: "YYYY-MM-DD_HHMMSS"
        if clean.len() >= 17 {
            if let Ok(naive) = NaiveDateTime::parse_from_str(&clean[0..17], "%Y-%m-%d_%H%M%S") {
                return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
            }
        }
        None
    }

    pub fn parse_filename_date(filename: &str) -> Option<DateTime<Utc>> {
        let stem = Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(filename);

        // 1. Pattern: "YYYY-MM-DD_HHMMSS..." (e.g. "2025-10-25_094918.jpg")
        if stem.len() >= 17 {
            let candidate = &stem[0..17];
            if let Ok(naive) = NaiveDateTime::parse_from_str(candidate, "%Y-%m-%d_%H%M%S") {
                return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
            }
        }

        // 2. Pattern: "YYYY-MM-DD..." (e.g. "2026-03-04_Stromzaehler.jpg")
        if stem.len() >= 10 {
            let date_part = &stem[0..10];
            if let Ok(date) = chrono::NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
                if let Some(naive) = date.and_hms_opt(12, 0, 0) {
                    return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
                }
            }
        }

        // 3. Pattern containing 13-digit Unix epoch milliseconds (e.g. "app_inventor_1554303518406.jpg")
        for part in stem.split(|c: char| !c.is_ascii_digit()) {
            if part.len() == 13 {
                if let Ok(ms) = part.parse::<i64>() {
                    if let chrono::LocalResult::Single(dt) = Utc.timestamp_millis_opt(ms) {
                        return Some(dt);
                    }
                }
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

        // 1. Priority: EXIF original / digitized / image timestamp
        if let Some(metadata) = &doc.metadata {
            let exif_candidates = [
                ("Exif.Photo.DateTimeOriginal", "exif_datetime_original"),
                ("Exif.DateTimeOriginal", "exif_datetime_original"),
                ("Exif.Date/Time Original", "exif_datetime_original"),
                ("Exif.Photo.DateTimeDigitized", "exif_datetime_digitized"),
                ("Exif.DateTimeDigitized", "exif_datetime_digitized"),
                ("Exif.Image.DateTime", "exif_datetime"),
                ("Exif.DateTime", "exif_datetime"),
                ("XMP.xmp.CreateDate", "xmp_create_date"),
            ];

            for (key, source_label) in exif_candidates {
                if let Some(raw_date) = metadata.raw_tags.get(key) {
                    if let Some(parsed) = Self::parse_exif_date(raw_date) {
                        taken_at = Some(parsed);
                        resolution_source = source_label.to_string();
                        break;
                    }
                }
            }
        }

        // 2. Priority: Parse timestamp encoded in filename
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
