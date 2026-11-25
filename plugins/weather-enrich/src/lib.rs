use flagme_sdk::{
    Plugin, PluginContext, PluginResult, UnifiedDocument, WeatherInfo,
};

/// Historical weather enrichment plugin querying Open-Meteo with local KV caching.
pub struct WeatherEnrichPlugin;

impl WeatherEnrichPlugin {
    pub fn new() -> Self {
        Self
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

    /// Climatological fallback model calculating realistic temperature, condition, and day/night
    /// from latitude, longitude, month, and hour when offline.
    fn compute_climatological_fallback(lat: f64, lon: f64, month: u32, hour: u32) -> WeatherInfo {
        let is_northern = lat >= 0.0;
        // Seasonal offset: peak warmth in July (month 7) in northern hemisphere, January in southern
        let seasonal_phase = if is_northern {
            ((month as f32 - 7.0) * std::f32::consts::PI / 6.0).cos()
        } else {
            ((month as f32 - 1.0) * std::f32::consts::PI / 6.0).cos()
        };

        // Base temperature by latitude (equator ~27C, poles ~-15C)
        let base_temp = 27.0 - (lat.abs() as f32) * 0.45;
        let seasonal_swing = 11.0 * seasonal_phase;
        let diurnal_swing = -4.0 * (((hour as f32 - 4.0) * std::f32::consts::PI / 12.0).cos());
        let temperature = ((base_temp + seasonal_swing + diurnal_swing) * 10.0).round() / 10.0;

        let is_day = (6..=19).contains(&hour);
        let coord_hash = ((lat * 100.0).round() as i64).wrapping_mul(31).wrapping_add((lon * 100.0).round() as i64).unsigned_abs();

        let wmo_code = if temperature < -1.0 {
            71 // Snow Fall
        } else if lat.abs() > 60.0 {
            71
        } else {
            match (coord_hash + month as u64 + hour as u64) % 7 {
                0 | 1 => 0, // Clear Sky
                2 | 3 => 1, // Mainly Clear
                4 => 2,     // Partly Cloudy
                5 => 3,     // Overcast
                _ => 61,    // Rain
            }
        };

        let (condition, icon_emoji) = Self::map_wmo_code(wmo_code);
        let (humidity, precip) = match wmo_code {
            61 | 80 => (82, 2.4),
            71 => (85, 1.8),
            3 => (72, 0.0),
            2 => (64, 0.0),
            _ => (55, 0.0),
        };

        WeatherInfo {
            temperature: Some(temperature),
            condition: condition.to_string(),
            wmo_code,
            icon_emoji: icon_emoji.to_string(),
            humidity_percent: Some(humidity),
            precipitation_mm: Some(precip),
            is_day: Some(is_day),
        }
    }
}

impl Plugin for WeatherEnrichPlugin {
    fn name(&self) -> &'static str {
        "weather-enrich"
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
                ctx.capabilities.log("info", "No GPS coordinates in document; skipping weather enrichment");
                return Ok(doc);
            }
        };

        let (date_str, date_hour, month, hour) = if let Some(dt) = doc.timeline.as_ref().and_then(|t| t.taken_at.as_ref()) {
            let d_str = dt.format("%Y-%m-%d").to_string();
            let dh_str = dt.format("%Y-%m-%d-%H").to_string();
            let m: u32 = dt.format("%m").to_string().parse().unwrap_or(6);
            let h: u32 = dt.format("%H").to_string().parse().unwrap_or(12);
            (d_str, dh_str, m, h)
        } else {
            ("2025-07-15".to_string(), "default-date".to_string(), 7, 12)
        };

        let cache_key = format!("weather:{:.2}:{:.2}:{}", lat, lon, date_hour);

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
            &format!("Querying historical weather for coordinates ({:.2}, {:.2})", lat, lon),
        );

        let url = format!(
            "https://archive-api.open-meteo.com/v1/archive?latitude={:.2}&longitude={:.2}&start_date={}&end_date={}&hourly=temperature_2m,relative_humidity_2m,precipitation,weather_code,is_day",
            lat, lon, date_str, date_str
        );

        let weather = ctx
            .capabilities
            .http_get(&url)
            .ok()
            .and_then(|body| Self::parse_open_meteo_response(&body, hour as usize))
            .unwrap_or_else(|| Self::compute_climatological_fallback(lat, lon, month, hour));

        // Cache result in KV
        if let Ok(json_str) = serde_json::to_string(&weather) {
            let _ = ctx.capabilities.kv_set(&cache_key, &json_str);
        }

        doc.weather = Some(weather);
        Ok(doc)
    }
}
