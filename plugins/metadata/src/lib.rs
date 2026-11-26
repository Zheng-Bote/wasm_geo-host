use flagme_sdk::{
    serde_json, MetadataInfo, Plugin, PluginContext, PluginResult, PluginSettings, SettingsCache,
    SettingsReader, UnifiedDocument,
};
use std::collections::HashMap;
use std::io::Cursor;

/// Configurable parameters of the metadata plugin (`config.yaml` `settings`).
#[derive(Debug, Clone, PartialEq)]
pub struct MetadataSettings {
    /// MakerNote values longer than this are replaced by `<MakerNote <n> bytes>`.
    pub makernote_max_length: usize,
    /// XMP values longer than this are dropped.
    pub xmp_max_value_length: usize,
}

impl Default for MetadataSettings {
    fn default() -> Self {
        Self {
            makernote_max_length: 512,
            xmp_max_value_length: 512,
        }
    }
}

impl PluginSettings for MetadataSettings {
    const PLUGIN: &'static str = "metadata";

    fn from_reader(base: &Self, r: &mut SettingsReader<'_>) -> Self {
        Self {
            makernote_max_length: r.u32("makernote_max_length", &[], 64..=65536, base.makernote_max_length as u32)
                as usize,
            xmp_max_value_length: r.u32("xmp_max_value_length", &[], 64..=65536, base.xmp_max_value_length as u32)
                as usize,
        }
    }
}

/// Metadata plugin extracting EXIF, IPTC, and XMP metadata without loss.
pub struct MetadataPlugin {
    settings: SettingsCache<MetadataSettings>,
}

impl MetadataPlugin {
    pub fn new() -> Self {
        Self::with_settings(MetadataSettings::default())
    }

    pub fn with_settings(settings: MetadataSettings) -> Self {
        Self {
            settings: SettingsCache::new(settings),
        }
    }

    /// Pure Rust parser extracting all EXIF, IPTC, and XMP metadata tags from raw image bytes,
    /// using the constructor settings.
    pub fn parse_image_metadata(
        &self,
        bytes: &[u8],
    ) -> (MetadataInfo, Option<f64>, Option<f64>, Option<f64>) {
        self.parse_image_metadata_with(bytes, self.settings.base())
    }

