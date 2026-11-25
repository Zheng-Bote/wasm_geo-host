# Metadata Plugin (`metadata.wasm`)

## Purpose
Extracts all available EXIF, IPTC, and XMP metadata from incoming media files in pure Rust without any metadata loss.

## Capabilities Required
- `filesystem`: Required to read raw image headers and byte streams from the host.

## Output
Enriches `UnifiedDocument.metadata` with:
- `camera_make`, `camera_model`, `lens_model`
- Exposure parameters: `iso`, `focal_length`, `f_number`, `exposure_time`
- `raw_tags`: Complete dictionary preserving every vendor tag, IPTC record, and XMP property.
