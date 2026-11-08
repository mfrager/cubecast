//! External ontology registry types and mapping records.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::id::{Id, OperationId};
use crate::model::GraphName;
use crate::owl::Severity;

/// A registered external ontology.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalOntology {
    /// Ontology IRI, preserved exactly as published.
    pub id: Id,
    /// Where the ontology was imported from (path or URI).
    pub source: String,
    /// Graph holding the imported statements.
    pub graph: GraphName,
    /// `rdfs:label` / `dcterms:title`, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// `owl:versionInfo`, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Discovered namespace of the ontology's vocabulary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Syntax used for the import.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Import timestamp.
    pub imported_at: DateTime<Utc>,
    /// Number of imported statements.
    pub quad_count: usize,
    /// Number of `owl:Class` declarations.
    pub class_count: usize,
    /// Number of property declarations.
    pub property_count: usize,
}

/// An import request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportRequest {
    /// Path or URI of the document.
    pub source: String,
    /// Optional ontology IRI override; by default `owl:Ontology` is discovered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ontology: Option<Id>,
    /// Syntax name (`turtle`, `rdfxml`, …); by default inferred from the path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Target graph; by default a dedicated graph per ontology is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<GraphName>,
}

impl ImportRequest {
    /// Starts an import request for `source`.
    #[must_use]
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            ontology: None,
            format: None,
            graph: None,
        }
    }

    /// Sets the format.
    #[must_use]
    pub fn with_format(mut self, format: impl Into<String>) -> Self {
        self.format = Some(format.into());
        self
    }

    /// Overrides the ontology IRI.
    #[must_use]
    pub fn with_ontology(mut self, ontology: Id) -> Self {
        self.ontology = Some(ontology);
        self
    }

    /// Sets the target graph.
    #[must_use]
    pub fn with_graph(mut self, graph: GraphName) -> Self {
        self.graph = Some(graph);
        self
    }
}

/// Outcome of an import or synchronization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReport {
    /// Imported ontology IRI.
    pub ontology: Id,
    /// Graph holding the imported statements.
    pub graph: GraphName,
    /// Statements written.
    pub inserted: u64,
    /// Classes discovered.
    pub class_count: usize,
    /// Properties discovered.
    pub property_count: usize,
    /// Whether the ontology was already registered before this import.
    pub already_present: bool,
}

/// How an external class or property maps to an internal one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MappingKind {
    /// Classes denote the same thing.
    EquivalentClass,
    /// The external class is a superclass of the internal one (or vice versa by convention).
    SubclassOf,
    /// Properties denote the same thing.
    EquivalentProperty,
    /// The property needs a value transformation.
    PropertyTransformation,
}

impl MappingKind {
    /// Stable storage name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EquivalentClass => "equivalent_class",
            Self::SubclassOf => "subclass_of",
            Self::EquivalentProperty => "equivalent_property",
            Self::PropertyTransformation => "property_transformation",
        }
    }

    /// Parses a storage name.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "equivalent_class" => Some(Self::EquivalentClass),
            "subclass_of" => Some(Self::SubclassOf),
            "equivalent_property" => Some(Self::EquivalentProperty),
            "property_transformation" => Some(Self::PropertyTransformation),
            _ => None,
        }
    }
}

/// An explicit external-to-internal class mapping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassMapping {
    /// Mapping record IRI.
    pub id: Id,
    /// External class IRI.
    pub external: Id,
    /// Internal class IRI.
    pub internal: Id,
    /// Mapping semantics.
    pub kind: MappingKind,
    /// Confidence in `0.0..=1.0`, when recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// When the mapping was recorded.
    pub recorded_at: DateTime<Utc>,
}

impl ClassMapping {
    /// Creates a mapping record.
    ///
    /// # Errors
    ///
    /// Returns an error when the generated mapping IRI is invalid.
    pub fn new(external: Id, internal: Id, kind: MappingKind) -> Result<Self> {
        Ok(Self {
            id: Id::new(format!(
                "urn:onto:mapping:class:{}",
                OperationId::generate()
            ))?,
            external,
            internal,
            kind,
            confidence: None,
            recorded_at: Utc::now(),
        })
    }

    /// Records a confidence value.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] when the value is outside `0.0..=1.0`.
    pub fn with_confidence(mut self, confidence: f64) -> Result<Self> {
        if !(0.0..=1.0).contains(&confidence) {
            return Err(Error::validation("confidence must be between 0.0 and 1.0"));
        }
        self.confidence = Some(confidence);
        Ok(self)
    }
}