    /// Like [`Self::parse_image_metadata`], with explicit settings.
    pub fn parse_image_metadata_with(
        &self,
        bytes: &[u8],
        settings: &MetadataSettings,
    ) -> (MetadataInfo, Option<f64>, Option<f64>, Option<f64>) {
        let mut info = MetadataInfo::default();
        let mut raw_tags = HashMap::new();
        let mut exif_obj = serde_json::Map::new();
        let mut iptc_obj = serde_json::Map::new();
        let mut xmp_obj = serde_json::Map::new();

        // 1. Detect Container Format & Dimensions
        if bytes.len() > 4 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
            raw_tags.insert("Format".to_string(), "JPEG".to_string());
            if let Some((w, h)) = parse_jpeg_dimensions(bytes) {
                raw_tags.insert("Image.Width".to_string(), w.to_string());
                raw_tags.insert("Image.Height".to_string(), h.to_string());
            }
        } else if bytes.len() > 24 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
            raw_tags.insert("Format".to_string(), "PNG".to_string());
            let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
            let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
            raw_tags.insert("Image.Width".to_string(), w.to_string());
            raw_tags.insert("Image.Height".to_string(), h.to_string());
        } else if bytes.len() > 12 && &bytes[4..8] == b"ftyp" {
            raw_tags.insert("Format".to_string(), "HEIC/AVIF".to_string());
        } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            raw_tags.insert("Format".to_string(), "WebP".to_string());
        }

        // 2. Extract Real EXIF using kamadak-exif (plus fallback raw TIFF scan for PNG/HEIC eXIf chunks)
        let mut lat = None;
        let mut lon = None;
        let mut alt = None;

        let exif_reader = {
            let mut cursor = Cursor::new(bytes);
            exif::Reader::new()
                .read_from_container(&mut cursor)
                .ok()
                .or_else(|| {
                    find_embedded_tiff_header(bytes)
                        .and_then(|tiff_slice| exif::Reader::new().read_raw(tiff_slice.to_vec()).ok())
                })
        };

        if let Some(reader) = exif_reader {
            let mut gps_lat_deg: Option<f64> = None;
            let mut gps_lat_ref: Option<String> = None;
            let mut gps_lon_deg: Option<f64> = None;
            let mut gps_lon_ref: Option<String> = None;

            for f in reader.fields() {
                let tag_name = format!("{}", f.tag);
                // Omit huge binary MakerNote dumps (> makernote_max_length chars) from cluttering text fields
                let raw_val_str = f.display_value().with_unit(&reader).to_string();
                let clean_val = raw_val_str.trim().trim_matches('"').trim().to_string();

                let display_val = if clean_val.len() > settings.makernote_max_length && f.tag == exif::Tag::MakerNote {
                    format!("<MakerNote {} bytes>", clean_val.len())
                } else {
                    clean_val.clone()
                };

                raw_tags.insert(format!("Exif.{}", tag_name), display_val.clone());
                exif_obj.insert(tag_name.clone(), serde_json::Value::String(display_val.clone()));

                match f.tag {
                    exif::Tag::Make => {
                        if !clean_val.is_empty() {
                            info.camera_make = Some(clean_val);
                        }
                    }
                    exif::Tag::Model => {
                        if !clean_val.is_empty() {
                            info.camera_model = Some(clean_val);
                        }
                    }
                    exif::Tag::LensModel => {
                        if !clean_val.is_empty() {
                            info.lens_model = Some(clean_val);
                        }
                    }
                    exif::Tag::PhotographicSensitivity | exif::Tag::StandardOutputSensitivity => {
                        if let exif::Value::Short(ref v) = f.value {
                            if let Some(&first) = v.first() {
                                info.iso = Some(first as u32);
                            }
                        } else if let exif::Value::Long(ref v) = f.value {
                            if let Some(&first) = v.first() {
                                info.iso = Some(first);
                            }
                        }
                    }
                    exif::Tag::FocalLength => {
                        if let exif::Value::Rational(ref v) = f.value {
                            if let Some(first) = v.first() {
                                if first.denom != 0 {
                                    info.focal_length = Some(first.num as f32 / first.denom as f32);
                                }
                            }
                        }
                    }
                    exif::Tag::FNumber => {
                        if let exif::Value::Rational(ref v) = f.value {
                            if let Some(first) = v.first() {
                                if first.denom != 0 {
                                    info.f_number = Some(first.num as f32 / first.denom as f32);
                                }
                            }
                        }
                    }
                    exif::Tag::ExposureTime => {
                        info.exposure_time = Some(raw_val_str);
                    }
                    exif::Tag::DateTimeOriginal => {
                        raw_tags.insert("Exif.Photo.DateTimeOriginal".to_string(), clean_val.clone());
                        exif_obj.insert("Photo.DateTimeOriginal".to_string(), serde_json::Value::String(clean_val));
                    }
                    exif::Tag::DateTimeDigitized => {
                        raw_tags.insert("Exif.Photo.DateTimeDigitized".to_string(), clean_val.clone());
                        exif_obj.insert("Photo.DateTimeDigitized".to_string(), serde_json::Value::String(clean_val));
                    }
                    exif::Tag::DateTime => {
                        raw_tags.insert("Exif.Image.DateTime".to_string(), clean_val.clone());
                        exif_obj.insert("Image.DateTime".to_string(), serde_json::Value::String(clean_val));
                    }
                    exif::Tag::BodySerialNumber => {
                        raw_tags.insert("CameraSerialNumber".to_string(), clean_val.clone());
                        exif_obj.insert("CameraSerialNumber".to_string(), serde_json::Value::String(clean_val));
                    }
                    exif::Tag::LensSerialNumber => {
                        raw_tags.insert("LensSerialNumber".to_string(), clean_val.clone());
                        exif_obj.insert("LensSerialNumber".to_string(), serde_json::Value::String(clean_val));
                    }
                    exif::Tag::GPSLatitude => {
                        if let exif::Value::Rational(ref v) = f.value {
                            if v.len() >= 3 && v[0].denom != 0 && v[1].denom != 0 && v[2].denom != 0 {
                                let deg = v[0].num as f64 / v[0].denom as f64;
                                let min = v[1].num as f64 / v[1].denom as f64;
                                let sec = v[2].num as f64 / v[2].denom as f64;
                                gps_lat_deg = Some(deg + min / 60.0 + sec / 3600.0);
                            }
                        }
                    }
                    exif::Tag::GPSLatitudeRef => {
                        gps_lat_ref = Some(clean_val.to_uppercase());
                    }
                    exif::Tag::GPSLongitude => {
                        if let exif::Value::Rational(ref v) = f.value {
                            if v.len() >= 3 && v[0].denom != 0 && v[1].denom != 0 && v[2].denom != 0 {
                                let deg = v[0].num as f64 / v[0].denom as f64;
                                let min = v[1].num as f64 / v[1].denom as f64;
                                let sec = v[2].num as f64 / v[2].denom as f64;
                                gps_lon_deg = Some(deg + min / 60.0 + sec / 3600.0);
                            }
                        }
                    }
                    exif::Tag::GPSLongitudeRef => {
                        gps_lon_ref = Some(clean_val.to_uppercase());
                    }
                    exif::Tag::GPSAltitude => {
                        if let exif::Value::Rational(ref v) = f.value {
                            if let Some(first) = v.first() {
                                if first.denom != 0 {
                                    alt = Some(first.num as f64 / first.denom as f64);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            if let Some(mut lat_val) = gps_lat_deg {
                if let Some(ref r) = gps_lat_ref {
                    if r.contains('S') {
                        lat_val = -lat_val;
                    }
                }
                lat = Some(lat_val);
            }

            if let Some(mut lon_val) = gps_lon_deg {
                if let Some(ref r) = gps_lon_ref {
                    if r.contains('W') {
                        lon_val = -lon_val;
                    }
                }
                lon = Some(lon_val);
            }
        }

        // 3. Extract all IPTC (IIM / 8BIM 0x0404 & PNG tEXt IPTC) fields
        extract_iptc_tags(bytes, &mut raw_tags, &mut iptc_obj);

        // 4. Extract all XMP (RDF attributes and elements) fields
        extract_xmp_tags(bytes, &mut raw_tags, &mut xmp_obj, settings.xmp_max_value_length);

        // Store structured objects "Exif", "IPTC", and "XMP" in raw_tags
        raw_tags.insert(
            "Exif".to_string(),
            serde_json::Value::Object(exif_obj).to_string(),
        );
        raw_tags.insert(
            "IPTC".to_string(),
            serde_json::Value::Object(iptc_obj).to_string(),
        );
        raw_tags.insert(
            "XMP".to_string(),
            serde_json::Value::Object(xmp_obj).to_string(),
        );

        info.raw_tags = raw_tags;
        (info, lat, lon, alt)
    }
}

fn find_embedded_tiff_header(bytes: &[u8]) -> Option<&[u8]> {
    // Search for 'Exif\0\0' followed by 'II*\0' or 'MM\0*'
    for i in 0..bytes.len().saturating_sub(14) {
        if &bytes[i..i + 6] == b"Exif\0\0" {
            let tiff = &bytes[i + 6..];
            if tiff.starts_with(b"II\x2a\x00") || tiff.starts_with(b"MM\x00\x2a") {
                return Some(tiff);
            }
        }
        // Or PNG 'eXIf' chunk
        if &bytes[i..i + 4] == b"eXIf" && i >= 4 {
            let len = u32::from_be_bytes([bytes[i - 4], bytes[i - 3], bytes[i - 2], bytes[i - 1]]) as usize;
            let start = i + 4;
            if start + len <= bytes.len() {
                return Some(&bytes[start..start + len]);
            }
        }
    }
    None
}

fn parse_jpeg_dimensions(bytes: &[u8]) -> Option<(u16, u16)> {
    let mut i = 2;
    while i + 8 < bytes.len() {
        if bytes[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = bytes[i + 1];
        if marker == 0xD9 || marker == 0xDA {
            break;
        }
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        if len < 2 {
            break;
        }
        if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) && i + 8 < bytes.len() {
            let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]);
            let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]);
            if w > 0 && h > 0 {
                return Some((w, h));
            }
        }
        i += 2 + len;
    }
    None
}

