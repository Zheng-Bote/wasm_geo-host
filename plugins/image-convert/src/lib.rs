//! `image-convert`: turns every supported input format into an oriented JPEG working image that
//! the pixel plugins read (ADR 0011).

pub mod convert;
pub mod detect;

use convert::{decode_oriented, encode_jpeg, jpeg_header, jpeg_is_complete, pixel_count};
use detect::SourceFormat;
use flagme_sdk::{
    Plugin, PluginContext, PluginResult, PluginSettings, SettingsCache, SettingsReader, UnifiedDocument,
    WorkingImage,
};
use image::metadata::Orientation;
use sha2::{Digest, Sha256};

/// Settings of `image-convert`.
#[derive(Debug, Clone)]
pub struct ImageConvertSettings {
    pub output_dir: String,
    pub quality: u32,
    pub max_pixels: u32,
}

impl Default for ImageConvertSettings {
    fn default() -> Self {
        Self { output_dir: "cache/working".to_string(), quality: 95, max_pixels: 50_000_000 }
    }
}

impl PluginSettings for ImageConvertSettings {
    const PLUGIN: &'static str = "image-convert";

    fn from_reader(base: &Self, r: &mut SettingsReader<'_>) -> Self {
        Self {
            output_dir: r.path("output_dir", &[], &base.output_dir),
            quality: r.u32("quality", &["jpeg_quality"], 1..=100, base.quality),
            max_pixels: r.u32("max_pixels", &[], 1_000_000..=100_000_000, base.max_pixels),
        }
    }
}

/// File name of the working image: hex SHA-256 of the source bytes plus `.working.jpg`.
pub fn working_file_name(source: &[u8]) -> String {
    let digest = Sha256::digest(source);
    let hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
    format!("{}.working.jpg", hex)
}

/// Working image plugin (ADR 0011).
pub struct ImageConvertPlugin {
    settings: SettingsCache<ImageConvertSettings>,
}

impl ImageConvertPlugin {
    pub fn new() -> Self {
        Self::with_settings(ImageConvertSettings::default())
    }

    pub fn with_settings(settings: ImageConvertSettings) -> Self {
        Self { settings: SettingsCache::new(settings) }
    }

    /// Determines the working image, writing it if needed; `Err` is the reason for the warning.
    fn working_image(&self, path: &str, ctx: &PluginContext) -> Result<WorkingImage, String> {
        let settings = self.settings.get_or_parse(ctx);
        let bytes = ctx.capabilities.fs_read(path).map_err(|e| format!("cannot read: {}", e))?;
        let fmt = SourceFormat::sniff(&bytes).ok_or_else(|| {
            "unsupported format (supported: JPEG, PNG, TIFF, BMP, WebP, HEIC/HEIF, AVIF)".to_string()
        })?;

        if fmt == SourceFormat::Jpeg {
            // The JPEG decoder fills missing scan data with grey instead of failing.
            if !jpeg_is_complete(&bytes) {
                return Err("truncated JPEG (no end-of-image marker after the scan data)".to_string());
            }
            let ((width, height), orientation) = jpeg_header(&bytes)?;
            if orientation == Orientation::NoTransforms {
                return Ok(WorkingImage {
                    path: path.to_string(),
                    source_format: fmt.as_str().to_string(),
                    width,
                    height,
                    converted: false,
                });
            }
        }

        let pixels = pixel_count(&bytes, fmt)?;
        if pixels > settings.max_pixels as u64 {
            return Err(format!("{} pixels exceed max_pixels {}", pixels, settings.max_pixels));
        }

        let out_path = format!("{}/{}", settings.output_dir.trim_end_matches('/'), working_file_name(&bytes));
        if let Ok(existing) = ctx.capabilities.fs_read(&out_path) {
            if jpeg_is_complete(&existing) {
                if let Ok(((width, height), _)) = jpeg_header(&existing) {
                    return Ok(WorkingImage {
                        path: out_path,
                        source_format: fmt.as_str().to_string(),
                        width,
                        height,
                        converted: true,
                    });
                }
            }
        }

        let decoded = decode_oriented(&bytes, fmt)?;
        let jpeg = encode_jpeg(&decoded, settings.quality as u8)?;
        ctx.capabilities
            .fs_write(&out_path, &jpeg)
            .map_err(|e| format!("cannot write '{}': {}", out_path, e))?;
        Ok(WorkingImage {
            path: out_path,
            source_format: fmt.as_str().to_string(),
            width: decoded.rgb.width(),
            height: decoded.rgb.height(),
            converted: true,
        })
    }
}

impl Default for ImageConvertPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for ImageConvertPlugin {
    fn name(&self) -> &'static str {
        "image-convert"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        match self.working_image(&doc.file.path, ctx) {
            Ok(working) => doc.working_image = Some(working),
            Err(reason) => {
                ctx.capabilities.log(
                    "warn",
                    &format!("image-convert: '{}': {}; no working image", doc.file.path, reason),
                );
                doc.working_image = None;
            }
        }
        Ok(doc)
    }
}

flagme_sdk::export_plugin!(ImageConvertPlugin, ImageConvertPlugin::new);
