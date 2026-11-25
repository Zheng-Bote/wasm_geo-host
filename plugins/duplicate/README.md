# Duplicate Plugin (`duplicate.wasm`)

## Purpose
Identifies exact file duplicates and perceptual visual matches.

## Algorithms
- **SHA256**: Cryptographic digest for byte-for-byte identity.
- **pHash / dHash**: Perceptual hash for invariant visual similarity.

## Capabilities Required
- `filesystem`: To read the binary stream of the image.
- `kv`: To query and update the persistent hash index.
