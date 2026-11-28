//! Detection of the source format from the file signature (not the file extension).

/// Input formats `image-convert` can turn into a working image (ADR 0011).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Jpeg,
    Png,
    Tiff,
    Bmp,
    Webp,
    Heic,
    Avif,
}

/// HEIF brands of HEVC-coded still images and sequences.
const HEIC_BRANDS: [&[u8; 4]; 7] = [b"heic", b"heix", b"heim", b"heis", b"hevc", b"hevx", b"mif1"];
/// Brands of AV1-coded HEIF files.
const AVIF_BRANDS: [&[u8; 4]; 2] = [b"avif", b"avis"];

impl SourceFormat {
    /// Detects the format from the leading bytes; `None` for anything else (e.g. GIF).
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Some(Self::Jpeg);
        }
        if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
            return Some(Self::Png);
        }
        if bytes.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || bytes.starts_with(&[0x4D, 0x4D, 0x00, 0x2A]) {
            return Some(Self::Tiff);
        }
        if bytes.starts_with(b"BM") && bytes.len() >= 26 {
            return Some(Self::Bmp);
        }
        if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            return Some(Self::Webp);
        }
        Self::sniff_heif(bytes)
    }

    /// `ftyp` box at offset 4: AVIF if any brand is an AV1 brand, HEIC if any is an HEVC brand.
    fn sniff_heif(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 16 || &bytes[4..8] != b"ftyp" {
            return None;
        }
        let size = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        if size < 16 || size > bytes.len() {
            return None;
        }
        // Major brand at 8..12, minor version at 12..16, compatible brands from 16.
        let mut brands = vec![&bytes[8..12]];
        brands.extend(bytes[16..size].chunks_exact(4));
        if brands.iter().any(|b| AVIF_BRANDS.iter().any(|a| *b == &a[..])) {
            return Some(Self::Avif);
        }
        if brands.iter().any(|b| HEIC_BRANDS.iter().any(|h| *b == &h[..])) {
            return Some(Self::Heic);
        }
        None
    }

    /// Name stored in `working_image.source_format`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Jpeg => "jpeg",
            Self::Png => "png",
            Self::Tiff => "tiff",
            Self::Bmp => "bmp",
            Self::Webp => "webp",
            Self::Heic => "heic",
            Self::Avif => "avif",
        }
    }
}
