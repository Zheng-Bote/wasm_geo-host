---
feature: embedded-web-ui
status: archived
archive-after:
touches:
  - "crates/host/**"
release: 0.2.0
---

# Tech Spec — embedded-web-ui (`embedded-web-ui`)

Setzt das PRD `docs/prd/embedded-web-ui.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern.

## Architektur & Komponenten

Die Web-UI- und API-Erweiterung integriert einen eingebetteten Webserver direkt in `flagme-host`:

```text
+-------------------------------------------------------------------------+
|                  flagme-host serve [--port 8080]                        |
+------------------------------------+------------------------------------+
                                     |
                                     v
+-------------------------------------------------------------------------+
|                        Axum Web Router & Server                         |
|  - Lifecycle Management & Graceful Shutdown (AC-1)                      |
|  - CORS & Compression via tower-http                                    |
+------------------------------------+------------------------------------+
       |                             |                             |
       v                             v                             v
+------------------+       +------------------+       +-------------------+
|  REST Photo API  |       |  GeoJSON API     |       |  Embedded Web UI  |
| - /api/photos    |       | - /api/map/      |       | - rust-embed      |
| - /api/photos/:id|       |   markers        |       | - Leaflet Map     |
| - /api/photos/   |       | - Point Features |       | - Photo Grid      |
|   :id/thumbnail  |       |   (AC-3)         |       |   (AC-4)          |
|   /:size (AC-2)  |       +------------------+       +-------------------+
+------------------+
       |
       v
+-------------------------------------------------------------------------+
|                         Storage (SQLite media.db)                       |
|  - Paginierte Abfragen, Dokumentensuche, FTS5 & Location-Filter         |
+-------------------------------------------------------------------------+
```

### 1. Embedded Webserver Lifecycle (`axum`) (AC-1)
- Initialisierung eines `axum::Router` mit konfigurierten API-Routen und Static-File-Handler.
- Binding an `SocketAddr` (Default: `0.0.0.0:8080` oder via CLI `--bind` und `--port`).
- Erfüllt **AC-1**: Starten und Verwalten des embedded Webservers ohne externe Abhängigkeiten.

### 2. REST Media Endpoints (AC-2)
- `GET /api/photos?page=1&limit=50&country=DE`: Paginierte Abfrage von Fotos mit Filtern.
- `GET /api/photos/:id`: Rückgabe des vollständigen `UnifiedDocument` als JSON.
- `GET /api/photos/:id/thumbnail/:size`: Rückgabe der Bildbytes des Thumbnails mit `Cache-Control: public, max-age=86400` und `ETag`.
- `GET /api/search?q=:query`: FTS5-Volltextabfrage via `Storage::search`.
- Erfüllt **AC-2**: Bereitstellung strukturierter Metadaten und Mediendateien.

### 3. GeoJSON Map API (AC-3)
- `GET /api/map/markers`: Aggregiert alle Fotos mit `location.latitude` und `location.longitude`.
- Gibt ein GeoJSON-Objekt zurück:
  ```json
  {
    "type": "FeatureCollection",
    "features": [
      {
        "type": "Feature",
        "geometry": {
          "type": "Point",
          "coordinates": [longitude, latitude]
        },
        "properties": {
          "id": "photo-id",
          "filename": "img.jpg",
          "city": "Berlin",
          "country": "Germany",
          "flag": "🇩🇪",
          "thumbnail_url": "/api/photos/photo-id/thumbnail/small"
        }
      }
    ]
  }
  ```
- Erfüllt **AC-3**: Standardkonforme GeoJSON Point Features für Kartenvisualisierungen.

### 4. Einkompiliertes Web Dashboard (`rust-embed`) (AC-4)
- Web-Assets (`crates/host/ui/index.html`, etc.) werden via `#[derive(RustEmbed)]` zur Compile-Zeit in die Binary eingebunden.
- Enthält HTML5, Tailwind CSS via CDN, Leaflet.js für interaktives Karten-Rendering und modales Detail-Popup.
- Erfüllt **AC-4**: Vollwertiges, installationsfreies Dashboard im Browser.
