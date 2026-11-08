//! Phase 5 production-hardening tests: metrics and rate limiting.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use ontology_api::{router, AppState};
use ontology_application::ApplicationService;
use ontology_rdf::OxigraphStore;
use serde_json::Value;
use tower::ServiceExt;

fn service() -> Arc<ApplicationService> {
    let store = OxigraphStore::in_memory().expect("in-memory store");
    store.bootstrap_schema().expect("bootstrap schema");
    Arc::new(ApplicationService::new(Arc::new(store)))
}

async fn call(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.expect("router response");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    (status, serde_json::from_slice(&body).expect("JSON body"))
}

fn get(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .body(Body::empty())
        .expect("request")
}

#[tokio::test]
async fn metrics_report_requests_and_errors() {
    let app = router(AppState::open(service()));

    let (status, _) = call(&app, get("/api/v1/system/status")).await;
    assert_eq!(status, StatusCode::OK);

    let (status, value) = call(&app, get("/api/v1/system/metrics")).await;
    assert_eq!(status, StatusCode::OK);
    // Metrics count completed requests, so the metrics call itself is not yet
    // included in the snapshot it returns.
    assert!(value["requests"].as_u64().expect("request count") >= 1);
    assert_eq!(value["errors"].as_u64(), Some(0));
}

#[tokio::test]
async fn rate_limit_returns_retry_after() {
    let app = router(AppState::open(service()).with_rate_limit(Some(1)));

    let (status, _) = call(&app, get("/api/v1/system/status")).await;
    assert_eq!(status, StatusCode::OK);

    let response = app
        .clone()
        .oneshot(get("/api/v1/system/status"))
        .await
        .expect("rate-limited response");
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        response.headers().get("retry-after"),
        Some(&"60".parse().unwrap())
    );
}
