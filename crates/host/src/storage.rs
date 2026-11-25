use wasm_geo_models::*;
use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

/// SQLite storage implementation for WASM Geo Host.
pub struct Storage {
    db_path: String,
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

    /// Applies migrations and creates tables in the SQLite database file on disk.
    pub fn init_schema(&self) -> anyhow::Result<()> {
        let migration_sql = include_str!("../migrations/20261125000000_init.sql");

        match Command::new("sqlite3")
            .arg(&self.db_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(mut child) => {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(migration_sql.as_bytes());
                    drop(stdin);
                }
                let status = child.wait()?;
                if !status.success() {
                    tracing::warn!(db = %self.db_path, "sqlite3 process exited with non-zero status");
                }
            }
            Err(e) => {
                eprintln!("[Storage::init_schema] Failed to spawn sqlite3: {:?}", e);
            }
        }

        tracing::info!(db = %self.db_path, "Applied database migrations successfully to SQLite database");
        Ok(())
    }

    /// Persists a UnifiedDocument into the SQLite tables on disk and in memory.
    pub fn save_document(&self, doc: &UnifiedDocument) -> anyhow::Result<()> {
        // 1. Update In-Memory cache
        {
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
        }

        // 2. Persist to disk SQLite database file via SQL statements
        let mut sql = String::new();
        let sha256_val = doc.file.sha256.as_deref().unwrap_or("");
        sql.push_str(&format!(
            "INSERT OR REPLACE INTO photos (id, file_path, filename, size_bytes, mime_type, sha256) VALUES ('{}', '{}', '{}', {}, '{}', '{}');\n",
            escape_sql(&doc.id),
            escape_sql(&doc.file.path),
            escape_sql(&doc.file.filename),
            doc.file.size_bytes,
            escape_sql(&doc.file.mime_type),
            escape_sql(sha256_val)
        ));

        if let Some(meta) = &doc.metadata {
            let raw_tags_json = serde_json::to_string(&meta.raw_tags).unwrap_or_else(|_| "{}".to_string());
            sql.push_str(&format!(
                "INSERT OR REPLACE INTO metadata (photo_id, camera_make, camera_model, lens_model, iso, focal_length, f_number, exposure_time, raw_tags) VALUES ('{}', '{}', '{}', '{}', {}, {}, {}, '{}', '{}');\n",
                escape_sql(&doc.id),
                escape_sql(meta.camera_make.as_deref().unwrap_or("")),
                escape_sql(meta.camera_model.as_deref().unwrap_or("")),
                escape_sql(meta.lens_model.as_deref().unwrap_or("")),
                meta.iso.unwrap_or(0),
                meta.focal_length.unwrap_or(0.0),
                meta.f_number.unwrap_or(0.0),
                escape_sql(meta.exposure_time.as_deref().unwrap_or("")),
                escape_sql(&raw_tags_json)
            ));
        }

        if let Some(loc) = &doc.location {
            sql.push_str(&format!(
                "INSERT OR REPLACE INTO locations (photo_id, latitude, longitude, altitude, country, country_code, region, county, city) VALUES ('{}', {}, {}, {}, '{}', '{}', '{}', '{}', '{}');\n",
                escape_sql(&doc.id),
                loc.latitude.unwrap_or(0.0),
                loc.longitude.unwrap_or(0.0),
                loc.altitude.unwrap_or(0.0),
                escape_sql(loc.country.as_deref().unwrap_or("")),
                escape_sql(loc.country_code.as_deref().unwrap_or("")),
                escape_sql(loc.region.as_deref().unwrap_or("")),
                escape_sql(loc.county.as_deref().unwrap_or("")),
                escape_sql(loc.city.as_deref().unwrap_or(""))
            ));
        }

        if let Some(country) = &doc.country {
            sql.push_str(&format!(
                "INSERT OR REPLACE INTO countries (country_code, country_name, flag_emoji, continent, region) VALUES ('{}', '{}', '{}', '{}', '{}');\n",
                escape_sql(&country.country_code),
                escape_sql(&country.country_name),
                escape_sql(&country.flag_emoji),
                escape_sql(&country.continent),
                escape_sql(&country.region)
            ));
        }

        if let Some(dup) = &doc.duplicate {
            let dup_json = serde_json::to_string(&dup.duplicates).unwrap_or_else(|_| "[]".to_string());
            sql.push_str(&format!(
                "INSERT OR REPLACE INTO duplicates (photo_id, sha256, phash, duplicate_ids) VALUES ('{}', '{}', '{}', '{}');\n",
                escape_sql(&doc.id),
                escape_sql(&dup.sha256),
                escape_sql(&dup.phash),
                escape_sql(&dup_json)
            ));
        }

        for thumb in &doc.thumbnails {
            sql.push_str(&format!(
                "INSERT INTO thumbnails (photo_id, size_label, width, height, file_path, cache_key) VALUES ('{}', '{}', {}, {}, '{}', '{}');\n",
                escape_sql(&doc.id),
                escape_sql(&thumb.size_label),
                thumb.width,
                thumb.height,
                escape_sql(&thumb.path),
                escape_sql(&thumb.cache_key)
            ));
        }

        if let Some(ai) = &doc.ai {
            for tag in &ai.tags {
                sql.push_str(&format!(
                    "INSERT INTO ai_tags (photo_id, label, confidence, model) VALUES ('{}', '{}', {}, '{}');\n",
                    escape_sql(&doc.id),
                    escape_sql(&tag.label),
                    tag.confidence,
                    escape_sql(&tag.model)
                ));
            }
        }

        if let Ok(mut child) = Command::new("sqlite3")
            .arg(&self.db_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(sql.as_bytes());
                drop(stdin); // Explicitly close stdin pipe so sqlite3 executes
            }
            let _ = child.wait();
        }

        tracing::info!(photo_id = %doc.id, db = %self.db_path, "Document persisted to SQLite database file");
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

fn escape_sql(s: &str) -> String {
    s.replace('\'', "''")
}
