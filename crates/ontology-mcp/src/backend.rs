//! Execution backends.
//!
//! The MCP server always talks to [`OntologyService`]. It can either hold a
//! local [`ontology_application::ApplicationService`] or forward commands to a
//! running HTTP API with [`HttpService`], which keeps the MCP process free of
//! direct database access when that is preferred.

use async_trait::async_trait;
use ontology_core::command::{Command, CommandResult};
use ontology_core::error::{Error, Result};
use ontology_core::rdf::OntologyService;

/// Executes commands against a remote `ontology-api` instance.
pub struct HttpService {
    base: String,
    client: reqwest::Client,
    api_key: Option<String>,
}

impl HttpService {
    /// Creates a client for the API rooted at `base`.
    #[must_use]
    pub fn new(base: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_owned(),
            client: reqwest::Client::new(),
            api_key,
        }
    }

    /// Returns the configured API base URL.
    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }

    fn url(&self) -> String {
        format!("{}/api/v1/commands", self.base)
    }
}

#[async_trait]
impl OntologyService for HttpService {
    async fn execute(&self, command: Command) -> Result<CommandResult> {
        let mut request = self.client.post(self.url()).json(&command);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        let response = request.send().await.map_err(|error| Error::Storage {
            detail: format!("MCP HTTP request failed: {error}"),
        })?;
        let status = response.status();
        let body = response.bytes().await.map_err(|error| Error::Storage {
            detail: format!("MCP HTTP response failed: {error}"),
        })?;

        if status.is_success() {
            return serde_json::from_slice(&body).map_err(Error::serialization);
        }

        let message = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| String::from_utf8_lossy(&body).into_owned());

        match status.as_u16() {
            404 => Err(Error::NotFound {
                kind: "remote resource",
                id: message,
            }),
            409 => Err(Error::Conflict {
                kind: "remote resource",
                id: message,
            }),
            501 => Err(Error::Unsupported { operation: message }),
            _ => Err(Error::Invalid {
                detail: format!("HTTP {status}: {message}"),
            }),
        }
    }
}
