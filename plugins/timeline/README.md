# Timeline Plugin (`timeline.wasm`)

## Purpose
Normalizes temporal attributes across EXIF tags, XMP blocks, and filesystem metadata.

## Conflict Resolution Strategy
1. **EXIF `DateTimeOriginal`**: Camera shutter time (highest priority).
2. **XMP `CreateDate`**: Authoring timestamp.
3. **Filesystem `Modified`**: File write timestamp.
4. **Host `Imported`**: Time of arrival in the WASM Geo Host pipeline.
