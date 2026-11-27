mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Repo `config.yaml` with the database redirected into `dir` and `plugin_dir` pointing at the
/// built components (the process runs in `dir`).
fn write_config(dir: &Path) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let original = std::fs::read_to_string(root.join("config.yaml")).unwrap();
    let db = dir.join("media.db");
    let config: String = original
        .lines()
        .map(|l| {
            if l.starts_with("database_path:") {
                format!("database_path: \"{}\"", db.display())
            } else if l.starts_with("plugin_dir:") {
                format!("plugin_dir: \"{}\"", common::components_dir().display())
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        // These tests need a deterministic plugin warning: without face-detection, privacy-redact
        // reports the missing face result.
        .replace(
            "  face-detection:\n  # built-in YuNet detector (ADR 0009); privacy-redact uses its boxes\n    enabled: true",
            "  face-detection:\n    enabled: false",
        );
    assert!(config.contains("  face-detection:\n    enabled: false"), "config.yaml layout changed");
    let path = dir.join("config.yaml");
    std::fs::write(&path, config).unwrap();
    path
}

fn run_process(dir: &Path, file: &str, rust_log: Option<&str>) -> Output {
    let config = write_config(dir);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_flagme-host"));
    cmd.args(["process", file, "--config", config.to_str().unwrap()]).current_dir(dir);
    cmd.env_remove("RUST_LOG");
    if let Some(level) = rust_log {
        cmd.env("RUST_LOG", level);
    }
    cmd.output().expect("flagme-host runs")
}

// wasm-geo-host AC-10: plugin warnings appear on stderr with the plugin name; stdout holds only the document JSON
#[test]
fn test_ac10_plugin_warnings_on_stderr() {
    let dir = common::temp_dir("log_ac10a");
    let file = common::write_file(&dir, "photo.jpg", &common::plain_jpeg(32, 24));

    let out = run_process(&dir, &file, None);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stderr = String::from_utf8_lossy(&out.stderr);
    let line = stderr
        .lines()
        .find(|l| l.contains("privacy-redact: no face detection result"))
        .unwrap_or_else(|| panic!("warning missing on stderr:\n{}", stderr));
    assert!(line.contains("WARN") && line.contains("plugin=privacy-redact"), "{}", line);

    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout is pure JSON");
    assert_eq!(doc["file"]["path"], file.as_str());
    let _ = std::fs::remove_dir_all(&dir);
}

// wasm-geo-host AC-10: an unreadable file is reported on stderr as "metadata: cannot read"
#[test]
fn test_ac10_unreadable_file_reported() {
    let dir = common::temp_dir("log_ac10b");
    let file = common::write_file(&dir, "locked.jpg", &common::plain_jpeg(16, 16));
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();

    let out = run_process(&dir, &file, None);
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(&format!("metadata: cannot read '{}'", file)),
        "stderr:\n{}",
        stderr
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// wasm-geo-host AC-10: RUST_LOG controls the level (default warn; error hides warnings, info adds pipeline progress)
#[test]
fn test_ac10_rust_log_controls_level() {
    let dir = common::temp_dir("log_ac10c");
    let file = common::write_file(&dir, "photo.jpg", &common::plain_jpeg(32, 24));

    let default = String::from_utf8_lossy(&run_process(&dir, &file, None).stderr).to_string();
    assert!(default.contains("WARN") && !default.contains(" INFO "), "{}", default);

    let quiet = run_process(&dir, &file, Some("error"));
    assert!(quiet.stderr.is_empty(), "{}", String::from_utf8_lossy(&quiet.stderr));
    serde_json::from_slice::<serde_json::Value>(&quiet.stdout).expect("stdout is pure JSON");

    let verbose = String::from_utf8_lossy(&run_process(&dir, &file, Some("info")).stderr).to_string();
    assert!(verbose.contains("INFO") && verbose.contains("Entering pipeline stage"), "{}", verbose);
    let _ = std::fs::remove_dir_all(&dir);
}
