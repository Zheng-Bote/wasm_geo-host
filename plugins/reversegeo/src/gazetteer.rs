//! Reader for the embedded GeoNames gazetteer `data/places.bin` (ADR 0006).
//!
//! The file is written by `tools/geonames-import` (`make geonames`). It is read in place: no
//! index is built at run time and no `unsafe` is used, so every call of the plugin (a fresh
//! component instance per document, ADR 0003) only checks the header and then reads the few
//! grid cells around a point. Malformed data yields [`FormatError`] or `None`, never a panic.
//!
//! Layout (format version 1, little endian, `NONE` = `u32::MAX` / `u16::MAX`):
//!
//! | Section | Content |
//! | :-- | :-- |
//! | header (48 bytes) | `"FGPL"`, version u16, reserved u16, n_places, n_countries, n_admin1, n_admin2, strings_len, meta (u32 each), grid_lat u16, grid_lon u16, 8 reserved bytes |
//! | countries | `code [u8; 2]`, `name`, `local`, `lang` (u32 string refs) — 14 bytes each |
//! | admin1 | `name`, `local` — 8 bytes each |
//! | admin2 | `name` — 4 bytes each |
//! | cells | `grid_lat * grid_lon + 1` × u32: first place index of each cell |
//! | places | `lat i32`, `lon i32` (1e-5°), `name u32`, `local u32`, `country u16`, `admin1 u16`, `admin2 u32` — 24 bytes each |
//! | strings | LEB128 length + UTF-8; a string ref is a byte offset into this block |

/// The embedded gazetteer.
pub static PLACES: &[u8] = include_bytes!("../data/places.bin");

/// Magic bytes at the start of the file.
pub const MAGIC: &[u8; 4] = b"FGPL";
/// Format version this reader understands.
pub const FORMAT_VERSION: u16 = 1;
/// Size of the header in bytes.
pub const HEADER_LEN: usize = 48;
/// Grid resolution: one cell per degree.
pub const GRID_LAT: usize = 180;
pub const GRID_LON: usize = 360;
/// Missing reference (string, admin2).
pub const NONE_U32: u32 = u32::MAX;
/// Missing reference (country, admin1).
pub const NONE_U16: u16 = u16::MAX;
/// Bytes per record.
pub const COUNTRY_LEN: usize = 14;
pub const ADMIN1_LEN: usize = 8;
pub const ADMIN2_LEN: usize = 4;
pub const PLACE_LEN: usize = 24;
/// Coordinates are stored in units of 1e-5 degree.
pub const COORD_SCALE: f64 = 100_000.0;

const EARTH_RADIUS_KM: f64 = 6371.0;

/// Why `places.bin` cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatError {
    TooShort,
    BadMagic,
    UnsupportedVersion(u16),
    BadGrid(u16, u16),
    BadLength { expected: usize, actual: usize },
    BadCells,
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatError::TooShort => write!(f, "file shorter than the header"),
            FormatError::BadMagic => write!(f, "not a gazetteer file (magic)"),
            FormatError::UnsupportedVersion(v) => {
                write!(f, "unsupported format version {} (expected {})", v, FORMAT_VERSION)
            }
            FormatError::BadGrid(a, b) => write!(f, "unsupported grid {}x{}", a, b),
            FormatError::BadLength { expected, actual } => {
                write!(f, "length {} does not match the header ({})", actual, expected)
            }
            FormatError::BadCells => write!(f, "cell index is not monotonic"),
        }
    }
}

fn u16_at(b: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(off..off + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(off..off + 4)?.try_into().ok()?))
}

fn i32_at(b: &[u8], off: usize) -> Option<i32> {
    Some(i32::from_le_bytes(b.get(off..off + 4)?.try_into().ok()?))
}

/// Grid cell of a coordinate (clamped to the grid).
pub fn cell_of(lat: f64, lon: f64) -> usize {
    let row = ((lat + 90.0).floor() as i64).clamp(0, GRID_LAT as i64 - 1) as usize;
    let col = ((lon + 180.0).floor() as i64).clamp(0, GRID_LON as i64 - 1) as usize;
    row * GRID_LON + col
}

