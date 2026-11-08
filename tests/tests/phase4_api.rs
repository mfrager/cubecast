//! Phase 4 HTTP API tests.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use ontology_api::{router, AppState};
use ontology_application::ApplicationService;
use ontology_core::id::Id;
use ontology_core::job::JobRequest;
use ontology_core::model::NewInstance;
use ontology_core::semantic::ClassificationOptions;
use ontology_integration_tests::memory_store_handle;
use serde_json::{json, Value};
use tower::ServiceExt;

fn app() -> Router {
    let service = Arc::new(ApplicationService::new(memory_store_handle()));
    router(AppState::open(service))
}

async fn call(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.expect("router response");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, value)
}

fn json_request(method: &str, uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("request")
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request")
}

#[tokio::test]
async fn reports_health_and_status() {
    let app = app();
    let (status, value) = call(&app, get("/api/v1/system/health")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["status"], "ok");
    assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));

    let (status, value) = call(&app, get("/api/v1/system/status")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["status"], "ok");
    assert_eq!(value["authentication"], false);
}

#[tokio::test]
async fn creates_reads_and_deletes_an_instance() {
    let app = app();
    let id = Id::new("urn:example:phase4-api-instance").expect("instance id");
    let created = NewInstance::new(id.clone())
        .with_type("https://example.org/Product".parse().expect("type"));
    let (status, value) = call(
        &app,
        json_request(
            "POST",
            "/api/v1/instances",
            &serde_json::to_value(&created).unwrap(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["type"], "resource");
    assert_eq!(value["kind"], "instance");

    let uri = format!("/api/v1/instances/{id}");
    let (status, value) = call(&app, get(&uri)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["resource"]["id"], id.as_str());

    let (status, _) = call(
        &app,
        Request::builder()
            .method("DELETE")
            .uri(&uri)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, value) = call(&app, get(&uri)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(value["error"]["kind"], "not_found");
}

#[tokio::test]
async fn runs_sparql_imports_and_exports() {
    let app = app();
    let turtle = r#"
@prefix ex: <https://example.org/> .
ex:phase4-import ex:name "Imported" .
"#;
    let (status, value) = call(
        &app,
        json_request(
            "POST",
            "/api/v1/import",
            &json!({ "format": "turtle", "data": turtle }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["type"], "report");

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            "/api/v1/sparql/query",
            &json!({ "query": "SELECT ?s WHERE { ?s ?p ?o } LIMIT 10" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["type"], "query");
    assert_eq!(value["resultType"], "solutions");

    let (status, value) = call(
        &app,
        json_request("POST", "/api/v1/export", &json!({ "format": "nquads" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["type"], "export");
    assert_eq!(value["format"], "nquads");
    assert!(value["data"]
        .as_str()
        .is_some_and(|data| data.contains("phase4-import")));
}

#[tokio::test]
async fn dispatches_generic_commands() {
    let app = app();
    let (status, value) = call(
        &app,
        json_request(
            "POST",
            "/api/v1/commands",
            &json!({ "family": "rdf", "command": { "action": "listGraphs" } }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["type"], "collection");
}

#[tokio::test]
async fn enforces_bearer_authentication() {
    let service = Arc::new(ApplicationService::new(memory_store_handle()));
    let app = router(AppState::new(service, Some("secret".to_owned())));

    let (status, value) = call(&app, get("/api/v1/system/status")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(value["error"]["kind"], "unauthorized");

    let request = Request::builder()
        .uri("/api/v1/system/status")
        .header(header::AUTHORIZATION, "Bearer secret")
        .body(Body::empty())
        .unwrap();
    let (status, _) = call(&app, request).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = call(&app, get("/api/v1/system/health")).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn reports_not_found_for_unknown_resources() {
    let app = app();
    let (status, value) = call(&app, get("/api/v1/instances/urn:example:missing")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(value["error"]["kind"], "not_found");
}

#[tokio::test]
async fn jobs_run_in_the_background() {
    let app = app();
    let request = JobRequest::ClassifyBatch {
        instances: Vec::new(),
        options: ClassificationOptions::default(),
    };
    let (status, value) = call(
        &app,
        json_request(
            "POST",
            "/api/v1/jobs",
            &serde_json::to_value(request).unwrap(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["type"], "job");
    let id = value["id"].as_str().expect("job id").to_owned();

    let mut finished = None;
    for _ in 0..200 {
        let (status, value) = call(&app, get(&format!("/api/v1/jobs/{id}"))).await;
        assert_eq!(status, StatusCode::OK);
        if matches!(
            value["state"].as_str(),
            Some("completed" | "failed" | "cancelled")
        ) {
            finished = Some(value);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let value = finished.expect("job finished");
    assert_eq!(value["state"], "completed");
    assert_eq!(value["progress"], 1.0);
}
