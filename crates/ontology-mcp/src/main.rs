//! `ontology-mcp` stdio server binary.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use ontology_application::ApplicationService;
use ontology_core::config::Config;
use ontology_core::rdf::OntologyService;
use ontology_core::tracing::init_tracing;
use ontology_graph::GraphBackend;
use ontology_mcp::{HttpService, McpServer, Policy, Resolver};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("onto-mcp: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = std::env::var("ONTO_CONFIG").ok().map(PathBuf::from);
    let config = Config::load(config_path.as_deref())?;
    config.validate()?;
    init_tracing(&config.logging);
    let policy = Policy::from_env()?;
    let namespaces = config.namespace_registry()?;
    let resolver = Resolver::new(namespaces.clone());

    let service: Arc<dyn OntologyService> = if let Ok(base) = std::env::var("ONTO_MCP_SERVER") {
        if base.trim().is_empty() {
            return Err("ONTO_MCP_SERVER must not be empty".into());
        }
        let api_key = std::env::var("ONTO_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty());
        tracing::info!(%base, "MCP forwarding commands to HTTP API");
        Arc::new(HttpService::new(base, api_key))
    } else {
        let backend = GraphBackend::from_config(&config)?;
        tracing::info!("MCP using local RDF store");
        Arc::new(
            ApplicationService::new(backend.rdf_store())
                .with_namespaces(namespaces)
                .with_configured_documents(&config)?,
        )
    };

    tracing::info!(
        access = ?policy.access,
        destructive = policy.allow_destructive,
        external = policy.allow_external,
        "starting MCP server on stdio"
    );
    McpServer::new(service, policy, resolver)
        .serve_stdio()
        .await?;
    Ok(())
}
