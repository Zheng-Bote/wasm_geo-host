use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use flagme_sdk::{
    Plugin, PluginContext, PluginResult, PluginSettings, SettingsCache, SettingsReader,
    TimelineInfo, UnifiedDocument,
};
use std::path::Path;

/// Time zone used for timestamps without an offset (EXIF, file names).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeZoneSetting {
    Utc,
    Fixed(FixedOffset),
    Iana(chrono_tz::Tz),
}

impl TimeZoneSetting {
    /// Parses `UTC`, an IANA name (`Europe/Berlin`) or an offset (`+02:00`, `-0530`).
    pub fn parse(value: &str) -> Option<Self> {
        let v = value.trim();
        if v.eq_ignore_ascii_case("utc") || v.eq_ignore_ascii_case("z") {
            return Some(Self::Utc);
        }
        if v.starts_with('+') || v.starts_with('-') {
            return parse_offset(v).map(Self::Fixed);
        }
        v.parse::<chrono_tz::Tz>().ok().map(Self::Iana)
    }

    /// Interprets a naive local time in this zone. Ambiguous times (DST fall-back) resolve
    /// to the earlier instant; non-existent times (DST spring-forward) yield `None`.
    pub fn to_utc(&self, naive: NaiveDateTime) -> Option<DateTime<Utc>> {
        match self {
            Self::Utc => Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc)),
            Self::Fixed(off) => off.from_local_datetime(&naive).earliest().map(|d| d.with_timezone(&Utc)),
            Self::Iana(tz) => tz.from_local_datetime(&naive).earliest().map(|d| d.with_timezone(&Utc)),
        }
    }
}

fn parse_offset(v: &str) -> Option<FixedOffset> {
    let sign = if v.starts_with('-') { -1 } else { 1 };
    let digits: String = v[1..].chars().filter(|c| c.is_ascii_digit()).collect();
    let (h, m) = match digits.len() {
        1 | 2 => (digits.parse::<i32>().ok()?, 0),
        4 => (digits[0..2].parse::<i32>().ok()?, digits[2..4].parse::<i32>().ok()?),
        _ => return None,
    };
    if h > 14 || m > 59 {
        return None;
    }
    FixedOffset::east_opt(sign * (h * 3600 + m * 60))
}

/// Sources for `taken_at`, in the order of the default priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeSource {
    ExifDateTimeOriginal,
    ExifDateTimeDigitized,
    ExifDateTime,
    XmpCreateDate,
    FilenameTimestamp,
    FilesystemModifiedTime,
}

impl TimeSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::ExifDateTimeOriginal => "exif_datetime_original",
            Self::ExifDateTimeDigitized => "exif_datetime_digitized",
            Self::ExifDateTime => "exif_datetime",
            Self::XmpCreateDate => "xmp_create_date",
            Self::FilenameTimestamp => "filename_timestamp",
            Self::FilesystemModifiedTime => "filesystem_modified_time",
        }
    }

    /// `raw_tags` keys read for metadata sources.
    fn raw_tag_keys(self) -> &'static [&'static str] {
        match self {
            Self::ExifDateTimeOriginal => &[
                "Exif.Photo.DateTimeOriginal",
                "Exif.DateTimeOriginal",
                "Exif.Date/Time Original",
            ],
            Self::ExifDateTimeDigitized => &["Exif.Photo.DateTimeDigitized", "Exif.DateTimeDigitized"],
            Self::ExifDateTime => &["Exif.Image.DateTime", "Exif.DateTime"],
            Self::XmpCreateDate => &["XMP.xmp.CreateDate"],
            Self::FilenameTimestamp | Self::FilesystemModifiedTime => &[],
        }
    }
}

pub const DEFAULT_SOURCE_PRIORITY: &[TimeSource] = &[
    TimeSource::ExifDateTimeOriginal,
    TimeSource::ExifDateTimeDigitized,
    TimeSource::ExifDateTime,
    TimeSource::XmpCreateDate,
    TimeSource::FilenameTimestamp,
    TimeSource::FilesystemModifiedTime,
];

const SOURCE_OPTIONS: &[(&[&str], TimeSource)] = &[
    (&["exif_datetime_original"], TimeSource::ExifDateTimeOriginal),
    (&["exif_datetime_digitized"], TimeSource::ExifDateTimeDigitized),
    (&["exif_datetime"], TimeSource::ExifDateTime),
    (&["xmp_create_date"], TimeSource::XmpCreateDate),
    (&["filename_timestamp"], TimeSource::FilenameTimestamp),
    (&["filesystem_modified_time"], TimeSource::FilesystemModifiedTime),
];

/// `resolution_source` when no source yields a capture time (ADR 0002: no invented time).
pub const RESOLUTION_NONE: &str = "none";

/// Configurable parameters of the timeline plugin (`config.yaml` `settings`).
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineSettings {
    /// Zone for timestamps without offset.
    pub timezone: TimeZoneSetting,
    /// Sources tried in this order; if none matches, `taken_at` stays `None` ([`RESOLUTION_NONE`]).
    pub source_priority: Vec<TimeSource>,
}

impl Default for TimelineSettings {
    fn default() -> Self {
        Self {
            timezone: TimeZoneSetting::Utc,
            source_priority: DEFAULT_SOURCE_PRIORITY.to_vec(),
        }
    }
}

impl PluginSettings for TimelineSettings {
    const PLUGIN: &'static str = "timeline";

