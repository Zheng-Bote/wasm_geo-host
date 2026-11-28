//! Guest side of the `media-plugin` world (`wit/media-plugin.wit`, ADR 0003).
//!
//! A plugin crate exports itself as a Wasm component with [`export_plugin!`](crate::export_plugin).
//! The adapter converts the WIT types to the types of `crates/models` and back, builds the
//! [`PluginContext`] from the `settings` argument and routes every capability call through
//! the host imports. Plugin code keeps implementing [`Plugin`] only.

use crate::{HostCapabilities, Plugin, PluginContext, PluginError, PluginResult};
use std::collections::HashMap;
use std::sync::Arc;
use wasm_geo_models as m;

#[doc(hidden)]
pub mod bindings {
    wit_bindgen::generate!({
        world: "media-plugin",
        path: "../../wit",
        pub_export_macro: true,
        export_macro_name: "__export_media_plugin",
        default_bindings_module: "flagme_sdk::component::bindings",
    });
}

use bindings::wasmgeohost::media::host_capabilities as host;
use bindings::wasmgeohost::media::types as w;

/// Capabilities backed by the host imports of the component.
pub struct ComponentCapabilities;

impl HostCapabilities for ComponentCapabilities {
    fn fs_read(&self, path: &str) -> PluginResult<Vec<u8>> {
        host::fs_read(path).map_err(PluginError::from)
    }
    fn fs_write(&self, path: &str, data: &[u8]) -> PluginResult<()> {
        host::fs_write(path, data).map_err(PluginError::from)
    }
    fn fs_modified(&self, path: &str) -> PluginResult<u64> {
        host::fs_modified(path).map_err(PluginError::from)
    }
    fn http_get(&self, url: &str) -> PluginResult<String> {
        host::http_get(url).map_err(PluginError::from)
    }
    fn kv_get(&self, key: &str) -> PluginResult<Option<String>> {
        host::kv_get(key).map_err(PluginError::from)
    }
    fn kv_set(&self, key: &str, value: &str) -> PluginResult<()> {
        host::kv_set(key, value).map_err(PluginError::from)
    }
    fn secret_get(&self, name: &str) -> PluginResult<Option<String>> {
        host::secret_get(name).map_err(PluginError::from)
    }
    fn log(&self, level: &str, message: &str) {
        host::log_event(level, message);
    }
}

/// Runs `plugin` for one `process` call of the component.
pub fn run<P: Plugin>(
    plugin: &P,
    doc: w::Document,
    settings: Vec<(String, String)>,
) -> Result<w::Document, w::PluginError> {
    let doc = document_from_wit(doc).map_err(w::PluginError::from)?;
    let ctx = PluginContext::with_settings(Arc::new(ComponentCapabilities), settings.into_iter().collect());
    plugin
        .process(doc, &ctx)
        .map(document_to_wit)
        .map_err(w::PluginError::from)
}

/// Exports a plugin type as the `media-plugin` component.
///
/// Active only on `wasm32` with the calling crate's feature `component`, so that the plugin's
/// standalone CLI binary (which links the same library) stays a plain WASI command.
/// `$ctor` builds the plugin once per component instance, e.g. `TimelinePlugin::new`.
#[macro_export]
macro_rules! export_plugin {
    ($ty:ty, $ctor:expr) => {
        #[cfg(all(target_arch = "wasm32", feature = "component"))]
        const _: () = {
            fn __flagme_plugin() -> &'static $ty {
                static PLUGIN: ::std::sync::OnceLock<$ty> = ::std::sync::OnceLock::new();
                PLUGIN.get_or_init($ctor)
            }

            struct __FlagmeComponent;

            impl $crate::component::bindings::Guest for __FlagmeComponent {
                fn name() -> ::std::string::String {
                    $crate::Plugin::name(__flagme_plugin()).to_string()
                }
                fn version() -> ::std::string::String {
                    $crate::Plugin::version(__flagme_plugin()).to_string()
                }
                fn process(
                    doc: $crate::component::bindings::wasmgeohost::media::types::Document,
                    settings: ::std::vec::Vec<(::std::string::String, ::std::string::String)>,
                ) -> ::std::result::Result<
                    $crate::component::bindings::wasmgeohost::media::types::Document,
                    $crate::component::bindings::wasmgeohost::media::types::PluginError,
                > {
                    $crate::component::run(__flagme_plugin(), doc, settings)
                }
            }

            $crate::component::bindings::__export_media_plugin!(__FlagmeComponent);
        };
    };
}