/// An explicit external-to-internal property mapping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyMapping {
    /// Mapping record IRI.
    pub id: Id,
    /// External property IRI.
    pub external: Id,
    /// Internal property IRI.
    pub internal: Id,
    /// Mapping semantics.
    pub kind: MappingKind,
    /// Confidence in `0.0..=1.0`, when recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// When the mapping was recorded.
    pub recorded_at: DateTime<Utc>,
}

impl PropertyMapping {
    /// Creates a mapping record.
    ///
    /// # Errors
    ///
    /// Returns an error when the generated mapping IRI is invalid.
    pub fn new(external: Id, internal: Id, kind: MappingKind) -> Result<Self> {
        Ok(Self {
            id: Id::new(format!(
                "urn:onto:mapping:property:{}",
                OperationId::generate()
            ))?,
            external,
            internal,
            kind,
            confidence: None,
            recorded_at: Utc::now(),
        })
    }

    /// Records a confidence value.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] when the value is outside `0.0..=1.0`.
    pub fn with_confidence(mut self, confidence: f64) -> Result<Self> {
        if !(0.0..=1.0).contains(&confidence) {
            return Err(Error::validation("confidence must be between 0.0 and 1.0"));
        }
        self.confidence = Some(confidence);
        Ok(self)
    }
}

/// Filter and pagination options for mapping listings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MappingQuery {
    /// Restrict by external IRI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<Id>,
    /// Restrict by internal IRI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal: Option<Id>,
    /// Maximum results.
    pub limit: usize,
    /// Results to skip.
    pub offset: usize,
}

impl Default for MappingQuery {
    fn default() -> Self {
        Self {
            external: None,
            internal: None,
            limit: 100,
            offset: 0,
        }
    }
}

impl MappingQuery {
    /// Restricts by external IRI.
    #[must_use]
    pub fn with_external(mut self, external: Id) -> Self {
        self.external = Some(external);
        self
    }

    /// Restricts by internal IRI.
    #[must_use]
    pub fn with_internal(mut self, internal: Id) -> Self {
        self.internal = Some(internal);
        self
    }
}

/// One mapping validation finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MappingIssue {
    /// Mapping record.
    pub mapping: Id,
    /// Error or warning.
    pub severity: Severity,
    /// Human-readable explanation.
    pub message: String,
}

/// Result of validating the mapping registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MappingReport {
    /// Number of mappings checked.
    pub checked: usize,
    /// Findings.
    pub issues: Vec<MappingIssue>,
}

impl MappingReport {
    /// Whether no error-severity issue was found.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self
            .issues
            .iter()
            .any(|issue| issue.severity == Severity::Error)
    }
}

/// External ontology registry and mapping capability.
#[async_trait::async_trait]
pub trait ExternalOntologyRegistry: Send + Sync {
    /// Imports an ontology document from a file path or URI.
    async fn import(&self, request: ImportRequest) -> Result<ImportReport>;
    /// Imports an ontology document from memory.
    async fn import_from_slice(&self, request: ImportRequest, data: &[u8]) -> Result<ImportReport>;
    /// Re-imports a registered ontology from its recorded source.
    async fn sync(&self, id: &Id) -> Result<ImportReport>;
    /// Loads a registry entry.
    async fn get(&self, id: &Id) -> Result<ExternalOntology>;
    /// Lists registry entries.
    async fn list(&self) -> Result<Vec<ExternalOntology>>;
    /// Removes an ontology's statements and registry entry.
    async fn delete(&self, id: &Id) -> Result<usize>;
    /// Records a class mapping.
    async fn map_class(&self, mapping: ClassMapping) -> Result<Id>;
    /// Records a property mapping.
    async fn map_property(&self, mapping: PropertyMapping) -> Result<Id>;
    /// Lists class mappings.
    async fn list_class_mappings(&self, query: MappingQuery) -> Result<Vec<ClassMapping>>;
    /// Lists property mappings.
    async fn list_property_mappings(&self, query: MappingQuery) -> Result<Vec<PropertyMapping>>;
    /// Validates that mapped external and internal terms are declared.
    async fn validate_mappings(&self) -> Result<MappingReport>;
}
