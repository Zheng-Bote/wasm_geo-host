use flagme_sdk::{
    Plugin, PluginContext, PluginResult, PrivacyInfo, UnifiedDocument,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionStyle {
    Pixelate,
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
    pub redact_faces: bool,
    pub fuzz_gps: bool,
    pub gps_decimal_places: u32,
    pub strip_serial_numbers: bool,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            style: RedactionStyle::Pixelate,
            redact_faces: true,
            fuzz_gps: true,
            gps_decimal_places: 2,
            strip_serial_numbers: true,
        }
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
}

impl Plugin for PrivacyRedactPlugin {
    fn name(&self) -> &'static str {
        "privacy-redact"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        ctx.capabilities.log(
            "info",
            &format!("Running privacy redaction and sanitization on '{}'", doc.file.path),
        );

        let mut applied_rules = Vec::new();
        let mut faces_count = 0;

        // 1. Visual Obfuscation (Faces)
        if self.settings.redact_faces {
            if let Some(faces_info) = &doc.faces {
                faces_count = faces_info.faces.len();
                if faces_count > 0 {
                    let style_str = match self.settings.style {
                        RedactionStyle::Pixelate => "pixelate",
                        RedactionStyle::Blackout => "blackout",
                    };
                    applied_rules.push(format!("redacted_{}_faces_via_{}", faces_count, style_str));
                }
            }
        }

        // 2. GPS Fuzzing
        let mut gps_fuzzed = false;
        if self.settings.fuzz_gps {
            if let Some(loc) = &mut doc.location {
                if let (Some(lat), Some(lon)) = (loc.latitude, loc.longitude) {
                    let factor = 10f64.powi(self.settings.gps_decimal_places as i32);
                    loc.latitude = Some((lat * factor).round() / factor);
                    loc.longitude = Some((lon * factor).round() / factor);
                    gps_fuzzed = true;
                    applied_rules.push(format!("fuzzed_gps_precision_{}_decimals", self.settings.gps_decimal_places));
                }
            }
        }

        // 3. Hardware Serial Number Stripping
        if self.settings.strip_serial_numbers {
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
            Some(format!("{}.redacted.jpg", doc.file.path))
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
