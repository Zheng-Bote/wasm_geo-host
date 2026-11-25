# Thumbnail Plugin (`thumbnail.wasm`)

## Purpose
Generates deterministically cached thumbnail variants for media items across various image resolutions.

## Supported Formats
- JPEG, PNG, WebP, TIFF, HEIC, AVIF

## Resolutions Generated
- `128px` (small)
- `512px` (medium)
- `1024px` (large)

## Caching Strategy
Every generated thumbnail receives a deterministic cache key derived from the original file hash and dimension.
