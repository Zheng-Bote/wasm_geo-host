mod common;

use chrono::{TimeZone, Utc};
use flagme_host::{AppConfig, DirectoryWatcher, MediaOrganizer, Pipeline, PluginRuntime, Storage, WatcherConfig, process_single_file};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use wasm_geo_models::{FileInfo, LocationInfo, TimelineInfo, UnifiedDocument};

fn create_test_pipeline(config: AppConfig, storage: Arc<Storage>) -> Pipeline {
    let config = common::with_components(config);
    let runtime = PluginRuntime::new(config.clone()).unwrap();
    Pipeline::new(config, runtime, storage)
}

// directory-watcher AC-1: Non-blocking Directory Watcher monitors filesystem directories without busy polling
#[test]
fn test_directory_watcher_ac1_lifecycle() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = std::env::temp_dir().join(format!("test_watcher_ac1_{}", std::process::id()));
        fs::create_dir_all(&temp_dir).expect("Failed to create temp watch dir");

        let db_path = temp_dir.join("media.db");
        let storage = Arc::new(Storage::new(db_path.to_str().unwrap()));
        storage.init_schema().expect("Schema init failed");

        let config = AppConfig::default();
        let pipeline = Arc::new(create_test_pipeline(config, storage.clone()));

        let watcher_config = WatcherConfig {
            watch_dir: temp_dir.clone(),
            target_dir: None,
            organize_pattern: "{year}/{month}/{country}/{city}".to_string(),
            copy_only: false,
            workers: 2,
        };

        let watcher = Arc::new(DirectoryWatcher::new(watcher_config, pipeline, storage));
        assert!(watcher.is_running(), "Watcher should be initialized in running state");

        let watcher_clone = watcher.clone();
        let handle = tokio::spawn(async move {
            let _ = watcher_clone.run().await;
        });

        // Let watcher spin up
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Stop watcher gracefully
        watcher.stop();
        assert!(!watcher.is_running(), "Watcher should transition to stopped state");

        let _ = handle.await;
        let _ = fs::remove_dir_all(&temp_dir);
    });
}

// directory-watcher AC-2: Thread pool concurrently executes the configured plugin pipeline on detected media files
#[test]
fn test_directory_watcher_ac2_concurrent_worker_pipeline_execution() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = std::env::temp_dir().join(format!("test_watcher_ac2_{}", std::process::id()));
        fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

        let db_path = temp_dir.join("media.db");
        let storage = Arc::new(Storage::new(db_path.to_str().unwrap()));
        storage.init_schema().expect("Schema init failed");

        let config = AppConfig::default();
        let pipeline = Arc::new(create_test_pipeline(config, storage.clone()));

        let watcher_config = WatcherConfig {
            watch_dir: temp_dir.clone(),
            target_dir: None,
            organize_pattern: "{year}/{month}/{country}/{city}".to_string(),
            copy_only: false,
            workers: 4,
        };

        // Create 3 dummy media files
        let mut files = Vec::new();
        for i in 1..=3 {
            let file_path = temp_dir.join(format!("photo_{}.jpg", i));
            fs::write(&file_path, format!("fake_jpeg_content_{}", i)).unwrap();
            files.push(file_path);
        }

        // Process files concurrently through worker pool function
        let mut handles = Vec::new();
        for f in files {
            let p = pipeline.clone();
            let s = storage.clone();
            let c = watcher_config.clone();
            handles.push(tokio::spawn(async move {
                process_single_file(&f, &p, &s, &c).await
            }));
        }

        for h in handles {
            let res = h.await.unwrap().expect("Processing should succeed");
            assert!(res.is_some(), "Document should be produced and persisted");
        }

        let _ = fs::remove_dir_all(&temp_dir);
    });
}