/// Great-circle distance in kilometres (haversine).
pub fn distance_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = p2 - p1;
    let dl = (lon2 - lon1).to_radians();
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * h.sqrt().min(1.0).asin()
}

/// A parsed, borrowed view of `places.bin`.
#[derive(Debug, Clone, Copy)]
pub struct Gazetteer<'a> {
    countries: &'a [u8],
    admin1: &'a [u8],
    admin2: &'a [u8],
    cells: &'a [u8],
    places: &'a [u8],
    strings: &'a [u8],
    n_places: usize,
    meta: u32,
}

/// One place of the gazetteer.
#[derive(Debug, Clone, Copy)]
pub struct Place<'a> {
    g: Gazetteer<'a>,
    index: usize,
}

impl<'a> Gazetteer<'a> {
    /// Checks the header and the section lengths; O(cells), no copying.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, FormatError> {
        if bytes.len() < HEADER_LEN {
            return Err(FormatError::TooShort);
        }
        if &bytes[0..4] != MAGIC {
            return Err(FormatError::BadMagic);
        }
        let version = u16_at(bytes, 4).ok_or(FormatError::TooShort)?;
        if version != FORMAT_VERSION {
            return Err(FormatError::UnsupportedVersion(version));
        }
        let field = |off| u32_at(bytes, off).map(|v| v as usize).ok_or(FormatError::TooShort);
        let n_places = field(8)?;
        let n_countries = field(12)?;
        let n_admin1 = field(16)?;
        let n_admin2 = field(20)?;
        let strings_len = field(24)?;
        let meta = u32_at(bytes, 28).ok_or(FormatError::TooShort)?;
        let (grid_lat, grid_lon) = (u16_at(bytes, 32).unwrap_or(0), u16_at(bytes, 34).unwrap_or(0));
        if grid_lat as usize != GRID_LAT || grid_lon as usize != GRID_LON {
            return Err(FormatError::BadGrid(grid_lat, grid_lon));
        }
        let n_cells = GRID_LAT * GRID_LON + 1;
        let sizes = [
            n_countries.checked_mul(COUNTRY_LEN),
            n_admin1.checked_mul(ADMIN1_LEN),
            n_admin2.checked_mul(ADMIN2_LEN),
            Some(n_cells * 4),
            n_places.checked_mul(PLACE_LEN),
            Some(strings_len),
        ];
        let mut offsets = [0usize; 7];
        offsets[0] = HEADER_LEN;
        for (i, size) in sizes.iter().enumerate() {
            let size = size.ok_or(FormatError::TooShort)?;
            offsets[i + 1] = offsets[i].checked_add(size).ok_or(FormatError::TooShort)?;
        }
        if offsets[6] != bytes.len() {
            return Err(FormatError::BadLength { expected: offsets[6], actual: bytes.len() });
        }
        let section = |i: usize| &bytes[offsets[i]..offsets[i + 1]];
        let g = Gazetteer {
            countries: section(0),
            admin1: section(1),
            admin2: section(2),
            cells: section(3),
            places: section(4),
            strings: section(5),
            n_places,
            meta,
        };
        let mut prev = 0u32;
        for c in 0..n_cells {
            let v = u32_at(g.cells, c * 4).ok_or(FormatError::BadCells)?;
            if v < prev || v as usize > n_places {
                return Err(FormatError::BadCells);
            }
            prev = v;
        }
        if prev as usize != n_places {
            return Err(FormatError::BadCells);
        }
        Ok(g)
    }

    /// Number of places.
    pub fn len(&self) -> usize {
        self.n_places
    }

    /// `true` if the gazetteer holds no place.
    pub fn is_empty(&self) -> bool {
        self.n_places == 0
    }

    /// Origin of the data (source, licence, dump date).
    pub fn meta(&self) -> Option<&'a str> {
        self.string(self.meta)
    }

    /// A string by reference, `None` for `NONE_U32` or a broken reference.
    pub fn string(&self, r: u32) -> Option<&'a str> {
        if r == NONE_U32 {
            return None;
        }
        let mut pos = r as usize;
        let (mut len, mut shift) = (0usize, 0u32);
        loop {
            let byte = *self.strings.get(pos)?;
            pos += 1;
            len |= ((byte & 0x7f) as usize).checked_shl(shift)?;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 28 {
                return None;
            }
        }
        std::str::from_utf8(self.strings.get(pos..pos.checked_add(len)?)?).ok()
    }

    /// Place by index.
    pub fn place(&self, index: usize) -> Option<Place<'a>> {
        (index < self.n_places).then_some(Place { g: *self, index })
    }

    fn cell_range(&self, cell: usize) -> (usize, usize) {
        let start = u32_at(self.cells, cell * 4).unwrap_or(0) as usize;
        let end = u32_at(self.cells, (cell + 1) * 4).unwrap_or(0) as usize;
        (start.min(self.n_places), end.min(self.n_places).max(start.min(self.n_places)))
    }

    /// Nearest place within `max_km` (great-circle distance); ties go to the lower index.
    pub fn nearest(&self, lat: f64, lon: f64, max_km: f64) -> Option<(Place<'a>, f64)> {
        if !lat.is_finite() || !lon.is_finite() || !(-90.0..=90.0).contains(&lat) || max_km <= 0.0 {
            return None;
        }
        let lon = ((lon + 180.0).rem_euclid(360.0)) - 180.0;
        let dlat = max_km / 111.0;
        let lat_min = (lat - dlat).max(-90.0);
        let lat_max = (lat + dlat).min(90.0);
        let cos_min = lat_min.abs().max(lat_max.abs()).to_radians().cos();
        let dlon = if cos_min <= 1e-6 { 360.0 } else { (max_km / (111.32 * cos_min)).min(360.0) };
        let row_min = ((lat_min + 90.0).floor() as i64).clamp(0, GRID_LAT as i64 - 1);
        let row_max = ((lat_max + 90.0).floor() as i64).clamp(0, GRID_LAT as i64 - 1);
        let (col_min, col_max) = if dlon >= 180.0 {
            (0i64, GRID_LON as i64 - 1)
        } else {
            (((lon - dlon) + 180.0).floor() as i64, ((lon + dlon) + 180.0).floor() as i64)
        };

        let mut best: Option<(usize, f64)> = None;
        let mut seen_cols = std::collections::BTreeSet::new();
        for col in col_min..=col_max {
            seen_cols.insert(col.rem_euclid(GRID_LON as i64) as usize);
        }
        for row in row_min..=row_max {
            for &col in &seen_cols {
                let (start, end) = self.cell_range(row as usize * GRID_LON + col);
                for i in start..end {
                    let Some((plat, plon)) = self.coords(i) else { continue };
                    let d = distance_km(lat, lon, plat, plon);
                    if d <= max_km && best.map(|(bi, bd)| d < bd || (d == bd && i < bi)).unwrap_or(true) {
                        best = Some((i, d));
                    }
                }
            }
        }
        best.and_then(|(i, d)| self.place(i).map(|p| (p, d)))
    }

    fn coords(&self, index: usize) -> Option<(f64, f64)> {
        let off = index * PLACE_LEN;
        Some((i32_at(self.places, off)? as f64 / COORD_SCALE, i32_at(self.places, off + 4)? as f64 / COORD_SCALE))
    }

    fn country_field(&self, country: u16, field: usize) -> Option<u32> {
        if country == NONE_U16 {
            return None;
        }
        u32_at(self.countries, country as usize * COUNTRY_LEN + 2 + field * 4)
    }
}

