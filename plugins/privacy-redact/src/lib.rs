use flagme_sdk::{
    BoundingBox, DetectedFace, Plugin, PluginContext, PluginResult, PluginSettings, PrivacyInfo,
    SettingsCache, SettingsReader, UnifiedDocument,
};
use image::codecs::jpeg::JpegEncoder;
use image::{imageops, ImageFormat, Rgb, RgbImage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionStyle {
    Pixelate,
    Blur,
    Blackout,
}

impl Default for RedactionStyle {
    fn default() -> Self {
        Self::Pixelate
    }
}

/// Configuration settings for the privacy redaction plugin.
#[derive(Debug, Clone)]
pub struct PrivacySettings {
    pub style: RedactionStyle,
    pub block_size: u32,
    pub blur_sigma: f32,
    pub redact_faces: bool,
    pub fuzz_gps: bool,
    pub gps_decimal_places: u32,
    pub strip_serial_numbers: bool,
    pub output_dir: String,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            style: RedactionStyle::Pixelate,
            block_size: 16,
            blur_sigma: 24.0,
            redact_faces: true,
            fuzz_gps: true,
            gps_decimal_places: 2,
            strip_serial_numbers: true,
            output_dir: "cache/redacted".to_string(),
        }
    }
}

impl PluginSettings for PrivacySettings {
    const PLUGIN: &'static str = "privacy-redact";

    /// Overlays `config.yaml` `settings` onto `base`. Supports canonical and alias names.
    fn from_reader(base: &Self, r: &mut SettingsReader<'_>) -> Self {
        Self {
            style: r.choice(
                "style",
                &["redaction_style"],
                &[
                    (&["pixelate", "mosaic"], RedactionStyle::Pixelate),
                    (&["blur", "gaussian"], RedactionStyle::Blur),
                    (&["blackout", "solid", "mask"], RedactionStyle::Blackout),
                ],
                &[],
                base.style,
            ),
            block_size: r.u32("block_size", &[], 2..=128, base.block_size),
            blur_sigma: r.f32("blur_sigma", &[], 1.0..=100.0, base.blur_sigma),
            redact_faces: r.bool("redact_faces", &[], base.redact_faces),
            fuzz_gps: r.bool("fuzz_gps", &["fuzz_gps_coordinates"], base.fuzz_gps),
            gps_decimal_places: r.u32("gps_decimal_places", &[], 0..=6, base.gps_decimal_places),
            strip_serial_numbers: r.bool(
                "strip_serial_numbers",
                &["strip_serials"],
                base.strip_serial_numbers,
            ),
            output_dir: r.path("output_dir", &[], &base.output_dir),
        }
    }
}

/// Privacy Redaction & Sanitizing Plugin.
pub struct PrivacyRedactPlugin {
    settings: SettingsCache<PrivacySettings>,
}

impl PrivacyRedactPlugin {
    pub fn new() -> Self {
        Self::with_settings(PrivacySettings::default())
    }

    pub fn with_settings(settings: PrivacySettings) -> Self {
        Self {
            settings: SettingsCache::new(settings),
        }
    }

    /// Reads, decodes and redacts the source image for every valid face bounding box, then
    /// re-encodes it as JPEG and writes it through the `filesystem` capability.
    ///
    /// Returns the number of faces actually processed, or a human-readable failure reason.
    fn write_redacted_image(
        &self,
        source_path: &str,
        out_path: &str,
        faces: &[DetectedFace],
        active_settings: &PrivacySettings,
        ctx: &PluginContext,
    ) -> Result<usize, String> {
        let bytes = ctx
            .capabilities
            .fs_read(source_path)
            .map_err(|e| format!("cannot read '{}': {}", source_path, e))?;
        let (encoded, processed) = redact_jpeg(&bytes, faces, active_settings, |msg| {
            ctx.capabilities.log("warn", msg)
        })?;
        ctx.capabilities
            .fs_write(out_path, &encoded)
            .map_err(|e| format!("cannot write '{}': {}", out_path, e))?;
        Ok(processed)
    }
}

/// JPEG quality used when re-encoding the redacted copy.
pub const REDACTED_JPEG_QUALITY: u8 = 90;

