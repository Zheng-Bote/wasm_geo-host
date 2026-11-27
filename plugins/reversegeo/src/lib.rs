use flagme_sdk::{
    serde_json, LocationInfo, Plugin, PluginContext, PluginResult, PluginSettings, SettingsCache,
    SettingsReader, UnifiedDocument,
};

pub mod gazetteer;

use gazetteer::{Gazetteer, PLACES};

/// Configurable parameters of the reverse geocoder (`config.yaml` `settings`).
#[derive(Debug, Clone, PartialEq)]
pub struct ReverseGeoSettings {
    /// Read and write results in the host KV cache.
    pub offline_cache: bool,
    /// Use the embedded city table and country boxes.
    pub offline_lookup: bool,
    /// Query the online geocoder when the offline lookup finds nothing.
    pub online_lookup: bool,
    /// Maximum distance in kilometres from the photo to the nearest place of the embedded
    /// GeoNames gazetteer (ADR 0006); farther places are not used.
    pub city_radius_km: u32,
    /// Decimal places of the coordinates in the cache key.
    pub cache_precision: u32,
    /// Base URL of the Nominatim reverse endpoint.
    pub nominatim_url: String,
    /// `accept-language` of the online request.
    pub accept_language: String,
}

impl Default for ReverseGeoSettings {
    fn default() -> Self {
        Self {
            offline_cache: true,
            offline_lookup: true,
            online_lookup: true,
            city_radius_km: 25,
            cache_precision: 3,
            nominatim_url: "https://nominatim.openstreetmap.org/reverse".to_string(),
            accept_language: "en".to_string(),
        }
    }
}

impl PluginSettings for ReverseGeoSettings {
    const PLUGIN: &'static str = "reversegeo";

    fn from_reader(base: &Self, r: &mut SettingsReader<'_>) -> Self {
        Self {
            offline_cache: r.bool("offline_cache", &[], base.offline_cache),
            offline_lookup: r.bool("offline_lookup", &[], base.offline_lookup),
            online_lookup: r.bool("online_lookup", &[], base.online_lookup),
            city_radius_km: r.u32("city_radius_km", &[], 1..=200, base.city_radius_km),
            cache_precision: r.u32("cache_precision", &[], 0..=6, base.cache_precision),
            nominatim_url: r.url("nominatim_url", &[], &base.nominatim_url),
            accept_language: r.string("accept_language", &[], &base.accept_language),
        }
    }
}

/// Dual online and offline reverse geocoder with caching.
pub struct ReverseGeoPlugin {
    settings: SettingsCache<ReverseGeoSettings>,
}


impl ReverseGeoPlugin {
    pub fn new() -> Self {
        Self::with_settings(ReverseGeoSettings::default())
    }

    pub fn with_settings(settings: ReverseGeoSettings) -> Self {
        Self {
            settings: SettingsCache::new(settings),
        }
    }

