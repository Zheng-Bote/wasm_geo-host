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
    raw_tags
);

