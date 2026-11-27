//! `geonames-import --input <dir> --output <file> --date <YYYY-MM-DD>` (see `make geonames`).

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let (Some(input), Some(output), Some(date)) = (value("--input"), value("--output"), value("--date")) else {
        eprintln!("usage: geonames-import --input <dir> --output <places.bin> --date <YYYY-MM-DD>");
        return ExitCode::from(2);
    };
    match geonames_import::write(&PathBuf::from(&input), &PathBuf::from(&output), &date) {
        Ok(s) => {
            eprintln!(
                "geonames-import: {} places ({} PPLX skipped), {} countries, {} admin1, {} admin2, {} strings, {} bytes -> {}",
                s.places, s.skipped_pplx, s.countries, s.admin1, s.admin2, s.strings, s.bytes, output
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("geonames-import: {}", e);
            ExitCode::FAILURE
        }
    }
}
