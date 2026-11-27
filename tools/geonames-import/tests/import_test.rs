use flagme_plugin_reversegeo::gazetteer::{Gazetteer, FORMAT_VERSION};
use std::path::{Path, PathBuf};

/// Minimal GeoNames dumps: DE (de) and CH (de, fr) with Munich, Moosburg, Geneva and a PPLX.
fn fixture(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("geonames_import_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let w = |name: &str, rows: &[&str]| std::fs::write(dir.join(name), rows.join("\n") + "\n").unwrap();
    // countryInfo: ISO ISO3 num fips Country Capital Area Pop Continent tld CurCode CurName Phone PostFmt PostRe Languages geonameid neighbours fips
    w(
        "countryInfo.txt",
        &[
            "#ISO\tISO3\tISO-Numeric\tfips\tCountry\tCapital\tArea\tPopulation\tContinent\ttld\tCurrencyCode\tCurrencyName\tPhone\tPostal Code Format\tPostal Code Regex\tLanguages\tgeonameid\tneighbours\tEquivalentFipsCode",
            "DE\tDEU\t276\tGM\tGermany\tBerlin\t357021\t82927922\tEU\t.de\tEUR\tEuro\t49\t#####\t^(\\d{5})$\tde\t100\tCH\t",
            "CH\tCHE\t756\tSZ\tSwitzerland\tBern\t41290\t8516543\tEU\t.ch\tCHF\tFranc\t41\t####\t^(\\d{4})$\tde-CH,fr-CH,it-CH,rm\t200\tDE\t",
        ],
    );
    w("admin1CodesASCII.txt", &["DE.02\tBavaria\tBavaria\t110", "CH.GE\tGeneva\tGeneva\t210"]);
    w("admin2Codes.txt", &["DE.02.091\tUpper Bavaria\tUpper Bavaria\t120"]);
    // cities: id name ascii alt lat lon class code cc cc2 a1 a2 a3 a4 pop elev dem tz mod
    let city = |id: u32, name: &str, lat: f64, lon: f64, code: &str, cc: &str, a1: &str, a2: &str| {
        format!("{id}\t{name}\t{name}\t\t{lat}\t{lon}\tP\t{code}\t{cc}\t\t{a1}\t{a2}\t\t\t1000\t\t500\tEurope/Berlin\t2025-01-01")
    };
    let cities = [
        city(1, "Munich", 48.13743, 11.57549, "PPLA", "DE", "02", "091"),
        city(2, "Moosburg", 48.47061, 11.93811, "PPL", "DE", "02", "091"),
        city(3, "Geneva", 46.20222, 6.14569, "PPLA", "CH", "GE", ""),
        city(4, "Schwabing", 48.16, 11.58, "PPLX", "DE", "02", "091"),
    ];
    w("cities1000.txt", &cities.iter().map(String::as_str).collect::<Vec<_>>());
    // alternateNamesV2: altId gid lang name pref short colloq hist from to
    w(
        "alternateNamesV2.txt",
        &[
            "1001\t1\tde\tMünchen\t1\t\t\t\t\t",
            "1002\t1\tde\tMinga\t\t\t1\t\t\t",
            "1003\t1\ten\tMunich\t1\t\t\t\t\t",
            "1004\t1\tlink\thttps://en.wikipedia.org/wiki/Munich\t\t\t\t\t\t",
            "1005\t1\tde\tMonachium\t1\t\t\t1\t\t",
            "3001\t3\tde\tGenf\t\t\t\t\t\t",
            "3002\t3\tfr\tGenève\t\t\t\t\t\t",
            "1101\t110\tde\tBayern\t1\t\t\t\t\t",
            "1102\t110\ten\tBavaria\t\t\t\t\t\t",
            "1001001\t100\tde\tBundesrepublik Deutschland\t1\t\t\t\t\t",
            "1001002\t100\tde\tDeutschland\t\t1\t\t\t\t",
            "2001\t200\tde\tSchweiz\t\t1\t\t\t\t",
        ],
    );
    dir
}

fn build(dir: &Path) -> Vec<u8> {
    geonames_import::build(dir, "2026-11-27").unwrap().0
}

// geonames-gazetteer AC-7: names follow ADR 0006 — preferred local name else the GeoNames main name, first country language in multilingual countries, local_country short form or empty, nothing taken from another place
#[test]
fn test_ac7_name_rules() {
    let dir = fixture("ac7");
    let bytes = build(&dir);
    let g = Gazetteer::parse(&bytes).unwrap();

    let (munich, _) = g.nearest(48.1374, 11.5755, 5.0).unwrap();
    assert_eq!(munich.city(), Some("Munich"));
    assert_eq!(munich.local_city(), Some("München"), "preferred, not colloquial/historic");
    assert_eq!((munich.region(), munich.local_region()), (Some("Bavaria"), Some("Bayern")));
    assert_eq!(munich.county(), Some("Upper Bavaria"));
    assert_eq!((munich.country_name(), munich.local_country()), (Some("Germany"), Some("Deutschland")));
    assert_eq!((munich.country_code(), munich.local_language()), (Some("DE"), Some("de")));

    // No alternate name at all: the GeoNames main name in both fields.
    let (moosburg, _) = g.nearest(48.4706, 11.9381, 5.0).unwrap();
    assert_eq!((moosburg.city(), moosburg.local_city()), (Some("Moosburg"), Some("Moosburg")));

    // Multilingual country: the first language (de) applies, documented limitation.
    let (geneva, _) = g.nearest(46.2044, 6.1432, 5.0).unwrap();
    assert_eq!((geneva.city(), geneva.local_city(), geneva.local_language()), (Some("Geneva"), Some("Genf"), Some("de")));
    assert_eq!((geneva.region(), geneva.local_region()), (Some("Geneva"), Some("Geneva")));
    assert_eq!(geneva.county(), None);
    assert_eq!(geneva.local_country(), Some("Schweiz"));
    let _ = std::fs::remove_dir_all(&dir);
}

// geonames-gazetteer AC-8: the same inputs give byte-identical output, PPLX is dropped, the header carries format version and dump date, broken input aborts with file and line
#[test]
fn test_ac8_deterministic_and_versioned() {
    let dir = fixture("ac8");
    let (a, stats) = geonames_import::build(&dir, "2026-11-27").unwrap();
    let b = build(&dir);
    assert_eq!(a, b, "output must be deterministic");
    assert_eq!((stats.places, stats.skipped_pplx), (3, 1));
    assert_eq!(u16::from_le_bytes([a[4], a[5]]), FORMAT_VERSION);
    let g = Gazetteer::parse(&a).unwrap();
    assert_eq!(g.len(), 3);
    let meta = g.meta().unwrap();
    assert!(meta.contains("GeoNames cities1000") && meta.contains("CC BY 4.0") && meta.contains("2026-11-27"), "{}", meta);
    // Schwabing (PPLX) is not a place: the nearest place to its point is Munich.
    assert_eq!(g.nearest(48.16, 11.58, 10.0).unwrap().0.city(), Some("Munich"));

    // Atomic write: the target appears only complete.
    let out = dir.join("places.bin");
    geonames_import::write(&dir, &out, "2026-11-27").unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), a);
    assert!(!dir.join("places.bin.tmp").exists());

    assert!(geonames_import::build(&dir, "27.11.2026").unwrap_err().contains("invalid date"));
    let mut cities = std::fs::read_to_string(dir.join("cities1000.txt")).unwrap();
    cities.push_str("5\tBroken\tBroken\t\tnorth\t11.0\tP\tPPL\tDE\t\t02\t\t\t\t0\t\t0\tEurope/Berlin\t2025-01-01\n");
    std::fs::write(dir.join("cities1000.txt"), cities).unwrap();
    let err = geonames_import::write(&dir, &out, "2026-11-27").unwrap_err();
    assert!(err.starts_with("cities1000.txt:5:") && err.contains("invalid coordinate"), "{}", err);
    assert_eq!(std::fs::read(&out).unwrap(), a, "a failed build leaves the previous file untouched");
    let _ = std::fs::remove_dir_all(&dir);
}
