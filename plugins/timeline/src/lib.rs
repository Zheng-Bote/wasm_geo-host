use chrono::{DateTime, Utc};
use flagme_sdk::{
    Plugin, PluginContext, PluginResult, TimelineInfo, UnifiedDocument,
};

/// Timeline normalization plugin with deterministic timestamp conflict resolution.
pub struct TimelinePlugin;

impl TimelinePlugin {
    pub fn new() -> Self {
        Self
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
            if let Some(raw_date) = metadata.raw_tags.get("Exif.Photo.DateTimeOriginal") {
                if let Ok(parsed) = DateTime::parse_from_rfc3339(raw_date) {
                    taken_at = Some(parsed.with_timezone(&Utc));
                    resolution_source = "exif_datetime_original".to_string();
                }
            } else if let Some(raw_date) = metadata.raw_tags.get("XMP.xmp.CreateDate") {
                // 2. Priority: XMP create date
                if let Ok(parsed) = DateTime::parse_from_rfc3339(raw_date) {
                    taken_at = Some(parsed.with_timezone(&Utc));
                    resolution_source = "xmp_create_date".to_string();
                }
            }
        }

        // If no EXIF/XMP timestamp, fallback to imported time
        if taken_at.is_none() {
            taken_at = Some(now);
            resolution_source = "imported_timestamp_fallback".to_string();
        }

        // Filesystem modification timestamp if available
        if let Ok(meta) = std::fs::metadata(&doc.file.path) {
            if let Ok(modified) = meta.modified() {
                let dt: DateTime<Utc> = modified.into();
                modified_at = Some(dt);
            }
        }

        ctx.capabilities.log(
            "info",
            &format!("Timeline resolved via '{}'", resolution_source),
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