/// Pixel rectangle inside an image (`x`, `y` = top-left corner).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    /// True if the pixel (`px`, `py`) lies inside the rectangle.
    pub fn contains(&self, px: u32, py: u32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }
}

/// Converts a normalised bounding box (0.0–1.0) into a pixel rectangle that fully covers it.
///
/// Coordinates are clamped to the image; non-finite values or an empty area yield `None`.
pub fn bbox_to_rect(bbox: &BoundingBox, img_width: u32, img_height: u32) -> Option<PixelRect> {
    let vals = [bbox.x, bbox.y, bbox.width, bbox.height];
    if vals.iter().any(|v| !v.is_finite()) || img_width == 0 || img_height == 0 {
        return None;
    }
    let span = |start: f32, len: f32, size: u32| -> (u32, u32) {
        let a = start.clamp(0.0, 1.0) as f64;
        let b = (start + len).clamp(0.0, 1.0) as f64;
        // EPS absorbs f32 representation error (e.g. 0.25 + 0.3 = 0.55000001).
        const EPS: f64 = 1e-3;
        let lo = (a * size as f64 + EPS).floor() as u32;
        let hi = ((b * size as f64 - EPS).ceil().max(0.0) as u32).min(size);
        (lo.min(size), hi)
    };
    let (x0, x1) = span(bbox.x, bbox.width, img_width);
    let (y0, y1) = span(bbox.y, bbox.height, img_height);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(PixelRect {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    })
}

/// Destructively obfuscates `rect` in `img`. Pixels outside `rect` are left untouched.
///
/// `rect` must lie inside the image (as produced by [`bbox_to_rect`]).
pub fn redact_region(
    img: &mut RgbImage,
    rect: PixelRect,
    style: RedactionStyle,
    block_size: u32,
    blur_sigma: f32,
) {
    match style {
        RedactionStyle::Blackout => {
            for py in rect.y..rect.y + rect.height {
                for px in rect.x..rect.x + rect.width {
                    img.put_pixel(px, py, Rgb([0, 0, 0]));
                }
            }
        }
        RedactionStyle::Pixelate => {
            let block = block_size.max(1);
            let mut by = rect.y;
            while by < rect.y + rect.height {
                let bh = block.min(rect.y + rect.height - by);
                let mut bx = rect.x;
                while bx < rect.x + rect.width {
                    let bw = block.min(rect.x + rect.width - bx);
                    let mut sum = [0u64; 3];
                    for py in by..by + bh {
                        for px in bx..bx + bw {
                            let p = img.get_pixel(px, py).0;
                            for c in 0..3 {
                                sum[c] += p[c] as u64;
                            }
                        }
                    }
                    let n = (bw as u64) * (bh as u64);
                    let avg = Rgb([
                        ((sum[0] + n / 2) / n) as u8,
                        ((sum[1] + n / 2) / n) as u8,
                        ((sum[2] + n / 2) / n) as u8,
                    ]);
                    for py in by..by + bh {
                        for px in bx..bx + bw {
                            img.put_pixel(px, py, avg);
                        }
                    }
                    bx += bw;
                }
                by += bh;
            }
        }
        RedactionStyle::Blur => {
            let sub = imageops::crop_imm(img, rect.x, rect.y, rect.width, rect.height).to_image();
            let blurred = imageops::blur(&sub, blur_sigma);
            imageops::replace(img, &blurred, rect.x as i64, rect.y as i64);
        }
    }
}

