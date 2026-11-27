# Test images for text recognition

No photos of the maintainer are stored here (ADR 0009, decision 8).

| File | Source | Author | Licence | Edit | Text |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `german-text.png` | generated for this project with ImageMagick (command below), font Arial (macOS) | WASM Geo Host contributors | CC0 1.0 (public domain dedication) | none | 4 lines |
| `strass.jpg` | <https://commons.wikimedia.org/wiki/File:Ortsschild_Stra%C3%9F_(Seeg).jpg> | Bruno413 | CC0 1.0 | downscaled with `sips -Z 1600 -s formatOptions 85` (longest side 1600 px) | place-name sign "Straß", "Rückholz" |

`german-text.png` was created with:

```bash
magick -size 1400x420 xc:white -font /System/Library/Fonts/Supplemental/Arial.ttf -pointsize 48 -fill black \
  -annotate +40+90 'Grüße aus München' -annotate +40+190 'Straße 12, Größe: 3,5 m' \
  -annotate +40+290 'Öffnungszeiten: Mo–Fr 8–18 Uhr' -annotate +40+390 'The quick brown fox jumps 2026' \
  -strip -define png:exclude-chunks=date,time german-text.png
```

SHA-256 of the files in this directory:

- `german-text.png`: `ed2167dc34c318fccf888122621713b3735a8eeb3746740af36d3e6e755e384c`
- `strass.jpg`: `7a81802b6de85d53394a70d0897b5cc9017b1b9ce30b2fae94290a378813b713`
