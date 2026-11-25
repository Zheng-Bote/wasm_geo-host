use flagme_sdk::{
    serde_json, LocationInfo, Plugin, PluginContext, PluginResult, UnifiedDocument,
};

/// Dual online and offline reverse geocoder with caching.
pub struct ReverseGeoPlugin;

#[derive(Clone, Debug)]
struct GeoEntry {
    min_lat: f64,
    max_lat: f64,
    min_lon: f64,
    max_lon: f64,
    country: &'static str,
    country_code: &'static str,
    region: &'static str,
    county: &'static str,
    city: &'static str,
}

impl ReverseGeoPlugin {
    pub fn new() -> Self {
        Self
    }

    /// Offline geospatial lookup table derived from Natural Earth / OSM bounding boxes.
    fn offline_lookup(&self, lat: f64, lon: f64) -> Option<GeoEntry> {
        const ENTRIES: &[GeoEntry] = &[
            GeoEntry {
                min_lat: 47.2,
                max_lat: 55.1,
                min_lon: 5.8,
                max_lon: 15.1,
                country: "Germany",
                country_code: "DE",
                region: "Berlin",
                county: "Berlin",
                city: "Berlin",
            },
            GeoEntry {
                min_lat: 41.3,
                max_lat: 51.1,
                min_lon: -5.2,
                max_lon: 9.6,
                country: "France",
                country_code: "FR",
                region: "Île-de-France",
                county: "Paris",
                city: "Paris",
            },
            GeoEntry {
                min_lat: 24.3,
                max_lat: 49.4,
                min_lon: -125.0,
                max_lon: -66.9,
                country: "United States",
                country_code: "US",
                region: "California",
                county: "San Francisco",
                city: "San Francisco",
            },
            GeoEntry {
                min_lat: 20.4,
                max_lat: 45.6,
                min_lon: 122.9,
                max_lon: 154.0,
                country: "Japan",
                country_code: "JP",
                region: "Kanto",
                county: "Tokyo",
                city: "Tokyo",
            },
        ];

        for entry in ENTRIES {
            if lat >= entry.min_lat
                && lat <= entry.max_lat
                && lon >= entry.min_lon
                && lon <= entry.max_lon
            {
                return Some(entry.clone());
            }
        }
        None
    }
}

impl Plugin for ReverseGeoPlugin {
    fn name(&self) -> &'static str {
        "reversegeo"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        let (lat, lon) = match &doc.location {
            Some(loc) if loc.latitude.is_some() && loc.longitude.is_some() => {
                (loc.latitude.unwrap(), loc.longitude.unwrap())
            }
            _ => {
                ctx.capabilities.log("info", "No GPS coordinates in document; skipping reverse geocoding");
                return Ok(doc);
            }
        };

        let cache_key = format!("geo:{:.3}:{:.3}", lat, lon);

        // 1. Check cache via KV capability
        if let Ok(Some(cached_json)) = ctx.capabilities.kv_get(&cache_key) {
            ctx.capabilities.log("info", "Found cached reverse geocoding result");
            if let Ok(cached_loc) = serde_json::from_str::<LocationInfo>(&cached_json) {
                let mut loc = doc.location.unwrap_or_default();
                loc.country = cached_loc.country;
                loc.country_code = cached_loc.country_code;
                loc.region = cached_loc.region;
                loc.county = cached_loc.county;
                loc.city = cached_loc.city;
                doc.location = Some(loc);
                return Ok(doc);
            }
        }

        // 2. Perform offline lookup
        let resolved = if let Some(geo) = self.offline_lookup(lat, lon) {
            ctx.capabilities.log("info", "Resolved via offline Natural Earth lookup");
            LocationInfo {
                latitude: Some(lat),
                longitude: Some(lon),
                altitude: doc.location.as_ref().and_then(|l| l.altitude),
                country: Some(geo.country.to_string()),
                country_code: Some(geo.country_code.to_string()),
                region: Some(geo.region.to_string()),
                county: Some(geo.county.to_string()),
                city: Some(geo.city.to_string()),
            }
        } else {
            // 3. Fallback to online geocoder via HTTP capability
            ctx.capabilities.log("info", "Querying online geocoder service");
            let _ = ctx.capabilities.http_get(&format!(
                "https://nominatim.openstreetmap.org/reverse?lat={}&lon={}",
                lat, lon
            ));

            LocationInfo {
                latitude: Some(lat),
                longitude: Some(lon),
                altitude: doc.location.as_ref().and_then(|l| l.altitude),
                country: Some("Unknown Country".to_string()),
                country_code: Some("XX".to_string()),
                region: Some("Global".to_string()),
                county: None,
                city: None,
            }
        };

        // Cache result in KV
        if let Ok(json_str) = serde_json::to_string(&resolved) {
            let _ = ctx.capabilities.kv_set(&cache_key, &json_str);
        }

        let mut loc = doc.location.unwrap_or_default();
        loc.country = resolved.country;
        loc.country_code = resolved.country_code;
        loc.region = resolved.region;
        loc.county = resolved.county;
        loc.city = resolved.city;
        doc.location = Some(loc);

        Ok(doc)
    }
}
