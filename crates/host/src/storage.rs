use wasm_geo_models::*;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

/// Search result item returned by full-text search.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    pub photo_id: String,
    pub filename: String,
    pub rank: f64,
}

/// Similarity search result item returned by vector cosine similarity matching.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SimilarityResult {
    pub photo_id: String,
    pub score: f32,
}

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
    weather: Arc<Mutex<HashMap<String, WeatherInfo>>>,
    ocr: Arc<Mutex<HashMap<String, OcrInfo>>>,
    faces: Arc<Mutex<HashMap<String, FacesInfo>>>,
    privacy: Arc<Mutex<HashMap<String, PrivacyInfo>>>,
}

impl Storage {
    pub fn new(db_path: impl Into<String>) -> Self {
        let storage = Self {
            db_path: db_path.into(),
            photos: Arc::new(Mutex::new(HashMap::new())),
            metadata: Arc::new(Mutex::new(HashMap::new())),
            locations: Arc::new(Mutex::new(HashMap::new())),
            countries: Arc::new(Mutex::new(HashMap::new())),
            thumbnails: Arc::new(Mutex::new(HashMap::new())),
            ai_tags: Arc::new(Mutex::new(HashMap::new())),
            duplicates: Arc::new(Mutex::new(HashMap::new())),
            timelines: Arc::new(Mutex::new(HashMap::new())),
            weather: Arc::new(Mutex::new(HashMap::new())),
            ocr: Arc::new(Mutex::new(HashMap::new())),
            faces: Arc::new(Mutex::new(HashMap::new())),
            privacy: Arc::new(Mutex::new(HashMap::new())),
        };
        storage.load_existing_from_disk();
        storage
    }

    /// Applies migrations and creates tables in the SQLite database file on disk,
    /// then synchronizes any existing records into memory.
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

        self.load_existing_from_disk();

