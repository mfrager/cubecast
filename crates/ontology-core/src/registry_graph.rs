//! Graph names for registry metadata and project management data.
//!
//! Registry metadata and project data are main-owned. Release and analysis
//! graphs are created at runtime under a dynamic prefix, so a multi-graph
//! router must send an unknown graph in these namespaces to the main store.

use crate::error::{Error, Result};
use crate::id::Id;
use crate::model::GraphName;

/// Registry namespace.
pub const REGISTRY_NAMESPACE: &str = "urn:onto:registry:";
/// Project namespace.
pub const PROJECT_NAMESPACE: &str = "urn:onto:project:";
/// Default catalog graph.
pub const CATALOG_GRAPH_IRI: &str = "urn:onto:registry:catalog";
/// Registry profile graph.
pub const PROFILE_GRAPH_IRI: &str = "urn:onto:registry:profiles";
/// Registry provenance graph.
pub const PROVENANCE_GRAPH_IRI: &str = "urn:onto:registry:provenance";
/// Project core graph.
pub const PROJECT_CORE_GRAPH_IRI: &str = "urn:onto:project:core";
/// Project goals graph.
pub const PROJECT_GOALS_GRAPH_IRI: &str = "urn:onto:project:goals";
/// Project requirements graph.
pub const PROJECT_REQUIREMENTS_GRAPH_IRI: &str = "urn:onto:project:requirements";
/// Project work graph.
pub const PROJECT_WORK_GRAPH_IRI: &str = "urn:onto:project:work";
/// Project deliverables graph.
pub const PROJECT_DELIVERABLES_GRAPH_IRI: &str = "urn:onto:project:deliverables";
/// Project resources graph.
pub const PROJECT_RESOURCES_GRAPH_IRI: &str = "urn:onto:project:resources";
/// Project governance graph.
pub const PROJECT_GOVERNANCE_GRAPH_IRI: &str = "urn:onto:project:governance";
/// Project plans graph.
pub const PROJECT_PLANS_GRAPH_IRI: &str = "urn:onto:project:plans";
/// Project events graph.
pub const PROJECT_EVENTS_GRAPH_IRI: &str = "urn:onto:project:events";

fn named(iri: &str) -> GraphName {
    GraphName::Iri(Id::new(iri).expect("registry graph constants are valid IRIs"))
}

/// Returns the default catalog graph.
#[must_use]
pub fn catalog_graph() -> GraphName {
    named(CATALOG_GRAPH_IRI)
}

/// Returns a release graph from a path-safe key.
///
/// # Errors
///
/// Returns [`Error::Validation`] when the key is empty or contains path
/// separators, fragment markers, or query markers.
pub fn release_graph(key: &str) -> Result<GraphName> {
    validate_key(key)?;
    Ok(named(&format!("{REGISTRY_NAMESPACE}release:{key}")))
}

/// Returns an analysis graph from a path-safe key.
///
/// # Errors
///
/// Returns [`Error::Validation`] for a malformed key.
pub fn analysis_graph(key: &str) -> Result<GraphName> {
    validate_key(key)?;
    Ok(named(&format!("{REGISTRY_NAMESPACE}analysis:{key}")))
}

/// Returns a validation report graph from a run identifier.
///
/// # Errors
///
/// Returns [`Error::Validation`] for a malformed identifier.
pub fn validation_graph(run: &str) -> Result<GraphName> {
    validate_key(run)?;
    Ok(named(&format!("{REGISTRY_NAMESPACE}validation:{run}")))
}

/// Returns a staging graph for a candidate identifier.
///
/// # Errors
///
/// Returns [`Error::Validation`] for a malformed identifier.
pub fn staging_graph(candidate: &str) -> Result<GraphName> {
    validate_key(candidate)?;
    Ok(named(&format!("{REGISTRY_NAMESPACE}staging:{candidate}")))
}

/// Returns the project core graph.
#[must_use]
pub fn project_core_graph() -> GraphName {
    named(PROJECT_CORE_GRAPH_IRI)
}

/// Returns the project goals graph.
#[must_use]
pub fn project_goals_graph() -> GraphName {
    named(PROJECT_GOALS_GRAPH_IRI)
}

/// Returns the project requirements graph.
#[must_use]
pub fn project_requirements_graph() -> GraphName {
    named(PROJECT_REQUIREMENTS_GRAPH_IRI)
}

/// Returns the project work graph.
#[must_use]
pub fn project_work_graph() -> GraphName {
    named(PROJECT_WORK_GRAPH_IRI)
}

/// Returns the project deliverables graph.
#[must_use]
pub fn project_deliverables_graph() -> GraphName {
    named(PROJECT_DELIVERABLES_GRAPH_IRI)
}

/// Returns the project resources graph.
#[must_use]
pub fn project_resources_graph() -> GraphName {
    named(PROJECT_RESOURCES_GRAPH_IRI)
}

/// Returns the project governance graph.
#[must_use]
pub fn project_governance_graph() -> GraphName {
    named(PROJECT_GOVERNANCE_GRAPH_IRI)
}

/// Returns the project plans graph.
#[must_use]
pub fn project_plans_graph() -> GraphName {
    named(PROJECT_PLANS_GRAPH_IRI)
}

/// Returns the project events graph.
#[must_use]
pub fn project_events_graph() -> GraphName {
    named(PROJECT_EVENTS_GRAPH_IRI)
}

/// Returns whether `iri` is a registry graph.
#[must_use]
pub fn is_registry_graph(iri: &Id) -> bool {
    iri.as_str().starts_with(REGISTRY_NAMESPACE)
}

/// Returns whether `iri` is a project graph.
#[must_use]
pub fn is_project_graph(iri: &Id) -> bool {
    iri.as_str().starts_with(PROJECT_NAMESPACE)
}

/// Returns whether a graph is immutable from the public SPARQL update path.
#[must_use]
pub fn is_protected_registry_graph(iri: &Id) -> bool {
    let value = iri.as_str();
    value == PROVENANCE_GRAPH_IRI
        || value.starts_with("urn:onto:registry:release:")
        || value.starts_with("urn:onto:registry:analysis:")
        || value.starts_with("urn:onto:registry:validation:")
}

/// Returns whether a graph is main-owned under a dynamic prefix.
#[must_use]
pub fn is_dynamic_main_graph(iri: &Id) -> bool {
    is_registry_graph(iri) || is_project_graph(iri)
}

fn validate_key(key: &str) -> Result<()> {
    if key.is_empty()
        || !key.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Err(Error::validation(format!(
            "graph key {key:?} must be non-empty and contain only ASCII alphanumerics, '-' or '_'"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_release_and_project_graphs() {
        assert_eq!(
            release_graph("4f2a9c1d7e803b66")
                .expect("release graph")
                .as_iri()
                .expect("named graph")
                .as_str(),
            "urn:onto:registry:release:4f2a9c1d7e803b66"
        );
        assert_eq!(
            project_work_graph().as_iri().expect("named graph").as_str(),
            "urn:onto:project:work"
        );
    }

    #[test]
    fn rejects_path_unsafe_keys() {
        for key in ["", "../x", "a/b", "a#b", "a?b"] {
            assert!(release_graph(key).is_err(), "{key} should be rejected");
        }
    }

    #[test]
    fn release_and_analysis_graphs_are_protected() {
        let release = release_graph("abc").expect("release");
        assert!(is_protected_registry_graph(
            release.as_iri().expect("named")
        ));
        assert!(is_dynamic_main_graph(release.as_iri().expect("named")));
    }
}
