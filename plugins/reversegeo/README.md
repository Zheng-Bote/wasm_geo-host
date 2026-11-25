# ReverseGeo Plugin (`reversegeo.wasm`)

## Purpose
Resolves geographic latitude and longitude coordinates to country, region, county, and city.

## Architecture
- **Offline Geocoder**: Fast, local spatial indexing using Natural Earth and OSM boundary polygons.
- **Online Geocoder**: Outbound HTTP requests to geocoding providers when coordinate is outside offline boundaries.
- **Cache**: Fast KV store cache to avoid redundant network or polygon operations.

## Capabilities Required
- `http`: Outbound access to geocoding APIs.
- `kv`: Key-value cache storage.
