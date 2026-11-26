use flagme_sdk::{
    DuplicateInfo, Plugin, PluginContext, PluginResult, PluginSettings, SettingsCache,
    SettingsReader, UnifiedDocument,
};
use image::imageops::FilterType;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageReader};
use sha2::{Digest, Sha256};
use std::io::Cursor;

/// KV key of the perceptual-hash index used when `phash_threshold > 0`.
pub const PHASH_INDEX_KEY: &str = "phash:index";

/// Configurable parameters of the duplicate plugin (`config.yaml` `settings`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DuplicateSettings {
    /// Maximum Hamming distance (0..=64) between two dHashes to count as duplicate.
    /// `0` keeps the exact lookup without index.
    pub phash_threshold: u32,
}

impl PluginSettings for DuplicateSettings {
    const PLUGIN: &'static str = "duplicate";

    fn from_reader(base: &Self, r: &mut SettingsReader<'_>) -> Self {
        Self {
            phash_threshold: r.u32("phash_threshold", &[], 0..=64, base.phash_threshold),
        }
    }
}

/// Hamming distance between two 16-digit hex dHashes; `None` if one is not valid hex.
pub fn hamming_distance(a: &str, b: &str) -> Option<u32> {
    let x = u64::from_str_radix(a, 16).ok()?;
    let y = u64::from_str_radix(b, 16).ok()?;
    Some((x ^ y).count_ones())
}

/// Duplicate detection plugin computing cryptographic SHA-256 and 64-bit difference perceptual hashes (dHash).
pub struct DuplicatePlugin {
    settings: SettingsCache<DuplicateSettings>,
}

impl DuplicatePlugin {
    pub fn new() -> Self {
        Self::with_settings(DuplicateSettings::default())
    }

    pub fn with_settings(settings: DuplicateSettings) -> Self {
        Self {
            settings: SettingsCache::new(settings),
        }
    }

    /// True cryptographic SHA-256 hash computation (FIPS 180-4).
    pub fn compute_sha256(&self, bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    }
}

/// 64-bit difference hash (dHash) over the decoded pixels, as 16 hex digits.
///
/// The image is decoded, rotated according to its EXIF orientation, converted to grayscale and
/// scaled to 9x8 pixels; bit `i` is set if a pixel is brighter than its right neighbour.
/// Returns `None` if the bytes cannot be decoded as an image.
pub fn compute_dhash(bytes: &[u8]) -> Option<String> {
    let mut decoder = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_decoder()
        .ok()?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder).ok()?;
    img.apply_orientation(orientation);
    let small = img
        .grayscale()
        .resize_exact(9, 8, FilterType::Triangle)
        .to_luma8();

    let mut bits: u64 = 0;
    for row in 0..8 {
        for col in 0..8 {
            if small.get_pixel(col, row)[0] > small.get_pixel(col + 1, row)[0] {
                bits |= 1u64 << (row * 8 + col);
            }
        }
    }
    Some(format!("{:016x}", bits))
}

impl Plugin for DuplicatePlugin {
    fn name(&self) -> &'static str {
        "duplicate"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn process(
        &self,
        mut doc: UnifiedDocument,
        ctx: &PluginContext,
    ) -> PluginResult<UnifiedDocument> {
        let bytes = match ctx.capabilities.fs_read(&doc.file.path) {
            Ok(bytes) => bytes,
            Err(e) => {
                ctx.capabilities.log(
                    "warn",
                    &format!(
                        "duplicate: cannot read '{}': {}; no hashes",
                        doc.file.path, e
                    ),
                );
                return Ok(doc);
            }
        };

        let sha256 = self.compute_sha256(&bytes);
        let phash = compute_dhash(&bytes);
        if phash.is_none() {
            ctx.capabilities.log(
                "info",
                &format!(
                    "duplicate: '{}' cannot be decoded; no perceptual hash",
                    doc.file.path
                ),
            );
        }

        ctx.capabilities.log(
            "info",
            &format!(
                "Computed hashes: sha256={}, phash={}",
                &sha256[0..8],
                phash.as_deref().unwrap_or("-")
            ),
        );

        let mut duplicates = Vec::new();
        let sha_key = format!("sha256:{}", sha256);
        if let Ok(Some(existing_id)) = ctx.capabilities.kv_get(&sha_key) {
            if existing_id != doc.id && !duplicates.contains(&existing_id) {
                duplicates.push(existing_id);
            }
        } else {
            let _ = ctx.capabilities.kv_set(&sha_key, &doc.id);
        }

        let settings = self.settings.get_or_parse(ctx);
        if let Some(phash) = phash.as_deref() {
            let phash_key = format!("phash:{}", phash);
            if settings.phash_threshold == 0 {
                if let Ok(Some(existing_id)) = ctx.capabilities.kv_get(&phash_key) {
                    if existing_id != doc.id && !duplicates.contains(&existing_id) {
                        ctx.capabilities.log(
                            "warn",
                            &format!("Duplicate detected matching photo id: {}", existing_id),
                        );
                        duplicates.push(existing_id);
                    }
                } else {
                    let _ = ctx.capabilities.kv_set(&phash_key, &doc.id);
                }
            } else {
                // Linear scan over the index; each line is "<phash> <doc.id>".
                let index = ctx
                    .capabilities
                    .kv_get(PHASH_INDEX_KEY)
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let mut already_indexed = false;
                for line in index.lines() {
                    let Some((other_hash, other_id)) = line.split_once(' ') else {
                        continue;
                    };
                    if other_id == doc.id {
                        already_indexed = true;
                        continue;
                    }
                    if let Some(dist) = hamming_distance(phash, other_hash) {
                        if dist <= settings.phash_threshold
                            && !duplicates.iter().any(|d| d == other_id)
                        {
                            ctx.capabilities.log(
                                "warn",
                                &format!(
                                "Duplicate detected matching photo id: {} (Hamming distance {})",
                                other_id, dist
                            ),
                            );
                            duplicates.push(other_id.to_string());
                        }
                    }
                }
                if !already_indexed {
                    let mut updated = index;
                    if !updated.is_empty() && !updated.ends_with('\n') {
                        updated.push('\n');
                    }
                    updated.push_str(&format!("{} {}\n", phash, doc.id));
                    let _ = ctx.capabilities.kv_set(PHASH_INDEX_KEY, &updated);
                }
            }
        }

        doc.file.sha256 = Some(sha256.clone());
        doc.duplicate = Some(DuplicateInfo {
            sha256,
            phash,
            duplicates,
        });

        Ok(doc)
    }
}

flagme_sdk::export_plugin!(DuplicatePlugin, DuplicatePlugin::new);
