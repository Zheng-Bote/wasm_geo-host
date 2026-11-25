use crate::config::AppConfig;
use crate::runtime::PluginRuntime;
use crate::storage::Storage;
use flagme_sdk::UnifiedDocument;
use std::sync::Arc;
use std::time::Instant;

/// Pipeline orchestrator executing plugins in configured sequence.
pub struct Pipeline {
    config: AppConfig,
    runtime: PluginRuntime,
    storage: Arc<Storage>,
}

impl Pipeline {
    pub fn new(config: AppConfig, runtime: PluginRuntime, storage: Arc<Storage>) -> Self {
        Self {
            config,
            runtime,
            storage,
        }
    }

    /// Run the full pipeline for an input media document.
    pub fn process(&self, mut doc: UnifiedDocument) -> anyhow::Result<UnifiedDocument> {
        let total_start = Instant::now();
        tracing::info!(
            photo_id = %doc.id,
            path = %doc.file.path,
            "Starting media processing pipeline"
        );

        for plugin_name in &self.config.pipeline {
            let stage_start = Instant::now();
            tracing::info!(stage = %plugin_name, "Entering pipeline stage");

            match self.runtime.execute_plugin(plugin_name, doc) {
                Ok(updated_doc) => {
                    doc = updated_doc;
                    let stage_time = stage_start.elapsed();
                    tracing::info!(
                        stage = %plugin_name,
                        duration_ms = stage_time.as_millis(),
                        "Stage finished successfully"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        stage = %plugin_name,
                        error = %e,
                        "Pipeline stage error"
                    );
                    return Err(anyhow::anyhow!("Pipeline stage '{}' failed: {}", plugin_name, e));
                }
            }
        }

        // Persist final document into database
        self.storage.save_document(&doc)?;

        let total_elapsed = total_start.elapsed();
        tracing::info!(
            photo_id = %doc.id,
            duration_ms = total_elapsed.as_millis(),
            "Pipeline finished and document persisted successfully"
        );

        Ok(doc)
    }

    pub fn storage(&self) -> Arc<Storage> {
        self.storage.clone()
    }
}
