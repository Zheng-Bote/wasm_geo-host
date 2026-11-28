use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unified document representing a media item through all pipeline stages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnifiedDocument {
    pub id: String,
    pub file: FileInfo,
    /// Image the pixel plugins read (ADR 0011); `None` until `image-convert` has run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_image: Option<WorkingImage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MetadataInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<LocationInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<CountryInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thumbnails: Vec<ThumbnailEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai: Option<AiInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplicate: Option<DuplicateInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeline: Option<TimelineInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weather: Option<WeatherInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ocr: Option<OcrInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faces: Option<FacesInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privacy: Option<PrivacyInfo>,
    #[serde(default)]
    pub custom: HashMap<String, serde_json::Value>,
}

impl UnifiedDocument {
    pub fn new(id: impl Into<String>, file: FileInfo) -> Self {
        Self {
            id: id.into(),
            file,
            working_image: None,
            metadata: None,
            location: None,
            country: None,
            thumbnails: Vec::new(),
            ai: None,
            duplicate: None,
            timeline: None,
            weather: None,
            ocr: None,
            faces: None,
            privacy: None,
            custom: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileInfo {
    pub path: String,
    pub filename: String,
    pub size_bytes: u64,
    pub mime_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// Working image of a document (ADR 0011): an oriented baseline JPEG written by
/// `image-convert`, or the original file when it is a complete JPEG without rotation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkingImage {
    pub path: String,
    /// Detected source format: `jpeg`, `png`, `tiff`, `bmp`, `webp`, `heic` or `avif`.
    pub source_format: String,
    pub width: u32,
    pub height: u32,
    /// `false` when `path` is the original file.
    pub converted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MetadataInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera_make: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lens_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iso: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focal_length: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub f_number: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposure_time: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub raw_tags: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LocationInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub altitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub county: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    // Localized geocoding based on Country Code
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_country: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_city: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CountryInfo {
    pub country_code: String,
    pub country_name: String,
    pub flag_emoji: String,
    pub continent: String,
    pub region: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThumbnailEntry {
    pub size_label: String,
    pub width: u32,
    pub height: u32,
    pub path: String,
    pub cache_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AiInfo {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<AiTag>,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiTag {
    pub label: String,
    pub confidence: f32,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WeatherInfo {
    pub temperature: Option<f32>,
    pub condition: String,
    pub wmo_code: u8,
    pub icon_emoji: String,
    pub humidity_percent: Option<u8>,
    pub precipitation_mm: Option<f32>,
    pub is_day: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OcrInfo {
    pub combined_text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<TextRegion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextRegion {
    pub text: String,
    pub confidence: f32,
    pub bbox: BoundingBox,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FacesInfo {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<DetectedFace>,
    pub detection_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DetectedFace {
    pub id: String,
    pub bbox: BoundingBox,
    pub confidence: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub landmarks: Option<FaceLandmarks>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaceLandmarks {
    pub left_eye: (f32, f32),
    pub right_eye: (f32, f32),
    pub nose: (f32, f32),
    pub mouth_left: (f32, f32),
    pub mouth_right: (f32, f32),
}

/// Result of the face redaction step of `privacy-redact` (ADR 0012).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FaceRedaction {
    /// Faces found and redacted; `redacted_image_path` is the redacted full-size image.
    Applied,
    /// Face result present but empty.
    NoFaces,
    /// `redact_faces: false`.
    Disabled,
    /// Redaction was required but could not be done; no image may be shown.
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PrivacyInfo {
    pub is_redacted: bool,
    pub faces_redacted_count: usize,
    pub license_plates_redacted_count: usize,
    pub original_gps_fuzzed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redacted_image_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied_rules: Vec<String>,
    /// `None`: unknown (documents of earlier runs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face_redaction: Option<FaceRedaction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DuplicateInfo {
    pub sha256: String,
    /// 64-bit dHash over the decoded pixels; `None` if the image cannot be decoded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phash: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub duplicates: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taken_at: Option<DateTime<Utc>>,
    pub imported_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<DateTime<Utc>>,
    pub resolution_source: String,
}
