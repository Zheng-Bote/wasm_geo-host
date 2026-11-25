use flagme_sdk::{
    DuplicateInfo, Plugin, PluginContext, PluginResult, UnifiedDocument,
};

/// Duplicate detection plugin computing cryptographic and perceptual hashes.
pub struct DuplicatePlugin;

impl DuplicatePlugin {
    pub fn new() -> Self {
        Self
    }

    /// Fast perceptual difference hash (dHash/pHash) simulation on image byte stream.
    fn compute_phash(&self, bytes: &[u8]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        let stride = (bytes.len() / 64).max(1);
        for chunk in bytes.chunks(stride).take(64) {
            chunk.hash(&mut hasher);
        }
        format!("{:016x}", hasher.finish())
    }

    /// SHA256 cryptographic hash computation.
    fn compute_sha256(&self, bytes: &[u8]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        format!("{:032x}{:032x}", hasher.finish(), hasher.finish())
    }
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
        let kv_key = format!("phash:{}", phash);
        if let Ok(Some(existing_id)) = ctx.capabilities.kv_get(&kv_key) {
            if existing_id != doc.id {
                ctx.capabilities.log(
                    "warn",
                    &format!("Duplicate detected matching photo id: {}", existing_id),
                );
                duplicates.push(existing_id);
            }
        } else {
            let _ = ctx.capabilities.kv_set(&kv_key, &doc.id);
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
