//! Shared fixtures for the Phase 1 integration tests.

use std::sync::Arc;

use ontology_core::graph::GraphKind;
use ontology_core::id::Id;
use ontology_core::model::{GraphName, Quad, Term};
use ontology_core::rdf::RdfStore;
use ontology_rdf::OxigraphStore;

/// Builds a valid example IRI.
///
/// # Panics
///
/// Panics only for malformed test input.
#[must_use]
pub fn ex(local: &str) -> Id {
    Id::new(format!("https://example.org/{local}")).expect("test IRIs are valid")
}

/// The default asserted graph.
#[must_use]
pub fn asserted_graph() -> GraphName {
    GraphName::Iri(GraphKind::Asserted.iri())
}

/// The inferred graph.
#[must_use]
pub fn inferred_graph() -> GraphName {
    GraphName::Iri(GraphKind::Inferred.iri())
}

/// An in-memory store.
///
/// # Panics
///
/// Panics only if the engine cannot allocate an in-memory store.
#[must_use]
pub fn memory_store() -> OxigraphStore {
    OxigraphStore::in_memory().expect("in-memory store")
}

/// An in-memory store behind the trait object used by application services.
#[must_use]
pub fn memory_store_handle() -> Arc<dyn RdfStore> {
    Arc::new(memory_store())
}

/// A quad with a string object in the asserted graph.
///
/// # Panics
///
/// Panics only for malformed test input.
#[must_use]
pub fn asserted_quad(subject: &str, predicate: &str, value: &str) -> Quad {
    Quad::in_graph(
        Term::Iri(ex(subject)),
        ex(predicate),
        Term::string(value),
        asserted_graph(),
    )
}

/// Turtle sample in the default graph.
#[must_use]
pub fn sample_turtle() -> &'static str {
    r#"
@prefix ex: <https://example.org/> .

ex:product-1 a ex:Product ;
    ex:name "Example Product" ;
    ex:description "A product used in tests" .

ex:product-2 a ex:Product ;
    ex:name "Second Product" .
"#
}

/// N-Quads sample using two named graphs.
#[must_use]
pub fn sample_nquads() -> &'static str {
    "<https://example.org/a> <https://example.org/name> \"A\" <urn:onto:data:asserted> .\n\
     <https://example.org/b> <https://example.org/name> \"B\" <urn:onto:data:asserted> .\n\
     <https://example.org/a> <https://example.org/derivedFrom> \"rule-1\" <urn:onto:data:inferred> .\n"
}