    fn from_reader(base: &Self, r: &mut SettingsReader<'_>) -> Self {
        Self {
            timezone: r.parse_with(
                "timezone",
                &[],
                "UTC, IANA name (e.g. Europe/Berlin) or offset (e.g. +02:00)",
                base.timezone,
                "UTC",
                TimeZoneSetting::parse,
            ),
            source_priority: r.list_choice("source_priority", SOURCE_OPTIONS, &base.source_priority),
        }
    }
}

/// Timeline normalization plugin with deterministic timestamp conflict resolution.
pub struct TimelinePlugin {
    settings: SettingsCache<TimelineSettings>,
}

impl TimelinePlugin {
    pub fn new() -> Self {
        Self::with_settings(TimelineSettings::default())
    }

    pub fn with_settings(settings: TimelineSettings) -> Self {
        Self {
            settings: SettingsCache::new(settings),
        }
    }

    /// Parses a metadata timestamp; times without offset are interpreted as UTC.
    pub fn parse_exif_date(s: &str) -> Option<DateTime<Utc>> {
        Self::parse_exif_date_in(s, &TimeZoneSetting::Utc)
    }

    /// Parses a metadata timestamp; times without offset are interpreted in `tz`.
    pub fn parse_exif_date_in(s: &str, tz: &TimeZoneSetting) -> Option<DateTime<Utc>> {
        let clean = s.trim().trim_matches('"').trim();
        // 1. Standard EXIF format: "YYYY:MM:DD HH:MM:SS"
        if let Ok(naive) = NaiveDateTime::parse_from_str(clean, "%Y:%m:%d %H:%M:%S") {
            return tz.to_utc(naive);
        }
        // 2. Standard ISO without T: "YYYY-MM-DD HH:MM:SS"
        if let Ok(naive) = NaiveDateTime::parse_from_str(clean, "%Y-%m-%d %H:%M:%S") {
            return tz.to_utc(naive);
        }
        // 3. RFC 3339 / ISO 8601 with its own offset: "YYYY-MM-DDTHH:MM:SSZ"
        if let Ok(parsed) = DateTime::parse_from_rfc3339(clean) {
            return Some(parsed.with_timezone(&Utc));
        }
        // 4. Filename timestamp pattern: "YYYY-MM-DD_HHMMSS"
        if clean.len() >= 17 {
            if let Ok(naive) = NaiveDateTime::parse_from_str(&clean[0..17], "%Y-%m-%d_%H%M%S") {
                return tz.to_utc(naive);
            }
        }
        None
    }

    /// Parses a timestamp from a file name; local patterns are interpreted as UTC.
    pub fn parse_filename_date(filename: &str) -> Option<DateTime<Utc>> {
        Self::parse_filename_date_in(filename, &TimeZoneSetting::Utc)
    }

    /// Parses a timestamp from a file name; local patterns are interpreted in `tz`.
    pub fn parse_filename_date_in(filename: &str, tz: &TimeZoneSetting) -> Option<DateTime<Utc>> {
        let stem = Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(filename);

        // 1. Pattern: "YYYY-MM-DD_HHMMSS..." (e.g. "2025-10-25_094918.jpg")
        if stem.len() >= 17 {
            if let Some(candidate) = stem.get(0..17) {
                if let Ok(naive) = NaiveDateTime::parse_from_str(candidate, "%Y-%m-%d_%H%M%S") {
                    return tz.to_utc(naive);
                }
            }
        }

        // 2. Pattern: "YYYY-MM-DD..." (e.g. "2026-03-04_Stromzaehler.jpg"), noon local time
        if stem.len() >= 10 {
            if let Some(date_part) = stem.get(0..10) {
                if let Ok(date) = chrono::NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
                    if let Some(naive) = date.and_hms_opt(12, 0, 0) {
                        return tz.to_utc(naive);
                    }
                }
            }
        }

        // 3. Pattern containing 13-digit Unix epoch milliseconds (absolute, no zone needed)
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
        env!("CARGO_PKG_VERSION")
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        let settings = self.settings.get_or_parse(ctx);
        let now = Utc::now();

        // Only via the `filesystem` capability; without a grant there is no modification time.
        let modified_at: Option<DateTime<Utc>> = ctx
            .capabilities
            .fs_modified(&doc.file.path)
            .ok()
            .and_then(|secs| i64::try_from(secs).ok())
            .and_then(|secs| Utc.timestamp_opt(secs, 0).single());

        let mut taken_at = None;
        let mut resolution_source = RESOLUTION_NONE.to_string();

        for source in &settings.source_priority {
            let found = match source {
                TimeSource::FilenameTimestamp => {
                    Self::parse_filename_date_in(&doc.file.filename, &settings.timezone)
                }
                TimeSource::FilesystemModifiedTime => modified_at,
                metadata_source => doc.metadata.as_ref().and_then(|metadata| {
                    metadata_source.raw_tag_keys().iter().find_map(|key| {
                        metadata
                            .raw_tags
                            .get(*key)
                            .and_then(|raw| Self::parse_exif_date_in(raw, &settings.timezone))
                    })
                }),
            };
            if let Some(dt) = found {
                taken_at = Some(dt);
                resolution_source = source.label().to_string();
                break;
            }
        }

        if taken_at.is_none() {
            let sources: Vec<&str> = settings.source_priority.iter().map(|s| s.label()).collect();
            ctx.capabilities.log(
                "warn",
                &format!(
                    "timeline: no capture time found for '{}' (sources: {}); taken_at left empty",
                    doc.file.path,
                    sources.join(", ")
                ),
            );
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

flagme_sdk::export_plugin!(TimelinePlugin, TimelinePlugin::new);