// ---------------------------------------------------------------------------------------
// Errors

impl From<w::PluginError> for PluginError {
    fn from(e: w::PluginError) -> Self {
        match e {
            w::PluginError::Serialization(s) => {
                PluginError::Serialization(serde_json::Error::io(std::io::Error::other(s)))
            }
            w::PluginError::CapabilityDenied(s) => PluginError::CapabilityDenied(s),
            w::PluginError::Io(s) => PluginError::Io(s),
            w::PluginError::ProcessingFailed(s) => PluginError::ProcessingFailed(s),
            w::PluginError::Config(s) => PluginError::Config(s),
        }
    }
}

impl From<PluginError> for w::PluginError {
    fn from(e: PluginError) -> Self {
        match e {
            PluginError::Serialization(e) => w::PluginError::Serialization(e.to_string()),
            PluginError::CapabilityDenied(s) => w::PluginError::CapabilityDenied(s),
            PluginError::Io(s) => w::PluginError::Io(s),
            PluginError::ProcessingFailed(s) => w::PluginError::ProcessingFailed(s),
            PluginError::Config(s) => w::PluginError::Config(s),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Document conversion (models <-> WIT). Keep in sync with `crates/host/src/component.rs`.

fn time_to_wit(t: &chrono::DateTime<chrono::Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
}

fn time_from_wit(s: &str) -> PluginResult<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&chrono::Utc))
        .map_err(|e| PluginError::ProcessingFailed(format!("invalid timestamp '{}': {}", s, e)))
}

fn bbox_to_wit(b: m::BoundingBox) -> w::BoundingBox {
    w::BoundingBox { x: b.x, y: b.y, width: b.width, height: b.height }
}

fn bbox_from_wit(b: w::BoundingBox) -> m::BoundingBox {
    m::BoundingBox { x: b.x, y: b.y, width: b.width, height: b.height }
}