/// Decodes a JPEG, redacts every valid face bounding box and re-encodes it as JPEG.
///
/// Invalid boxes are skipped and reported through `warn`. Returns the encoded bytes and the
/// number of processed faces, or an error if decoding/encoding fails or no box has a valid area.
pub fn redact_jpeg(
    source: &[u8],
    faces: &[DetectedFace],
    settings: &PrivacySettings,
    mut warn: impl FnMut(&str),
) -> Result<(Vec<u8>, usize), String> {
    let decoded = image::load_from_memory_with_format(source, ImageFormat::Jpeg)
        .map_err(|e| format!("cannot decode source image as JPEG: {}", e))?;
    let mut img = decoded.to_rgb8();
    let (w, h) = img.dimensions();

    let mut processed = 0;
    for face in faces {
        match bbox_to_rect(&face.bbox, w, h) {
            Some(rect) => {
                redact_region(&mut img, rect, settings.style, settings.block_size, settings.blur_sigma);
                processed += 1;
            }
            None => warn(&format!(
                "privacy-redact: face '{}' skipped, bounding box {:?} has no valid area in {}x{} image",
                face.id, face.bbox, w, h
            )),
        }
    }
    if processed == 0 {
        return Err("no face bounding box with a valid area".to_string());
    }

    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, REDACTED_JPEG_QUALITY)
        .encode_image(&img)
        .map_err(|e| format!("cannot encode redacted JPEG: {}", e))?;
    Ok((out, processed))
}

impl Plugin for PrivacyRedactPlugin {
    fn name(&self) -> &'static str {
        "privacy-redact"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        let active_settings = self.settings.get_or_parse(ctx);

        ctx.capabilities.log(
            "info",
            &format!("Running privacy redaction and sanitization on '{}'", doc.file.path),
        );

        let mut applied_rules = Vec::new();
        let mut faces_count = 0;
        let mut redacted_path = None;

        // 1. Visual Obfuscation (Faces)
        if active_settings.redact_faces && doc.faces.is_none() {
            ctx.capabilities.log(
                "warn",
                &format!(
                    "privacy-redact: no face detection result for '{}'; faces NOT redacted",
                    doc.file.path
                ),
            );
        }
        if active_settings.redact_faces {
            if let Some(faces_info) = doc.faces.as_ref().filter(|f| !f.faces.is_empty()) {
                let safe_id = doc.id.replace('/', "_");
                let out_path = format!("{}/{}.redacted.jpg", active_settings.output_dir, safe_id);
                match self.write_redacted_image(
                    &doc.file.path,
                    &out_path,
                    &faces_info.faces,
                    &active_settings,
                    ctx,
                ) {
                    Ok(processed) => {
                        let style_str = match active_settings.style {
                            RedactionStyle::Pixelate => "pixelate",
                            RedactionStyle::Blur => "blur",
                            RedactionStyle::Blackout => "blackout",
                        };
                        applied_rules.push(format!("redacted_{}_faces_via_{}", processed, style_str));
                        faces_count = processed;
                        redacted_path = Some(out_path);
                    }
                    Err(reason) => ctx.capabilities.log(
                        "warn",
                        &format!(
                            "privacy-redact: faces in '{}' NOT redacted: {}",
                            doc.file.path, reason
                        ),
                    ),
                }
            }
        }

        // 2. GPS Fuzzing
        let mut gps_fuzzed = false;
        if active_settings.fuzz_gps {
            if let Some(loc) = &mut doc.location {
                if let (Some(lat), Some(lon)) = (loc.latitude, loc.longitude) {
                    let factor = 10f64.powi(active_settings.gps_decimal_places as i32);
                    loc.latitude = Some((lat * factor).round() / factor);
                    loc.longitude = Some((lon * factor).round() / factor);
                    gps_fuzzed = true;
                    applied_rules.push(format!(
                        "fuzzed_gps_precision_{}_decimals",
                        active_settings.gps_decimal_places
                    ));
                }
            }
        }

        // 3. Hardware Serial Number Stripping
        if active_settings.strip_serial_numbers {
            if let Some(meta) = &mut doc.metadata {
                let initial_len = meta.raw_tags.len();
                meta.raw_tags.retain(|k, _| {
                    let key_lower = k.to_lowercase();
                    !key_lower.contains("serialnumber") && !key_lower.contains("serial_number")
                });
                if meta.raw_tags.len() < initial_len {
                    applied_rules.push("stripped_hardware_serial_numbers".to_string());
                }
            }
        }

        doc.privacy = Some(PrivacyInfo {
            is_redacted: !applied_rules.is_empty(),
            faces_redacted_count: faces_count,
            license_plates_redacted_count: 0,
            original_gps_fuzzed: gps_fuzzed,
            redacted_image_path: redacted_path,
            applied_rules,
        });

        Ok(doc)
    }
}