/// Extracts all IPTC-NAA IIM Application2 datasets (from 8BIM 0x0404 blocks and PNG tEXt chunks).
fn extract_iptc_tags(
    bytes: &[u8],
    raw_tags: &mut HashMap<String, String>,
    iptc_obj: &mut serde_json::Map<String, serde_json::Value>,
) {
    // 1. Scan for Photoshop 8BIM resource 0x0404 (IPTC-NAA)
    let mut i = 0;
    while i + 12 < bytes.len() {
        if &bytes[i..i + 6] == b"8BIM\x04\x04" {
            let name_len = bytes[i + 6] as usize;
            // Pascal string padded to even number of bytes (including length byte)
            let padded_name_len = if (name_len + 1) % 2 == 0 {
                name_len + 1
            } else {
                name_len + 2
            };
            let size_offset = i + 6 + padded_name_len;
            if size_offset + 4 <= bytes.len() {
                let data_len = u32::from_be_bytes([
                    bytes[size_offset],
                    bytes[size_offset + 1],
                    bytes[size_offset + 2],
                    bytes[size_offset + 3],
                ]) as usize;
                let data_start = size_offset + 4;
                let data_end = (data_start + data_len).min(bytes.len());
                if data_start < data_end {
                    parse_iptc_iim_block(&bytes[data_start..data_end], raw_tags, iptc_obj);
                }
                i = data_end;
                continue;
            }
        }
        i += 1;
    }

    // 2. Scan for PNG tEXt chunks with "IPTC:" prefix
    let mut p = 0;
    while p + 12 < bytes.len() {
        if &bytes[p..p + 5] == b"IPTC:" {
            let rest = &bytes[p + 5..];
            if let Some(null_idx) = rest.iter().position(|&b| b == 0) {
                let key_name = String::from_utf8_lossy(&rest[..null_idx]).to_string();
                let val_slice = &rest[null_idx + 1..];
                let val_end = val_slice
                    .iter()
                    .position(|&b| b == 0 || b < 0x20 && b != b'\t' && b != b'\n')
                    .unwrap_or(val_slice.len().min(256));
                let val_str = String::from_utf8_lossy(&val_slice[..val_end]).trim().to_string();
                if !key_name.is_empty() && !val_str.is_empty() {
                    let full_key = format!("Application2.{}", key_name);
                    raw_tags.insert(format!("IPTC.{}", full_key), val_str.clone());
                    iptc_obj.insert(full_key, serde_json::Value::String(val_str));
                }
            }
        }
        p += 1;
    }
}

