use flagme_sdk::{
    CountryInfo, Plugin, PluginContext, PluginResult, PluginSettings, SettingsCache,
    SettingsReader, UnifiedDocument,
};

/// The country flag plugin has no parameters; the settings only exist so that unknown
/// keys in `config.yaml` are reported.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CountryFlagSettings;

impl PluginSettings for CountryFlagSettings {
    const PLUGIN: &'static str = "countryflag";

    fn from_reader(_base: &Self, _reader: &mut SettingsReader<'_>) -> Self {
        Self
    }
}

/// Offline Country and Flag emoji lookup database covering ISO-3166-1 alpha-2 codes.
pub struct CountryFlagPlugin {
    settings: SettingsCache<CountryFlagSettings>,
}

impl CountryFlagPlugin {
    pub fn new() -> Self {
        Self {
            settings: SettingsCache::new(CountryFlagSettings),
        }
    }

    /// Computes the Unicode Regional Indicator Flag Emoji for any 2-letter ISO-3166 alpha-2 code.
    pub fn iso_to_flag_emoji(code: &str) -> String {
        let upper = code.trim().to_ascii_uppercase();
        let bytes = upper.as_bytes();
        if bytes.len() == 2 && bytes[0].is_ascii_uppercase() && bytes[1].is_ascii_uppercase() {
            let c1 = char::from_u32(0x1F1E6 + (bytes[0] - b'A') as u32).unwrap_or('🌐');
            let c2 = char::from_u32(0x1F1E6 + (bytes[1] - b'A') as u32).unwrap_or('🌐');
            let mut s = String::with_capacity(8);
            s.push(c1);
            s.push(c2);
            s
        } else {
            "🌐".to_string()
        }
    }

    /// Embedded offline ISO-3166 database. No network calls allowed.
    pub fn lookup(&self, code: &str) -> Option<CountryInfo> {
        let code = code.trim().to_uppercase();
        if code.is_empty() {
            return None;
        }

        let (name, continent, region) = match code.as_str() {
            "DE" => ("Germany", "Europe", "Western Europe"),
            "FR" => ("France", "Europe", "Western Europe"),
            "US" => ("United States", "North America", "Northern America"),
            "GB" | "UK" => ("United Kingdom", "Europe", "Northern Europe"),
            "JP" => ("Japan", "Asia", "Eastern Asia"),
            "IT" => ("Italy", "Europe", "Southern Europe"),
            "ES" => ("Spain", "Europe", "Southern Europe"),
            "CA" => ("Canada", "North America", "Northern America"),
            "AU" => ("Australia", "Oceania", "Australia and New Zealand"),
            "CH" => ("Switzerland", "Europe", "Western Europe"),
            "AT" => ("Austria", "Europe", "Western Europe"),
            "NL" => ("Netherlands", "Europe", "Western Europe"),
            "BE" => ("Belgium", "Europe", "Western Europe"),
            "LU" => ("Luxembourg", "Europe", "Western Europe"),
            "DK" => ("Denmark", "Europe", "Northern Europe"),
            "NO" => ("Norway", "Europe", "Northern Europe"),
            "SE" => ("Sweden", "Europe", "Northern Europe"),
            "FI" => ("Finland", "Europe", "Northern Europe"),
            "IS" => ("Iceland", "Europe", "Northern Europe"),
            "IE" => ("Ireland", "Europe", "Northern Europe"),
            "PT" => ("Portugal", "Europe", "Southern Europe"),
            "GR" => ("Greece", "Europe", "Southern Europe"),
            "PL" => ("Poland", "Europe", "Eastern Europe"),
            "CZ" => ("Czechia", "Europe", "Eastern Europe"),
            "SK" => ("Slovakia", "Europe", "Eastern Europe"),
            "HU" => ("Hungary", "Europe", "Eastern Europe"),
            "RO" => ("Romania", "Europe", "Eastern Europe"),
            "BG" => ("Bulgaria", "Europe", "Eastern Europe"),
            "HR" => ("Croatia", "Europe", "Southern Europe"),
            "SI" => ("Slovenia", "Europe", "Southern Europe"),
            "RS" => ("Serbia", "Europe", "Southern Europe"),
            "UA" => ("Ukraine", "Europe", "Eastern Europe"),
            "EE" => ("Estonia", "Europe", "Northern Europe"),
            "LV" => ("Latvia", "Europe", "Northern Europe"),
            "LT" => ("Lithuania", "Europe", "Northern Europe"),
            "TR" => ("Turkey", "Asia", "Western Asia"),
            "CN" => ("China", "Asia", "Eastern Asia"),
            "KR" => ("South Korea", "Asia", "Eastern Asia"),
            "TW" => ("Taiwan", "Asia", "Eastern Asia"),
            "HK" => ("Hong Kong", "Asia", "Eastern Asia"),
            "SG" => ("Singapore", "Asia", "South-Eastern Asia"),
            "TH" => ("Thailand", "Asia", "South-Eastern Asia"),
            "VN" => ("Vietnam", "Asia", "South-Eastern Asia"),
            "MY" => ("Malaysia", "Asia", "South-Eastern Asia"),
            "ID" => ("Indonesia", "Asia", "South-Eastern Asia"),
            "PH" => ("Philippines", "Asia", "South-Eastern Asia"),
            "IN" => ("India", "Asia", "Southern Asia"),
            "AE" => ("United Arab Emirates", "Asia", "Western Asia"),
            "IL" => ("Israel", "Asia", "Western Asia"),
            "SA" => ("Saudi Arabia", "Asia", "Western Asia"),
            "NZ" => ("New Zealand", "Oceania", "Australia and New Zealand"),
            "MX" => ("Mexico", "North America", "Central America"),
            "BR" => ("Brazil", "South America", "South America"),
            "AR" => ("Argentina", "South America", "South America"),
            "CL" => ("Chile", "South America", "South America"),
            "CO" => ("Colombia", "South America", "South America"),
            "PE" => ("Peru", "South America", "South America"),
            "ZA" => ("South Africa", "Africa", "Southern Africa"),
            "EG" => ("Egypt", "Africa", "Northern Africa"),
            "MA" => ("Morocco", "Africa", "Northern Africa"),
            "KE" => ("Kenya", "Africa", "Eastern Africa"),
            "NG" => ("Nigeria", "Africa", "Western Africa"),
            "XX" => ("Global / Unknown", "Unknown", "Unknown"),
            _ => ("Global / Unknown", "Unknown", "Unknown"),
        };

        let emoji = if name == "Global / Unknown" && code == "XX" {
            "🌐".to_string()
        } else {
            Self::iso_to_flag_emoji(&code)
        };

        Some(CountryInfo {
            country_code: code,
            country_name: name.to_string(),
            flag_emoji: emoji,
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
        let _settings = self.settings.get_or_parse(ctx);
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
