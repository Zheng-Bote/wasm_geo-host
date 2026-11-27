//! Builds `plugins/reversegeo/data/places.bin` from GeoNames dumps (ADR 0006).
//!
//! Inputs (one directory): `cities1000.txt`, `admin1CodesASCII.txt`, `admin2Codes.txt`,
//! `countryInfo.txt`, `alternateNamesV2.txt` from <https://download.geonames.org/export/dump/>
//! (CC BY 4.0). The output layout is documented in `plugins/reversegeo/src/gazetteer.rs`; the
//! constants below must match the reader there (checked by the round-trip tests).
//!
//! The downloaded files are untrusted input: they are only parsed, every malformed line aborts
//! the build with file and line number, and no partial output is written.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub const MAGIC: &[u8; 4] = b"FGPL";
pub const FORMAT_VERSION: u16 = 1;
pub const HEADER_LEN: usize = 48;
pub const GRID_LAT: usize = 180;
pub const GRID_LON: usize = 360;
pub const NONE_U32: u32 = u32::MAX;
pub const NONE_U16: u16 = u16::MAX;
pub const COORD_SCALE: f64 = 100_000.0;
/// Longest accepted name in bytes.
pub const MAX_STRING_LEN: usize = 65_535;

/// Statistics of a build, reported on stderr.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stats {
    pub places: usize,
    pub skipped_pplx: usize,
    pub countries: usize,
    pub admin1: usize,
    pub admin2: usize,
    pub strings: usize,
    pub bytes: usize,
}

/// A name from `alternateNamesV2.txt`.
#[derive(Debug, Clone)]
struct AltName {
    id: u64,
    name: String,
    preferred: bool,
    short: bool,
}

struct Country {
    code: String,
    name: String,
    lang: Option<String>,
    geonameid: Option<u64>,
}

struct Admin {
    key: String,
    name: String,
    geonameid: Option<u64>,
}

struct City {
    geonameid: u64,
    name: String,
    lat: i32,
    lon: i32,
    country: String,
    admin1: String,
    admin2: String,
}

fn open(dir: &Path, file: &str) -> Result<BufReader<File>, String> {
    File::open(dir.join(file)).map(BufReader::new).map_err(|e| format!("{}: {}", file, e))
}

/// Calls `f(line_no, columns)` for each non-empty, non-comment line.
fn for_each_row(
    dir: &Path,
    file: &str,
    mut f: impl FnMut(usize, &[&str]) -> Result<(), String>,
) -> Result<(), String> {
    let reader = open(dir, file)?;
    for (i, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| format!("{}:{}: {}", file, i + 1, e))?;
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        f(i + 1, &cols).map_err(|e| format!("{}:{}: {}", file, i + 1, e))?;
    }
    Ok(())
}

fn col<'a>(cols: &[&'a str], i: usize) -> Result<&'a str, String> {
    cols.get(i).copied().ok_or_else(|| format!("missing column {}", i + 1))
}

fn parse_id(s: &str) -> Result<u64, String> {
    s.trim().parse::<u64>().map_err(|_| format!("invalid geonameid '{}'", s))
}

fn parse_coord(s: &str, limit: f64) -> Result<i32, String> {
    let v: f64 = s.trim().parse().map_err(|_| format!("invalid coordinate '{}'", s))?;
    if !v.is_finite() || v.abs() > limit {
        return Err(format!("coordinate out of range '{}'", s));
    }
    Ok((v * COORD_SCALE).round() as i32)
}

/// First language of a `countryInfo.txt` language list (`de-CH,fr-CH,…` → `de`).
fn first_language(list: &str) -> Option<String> {
    let first = list.split(',').next()?.trim();
    let lang = first.split('-').next()?.trim().to_ascii_lowercase();
    is_language_code(&lang).then_some(lang)
}

/// GeoNames uses pseudo languages (`link`, `post`, `iata`, `wkdt`, …) for other data.
fn is_language_code(s: &str) -> bool {
    (2..=3).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase())
}

