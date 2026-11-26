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

### 2a. Pixelgenaue Bildredaktion (AC-7 bis AC-11, Issue #22)

Ersetzt die frühere Manipulation kodierter Bytes. Ablauf in `PrivacyRedactPlugin::process`, nur wenn `redact_faces = true` und `doc.faces` mindestens ein Gesicht enthält:

1. **Lesen** über `ctx.capabilities.fs_read(doc.file.path)`. Kein direkter `std::fs`-Zugriff (Verfassung Security 2).
2. **Dekodieren** mit der Crate `image` (reines Rust, `default-features = false`, Feature `jpeg`), anschließend `to_rgb8()`. Unterstützt wird nur JPEG.
3. **Rechteck berechnen** mit `bbox_to_rect(bbox, w, h) -> Option<PixelRect>`: Koordinaten auf `[0, 1]` klemmen, `x0 = floor(x·w + ε)`, `x1 = ceil((x+width)·w − ε)` mit ε = 10⁻³ px gegen f32-Darstellungsfehler (analog für y), damit das Gesicht vollständig abgedeckt ist. Nicht endliche Werte oder eine leere Fläche ergeben `None`; das Gesicht wird mit Warnung übersprungen. Die Box bezieht sich auf das gespeicherte Pixelraster, die EXIF-Orientierung wird nicht ausgewertet.
4. **Region bearbeiten** mit `redact_region(&mut RgbImage, PixelRect, style, block_size, blur_sigma)`. Pixel außerhalb von `rect` werden nicht verändert:
   - `Blackout`: alle Pixel auf `[0, 0, 0]`.
   - `Pixelate`: Raster mit `block_size × block_size` Pixeln, ausgerichtet am Ursprung des Rechtecks. Jeder Block (geschnitten mit dem Rechteck) wird mit dem Mittelwert seiner Pixel gefüllt.
   - `Blur`: Ausschnitt `crop_imm` → `imageops::blur(sub, blur_sigma)` (Gauß) → `imageops::replace` an derselben Position.
5. **Kodieren** als JPEG mit fester Qualität 90 (`JpegEncoder::new_with_quality`). Die Abmessungen bleiben erhalten. Die Kopie enthält keine Metadaten des Originals (kein EXIF/XMP/IPTC), sodass GPS und Seriennummern dort nicht wieder auftauchen.
6. **Schreiben** über `ctx.capabilities.fs_write(<output_dir>/<safe_id>.redacted.jpg)`. Der bisherige `std::fs`-Fallback entfällt (Verfassung Security 2, Issue #25).

Ergebnis und Fehlerverhalten (AC-11):

| Fall | Log | `applied_rules` | `faces_redacted_count` | `redacted_image_path` |
| :--- | :--- | :--- | :--- | :--- |
| Erfolg, `N` gültige Boxen | `info` | `redacted_<N>_faces_via_<style>` | `N` | `Some(..)` |
| Lesen/Dekodieren/Kodieren/Schreiben schlägt fehl | `warn` mit Ursache | — | `0` | `None` |
| Keine Box mit gültiger Fläche | `warn` | — | `0` | `None` |
| Einzelne ungültige Box | `warn` je Box | Regel mit `N` = gültige Boxen | `N` | `Some(..)` |

Erfüllt **AC-7** (nur Bounding-Box-Pixel verändert), **AC-8** (`blur_sigma`), **AC-9** (`block_size`), **AC-10** (gültiges JPEG gleicher Größe) und **AC-11** (ehrliche Protokollierung).

`is_redacted` ist genau dann `true`, wenn mindestens eine Regel angewandt wurde. GPS-Fuzzing und Seriennummern-Entfernung laufen unabhängig vom Ergebnis der Bildredaktion weiter.

Tests (`crates/host/tests/privacy_test.rs`) erzeugen ihre JPEG-Quellen selbst mit `image` (Dev-Dependency des Hosts). AC-7 und AC-9 prüfen `redact_region` exakt auf dem Pixelpuffer und zusätzlich End-to-End mit Toleranz. Beim End-to-End-Vergleich bleibt ein Rand von 8 px um jede Box (JPEG-Blockgröße) unberücksichtigt. Als Kodiertoleranz gilt die Abweichung einer reinen Neukodierung der Quelle mit `REDACTED_JPEG_QUALITY` (×1,1 + 0,5).

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
