-- Database schema for WASM Geo Host

CREATE TABLE IF NOT EXISTS photos (
    id TEXT PRIMARY KEY,
    file_path TEXT NOT NULL,
    filename TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    mime_type TEXT NOT NULL,
    sha256 TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS metadata (
    photo_id TEXT PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
    camera_make TEXT,
    camera_model TEXT,
    lens_model TEXT,
    iso INTEGER,
    focal_length REAL,
    f_number REAL,
    exposure_time TEXT,
    raw_tags JSON NOT NULL
);

CREATE TABLE IF NOT EXISTS locations (
    photo_id TEXT PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
    latitude REAL,
    longitude REAL,
    altitude REAL,
    country TEXT,
    country_code TEXT,
    region TEXT,
    county TEXT,
    city TEXT,
    local_country TEXT,
    local_region TEXT,
    local_city TEXT,
    local_language TEXT
);

CREATE TABLE IF NOT EXISTS countries (
    country_code TEXT PRIMARY KEY,
    country_name TEXT NOT NULL,
    flag_emoji TEXT NOT NULL,
    continent TEXT NOT NULL,
    region TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS thumbnails (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    photo_id TEXT NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    size_label TEXT NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    file_path TEXT NOT NULL,
    cache_key TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS ai_tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    photo_id TEXT NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    confidence REAL NOT NULL,
    model TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS duplicates (
    photo_id TEXT PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
    sha256 TEXT NOT NULL,
    phash TEXT NOT NULL,
    duplicate_ids JSON NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_photos_sha256 ON photos(sha256);
CREATE INDEX IF NOT EXISTS idx_duplicates_phash ON duplicates(phash);
CREATE INDEX IF NOT EXISTS idx_locations_coords ON locations(latitude, longitude);

-- Full-Text Search (FTS5) virtual table
CREATE VIRTUAL TABLE IF NOT EXISTS photos_fts USING fts5(
    photo_id UNINDEXED,
    filename,
    camera_make,
    camera_model,
    country,
    local_country,
    city,
    local_city,
    ai_tags,
    description,
    raw_tags,
    ocr_text
);

-- Dense Vector Embeddings (512-dim Float32 BLOBs)
CREATE TABLE IF NOT EXISTS photo_embeddings (
    photo_id TEXT PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
    embedding BLOB NOT NULL,
    model TEXT NOT NULL
);

-- Historical Meteorological Weather Conditions
CREATE TABLE IF NOT EXISTS weather (
    photo_id TEXT PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
    temperature REAL,
    condition TEXT NOT NULL,
    wmo_code INTEGER NOT NULL,
    icon_emoji TEXT NOT NULL,
    humidity_percent INTEGER,
    precipitation_mm REAL,
    is_day INTEGER
);

CREATE INDEX IF NOT EXISTS idx_weather_condition ON weather(condition);
CREATE INDEX IF NOT EXISTS idx_weather_temp ON weather(temperature);

-- OCR Optical Character Recognition Texts & Regions
CREATE TABLE IF NOT EXISTS ocr_texts (
    photo_id TEXT PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
    combined_text TEXT NOT NULL,
    primary_language TEXT
);

CREATE TABLE IF NOT EXISTS ocr_regions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    photo_id TEXT NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    text TEXT NOT NULL,
    confidence REAL NOT NULL,
    bbox_x REAL NOT NULL,
    bbox_y REAL NOT NULL,
    bbox_w REAL NOT NULL,
    bbox_h REAL NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ocr_regions_photo ON ocr_regions(photo_id);



