//! Decoding, orientation and JPEG encoding of the working image (ADR 0011).

use crate::detect::SourceFormat;
use image::codecs::jpeg::JpegEncoder;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageFormat, ImageReader, RgbImage, RgbaImage};
use std::io::Cursor;

/// Decoded, oriented image without alpha, plus the source's ICC profile.
pub struct Decoded {
    pub rgb: RgbImage,
    pub icc: Option<Vec<u8>>,
}

fn image_format(fmt: SourceFormat) -> Option<ImageFormat> {
    match fmt {
        SourceFormat::Jpeg => Some(ImageFormat::Jpeg),
        SourceFormat::Png => Some(ImageFormat::Png),
        SourceFormat::Tiff => Some(ImageFormat::Tiff),
        SourceFormat::Bmp => Some(ImageFormat::Bmp),
        SourceFormat::Webp => Some(ImageFormat::WebP),
        SourceFormat::Heic | SourceFormat::Avif => None,
    }
}

/// Pixel count of the image as stored, read from the header without decoding. Rotation does not
/// change it, so it is enough for the `max_pixels` check.
pub fn pixel_count(bytes: &[u8], fmt: SourceFormat) -> Result<u64, String> {
    let (w, h) = match fmt {
        SourceFormat::Heic => {
            let info = heic_rs::probe(bytes).map_err(|e| format!("cannot read HEIC header: {}", e))?;
            (info.width, info.height)
        }
        SourceFormat::Avif => {
            let info = oxideav_avif::info(bytes).map_err(|e| format!("cannot read AVIF header: {}", e))?;
            (info.width, info.height)
        }
        _ => {
            let format = image_format(fmt).expect("raster format");
            ImageReader::with_format(Cursor::new(bytes), format)
                .into_dimensions()
                .map_err(|e| format!("cannot read {} header: {}", fmt.as_str(), e))?
        }
    };
    Ok(w as u64 * h as u64)
}

/// Lays RGBA pixels on white: `c' = (c·a + 255·(255−a)) / 255`, rounded.
pub fn flatten_on_white(rgba: &RgbaImage) -> RgbImage {
    RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let p = rgba.get_pixel(x, y).0;
        let a = p[3] as u32;
        let mix = |c: u8| ((c as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
        image::Rgb([mix(p[0]), mix(p[1]), mix(p[2])])
    })
}

fn to_rgb(img: DynamicImage) -> RgbImage {
    if img.color().has_alpha() {
        flatten_on_white(&img.to_rgba8())
    } else {
        img.to_rgb8()
    }
}

fn rgba_from_raw(width: u32, height: u32, data: Vec<u8>, what: &str) -> Result<RgbaImage, String> {
    RgbaImage::from_raw(width, height, data)
        .ok_or_else(|| format!("{} decoder returned a buffer that does not match {}×{}", what, width, height))
}

/// Decodes `bytes` and applies the orientation: EXIF Orientation for JPEG, PNG, TIFF, BMP and
/// WebP; the container transforms (`irot`, `imir`, `clap`) for HEIC and AVIF, whose decoders apply
/// them. EXIF Orientation inside HEIF files is not applied on top (ADR 0011, decision 3).
pub fn decode_oriented(bytes: &[u8], fmt: SourceFormat) -> Result<Decoded, String> {
    match fmt {
        SourceFormat::Heic => {
            let options = heic_rs::DecodeOptions {
                layout: heic_rs::PixelLayout::Rgba8,
                apply_transforms: true,
                threads: Some(1),
                ..Default::default()
            };
            let img = heic_rs::decode(bytes, &options).map_err(|e| format!("cannot decode HEIC image: {}", e))?;
            let icc = heic_icc(bytes);
            let rgba = rgba_from_raw(img.width, img.height, img.data, "HEIC")?;
            Ok(Decoded { rgb: flatten_on_white(&rgba), icc })
        }
        SourceFormat::Avif => {
            let img = oxideav_avif::decode(bytes).map_err(|e| format!("cannot decode AVIF image: {}", e))?;
            let data = match img.try_to_rgba8() {
                Ok(data) => data,
                // Matrix without an exact kernel: the crate's documented BT.601 fallback.
                Err(oxideav_avif::Error::Unsupported(_)) => img.to_rgba8(),
                Err(e) => return Err(format!("cannot convert AVIF image to RGB: {}", e)),
            };
            let rgba = rgba_from_raw(img.width, img.height, data, "AVIF")?;
            Ok(Decoded { rgb: flatten_on_white(&rgba), icc: img.metadata.icc.clone() })
        }
        _ => {
            let format = image_format(fmt).expect("raster format");
            let mut decoder = ImageReader::with_format(Cursor::new(bytes), format)
                .into_decoder()
                .map_err(|e| format!("cannot decode {} image: {}", fmt.as_str(), e))?;
            let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
            let icc = decoder.icc_profile().ok().flatten();
            let mut img = DynamicImage::from_decoder(decoder)
                .map_err(|e| format!("cannot decode {} image: {}", fmt.as_str(), e))?;
            img.apply_orientation(orientation);
            Ok(Decoded { rgb: to_rgb(img), icc })
        }
    }
}

/// ICC profile of the primary HEIC item, if its `colr` box carries one.
fn heic_icc(bytes: &[u8]) -> Option<Vec<u8>> {
    let info = heic_rs::probe(bytes).ok()?;
    let ctx = heic_rs::context::Context::open(bytes).ok()?;
    let props = ctx.props(info.primary_item).ok()?;
    ctx.icc(&props).map(|icc| icc.to_vec())
}

/// Baseline JPEG with the ICC profile (if any) and no other metadata (no EXIF, GPS or XMP).
pub fn encode_jpeg(decoded: &Decoded, quality: u8) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut out, quality);
    if let Some(icc) = &decoded.icc {
        encoder
            .set_icc_profile(icc.clone())
            .map_err(|e| format!("cannot embed ICC profile: {}", e))?;
    }
    let rgb = &decoded.rgb;
    encoder
        .write_image(rgb.as_raw(), rgb.width(), rgb.height(), image::ExtendedColorType::Rgb8)
        .map_err(|e| format!("cannot encode JPEG: {}", e))?;
    Ok(out)
}

/// `true` if an end-of-image marker (`FF D9`) follows the first start-of-scan marker (`FF DA`).
/// Copy of the check in `face-detection` (constitution, Architecture/Layering 3).
pub fn jpeg_is_complete(data: &[u8]) -> bool {
    let Some(sos) = data.windows(2).position(|w| w == [0xFF, 0xDA]) else {
        return false;
    };
    data[sos + 2..].windows(2).any(|w| w == [0xFF, 0xD9])
}

/// Dimensions and EXIF orientation of a JPEG, read without decoding the scan.
pub fn jpeg_header(bytes: &[u8]) -> Result<((u32, u32), Orientation), String> {
    let mut decoder = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Jpeg)
        .into_decoder()
        .map_err(|e| format!("cannot read JPEG header: {}", e))?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    Ok((decoder.dimensions(), orientation))
}
