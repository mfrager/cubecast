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
pub mod document;
pub mod error;
pub mod external;
pub mod format;
pub mod graph;
pub mod id;
pub mod job;
pub mod model;
pub mod namespace;
pub mod owl;
pub mod project;
pub mod rdf;
pub mod registry;
pub mod registry_graph;
pub mod semantic;
pub mod skos;
pub mod tracing;

pub use command::{
    CollectionResult, Command, CommandResult, ExternalCommand, JobCommand, OntologyCommand,
    OperationReport, ProjectCommand, RdfCommand, RegistryCommand, ResourceResult, SemanticCommand,
    TaxonomyCommand,
};
pub use config::{
    Config, ExtractionConfig, GraphsConfig, GraphsSync, LimitsConfig, LoggingConfig, ProjectConfig,
    RegistryConfig, StorageBackend, StorageConfig, UpdatePolicy,
};
pub use document::CacheIdentity;
pub use document::{
    AssetRef, Author, Block, BlockKind, Caption, Citation, DocumentExtractor, DocumentMetadata,
    ExtractionContext, ExtractionMode, ExtractionOutput, ExtractionProvenance, ExtractionRequest,
    ExtractionWarning, Formula, FormulaType, Geometry, Line, ListBlock, Page, Reference,
    ResolvedWork, Section, Span, StructuredDocument, Table, TableCell, TableRow, TextFormat,
    TextOutput,
};
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
pub use project::{
    AssetSummary, Deliverable, DeliverableStatus, DependencyEdge, Goal, NewProject, Project,
    ProjectOverview, ProjectResult, ProjectSnapshot, ProjectStatus, ReleaseSummary, Requirement,
    SemanticAssetCatalog, TaskEvent, TimelineEntry, WorkItem, WorkKind, WorkStatus,
};
pub use rdf::{OntologyService, RdfStore, RdfTransaction};
pub use registry::{
    ArtifactRef, Asset, Catalog, DiffReport, Distribution, ImportDependency, IngestRequest,
    Profile, RegistryResult, Release, ReleaseCandidate, ReleaseStatistics, SearchHit, SearchKind,
    SearchResult, TermSummary,
};
pub use registry_graph::{
    analysis_graph, catalog_graph, is_dynamic_main_graph, is_project_graph,
    is_protected_registry_graph, is_registry_graph, project_core_graph, project_deliverables_graph,
    project_events_graph, project_goals_graph, project_governance_graph, project_plans_graph,
    project_requirements_graph, project_resources_graph, project_work_graph, release_graph,
    staging_graph, validation_graph,
};
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
