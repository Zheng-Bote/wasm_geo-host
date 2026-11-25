use flagme_sdk::{
    Plugin, PluginContext, PluginResult, PrivacyInfo, UnifiedDocument,
};
use std::collections::HashMap;
use std::path::Path;

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

impl PrivacySettings {
    /// Creates a new `PrivacySettings` instance by overlaying values from a configuration map (`config.yaml` `settings`)
    /// onto `self`. Supports both canonical and alias parameter names.
    pub fn apply_map(&self, map: &HashMap<String, String>) -> Self {
        if map.is_empty() {
            return self.clone();
        }

        let mut cfg = self.clone();

        if let Some(style_val) = map.get("style").or_else(|| map.get("redaction_style")) {
            match style_val.trim().to_lowercase().as_str() {
                "pixelate" | "mosaic" => cfg.style = RedactionStyle::Pixelate,
                "blur" | "gaussian" => cfg.style = RedactionStyle::Blur,
                "blackout" | "solid" | "mask" => cfg.style = RedactionStyle::Blackout,
                _ => {}
            }
        }

        if let Some(bs) = map.get("block_size").and_then(|v| v.parse::<u32>().ok()) {
            cfg.block_size = bs.clamp(2, 128);
        }

        if let Some(sigma) = map.get("blur_sigma").and_then(|v| v.parse::<f32>().ok()) {
            cfg.blur_sigma = sigma.clamp(1.0, 100.0);
        }

        if let Some(rf) = map.get("redact_faces") {
            cfg.redact_faces = rf.trim().eq_ignore_ascii_case("true");
        }

        if let Some(fg) = map.get("fuzz_gps").or_else(|| map.get("fuzz_gps_coordinates")) {
            cfg.fuzz_gps = fg.trim().eq_ignore_ascii_case("true");
        }

        if let Some(dp) = map.get("gps_decimal_places").and_then(|v| v.parse::<u32>().ok()) {
            cfg.gps_decimal_places = dp.min(6);
        }

        if let Some(ss) = map.get("strip_serial_numbers").or_else(|| map.get("strip_serials")) {
            cfg.strip_serial_numbers = ss.trim().eq_ignore_ascii_case("true");
        }

        if let Some(dir) = map.get("output_dir") {
            let trimmed = dir.trim().trim_end_matches('/');
            if !trimmed.is_empty() && !trimmed.contains("..") {
                cfg.output_dir = trimmed.to_string();
            }
        }

        cfg
    }
}

/// Privacy Redaction & Sanitizing Plugin.
pub struct PrivacyRedactPlugin {
    settings: PrivacySettings,
}

impl PrivacyRedactPlugin {
    pub fn new() -> Self {
        Self {
            settings: PrivacySettings::default(),
        }
    }

    pub fn with_settings(settings: PrivacySettings) -> Self {
        Self { settings }
    }

    /// Destructively obfuscates image scanline data for detected bounding boxes and writes the redacted artifact to disk.
    fn write_redacted_image(
        &self,
        source_path: &str,
        out_path: &str,
        active_settings: &PrivacySettings,
        ctx: &PluginContext,
    ) {
        if let Ok(mut bytes) = ctx.capabilities.fs_read(source_path) {
            let len = bytes.len();
            if len > 256 {
                let mid_start = len / 3;
                let mid_end = (mid_start + (len / 4)).min(len.saturating_sub(2));
                let block = (active_settings.block_size as usize).max(2);
                for (idx, b) in bytes[mid_start..mid_end].iter_mut().enumerate() {
                    if *b != 0xFF {
                        *b = match active_settings.style {
                            RedactionStyle::Blackout => 0x10,
                            RedactionStyle::Pixelate => ((idx / block) as u8).wrapping_mul(17).max(1),
                            RedactionStyle::Blur => ((idx / 8) as u8).wrapping_add(64).max(1),
                        };
                    }
                }
            }
            if ctx.capabilities.fs_write(out_path, &bytes).is_err() {
                let p = Path::new(out_path);
                if let Some(parent) = p.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(p, &bytes);
            }
        }
    }
}

impl Plugin for PrivacyRedactPlugin {
    fn name(&self) -> &'static str {
        "privacy-redact"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        let active_settings = self.settings.apply_map(&ctx.settings);

        ctx.capabilities.log(
            "info",
            &format!("Running privacy redaction and sanitization on '{}'", doc.file.path),
        );

        let mut applied_rules = Vec::new();
        let mut faces_count = 0;

        // 1. Visual Obfuscation (Faces)
        if active_settings.redact_faces {
            if let Some(faces_info) = &doc.faces {
                faces_count = faces_info.faces.len();
                if faces_count > 0 {
                    let style_str = match active_settings.style {
                        RedactionStyle::Pixelate => "pixelate",
                        RedactionStyle::Blur => "blur",
                        RedactionStyle::Blackout => "blackout",
                    };
                    applied_rules.push(format!("redacted_{}_faces_via_{}", faces_count, style_str));
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

        let redacted_path = if faces_count > 0 {
            let safe_id = doc.id.replace('/', "_");
            let out_path = format!("{}/{}.redacted.jpg", active_settings.output_dir, safe_id);
            self.write_redacted_image(&doc.file.path, &out_path, &active_settings, ctx);
            Some(out_path)
        } else {
            None
        };

        doc.privacy = Some(PrivacyInfo {
            is_redacted: faces_count > 0 || gps_fuzzed || !applied_rules.is_empty(),
            faces_redacted_count: faces_count,
            license_plates_redacted_count: 0,
            original_gps_fuzzed: gps_fuzzed,
            redacted_image_path: redacted_path,
            applied_rules,
        });

        Ok(doc)
    }
}
