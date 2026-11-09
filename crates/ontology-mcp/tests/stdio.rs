//! End-to-end MCP protocol tests over stdio.

use std::error::Error;

use rmcp::model::CallToolRequestParams;
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;
use serde_json::{json, Map, Value};

fn arguments(value: Value) -> Map<String, Value> {
    value.as_object().expect("arguments object").clone()
}

fn binary() -> tokio::process::Command {
    tokio::process::Command::new(env!("CARGO_BIN_EXE_ontology-mcp"))
}

#[tokio::test(flavor = "multi_thread")]
async fn exposes_six_tools_and_enforces_read_only() -> Result<(), Box<dyn Error>> {
    let mut command = binary();
    command
        .env("ONTO_MCP_ACCESS", "read-only")
        .env_remove("ONTO_MCP_SERVER")
        .env_remove("ONTO_STORAGE_PATH");
    let mut client = ().serve(TokioChildProcess::new(command)?).await?;

    let mut names: Vec<String> = client
        .list_all_tools()
        .await?
        .into_iter()
        .map(|tool| tool.name.into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["jobs", "ontology", "project", "rdf", "registry", "semantic"]
    );

    let result = client
        .call_tool(
            CallToolRequestParams::new("rdf").with_arguments(arguments(json!({
                "action": "query",
                "sparql": "ASK { ?s ?p ?o }"
            }))),
        )
        .await?;
    let body = result.structured_content.expect("structured result");
    assert_eq!(body["success"], true);
    assert_eq!(body["result"]["resultType"], "boolean");

    let catalogs = client
        .call_tool(
            CallToolRequestParams::new("registry").with_arguments(arguments(json!({
                "action": "catalog",
                "operation": "list"
            }))),
        )
        .await?;
    assert_eq!(
        catalogs.structured_content.expect("registry list")["result"]["kind"],
        "catalogs"
    );

    let denied = client
        .call_tool(
            CallToolRequestParams::new("rdf").with_arguments(arguments(json!({
                "action": "create",
                "target": "urn:example:read-only",
                "types": ["urn:example:Thing"]
            }))),
        )
        .await?;
    assert_eq!(denied.is_error, Some(true));
    let body = denied.structured_content.expect("structured error");
    assert_eq!(body["error"]["kind"], "unsupported");

    let denied = client
        .call_tool(
            CallToolRequestParams::new("registry").with_arguments(arguments(json!({
                "action": "catalog",
                "operation": "create",
                "body": { "title": "Denied catalog" }
            }))),
        )
        .await?;
    assert_eq!(denied.is_error, Some(true));
    assert_eq!(
        denied.structured_content.expect("registry error")["error"]["kind"],
        "unsupported"
    );

    let denied = client
        .call_tool(
            CallToolRequestParams::new("project").with_arguments(arguments(json!({
                "action": "project",
                "operation": "create",
                "body": { "title": "Denied project" }
            }))),
        )
        .await?;
    assert_eq!(denied.is_error, Some(true));
    assert_eq!(
        denied.structured_content.expect("project error")["error"]["kind"],
        "unsupported"
    );

    client.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn writes_jobs_and_destructive_confirmation_work() -> Result<(), Box<dyn Error>> {
    let mut command = binary();
    command
        .env("ONTO_MCP_ACCESS", "read-write")
        .env("ONTO_MCP_ALLOW_DESTRUCTIVE", "true")
        .env_remove("ONTO_MCP_SERVER")
        .env_remove("ONTO_STORAGE_PATH");
    let mut client = ().serve(TokioChildProcess::new(command)?).await?;

    let created = client
        .call_tool(
            CallToolRequestParams::new("rdf").with_arguments(arguments(json!({
                "action": "create",
                "body": {
                    "id": "urn:example:phase5",
                    "types": ["urn:example:Thing"]
                }
            }))),
        )
        .await?;
    let body = created.structured_content.expect("create result");
    assert_eq!(body["success"], true);
    assert_eq!(body["result"]["kind"], "instance");

    let fetched = client
        .call_tool(
            CallToolRequestParams::new("rdf").with_arguments(arguments(json!({
                "action": "get",
                "target": "urn:example:phase5"
            }))),
        )
        .await?;
    assert_eq!(
        fetched.structured_content.unwrap()["result"]["resource"]["id"],
        "urn:example:phase5"
    );

    let submitted = client
        .call_tool(
            CallToolRequestParams::new("jobs").with_arguments(arguments(json!({
                "action": "submit",
                "kind": "rdf_export",
                "format": "nquads"
            }))),
        )
        .await?;
    let body = submitted.structured_content.expect("job submission");
    assert_eq!(body["success"], true);
    let job_id = body["result"]["id"].as_str().expect("job id").to_owned();
    assert_eq!(body["metadata"]["state"], "queued");

    let mut completed = false;
    for _ in 0..50 {
        let status = client
            .call_tool(
                CallToolRequestParams::new("jobs").with_arguments(arguments(json!({
                    "action": "get",
                    "id": job_id
                }))),
            )
            .await?;
        let body = status.structured_content.expect("job status");
        if body["result"]["state"] == "completed" {
            completed = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(completed, "export job did not complete");

    let refused = client
        .call_tool(
            CallToolRequestParams::new("rdf").with_arguments(arguments(json!({
                "action": "delete",
                "target": "urn:example:phase5"
            }))),
        )
        .await?;
    assert_eq!(refused.is_error, Some(true));
    assert_eq!(
        refused.structured_content.unwrap()["error"]["kind"],
        "validation"
    );

    let deleted = client
        .call_tool(
            CallToolRequestParams::new("rdf").with_arguments(arguments(json!({
                "action": "delete",
                "target": "urn:example:phase5",
                "confirm": true
            }))),
        )
        .await?;
    let body = deleted.structured_content.expect("delete result");
    assert_eq!(body["success"], true);
    assert_eq!(body["result"]["type"], "report");

    client.close().await?;
    Ok(())
}
