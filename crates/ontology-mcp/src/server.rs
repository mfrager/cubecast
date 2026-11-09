//! The four-tool MCP server.

use std::sync::Arc;

use ontology_core::command::{Command, CommandResult};
use ontology_core::error::Error;
use ontology_core::model::QueryResult;
use ontology_core::rdf::OntologyService;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler, ServiceExt};
use serde_json::{json, Value};

use crate::dispatch::{
    jobs_command, ontology_command, project_command, rdf_command, registry_command,
    semantic_command, DispatchContext, Resolver,
};
use crate::params::{JobsArgs, OntologyArgs, ProjectArgs, RdfArgs, RegistryArgs, SemanticArgs};
use crate::policy::Policy;

/// MCP server exposing the engine through `rdf`, `ontology`, `semantic`, and
/// `jobs`.
#[derive(Clone)]
pub struct McpServer {
    service: Arc<dyn OntologyService>,
    policy: Policy,
    resolver: Resolver,
}

#[tool_router]
impl McpServer {
    /// Creates a server over an execution backend.
    #[must_use]
    pub fn new(service: Arc<dyn OntologyService>, policy: Policy, resolver: Resolver) -> Self {
        Self {
            service,
            policy,
            resolver,
        }
    }

    /// Returns the active policy.
    #[must_use]
    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    /// RDF CRUD, statements, graphs, SPARQL, and bulk import/export.
    #[tool(
        name = "rdf",
        description = "Manage and query RDF data. Actions: get, search, list_instances, \
                       create, update, delete, query, sparql_update, import, export, inspect, \
                       list_graphs, statements, add_statements, delete_statements, clear_graph, \
                       extract_document.",
        annotations(
            title = "RDF data",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn rdf(
        &self,
        Parameters(args): Parameters<RdfArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let action = args.action;
        let command = rdf_command(action, &args.args, &self.context());
        Ok(self
            .invoke(
                "rdf",
                action.as_str(),
                args.args.target.as_deref(),
                args.args.confirm,
                args.args.dry_run,
                command,
            )
            .await)
    }

    /// OWL, SKOS, taxonomies, external ontologies, and mappings.
    #[tool(
        name = "ontology",
        description = "Manage OWL schemas, SKOS schemes and concepts, taxonomy hierarchies, \
                       external ontology imports, and class/property mappings. Actions: schema, \
                       validate, scheme, concept, taxonomy, search, external_import, \
                       external_sync, mapping, merge.",
        annotations(
            title = "Ontology structure",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn ontology(
        &self,
        Parameters(args): Parameters<OntologyArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let action = args.action;
        let command = ontology_command(action, &args.args, &self.context());
        Ok(self
            .invoke(
                "ontology",
                action.as_str(),
                args.args.target.as_deref(),
                args.args.confirm,
                args.args.dry_run,
                command,
            )
            .await)
    }

    /// Classification, embeddings, inference, enrichment, and explanations.
    #[tool(
        name = "semantic",
        description = "Classify instances, generate and search embeddings, preview or apply \
                       inference, explain results, review classifications, and reclassify. \
                       Actions: classify, classify_batch, embed, similarity, infer, preview, \
                       apply, explain, reclassify, reembed, classifications, embeddings, \
                       delete_embeddings, review, override.",
        annotations(
            title = "Semantic processing",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn semantic(
        &self,
        Parameters(args): Parameters<SemanticArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let action = args.action;
        let command = semantic_command(action, &args.args, &self.context());
        Ok(self
            .invoke(
                "semantic",
                action.as_str(),
                args.args.target.as_deref(),
                args.args.confirm,
                args.args.dry_run,
                command,
            )
            .await)
    }

    /// Semantic registry: catalogs, assets, ingestion, releases, and reports.
    #[tool(
        name = "registry",
        description = "Manage the semantic registry. Actions: catalog, asset, ingest, \
                       candidate, analyze, validate, publish, release, terms, imports, \
                       diff, reports, profile. Use operation=... for sub-operations.",
        annotations(
            title = "Semantic registry",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn registry(
        &self,
        Parameters(args): Parameters<RegistryArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let action = args.action;
        let command = registry_command(action, &args.args, &self.context());
        Ok(self
            .invoke(
                "registry",
                action.as_str(),
                args.args.target.as_deref(),
                args.args.confirm,
                args.args.dry_run,
                command,
            )
            .await)
    }

    /// Project management and deliverable acceptance.
    #[tool(
        name = "project",
        description = "Manage projects and semantic deliverables. Actions: project, goal, \
                       requirement, work, assign, transition, deliverable, accept, snapshot, \
                       overview, timeline. Use operation=... for sub-operations.",
        annotations(
            title = "Project management",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn project(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let action = args.action;
        let command = project_command(action, &args.args, &self.context());
        Ok(self
            .invoke(
                "project",
                action.as_str(),
                args.args.target.as_deref(),
                args.args.confirm,
                args.args.dry_run,
                command,
            )
            .await)
    }

    /// Submit, inspect, retrieve, cancel, and retry background jobs.
    #[tool(
        name = "jobs",
        description = "Manage background work. Actions: submit (classify_batch, \
                       reclassify_batch, embed_batch, infer_batch, rdf_import, rdf_export, \
                       external_import, external_sync), list, get, results, cancel, retry.",
        annotations(
            title = "Background jobs",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn jobs(
        &self,
        Parameters(args): Parameters<JobsArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let action = args.action;
        let command = jobs_command(action, &args.args, &self.context());
        Ok(self
            .invoke(
                "jobs",
                action.as_str(),
                args.args.target.as_deref(),
                args.args.confirm,
                args.args.dry_run,
                command,
            )
            .await)
    }

    fn context(&self) -> DispatchContext<'_> {
        DispatchContext {
            resolver: &self.resolver,
            max_results: self.policy.max_results,
            allow_write: self.policy.access == crate::policy::AccessMode::ReadWrite,
        }
    }

    async fn invoke(
        &self,
        tool: &str,
        action: &str,
        target: Option<&str>,
        confirm: bool,
        dry_run: bool,
        command: ontology_core::error::Result<Command>,
    ) -> CallToolResult {
        let command = match command {
            Ok(command) => command,
            Err(error) => {
                self.policy
                    .audit(tool, action, target, None, &format!("invalid: {error}"));
                return error_response(tool, action, target, &error);
            }
        };
        if let Err(error) = self.policy.authorize(&command, confirm) {
            self.policy.audit(
                tool,
                action,
                target,
                Some(&command),
                &format!("denied: {error}"),
            );
            return error_response(tool, action, target, &error);
        }
        if dry_run {
            self.policy
                .audit(tool, action, target, Some(&command), "dry_run");
            return success_response(json!({
                "success": true,
                "tool": tool,
                "action": action,
                "target": target,
                "dryRun": true,
                "command": command,
            }));
        }
        match self.service.execute(command.clone()).await {
            Ok(result) => {
                self.policy
                    .audit(tool, action, target, Some(&command), "ok");
                let (value, metadata) = compact(result, &self.policy);
                success_response(json!({
                    "success": true,
                    "tool": tool,
                    "action": action,
                    "target": target,
                    "result": value,
                    "metadata": metadata,
                }))
            }
            Err(error) => {
                self.policy.audit(
                    tool,
                    action,
                    target,
                    Some(&command),
                    &format!("error: {error}"),
                );
                error_response(tool, action, target, &error)
            }
        }
    }

    /// Serves MCP over stdio until the client disconnects.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the transport cannot start or finish.
    pub async fn serve_stdio(self) -> ontology_core::error::Result<()> {
        let running = self
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|error| Error::Io {
                detail: format!("MCP server failed: {error}"),
            })?;
        running.waiting().await.map_err(|error| Error::Io {
            detail: format!("MCP server stopped: {error}"),
        })?;
        Ok(())
    }
}

#[tool_handler(
    name = "onto-mcp",
    version = "0.1.0",
    instructions = "RDF ontology engine with four tools: rdf, ontology, semantic, and jobs. \
                    Start with read-only discovery (rdf query/inspect, ontology search, semantic \
                    preview), use dry_run to inspect writes, and submit long-running work as jobs. \
                    Write, destructive, and external operations may be disabled by server policy."
)]
impl ServerHandler for McpServer {}

fn success_response(value: Value) -> CallToolResult {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    let mut result = CallToolResult::structured(value);
    result.content = vec![ContentBlock::text(text)];
    result.is_error = Some(false);
    result
}

fn error_response(tool: &str, action: &str, target: Option<&str>, error: &Error) -> CallToolResult {
    let value = json!({
        "success": false,
        "tool": tool,
        "action": action,
        "target": target,
        "error": {
            "kind": error_kind(error),
            "message": error.to_string(),
        }
    });
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    let mut result = CallToolResult::structured_error(value);
    result.content = vec![ContentBlock::text(text)];
    result
}

fn error_kind(error: &Error) -> &'static str {
    match error {
        Error::InvalidIri { .. } => "invalid_iri",
        Error::UnknownPrefix { .. } => "unknown_prefix",
        Error::StoreOpen { .. } => "store_open",
        Error::Storage { .. } => "storage",
        Error::Parse { .. } => "parse",
        Error::Serialization { .. } => "serialization",
        Error::Query { .. } => "query",
        Error::Update { .. } => "update",
        Error::NotFound { .. } => "not_found",
        Error::Conflict { .. } => "conflict",
        Error::Validation { .. } => "validation",
        Error::Invalid { .. } => "invalid_request",
        Error::Config { .. } => "config",
        Error::Io { .. } => "io",
        Error::Unsupported { .. } => "unsupported",
        Error::GraphRegistry { .. } => "graph_registry",
        Error::GraphOpen { .. } => "graph_open",
        Error::GraphReadOnly { .. } => "graph_read_only",
        Error::GraphPartial { .. } => "graph_partial",
        Error::Extraction { .. } => "extraction",
        Error::Python { .. } => "python",
        Error::Registry { .. } => "registry",
        Error::Project { .. } => "project",
        Error::Limit { .. } => "limit",
    }
}

fn compact(result: CommandResult, policy: &Policy) -> (Value, Value) {
    let mut metadata = json!({});
    match result {
        CommandResult::Query(QueryResult::Solutions { variables, rows }) => {
            let total = rows.len();
            let rows: Vec<_> = rows.into_iter().take(policy.max_results).collect();
            let returned = rows.len();
            if total > returned {
                metadata["truncated"] = json!(true);
            }
            metadata["count"] = json!(total);
            metadata["returned"] = json!(returned);
            (
                json!({
                    "resultType": "solutions",
                    "variables": variables,
                    "rows": rows,
                }),
                metadata,
            )
        }
        CommandResult::Query(QueryResult::Boolean(value)) => {
            (json!({ "resultType": "boolean", "value": value }), metadata)
        }
        CommandResult::Query(QueryResult::Graph(quads)) => {
            let total = quads.len();
            let quads: Vec<_> = quads.into_iter().take(policy.max_results).collect();
            let returned = quads.len();
            if total > returned {
                metadata["truncated"] = json!(true);
            }
            metadata["count"] = json!(total);
            metadata["returned"] = json!(returned);
            (json!({ "resultType": "graph", "quads": quads }), metadata)
        }
        CommandResult::Collection(mut collection) => {
            let total = collection.count;
            if collection.items.len() > policy.max_results {
                collection.items.truncate(policy.max_results);
                metadata["truncated"] = json!(true);
            }
            let returned = collection.items.len();
            collection.count = returned;
            metadata["count"] = json!(total);
            metadata["returned"] = json!(returned);
            metadata["limit"] = json!(collection.limit);
            metadata["offset"] = json!(collection.offset);
            (
                serde_json::to_value(collection).unwrap_or(Value::Null),
                metadata,
            )
        }
        CommandResult::Export { format, data } => {
            let bytes = data.len();
            metadata["bytes"] = json!(bytes);
            if bytes > policy.max_export_bytes {
                let preview = truncate_chars(&data, policy.max_export_bytes);
                metadata["truncated"] = json!(true);
                metadata["previewBytes"] = json!(preview.len());
                (
                    json!({
                        "format": format,
                        "preview": preview,
                        "truncated": true,
                    }),
                    metadata,
                )
            } else {
                (json!({ "format": format, "data": data }), metadata)
            }
        }
        CommandResult::Job(status) => {
            metadata["job"] = json!(status.id);
            metadata["state"] = json!(status.state);
            metadata["progress"] = json!(status.progress);
            (
                serde_json::to_value(status).unwrap_or(Value::Null),
                metadata,
            )
        }
        CommandResult::Classifications(ref items) => {
            metadata["count"] = json!(items.len());
            (
                serde_json::to_value(result).unwrap_or(Value::Null),
                metadata,
            )
        }
        CommandResult::Embeddings(ref items) => {
            metadata["count"] = json!(items.len());
            (
                serde_json::to_value(result).unwrap_or(Value::Null),
                metadata,
            )
        }
        CommandResult::Similarity(ref items) => {
            metadata["count"] = json!(items.len());
            (
                serde_json::to_value(result).unwrap_or(Value::Null),
                metadata,
            )
        }
        other => (serde_json::to_value(other).unwrap_or(Value::Null), metadata),
    }
}

fn truncate_chars(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let end = value
        .char_indices()
        .take_while(|(index, _)| *index < max_bytes)
        .last()
        .map_or(0, |(index, character)| index + character.len_utf8());
    value[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ontology_core::command::CollectionResult;

    #[test]
    fn compact_collections_are_limited() {
        let policy = Policy {
            max_results: 2,
            ..Policy::default()
        };
        let result = CommandResult::Collection(CollectionResult {
            items: vec![json!(1), json!(2), json!(3)],
            count: 3,
            limit: 10,
            offset: 0,
        });
        let (value, metadata) = compact(result, &policy);
        assert_eq!(value["items"].as_array().unwrap().len(), 2);
        assert_eq!(metadata["count"], 3);
        assert_eq!(metadata["truncated"], true);
    }

    #[test]
    fn large_exports_become_previews() {
        let policy = Policy {
            max_export_bytes: 8,
            ..Policy::default()
        };
        let result = CommandResult::Export {
            format: "turtle".to_owned(),
            data: "0123456789abcdef".to_owned(),
        };
        let (value, metadata) = compact(result, &policy);
        assert_eq!(value["truncated"], true);
        assert_eq!(value["preview"], "01234567");
        assert_eq!(metadata["bytes"], 16);
    }

    #[test]
    fn truncation_respects_utf8_boundaries() {
        assert_eq!(truncate_chars("aébc", 3), "aé");
    }
}