fn read_countries(dir: &Path) -> Result<Vec<Country>, String> {
    let mut out = Vec::new();
    for_each_row(dir, "countryInfo.txt", |_, c| {
        let code = col(c, 0)?.trim();
        if code.len() != 2 || !code.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(format!("invalid ISO code '{}'", code));
        }
        let gid = col(c, 16)?.trim();
        out.push(Country {
            code: code.to_string(),
            name: col(c, 4)?.trim().to_string(),
            lang: first_language(col(c, 15)?),
            geonameid: if gid.is_empty() { None } else { Some(parse_id(gid)?) },
        });
        Ok(())
    })?;
    out.sort_by(|a, b| a.code.cmp(&b.code));
    out.dedup_by(|a, b| a.code == b.code);
    Ok(out)
}

fn read_admin(dir: &Path, file: &str) -> Result<Vec<Admin>, String> {
    let mut out = Vec::new();
    for_each_row(dir, file, |_, c| {
        let gid = col(c, 3)?.trim();
        out.push(Admin {
            key: col(c, 0)?.trim().to_string(),
            name: col(c, 1)?.trim().to_string(),
            geonameid: if gid.is_empty() { None } else { Some(parse_id(gid)?) },
        });
        Ok(())
    })?;
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out.dedup_by(|a, b| a.key == b.key);
    Ok(out)
}

fn read_cities(dir: &Path) -> Result<(Vec<City>, usize), String> {
    let mut out = Vec::new();
    let mut skipped = 0;
    for_each_row(dir, "cities1000.txt", |_, c| {
        if col(c, 7)? == "PPLX" {
            skipped += 1;
            return Ok(());
        }
        out.push(City {
            geonameid: parse_id(col(c, 0)?)?,
            name: col(c, 1)?.trim().to_string(),
            lat: parse_coord(col(c, 4)?, 90.0)?,
            lon: parse_coord(col(c, 5)?, 180.0)?,
            country: col(c, 8)?.trim().to_string(),
            admin1: col(c, 10)?.trim().to_string(),
            admin2: col(c, 11)?.trim().to_string(),
        });
        Ok(())
    })?;
    Ok((out, skipped))
}

/// Alternate names of the wanted ids in the wanted languages, without historic and
/// colloquial names; keyed by (geonameid, language).
fn read_alternate_names(
    dir: &Path,
    ids: &HashSet<u64>,
    langs: &HashSet<String>,
) -> Result<HashMap<(u64, String), Vec<AltName>>, String> {
    let mut out: HashMap<(u64, String), Vec<AltName>> = HashMap::new();
    for_each_row(dir, "alternateNamesV2.txt", |_, c| {
        let gid = parse_id(col(c, 1)?)?;
        if !ids.contains(&gid) {
            return Ok(());
        }
        let lang = col(c, 2)?;
        if !is_language_code(lang) || !langs.contains(lang) {
            return Ok(());
        }
        let flag = |i: usize| c.get(i).map(|v| *v == "1").unwrap_or(false);
        if flag(6) || flag(7) {
            return Ok(()); // colloquial or historic
        }
        let name = col(c, 3)?.trim();
        if name.is_empty() {
            return Ok(());
        }
        out.entry((gid, lang.to_string())).or_default().push(AltName {
            id: parse_id(col(c, 0)?)?,
            name: name.to_string(),
            preferred: flag(4),
            short: flag(5),
        });
        Ok(())
    })?;
    Ok(out)
}

struct Names(HashMap<(u64, String), Vec<AltName>>);

impl Names {
    /// Places and divisions: preferred first, then not short, then the lowest id.
    fn place(&self, gid: Option<u64>, lang: Option<&str>) -> Option<&str> {
        let list = self.0.get(&(gid?, lang?.to_string()))?;
        list.iter().min_by_key(|a| (!a.preferred, a.short, a.id)).map(|a| a.name.as_str())
    }

    /// Countries: short form first, then preferred, then the lowest id.
    fn country(&self, gid: Option<u64>, lang: Option<&str>) -> Option<&str> {
        let list = self.0.get(&(gid?, lang?.to_string()))?;
        list.iter().min_by_key(|a| (!a.short, !a.preferred, a.id)).map(|a| a.name.as_str())
    }
}

/// Deduplicating string table in order of first insertion.
#[derive(Default)]
struct Strings {
    bytes: Vec<u8>,
    index: HashMap<String, u32>,
}

