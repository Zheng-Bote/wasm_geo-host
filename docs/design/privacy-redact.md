---
feature: privacy-redact
status: draft
archive-after:
touches:
  - "crates/models/**"
  - "plugins/privacy-redact/**"
  - "crates/host/**"
  - "crates/host/migrations/**"
  - "Cargo.toml"
---

# Tech Spec — privacy-redact (`privacy-redact`)

Setzt das PRD `docs/prd/privacy-redact.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern.

## Architektur & Komponenten

Das Plugin schließt sich an Analyse- und Erkennungs-Plugins (`face-detection`, `metadata`, `reversegeo`) an:

```text
+-------------------------------------------------------------+
|                     flagme-host (Host)                      |
|  - Capabilities: filesystem                                 |
|  - SQLite Storage Engine (photos, locations)                |
+------------------------------+------------------------------+
                               |
                               v
+-------------------------------------------------------------+
|                flagme-plugin-privacy-redact                 |
|  1. Visuelle Obfuskation (Pixelate / Blur / Blackout)       |
|  2. GPS-Fuzzing (Dezimalstellen-Rundung)                    |
|  3. Entfernung von Seriennummern aus raw_tags               |
|  4. Befüllt UnifiedDocument.privacy                         |
+-------------------------------------------------------------+
                               |
                               v
+-------------------------------------------------------------+
|                   SQLite (media.db)                         |
|  - Spalten: is_redacted, redacted_file_path, is_gps_fuzzed  |
+-------------------------------------------------------------+
```

### 1. Datenmodell (AC-1)
In `crates/models/src/lib.rs`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PrivacyInfo {
    pub is_redacted: bool,
    pub faces_redacted_count: usize,
    pub license_plates_redacted_count: usize,
    pub original_gps_fuzzed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redacted_image_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied_rules: Vec<String>,
}
```
Erfüllt **AC-1**: Datenmodell für Schutzmaßnahmen.

### 2. Plugin & Redaktions-Engines (AC-2, AC-3, AC-4, AC-5)
In `plugins/privacy-redact/src/lib.rs`:
- Konfigurierbare Strategien:
  - `RedactionStyle::Pixelate`: Mosaik-Rasterung.
  - `RedactionStyle::Blackout`: Vollständige Schwärzung.
- GPS-Fuzzing: `round_to_decimals(lat, lon, decimals)` (z. B. 2 Dezimalstellen).
- Säuberung von `raw_tags`: Entfernt Schlüssel wie `CameraSerialNumber`, `LensSerialNumber`, `SerialNumber`.
Erfüllt **AC-2** (WASI Kompilierung), **AC-3** (Bildobfuskation), **AC-4** (GPS Fuzzing) und **AC-5** (Seriennummern-Bereinigung).

### 3. SQLite Relationale Persistenz
In `crates/host/migrations/20261125000000_init.sql`:
```sql
ALTER TABLE photos ADD COLUMN is_redacted INTEGER DEFAULT 0;
ALTER TABLE photos ADD COLUMN redacted_file_path TEXT;
ALTER TABLE locations ADD COLUMN is_gps_fuzzed INTEGER DEFAULT 0;
```
In `crates/host/src/storage.rs`:
- Persistierung der Flags `is_redacted`, `redacted_file_path` und `is_gps_fuzzed`.
- Wiederherstellung im `UnifiedDocument`.

### 4. Integration Verification (AC-6)
In `crates/host/tests/privacy_test.rs`:
- Überprüfung von Bounding-Box-Verpixelung, GPS-Rundung und Entfernung sensitiver Metadaten.
Erfüllt **AC-6**: Vollständige Testabsicherung.
