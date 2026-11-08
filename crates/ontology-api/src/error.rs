//! Translation from domain errors to HTTP responses.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use ontology_core::error::Error;
use serde_json::json;

/// Wrapper that renders a domain error as a JSON HTTP response.
pub struct ApiError(pub Error);

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        Self(error)
    }
}

fn status_for(error: &Error) -> StatusCode {
    match error {
        Error::InvalidIri { .. }
        | Error::UnknownPrefix { .. }
        | Error::Invalid { .. }
        | Error::Validation { .. }
        | Error::Parse { .. }
        | Error::Query { .. }
        | Error::Update { .. }
        | Error::Config { .. }
        | Error::GraphRegistry { .. } => StatusCode::BAD_REQUEST,
        Error::NotFound { .. } => StatusCode::NOT_FOUND,
        Error::Conflict { .. } => StatusCode::CONFLICT,
        Error::GraphReadOnly { .. } => StatusCode::FORBIDDEN,
        Error::Unsupported { .. } => StatusCode::NOT_IMPLEMENTED,
        Error::StoreOpen { .. }
        | Error::Storage { .. }
        | Error::Serialization { .. }
        | Error::GraphOpen { .. }
        | Error::GraphPartial { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        Error::Io { .. } => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn kind_for(error: &Error) -> &'static str {
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
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = status_for(&self.0);
        let body = Json(json!({
            "error": {
                "kind": kind_for(&self.0),
                "message": self.0.to_string(),
            }
        }));
        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_domain_errors_to_http_statuses() {
        assert_eq!(
            status_for(&Error::validation("bad")),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            status_for(&Error::NotFound {
                kind: "instance",
                id: "urn:example:missing".to_owned(),
            }),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status_for(&Error::Unsupported {
                operation: "remote".to_owned(),
            }),
            StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            status_for(&Error::Storage {
                detail: "disk".to_owned(),
            }),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn maps_errors_to_stable_kinds() {
        assert_eq!(kind_for(&Error::validation("bad")), "validation");
        assert_eq!(
            kind_for(&Error::Conflict {
                kind: "instance",
                id: "urn:example:x".to_owned(),
            }),
            "conflict"
        );
        assert_eq!(
            kind_for(&Error::Invalid {
                detail: "request".to_owned(),
            }),
            "invalid_request"
        );
    }

    #[test]
    fn invalid_requests_are_bad_requests() {
        assert_eq!(
            status_for(&Error::Invalid {
                detail: "request".to_owned(),
            }),
            StatusCode::BAD_REQUEST
        );
    }
}
