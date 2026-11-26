use flagme_sdk::{
    Plugin, PluginContext, PluginResult, PluginSettings, SettingsCache, SettingsReader,
    UnifiedDocument, WeatherInfo,
};

/// Configurable parameters of the weather plugin (`config.yaml` `settings`).
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherSettings {
    /// Base URL of the Open-Meteo archive API.
    pub endpoint: String,
    /// Decimal places of the coordinates in the request and the cache key.
    pub coordinate_precision: u32,
}

impl Default for WeatherSettings {
    fn default() -> Self {
        Self {
            endpoint: "https://archive-api.open-meteo.com/v1/archive".to_string(),
            coordinate_precision: 2,
        }
    }
}

impl PluginSettings for WeatherSettings {
    const PLUGIN: &'static str = "weather-enrich";

    fn from_reader(base: &Self, r: &mut SettingsReader<'_>) -> Self {
        r.not_supported(
            "offline_fallback",
            &[],
            "the climatological estimate was removed, see ADR 0002",
        );
        Self {
            endpoint: r.url("endpoint", &[], &base.endpoint),
            coordinate_precision: r.u32("coordinate_precision", &[], 0..=4, base.coordinate_precision),
        }
    }
}

/// Historical weather enrichment plugin querying Open-Meteo with local KV caching.
pub struct WeatherEnrichPlugin {
    settings: SettingsCache<WeatherSettings>,
}

impl WeatherEnrichPlugin {
    pub fn new() -> Self {
        Self::with_settings(WeatherSettings::default())
    }

    pub fn with_settings(settings: WeatherSettings) -> Self {
        Self {
            settings: SettingsCache::new(settings),
        }
    }

    /// Maps standard WMO weather interpretation codes (0-99) to condition names and emoji icons.
    pub fn map_wmo_code(code: u8) -> (&'static str, &'static str) {
        match code {
            0 => ("Clear Sky", "☀️"),
            1 => ("Mainly Clear", "🌤️"),
            2 => ("Partly Cloudy", "⛅"),
            3 => ("Overcast", "☁️"),
            45 | 48 => ("Foggy", "🌫️"),
            51 | 53 | 55 => ("Drizzle", "🌦️"),
            56 | 57 => ("Freezing Drizzle", "🌧️"),
            61 | 63 | 65 => ("Rain", "🌧️"),
            66 | 67 => ("Freezing Rain", "🌧️"),
            71 | 73 | 75 => ("Snow Fall", "❄️"),
            77 => ("Snow Grains", "❄️"),
            80 | 81 | 82 => ("Rain Showers", "🌦️"),
            85 | 86 => ("Snow Showers", "🌨️"),
            95 => ("Thunderstorm", "⛈️"),
            96 | 99 => ("Thunderstorm with Hail", "⛈️"),
            _ => ("Variable Weather", "🌡️"),
        }
    }

    /// Parses Open-Meteo Historical Archive API JSON response at the given target hour (0..23).
    pub fn parse_open_meteo_response(body: &str, target_hour: usize) -> Option<WeatherInfo> {
        let val: serde_json::Value = serde_json::from_str(body).ok()?;
        let hourly = val.get("hourly")?;

        let temps = hourly.get("temperature_2m")?.as_array()?;
        let codes = hourly.get("weather_code")?.as_array()?;
        let idx = target_hour.min(temps.len().saturating_sub(1)).min(codes.len().saturating_sub(1));

        let temp = temps.get(idx)?.as_f64()? as f32;
        let wmo_code = codes.get(idx)?.as_u64()? as u8;
        let (condition, icon_emoji) = Self::map_wmo_code(wmo_code);

        let humidity = hourly
            .get("relative_humidity_2m")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.get(idx))
            .and_then(|v| v.as_u64())
            .map(|h| h.min(100) as u8);

        let precip = hourly
            .get("precipitation")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.get(idx))
            .and_then(|v| v.as_f64())
            .map(|p| p as f32);

        let is_day = hourly
            .get("is_day")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.get(idx))
            .and_then(|v| v.as_u64())
            .map(|d| d == 1);

        Some(WeatherInfo {
            temperature: Some(temp),
            condition: condition.to_string(),
            wmo_code,
            icon_emoji: icon_emoji.to_string(),
            humidity_percent: humidity,
            precipitation_mm: precip,
            is_day,
        })
    }
}

impl Plugin for WeatherEnrichPlugin {
    fn name(&self) -> &'static str {
        "weather-enrich"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        let (lat, lon) = match &doc.location {
            Some(loc) if loc.latitude.is_some() && loc.longitude.is_some() => {
                (loc.latitude.unwrap(), loc.longitude.unwrap())
            }
            _ => {
                ctx.capabilities.log("info", "No GPS coordinates in document; skipping weather enrichment");
                return Ok(doc);
            }
        };

        let Some(taken_at) = doc.timeline.as_ref().and_then(|t| t.taken_at) else {
            ctx.capabilities.log(
                "warn",
                &format!(
                    "weather-enrich: no capture time for '{}'; weather not queried",
                    doc.file.path
                ),
            );
            return Ok(doc);
        };
        let date_str = taken_at.format("%Y-%m-%d").to_string();
        let date_hour = taken_at.format("%Y-%m-%d-%H").to_string();
        let hour: usize = taken_at.format("%H").to_string().parse().unwrap_or(0);

        let settings = self.settings.get_or_parse(ctx);
        let p = settings.coordinate_precision as usize;
        let cache_key = format!("weather:{:.p$}:{:.p$}:{}", lat, lon, date_hour, p = p);

        // 1. Check local KV cache
        if let Ok(Some(cached_json)) = ctx.capabilities.kv_get(&cache_key) {
            ctx.capabilities.log("info", "Found cached historical weather data");
            if let Ok(weather) = serde_json::from_str::<WeatherInfo>(&cached_json) {
                doc.weather = Some(weather);
                return Ok(doc);
            }
        }

        // 2. Outbound HTTP query to Open-Meteo Historical Weather API
        ctx.capabilities.log(
            "info",
            &format!("Querying historical weather for coordinates ({:.p$}, {:.p$})", lat, lon, p = p),
        );

        let url = format!(
            "{}?latitude={:.p$}&longitude={:.p$}&start_date={}&end_date={}&hourly=temperature_2m,relative_humidity_2m,precipitation,weather_code,is_day",
            settings.endpoint, lat, lon, date_str, date_str, p = p
        );

        let response = ctx.capabilities.http_get(&url);
        let weather = match response {
            Ok(body) => match Self::parse_open_meteo_response(&body, hour) {
                Some(w) => w,
                None => {
                    ctx.capabilities.log(
                        "warn",
                        &format!(
                            "weather-enrich: no measured weather for '{}': unreadable response",
                            doc.file.path
                        ),
                    );
                    return Ok(doc);
                }
            },
            Err(e) => {
                ctx.capabilities.log(
                    "warn",
                    &format!("weather-enrich: no measured weather for '{}': {}", doc.file.path, e),
                );
                return Ok(doc);
            }
        };

        // Cache result in KV
        if let Ok(json_str) = serde_json::to_string(&weather) {
            let _ = ctx.capabilities.kv_set(&cache_key, &json_str);
        }

        doc.weather = Some(weather);
        Ok(doc)
    }
}

flagme_sdk::export_plugin!(WeatherEnrichPlugin, WeatherEnrichPlugin::new);
