---
feature: plugin-settings
status: accepted
archive-after:
touches:
  - crates/sdk/**
  - plugins/**
  - crates/host/src/config.rs
  - crates/host/tests/**
  - config*.yaml
---

# Tech Spec — Plugin-Settings (`plugin-settings`)

Setzt das PRD `docs/prd/plugin-settings.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern.

## Architektur

```text
config.yaml ──► crates/host/src/config.rs (YAML-Leser, AC-10)
                    │  PluginConfig.settings: HashMap<String, String>
                    ▼
PluginRuntime::execute_plugin ──► PluginContext { capabilities, settings }   (unverändert)
                    │
                    ▼
Plugin::process(doc, ctx)
   └─ self.settings_cache.get_or_parse(ctx)           crates/sdk::settings
         └─ <Plugin>Settings::from_reader(&mut SettingsReader)
               ├─ reader.bool / u32 / f32 / choice / list / path / url
               └─ reader.finish()  → Warnung je unbekanntem Schlüssel
```

- **Schichten (Verfassung, Architektur 1–3):** Die gemeinsame Logik liegt ausschließlich im SDK (`crates/sdk/src/settings.rs`). Plugins hängen nur vom SDK ab; kein Plugin kennt die Settings eines anderen.
- **Host:** `PluginRuntime` und `main.rs` bleiben unverändert. Die Plugins werden weiter mit `::new()` registriert; Modell- und Tier-Wahl geschieht pro Aufruf aus `ctx.settings` (AC-9). Geändert wird nur der YAML-Leser (`config.rs`).
- **WIT:** unverändert (`settings` bleibt eine Zeichenketten-Map; Nicht-Ziel des PRD).

## Schnittstellen

### SDK: `flagme_sdk::settings`

```rust
/// Liest Werte aus ctx.settings, protokolliert Probleme über ctx.capabilities.log("warn", …)
/// und merkt sich, welche Schlüssel gelesen wurden.
pub struct SettingsReader<'a> { /* map, capabilities, plugin: &'static str, seen: HashSet<String> */ }

impl<'a> SettingsReader<'a> {
    pub fn new(plugin: &'static str, ctx: &'a PluginContext) -> Self;

    pub fn bool(&mut self, key: &str, aliases: &[&str], default: bool) -> bool;
    pub fn u32(&mut self, key: &str, aliases: &[&str], range: RangeInclusive<u32>, default: u32) -> u32;
    pub fn f32(&mut self, key: &str, aliases: &[&str], range: RangeInclusive<f32>, default: f32) -> f32;
    pub fn choice<T: Copy>(&mut self, key: &str, aliases: &[&str], options: &[(&[&str], T)], default: T) -> T;
    pub fn list_u32(&mut self, key: &str, range: RangeInclusive<u32>, default: &[u32]) -> Vec<u32>;
    pub fn string(&mut self, key: &str, default: &str) -> String;
    pub fn path(&mut self, key: &str, default: &str) -> String;   // ohne "..", ohne abschließendes "/"
    pub fn url(&mut self, key: &str, default: &str) -> String;    // nur https:// bzw. http://

    /// Warnt einmal je Schlüssel, der in der Map steht, aber nicht gelesen wurde.
    pub fn finish(self);
}

