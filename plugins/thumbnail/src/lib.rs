use flagme_sdk::{
    Plugin, PluginContext, PluginResult, PluginSettings, SettingsCache, SettingsReader,
    ThumbnailEntry, UnifiedDocument,
};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageReader};
use std::io::Cursor;

/// JPEG quality used when `settings.quality` is not set (the encoder's default).
pub const DEFAULT_JPEG_QUALITY: u8 = 75;

/// Configurable parameters of the thumbnail plugin (`config.yaml` `settings`).
#[derive(Debug, Clone, PartialEq)]
pub struct ThumbnailSettings {
    /// Target sizes (longest side in px), ascending, without duplicates.
    pub variants: Vec<u32>,
    /// Upper bound for the variants; larger variants are dropped.
    pub max_dimension: Option<u32>,
    /// JPEG quality 1..=100; `None` uses [`DEFAULT_JPEG_QUALITY`].
    pub quality: Option<u32>,
    /// Output directory of the thumbnail files.
    pub output_dir: String,
}

impl Default for ThumbnailSettings {
    fn default() -> Self {
        Self {
            variants: vec![128, 512, 1024],
            max_dimension: None,
            quality: None,
            output_dir: "cache/thumbnails".to_string(),
        }
    }
}

impl PluginSettings for ThumbnailSettings {
    const PLUGIN: &'static str = "thumbnail";

    fn from_reader(base: &Self, r: &mut SettingsReader<'_>) -> Self {
        Self {
            variants: r.list_u32("variants", 16..=8192, &base.variants),
            max_dimension: r.opt_u32("max_dimension", &[], 16..=8192).or(base.max_dimension),
            quality: r.opt_u32("quality", &[], 1..=100).or(base.quality),
            output_dir: r.path("output_dir", &[], &base.output_dir),
        }
    }
}

impl ThumbnailSettings {
    /// Effective sizes after applying `max_dimension`. If every variant exceeds the limit,
    /// exactly `max_dimension` is used.
    pub fn effective_sizes(&self) -> Vec<u32> {
        match self.max_dimension {
            Some(max) => {
                let sizes: Vec<u32> = self.variants.iter().copied().filter(|v| *v <= max).collect();
                if sizes.is_empty() {
                    vec![max]
                } else {
                    sizes
                }
            }
            None => self.variants.clone(),
        }
    }
}

/// Assigns size labels to ascending sizes: smallest `small`, largest `large`, the remaining
/// size closest to 512 `medium`, every other size `<px>px`.
pub fn size_labels(sizes: &[u32]) -> Vec<String> {
    let n = sizes.len();
    let mut labels: Vec<String> = sizes.iter().map(|s| format!("{}px", s)).collect();
    if n == 0 {
        return labels;
    }
    labels[0] = "small".to_string();
    if n >= 2 {
        labels[n - 1] = "large".to_string();
    }
    if n >= 3 {
        let medium = (1..n - 1)
            .min_by_key(|&i| (sizes[i] as i64 - 512).abs())
            .unwrap_or(1);
        labels[medium] = "medium".to_string();
    }
    labels
}

/// Thumbnail generation plugin with multi-format and deterministic caching support.
pub struct ThumbnailPlugin {
    settings: SettingsCache<ThumbnailSettings>,
}

impl ThumbnailPlugin {
    pub fn new() -> Self {
        Self::with_settings(ThumbnailSettings::default())
    }

    pub fn with_settings(settings: ThumbnailSettings) -> Self {
        Self {
            settings: SettingsCache::new(settings),
        }
    }

    fn calculate_cache_key(&self, path: &str, size: u32) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        size.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }
}

/// Decodes an image (format detected from the content) and applies its EXIF orientation.
pub fn decode_oriented(bytes: &[u8]) -> Result<DynamicImage, String> {
    let mut decoder = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_decoder()
        .map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// Scales `img` so that its longer side is at most `size` (aspect ratio kept, never upscaled)
/// and encodes it as JPEG. Returns the bytes and the actual dimensions.
pub fn render_thumbnail(img: &DynamicImage, size: u32, quality: u8) -> Result<(Vec<u8>, u32, u32), String> {
    let scaled = if img.width().max(img.height()) > size {
        img.resize(size, size, FilterType::Lanczos3)
    } else {
        img.clone()
    };
    let rgb = scaled.to_rgb8();
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality)
        .encode_image(&rgb)
        .map_err(|e| e.to_string())?;
    Ok((out, rgb.width(), rgb.height()))
}

impl Plugin for ThumbnailPlugin {
    fn name(&self) -> &'static str {
        "thumbnail"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        let path = doc.file.path.clone();
        let warn = |msg: String| ctx.capabilities.log("warn", &format!("thumbnail: {}", msg));

        let bytes = match ctx.capabilities.fs_read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                warn(format!("cannot read '{}': {}; no thumbnails", path, e));
                return Ok(doc);
            }
        };
        let img = match decode_oriented(&bytes) {
            Ok(img) => img,
            Err(e) => {
                warn(format!("cannot decode '{}': {}; no thumbnails", path, e));
                return Ok(doc);
            }
        };

        let settings = self.settings.get_or_parse(ctx);
        let quality = settings.quality.map(|q| q as u8).unwrap_or(DEFAULT_JPEG_QUALITY);
        let sizes = settings.effective_sizes();
        let labels = size_labels(&sizes);
        ctx.capabilities.log("info", &format!("Generating thumbnails for '{}'", path));

        let mut entries = Vec::new();
        for (label, size) in labels.into_iter().zip(sizes) {
            let cache_key = self.calculate_cache_key(&path, size);
            let thumb_path = format!("{}/{}_{}.jpg", settings.output_dir, cache_key, size);
            let (jpeg, width, height) = match render_thumbnail(&img, size, quality) {
                Ok(r) => r,
                Err(e) => {
                    warn(format!("cannot encode {}px thumbnail of '{}': {}", size, path, e));
                    continue;
                }
            };
            if let Err(e) = ctx.capabilities.fs_write(&thumb_path, &jpeg) {
                warn(format!("cannot write '{}': {}", thumb_path, e));
                continue;
            }
            entries.push(ThumbnailEntry {
                size_label: label,
                width,
                height,
                path: thumb_path,
                cache_key,
            });
        }

        doc.thumbnails = entries;
        Ok(doc)
    }
}

flagme_sdk::export_plugin!(ThumbnailPlugin, ThumbnailPlugin::new);
