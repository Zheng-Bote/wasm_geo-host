use flagme_sdk::{
    Plugin, PluginContext, PluginResult, ThumbnailEntry, UnifiedDocument,
};
use std::path::Path;

/// Thumbnail generation plugin with multi-format and deterministic caching support.
pub struct ThumbnailPlugin;

impl ThumbnailPlugin {
    pub fn new() -> Self {
        Self
    }

    fn calculate_cache_key(&self, path: &str, size: u32) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        size.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    fn is_supported_format(&self, path: &str) -> bool {
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        matches!(
            ext.as_str(),
            "jpg" | "jpeg" | "png" | "webp" | "tiff" | "tif" | "heic" | "avif"
        )
    }
}

impl Plugin for ThumbnailPlugin {
    fn name(&self) -> &'static str {
        "thumbnail"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        let path = &doc.file.path;
        if !self.is_supported_format(path) && !path.ends_with(".dat") {
            ctx.capabilities.log(
                "warn",
                &format!("File format of '{}' is not supported by thumbnail generator", path),
            );
            return Ok(doc);
        }

        ctx.capabilities.log(
            "info",
            &format!("Generating deterministic thumbnails for '{}'", path),
        );

        let target_sizes = [
            ("small", 128u32, 128u32),
            ("medium", 512u32, 512u32),
            ("large", 1024u32, 1024u32),
        ];

        let mut entries = Vec::new();
        for (label, width, height) in target_sizes {
            let cache_key = self.calculate_cache_key(path, width);
            let thumb_path = format!("cache/thumbnails/{}_{}.jpg", cache_key, width);

            entries.push(ThumbnailEntry {
                size_label: label.to_string(),
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
