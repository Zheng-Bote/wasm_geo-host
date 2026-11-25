use crate::storage::Storage;
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use wasm_geo_models::UnifiedDocument;

/// Minimal valid 1x1 JPEG image bytes if neither thumbnail nor original file exists on disk.
const FALLBACK_JPEG_1X1: &[u8] = &[
    0xFF, 0xD8, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10,
    0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10,
    0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10,
    0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10,
    0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01,
    0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00, 0x1F, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06,
    0x07, 0x08, 0x09, 0x0A, 0x0B, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0xD2,
    0xCF, 0x20, 0xFF, 0xD9,
];

#[derive(RustEmbed)]
#[folder = "ui/"]
pub struct WebAssets;

#[derive(Clone)]
pub struct ServerState {
    pub storage: Arc<Storage>,
}

#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    pub page: Option<usize>,
    pub limit: Option<usize>,
    pub country: Option<String>,
    pub tag: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub q: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GeoJsonFeatureCollection {
    pub r#type: &'static str,
    pub features: Vec<GeoJsonFeature>,
}

#[derive(Debug, Serialize)]
pub struct GeoJsonFeature {
    pub r#type: &'static str,
    pub geometry: GeoJsonGeometry,
    pub properties: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct GeoJsonGeometry {
    pub r#type: &'static str,
    pub coordinates: [f64; 2], // [lng, lat]
}

/// Builds the Axum router for the embedded web server and REST/GeoJSON API.
pub fn build_router(state: ServerState) -> Router {
    Router::new()
        .route("/api/photos", get(get_photos))
        .route("/api/photos/:id", get(get_photo_detail))
        .route("/api/photos/:id/thumbnail/:size", get(get_photo_thumbnail))
        .route("/api/map/markers", get(get_map_markers))
        .route("/api/search", get(search_photos))
        .fallback(static_handler)
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// GET /api/photos: Paginated list of UnifiedDocuments with optional country & tag filtering.
async fn get_photos(
    State(state): State<ServerState>,
    Query(params): Query<PaginationParams>,
) -> Json<Vec<UnifiedDocument>> {
    let page = params.page.unwrap_or(1);
    let limit = params.limit.unwrap_or(50);

    if params.country.is_none() && params.tag.is_none() {
        let docs = state.storage.list_documents(page, limit);
        return Json(docs);
    }

    let all_docs = state.storage.list_documents(1, 10_000);
    let filtered: Vec<UnifiedDocument> = all_docs
        .into_iter()
        .filter(|doc| {
            if let Some(ref c_filter) = params.country {
                let cf = c_filter.trim().to_lowercase();
                if !cf.is_empty() {
                    let match_loc = doc
                        .location
                        .as_ref()
                        .map(|l| {
                            l.country_code
                                .as_deref()
                                .map(|c| c.to_lowercase() == cf)
                                .unwrap_or(false)
                                || l.country
                                    .as_deref()
                                    .map(|c| c.to_lowercase().contains(&cf))
                                    .unwrap_or(false)
                        })
                        .unwrap_or(false);
                    if !match_loc {
                        return false;
                    }
                }
            }
            if let Some(ref t_filter) = params.tag {
                let tf = t_filter.trim().to_lowercase();
                if !tf.is_empty() {
                    let match_tag = doc
                        .ai
                        .as_ref()
                        .map(|a| a.tags.iter().any(|t| t.label.to_lowercase().contains(&tf)))
                        .unwrap_or(false);
                    if !match_tag {
                        return false;
                    }
                }
            }
            true
        })
        .skip(page.saturating_sub(1) * limit)
        .take(limit)
        .collect();

    Json(filtered)
}

/// GET /api/photos/:id: Single UnifiedDocument by ID.
async fn get_photo_detail(
    State(state): State<ServerState>,
    Path(id): Path<String>,
) -> Result<Json<UnifiedDocument>, StatusCode> {
    state
        .storage
        .get_document(&id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// GET /api/photos/:id/thumbnail/:size: Serves thumbnail bytes with HTTP caching headers.
async fn get_photo_thumbnail(
    State(state): State<ServerState>,
    Path((id, size)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    let doc = state.storage.get_document(&id).ok_or(StatusCode::NOT_FOUND)?;

    let thumb = doc
        .thumbnails
        .iter()
        .find(|t| t.size_label.eq_ignore_ascii_case(&size))
        .or_else(|| doc.thumbnails.first());

    let (bytes, cache_key) = if let Some(t) = thumb {
        if let Ok(b) = std::fs::read(&t.path) {
            (b, t.cache_key.clone())
        } else if let Ok(orig) = std::fs::read(&doc.file.path) {
            (orig, t.cache_key.clone())
        } else {
            return Err(StatusCode::NOT_FOUND);
        }
    } else if let Ok(orig) = std::fs::read(&doc.file.path) {
        (orig, doc.id.clone())
    } else {
        (FALLBACK_JPEG_1X1.to_vec(), doc.id.clone())
    };

    let etag = format!("\"{}\"", cache_key);

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400"),
    );
    if let Ok(val) = HeaderValue::from_str(&etag) {
        headers.insert(header::ETAG, val);
    }

    Ok((headers, bytes).into_response())
}

/// GET /api/map/markers: Standard GeoJSON FeatureCollection of all photos with coordinates.
async fn get_map_markers(State(state): State<ServerState>) -> Json<GeoJsonFeatureCollection> {
    let all_docs = state.storage.list_documents(1, 10_000);
    let mut features = Vec::new();

    for doc in all_docs {
        if let Some(loc) = &doc.location {
            if let (Some(lat), Some(lng)) = (loc.latitude, loc.longitude) {
                let country_name = loc.country.clone().unwrap_or_default();
                let city_name = loc.city.clone().unwrap_or_default();
                let flag = doc.country.as_ref().map(|c| c.flag_emoji.clone()).unwrap_or_default();
                let weather_str = doc
                    .weather
                    .as_ref()
                    .map(|w| {
                        format!(
                            "{} {} ({:.1}°C)",
                            w.icon_emoji,
                            w.condition,
                            w.temperature.unwrap_or(0.0)
                        )
                    })
                    .unwrap_or_default();

                let feature = GeoJsonFeature {
                    r#type: "Feature",
                    geometry: GeoJsonGeometry {
                        r#type: "Point",
                        coordinates: [lng, lat],
                    },
                    properties: serde_json::json!({
                        "id": doc.id,
                        "filename": doc.file.filename,
                        "city": city_name,
                        "country": country_name,
                        "flag": flag,
                        "weather": weather_str,
                        "thumbnail_url": format!("/api/photos/{}/thumbnail/small", doc.id)
                    }),
                };
                features.push(feature);
            }
        }
    }

    Json(GeoJsonFeatureCollection {
        r#type: "FeatureCollection",
        features,
    })
}

/// GET /api/search?q=:query: Full-text search via FTS5.
async fn search_photos(
    State(state): State<ServerState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<crate::storage::SearchResult>>, StatusCode> {
    let query = params.q.unwrap_or_default();
    if query.trim().is_empty() {
        return Ok(Json(Vec::new()));
    }

    match state.storage.search(&query) {
        Ok(results) => Ok(Json(results)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Fallback handler serving embedded SPA assets.
async fn static_handler(uri: axum::http::Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/').to_string();
    if path.is_empty() {
        path = "index.html".to_string();
    }

    match WebAssets::get(&path) {
        Some(content) => {
            let mime = if path.ends_with(".html") {
                "text/html; charset=utf-8"
            } else if path.ends_with(".js") {
                "application/javascript"
            } else if path.ends_with(".css") {
                "text/css"
            } else if path.ends_with(".png") {
                "image/png"
            } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
                "image/jpeg"
            } else {
                "application/octet-stream"
            };
            ([(header::CONTENT_TYPE, mime)], content.data).into_response()
        }
        None => {
            if let Some(index) = WebAssets::get("index.html") {
                ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], index.data).into_response()
            } else {
                (StatusCode::NOT_FOUND, "404 Not Found").into_response()
            }
        }
    }
}

/// Runs the embedded Axum web server on the given address.
pub async fn run_server(addr: SocketAddr, storage: Arc<Storage>) -> anyhow::Result<()> {
    let state = ServerState { storage };
    let router = build_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(address = %addr, "Embedded web server listening");
    axum::serve(listener, router).await?;
    Ok(())
}
