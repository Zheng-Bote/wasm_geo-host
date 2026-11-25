---
feature: embedded-web-ui
status: draft
archive-after:
touches:
  - "crates/host/**"
---

# PRD — embedded-web-ui (`embedded-web-ui`)

## Problem

Bislang erfordert die Einsicht und Verifikation von Verarbeitungsergebnissen der Host-Pipeline manuelle CLI-Befehle (`flagme-host search`, `flagme-host similar`) oder direkte Datenbankabfragen (`sqlite3 media.db`). Endanwendern und Entwicklern fehlt eine integrierte, visuelle Benutzeroberfläche, um verarbeitete Medien auf einer interaktiven Weltkarte zu explorieren, Vorschaubilder (Thumbnails) zu durchsuchen und Filter nach Ländern, Kameratypen und Aufnahmezeiträumen zu setzen. Zudem existiert keine standardisierte REST- oder GeoJSON-Schnittstelle zur Einbindung externer Map-Clients oder Web-Frontends.

## Ziel

Bereitstellung eines leichtgewichtigen, eingebetteten HTTP-Webservers (`flagme-host serve`) und eines in das Binary einkompilierten Web-Dashboards auf Basis von `axum` und `rust-embed`:
1. Embedded Webserver mit konfigurierbarem Port und Bind-Adresse.
2. Vollständige REST-API für paginierte Fotoabfragen, Dokument-Details (`UnifiedDocument`) und HTTP-Caching für Vorschaubilder.
3. Standardisierte GeoJSON-Schnittstelle (`/api/map/markers`), die Point-Features mit GPS-Koordinaten und Foto-Metadaten für Karten bereitstellt.
4. Selbständiges, eingebettetes Single-Page-Web-Dashboard mit interaktiver Leaflet/MapLibre-Kartenansicht und Galerie-Raster inklusive Volltextsuche.

## Nutzer

- Endanwender und Fotografen, die ihre Medienbibliothek komfortabel im Browser über Karte und Galerie ansehen möchten.
- Entwickler und Integratoren, die Metadaten und Geokoordinaten über REST und GeoJSON abrufen wollen.

## Umfang

Die Umsetzung umfasst:
- Embedded HTTP-Server mit `axum` und `tower-http` zur Bereitstellung von Web-Endpunkten.
- REST-Endpunkte:
  - `GET /api/photos`: Paginierte Liste von Fotos mit Filteroptionen (z. B. Land, Tag, Suchbegriff).
  - `GET /api/photos/{id}`: Detailabfrage des gesamten `UnifiedDocument`.
  - `GET /api/photos/{id}/thumbnail/{size}`: Bereitstellung von Thumbnails mit Caching-Headern (`Cache-Control`, `ETag`).
  - `GET /api/map/markers`: GeoJSON `FeatureCollection` aller Fotos mit gültigen Geokoordinaten.
  - `GET /api/search?q={query}`: Volltextsuche über die FTS5-Indexierung.
- Embedded Web-Dashboard via `rust-embed` (HTML, CSS, JavaScript, Leaflet-Map, Galerie-Grid).
- CLI-Subcommand `flagme-host serve [--config <CONFIG>] [--port <PORT>] [--bind <ADDR>]`.
- Integrationstests je Kriterium mit Kennzeichnung.

## Nicht-Ziele

- Keine Cloud-Hosting-Infrastruktur oder Multi-User-Authentifizierung (Single-User Home/Local Server Scope).
- Kein externes Node.js Build-System zur Laufzeit (alle Web-Assets liegen einkompiliert im Host-Binary vor).

## Akzeptanzkriterien

- **AC-1** Embedded Server Lifecycle: CLI-Subcommand `flagme-host serve` startet den Axum-Webserver auf der konfigurierten Bind-Adresse und dem Port.
- **AC-2** REST Media Endpoints: Die REST-API liefert paginierte Fotolisten, vollständige Dokument-Details und Thumbnails mit Caching-Headern aus.
- **AC-3** GeoJSON Map API: Der Endpunkt `/api/map/markers` gibt eine standardkonforme GeoJSON-`FeatureCollection` aller verorteten Fotos zurück.
- **AC-4** Embedded Web Dashboard: Ein einkompiliertes Web-Dashboard rendert eine interaktive Kartenansicht und Galerie-Raster im Browser.