    /// Parses a Nominatim JSON response if online HTTP lookup succeeded.
    fn parse_nominatim_response(
        &self,
        body: &str,
        lat: f64,
        lon: f64,
        alt: Option<f64>,
        language: &str,
    ) -> Option<LocationInfo> {
        let val: serde_json::Value = serde_json::from_str(body).ok()?;
        let addr = val.get("address")?;

        let country = addr.get("country").and_then(|v| v.as_str())?.to_string();
        let country_code = addr
            .get("country_code")
            .and_then(|v| v.as_str())
            .map(|s| s.to_uppercase())?;

        let city = addr
            .get("city")
            .or_else(|| addr.get("town"))
            .or_else(|| addr.get("village"))
            .or_else(|| addr.get("municipality"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let region = addr
            .get("state")
            .or_else(|| addr.get("region"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| city.clone());

        let county = addr
            .get("county")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| city.clone());

        Some(LocationInfo {
            latitude: Some(lat),
            longitude: Some(lon),
            altitude: alt,
            country: Some(country.clone()),
            country_code: Some(country_code),
            region: region.clone(),
            county,
            city: city.clone(),
            local_country: Some(country),
            local_region: region,
            local_city: city,
            local_language: Some(language.to_string()),
        })
    }

    /// Offline lookup in the embedded GeoNames gazetteer (ADR 0006): the nearest place within
    /// `radius_km` with its own division and country names. Returns the location and the
    /// distance in km, `Ok(None)` if no place is close enough, `Err` if the data is unusable.
    fn offline_lookup(
        &self,
        lat: f64,
        lon: f64,
        alt: Option<f64>,
        radius_km: u32,
    ) -> Result<Option<(LocationInfo, f64)>, gazetteer::FormatError> {
        let g = Gazetteer::parse(PLACES)?;
        let Some((place, km)) = g.nearest(lat, lon, radius_km as f64) else {
            return Ok(None);
        };
        let s = |v: Option<&str>| v.map(str::to_string);
        Ok(Some((
            LocationInfo {
                latitude: Some(lat),
                longitude: Some(lon),
                altitude: alt,
                country: s(place.country_name()),
                country_code: s(place.country_code()),
                region: s(place.region()),
                county: s(place.county()),
                city: s(place.city()),
                local_country: s(place.local_country()),
                local_region: s(place.local_region()),
                local_city: s(place.local_city()),
                local_language: s(place.local_language()),
            },
            km,
        )))
    }
}

impl Plugin for ReverseGeoPlugin {
    fn name(&self) -> &'static str {
        "reversegeo"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn process(
        &self,
        mut doc: UnifiedDocument,
        ctx: &PluginContext,
    ) -> PluginResult<UnifiedDocument> {
        let (lat, lon, alt) = match &doc.location {
            Some(loc) if loc.latitude.is_some() && loc.longitude.is_some() => {
                (loc.latitude.unwrap(), loc.longitude.unwrap(), loc.altitude)
            }
            _ => {
                ctx.capabilities.log(
                    "info",
                    "No GPS coordinates in document; skipping reverse geocoding",
                );
                return Ok(doc);
            }
        };

        let settings = self.settings.get_or_parse(ctx);
        let precision = settings.cache_precision as usize;
        let cache_key = format!("geo:{:.p$}:{:.p$}", lat, lon, p = precision);

        // 1. Check cache via KV capability
        let cached = if settings.offline_cache {
            ctx.capabilities.kv_get(&cache_key).ok().flatten()
        } else {
            None
        };
        if let Some(cached_json) = cached {
            ctx.capabilities
                .log("info", "Found cached reverse geocoding result");
            if let Ok(cached_loc) = serde_json::from_str::<LocationInfo>(&cached_json) {
                let mut loc = doc.location.unwrap_or_default();
                loc.country = cached_loc.country;
                loc.country_code = cached_loc.country_code;
                loc.region = cached_loc.region;
                loc.county = cached_loc.county;
                loc.city = cached_loc.city;
                loc.local_country = cached_loc.local_country;
                loc.local_region = cached_loc.local_region;
                loc.local_city = cached_loc.local_city;
                loc.local_language = cached_loc.local_language;
                doc.location = Some(loc);
                return Ok(doc);
            }
        }

        // 2. Perform offline lookup first for fast deterministic resolution
        let offline = if settings.offline_lookup {
            match self.offline_lookup(lat, lon, alt, settings.city_radius_km) {
                Ok(found) => found,
                Err(e) => {
                    ctx.capabilities.log("warn", &format!("reversegeo: offline data unusable: {}", e));
                    None
                }
            }
        } else {
            None
        };
        let resolved = if let Some((geo, km)) = offline {
            ctx.capabilities.log(
                "info",
                &format!(
                    "Resolved via offline GeoNames lookup ({:.1} km to {})",
                    km,
                    geo.city.as_deref().unwrap_or("?")
                ),
            );
            geo
        } else {
            // 3. Fallback to online geocoder via HTTP capability. Without a result the names stay
            //    empty and nothing is cached; no placeholder location is invented (#52, ADR 0002).
            let unresolved = |cause: String| {
                ctx.capabilities.log(
                    "warn",
                    &format!(
                        "reversegeo: no location names for '{}' ({}, {}): {}",
                        doc.file.path, lat, lon, cause
                    ),
                );
            };
            if !settings.online_lookup {
                unresolved(format!(
                    "no place within city_radius_km ({} km) in the offline data and online_lookup is off",
                    settings.city_radius_km
                ));
                return Ok(doc);
            }
            ctx.capabilities
                .log("info", "Querying online geocoder service");
            let url = format!(
                "{}?format=jsonv2&lat={}&lon={}&accept-language={}",
                settings.nominatim_url, lat, lon, settings.accept_language
            );
            match ctx.capabilities.http_get(&url) {
                Ok(body) => match self.parse_nominatim_response(&body, lat, lon, alt, &settings.accept_language) {
                    Some(geo) => geo,
                    None => {
                        unresolved("unreadable geocoder response".to_string());
                        return Ok(doc);
                    }
                },
                Err(e) => {
                    unresolved(e.to_string());
                    return Ok(doc);
                }
            }
        };

        // Cache result in KV
        if settings.offline_cache {
            if let Ok(json_str) = serde_json::to_string(&resolved) {
                let _ = ctx.capabilities.kv_set(&cache_key, &json_str);
            }
        }

        let mut loc = doc.location.unwrap_or_default();
        loc.country = resolved.country;
        loc.country_code = resolved.country_code;
        loc.region = resolved.region;
        loc.county = resolved.county;
        loc.city = resolved.city;
        loc.local_country = resolved.local_country;
        loc.local_region = resolved.local_region;
        loc.local_city = resolved.local_city;
        loc.local_language = resolved.local_language;
        doc.location = Some(loc);

        Ok(doc)
    }
}

flagme_sdk::export_plugin!(ReverseGeoPlugin, ReverseGeoPlugin::new);
