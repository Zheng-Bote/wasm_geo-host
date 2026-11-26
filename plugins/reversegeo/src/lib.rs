use flagme_sdk::{
    serde_json, LocationInfo, Plugin, PluginContext, PluginResult, PluginSettings, SettingsCache,
    SettingsReader, UnifiedDocument,
};

/// Kilometres per degree used for the city search radius. Chosen so that the default of
/// 200 km equals the previous fixed radius of 1.8° (≈ 111.1 km per degree).
const KM_PER_DEGREE: f64 = 200.0 / 1.8;

/// Configurable parameters of the reverse geocoder (`config.yaml` `settings`).
#[derive(Debug, Clone, PartialEq)]
pub struct ReverseGeoSettings {
    /// Read and write results in the host KV cache.
    pub offline_cache: bool,
    /// Use the embedded city table and country boxes.
    pub offline_lookup: bool,
    /// Query the online geocoder when the offline lookup finds nothing.
    pub online_lookup: bool,
    /// Search radius around the reference cities in kilometres.
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
            city_radius_km: 200,
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
            city_radius_km: r.u32("city_radius_km", &[], 1..=2000, base.city_radius_km),
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

#[derive(Clone, Debug)]
struct CityRef {
    lat: f64,
    lon: f64,
    country: &'static str,
    country_code: &'static str,
    region: &'static str,
    county: &'static str,
    city: &'static str,
    local_country: &'static str,
    local_region: &'static str,
    local_city: &'static str,
    local_language: &'static str,
}

#[derive(Clone, Debug)]
struct CountryBox {
    min_lat: f64,
    max_lat: f64,
    min_lon: f64,
    max_lon: f64,
    country: &'static str,
    country_code: &'static str,
    region: &'static str,
    city: &'static str,
    local_country: &'static str,
    local_region: &'static str,
    local_city: &'static str,
    local_language: &'static str,
}

const CITIES: &[CityRef] = &[
    // Germany
    CityRef {
        lat: 52.5200,
        lon: 13.4050,
        country: "Germany",
        country_code: "DE",
        region: "Berlin",
        county: "Berlin",
        city: "Berlin",
        local_country: "Deutschland",
        local_region: "Berlin",
        local_city: "Berlin",
        local_language: "de",
    },
    CityRef {
        lat: 48.1351,
        lon: 11.5820,
        country: "Germany",
        country_code: "DE",
        region: "Bavaria",
        county: "Munich",
        city: "Munich",
        local_country: "Deutschland",
        local_region: "Bayern",
        local_city: "München",
        local_language: "de",
    },
    CityRef {
        lat: 53.5511,
        lon: 9.9937,
        country: "Germany",
        country_code: "DE",
        region: "Hamburg",
        county: "Hamburg",
        city: "Hamburg",
        local_country: "Deutschland",
        local_region: "Hamburg",
        local_city: "Hamburg",
        local_language: "de",
    },
    CityRef {
        lat: 50.9375,
        lon: 6.9603,
        country: "Germany",
        country_code: "DE",
        region: "North Rhine-Westphalia",
        county: "Cologne",
        city: "Cologne",
        local_country: "Deutschland",
        local_region: "Nordrhein-Westfalen",
        local_city: "Köln",
        local_language: "de",
    },
    CityRef {
        lat: 50.1109,
        lon: 8.6821,
        country: "Germany",
        country_code: "DE",
        region: "Hesse",
        county: "Frankfurt",
        city: "Frankfurt",
        local_country: "Deutschland",
        local_region: "Hessen",
        local_city: "Frankfurt am Main",
        local_language: "de",
    },
    CityRef {
        lat: 48.7758,
        lon: 9.1829,
        country: "Germany",
        country_code: "DE",
        region: "Baden-Württemberg",
        county: "Stuttgart",
        city: "Stuttgart",
        local_country: "Deutschland",
        local_region: "Baden-Württemberg",
        local_city: "Stuttgart",
        local_language: "de",
    },
    CityRef {
        lat: 51.2277,
        lon: 6.7735,
        country: "Germany",
        country_code: "DE",
        region: "North Rhine-Westphalia",
        county: "Düsseldorf",
        city: "Düsseldorf",
        local_country: "Deutschland",
        local_region: "Nordrhein-Westfalen",
        local_city: "Düsseldorf",
        local_language: "de",
    },
    CityRef {
        lat: 51.3397,
        lon: 12.3731,
        country: "Germany",
        country_code: "DE",
        region: "Saxony",
        county: "Leipzig",
        city: "Leipzig",
        local_country: "Deutschland",
        local_region: "Sachsen",
        local_city: "Leipzig",
        local_language: "de",
    },
    CityRef {
        lat: 51.0504,
        lon: 13.7373,
        country: "Germany",
        country_code: "DE",
        region: "Saxony",
        county: "Dresden",
        city: "Dresden",
        local_country: "Deutschland",
        local_region: "Sachsen",
        local_city: "Dresden",
        local_language: "de",
    },
    // Austria & Switzerland
    CityRef {
        lat: 48.2082,
        lon: 16.3738,
        country: "Austria",
        country_code: "AT",
        region: "Vienna",
        county: "Vienna",
        city: "Vienna",
        local_country: "Österreich",
        local_region: "Wien",
        local_city: "Wien",
        local_language: "de",
    },
    CityRef {
        lat: 47.2692,
        lon: 11.4041,
        country: "Austria",
        country_code: "AT",
        region: "Tyrol",
        county: "Innsbruck",
        city: "Innsbruck",
        local_country: "Österreich",
        local_region: "Tirol",
        local_city: "Innsbruck",
        local_language: "de",
    },
    CityRef {
        lat: 47.3769,
        lon: 8.5417,
        country: "Switzerland",
        country_code: "CH",
        region: "Zurich",
        county: "Zurich",
        city: "Zurich",
        local_country: "Schweiz",
        local_region: "Zürich",
        local_city: "Zürich",
        local_language: "de",
    },
    CityRef {
        lat: 46.2044,
        lon: 6.1432,
        country: "Switzerland",
        country_code: "CH",
        region: "Geneva",
        county: "Geneva",
        city: "Geneva",
        local_country: "Suisse",
        local_region: "Genève",
        local_city: "Genève",
        local_language: "fr",
    },
    // France
    CityRef {
        lat: 48.8566,
        lon: 2.3522,
        country: "France",
        country_code: "FR",
        region: "Île-de-France",
        county: "Paris",
        city: "Paris",
        local_country: "France",
        local_region: "Île-de-France",
        local_city: "Paris",
        local_language: "fr",
    },
    CityRef {
        lat: 45.7640,
        lon: 4.8357,
        country: "France",
        country_code: "FR",
        region: "Auvergne-Rhône-Alpes",
        county: "Lyon",
        city: "Lyon",
        local_country: "France",
        local_region: "Auvergne-Rhône-Alpes",
        local_city: "Lyon",
        local_language: "fr",
    },
    CityRef {
        lat: 43.2965,
        lon: 5.3698,
        country: "France",
        country_code: "FR",
        region: "Provence-Alpes-Côte d'Azur",
        county: "Marseille",
        city: "Marseille",
        local_country: "France",
        local_region: "Provence-Alpes-Côte d'Azur",
        local_city: "Marseille",
        local_language: "fr",
    },
    // Italy
    CityRef {
        lat: 41.9028,
        lon: 12.4964,
        country: "Italy",
        country_code: "IT",
        region: "Lazio",
        county: "Rome",
        city: "Rome",
        local_country: "Italia",
        local_region: "Lazio",
        local_city: "Roma",
        local_language: "it",
    },
    CityRef {
        lat: 45.4642,
        lon: 9.1900,
        country: "Italy",
        country_code: "IT",
        region: "Lombardy",
        county: "Milan",
        city: "Milan",
        local_country: "Italia",
        local_region: "Lombardia",
        local_city: "Milano",
        local_language: "it",
    },
    CityRef {
        lat: 45.4408,
        lon: 12.3155,
        country: "Italy",
        country_code: "IT",
        region: "Veneto",
        county: "Venice",
        city: "Venice",
        local_country: "Italia",
        local_region: "Veneto",
        local_city: "Venezia",
        local_language: "it",
    },
    // Spain & Portugal
    CityRef {
        lat: 40.4168,
        lon: -3.7038,
        country: "Spain",
        country_code: "ES",
        region: "Community of Madrid",
        county: "Madrid",
        city: "Madrid",
        local_country: "España",
        local_region: "Comunidad de Madrid",
        local_city: "Madrid",
        local_language: "es",
    },
    CityRef {
        lat: 41.3874,
        lon: 2.1686,
        country: "Spain",
        country_code: "ES",
        region: "Catalonia",
        county: "Barcelona",
        city: "Barcelona",
        local_country: "España",
        local_region: "Catalunya",
        local_city: "Barcelona",
        local_language: "es",
    },
    CityRef {
        lat: 38.7223,
        lon: -9.1393,
        country: "Portugal",
        country_code: "PT",
        region: "Lisbon",
        county: "Lisbon",
        city: "Lisbon",
        local_country: "Portugal",
        local_region: "Lisboa",
        local_city: "Lisboa",
        local_language: "pt",
    },
    // UK, Netherlands, Nordic
    CityRef {
        lat: 51.5074,
        lon: -0.1278,
        country: "United Kingdom",
        country_code: "GB",
        region: "England",
        county: "Greater London",
        city: "London",
        local_country: "United Kingdom",
        local_region: "England",
        local_city: "London",
        local_language: "en",
    },
    CityRef {
        lat: 52.3676,
        lon: 4.9041,
        country: "Netherlands",
        country_code: "NL",
        region: "North Holland",
        county: "Amsterdam",
        city: "Amsterdam",
        local_country: "Nederland",
        local_region: "Noord-Holland",
        local_city: "Amsterdam",
        local_language: "nl",
    },
    CityRef {
        lat: 59.3293,
        lon: 18.0686,
        country: "Sweden",
        country_code: "SE",
        region: "Stockholm",
        county: "Stockholm",
        city: "Stockholm",
        local_country: "Sverige",
        local_region: "Stockholm",
        local_city: "Stockholm",
        local_language: "sv",
    },
    CityRef {
        lat: 59.9139,
        lon: 10.7522,
        country: "Norway",
        country_code: "NO",
        region: "Oslo",
        county: "Oslo",
        city: "Oslo",
        local_country: "Norge",
        local_region: "Oslo",
        local_city: "Oslo",
        local_language: "no",
    },
    CityRef {
        lat: 55.6761,
        lon: 12.5683,
        country: "Denmark",
        country_code: "DK",
        region: "Capital Region",
        county: "Copenhagen",
        city: "Copenhagen",
        local_country: "Danmark",
        local_region: "Hovedstaden",
        local_city: "København",
        local_language: "da",
    },
    // North America
    CityRef {
        lat: 37.7749,
        lon: -122.4194,
        country: "United States",
        country_code: "US",
        region: "California",
        county: "San Francisco",
        city: "San Francisco",
        local_country: "United States",
        local_region: "California",
        local_city: "San Francisco",
        local_language: "en",
    },
    CityRef {
        lat: 34.0522,
        lon: -118.2437,
        country: "United States",
        country_code: "US",
        region: "California",
        county: "Los Angeles",
        city: "Los Angeles",
        local_country: "United States",
        local_region: "California",
        local_city: "Los Angeles",
        local_language: "en",
    },
    CityRef {
        lat: 40.7128,
        lon: -74.0060,
        country: "United States",
        country_code: "US",
        region: "New York",
        county: "New York",
        city: "New York",
        local_country: "United States",
        local_region: "New York",
        local_city: "New York",
        local_language: "en",
    },
    CityRef {
        lat: 43.6532,
        lon: -79.3832,
        country: "Canada",
        country_code: "CA",
        region: "Ontario",
        county: "Toronto",
        city: "Toronto",
        local_country: "Canada",
        local_region: "Ontario",
        local_city: "Toronto",
        local_language: "en",
    },
    // Asia & Oceania
    CityRef {
        lat: 35.6762,
        lon: 139.6503,
        country: "Japan",
        country_code: "JP",
        region: "Kanto",
        county: "Tokyo",
        city: "Tokyo",
        local_country: "日本",
        local_region: "関東",
        local_city: "東京",
        local_language: "ja",
    },
    CityRef {
        lat: 34.6937,
        lon: 135.5023,
        country: "Japan",
        country_code: "JP",
        region: "Kansai",
        county: "Osaka",
        city: "Osaka",
        local_country: "日本",
        local_region: "関西",
        local_city: "大阪",
        local_language: "ja",
    },
    CityRef {
        lat: -33.8688,
        lon: 151.2093,
        country: "Australia",
        country_code: "AU",
        region: "New South Wales",
        county: "Sydney",
        city: "Sydney",
        local_country: "Australia",
        local_region: "New South Wales",
        local_city: "Sydney",
        local_language: "en",
    },
];

const COUNTRY_BOXES: &[CountryBox] = &[
    CountryBox {
        min_lat: 47.2,
        max_lat: 55.1,
        min_lon: 5.8,
        max_lon: 15.1,
        country: "Germany",
        country_code: "DE",
        region: "Berlin",
        city: "Berlin",
        local_country: "Deutschland",
        local_region: "Berlin",
        local_city: "Berlin",
        local_language: "de",
    },
    CountryBox {
        min_lat: 41.3,
        max_lat: 51.1,
        min_lon: -5.2,
        max_lon: 8.3,
        country: "France",
        country_code: "FR",
        region: "Île-de-France",
        city: "Paris",
        local_country: "France",
        local_region: "Île-de-France",
        local_city: "Paris",
        local_language: "fr",
    },
    CountryBox {
        min_lat: 35.4,
        max_lat: 47.1,
        min_lon: 6.6,
        max_lon: 18.5,
        country: "Italy",
        country_code: "IT",
        region: "Lazio",
        city: "Rome",
        local_country: "Italia",
        local_region: "Lazio",
        local_city: "Roma",
        local_language: "it",
    },
    CountryBox {
        min_lat: 24.3,
        max_lat: 49.4,
        min_lon: -125.0,
        max_lon: -66.9,
        country: "United States",
        country_code: "US",
        region: "California",
        city: "San Francisco",
        local_country: "United States",
        local_region: "California",
        local_city: "San Francisco",
        local_language: "en",
    },
    CountryBox {
        min_lat: 20.4,
        max_lat: 45.6,
        min_lon: 122.9,
        max_lon: 154.0,
        country: "Japan",
        country_code: "JP",
        region: "Kanto",
        city: "Tokyo",
        local_country: "日本",
        local_region: "関東",
        local_city: "東京",
        local_language: "ja",
    },
    CountryBox {
        min_lat: 36.0,
        max_lat: 43.8,
        min_lon: -9.5,
        max_lon: 3.3,
        country: "Spain",
        country_code: "ES",
        region: "Madrid",
        city: "Madrid",
        local_country: "España",
        local_region: "Madrid",
        local_city: "Madrid",
        local_language: "es",
    },
    CountryBox {
        min_lat: 49.9,
        max_lat: 60.9,
        min_lon: -8.6,
        max_lon: 1.8,
        country: "United Kingdom",
        country_code: "GB",
        region: "England",
        city: "London",
        local_country: "United Kingdom",
        local_region: "England",
        local_city: "London",
        local_language: "en",
    },
];

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