fn parse_iptc_iim_block(
    block: &[u8],
    raw_tags: &mut HashMap<String, String>,
    iptc_obj: &mut serde_json::Map<String, serde_json::Value>,
) {
    let mut idx = 0;
    while idx + 5 <= block.len() {
        // IPTC tag marker is 0x1C
        if block[idx] != 0x1C {
            idx += 1;
            continue;
        }
        let record = block[idx + 1];
        let dataset = block[idx + 2];
        let len = u16::from_be_bytes([block[idx + 3], block[idx + 4]]) as usize;
        let val_start = idx + 5;
        if val_start + len > block.len() {
            break;
        }
        let val_bytes = &block[val_start..val_start + len];

        if record == 2 {
            let field_name = match dataset {
                0 => "RecordVersion",
                5 => "ObjectName",
                7 => "EditStatus",
                10 => "Urgency",
                12 => "Subject",
                15 => "Category",
                20 => "SuppCategory",
                25 => "Keywords",
                40 => "SpecialInstructions",
                55 => "DateCreated",
                60 => "TimeCreated",
                62 => "DigitalCreationDate",
                63 => "DigitalCreationTime",
                80 => "Byline",
                85 => "BylineTitle",
                90 => "City",
                92 => "SubLocation",
                95 => "ProvinceState",
                100 => "CountryCode",
                101 => "CountryName",
                103 => "OriginalTransmissionRef",
                105 => "Headline",
                110 => "Credit",
                115 => "Source",
                116 => "Copyright",
                118 => "Contact",
                120 => "Caption",
                122 => "WriterEditor",
                _ => "",
            };

            let val_str = if dataset == 0 && len == 2 {
                u16::from_be_bytes([val_bytes[0], val_bytes[1]]).to_string()
            } else {
                String::from_utf8_lossy(val_bytes).trim().to_string()
            };

            let key = if field_name.is_empty() {
                format!("Application2.Dataset{}", dataset)
            } else {
                format!("Application2.{}", field_name)
            };

            if !val_str.is_empty() {
                raw_tags.insert(format!("IPTC.{}", key), val_str.clone());
                iptc_obj.insert(key, serde_json::Value::String(val_str));
            }
        }

        idx = val_start + len;
    }
}

