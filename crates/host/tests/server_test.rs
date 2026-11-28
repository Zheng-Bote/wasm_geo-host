use axum::http::StatusCode;
use flagme_host::{build_router, ServerState, Storage};
use std::fs;
use std::sync::Arc;
use tower::ServiceExt;
use wasm_geo_models::{CountryInfo, FileInfo, LocationInfo, ThumbnailEntry, UnifiedDocument};

// embedded-web-ui AC-1: Embedded Server Lifecycle: CLI subcommand flagme-host serve starts the Axum server on configured port
#[test]
fn test_embedded_web_ui_ac1_server_lifecycle() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = std::env::temp_dir().join(format!("test_server_ac1_{}", std::process::id()));
        fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

        let db_path = temp_dir.join("media.db");
        let storage = Arc::new(Storage::new(db_path.to_str().unwrap()));
        storage.init_schema().expect("Schema init failed");

        let state = ServerState { storage };
        let app = build_router(state);

        // Verify router handles requests directly via ServiceExt::oneshot
        let req = axum::http::Request::builder()
            .uri("/api/photos")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let _ = fs::remove_dir_all(&temp_dir);
    });
}

// embedded-web-ui AC-2: REST Media Endpoints: REST API delivers paginated photo lists, document details, and cached thumbnails
#[test]
fn test_embedded_web_ui_ac2_rest_media_endpoints() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let temp_dir = std::env::temp_dir().join(format!("test_server_ac2_{}", std::process::id()));
        fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

        let db_path = temp_dir.join("media.db");
        let storage = Arc::new(Storage::new(db_path.to_str().unwrap()));
        storage.init_schema().expect("Schema init failed");

        // Create sample thumbnail file
        let thumb_path = temp_dir.join("thumb_small.jpg");
        fs::write(&thumb_path, b"test_jpeg_thumbnail_bytes").unwrap();

        let mut doc = UnifiedDocument::new(
            "photo-web-1",
            FileInfo {
                path: "/media/photo1.jpg".to_string(),
                filename: "photo1.jpg".to_string(),
                size_bytes: 1024,
                mime_type: "image/jpeg".to_string(),
                sha256: Some("sha256-photo1".to_string()),
            },
        );
        doc.thumbnails.push(ThumbnailEntry {
            size_label: "small".to_string(),
            width: 320,
            height: 240,
            path: thumb_path.to_str().unwrap().to_string(),
            cache_key: "etag-key-123".to_string(),
        });
        storage.save_document(&doc).expect("Failed to save document");

        let state = ServerState { storage };
        let app = build_router(state);

        // 1. Test GET /api/photos
        let req = axum::http::Request::builder()
            .uri("/api/photos?page=1&limit=10")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let docs: Vec<UnifiedDocument> = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].id, "photo-web-1");

        // 2. Test GET /api/photos/photo-web-1
        let req_detail = axum::http::Request::builder()
            .uri("/api/photos/photo-web-1")
            .body(axum::body::Body::empty())
            .unwrap();
        let res_detail = app.clone().oneshot(req_detail).await.unwrap();
        assert_eq!(res_detail.status(), StatusCode::OK);

        // 3. Test GET /api/photos/photo-web-1/thumbnail/small (Cache headers)
        let req_thumb = axum::http::Request::builder()
            .uri("/api/photos/photo-web-1/thumbnail/small")
            .body(axum::body::Body::empty())
            .unwrap();
        let res_thumb = app.clone().oneshot(req_thumb).await.unwrap();
        assert_eq!(res_thumb.status(), StatusCode::OK);
        assert_eq!(
            res_thumb.headers().get("cache-control").unwrap(),
            "private, max-age=86400"
        );
        assert_eq!(
            res_thumb.headers().get("etag").unwrap(),
            "\"etag-key-123\""
        );

        let _ = fs::remove_dir_all(&temp_dir);
    });
}

// embedded-web-ui AC-3: GeoJSON Map API: /api/map/markers returns standard FeatureCollection of mapped photos
#[test]
fn test_embedded_web_ui_ac3_geojson_map_markers() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = std::env::temp_dir().join(format!("test_server_ac3_{}", std::process::id()));
        fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

        let db_path = temp_dir.join("media.db");
        let storage = Arc::new(Storage::new(db_path.to_str().unwrap()));
        storage.init_schema().expect("Schema init failed");

        let mut doc = UnifiedDocument::new(
            "photo-geo-1",
            FileInfo {
                path: "/media/geo1.jpg".to_string(),
                filename: "geo1.jpg".to_string(),
                size_bytes: 2048,
                mime_type: "image/jpeg".to_string(),
                sha256: Some("sha-geo1".to_string()),
            },
        );

        let mut loc = LocationInfo::default();
        loc.latitude = Some(52.52);
        loc.longitude = Some(13.405);
        loc.city = Some("Berlin".to_string());
        loc.country = Some("Germany".to_string());
        loc.country_code = Some("DE".to_string());
        doc.location = Some(loc);

        doc.country = Some(CountryInfo {
            country_code: "DE".to_string(),
            country_name: "Germany".to_string(),
            flag_emoji: "🇩🇪".to_string(),
            continent: "Europe".to_string(),
            region: "Western Europe".to_string(),
        });

        storage.save_document(&doc).expect("Failed to save doc");

        let state = ServerState { storage };
        let app = build_router(state);

        let req = axum::http::Request::builder()
            .uri("/api/map/markers")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json_val: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(json_val["type"], "FeatureCollection");
        let features = json_val["features"].as_array().expect("features array");
        assert_eq!(features.len(), 1);

        let feat = &features[0];
        assert_eq!(feat["type"], "Feature");
        assert_eq!(feat["geometry"]["type"], "Point");
        assert_eq!(feat["geometry"]["coordinates"][0], 13.405); // lng
        assert_eq!(feat["geometry"]["coordinates"][1], 52.52);  // lat
        assert_eq!(feat["properties"]["city"], "Berlin");
        assert_eq!(feat["properties"]["flag"], "🇩🇪");

        let _ = fs::remove_dir_all(&temp_dir);
    });
}

// embedded-web-ui AC-4: Embedded Web Dashboard: Embedded SPA renders index.html with interactive map and photo grid
#[test]
fn test_embedded_web_ui_ac4_embedded_dashboard_assets() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = std::env::temp_dir().join(format!("test_server_ac4_{}", std::process::id()));
        fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

        let db_path = temp_dir.join("media.db");
        let storage = Arc::new(Storage::new(db_path.to_str().unwrap()));
        storage.init_schema().expect("Schema init failed");

        let state = ServerState { storage };
        let app = build_router(state);

        let req = axum::http::Request::builder()
            .uri("/")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert!(
            res.headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("text/html")
        );

        let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let html_str = String::from_utf8_lossy(&body_bytes);
        assert!(html_str.contains("flagme-host — Media Dashboard"));
        assert!(html_str.contains("id=\"map\""));
        assert!(html_str.contains("id=\"galleryGrid\""));

        let _ = fs::remove_dir_all(&temp_dir);
    });
}
