# Privacy Redaction & Sanitizing Plugin (`flagme-plugin-privacy-redact`)

The **Privacy Redaction & Sanitizing Plugin** provides automated pixel obfuscation and metadata sanitization to protect user privacy (GDPR/DSGVO) prior to media export or sharing.

---

## 1. Features
- **Visual Redaction**:
  - `Pixelate` (Default): Downscales face/plate regions into mosaic blocks.
  - `Blackout`: Irreversibly replaces sensitive bounding boxes with solid black rects.
- **GPS Fuzzing**: Rounds high-precision coordinates to configured decimal places (e.g. 2 decimals ~1 km accuracy) to protect private residences.
- **Hardware Sanitization**: Purges camera and lens serial numbers from `raw_tags`.
- **WASI 0.2 Component**: Targets `wasm32-wasip2`.

---

## 2. Capabilities Required
- `filesystem`: Reading and writing sanitized media variants.

---

## 3. Data Output (`UnifiedDocument.privacy`)
```json
{
  "is_redacted": true,
  "faces_redacted_count": 2,
  "license_plates_redacted_count": 0,
  "original_gps_fuzzed": true,
  "redacted_image_path": "/media/sample.jpg.redacted.jpg",
  "applied_rules": [
    "redacted_2_faces_via_pixelate",
    "fuzzed_gps_precision_2_decimals",
    "stripped_hardware_serial_numbers"
  ]
}
```
