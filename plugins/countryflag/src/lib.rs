use flagme_sdk::{
    CountryInfo, Plugin, PluginContext, PluginResult, UnifiedDocument,
};

/// Offline Country and Flag emoji lookup database.
pub struct CountryFlagPlugin;

impl CountryFlagPlugin {
    pub fn new() -> Self {
        Self
    }

    /// Embedded offline ISO-3166 database. No network calls allowed.
    fn lookup(&self, code: &str) -> Option<CountryInfo> {
        let code = code.to_uppercase();
        let (name, emoji, continent, region) = match code.as_str() {
            "DE" => ("Germany", "🇩🇪", "Europe", "Western Europe"),
            "FR" => ("France", "🇫🇷", "Europe", "Western Europe"),
            "US" => ("United States", "🇺🇸", "North America", "Northern America"),
            "GB" => ("United Kingdom", "🇬🇧", "Europe", "Northern Europe"),
            "JP" => ("Japan", "🇯🇵", "Asia", "Eastern Asia"),
            "IT" => ("Italy", "🇮🇹", "Europe", "Southern Europe"),
            "ES" => ("Spain", "🇪🇸", "Europe", "Southern Europe"),
            "CA" => ("Canada", "🇨🇦", "North America", "Northern America"),
            "AU" => ("Australia", "🇦🇺", "Oceania", "Australia and New Zealand"),
            "CH" => ("Switzerland", "🇨🇭", "Europe", "Western Europe"),
            "AT" => ("Austria", "🇦🇹", "Europe", "Western Europe"),
            "NL" => ("Netherlands", "🇳🇱", "Europe", "Western Europe"),
            "NO" => ("Norway", "🇳🇴", "Europe", "Northern Europe"),
            "SE" => ("Sweden", "🇸🇪", "Europe", "Northern Europe"),
            _ => ("Global / Unknown", "🌐", "Unknown", "Unknown"),
        };

        Some(CountryInfo {
            country_code: code,
            country_name: name.to_string(),
            flag_emoji: emoji.to_string(),
            continent: continent.to_string(),
            region: region.to_string(),
        })
    }
}

impl Plugin for CountryFlagPlugin {
    fn name(&self) -> &'static str {
        "countryflag"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        let country_code = match &doc.location {
            Some(loc) if loc.country_code.is_some() => loc.country_code.clone().unwrap(),
            _ => {
                ctx.capabilities.log("info", "No country code present in document location; skipping country flag");
                return Ok(doc);
            }
        };

        if let Some(info) = self.lookup(&country_code) {
            ctx.capabilities.log(
                "info",
                &format!("Enriched country metadata for {}: {}", country_code, info.flag_emoji),
            );
            doc.country = Some(info);
        }

        Ok(doc)
    }
}