    /// Offline geospatial lookup using nearest city within threshold and country bounding boxes.
    fn offline_lookup(
        &self,
        lat: f64,
        lon: f64,
        alt: Option<f64>,
        radius_km: u32,
    ) -> Option<LocationInfo> {
        // 1. Find closest known city within the configured radius (default 200 km = 1.8°)
        let radius_deg = radius_km as f64 / KM_PER_DEGREE;
        let max_dist_sq = radius_deg * radius_deg;
        let mut best_city: Option<(&CityRef, f64)> = None;
        for c in CITIES {
            let dlat = lat - c.lat;
            let dlon = (lon - c.lon) * (lat.to_radians().cos().abs().max(0.2));
            let dist_sq = dlat * dlat + dlon * dlon;
            if dist_sq <= max_dist_sq {
                if best_city.map(|(_, b)| dist_sq < b).unwrap_or(true) {
                    best_city = Some((c, dist_sq));
                }
            }
        }

        if let Some((c, _)) = best_city {
            return Some(LocationInfo {
                latitude: Some(lat),
                longitude: Some(lon),
                altitude: alt,
                country: Some(c.country.to_string()),
                country_code: Some(c.country_code.to_string()),
                region: Some(c.region.to_string()),
                county: Some(c.county.to_string()),
                city: Some(c.city.to_string()),
                local_country: Some(c.local_country.to_string()),
                local_region: Some(c.local_region.to_string()),
                local_city: Some(c.local_city.to_string()),
                local_language: Some(c.local_language.to_string()),
            });
        }

        // 2. Fallback to country bounding box
        for entry in COUNTRY_BOXES {
            if lat >= entry.min_lat
                && lat <= entry.max_lat
                && lon >= entry.min_lon
                && lon <= entry.max_lon
            {
                return Some(LocationInfo {
                    latitude: Some(lat),
                    longitude: Some(lon),
                    altitude: alt,
                    country: Some(entry.country.to_string()),
                    country_code: Some(entry.country_code.to_string()),
                    region: Some(entry.region.to_string()),
                    county: Some(entry.city.to_string()),
                    city: Some(entry.city.to_string()),
                    local_country: Some(entry.local_country.to_string()),
                    local_region: Some(entry.local_region.to_string()),
                    local_city: Some(entry.local_city.to_string()),
                    local_language: Some(entry.local_language.to_string()),
                });
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
            self.offline_lookup(lat, lon, alt, settings.city_radius_km)
        } else {
            None
        };
        let resolved = if let Some(geo) = offline {
            ctx.capabilities
                .log("info", "Resolved via offline Natural Earth lookup");
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
                unresolved("not in the offline data and online_lookup is off".to_string());
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
