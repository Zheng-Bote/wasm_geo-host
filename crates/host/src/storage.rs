use wasm_geo_models::*;
use std::collections::HashMap;
use std::io::Write;
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
                "INSERT OR REPLACE INTO locations (photo_id, latitude, longitude, altitude, country, country_code, region, county, city, local_country, local_region, local_city, local_language) VALUES ('{}', {}, {}, {}, '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}');\n",
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
                escape_sql(loc.local_language.as_deref().unwrap_or(""))
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
        let description = doc.custom.get("description").and_then(|v| v.as_str()).unwrap_or("");

        sql.push_str(&format!(
            "DELETE FROM photos_fts WHERE photo_id = '{}';\n",
            escape_sql(&doc.id)
        ));
        sql.push_str(&format!(
            "INSERT INTO photos_fts (photo_id, filename, camera_make, camera_model, country, local_country, city, local_city, ai_tags, description, raw_tags) VALUES ('{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}');\n",
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
            escape_sql(&raw_tags_str)
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
                let model = ai.tags.first().map(|t| t.model.as_str()).unwrap_or("CLIP");
                sql.push_str(&format!(
                    "INSERT OR REPLACE INTO photo_embeddings (photo_id, embedding, model) VALUES ('{}', X'{}', '{}');\n",
                    escape_sql(&doc.id),
                    blob_hex,
                    escape_sql(model)
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
        // Format from quote(blob) is X'010203...'
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

        // Sort descending by similarity score
        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(limit);

        Ok(candidates)
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
