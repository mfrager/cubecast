//! HTTP coverage for the registry and project routes.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use ontology_api::{router, AppState};
use ontology_application::ApplicationService;
use ontology_core::config::{Config, RegistryConfig};
use ontology_rdf::OxigraphStore;
use serde_json::{json, Value};
use tower::ServiceExt;

fn app() -> (tempfile::TempDir, Router) {
    let directory = tempfile::tempdir().expect("temp dir");
    let store = OxigraphStore::in_memory().expect("store");
    store.bootstrap_schema().expect("schema");
    let config = Config {
        registry: RegistryConfig {
            artifact_dir: directory.path().join("artifacts"),
            ..RegistryConfig::default()
        },
        ..Config::default()
    };
    let service = Arc::new(
        ApplicationService::new(Arc::new(store))
            .with_semantic_layer(&config)
            .expect("semantic layer"),
    );
    (directory, router(AppState::open(service)))
}

async fn call(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.expect("router response");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    (status, serde_json::from_slice(&body).expect("JSON body"))
}

async fn call_bytes(app: &Router, request: Request<Body>) -> (StatusCode, Vec<u8>) {
    let response = app.clone().oneshot(request).await.expect("router response");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    (status, body.to_vec())
}

fn get(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .body(Body::empty())
        .expect("request")
}

fn json_request(method: &str, uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("request")
}

fn planning() -> &'static str {
    include_str!("../fixtures/registry/planning-1.0.ttl")
}

#[tokio::test]
async fn registry_and_project_http_flow() {
    let (_directory, app) = app();

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            "/api/v1/registry/catalogs",
            &json!({ "title": "HTTP catalog", "description": "Registry over HTTP" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["type"], "registry");
    assert_eq!(value["kind"], "catalog");
    let catalog = value["id"].as_str().expect("catalog id").to_owned();

    let (status, value) = call(&app, get("/api/v1/registry/catalogs")).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "catalogs");
    assert_eq!(value["count"], 1);

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            "/api/v1/registry/ingestions",
            &json!({
                "catalog": catalog,
                "source": "planning.ttl",
                "data": planning(),
                "asset_iri": "https://example.org/planning",
                "version": "1.0",
                "format": "turtle"
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "candidate");
    let candidate = value["id"].as_str().expect("candidate id").to_owned();
    let candidate_uri = candidate.clone();
    assert_eq!(
        value["release_key"].as_str().expect("release key").len(),
        16
    );

    let (status, value) = call(
        &app,
        get(&format!("/api/v1/registry/ingestions/{candidate}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "candidate");

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/registry/ingestions/{candidate}/analyze"),
            &json!({}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "candidate");

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/registry/ingestions/{candidate}/validate"),
            &json!({}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "validation");
    assert_eq!(value["conforms"], true);

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/registry/candidates/{candidate}/publish"),
            &json!({ "channel": "stable" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "release");
    let release = value["id"].as_str().expect("release id").to_owned();
    let key = value["key"].as_str().expect("release key").to_owned();

    let (status, value) = call(&app, get(&format!("/api/v1/registry/releases/{key}"))).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "release");
    assert_eq!(value["channel"], "stable");

    let (status, assets) = call(
        &app,
        get(&format!("/api/v1/registry/catalogs/{catalog}/assets")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{assets}");
    let asset_key = assets["items"][0]["key"]
        .as_str()
        .expect("asset key")
        .to_owned();
    let (status, value) = call(
        &app,
        get(&format!("/api/v1/registry/assets/{asset_key}/releases")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "releases");
    assert_eq!(value["count"], 1);

    let (status, value) = call(
        &app,
        get(&format!("/api/v1/registry/releases/{key}/terms?q=Plan")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "terms");
    assert_eq!(value["count"], 1);

    let (status, value) = call(
        &app,
        get(&format!("/api/v1/registry/releases/{key}/statistics")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "statistics");
    assert_eq!(value["statistics"]["quads"], 13);

    let (status, value) = call(
        &app,
        get(&format!("/api/v1/registry/releases/{key}/imports")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "imports");
    assert_eq!(value["items"].as_array().expect("imports").len(), 1);

    let (status, value) = call(
        &app,
        get(&format!("/api/v1/registry/releases/{key}/reports")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "reports");

    let (status, value) = call(&app, get("/api/v1/registry/search?q=Planning")).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "search");
    assert!(!value["items"].as_array().expect("hits").is_empty());

    let (status, bytes) = call_bytes(
        &app,
        get(&format!("/api/v1/registry/download/{key}/turtle")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(String::from_utf8(bytes)
        .expect("turtle")
        .contains("Planning"));

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            "/api/v1/projects",
            &json!({ "title": "HTTP project", "owner": "team" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "project");
    let project = value["id"].as_str().expect("project id").to_owned();

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/projects/{project}/goals"),
            &json!({ "title": "Ship planning" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "goal");

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/projects/{project}/requirements"),
            &json!({ "title": "Support plans", "kind": "capability" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "requirement");

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/projects/{project}/work"),
            &json!({ "kind": "task", "title": "Draft", "assignee": "alice" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "work");
    let work = value["id"].as_str().expect("work id").to_owned();
    let work_uri = work.clone();

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/work/{work}/transition"),
            &json!({ "event": "start" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["status"], "active");

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/work/{work}/assignments"),
            &json!({ "assignee": "bob" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["assignee"], "bob");

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/projects/{project}/deliverables"),
            &json!({
                "title": "Planning ontology",
                "kinds": ["ontology"],
                "asset": "https://example.org/planning",
                "release": release,
                "acceptance_criteria": ["validated"]
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "deliverable");
    let deliverable = value["id"].as_str().expect("deliverable id").to_owned();
    let deliverable_uri = deliverable.clone();

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/deliverables/{deliverable}/accept"),
            &json!({ "evidence": "publication", "criterion": "validated" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["status"], "accepted");

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/projects/{project}/snapshots"),
            &json!({}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "snapshot");

    let (status, value) = call(&app, get(&format!("/api/v1/projects/{project}/overview"))).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "overview");
    assert_eq!(value["accepted_deliverables"], 1);

    let (status, value) = call(&app, get(&format!("/api/v1/projects/{project}/timeline"))).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "timeline");

    // A different body for an existing version is a conflict, and malformed
    // source data maps to 422 rather than a generic 400.
    let (status, value) = call(
        &app,
        json_request(
            "POST",
            "/api/v1/registry/ingestions",
            &json!({
                "source": "planning-copy.ttl",
                "data": planning().replace("Planning ontology", "Renamed planning"),
                "asset_iri": "https://example.org/planning",
                "version": "1.0",
                "format": "turtle"
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{value}");

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            "/api/v1/registry/ingestions",
            &json!({
                "source": "broken.ttl",
                "data": "this is not turtle",
                "asset_iri": "https://example.org/broken",
                "version": "1.0",
                "format": "turtle"
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{value}");
    let _ = (candidate_uri, work_uri, deliverable_uri);
}