        tracing::info!(db = %self.db_path, "Applied database migrations successfully to SQLite database");
        Ok(())
    }

    /// Loads existing documents from the SQLite database file into the in-memory caches.
    pub fn load_existing_from_disk(&self) {
        if !Path::new(&self.db_path).exists() {
            return;
        }

        // 1. Load complete UnifiedDocuments from documents_store if available
        if let Ok(output) = Command::new("sqlite3")
            .arg(&self.db_path)
            .arg("SELECT doc_json FROM documents_store;")
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with('{') {
                        if let Ok(doc) = serde_json::from_str::<UnifiedDocument>(trimmed) {
                            self.cache_document_in_memory(&doc);
                        }
                    }
                }
            }
        }

        // 2. Also load any rows from relational tables that might not be in documents_store
        if let Ok(output) = Command::new("sqlite3")
            .arg(&self.db_path)
            .arg("-separator")
            .arg("\t")
            .arg("SELECT id, file_path, filename, size_bytes, mime_type, COALESCE(sha256, '') FROM photos;")
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 6 {
                        let id = parts[0].trim().to_string();
                        let has_already = { self.photos.lock().unwrap().contains_key(&id) };
                        if !has_already {
                            let sha256 = if parts[5].trim().is_empty() {
                                None
                            } else {
                                Some(parts[5].trim().to_string())
                            };
                            let file_info = FileInfo {
                                path: parts[1].trim().to_string(),
                                filename: parts[2].trim().to_string(),
                                size_bytes: parts[3].trim().parse().unwrap_or(0),
                                mime_type: parts[4].trim().to_string(),
                                sha256,
                            };
                            self.photos.lock().unwrap().insert(id, file_info);
                        }
                    }
                }
            }
        }

        if let Ok(output) = Command::new("sqlite3")
            .arg(&self.db_path)
            .arg("-separator")
            .arg("\t")
            .arg("SELECT country_code, country_name, flag_emoji, continent, region FROM countries;")
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let mut countries = self.countries.lock().unwrap();
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 5 {
                        let code = parts[0].trim().to_string();
                        countries.entry(code.clone()).or_insert_with(|| CountryInfo {
                            country_code: code,
                            country_name: parts[1].trim().to_string(),
                            flag_emoji: parts[2].trim().to_string(),
                            continent: parts[3].trim().to_string(),
                            region: parts[4].trim().to_string(),
                        });
                    }
                }
            }
        }

        if let Ok(output) = Command::new("sqlite3")
            .arg(&self.db_path)
            .arg("-separator")
            .arg("\t")
            .arg("SELECT photo_id, latitude, longitude, altitude, country, country_code, region, county, city, local_country, local_region, local_city, local_language FROM locations;")
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let mut locations = self.locations.lock().unwrap();
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 13 {
                        let id = parts[0].trim().to_string();
                        locations.entry(id).or_insert_with(|| LocationInfo {
                            latitude: parts[1].trim().parse().ok(),
                            longitude: parts[2].trim().parse().ok(),
                            altitude: parts[3].trim().parse().ok(),
                            country: non_empty_opt(parts[4]),
                            country_code: non_empty_opt(parts[5]),
                            region: non_empty_opt(parts[6]),
                            county: non_empty_opt(parts[7]),
                            city: non_empty_opt(parts[8]),
                            local_country: non_empty_opt(parts[9]),
                            local_region: non_empty_opt(parts[10]),
                            local_city: non_empty_opt(parts[11]),
                            local_language: non_empty_opt(parts[12]),
                        });
                    }
                }
            }
        }
    }

    fn cache_document_in_memory(&self, doc: &UnifiedDocument) {
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

        if let Some(w) = &doc.weather {
            let mut weather = self.weather.lock().unwrap();
            weather.insert(doc.id.clone(), w.clone());
        }

        if let Some(o) = &doc.ocr {
            let mut ocr = self.ocr.lock().unwrap();
            ocr.insert(doc.id.clone(), o.clone());
        }

        if let Some(f) = &doc.faces {
            let mut faces = self.faces.lock().unwrap();
            faces.insert(doc.id.clone(), f.clone());
        }

        if let Some(p) = &doc.privacy {
            let mut privacy = self.privacy.lock().unwrap();
            privacy.insert(doc.id.clone(), p.clone());
        }
    }

    /// Persists a UnifiedDocument into the SQLite tables on disk and in memory.
    pub fn save_document(&self, doc: &UnifiedDocument) -> anyhow::Result<()> {
        // 1. Update In-Memory cache
        self.cache_document_in_memory(doc);

        // 2. Persist to disk SQLite database file via SQL statements
        let mut sql = String::new();
        let sha256_val = doc.file.sha256.as_deref().unwrap_or("");
        let is_redacted = if doc.privacy.as_ref().map(|p| p.is_redacted).unwrap_or(false) { 1 } else { 0 };
        let redacted_path = doc.privacy.as_ref().and_then(|p| p.redacted_image_path.as_deref()).unwrap_or("");

        sql.push_str(&format!(
            "INSERT OR REPLACE INTO photos (id, file_path, filename, size_bytes, mime_type, sha256, is_redacted, redacted_file_path) VALUES ('{}', '{}', '{}', {}, '{}', '{}', {}, '{}');\n",
            escape_sql(&doc.id),
            escape_sql(&doc.file.path),
            escape_sql(&doc.file.filename),
            doc.file.size_bytes,
            escape_sql(&doc.file.mime_type),
            escape_sql(sha256_val),
            is_redacted,
            escape_sql(redacted_path)
        ));

        if let Ok(full_doc_json) = serde_json::to_string(doc) {
            sql.push_str(&format!(
                "INSERT OR REPLACE INTO documents_store (photo_id, doc_json) VALUES ('{}', '{}');\n",
                escape_sql(&doc.id),
                escape_sql(&full_doc_json)
            ));
        }

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
            let is_gps_fuzzed = if doc.privacy.as_ref().map(|p| p.original_gps_fuzzed).unwrap_or(false) { 1 } else { 0 };
            sql.push_str(&format!(
                "INSERT OR REPLACE INTO locations (photo_id, latitude, longitude, altitude, country, country_code, region, county, city, local_country, local_region, local_city, local_language, is_gps_fuzzed) VALUES ('{}', {}, {}, {}, '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', {});\n",
                escape_sql(&doc.id),
                loc.latitude.unwrap_or(0.0),
                loc.longitude.unwrap_or(0.0),
                loc.altitude.unwrap_or(0.0),
                escape_sql(loc.country.as_deref().unwrap_or("")),
                escape_sql(loc.country_code.as_deref().unwrap_or("")),
                escape_sql(loc.region.as_deref().unwrap_or("")),
                escape_sql(loc.county.as_deref().unwrap_or("")),
                escape_sql(loc.city.as_deref().unwrap_or("")),
                escape_sql(loc.local_country.as_deref().unwrap_or("")),
                escape_sql(loc.local_region.as_deref().unwrap_or("")),
                escape_sql(loc.local_city.as_deref().unwrap_or("")),
                escape_sql(loc.local_language.as_deref().unwrap_or("")),
                is_gps_fuzzed
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
                escape_sql(dup.phash.as_deref().unwrap_or_default()),
                escape_sql(&dup_json)
            ));
        }

        if let Some(tl) = &doc.timeline {
            let taken_str = tl.taken_at.map(|d| d.to_rfc3339()).unwrap_or_default();
            let imported_str = tl.imported_at.to_rfc3339();
            let modified_str = tl.modified_at.map(|d| d.to_rfc3339()).unwrap_or_default();
            sql.push_str(&format!(
                "INSERT OR REPLACE INTO timelines (photo_id, taken_at, imported_at, modified_at, resolution_source) VALUES ('{}', '{}', '{}', '{}', '{}');\n",
                escape_sql(&doc.id),
                escape_sql(&taken_str),
                escape_sql(&imported_str),
                escape_sql(&modified_str),
                escape_sql(&tl.resolution_source)
            ));
        }

        if let Some(p) = &doc.privacy {
            let rules_json = serde_json::to_string(&p.applied_rules).unwrap_or_else(|_| "[]".to_string());
            sql.push_str(&format!(
                "INSERT OR REPLACE INTO privacy_redactions (photo_id, is_redacted, faces_redacted_count, license_plates_redacted_count, original_gps_fuzzed, redacted_image_path, applied_rules) VALUES ('{}', {}, {}, {}, {}, '{}', '{}');\n",
                escape_sql(&doc.id),
                if p.is_redacted { 1 } else { 0 },
                p.faces_redacted_count,
                p.license_plates_redacted_count,
                if p.original_gps_fuzzed { 1 } else { 0 },
                escape_sql(p.redacted_image_path.as_deref().unwrap_or("")),
                escape_sql(&rules_json)
            ));
        }

        if !doc.thumbnails.is_empty() {
            sql.push_str(&format!(
                "DELETE FROM thumbnails WHERE photo_id = '{}';\n",
                escape_sql(&doc.id)
            ));
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
        }

        if let Some(ai) = &doc.ai {
            sql.push_str(&format!(
                "DELETE FROM ai_tags WHERE photo_id = '{}';\n",
                escape_sql(&doc.id)
            ));
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

        // Full-Text Search (FTS5) Synchronization
        let camera_make = doc.metadata.as_ref().and_then(|m| m.camera_make.as_deref()).unwrap_or("");
        let camera_model = doc.metadata.as_ref().and_then(|m| m.camera_model.as_deref()).unwrap_or("");
        let country = doc.location.as_ref().and_then(|l| l.country.as_deref()).unwrap_or("");
        let local_country = doc.location.as_ref().and_then(|l| l.local_country.as_deref()).unwrap_or("");
        let city = doc.location.as_ref().and_then(|l| l.city.as_deref()).unwrap_or("");
        let local_city = doc.location.as_ref().and_then(|l| l.local_city.as_deref()).unwrap_or("");
        let ai_tags_str = doc.ai.as_ref().map(|a| {
            a.tags.iter().map(|t| t.label.as_str()).collect::<Vec<_>>().join(" ")
        }).unwrap_or_default();
        let raw_tags_str = doc.metadata.as_ref().map(|m| {
            m.raw_tags.values().map(|v| v.as_str()).collect::<Vec<_>>().join(" ")
        }).unwrap_or_default();
        let description = doc.ai.as_ref().map(|a| a.description.as_str()).filter(|s| !s.is_empty()).or_else(|| doc.custom.get("description").and_then(|v| v.as_str())).unwrap_or("");
        let ocr_text_str = doc.ocr.as_ref().map(|o| o.combined_text.as_str()).unwrap_or("");

        sql.push_str(&format!(
            "DELETE FROM photos_fts WHERE photo_id = '{}';\n",
            escape_sql(&doc.id)
        ));
        sql.push_str(&format!(
            "INSERT INTO photos_fts (photo_id, filename, camera_make, camera_model, country, local_country, city, local_city, ai_tags, description, raw_tags, ocr_text) VALUES ('{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}');\n",
            escape_sql(&doc.id),
            escape_sql(&doc.file.filename),
            escape_sql(camera_make),
            escape_sql(camera_model),
            escape_sql(country),
            escape_sql(local_country),
            escape_sql(city),
            escape_sql(local_city),
            escape_sql(&ai_tags_str),
            escape_sql(description),
            escape_sql(&raw_tags_str),
            escape_sql(ocr_text_str)
        ));

        // Persist dense vector embedding as SQLite BLOB if present
        if let Some(ai) = &doc.ai {
            if let Some(emb) = &ai.embedding {
                let mut blob_hex = String::with_capacity(emb.len() * 8);
                for val in emb {
                    for b in val.to_le_bytes() {
                        use std::fmt::Write;
                        let _ = write!(&mut blob_hex, "{:02x}", b);
                    }
                }
                let model = ai.tags.first().map(|t| t.model.as_str()).unwrap_or("unknown");
                sql.push_str(&format!(
                    "INSERT OR REPLACE INTO photo_embeddings (photo_id, embedding, model) VALUES ('{}', X'{}', '{}');\n",
                    escape_sql(&doc.id),
                    blob_hex,
                    escape_sql(model)
                ));
            }
        }

        if let Some(w) = &doc.weather {
            sql.push_str(&format!(
                "INSERT OR REPLACE INTO weather (photo_id, temperature, condition, wmo_code, icon_emoji, humidity_percent, precipitation_mm, is_day) VALUES ('{}', {}, '{}', {}, '{}', {}, {}, {});\n",
                escape_sql(&doc.id),
                w.temperature.unwrap_or(0.0),
                escape_sql(&w.condition),
                w.wmo_code,
                escape_sql(&w.icon_emoji),
                w.humidity_percent.unwrap_or(0),
                w.precipitation_mm.unwrap_or(0.0),
                if w.is_day.unwrap_or(false) { 1 } else { 0 }
            ));
        }

        if let Some(ocr) = &doc.ocr {
            let primary_lang = ocr.primary_language.as_deref().unwrap_or("");
            sql.push_str(&format!(
                "INSERT OR REPLACE INTO ocr_texts (photo_id, combined_text, primary_language) VALUES ('{}', '{}', '{}');\n",
                escape_sql(&doc.id),
                escape_sql(&ocr.combined_text),
                escape_sql(primary_lang)
            ));
            sql.push_str(&format!(
                "DELETE FROM ocr_regions WHERE photo_id = '{}';\n",
                escape_sql(&doc.id)
            ));
            for reg in &ocr.regions {
                sql.push_str(&format!(
                    "INSERT INTO ocr_regions (photo_id, text, confidence, bbox_x, bbox_y, bbox_w, bbox_h) VALUES ('{}', '{}', {}, {}, {}, {}, {});\n",
                    escape_sql(&doc.id),
                    escape_sql(&reg.text),
                    reg.confidence,
                    reg.bbox.x,
                    reg.bbox.y,
                    reg.bbox.width,
                    reg.bbox.height
                ));
            }
        }

        if let Some(faces_info) = &doc.faces {
            for face in &faces_info.faces {
                let emb_sql = if let Some(emb) = &face.embedding {
                    let mut hex = String::with_capacity(emb.len() * 8);
                    for v in emb {
                        for b in v.to_le_bytes() {
                            use std::fmt::Write;
                            let _ = write!(&mut hex, "{:02x}", b);
                        }
                    }
                    format!("X'{}'", hex)
                } else {
                    "NULL".to_string()
                };

                let person_sql = face
                    .person_id
                    .as_deref()
                    .map(|p| {
                        sql.push_str(&format!(
                            "INSERT OR IGNORE INTO persons (id, name, representative_face_id) VALUES ('{}', '{}', '{}');\n",
                            escape_sql(p),
                            escape_sql(p),
                            escape_sql(&face.id)
                        ));
                        format!("'{}'", escape_sql(p))
                    })
                    .unwrap_or_else(|| "NULL".to_string());

                sql.push_str(&format!(
                    "INSERT OR REPLACE INTO faces (id, photo_id, bbox_x, bbox_y, bbox_w, bbox_h, confidence, embedding, person_id) VALUES ('{}', '{}', {}, {}, {}, {}, {}, {}, {});\n",
                    escape_sql(&face.id),
                    escape_sql(&doc.id),
                    face.bbox.x,
                    face.bbox.y,
                    face.bbox.width,
                    face.bbox.height,
                    face.confidence,
                    emb_sql,
                    person_sql
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
                drop(stdin);
            }
            let _ = child.wait();
        }

        tracing::info!(photo_id = %doc.id, db = %self.db_path, "Document persisted to SQLite database file");
        Ok(())
    }

    pub fn get_document(&self, id: &str) -> Option<UnifiedDocument> {
        // Reload from disk if not yet in memory
        let has_in_memory = { self.photos.lock().unwrap().contains_key(id) };
        if !has_in_memory {
            self.load_existing_from_disk();
        }

        let photos = self.photos.lock().unwrap();
        let file = photos.get(id)?.clone();
        drop(photos);

        let metadata = self.metadata.lock().unwrap().get(id).cloned();
        let location = self.locations.lock().unwrap().get(id).cloned();
        let thumbnails = self.thumbnails.lock().unwrap().get(id).cloned().unwrap_or_default();
        let ai = self.ai_tags.lock().unwrap().get(id).cloned();
        let duplicate = self.duplicates.lock().unwrap().get(id).cloned();
        let timeline = self.timelines.lock().unwrap().get(id).cloned();
        let weather = self.weather.lock().unwrap().get(id).cloned();
        let ocr = self.ocr.lock().unwrap().get(id).cloned();
        let faces = self.faces.lock().unwrap().get(id).cloned();
        let privacy = self.privacy.lock().unwrap().get(id).cloned();

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
            weather,
            ocr,
            faces,
            privacy,
            custom: HashMap::new(),
        })
    }

    /// Returns a list of all documents currently stored in memory or on disk.
    pub fn list_documents(&self, page: usize, limit: usize) -> Vec<UnifiedDocument> {
        self.load_existing_from_disk();

        let ids: Vec<String> = {
            let photos = self.photos.lock().unwrap();
            let mut list: Vec<String> = photos.keys().cloned().collect();
            list.sort();
            list
        };

        let start = page.saturating_sub(1) * limit;
        if start >= ids.len() {
            return Vec::new();
        }

        ids.into_iter()
            .skip(start)
            .take(limit)
            .filter_map(|id| self.get_document(&id))
            .collect()
    }

    /// Checks if a file with the given SHA-256 hash has already been stored.
    pub fn has_sha256(&self, sha256_hash: &str) -> bool {
        if sha256_hash.is_empty() {
            return false;
        }

        // Check in-memory photos cache first
        {
            let photos = self.photos.lock().unwrap();
            for file in photos.values() {
                if file.sha256.as_deref() == Some(sha256_hash) {
                    return true;
                }
            }
        }

        // Query disk SQLite database
        let sql = format!(
            "SELECT count(*) FROM photos WHERE sha256 = '{}';",
            escape_sql(sha256_hash)
        );

        if let Ok(output) = Command::new("sqlite3")
            .arg(&self.db_path)
            .arg(&sql)
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Ok(count) = stdout.trim().parse::<i64>() {
                    return count > 0;
                }
            }
        }

        false
    }

    /// Queries the FTS5 full-text search index and returns ranked results.
    pub fn search(&self, query: &str) -> anyhow::Result<Vec<SearchResult>> {
        let fts_query = format!(
            "SELECT photo_id, filename, bm25(photos_fts) AS rank FROM photos_fts WHERE photos_fts MATCH '{}' ORDER BY rank LIMIT 50;",
            escape_sql(query)
        );

        let output = Command::new("sqlite3")
            .arg(&self.db_path)
            .arg("-separator")
            .arg("\t")
            .arg(&fts_query)
            .output()?;

        if !output.status.success() {
            let err_msg = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("FTS5 search failed: {}", err_msg));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut results = Vec::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let photo_id = parts[0].trim().to_string();
                let filename = parts[1].trim().to_string();
                let rank: f64 = parts[2].trim().parse().unwrap_or(0.0);
                results.push(SearchResult {
                    photo_id,
                    filename,
                    rank,
                });
            }
        }

        Ok(results)
    }

    /// Retrieves the raw 512-D float embedding vector for a given photo.
    pub fn get_embedding(&self, photo_id: &str) -> Option<Vec<f32>> {
        let sql = format!(
            "SELECT quote(embedding) FROM photo_embeddings WHERE photo_id = '{}';",
            escape_sql(photo_id)
        );

        let output = Command::new("sqlite3")
            .arg(&self.db_path)
            .arg(&sql)
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let hex_str = raw.strip_prefix("X'")?.strip_suffix('\'')?;
        decode_hex_to_floats(hex_str)
    }

    /// Finds top-K similar photos using Cosine Similarity over stored embeddings.
    pub fn find_similar(&self, target_id: &str, limit: usize) -> anyhow::Result<Vec<SimilarityResult>> {
        let target_emb = self
            .get_embedding(target_id)
            .ok_or_else(|| anyhow::anyhow!("Embedding not found for photo_id: {}", target_id))?;

        let sql = format!(
            "SELECT photo_id, quote(embedding) FROM photo_embeddings WHERE photo_id != '{}';",
            escape_sql(target_id)
        );

        let output = Command::new("sqlite3")
            .arg(&self.db_path)
            .arg("-separator")
            .arg("\t")
            .arg(&sql)
            .output()?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to query embeddings"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut candidates = Vec::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                let photo_id = parts[0].trim().to_string();
                let raw_hex = parts[1].trim();
                if let Some(hex_str) = raw_hex.strip_prefix("X'").and_then(|s| s.strip_suffix('\'')) {
                    if let Some(emb) = decode_hex_to_floats(hex_str) {
                        let score = cosine_similarity(&target_emb, &emb);
                        candidates.push(SimilarityResult { photo_id, score });
                    }
                }
            }
        }

        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(limit);

        Ok(candidates)
    }
}

fn non_empty_opt(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn decode_hex_to_floats(hex: &str) -> Option<Vec<f32>> {
    if hex.len() % 8 != 0 {
        return None;
    }
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect();

    if bytes.len() % 4 != 0 {
        return None;
    }

    let floats: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    Some(floats)
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a > 0.0 && norm_b > 0.0 {
        dot / (norm_a * norm_b)
    } else {
        0.0
    }
}

fn escape_sql(s: &str) -> String {
    s.replace('\'', "''")
}