pub trait PluginSettings: Sized + Clone + Default + Send + Sync {
    const PLUGIN: &'static str;
    fn from_reader(r: &mut SettingsReader<'_>) -> Self;
}

/// Parst einmal je unterschiedlicher Settings-Map; Warnungen erscheinen dadurch nur einmal
/// pro Konfiguration und nicht für jedes Dokument.
pub struct SettingsCache<T: PluginSettings> { inner: Mutex<Option<(HashMap<String, String>, Arc<T>)>> }

impl<T: PluginSettings> SettingsCache<T> {
    pub fn get_or_parse(&self, ctx: &PluginContext) -> Arc<T>;
}
```

### Einleseregeln

| Typ       | Regel                                                                                                               | Ungültig                                        |
| :-------- | :------------------------------------------------------------------------------------------------------------------ | :---------------------------------------------- |
| bool      | `true`/`yes`/`on`/`1` → wahr, `false`/`no`/`off`/`0` → falsch; Groß-/Kleinschreibung und Leerraum egal              | Warnung, Standard                               |
| u32 / f32 | Parse; außerhalb von `range` → Grenze **und** Warnung                                                               | Warnung, Standard                               |
| choice    | Vergleich ohne Groß-/Kleinschreibung gegen alle Aliase einer Option                                                 | Warnung mit Liste der erlaubten Werte, Standard |
| list_u32  | kommagetrennt, Leerraum egal, jedes Element wie u32; leere Liste ungültig; Duplikate entfernt, aufsteigend sortiert | Warnung, Standard                               |
| path      | leer, enthält `..` → ungültig; abschließendes `/` entfernt                                                          | Warnung, Standard                               |
| url       | muss mit `https://` oder `http://` beginnen                                                                         | Warnung, Standard                               |
| Alias     | kanonischer Schlüssel gewinnt; sind beide gesetzt, Warnung „alias ignored“                                          | —                                               |

Warnungstexte (stabil, für Tests):

- `settings: unknown key '<key>' ignored`
- `settings: invalid value '<value>' for '<key>' (expected <erwartet>), using default '<default>'`
- `settings: value '<value>' for '<key>' out of range <min>..=<max>, using '<clamped>'`
- `settings: '<value>' for '<key>' is not supported yet (supported: <liste>), using default '<default>'` (AC-6)

Den Plugin-Namen ergänzt der Host bereits über `CapabilityManager::log` (`plugin = …`).

### Parameter je Plugin

Abweichend vom ersten PRD-Entwurf: `ocr-text.max_image_dimension` entfällt (ohne Bilddekodierung nicht ehrlich umsetzbar, folgt mit #23); `thumbnail.quality` hat keinen festen Standard.

| Plugin           | Schlüssel              | Typ / Bereich                                                                                                                                       | Standard                                        | Umsetzung                                                                                    |
| :--------------- | :--------------------- | :-------------------------------------------------------------------------------------------------------------------------------------------------- | :---------------------------------------------- | :------------------------------------------------------------------------------------------- |
| `metadata`       | `makernote_max_length` | u32 64–65536                                                                                                                                        | 512                                             | ersetzt Konstante in `parse_image_metadata`                                                  |
|                  | `xmp_max_value_length` | u32 64–65536                                                                                                                                        | 512                                             | ersetzt Konstante in `parse_single_xmp_packet` / `parse_xml_attributes`                      |
| `reversegeo`     | `offline_cache`        | bool                                                                                                                                                | true                                            | `false` → kein `kv_get`/`kv_set`                                                             |
|                  | `offline_lookup`       | bool                                                                                                                                                | true                                            | `false` → `offline_lookup()` übersprungen                                                    |
|                  | `online_lookup`        | bool                                                                                                                                                | true                                            | `false` → kein HTTP; direkt Fallback `Unknown Country`                                       |
|                  | `city_radius_km`       | u32 1–2000                                                                                                                                          | 200                                             | `dist² ≤ (km / (200/1.8))²`; 200 km = bisherige 1,8° (AC-5)                                                           |
|                  | `cache_precision`      | u32 0–6                                                                                                                                             | 3                                               | Nachkommastellen im Schlüssel `geo:<lat>:<lon>`                                              |
|                  | `nominatim_url`        | url                                                                                                                                                 | `https://nominatim.openstreetmap.org/reverse`   | Basis; Query-Parameter hängt das Plugin an                                                   |
|                  | `accept_language`      | string                                                                                                                                              | `en`                                            | Query-Parameter; Online-Ergebnis: `local_language` = dieser Wert                             |
| `countryflag`    | —                      | —                                                                                                                                                   | —                                               | nur `finish()` (AC-3)                                                                        |
| `thumbnail`      | `variants`             | list_u32 16–8192                                                                                                                                    | `128,512,1024`                                  | Labels siehe unten                                                                           |
|                  | `max_dimension`        | u32 16–8192                                                                                                                                         | keine Grenze                                    | Varianten > Grenze entfallen; ist keine übrig → genau `max_dimension`                        |
|                  | `quality`              | u32 1–100                                                                                                                                           | nicht gesetzt                                   | `sips -s formatOptions <q>`; ohne Wert Encoder-Standard (AC-5)                               |
|                  | `output_dir`           | path                                                                                                                                                | `cache/thumbnails`                              | Zielordner                                                                                   |
| `ai-tagging`     | `model`                | choice `CLIP-ViT-B/32` (`clip`), `MobileNetV3` (`mobilenet`), `OpenCLIP-ConvNeXt-Large` (`openclip`)                                                | aus Konstruktor (`new()` → CLIP)                | Auswahl pro Aufruf                                                                           |
|                  | `model_tier`           | choice `lightweight` → MobileNetV3, `heavy` → OpenCLIP                                                                                              | —                                               | nur wenn `model` fehlt                                                                       |
|                  | `embedding_dim`        | u32 64–2048                                                                                                                                         | 512                                             | `build_embedding(dim)` statt `build_512d_embedding`                                          |
|                  | `device`               | choice `cpu`; bekannte, nicht unterstützte: `mps`, `cuda`, `metal`                                                                                  | `cpu`                                           | nicht unterstützt → AC-6-Warnung                                                             |
| `duplicate`      | `phash_threshold`      | u32 0–64                                                                                                                                            | 0                                               | siehe Datenmodell                                                                            |
| `timeline`       | `timezone`             | `UTC`, IANA-Name (`Europe/Berlin`) oder Offset (`+02:00`)                                                                                           | `UTC`                                           | nur für EXIF-/Dateinamen-Zeiten ohne Offset; Crate `chrono-tz`                               |
|                  | `source_priority`      | Liste aus `exif_datetime_original`, `exif_datetime_digitized`, `exif_datetime`, `xmp_create_date`, `filename_timestamp`, `filesystem_modified_time` | Reihenfolge der README                          | `imported_timestamp_fallback` bleibt immer letzter Schritt; nicht genannte Quellen entfallen |
| `weather-enrich` | `endpoint`             | url                                                                                                                                                 | `https://archive-api.open-meteo.com/v1/archive` | Basis der Abfrage                                                                            |
|                  | `coordinate_precision` | u32 0–4                                                                                                                                             | 2                                               | Abfrage und Cache-Schlüssel                                                                  |
|                  | `offline_fallback`     | bool                                                                                                                                                | true                                            | `false` → bei fehlgeschlagener Abfrage `doc.weather = None` (kein Schätzwert)                |
| `ocr-text`       | `engine`               | choice `default`; bekannte, nicht unterstützte: `PaddleOCR-v4-Server`, `CRAFT-CRNN-Quantized`                                                       | `default`                                       | nicht unterstützt → AC-6-Warnung                                                             |
|                  | `min_confidence`       | f32 0.0–1.0                                                                                                                                         | 0.0                                             | Regionen mit `confidence < X` entfallen; `combined_text` daraus neu gebildet                 |
| `face-detection` | `tier`                 | choice `lightweight`, `heavy`                                                                                                                       | aus Konstruktor (`new()` → lightweight)         | Auswahl pro Aufruf                                                                           |
|                  | `min_confidence`       | f32 0.0–1.0                                                                                                                                         | 0.0                                             | Gesichter mit `confidence < X` entfallen                                                     |
|                  | `device`               | wie `ai-tagging.device`                                                                                                                             | `cpu`                                           | AC-6-Warnung                                                                                 |
| `privacy-redact` | bestehende Schlüssel   | wie README                                                                                                                                          | wie README                                      | `apply_map` wird durch `PluginSettings::from_reader` ersetzt; Aliase bleiben                 |

**Thumbnail-Labels:** Varianten aufsteigend; kleinste → `small`, größte → `large`, von den übrigen die nächste an 512 → `medium`, weitere → `<px>px`. Bei einer Variante nur `small`, bei zwei `small`/`large`. Damit bleibt `/api/photos/:id/thumbnail/small` (Web-UI) gültig, und der Standard ergibt exakt die heutigen Labels (AC-5).

**Vorrang Konstruktor ↔ Settings (ai-tagging, face-detection, privacy-redact):** `with_model` / `with_tier` / `with_settings` setzen den Standard; Schlüssel in `ctx.settings` überschreiben ihn.

### Host: YAML-Leser (`config.rs`)

`raw_line.find('#')` wird durch eine Funktion `strip_comment(line)` ersetzt, die `#` nur außerhalb von `"…"` und `'…'` als Kommentarbeginn wertet. `clean_yaml_scalar` bleibt unverändert.

## Datenmodell

- **Keine Änderung** an `crates/models`, an SQLite-Schema oder WIT.
- **`duplicate`, `phash_threshold > 0`:** Die KV-Schnittstelle kennt nur `get`/`set`. Das Plugin führt zusätzlich einen Index unter dem Schlüssel `phash:index` (Zeilen `<16 hex> <doc.id>`, `\n`-getrennt). Bei jedem Dokument: Index lesen, Hamming-Distanz (`(a ^ b).count_ones()`) zu allen Einträgen, Treffer `≤ N` mit anderer `doc.id` in `duplicates`, eigenen Eintrag anhängen, falls die `doc.id` noch fehlt. Mit `phash_threshold: 0` bleibt der heutige exakte Pfad (`phash:<hex>`) ohne Index-Zugriff (AC-5).
- **Neue Abhängigkeit:** `chrono-tz` in `plugins/timeline` (kompiliert für `wasm32-wasip2`).

## Umsetzung der Akzeptanzkriterien

| AC    | Umsetzung                                                                         | Test                                                                                                                                  |
| ----- | --------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| AC-1  | `crates/sdk/src/settings.rs`: `SettingsReader`, `PluginSettings`, `SettingsCache` | `crates/sdk/tests/settings_test.rs::reader_types_fall_back_with_warning`                                                              |
| AC-2  | `SettingsReader::bool`, von allen Plugins genutzt                                 | `crates/sdk/tests/settings_test.rs::bool_rule`, `crates/host/tests/settings_test.rs::bool_rule_in_privacy_redact`                     |
| AC-3  | `SettingsReader::finish` + `SettingsCache`                                        | `crates/sdk/tests/settings_test.rs::unknown_key_warns_once`                                                                           |
| AC-4  | Settings-Structs je Plugin (Tabelle oben)                                         | `crates/host/tests/settings_test.rs::<plugin>_<key>` (ein Test je Schlüssel)                                                          |
| AC-5  | Standardwerte = heutige Konstanten; `phash_threshold: 0` nutzt alten Pfad         | `crates/host/tests/settings_test.rs::defaults_match_previous_behaviour`                                                               |
| AC-6  | `choice` mit Liste „bekannt, nicht unterstützt“                                   | `crates/host/tests/settings_test.rs::unsupported_value_warns`                                                                         |
| AC-7  | `duplicate`: Index `phash:index`, Hamming-Vergleich                               | `crates/host/tests/settings_test.rs::duplicate_phash_threshold`                                                                       |
| AC-8  | Filter in `face-detection` und `ocr-text`                                         | `crates/host/tests/settings_test.rs::min_confidence_filters`                                                                          |
| AC-9  | Modell-/Tier-Auswahl in `process`                                                 | `crates/host/tests/settings_test.rs::model_and_tier_selected_per_call`                                                                |
| AC-10 | `strip_comment` in `config.rs`                                                    | `crates/host/tests/settings_test.rs::hash_inside_quotes_kept`                                                                         |
| AC-11 | Bereinigte Profile                                                                | `crates/host/tests/settings_test.rs::profiles_have_no_unknown_keys`                                                                   |
| AC-12 | README §2.2 je Plugin                                                             | `crates/host/tests/settings_test.rs::readmes_list_all_keys` (prüft, dass jeder Schlüssel der Settings-Structs in der README vorkommt) |

Jeder Test trägt darüber `// plugin-settings AC-<n>: …` (Verfassung, Strikte Regeln 3).

Für AC-3/AC-6 nutzen die Tests eine aufzeichnende `HostCapabilities`-Implementierung **im Testcode** (sammelt `log`-Aufrufe); im Produktionspfad gibt es keine Test-Doubles.

## Sicherheit

- **Keine neue Ambient Authority.** Settings verändern nur Parameter; sie erweitern keine Capabilities. Ein Setting, das eine Ressource voraussetzt (z. B. `online_lookup: true`), wirkt nur, wenn die Capability (`http`) gewährt ist.
- **Pfade:** `path` lehnt `..` ab; `output_dir` (thumbnail, privacy-redact) wird weiter über `fs_write` geschrieben, also von der `filesystem`-Capability des Hosts geprüft.
- **URLs:** `nominatim_url` und `endpoint` erlauben nur `http(s)://`; der Host prüft das Schema in `http_get` zusätzlich. Eine frei wählbare URL ist bewusst zulässig (eigene Nominatim-/Open-Meteo-Instanz); die Freigabe von `http` bleibt die Kontrolle.
- **Ressourcen:** `variants` und `embedding_dim` sind nach oben begrenzt (8192 px, 2048 Werte), damit eine Konfiguration die Speicher- und Zeitgrenzen der Sandbox (Security 3) nicht unterläuft. `phash:index` wächst linear (≈ 34 Byte je Foto).

## Entscheidungen (2026-11-25)

- `timeline.timezone` wird mit `chrono-tz` umgesetzt (IANA-Namen inkl. Sommerzeit); die größere Wasm-Datei wird in Kauf genommen.
- `device` wird aus den Profilen entfernt (`config.mac-studio.yaml`), damit sie warnungsfrei laden (AC-11).
- `phash:index` mit linearem Vergleich ist für jetzt akzeptiert. Eine Präfix-Abfrage in der KV-Capability (WIT-Änderung) braucht später ein eigenes ADR.