impl Strings {
    fn add(&mut self, s: &str) -> Result<u32, String> {
        if let Some(&r) = self.index.get(s) {
            return Ok(r);
        }
        if s.len() > MAX_STRING_LEN {
            return Err(format!("name longer than {} bytes: '{}…'", MAX_STRING_LEN, s.chars().take(40).collect::<String>()));
        }
        let r = u32::try_from(self.bytes.len()).map_err(|_| "string table exceeds 4 GiB".to_string())?;
        let mut len = s.len();
        loop {
            let byte = (len & 0x7f) as u8;
            len >>= 7;
            if len == 0 {
                self.bytes.push(byte);
                break;
            }
            self.bytes.push(byte | 0x80);
        }
        self.bytes.extend_from_slice(s.as_bytes());
        self.index.insert(s.to_string(), r);
        Ok(r)
    }

    fn opt(&mut self, s: Option<&str>) -> Result<u32, String> {
        match s {
            Some(s) if !s.is_empty() => self.add(s),
            _ => Ok(NONE_U32),
        }
    }
}

fn cell_of(lat: i32, lon: i32) -> usize {
    let lat = lat as f64 / COORD_SCALE;
    let lon = lon as f64 / COORD_SCALE;
    let row = ((lat + 90.0).floor() as i64).clamp(0, GRID_LAT as i64 - 1) as usize;
    let col = ((lon + 180.0).floor() as i64).clamp(0, GRID_LON as i64 - 1) as usize;
    row * GRID_LON + col
}