/// Converts a models document to the WIT record.
pub fn document_to_wit(d: m::UnifiedDocument) -> w::Document {
    w::Document {
        id: d.id,
        file: w::FileInfo {
            path: d.file.path,
            filename: d.file.filename,
            size_bytes: d.file.size_bytes,
            mime_type: d.file.mime_type,
            sha256: d.file.sha256,
        },
        working_image: d.working_image.map(|x| w::WorkingImage {
            path: x.path,
            source_format: x.source_format,
            width: x.width,
            height: x.height,
            converted: x.converted,
        }),
        metadata: d.metadata.map(|x| w::MetadataInfo {
            camera_make: x.camera_make,
            camera_model: x.camera_model,
            lens_model: x.lens_model,
            iso: x.iso,
            focal_length: x.focal_length,
            f_number: x.f_number,
            exposure_time: x.exposure_time,
            raw_tags: x.raw_tags.into_iter().collect(),
        }),
        location: d.location.map(|x| w::LocationInfo {
            latitude: x.latitude,
            longitude: x.longitude,
            altitude: x.altitude,
            country: x.country,
            country_code: x.country_code,
            region: x.region,
            county: x.county,
            city: x.city,
            local_country: x.local_country,
            local_region: x.local_region,
            local_city: x.local_city,
            local_language: x.local_language,
        }),
        country: d.country.map(|x| w::CountryInfo {
            country_code: x.country_code,
            country_name: x.country_name,
            flag_emoji: x.flag_emoji,
            continent: x.continent,
            region: x.region,
        }),
        thumbnails: d
            .thumbnails
            .into_iter()
            .map(|x| w::ThumbnailEntry {
                size_label: x.size_label,
                width: x.width,
                height: x.height,
                path: x.path,
                cache_key: x.cache_key,
            })
            .collect(),
        ai: d.ai.map(|x| w::AiInfo {
            tags: x
                .tags
                .into_iter()
                .map(|t| w::AiTag { label: t.label, confidence: t.confidence, model: t.model })
                .collect(),
            description: x.description,
            embedding: x.embedding,
        }),
        duplicate: d.duplicate.map(|x| w::DuplicateInfo {
            sha256: x.sha256,
            phash: x.phash,
            duplicates: x.duplicates,
        }),
        timeline: d.timeline.map(|x| w::TimelineInfo {
            taken_at: x.taken_at.as_ref().map(time_to_wit),
            imported_at: time_to_wit(&x.imported_at),
            modified_at: x.modified_at.as_ref().map(time_to_wit),
            resolution_source: x.resolution_source,
        }),
        weather: d.weather.map(|x| w::WeatherInfo {
            temperature: x.temperature,
            condition: x.condition,
            wmo_code: x.wmo_code,
            icon_emoji: x.icon_emoji,
            humidity_percent: x.humidity_percent,
            precipitation_mm: x.precipitation_mm,
            is_day: x.is_day,
        }),
        ocr: d.ocr.map(|x| w::OcrInfo {
            combined_text: x.combined_text,
            regions: x
                .regions
                .into_iter()
                .map(|r| w::TextRegion { text: r.text, confidence: r.confidence, bbox: bbox_to_wit(r.bbox) })
                .collect(),
            primary_language: x.primary_language,
        }),
        faces: d.faces.map(|x| w::FacesInfo {
            faces: x
                .faces
                .into_iter()
                .map(|f| w::DetectedFace {
                    id: f.id,
                    bbox: bbox_to_wit(f.bbox),
                    confidence: f.confidence,
                    landmarks: f.landmarks.map(|l| w::FaceLandmarks {
                        left_eye: l.left_eye,
                        right_eye: l.right_eye,
                        nose: l.nose,
                        mouth_left: l.mouth_left,
                        mouth_right: l.mouth_right,
                    }),
                    embedding: f.embedding,
                    person_id: f.person_id,
                })
                .collect(),
            detection_model: x.detection_model,
        }),
        privacy: d.privacy.map(|x| w::PrivacyInfo {
            is_redacted: x.is_redacted,
            faces_redacted_count: x.faces_redacted_count as u64,
            license_plates_redacted_count: x.license_plates_redacted_count as u64,
            original_gps_fuzzed: x.original_gps_fuzzed,
            redacted_image_path: x.redacted_image_path,
            applied_rules: x.applied_rules,
        }),
        custom_json: serde_json::to_string(&d.custom).unwrap_or_else(|_| "{}".to_string()),
    }
}

