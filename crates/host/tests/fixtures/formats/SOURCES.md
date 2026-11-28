# Test images for image-convert

Derived from `../faces/apollo11.jpg` (NASA, Wikimedia Commons, public domain; see `../faces/SOURCES.md`) with ImageMagick 7.1.2-35 and libheif 1.23.7 (HEIC: x265 4.3, AVIF: AOM 3.15.3). `generate.sh` regenerates every file; `make_icc.py` writes the self-made ICC profile `test-gamma22.icc` (sRGB primaries, gamma 2.2; CC0). Encoder output is not bit-identical across versions, so update the SHA-256 below after regenerating. No photos of the maintainer are stored here (ADR 0009, decision 8).

| File | Licence | Generation (`generate.sh`) | Content |
| :--- | :--- | :--- | :--- |
| `apollo11.png` | Public domain | `magick ../faces/apollo11.jpg -resize 640x` | reference, 640 × 503 |
| `apollo11.tiff` | Public domain | from `apollo11.png`, `-compress zip` | TIFF, Deflate |
| `apollo11.bmp` | Public domain | from `apollo11.png`, `BMP3:` | BMP, 24 bit |
| `apollo11.webp` | Public domain | from `apollo11.png`, `-quality 90` | WebP, lossy |
| `apollo11.heic` | Public domain | from `apollo11.png`, `-quality 80` | HEIC, 8 bit |
| `apollo11.avif` | Public domain | from `apollo11.png`, `-quality 80` | AVIF, 8 bit |
| `apollo11-10bit.avif` | Public domain | from `apollo11.png`, `-depth 10 -quality 80` | AVIF, 10 bit |
| `apollo11-alpha.png` | Public domain | `-resize 160x -alpha set -channel A -fx "i<40?0:1"` | 160 × 126, columns 0–39 transparent |
| `apollo11-icc.png` | Public domain | `-resize 160x -profile test-gamma22.icc` | PNG with `iCCP` |
| `apollo11-icc.heic` | Public domain | from `apollo11-icc.png`, `-quality 80` | HEIC with `colr prof` |
| `apollo11-rot90.jpg` | Public domain | from `apollo11.png`, `-orient RightTop -quality 90` | EXIF Orientation 6, pixels not rotated |
| `apollo11-rot90.heic` | Public domain | from `apollo11.png`, `-orient RightTop -quality 80` | `irot` 270° (counter-clockwise) |
| `apollo11-rot90.avif` | Public domain | from `apollo11.png`, `-orient RightTop -quality 80` | `irot` 270° (counter-clockwise) |
| `apollo11-trunc.heic` | Public domain | first 60 % of the bytes of `apollo11.heic` | truncated |
| `apollo11-trunc.avif` | Public domain | first 60 % of the bytes of `apollo11.avif` | truncated |
| `apollo11.gif` | CC0 | `magick -size 16x16 xc:red` | unsupported format (despite the name, a red square) |
| `gradient-48mp.heic` | CC0 | `magick -size 8064x6048 gradient:navy-orange -quality 50` | 48 MP for the limits test |
| `test-gamma22.icc` | CC0 | `python3 -I make_icc.py test-gamma22.icc` | ICC v2 RGB display profile |
| `generate.sh`, `make_icc.py` | CC0 | — | generators |

SHA-256 of the files in this directory:

- `apollo11-10bit.avif`: `60a151e48bfeef68edcf00116321f23491b27f24b043a281e3d6a19a761ece86`
- `apollo11-alpha.png`: `fbbea5220690aca8ef8a7b5985f3f8986ea4d90984e2fb4eb557f0951a16e9a5`
- `apollo11-icc.heic`: `266a8d5e4b8bcc40514467171cc68dcf6d7fc9ee58d262e45251cee2e30b4b53`
- `apollo11-icc.png`: `0a5dd0c1f32af99b33c790a24bc357ac01a94f83f459ed48b64abb7f993cfa06`
- `apollo11-rot90.avif`: `e6a6f2545f88986ff223615471faf42de0bec3d9bb2fdfa5e04a2eba6d7e8cf3`
- `apollo11-rot90.heic`: `07a5c853e402c5c77fc697b35e93638e1a45ee58fbe3b767fba1bdd26e9186cb`
- `apollo11-rot90.jpg`: `678c4135406c81e9a72d489acda3e8f98e8fc7ad5ce2861740b681a9639f0abf`
- `apollo11-trunc.avif`: `5913a671a64a177e29ab7ae91e08692070e9c5e6cf58b17af1c9a507ff73cc24`
- `apollo11-trunc.heic`: `3e4cb529f57aa615b03eb5ce8a64f097be8f87eb46eaf5dccb5fecd21cc8508a`
- `apollo11.avif`: `db96e5aee24b93a8b72eec274817cd16b8a003a50e404ba5690bdeacfccd8928`
- `apollo11.bmp`: `103fffa99d720c721714e5cb8444f87ae693d1d3be591a89ca9483ac0f8649a8`
- `apollo11.gif`: `51b363caaaa625821cdb63ea9ca15dcc647b53aa109acaf6fd42df016868ca35`
- `apollo11.heic`: `68a0f9cfa1dce1054aceeb882efa1273dc7d35ce6594e9e488a199e8352f170f`
- `apollo11.png`: `76c0deca14453285e57891d3e7e38ee3a1c5df3d0c4d63af1c05abd54b7fb53e`
- `apollo11.tiff`: `a52e45b84a27fa5153e462d1c56c9eb097476765803315da447b2f2e7c032fa2`
- `apollo11.webp`: `ce21c69f88b767459e6cb917eb7780677bde2af3c150df94527be45a56ec8d98`
- `generate.sh`: `a2f8318d54b4bc569b95dd8a6e51ac384d32cfcbf50c093518849b736ee5c35f`
- `gradient-48mp.heic`: `7ef40be3d5aefa78ac5adf54a97e2f4264318164724c378642b89114a56dcbbf`
- `make_icc.py`: `9b4fa09e1f4cc6309c3bdf104fd44f587f2fea9d59921480d9481eb5717521b3`
- `test-gamma22.icc`: `688bc06bbb02768226ca63a5d0027c5046c0f43dc8eec8bf068ee4226ac3d16d`
