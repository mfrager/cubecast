//! Shared foundation for the ontology management system.
//!
//! This crate holds everything the other crates need without pulling in the
//! storage engine: identifiers, the application-level RDF data model, the
//! namespace registry, graph naming conventions, configuration, errors, and
//! the capability traits (`RdfStore`, `RdfTransaction`, `OntologyService`)
//! that every interface and implementation is written against.
//!
//! Oxigraph types never appear here. The storage adapter in `ontology-rdf`
//! translates between this model and OxRDF, so the domain stays independent of
//! the RDF engine.
//!
//! Serialization convention: all JSON uses snake_case field names and tagged
//! enums (`{"term": "iri", "value": "..."}`) so interfaces can share payloads.

pub mod command;
pub mod config;
pub mod error;
pub mod external;
pub mod format;
pub mod graph;
pub mod id;
pub mod job;
pub mod model;
pub mod namespace;
pub mod owl;
pub mod rdf;
pub mod semantic;
pub mod skos;
pub mod tracing;

pub use command::{
    CollectionResult, Command, CommandResult, ExternalCommand, JobCommand, OntologyCommand,
    OperationReport, RdfCommand, ResourceResult, SemanticCommand, TaxonomyCommand,
};
pub use config::{Config, LimitsConfig, LoggingConfig, StorageBackend, StorageConfig};
pub use error::{Error, Result};
pub use external::{
    ClassMapping, ExternalOntology, ExternalOntologyRegistry, ImportReport, ImportRequest,
    MappingIssue, MappingKind, MappingQuery, MappingReport, PropertyMapping,
};
pub use format::RdfFormat;
pub use graph::GraphKind;
pub use id::{Id, OperationId};
pub use job::{JobRequest, JobState, JobStatus};
pub use model::{
    GraphName, Instance, InstancePatch, InstanceQuery, InstanceSummary, Literal, NewInstance, Quad,
    QuadPattern, QueryResult, Relationship, RelationshipDirection, Statement, StatementId,
    StatementInput, StatementKind, Term,
};
pub use namespace::NamespaceRegistry;
pub use owl::{
    ClassHierarchy, ClassQuery, GraphScope, Ontology, OntologyManager, OntologyVersion, OwlClass,
    OwlProperty, PropertyCharacteristic, PropertyKind, PropertyQuery, Restriction, Severity,
    ValidationIssue, ValidationReport,
};
pub use rdf::{OntologyService, RdfStore, RdfTransaction};
pub use semantic::{
    ApplyPolicy, AttributeMatch, Classification, ClassificationBatch, ClassificationOptions,
    ClassificationRule, ClassificationStrategy, Classifier, Embedding, EmbeddingProvider,
    EmbeddingRecord, Explanation, InferenceEngine, InferenceMethod, InferenceOptions,
    InferencePreview, InferenceReport, InferredClass, InferredProperty, InferredStatement,
    Rejection, ReviewStatus, RuleOperator, SimilarityMetric, VectorFilter, VectorIndex,
    VectorMatch, VectorRecord,
};
pub use skos::{
    Concept, ConceptCollection, ConceptMatch, ConceptRelation, ConceptScheme, ConceptTree,
    LabelKind, LocalizedText, SchemeVersion, SkosRelation, TaxonomyIssue, TaxonomyManager,
    TaxonomyQuery, TaxonomyReport,
};