/// Converts the WIT record to a models document; fails on invalid timestamps or `custom-json`.
pub fn document_from_wit(d: w::Document) -> PluginResult<m::UnifiedDocument> {
    let timeline = match d.timeline {
        Some(x) => Some(m::TimelineInfo {
            taken_at: x.taken_at.as_deref().map(time_from_wit).transpose()?,
            imported_at: time_from_wit(&x.imported_at)?,
            modified_at: x.modified_at.as_deref().map(time_from_wit).transpose()?,
            resolution_source: x.resolution_source,
        }),
        None => None,
    };
    let custom: HashMap<String, serde_json::Value> = serde_json::from_str(&d.custom_json)?;
    Ok(m::UnifiedDocument {
        id: d.id,
        file: m::FileInfo {
            path: d.file.path,
            filename: d.file.filename,
            size_bytes: d.file.size_bytes,
            mime_type: d.file.mime_type,
            sha256: d.file.sha256,
        },
        working_image: d.working_image.map(|x| m::WorkingImage {
            path: x.path,
            source_format: x.source_format,
            width: x.width,
            height: x.height,
            converted: x.converted,
        }),
        metadata: d.metadata.map(|x| m::MetadataInfo {
            camera_make: x.camera_make,
            camera_model: x.camera_model,
            lens_model: x.lens_model,
            iso: x.iso,
            focal_length: x.focal_length,
            f_number: x.f_number,
            exposure_time: x.exposure_time,
            raw_tags: x.raw_tags.into_iter().collect(),
        }),
        location: d.location.map(|x| m::LocationInfo {
            latitude: x.latitude,
            longitude: x.longitude,
            altitude: x.altitude,
            country: x.country,
            country_code: x.country_code,
            region: x.region,
            county: x.county,
            city: x.city,
            local_country: x.local_country,
            local_region: x.local_region,
            local_city: x.local_city,
            local_language: x.local_language,
        }),
        country: d.country.map(|x| m::CountryInfo {
            country_code: x.country_code,
            country_name: x.country_name,
            flag_emoji: x.flag_emoji,
            continent: x.continent,
            region: x.region,
        }),
        thumbnails: d
            .thumbnails
            .into_iter()
            .map(|x| m::ThumbnailEntry {
                size_label: x.size_label,
                width: x.width,
                height: x.height,
                path: x.path,
                cache_key: x.cache_key,
            })
            .collect(),
        ai: d.ai.map(|x| m::AiInfo {
            tags: x
                .tags
                .into_iter()
                .map(|t| m::AiTag { label: t.label, confidence: t.confidence, model: t.model })
                .collect(),
            description: x.description,
            embedding: x.embedding,
        }),
        duplicate: d.duplicate.map(|x| m::DuplicateInfo {
            sha256: x.sha256,
            phash: x.phash,
            duplicates: x.duplicates,
        }),
        timeline,
        weather: d.weather.map(|x| m::WeatherInfo {
            temperature: x.temperature,
            condition: x.condition,
            wmo_code: x.wmo_code,
            icon_emoji: x.icon_emoji,
            humidity_percent: x.humidity_percent,
            precipitation_mm: x.precipitation_mm,
            is_day: x.is_day,
        }),
        ocr: d.ocr.map(|x| m::OcrInfo {
            combined_text: x.combined_text,
            regions: x
                .regions
                .into_iter()
                .map(|r| m::TextRegion { text: r.text, confidence: r.confidence, bbox: bbox_from_wit(r.bbox) })
                .collect(),
            primary_language: x.primary_language,
        }),
        faces: d.faces.map(|x| m::FacesInfo {
            faces: x
                .faces
                .into_iter()
                .map(|f| m::DetectedFace {
                    id: f.id,
                    bbox: bbox_from_wit(f.bbox),
                    confidence: f.confidence,
                    landmarks: f.landmarks.map(|l| m::FaceLandmarks {
                        left_eye: l.left_eye,
                        right_eye: l.right_eye,
                        nose: l.nose,
                        mouth_left: l.mouth_left,
                        mouth_right: l.mouth_right,
                    }),
                    embedding: f.embedding,
                    person_id: f.person_id,
                })
                .collect(),
            detection_model: x.detection_model,
        }),
        privacy: d.privacy.map(|x| m::PrivacyInfo {
            is_redacted: x.is_redacted,
            faces_redacted_count: x.faces_redacted_count as usize,
            license_plates_redacted_count: x.license_plates_redacted_count as usize,
            original_gps_fuzzed: x.original_gps_fuzzed,
            redacted_image_path: x.redacted_image_path,
            applied_rules: x.applied_rules,
        }),
        custom,
    })
}
