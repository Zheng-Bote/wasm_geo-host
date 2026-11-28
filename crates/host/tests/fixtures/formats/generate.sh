#!/bin/sh
# Regenerates the test images in this directory from ../faces/apollo11.jpg (NASA, public domain).
# Needs ImageMagick 7 with libheif (HEIC, AVIF) and python3. Output is not bit-identical across
# encoder versions; update SOURCES.md (SHA-256) after regenerating.
set -eu
cd "$(dirname "$0")"
python3 -I make_icc.py test-gamma22.icc
magick ../faces/apollo11.jpg -resize 640x apollo11.png
magick apollo11.png -compress zip apollo11.tiff
magick apollo11.png BMP3:apollo11.bmp
magick apollo11.png -quality 90 apollo11.webp
magick apollo11.png -quality 80 apollo11.heic
magick apollo11.png -quality 80 apollo11.avif
magick apollo11.png -depth 10 -quality 80 apollo11-10bit.avif
magick apollo11.png -resize 160x -alpha set -channel A -fx "i<40?0:1" +channel apollo11-alpha.png
magick apollo11.png -resize 160x -profile test-gamma22.icc apollo11-icc.png
magick apollo11-icc.png -quality 80 apollo11-icc.heic
magick apollo11.png -orient RightTop -quality 90 apollo11-rot90.jpg
magick apollo11.png -orient RightTop -quality 80 apollo11-rot90.heic
magick apollo11.png -orient RightTop -quality 80 apollo11-rot90.avif
for f in apollo11.heic apollo11.avif; do
  n=$(wc -c < "$f"); head -c $((n * 6 / 10)) "$f" > "apollo11-trunc.${f##*.}"
done
magick -size 16x16 xc:red apollo11.gif
magick -size 8064x6048 gradient:navy-orange -quality 50 gradient-48mp.heic