/// Builds the gazetteer from the dumps in `dir`; `date` (YYYY-MM-DD) is the dump date.
pub fn build(dir: &Path, date: &str) -> Result<(Vec<u8>, Stats), String> {
    let valid_date = date.len() == 10
        && date.bytes().enumerate().all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() });
    if !valid_date {
        return Err(format!("invalid date '{}', expected YYYY-MM-DD", date));
    }
    let countries = read_countries(dir)?;
    let admin1 = read_admin(dir, "admin1CodesASCII.txt")?;
    let admin2 = read_admin(dir, "admin2Codes.txt")?;
    let (mut cities, skipped_pplx) = read_cities(dir)?;
    if admin1.len() >= NONE_U16 as usize || countries.len() >= NONE_U16 as usize {
        return Err("too many countries or admin1 divisions for a u16 index".to_string());
    }

    let country_index: HashMap<&str, usize> = countries.iter().enumerate().map(|(i, c)| (c.code.as_str(), i)).collect();
    let admin1_index: HashMap<&str, usize> = admin1.iter().enumerate().map(|(i, a)| (a.key.as_str(), i)).collect();
    let admin2_index: HashMap<&str, usize> = admin2.iter().enumerate().map(|(i, a)| (a.key.as_str(), i)).collect();
    let lang_of = |code: &str| country_index.get(code).and_then(|&i| countries[i].lang.as_deref());

    let mut ids: HashSet<u64> = cities.iter().map(|c| c.geonameid).collect();
    ids.extend(countries.iter().filter_map(|c| c.geonameid));
    ids.extend(admin1.iter().filter_map(|a| a.geonameid));
    ids.extend(admin2.iter().filter_map(|a| a.geonameid));
    let mut langs: HashSet<String> = countries.iter().filter_map(|c| c.lang.clone()).collect();
    langs.insert("en".to_string());
    let names = Names(read_alternate_names(dir, &ids, &langs)?);

    cities.sort_by_key(|c| (cell_of(c.lat, c.lon), c.geonameid));

    let mut strings = Strings::default();
    let meta = strings.add(&format!(
        "GeoNames cities1000 (https://www.geonames.org/, CC BY 4.0), dumps of {}; PPLX excluded; format {}",
        date, FORMAT_VERSION
    ))?;

    let mut out = vec![0u8; HEADER_LEN];
    for c in &countries {
        out.extend_from_slice(c.code.as_bytes());
        out.extend_from_slice(&strings.opt(Some(&c.name))?.to_le_bytes());
        out.extend_from_slice(&strings.opt(names.country(c.geonameid, c.lang.as_deref()))?.to_le_bytes());
        out.extend_from_slice(&strings.opt(c.lang.as_deref())?.to_le_bytes());
    }
    for a in &admin1 {
        let lang = a.key.split('.').next().and_then(lang_of);
        let en = names.place(a.geonameid, Some("en")).unwrap_or(&a.name);
        let local = names.place(a.geonameid, lang).unwrap_or(&a.name);
        out.extend_from_slice(&strings.opt(Some(en))?.to_le_bytes());
        out.extend_from_slice(&strings.opt(Some(local))?.to_le_bytes());
    }
    for a in &admin2 {
        let en = names.place(a.geonameid, Some("en")).unwrap_or(&a.name);
        out.extend_from_slice(&strings.opt(Some(en))?.to_le_bytes());
    }
    let mut counts = vec![0u32; GRID_LAT * GRID_LON];
    for c in &cities {
        counts[cell_of(c.lat, c.lon)] += 1;
    }
    let mut start = 0u32;
    for n in &counts {
        out.extend_from_slice(&start.to_le_bytes());
        start += n;
    }
    out.extend_from_slice(&start.to_le_bytes());
    for c in &cities {
        let lang = lang_of(&c.country);
        let en = names.place(Some(c.geonameid), Some("en")).unwrap_or(&c.name);
        let local = names.place(Some(c.geonameid), lang).unwrap_or(&c.name);
        let country = country_index.get(c.country.as_str()).map(|&i| i as u16).unwrap_or(NONE_U16);
        let a1 = admin1_index
            .get(format!("{}.{}", c.country, c.admin1).as_str())
            .map(|&i| i as u16)
            .unwrap_or(NONE_U16);
        let a2 = if c.admin2.is_empty() {
            NONE_U32
        } else {
            admin2_index
                .get(format!("{}.{}.{}", c.country, c.admin1, c.admin2).as_str())
                .map(|&i| i as u32)
                .unwrap_or(NONE_U32)
        };
        out.extend_from_slice(&c.lat.to_le_bytes());
        out.extend_from_slice(&c.lon.to_le_bytes());
        out.extend_from_slice(&strings.opt(Some(en))?.to_le_bytes());
        out.extend_from_slice(&strings.opt(Some(local))?.to_le_bytes());
        out.extend_from_slice(&country.to_le_bytes());
        out.extend_from_slice(&a1.to_le_bytes());
        out.extend_from_slice(&a2.to_le_bytes());
    }
    let strings_len = u32::try_from(strings.bytes.len()).map_err(|_| "string table too large".to_string())?;
    out.extend_from_slice(&strings.bytes);

    let header = &mut out[..HEADER_LEN];
    header[0..4].copy_from_slice(MAGIC);
    header[4..6].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
    let u32s = [cities.len(), countries.len(), admin1.len(), admin2.len()];
    for (i, v) in u32s.iter().enumerate() {
        let v = u32::try_from(*v).map_err(|_| "count exceeds u32".to_string())?;
        header[8 + i * 4..12 + i * 4].copy_from_slice(&v.to_le_bytes());
    }
    header[24..28].copy_from_slice(&strings_len.to_le_bytes());
    header[28..32].copy_from_slice(&meta.to_le_bytes());
    header[32..34].copy_from_slice(&(GRID_LAT as u16).to_le_bytes());
    header[34..36].copy_from_slice(&(GRID_LON as u16).to_le_bytes());

    let stats = Stats {
        places: cities.len(),
        skipped_pplx,
        countries: countries.len(),
        admin1: admin1.len(),
        admin2: admin2.len(),
        strings: strings.index.len(),
        bytes: out.len(),
    };
    Ok((out, stats))
}

/// Builds and writes the gazetteer atomically (`<output>.tmp`, then rename).
pub fn write(dir: &Path, output: &Path, date: &str) -> Result<Stats, String> {
    let (bytes, stats) = build(dir, date)?;
    let tmp = output.with_extension("bin.tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| format!("{}: {}", tmp.display(), e))?;
    std::fs::rename(&tmp, output).map_err(|e| format!("{}: {}", output.display(), e))?;
    Ok(stats)
}
