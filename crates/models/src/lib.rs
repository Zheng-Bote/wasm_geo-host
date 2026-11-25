use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unified document representing a media item through all pipeline stages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnifiedDocument {
    pub id: String,
    pub file: FileInfo,
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
    #[serde(default)]
    pub custom: HashMap<String, serde_json::Value>,
}

impl UnifiedDocument {
    pub fn new(id: impl Into<String>, file: FileInfo) -> Self {
        Self {
            id: id.into(),
            file,
            metadata: None,
            location: None,
            country: None,
            thumbnails: Vec::new(),
            ai: None,
            duplicate: None,
            timeline: None,
            weather: None,
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
pub struct DuplicateInfo {
    pub sha256: String,
    pub phash: String,
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
