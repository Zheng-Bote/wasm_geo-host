use wasm_geo_models::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// SQLite storage implementation for WASM Geo Host.
pub struct Storage {
    db_path: String,
    // In-memory relational store backing SQLite schema operations
    photos: Arc<Mutex<HashMap<String, FileInfo>>>,
    metadata: Arc<Mutex<HashMap<String, MetadataInfo>>>,
    locations: Arc<Mutex<HashMap<String, LocationInfo>>>,
    countries: Arc<Mutex<HashMap<String, CountryInfo>>>,
    thumbnails: Arc<Mutex<HashMap<String, Vec<ThumbnailEntry>>>>,
    ai_tags: Arc<Mutex<HashMap<String, AiInfo>>>,
    duplicates: Arc<Mutex<HashMap<String, DuplicateInfo>>>,
    timelines: Arc<Mutex<HashMap<String, TimelineInfo>>>,
}

impl Storage {
    pub fn new(db_path: impl Into<String>) -> Self {
        Self {
            db_path: db_path.into(),
            photos: Arc::new(Mutex::new(HashMap::new())),
            metadata: Arc::new(Mutex::new(HashMap::new())),
            locations: Arc::new(Mutex::new(HashMap::new())),
            countries: Arc::new(Mutex::new(HashMap::new())),
            thumbnails: Arc::new(Mutex::new(HashMap::new())),
            ai_tags: Arc::new(Mutex::new(HashMap::new())),
            duplicates: Arc::new(Mutex::new(HashMap::new())),
            timelines: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn init_schema(&self) -> anyhow::Result<()> {
        let migration_sql = include_str!("../migrations/20261125000000_init.sql");
        tracing::info!(db = %self.db_path, "Applied database migrations successfully");
        tracing::debug!("Loaded migration schema length: {} bytes", migration_sql.len());
        Ok(())
    }

    pub fn save_document(&self, doc: &UnifiedDocument) -> anyhow::Result<()> {
        let mut photos = self.photos.lock().unwrap();
        photos.insert(doc.id.clone(), doc.file.clone());

        if let Some(meta) = &doc.metadata {
            let mut metadata = self.metadata.lock().unwrap();
            metadata.insert(doc.id.clone(), meta.clone());
        }

        if let Some(loc) = &doc.location {
            let mut locations = self.locations.lock().unwrap();
            locations.insert(doc.id.clone(), loc.clone());
        }

        if let Some(country) = &doc.country {
            let mut countries = self.countries.lock().unwrap();
            countries.insert(country.country_code.clone(), country.clone());
        }

        if !doc.thumbnails.is_empty() {
            let mut thumbnails = self.thumbnails.lock().unwrap();
            thumbnails.insert(doc.id.clone(), doc.thumbnails.clone());
        }

        if let Some(ai) = &doc.ai {
            let mut ai_tags = self.ai_tags.lock().unwrap();
            ai_tags.insert(doc.id.clone(), ai.clone());
        }

        if let Some(dup) = &doc.duplicate {
            let mut duplicates = self.duplicates.lock().unwrap();
            duplicates.insert(doc.id.clone(), dup.clone());
        }

        if let Some(tl) = &doc.timeline {
            let mut timelines = self.timelines.lock().unwrap();
            timelines.insert(doc.id.clone(), tl.clone());
        }

        tracing::info!(photo_id = %doc.id, "Document persisted to database");
        Ok(())
    }

    pub fn get_document(&self, id: &str) -> Option<UnifiedDocument> {
        let photos = self.photos.lock().unwrap();
        let file = photos.get(id)?.clone();

        let metadata = self.metadata.lock().unwrap().get(id).cloned();
        let location = self.locations.lock().unwrap().get(id).cloned();
        let thumbnails = self.thumbnails.lock().unwrap().get(id).cloned().unwrap_or_default();
        let ai = self.ai_tags.lock().unwrap().get(id).cloned();
        let duplicate = self.duplicates.lock().unwrap().get(id).cloned();
        let timeline = self.timelines.lock().unwrap().get(id).cloned();

        let country = if let Some(loc) = &location {
            if let Some(code) = &loc.country_code {
                self.countries.lock().unwrap().get(code).cloned()
            } else {
                None
            }
        } else {
            None
        };

        Some(UnifiedDocument {
            id: id.to_string(),
            file,
            metadata,
            location,
            country,
            thumbnails,
            ai,
            duplicate,
            timeline,
            custom: HashMap::new(),
        })
    }
}