impl<'a> Place<'a> {
    fn off(&self) -> usize {
        self.index * PLACE_LEN
    }

    /// Latitude and longitude in degrees.
    pub fn coords(&self) -> Option<(f64, f64)> {
        self.g.coords(self.index)
    }

    fn country(&self) -> u16 {
        u16_at(self.g.places, self.off() + 16).unwrap_or(NONE_U16)
    }

    fn admin1(&self) -> u16 {
        u16_at(self.g.places, self.off() + 18).unwrap_or(NONE_U16)
    }

    fn admin2(&self) -> u32 {
        u32_at(self.g.places, self.off() + 20).unwrap_or(NONE_U32)
    }

    /// English (or GeoNames main) name of the place.
    pub fn city(&self) -> Option<&'a str> {
        self.g.string(u32_at(self.g.places, self.off() + 8)?)
    }

    /// Name in the country's first language, else the GeoNames main name.
    pub fn local_city(&self) -> Option<&'a str> {
        self.g.string(u32_at(self.g.places, self.off() + 12)?)
    }

    /// English (or main) name of the second-level division.
    pub fn county(&self) -> Option<&'a str> {
        let a2 = self.admin2();
        if a2 == NONE_U32 {
            return None;
        }
        self.g.string(u32_at(self.g.admin2, a2 as usize * ADMIN2_LEN)?)
    }

    /// English (or main) name of the first-level division.
    pub fn region(&self) -> Option<&'a str> {
        let a1 = self.admin1();
        if a1 == NONE_U16 {
            return None;
        }
        self.g.string(u32_at(self.g.admin1, a1 as usize * ADMIN1_LEN)?)
    }

    /// Local name of the first-level division.
    pub fn local_region(&self) -> Option<&'a str> {
        let a1 = self.admin1();
        if a1 == NONE_U16 {
            return None;
        }
        self.g.string(u32_at(self.g.admin1, a1 as usize * ADMIN1_LEN + 4)?)
    }

    /// ISO 3166-1 alpha-2 code.
    pub fn country_code(&self) -> Option<&'a str> {
        let c = self.country();
        if c == NONE_U16 {
            return None;
        }
        let off = c as usize * COUNTRY_LEN;
        std::str::from_utf8(self.g.countries.get(off..off + 2)?).ok()
    }

    /// English country name.
    pub fn country_name(&self) -> Option<&'a str> {
        self.g.string(self.g.country_field(self.country(), 0)?)
    }

    /// Country name in its first language (short form preferred); `None` if GeoNames has none.
    pub fn local_country(&self) -> Option<&'a str> {
        self.g.string(self.g.country_field(self.country(), 1)?)
    }

    /// First language of the country (ISO 639).
    pub fn local_language(&self) -> Option<&'a str> {
        self.g.string(self.g.country_field(self.country(), 2)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // geonames-gazetteer AC-8: the reader rejects a file with an unknown format version, a wrong magic or a wrong length
    #[test]
    fn test_ac8_rejects_unknown_version() {
        assert!(Gazetteer::parse(PLACES).is_ok(), "embedded data must parse");
        let mut v2 = PLACES.to_vec();
        v2[4..6].copy_from_slice(&2u16.to_le_bytes());
        assert_eq!(Gazetteer::parse(&v2).err(), Some(FormatError::UnsupportedVersion(2)));
        let mut magic = PLACES.to_vec();
        magic[0] = b'X';
        assert_eq!(Gazetteer::parse(&magic).err(), Some(FormatError::BadMagic));
        let short = &PLACES[..PLACES.len() - 1];
        assert!(matches!(Gazetteer::parse(short), Err(FormatError::BadLength { .. })));
        assert_eq!(Gazetteer::parse(&PLACES[..10]).err(), Some(FormatError::TooShort));
    }

    #[test]
    fn distance_is_great_circle() {
        let d = distance_km(48.1374, 11.5755, 52.5200, 13.4050);
        assert!((d - 504.0).abs() < 5.0, "{}", d);
        assert_eq!(cell_of(48.4, 11.7), (138 * 360) + 191);
    }
}
