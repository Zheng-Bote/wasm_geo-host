# Feature Spec: Web-UI & REST/GeoJSON API (`flagme-host serve`)

- **Feature Name**: `web-ui-map-dashboard`
- **Component**: `crates/host`
- **Issue Reference**: `data/ideen.md` Punkt 2.4
- **Issue Type**: Platform Feature / User Interface

---

## 1. Problem Statement & Motivation
Currently, verifying processing results requires CLI commands or manual SQLite inspection (`sqlite3 media.db`). Users lack an out-of-the-box visual interface to explore media on an interactive map, browse photo thumbnails, and filter by tags, country flags, and capture dates.

A lightweight embedded web server provides an instant web UI and REST API without requiring external web servers or frameworks.

---

## 2. Technical Specification

### 2.1 Embedded Web Server (`flagme-host serve`)
Add an embedded web server built with `axum` and `tokio`:
```bash
flagme-host serve --config config.yaml --port 8080 --bind 0.0.0.0
```

### 2.2 REST & GeoJSON Endpoints
- `GET /api/photos`: Paginated photo list with filtering (by country, camera, date range, AI tag).
- `GET /api/photos/{id}`: Detailed `UnifiedDocument` JSON.
- `GET /api/photos/{id}/thumbnail/{size}`: Serves thumbnail image files directly with HTTP caching (`ETag`, `Cache-Control`).
- `GET /api/map/markers`: GeoJSON FeatureCollection of all photos with GPS coordinates for instant mapping.
- `GET /api/search?q={query}`: Full-text search endpoint querying the FTS5 index.

### 2.3 Minimal Embedded Web Dashboard
Embed a self-contained Single Page Application (HTML + Tailwind + Leaflet.js / MapLibre) via `rust-embed`:
- **Interactive Map View**: Displays photo markers clustered on a world map; clicking a marker shows thumbnail and EXIF details.
- **Gallery Grid View**: Responsive thumbnail grid with country flag badges and search bar.

---

## 3. Acceptance Criteria
- [ ] **AC-1**: CLI subcommand `flagme-host serve` starts the Axum server on the configured port.
- [ ] **AC-2**: REST endpoints return paginated photos, document details, and cached thumbnails.
- [ ] **AC-3**: GeoJSON endpoint emits valid Point features for mapped photos.
- [ ] **AC-4**: Embedded web dashboard renders an interactive world map and photo grid in standard browsers.
