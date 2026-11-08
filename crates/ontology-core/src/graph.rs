//! Graph naming conventions.
//!
//! The system separates classes of information into reserved named graphs so
//! asserted data is never silently overwritten by inferred or derived data.

use crate::error::{Error, Result};
use crate::id::Id;
use crate::namespace::{DATA, SYSTEM};

/// The reserved graph families defined by the system design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GraphKind {
    /// Internal OWL schema describing managed resources.
    Schema,
    /// Managed SKOS ConceptSchemes.
    Taxonomy,
    /// Imported external ontologies.
    External,
    /// Explicit instance statements.
    Asserted,
    /// Automatically inferred statements.
    Inferred,
    /// Taxonomy classification results.
    Classification,
    /// Source and derivation metadata.
    Provenance,
}

impl GraphKind {
    /// Every reserved graph, in design-document order.
    pub const ALL: [Self; 7] = [
        Self::Schema,
        Self::Taxonomy,
        Self::External,
        Self::Asserted,
        Self::Inferred,
        Self::Classification,
        Self::Provenance,
    ];

    /// The local graph name (`schema`, `asserted`, …).
    #[must_use]
    pub const fn local_name(self) -> &'static str {
        match self {
            Self::Schema => "schema",
            Self::Taxonomy => "taxonomy",
            Self::External => "external",
            Self::Asserted => "asserted",
            Self::Inferred => "inferred",
            Self::Classification => "classification",
            Self::Provenance => "provenance",
        }
    }

    /// The absolute graph IRI (`urn:onto:system:schema`, `urn:onto:data:asserted`, …).
    #[must_use]
    pub const fn iri_str(self) -> &'static str {
        match self {
            Self::Schema => "urn:onto:system:schema",
            Self::Taxonomy => "urn:onto:system:taxonomy",
            Self::External => "urn:onto:system:external",
            Self::Asserted => "urn:onto:data:asserted",
            Self::Inferred => "urn:onto:data:inferred",
            Self::Classification => "urn:onto:data:classification",
            Self::Provenance => "urn:onto:data:provenance",
        }
    }

    /// Returns the graph IRI as an [`Id`].
    ///
    /// # Panics
    ///
    /// Panics only if a constant in this module is not a valid IRI; a unit test
    /// covers every variant.
    #[must_use]
    pub fn iri(self) -> Id {
        Id::new(self.iri_str()).expect("reserved graph IRIs are valid")
    }

    /// Looks up the kind from its graph IRI.
    #[must_use]
    pub fn from_iri_str(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.iri_str() == value)
    }

    /// Looks up the kind from a graph identifier.
    #[must_use]
    pub fn from_iri(value: &Id) -> Option<Self> {
        Self::from_iri_str(value.as_str())
    }

    /// Whether the graph belongs to the `system:` namespace.
    #[must_use]
    pub fn is_system(self) -> bool {
        self.iri_str().starts_with(SYSTEM)
    }

    /// Whether the graph belongs to the `data:` namespace.
    #[must_use]
    pub fn is_data(self) -> bool {
        self.iri_str().starts_with(DATA)
    }
}

/// Returns the isolated graph IRI for a tenant/workspace/project.
///
/// The reserved graphs keep their names; tenant graphs are placed under
/// `urn:onto:tenant:<tenant>:<kind>`, so one database can host several isolated
/// projects without changing the core conventions.
///
/// # Errors
///
/// Returns [`Error::Validation`] when the tenant name is empty or contains
/// characters that would make the IRI ambiguous.
pub fn tenant_graph(tenant: &str, kind: GraphKind) -> Result<Id> {
    let tenant = tenant.trim();
    if tenant.is_empty() {
        return Err(Error::validation("tenant name must not be empty"));
    }
    if !tenant
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Error::validation(format!(
            "tenant name {tenant:?} must be alphanumeric, '-' or '_'"
        )));
    }
    Id::new(format!("urn:onto:tenant:{tenant}:{}", kind.local_name()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reserved_graph_is_a_valid_iri() {
        for kind in GraphKind::ALL {
            let iri = kind.iri();
            assert_eq!(GraphKind::from_iri(&iri), Some(kind));
            assert!(iri.as_str().starts_with("urn:onto:"));
        }
    }

    #[test]
    fn design_graph_names_are_preserved() {
        assert_eq!(GraphKind::Schema.iri_str(), "urn:onto:system:schema");
        assert_eq!(GraphKind::Asserted.iri_str(), "urn:onto:data:asserted");
        assert_eq!(
            GraphKind::Classification.iri_str(),
            "urn:onto:data:classification"
        );
    }

    #[test]
    fn builds_tenant_graphs() {
        let graph = tenant_graph("acme", GraphKind::Asserted).unwrap();
        assert_eq!(graph.as_str(), "urn:onto:tenant:acme:asserted");
        assert!(tenant_graph("", GraphKind::Asserted).is_err());
        assert!(tenant_graph("a b", GraphKind::Asserted).is_err());
    }
}
