//! Server bootstrap shared by the `ontology-api` binary and `onto server`.

use std::sync::Arc;

use ontology_application::ApplicationService;
use ontology_core::config::Config;
use ontology_core::error::{Error, Result};
use ontology_graph::GraphBackend;
use tokio::net::TcpListener;

use crate::router::router;
use crate::state::AppState;

/// Runtime options for the HTTP server.
#[derive(Debug, Clone)]
pub struct ServeOptions {
    /// Application configuration.
    pub config: Config,
    /// Address to bind, e.g. `127.0.0.1:8080`.
    pub bind: String,
    /// Optional bearer token required from clients.
    pub api_key: Option<String>,
    /// Optional global requests-per-minute limit.
    pub rate_limit_per_minute: Option<u32>,
}

impl ServeOptions {
    /// Builds options from the environment.
    ///
    /// `ONTO_BIND` selects the listen address (default `127.0.0.1:8080`) and
    /// `ONTO_API_KEY` enables bearer authentication.
    #[must_use]
    pub fn from_env(config: Config) -> Self {
        let bind = std::env::var("ONTO_BIND")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "127.0.0.1:8080".to_owned());
        let api_key = std::env::var("ONTO_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let rate_limit_per_minute = std::env::var("ONTO_RATE_LIMIT").ok().and_then(|value| {
            match value.trim().parse::<u32>() {
                Ok(limit) if limit > 0 => Some(limit),
                _ => {
                    tracing::warn!(%value, "ignoring invalid ONTO_RATE_LIMIT");
                    None
                }
            }
        });
        Self {
            config,
            bind,
            api_key,
            rate_limit_per_minute,
        }
    }
}

/// Opens the configured store and serves the API until shutdown.
///
/// # Errors
///
/// Propagates storage, configuration, and listener failures.
pub async fn serve(options: ServeOptions) -> Result<()> {
    options.config.validate()?;
    let backend = GraphBackend::from_config(&options.config)?;
    let namespaces = options.config.namespace_registry()?;
    let service = Arc::new(
        ApplicationService::new(backend.rdf_store())
            .with_namespaces(namespaces)
            .with_configured_documents(&options.config)?,
    );
    let state =
        AppState::new(service, options.api_key).with_rate_limit(options.rate_limit_per_minute);
    let app = router(state);

    let listener = TcpListener::bind(&options.bind)
        .await
        .map_err(|error| Error::Io {
            detail: format!("could not bind {}: {error}", options.bind),
        })?;
    tracing::info!(address = %options.bind, "HTTP API listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|error| Error::Io {
            detail: format!("HTTP server failed: {error}"),
        })
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