// directory-watcher AC-3: Duplicate detection skips re-processing already ingested files
#[test]
fn test_directory_watcher_ac3_duplicate_detection_skips_ingestion() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = std::env::temp_dir().join(format!("test_watcher_ac3_{}", std::process::id()));
        fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

        let db_path = temp_dir.join("media.db");
        let storage = Arc::new(Storage::new(db_path.to_str().unwrap()));
        storage.init_schema().expect("Schema init failed");

        let config = AppConfig::default();
        let pipeline = Arc::new(create_test_pipeline(config, storage.clone()));

        let watcher_config = WatcherConfig {
            watch_dir: temp_dir.clone(),
            target_dir: None,
            organize_pattern: "{year}/{month}/{country}/{city}".to_string(),
            copy_only: false,
            workers: 1,
        };

        let sample_file = temp_dir.join("duplicate_test.jpg");
        fs::write(&sample_file, "identical_media_bytes_12345").unwrap();

        // First ingestion should succeed
        let first_result = process_single_file(&sample_file, &pipeline, &storage, &watcher_config)
            .await
            .expect("First ingest should run");
        assert!(first_result.is_some(), "First file should be ingested");

        // Second ingestion with exact same file / hash must return Ok(None) -> skipped
        let second_result = process_single_file(&sample_file, &pipeline, &storage, &watcher_config)
            .await
            .expect("Second ingest should run without error");
        assert!(second_result.is_none(), "Duplicate file ingestion should be skipped");

        let _ = fs::remove_dir_all(&temp_dir);
    });
}

// directory-watcher AC-4: Target organization moves/copies files into customizable directory hierarchies based on resolved metadata
#[test]
fn test_directory_watcher_ac4_target_organization_directory_hierarchy() {
    let temp_dir = std::env::temp_dir().join(format!("test_watcher_ac4_{}", std::process::id()));
    let incoming_dir = temp_dir.join("incoming");
    let library_dir = temp_dir.join("library");
    fs::create_dir_all(&incoming_dir).unwrap();
    fs::create_dir_all(&library_dir).unwrap();

    let sample_file = incoming_dir.join("vacation.jpg");
    fs::write(&sample_file, "vacation_photo_content").unwrap();

    let mut doc = UnifiedDocument::new(
        "photo-org-1",
        FileInfo {
            path: sample_file.to_string_lossy().to_string(),
            filename: "vacation.jpg".to_string(),
            size_bytes: 22,
            mime_type: "image/jpeg".to_string(),
            sha256: Some("sha-vacation".to_string()),
        },
    );

    let dt = Utc.with_ymd_and_hms(2026, 11, 25, 14, 30, 0).unwrap();
    doc.timeline = Some(TimelineInfo {
        taken_at: Some(dt),
        imported_at: dt,
        modified_at: None,
        resolution_source: "exif".to_string(),
    });

    let mut loc = LocationInfo::default();
    loc.country = Some("Germany".to_string());
    loc.city = Some("Berlin".to_string());
    doc.location = Some(loc);

    // Test relative destination path formatting
    let rel_path = MediaOrganizer::format_destination_path(&doc, "{year}/{month}/{country}/{city}");
    assert_eq!(
        rel_path,
        PathBuf::from("2026/11/Germany/Berlin/vacation.jpg")
    );

    // Test file organization (copy mode)
    let organized_copy = MediaOrganizer::organize(
        &sample_file,
        &doc,
        &library_dir,
        "{year}/{month}/{country}/{city}",
        true, // copy_only
    )
    .expect("Organization copy failed");

    assert!(organized_copy.exists(), "Organized copy must exist");
    assert!(sample_file.exists(), "Source file should remain when copy_only is true");
    assert_eq!(
        organized_copy,
        library_dir.join("2026/11/Germany/Berlin/vacation.jpg")
    );

    // Test file organization (move mode)
    let move_target_dir = temp_dir.join("library_move");
    let organized_move = MediaOrganizer::organize(
        &sample_file,
        &doc,
        &move_target_dir,
        "{year}/{month}/{country}/{city}",
        false, // move
    )
    .expect("Organization move failed");

    assert!(organized_move.exists(), "Organized moved file must exist");
    assert!(!sample_file.exists(), "Source file should be moved/removed");

    let _ = fs::remove_dir_all(&temp_dir);
}

// directory-watcher AC-5: CLI subcommand flagme-host watch integrates all parameters
#[test]
fn test_ac5_cli_watch_subcommand_arguments() {
    let output = std::process::Command::new("cargo")
        .args(["run", "-p", "flagme-host", "--bin", "flagme-host", "--", "watch"])
        .output()
        .expect("Failed to execute flagme-host binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "Calling watch without arguments should fail");
    assert!(
        stderr.contains("Usage: flagme-host watch <DIR>"),
        "Usage instruction must mention watch <DIR>"
    );
    assert!(
        stderr.contains("--target-dir"),
        "Usage instruction must mention --target-dir"
    );
}
