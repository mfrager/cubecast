//! OpenAPI description of the HTTP interface.
//!
//! The document is assembled from the router's path list so clients and agents
//! can discover the API from a single endpoint.

use serde_json::{json, Value};

/// Builds the OpenAPI 3.0 document for the v1 API.
#[must_use]
pub fn document() -> Value {
    let paths = [
        ("/api/v1/system/health", &["get"][..]),
        ("/api/v1/system/status", &["get"][..]),
        ("/api/v1/openapi.json", &["get"][..]),
        ("/api/v1/config", &["get"][..]),
        ("/api/v1/commands", &["post"][..]),
        ("/api/v1/rdf", &["post"][..]),
        ("/api/v1/ontology", &["post"][..]),
        ("/api/v1/taxonomy", &["post"][..]),
        ("/api/v1/external", &["post"][..]),
        ("/api/v1/semantic", &["post"][..]),
        ("/api/v1/import", &["post"][..]),
        ("/api/v1/export", &["post"][..]),
        ("/api/v1/rdf/import", &["post"][..]),
        ("/api/v1/rdf/export", &["post"][..]),
        ("/api/v1/jobs", &["get", "post"][..]),
        ("/api/v1/jobs/{id}", &["get", "delete"][..]),
        ("/api/v1/jobs/{id}/results", &["get"][..]),
        ("/api/v1/jobs/{id}/cancel", &["post"][..]),
        ("/api/v1/jobs/{id}/retry", &["post"][..]),
        ("/api/v1/instances", &["get", "post"][..]),
        ("/api/v1/instances/{id}", &["get", "put", "delete"][..]),
        (
            "/api/v1/instances/{id}/statements",
            &["get", "post", "delete"][..],
        ),
        ("/api/v1/instances/{id}/relationships", &["get"][..]),
        ("/api/v1/graphs", &["get"][..]),
        ("/api/v1/sparql/query", &["post"][..]),
        ("/api/v1/sparql/update", &["post"][..]),
        ("/api/v1/classes", &["get", "post"][..]),
        ("/api/v1/classes/{id}", &["get", "put", "delete"][..]),
        ("/api/v1/classes/{id}/hierarchy", &["get"][..]),
        ("/api/v1/applicable-properties/{class}", &["get"][..]),
        ("/api/v1/applicable-classes", &["post"][..]),
        ("/api/v1/ontology/validate", &["post"][..]),
        ("/api/v1/properties", &["get", "post"][..]),
        ("/api/v1/properties/{id}", &["get", "put", "delete"][..]),
        ("/api/v1/ontologies", &["get", "post"][..]),
        ("/api/v1/ontologies/{id}", &["get", "delete"][..]),
        ("/api/v1/ontologies/{id}/versions", &["get"][..]),
        ("/api/v1/schemes", &["get", "post"][..]),
        ("/api/v1/schemes/{id}", &["get", "put", "delete"][..]),
        ("/api/v1/schemes/{id}/versions", &["get"][..]),
        ("/api/v1/concepts", &["get", "post"][..]),
        ("/api/v1/concepts/{id}", &["get", "put", "delete"][..]),
        ("/api/v1/concepts/{id}/relations", &["get"][..]),
        ("/api/v1/concepts/{id}/descendants", &["get"][..]),
        ("/api/v1/concepts/{id}/ancestors", &["get"][..]),
        ("/api/v1/concepts/{id}/move", &["post"][..]),
        ("/api/v1/concepts/search", &["get"][..]),
        ("/api/v1/taxonomies/{scheme}/tree", &["get"][..]),
        ("/api/v1/taxonomies/merge", &["post"][..]),
        ("/api/v1/taxonomies/validate", &["post"][..]),
        ("/api/v1/relations", &["post", "delete"][..]),
        ("/api/v1/collections", &["get", "post"][..]),
        ("/api/v1/collections/{id}", &["get", "delete"][..]),
        ("/api/v1/classifications/run", &["post"][..]),
        ("/api/v1/classifications/batch", &["post"][..]),
        ("/api/v1/classifications/{instance}", &["get"][..]),
        ("/api/v1/classifications/override", &["post"][..]),
        ("/api/v1/classifications/review", &["post"][..]),
        ("/api/v1/classifications/reclassify", &["post"][..]),
        ("/api/v1/embeddings/generate", &["post"][..]),
        ("/api/v1/embeddings/{instance}", &["get", "delete"][..]),
        ("/api/v1/similarity/search", &["post"][..]),
        ("/api/v1/inference/preview", &["post"][..]),
        ("/api/v1/inference/apply", &["post"][..]),
        ("/api/v1/inference/explain", &["post"][..]),
        ("/api/v1/external-ontologies", &["get", "post"][..]),
        ("/api/v1/external-ontologies/{id}", &["get", "delete"][..]),
        ("/api/v1/external-ontologies/{id}/sync", &["post"][..]),
        ("/api/v1/mappings/classes", &["get", "post"][..]),
        ("/api/v1/mappings/properties", &["get", "post"][..]),
        ("/api/v1/mappings/validate", &["post"][..]),
    ];

    let mut map = serde_json::Map::new();
    for (path, methods) in paths {
        let mut operations = serde_json::Map::new();
        for method in methods {
            operations.insert(
                (*method).to_owned(),
                json!({
                    "summary": format!("{method} {path}"),
                    "responses": {
                        "200": {
                            "description": "Command result",
                            "content": {
                                "application/json": {
                                    "schema": {"$ref": "#/components/schemas/CommandResult"}
                                }
                            }
                        },
                        "400": {"description": "Invalid request"},
                        "401": {"description": "Authentication required"},
                        "404": {"description": "Resource not found"},
                        "409": {"description": "Conflicting state"},
                        "500": {"description": "Storage failure"}
                    }
                }),
            );
        }
        map.insert(path.to_owned(), Value::Object(operations));
    }

    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Ontology Management System API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "RDF storage, OWL/SKOS management, classification, embeddings, inference, and background jobs."
        },
        "servers": [{"url": "/"}],
        "paths": Value::Object(map),
        "components": {
            "securitySchemes": {
                "bearerAuth": {"type": "http", "scheme": "bearer"}
            },
            "schemas": {
                "CommandResult": {
                    "type": "object",
                    "description": "Tagged application result; `type` identifies the payload."
                },
                "Error": {
                    "type": "object",
                    "properties": {
                        "error": {
                            "type": "object",
                            "properties": {
                                "kind": {"type": "string"},
                                "message": {"type": "string"}
                            }
                        }
                    }
                }
            }
        }
    })
}
