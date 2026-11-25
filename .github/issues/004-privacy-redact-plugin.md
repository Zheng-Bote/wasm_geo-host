# Feature Spec: Privacy Redaction & Sanitizing Plugin (`flagme-plugin-privacy-redact`)

- **Feature Name**: `privacy-redact`
- **Component**: `plugins/privacy-redact`, `crates/models`, `crates/host`
- **Issue Type**: New Feature / Plugin Proposal

---

## 1. Problem Statement & Motivation
Media libraries shared publicly (via blogs, cloud galleries, or public archives) frequently contain sensitive personal data subject to privacy regulations (e.g. GDPR/DSGVO) and security concerns:
1. **Unintended Identifiable Persons & Vehicles**: Bystanders' faces and vehicle license plates appear in street/travel photography.
2. **Exact Domestic Geolocation Exposure**: High-precision GPS coordinates reveal exact private residences and workplaces down to the meter.
3. **Hardware Device Tracking**: Camera and lens serial numbers embedded in EXIF tags uniquely track photographers across platforms.

A dedicated `privacy-redact` plugin automates visual pixel obfuscation and metadata sanitization before media files or thumbnails are published.

---

## 2. Technical Architecture & Pipeline Integration

The `privacy-redact` plugin executes downstream of detection and metadata extraction plugins:

```
[metadata] ──> [reversegeo] ──> [face-detection] 
                                       │
                                       ▼
                         [flagme-plugin-privacy-redact]
                           ├── Visual Redaction (Pixelate / Blur / Blackout)
                           │     └── Writes sanitized variant to disk
                           └── Metadata Sanitization
                                 ├── GPS Fuzzing (Rounds lat/lon precision)
                                 └── Strips hardware serial numbers
                                       │
                                       ▼
                               [thumbnail / export]
```

---

## 3. Configuration Specification (`config.yaml`)

```yaml
plugins:
  privacy-redact:
    enabled: true
    capabilities:
      - filesystem
    settings:
      # Visual obfuscation targets
      redact_faces: "true"              # Consumes doc.faces bounding boxes
      redact_license_plates: "true"     # Optical plate pattern detection
      
      # Redaction style: "pixelate" | "blur" | "blackout"
      redaction_style: "pixelate"
      pixel_size: "20"                  # Block dimension for mosaic pixelation
      blur_sigma: "15.0"                # Blur radius for Gaussian mode
      
      # Metadata sanitization
      strip_serial_numbers: "true"      # Strips CameraSerialNumber, LensSerialNumber
      fuzz_gps_coordinates: "true"      # Lowers GPS decimal resolution
      gps_precision_decimals: "2"       # 2 decimals = ~1.1 km radius (protects exact home address)
      
      # Storage & Execution mode
      target_mode: "variant"            # "variant" (saves to cache/redacted/) | "in_place"
      output_folder: "cache/redacted"
```

---

## 4. Visual Redaction Algorithms

- **`pixelate` (Default)**: Downscales the bounding box region by factor `pixel_size` and re-upscales via nearest-neighbor interpolation. Irreversible and clearly visible.
- **`blur`**: Applies a 2D Gaussian kernel with configurable standard deviation (`blur_sigma`).
- **`blackout`**: Destructively replaces the rectangular or polygonal region with `#000000` (100% cryptographic security guarantee).

---

## 5. Data Model Output (`wasm-geo-models`)

Add `PrivacyInfo` to `UnifiedDocument`:

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

---

## 6. SQLite Persistence (`media.db`)

Add privacy audit columns to the `photos` and `locations` persistence:

```sql
ALTER TABLE photos ADD COLUMN is_redacted INTEGER DEFAULT 0;
ALTER TABLE photos ADD COLUMN redacted_file_path TEXT;
ALTER TABLE locations ADD COLUMN is_gps_fuzzed INTEGER DEFAULT 0;
```

---

## 7. Acceptance Criteria
- [ ] **AC-1**: `wasm-geo-models` introduces `PrivacyInfo` and serializes it in `UnifiedDocument.privacy`.
- [ ] **AC-2**: `flagme-plugin-privacy-redact` compiles to both `cdylib` and standalone `[[bin]]` under `wasm32-wasip2`.
- [ ] **AC-3**: Visual obfuscation destructively renders mosaic pixelation and blackout rectangles over input bounding boxes.
- [ ] **AC-4**: GPS coordinates are fuzzed to the configured number of decimal places when `fuzz_gps_coordinates` is active.
- [ ] **AC-5**: Hardware serial numbers (`CameraSerialNumber`, `LensSerialNumber`) are purged from `raw_tags` when `strip_serial_numbers` is active.
- [ ] **AC-6**: Integration tests verify that exported redacted image variants do not contain recoverable facial pixels.
