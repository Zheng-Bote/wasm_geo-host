use flagme_sdk::{
    DuplicateInfo, Plugin, PluginContext, PluginResult, PluginSettings, SettingsCache,
    SettingsReader, UnifiedDocument,
};
use sha2::{Digest, Sha256};

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

    /// Computes a 64-bit gradient difference hash (dHash) over 9x8 luminance samples derived from the image payload.
    pub fn compute_phash(&self, bytes: &[u8]) -> String {
        if bytes.is_empty() {
            return "0000000000000000".to_string();
        }

        // Skip fixed file headers (EXIF/APP markers) to sample actual image scanline entropy
        let payload = find_image_entropy_payload(bytes);
        let mut grid = [0u8; 72]; // 9 columns x 8 rows

        let chunk_len = (payload.len() / 72).max(1);
        for (idx, cell) in grid.iter_mut().enumerate() {
            let start = (idx * chunk_len).min(payload.len().saturating_sub(1));
            let end = ((idx + 1) * chunk_len).min(payload.len());
            let slice = &payload[start..end.max(start + 1)];

            let sum: u64 = slice.iter().map(|&b| b as u64).sum();
            *cell = (sum / (slice.len() as u64)) as u8;
        }

        // 8 rows of 8 horizontal gradient comparisons -> 64-bit perceptual hash
        let mut hash_bits: u64 = 0;
        let mut bit_index = 0;
        for row in 0..8 {
            for col in 0..8 {
                let left = grid[row * 9 + col];
                let right = grid[row * 9 + col + 1];
                if left > right {
                    hash_bits |= 1u64 << bit_index;
                }
                bit_index += 1;
            }
        }

        format!("{:016x}", hash_bits)
    }

    /// True cryptographic SHA-256 hash computation (FIPS 180-4).
    pub fn compute_sha256(&self, bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    }
}

fn find_image_entropy_payload(bytes: &[u8]) -> &[u8] {
    // For JPEGs, scan past APP metadata segments to the SOS (Start of Scan, 0xFF 0xDA) marker
    if bytes.len() > 4 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        let mut i = 2;
        while i + 4 < bytes.len() {
            if bytes[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = bytes[i + 1];
            if marker == 0xDA {
                let header_len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
                let scan_start = (i + 2 + header_len).min(bytes.len());
                if scan_start < bytes.len() {
                    return &bytes[scan_start..];
                }
                break;
            }
            let seg_len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            if seg_len < 2 {
                break;
            }
            i += 2 + seg_len;
        }
    }
    bytes
}

impl Plugin for DuplicatePlugin {
    fn name(&self) -> &'static str {
        "duplicate"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        let bytes = ctx
            .capabilities
            .fs_read(&doc.file.path)
            .unwrap_or_else(|_| doc.file.path.as_bytes().to_vec());

        let sha256 = self.compute_sha256(&bytes);
        let phash = self.compute_phash(&bytes);

        ctx.capabilities.log(
            "info",
            &format!("Computed hashes: sha256={}, phash={}", &sha256[0..8], phash),
        );

        let mut duplicates = Vec::new();
        let sha_key = format!("sha256:{}", sha256);
        let phash_key = format!("phash:{}", phash);

        if let Ok(Some(existing_id)) = ctx.capabilities.kv_get(&sha_key) {
            if existing_id != doc.id && !duplicates.contains(&existing_id) {
                duplicates.push(existing_id);
            }
        } else {
            let _ = ctx.capabilities.kv_set(&sha_key, &doc.id);
        }

        let settings = self.settings.get_or_parse(ctx);
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
                if let Some(dist) = hamming_distance(&phash, other_hash) {
                    if dist <= settings.phash_threshold && !duplicates.iter().any(|d| d == other_id) {
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

        doc.file.sha256 = Some(sha256.clone());
        doc.duplicate = Some(DuplicateInfo {
            sha256,
            phash,
            duplicates,
        });

        Ok(doc)
    }
}
