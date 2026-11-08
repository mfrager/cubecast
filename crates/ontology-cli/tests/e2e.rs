//! End-to-end CLI tests against a live HTTP server.

use std::process::Command as ProcessCommand;
use std::sync::Arc;

use axum::Router;
use ontology_api::{router, AppState};
use ontology_application::ApplicationService;
use ontology_rdf::OxigraphStore;
use serde_json::Value;

fn app() -> Router {
    let store = OxigraphStore::in_memory().expect("in-memory store");
    store.bootstrap_schema().expect("bootstrap schema");
    router(AppState::open(Arc::new(ApplicationService::new(Arc::new(
        store,
    )))))
}

fn onto(server: &str, args: &[&str]) -> std::process::Output {
    ProcessCommand::new(env!("CARGO_BIN_EXE_onto"))
        .arg("--server")
        .arg(server)
        .args(args)
        .output()
        .expect("run onto")
}

fn parse(output: &std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "onto failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON output")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_round_trips_instances_and_exports_over_http() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        axum::serve(listener, app()).await.expect("serve");
    });
    let base = format!("http://{address}");

    let config = tokio::task::spawn_blocking({
        let base = base.clone();
        move || onto(&base, &["config"])
    })
    .await
    .expect("config task");
    let config = parse(&config);
    assert_eq!(config["server"], base);

    let payload =
        r#"{"id":"urn:example:cli-e2e","types":["https://example.org/Thing"],"properties":{}}"#;
    let created = tokio::task::spawn_blocking({
        let base = base.clone();
        move || onto(&base, &["instance", "create", "--json", payload])
    })
    .await
    .expect("create task");
    let created = parse(&created);
    assert_eq!(created["type"], "resource");
    assert_eq!(created["kind"], "instance");

    let fetched = tokio::task::spawn_blocking({
        let base = base.clone();
        move || onto(&base, &["instance", "get", "urn:example:cli-e2e"])
    })
    .await
    .expect("get task");
    let fetched = parse(&fetched);
    assert_eq!(fetched["resource"]["id"], "urn:example:cli-e2e");

    let graphs = tokio::task::spawn_blocking({
        let base = base.clone();
        move || onto(&base, &["rdf", "graphs"])
    })
    .await
    .expect("graphs task");
    let graphs = parse(&graphs);
    assert_eq!(graphs["type"], "collection");

    let submitted = tokio::task::spawn_blocking({
        let base = base.clone();
        move || onto(&base, &["job", "export", "--format", "nquads"])
    })
    .await
    .expect("export task");
    let submitted = parse(&submitted);
    assert_eq!(submitted["type"], "job");
    let job = submitted["id"].as_str().expect("job id").to_owned();

    let watched = tokio::task::spawn_blocking({
        let base = base.clone();
        move || onto(&base, &["job", "watch", &job, "--interval-secs", "1"])
    })
    .await
    .expect("watch task");
    let watched = parse(&watched);
    assert_eq!(watched["state"], "completed");

    server.abort();
}