/// Extracts all XMP RDF attributes and element tags from embedded `<x:xmpmeta>` packets.
fn extract_xmp_tags(
    bytes: &[u8],
    raw_tags: &mut HashMap<String, String>,
    xmp_obj: &mut serde_json::Map<String, serde_json::Value>,
    max_len: usize,
) {
    let text = String::from_utf8_lossy(bytes);
    let mut search_from = 0;

    while let Some(rel_start) = text[search_from..].find("<x:xmpmeta") {
        let start = search_from + rel_start;
        let Some(rel_end) = text[start..].find("</x:xmpmeta>") else {
            break;
        };
        let end = start + rel_end + "</x:xmpmeta>".len();
        let xmp_block = &text[start..end];
        parse_single_xmp_packet(xmp_block, raw_tags, xmp_obj, max_len);
        search_from = end;
    }
}

fn parse_single_xmp_packet(
    xmp_block: &str,
    raw_tags: &mut HashMap<String, String>,
    xmp_obj: &mut serde_json::Map<String, serde_json::Value>,
    max_len: usize,
) {
    // 1. Parse prefixed attributes (e.g. photoshop:City="Berlin", hdrgm:Version="1.0")
    // inside any XML tag in the XMP packet
    let mut cursor = 0;
    while let Some(tag_open) = xmp_block[cursor..].find('<') {
        let abs_open = cursor + tag_open;
        let Some(tag_close) = xmp_block[abs_open..].find('>') else {
            break;
        };
        let abs_close = abs_open + tag_close;
        let tag_header = &xmp_block[abs_open + 1..abs_close];

        if !tag_header.starts_with('/') && !tag_header.starts_with('?') && !tag_header.starts_with('!') {
            parse_xml_attributes(tag_header, raw_tags, xmp_obj, max_len);
        }
        cursor = abs_close + 1;
    }

    // 2. Parse leaf and container elements with namespace prefix (e.g. <dc:creator>...</dc:creator>, <xmp:CreateDate>...</xmp:CreateDate>)
    let mut pos = 0;
    while let Some(open_lt) = xmp_block[pos..].find('<') {
        let abs_lt = pos + open_lt;
        let Some(open_gt) = xmp_block[abs_lt..].find('>') else {
            break;
        };
        let abs_gt = abs_lt + open_gt;
        let tag_inside = xmp_block[abs_lt + 1..abs_gt].trim();

        pos = abs_gt + 1;

        if tag_inside.starts_with('/')
            || tag_inside.starts_with('?')
            || tag_inside.starts_with('!')
            || tag_inside.ends_with('/')
        {
            continue;
        }

        let tag_name = tag_inside.split_whitespace().next().unwrap_or("");
        let Some((ns, prop)) = tag_name.split_once(':') else {
            continue;
        };

        if matches!(ns, "x" | "rdf" | "xmlns" | "xml" | "Container") {
            continue;
        }

        let close_tag = format!("</{}>", tag_name);
        if let Some(close_rel) = xmp_block[pos..].find(&close_tag) {
            let inner = &xmp_block[pos..pos + close_rel];
            let clean_val = extract_rdf_inner_text(inner);
            if !clean_val.is_empty() && clean_val.len() <= max_len {
                let norm_key = format!("{}.{}", normalize_xmp_ns(ns), prop);
                raw_tags.insert(format!("XMP.{}", norm_key), clean_val.clone());
                xmp_obj.insert(norm_key, serde_json::Value::String(clean_val));
            }
        }
    }
}

