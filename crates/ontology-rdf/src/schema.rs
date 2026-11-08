//! The internal OWL schema describing managed resources.

use ontology_core::error::Result;
use ontology_core::graph::GraphKind;
use ontology_core::model::{GraphName, QuadPattern};
use ontology_core::rdf::RdfStore;

use crate::format::RdfFormat;
use crate::store::OxigraphStore;

/// The internal schema, shipped with the binary.
pub const SYSTEM_SCHEMA_TURTLE: &str = include_str!("../../../ontologies/system-owl.ttl");

/// The graph that holds the internal schema.
#[must_use]
pub fn schema_graph() -> GraphName {
    GraphName::Iri(GraphKind::Schema.iri())
}

/// Whether the internal schema has already been loaded.
///
/// # Errors
///
/// Propagates storage failures.
pub fn is_initialized(store: &OxigraphStore) -> Result<bool> {
    Ok(!store
        .get(QuadPattern::new().with_graph(schema_graph()))?
        .is_empty())
}

/// Loads the internal schema into `system:schema` if it is not there yet.
///
/// Idempotent: a second call is a no-op returning `0`. Returns the number of
/// statements inserted.
///
/// # Errors
///
/// Propagates parse and storage failures.
pub fn bootstrap(store: &OxigraphStore) -> Result<u64> {
    if is_initialized(store)? {
        return Ok(0);
    }
    store.load_from_slice(
        RdfFormat::Turtle,
        SYSTEM_SCHEMA_TURTLE.as_bytes(),
        None,
        Some(&schema_graph()),
    )
}
