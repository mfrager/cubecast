//! End-to-end MCP tests spanning schema, taxonomy, semantics, and jobs.

use std::error::Error;
use std::time::Duration;

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
async fn semantic_taxonomy_and_job_tools_work_together() -> Result<(), Box<dyn Error>> {
    let class = "https://example.org/Equipment";
    let subclass = "https://example.org/Hardware";
    let scheme = "https://example.org/categories";
    let concept = "https://example.org/networking";
    let instance = "https://example.org/mcp-router";

    let mut command = binary();
    command
        .env("ONTO_MCP_ACCESS", "read-write")
        .env_remove("ONTO_MCP_ALLOW_DESTRUCTIVE")
        .env_remove("ONTO_MCP_SERVER")
        .env_remove("ONTO_STORAGE_PATH");
    let mut client = ().serve(TokioChildProcess::new(command)?).await?;

    let created = client
        .call_tool(
            CallToolRequestParams::new("ontology").with_arguments(arguments(json!({
                "action": "schema",
                "kind": "class",
                "operation": "create",
                "target": class
            }))),
        )
        .await?;
    let body = created.structured_content.expect("class result");
    assert_eq!(body["success"], true, "{body}");

    let created = client
        .call_tool(
            CallToolRequestParams::new("ontology").with_arguments(arguments(json!({
                "action": "schema",
                "kind": "class",
                "operation": "create",
                "body": {
                    "id": subclass,
                    "sub_class_of": [class]
                }
            }))),
        )
        .await?;
    let body = created.structured_content.expect("subclass result");
    assert_eq!(body["success"], true, "{body}");

    client
        .call_tool(
            CallToolRequestParams::new("ontology").with_arguments(arguments(json!({
                "action": "scheme",
                "operation": "create",
                "target": scheme,
                "label": "Categories",
                "language": "en"
            }))),
        )
        .await?;
    client
        .call_tool(
            CallToolRequestParams::new("ontology").with_arguments(arguments(json!({
                "action": "concept",
                "operation": "create",
                "target": concept,
                "scheme": scheme,
                "label": "Networking Equipment",
                "language": "en"
            }))),
        )
        .await?;

    let created = client
        .call_tool(
            CallToolRequestParams::new("rdf").with_arguments(arguments(json!({
                "action": "create",
                "body": {
                    "id": instance,
                    "types": [subclass],
                    "properties": {
                        "https://example.org/name": [
                            { "term": "literal", "value": { "value": "Networking Equipment" } }
                        ]
                    }
                }
            }))),
        )
        .await?;
    let body = created.structured_content.expect("instance result");
    assert_eq!(body["success"], true, "{body}");

    let classified = client
        .call_tool(
            CallToolRequestParams::new("semantic").with_arguments(arguments(json!({
                "action": "classify",
                "target": instance,
                "threshold": 0.5
            }))),
        )
        .await?;
    let body = classified
        .structured_content
        .expect("classification result");
    assert_eq!(body["success"], true, "{body}");
    let items = body["result"]["items"]
        .as_array()
        .expect("classification items");
    assert!(!items.is_empty(), "{body}");
    let classification = items[0]["id"]
        .as_str()
        .expect("classification id")
        .to_owned();
    assert_eq!(items[0]["concept"], concept);

    let reviewed = client
        .call_tool(
            CallToolRequestParams::new("semantic").with_arguments(arguments(json!({
                "action": "review",
                "id": classification,
                "approved": true
            }))),
        )
        .await?;
    let body = reviewed.structured_content.expect("review result");
    assert_eq!(body["success"], true, "{body}");
    assert_eq!(body["result"]["items"][0]["status"], "approved");

    let preview = client
        .call_tool(
            CallToolRequestParams::new("semantic").with_arguments(arguments(json!({
                "action": "infer",
                "target": instance
            }))),
        )
        .await?;
    let body = preview.structured_content.expect("inference result");
    assert_eq!(body["success"], true, "{body}");
    assert!(body["result"]["classes"]
        .as_array()
        .expect("classes")
        .iter()
        .any(|value| value["class"] == class));

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
    let job = body["result"]["id"].as_str().expect("job id").to_owned();

    let mut completed = false;
    for _ in 0..100 {
        let status = client
            .call_tool(
                CallToolRequestParams::new("jobs").with_arguments(arguments(json!({
                    "action": "get",
                    "id": job
                }))),
            )
            .await?;
        let body = status.structured_content.expect("job status");
        if body["result"]["state"] == "completed" {
            completed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(completed, "export job did not complete");

    client.close().await?;
    Ok(())
}