fn parse_xml_attributes(
    tag_header: &str,
    raw_tags: &mut HashMap<String, String>,
    xmp_obj: &mut serde_json::Map<String, serde_json::Value>,
    max_len: usize,
) {
    let mut rest = tag_header;
    // Skip element name
    if let Some(idx) = rest.find(char::is_whitespace) {
        rest = &rest[idx..];
    } else {
        return;
    }

    while let Some(eq_idx) = rest.find('=') {
        let attr_name = rest[..eq_idx].trim().trim_start_matches('/').trim();
        let after_eq = rest[eq_idx + 1..].trim_start();
        let Some(quote_char) = after_eq.chars().next() else {
            break;
        };
        if quote_char != '"' && quote_char != '\'' {
            break;
        }
        let val_rest = &after_eq[1..];
        let Some(end_quote) = val_rest.find(quote_char) else {
            break;
        };
        let attr_val = val_rest[..end_quote].trim();

        if let Some((ns, prop)) = attr_name.split_once(':') {
            if !matches!(ns, "xmlns" | "rdf" | "xml" | "x")
                && !attr_val.is_empty()
                && attr_val.len() <= max_len
            {
                let norm_key = format!("{}.{}", normalize_xmp_ns(ns), prop);
                raw_tags.insert(format!("XMP.{}", norm_key), attr_val.to_string());
                xmp_obj.insert(norm_key, serde_json::Value::String(attr_val.to_string()));
            }
        }

        rest = &val_rest[end_quote + 1..];
    }
}

fn normalize_xmp_ns(ns: &str) -> &str {
    match ns {
        "Iptc4xmpCore" => "iptc",
        other => other,
    }
}

fn extract_rdf_inner_text(inner: &str) -> String {
    // If inner contains <rdf:li...>, collect all <rdf:li> values
    if inner.contains("<rdf:li") {
        let mut items = Vec::new();
        let mut search = 0;
        while let Some(li_start) = inner[search..].find("<rdf:li") {
            let abs_li = search + li_start;
            let Some(gt) = inner[abs_li..].find('>') else {
                break;
            };
            let content_start = abs_li + gt + 1;
            if inner[abs_li..abs_li + gt].ends_with('/') {
                search = content_start;
                continue;
            }
            let Some(li_end) = inner[content_start..].find("</rdf:li>") else {
                break;
            };
            let item_text = inner[content_start..content_start + li_end].trim();
            if !item_text.is_empty() && !item_text.contains('<') {
                items.push(item_text);
            }
            search = content_start + li_end + "</rdf:li>".len();
        }
        return items.join(", ");
    }

    if !inner.contains('<') {
        return inner.trim().to_string();
    }

    String::new()
}

impl Plugin for MetadataPlugin {
    fn name(&self) -> &'static str {
        "metadata"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        ctx.capabilities.log("info", "Starting lossless metadata extraction");
        let settings = self.settings.get_or_parse(ctx);

        let file_bytes = match ctx.capabilities.fs_read(&doc.file.path) {
            Ok(bytes) => bytes,
            Err(e) => {
                ctx.capabilities.log(
                    "warn",
                    &format!("metadata: cannot read '{}': {}; metadata left empty", doc.file.path, e),
                );
                return Ok(doc);
            }
        };

        let (metadata_info, lat, lon, alt) = self.parse_image_metadata_with(&file_bytes, &settings);
        doc.metadata = Some(metadata_info);

        if lat.is_some() || lon.is_some() {
            let mut loc = doc.location.unwrap_or_default();
            loc.latitude = lat;
            loc.longitude = lon;
            loc.altitude = alt;
            doc.location = Some(loc);
        }

        Ok(doc)
    }
}
