//! Shared command and result model.
//!
//! Interfaces translate their transport-specific input into these commands and
//! render the results back. Phase 1 introduced the RDF family, and Phase 2 adds
//! the OWL, SKOS taxonomy, and external ontology families. Semantic and job
//! families are added by their phases.

use serde::{Deserialize, Serialize};

use crate::document::{ExtractionOutput, ExtractionRequest};
use crate::external::{ClassMapping, ImportRequest, MappingQuery, MappingReport, PropertyMapping};
use crate::id::Id;
use crate::job::{JobRequest, JobStatus};
use crate::model::{
    GraphName, InstancePatch, InstanceQuery, NewInstance, QueryResult, StatementId, StatementInput,
};
use crate::owl::{
    ClassQuery, GraphScope, Ontology, OwlClass, OwlProperty, PropertyQuery, ValidationReport,
};
use crate::semantic::{
    ApplyPolicy, Classification, ClassificationOptions, EmbeddingRecord, InferenceOptions,
    InferencePreview, InferenceReport, VectorFilter, VectorMatch,
};
use crate::skos::{
    Concept, ConceptCollection, ConceptScheme, SkosRelation, TaxonomyQuery, TaxonomyReport,
};

/// A command accepted by [`crate::rdf::OntologyService`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "family", content = "command", rename_all = "camelCase")]
pub enum Command {
    /// RDF dataset, instance, and statement operations.
    Rdf(RdfCommand),
    /// OWL schema and ontology operations.
    Ontology(OntologyCommand),
    /// SKOS scheme, concept, and taxonomy operations.
    Taxonomy(TaxonomyCommand),
    /// External ontology import and mapping operations.
    External(ExternalCommand),
    /// Classification, embedding, inference, and enrichment operations.
    Semantic(SemanticCommand),
    /// Background job submission and monitoring.
    Job(JobCommand),
    /// Document extraction through MinerU.
    Document(DocumentCommand),
}

/// Document extraction operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum DocumentCommand {
    /// Extracts one document.
    Extract(ExtractionRequest),
    /// Loads the latest stored extraction for a document id.
    Get {
        /// `urn:sha256:<hex>` document identifier.
        id: String,
    },
}

/// RDF operations available in Phase 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum RdfCommand {
    /// Creates a new instance.
    CreateInstance(NewInstance),
    /// Retrieves one instance.
    GetInstance {
        /// Instance IRI.
        id: Id,
        /// Restrict to a graph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        graph: Option<GraphName>,
    },
    /// Lists instances.
    ListInstances(InstanceQuery),
    /// Applies a partial update.
    UpdateInstance {
        /// Instance IRI.
        id: Id,
        /// Changes to apply.
        patch: InstancePatch,
        /// Restrict to a graph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        graph: Option<GraphName>,
        /// Optional provenance source.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    /// Deletes an instance and all of its statements.
    DeleteInstance {
        /// Instance IRI.
        id: Id,
        /// Restrict to a graph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        graph: Option<GraphName>,
    },
    /// Lists an instance's statements.
    ListStatements {
        /// Instance IRI.
        id: Id,
        /// Restrict to a graph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        graph: Option<GraphName>,
    },
    /// Adds statements to an instance in one batch.
    AddStatements {
        /// Instance IRI.
        id: Id,
        /// Statements to add.
        statements: Vec<StatementInput>,
        /// Target graph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        graph: Option<GraphName>,
        /// Optional provenance source.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    /// Deletes statements from an instance in one batch.
    DeleteStatements {
        /// Instance IRI.
        id: Id,
        /// Statement identifiers.
        statement_ids: Vec<StatementId>,
        /// Restrict to a graph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        graph: Option<GraphName>,
    },
    /// Lists incoming and outgoing relationships.
    GetRelationships {
        /// Resource IRI.
        id: Id,
    },
    /// Evaluates a SPARQL query.
    Query {
        /// SPARQL query text.
        sparql: String,
    },
    /// Evaluates a SPARQL update.
    Update {
        /// SPARQL update text.
        sparql: String,
    },
    /// Lists named graphs.
    ListGraphs,
    /// Parses RDF into the store from an inline document.
    Import {
        /// RDF syntax name.
        format: String,
        /// Document text.
        data: String,
        /// Optional target graph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        graph: Option<GraphName>,
    },
    /// Serializes the dataset or one graph.
    Export {
        /// RDF syntax name.
        format: String,
        /// Restrict to a graph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        graph: Option<GraphName>,
    },
    /// Removes every quad in a graph.
    ClearGraph {
        /// Graph to clear.
        graph: GraphName,
    },
}

/// OWL schema operations available in Phase 2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum OntologyCommand {
    /// Creates a class.
    CreateClass(OwlClass),
    /// Retrieves a class.
    GetClass {
        /// Class IRI.
        id: Id,
    },
    /// Lists classes.
    ListClasses(ClassQuery),
    /// Replaces a class definition.
    UpdateClass(OwlClass),
    /// Deletes a class definition.
    DeleteClass {
        /// Class IRI.
        id: Id,
    },
    /// Creates a property.
    CreateProperty(OwlProperty),
    /// Retrieves a property.
    GetProperty {
        /// Property IRI.
        id: Id,
    },
    /// Lists properties.
    ListProperties(PropertyQuery),
    /// Replaces a property definition.
    UpdateProperty(OwlProperty),
    /// Deletes a property definition.
    DeleteProperty {
        /// Property IRI.
        id: Id,
    },
    /// Returns the subclass hierarchy around a class.
    ClassHierarchy {
        /// Class IRI.
        id: Id,
    },
    /// Validates a schema scope.
    Validate {
        /// Scope to validate.
        scope: GraphScope,
    },
    /// Registers an ontology.
    CreateOntology(Ontology),
    /// Retrieves an ontology.
    GetOntology {
        /// Ontology IRI.
        id: Id,
    },
    /// Deletes an ontology and its recorded versions.
    DeleteOntology {
        /// Ontology IRI.
        id: Id,
    },
    /// Lists registered ontologies.
    ListOntologies,
    /// Replaces an ontology definition and records a version.
    UpdateOntology(Ontology),
    /// Lists recorded versions of an ontology.
    OntologyVersions {
        /// Ontology IRI.
        id: Id,
    },
    /// Lists properties applicable to a class.
    ApplicableProperties {
        /// Class IRI.
        class: Id,
    },
    /// Lists classes whose restrictions reference the given properties.
    ApplicableClasses {
        /// Property IRIs.
        properties: Vec<Id>,
    },
}

/// SKOS taxonomy operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum TaxonomyCommand {
    /// Creates a scheme and records its initial version.
    CreateScheme(ConceptScheme),
    /// Retrieves a scheme.
    GetScheme {
        /// Scheme IRI.
        id: Id,
    },
    /// Lists schemes.
    ListSchemes,
    /// Replaces a scheme definition and records a version.
    UpdateScheme(ConceptScheme),
    /// Deletes a scheme definition.
    DeleteScheme {
        /// Scheme IRI.
        id: Id,
    },
    /// Lists recorded versions of a scheme.
    SchemeVersions {
        /// Scheme IRI.
        id: Id,
    },
    /// Creates a concept.
    CreateConcept(Concept),
    /// Retrieves a concept.
    GetConcept {
        /// Concept IRI.
        id: Id,
    },
    /// Lists concepts.
    ListConcepts(TaxonomyQuery),
    /// Replaces a concept definition.
    UpdateConcept(Concept),
    /// Deletes a concept.
    DeleteConcept {
        /// Concept IRI.
        id: Id,
    },
    /// Searches labels and notations.
    SearchConcepts {
        /// Search text.
        query: String,
        /// Restrict to a scheme.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scheme: Option<Id>,
    },
    /// Adds a relation.
    AddRelation {
        /// Source concept.
        source: Id,
        /// Relation kind.
        relation: SkosRelation,
        /// Target concept.
        target: Id,
    },
    /// Removes a relation.
    RemoveRelation {
        /// Source concept.
        source: Id,
        /// Relation kind.
        relation: SkosRelation,
        /// Target concept.
        target: Id,
    },
    /// Lists relations involving a concept.
    ListRelations {
        /// Concept IRI.
        id: Id,
    },
    /// Returns transitive narrower concepts.
    Descendants {
        /// Concept IRI.
        id: Id,
    },
    /// Returns transitive broader concepts.
    Ancestors {
        /// Concept IRI.
        id: Id,
    },
    /// Builds a scheme tree.
    ConceptTree {
        /// Scheme IRI.
        scheme: Id,
    },
    /// Moves a concept under a new parent and/or scheme.
    MoveConcept {
        /// Concept IRI.
        id: Id,
        /// New parent, when changing it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        broader: Option<Id>,
        /// New scheme, when changing it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scheme: Option<Id>,
    },
    /// Merges one scheme into another.
    MergeSchemes {
        /// Scheme to merge from.
        source: Id,
        /// Scheme to merge into.
        target: Id,
    },
    /// Validates a scheme or the whole taxonomy store.
    ValidateTaxonomy {
        /// Scheme to validate.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scheme: Option<Id>,
    },
    /// Creates a concept collection.
    CreateCollection(ConceptCollection),
    /// Retrieves a collection.
    GetCollection {
        /// Collection IRI.
        id: Id,
    },
    /// Lists collections.
    ListCollections {
        /// Restrict to a scheme.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scheme: Option<Id>,
    },
    /// Deletes a collection.
    DeleteCollection {
        /// Collection IRI.
        id: Id,
    },
}

/// External ontology registry and mapping operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum ExternalCommand {
    /// Imports an ontology document.
    Import(ImportRequest),
    /// Retrieves a registry entry.
    Get {
        /// Ontology IRI.
        id: Id,
    },
    /// Lists registry entries.
    List,
    /// Re-imports an ontology from its recorded source.
    Sync {
        /// Ontology IRI.
        id: Id,
    },
    /// Removes an ontology and its statements.
    Delete {
        /// Ontology IRI.
        id: Id,
    },
    /// Records a class mapping.
    MapClass(ClassMapping),
    /// Records a property mapping.
    MapProperty(PropertyMapping),
    /// Lists class mappings.
    ListClassMappings(MappingQuery),
    /// Lists property mappings.
    ListPropertyMappings(MappingQuery),
    /// Validates the mapping registry.
    ValidateMappings,
}

/// Semantic engine operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum SemanticCommand {
    /// Classifies one instance.
    Classify {
        /// Instance IRI.
        instance: Id,
        /// Classification options.
        options: ClassificationOptions,
    },
    /// Classifies several instances.
    ClassifyBatch {
        /// Instance IRIs.
        instances: Vec<Id>,
        /// Classification options.
        options: ClassificationOptions,
    },
    /// Lists stored classifications for an instance.
    ListClassifications {
        /// Instance IRI.
        instance: Id,
    },
    /// Records a manual classification override.
    OverrideClassification {
        /// Instance IRI.
        instance: Id,
        /// Concept IRI.
        concept: Id,
        /// Confidence in `0.0..=1.0`.
        confidence: f64,
    },
    /// Approves or rejects a classification.
    ReviewClassification {
        /// Classification record IRI.
        classification: Id,
        /// Whether to approve it.
        approved: bool,
    },
    /// Recomputes and replaces automatic classifications.
    Reclassify {
        /// Instance IRI.
        instance: Id,
        /// Classification options.
        options: ClassificationOptions,
    },
    /// Generates attribute and aggregate embeddings.
    GenerateEmbeddings {
        /// Instance IRI.
        instance: Id,
        /// Attributes to embed; empty means all literals.
        #[serde(default)]
        attributes: Vec<Id>,
    },
    /// Searches for instances similar to one instance.
    SearchSimilar {
        /// Instance IRI.
        instance: Id,
        /// Maximum results.
        limit: usize,
        /// Optional metadata filter.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter: Option<VectorFilter>,
    },
    /// Lists stored embedding metadata for an instance.
    ListEmbeddings {
        /// Instance IRI.
        instance: Id,
    },
    /// Deletes stored embeddings for an instance.
    DeleteEmbeddings {
        /// Instance IRI.
        instance: Id,
    },
    /// Previews inferred classes and properties.
    PreviewInference {
        /// Instance IRI.
        instance: Id,
        /// Inference options.
        options: InferenceOptions,
    },
    /// Applies inferred statements under a policy.
    ApplyInference {
        /// Instance IRI.
        instance: Id,
        /// Inference options.
        options: InferenceOptions,
        /// Application policy.
        policy: ApplyPolicy,
    },
    /// Explains inferred classes and properties.
    ExplainInference {
        /// Instance IRI.
        instance: Id,
        /// Inference options.
        options: InferenceOptions,
    },
}

/// Background job operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum JobCommand {
    /// Submits a job.
    Submit(JobRequest),
    /// Retrieves a job's status.
    Get {
        /// Job identifier.
        id: Id,
    },
    /// Lists jobs.
    List,
    /// Retrieves a job's result.
    Results {
        /// Job identifier.
        id: Id,
    },
    /// Requests cancellation.
    Cancel {
        /// Job identifier.
        id: Id,
    },
    /// Re-enqueues a failed or cancelled job.
    Retry {
        /// Job identifier.
        id: Id,
    },
}

/// A single resource returned by a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceResult {
    /// Resource IRI.
    pub id: String,
    /// Resource kind (e.g. `"instance"`).
    pub kind: String,
    /// Serialized resource.
    pub resource: serde_json::Value,
}

/// A page of resources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionResult {
    /// Serialized items.
    pub items: Vec<serde_json::Value>,
    /// Number of items in this page.
    pub count: usize,
    /// Requested page size.
    pub limit: usize,
    /// Requested offset.
    pub offset: usize,
}

/// The outcome of a mutating operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationReport {
    /// Operation name.
    pub operation: String,
    /// Number of affected quads or statements.
    pub affected: usize,
    /// Human-readable detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Result of executing a [`Command`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(into = "CommandResultRepr", from = "CommandResultRepr")]
pub enum CommandResult {
    /// A single resource.
    Resource(ResourceResult),
    /// A page of resources.
    Collection(CollectionResult),
    /// Query results.
    Query(QueryResult),
    /// A mutation report.
    Report(OperationReport),
    /// A schema validation report.
    Validation(ValidationReport),
    /// A taxonomy validation report.
    Taxonomy(TaxonomyReport),
    /// A mapping validation report.
    Mapping(MappingReport),
    /// Classification results.
    Classifications(Vec<Classification>),
    /// Embedding records.
    Embeddings(Vec<EmbeddingRecord>),
    /// Similarity search hits.
    Similarity(Vec<VectorMatch>),
    /// An inference preview or explanation.
    Preview(InferencePreview),
    /// An inference application report.
    Inference(InferenceReport),
    /// Serialized RDF.
    Export {
        /// RDF syntax name.
        format: String,
        /// Serialized document.
        data: String,
    },
    /// A background job status.
    Job(JobStatus),
    /// One document extraction result.
    Document(ExtractionOutput),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum CommandResultRepr {
    Resource(ResourceResult),
    Collection(CollectionResult),
    Query(QueryResult),
    Report(OperationReport),
    Validation(ValidationReport),
    Taxonomy(TaxonomyReport),
    Mapping(MappingReport),
    Classifications { items: Vec<Classification> },
    Embeddings { items: Vec<EmbeddingRecord> },
    Similarity { items: Vec<VectorMatch> },
    Preview(InferencePreview),
    Inference(InferenceReport),
    Export { format: String, data: String },
    Job(JobStatus),
    Document(ExtractionOutput),
}

impl From<CommandResult> for CommandResultRepr {
    fn from(result: CommandResult) -> Self {
        match result {
            CommandResult::Resource(value) => Self::Resource(value),
            CommandResult::Collection(value) => Self::Collection(value),
            CommandResult::Query(value) => Self::Query(value),
            CommandResult::Report(value) => Self::Report(value),
            CommandResult::Validation(value) => Self::Validation(value),
            CommandResult::Taxonomy(value) => Self::Taxonomy(value),
            CommandResult::Mapping(value) => Self::Mapping(value),
            CommandResult::Classifications(items) => Self::Classifications { items },
            CommandResult::Embeddings(items) => Self::Embeddings { items },
            CommandResult::Similarity(items) => Self::Similarity { items },
            CommandResult::Preview(value) => Self::Preview(value),
            CommandResult::Inference(value) => Self::Inference(value),
            CommandResult::Export { format, data } => Self::Export { format, data },
            CommandResult::Job(value) => Self::Job(value),
            CommandResult::Document(value) => Self::Document(value),
        }
    }
}

impl From<CommandResultRepr> for CommandResult {
    fn from(result: CommandResultRepr) -> Self {
        match result {
            CommandResultRepr::Resource(value) => Self::Resource(value),
            CommandResultRepr::Collection(value) => Self::Collection(value),
            CommandResultRepr::Query(value) => Self::Query(value),
            CommandResultRepr::Report(value) => Self::Report(value),
            CommandResultRepr::Validation(value) => Self::Validation(value),
            CommandResultRepr::Taxonomy(value) => Self::Taxonomy(value),
            CommandResultRepr::Mapping(value) => Self::Mapping(value),
            CommandResultRepr::Classifications { items } => Self::Classifications(items),
            CommandResultRepr::Embeddings { items } => Self::Embeddings(items),
            CommandResultRepr::Similarity { items } => Self::Similarity(items),
            CommandResultRepr::Preview(value) => Self::Preview(value),
            CommandResultRepr::Inference(value) => Self::Inference(value),
            CommandResultRepr::Export { format, data } => Self::Export { format, data },
            CommandResultRepr::Job(value) => Self::Job(value),
            CommandResultRepr::Document(value) => Self::Document(value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::QueryResult;
    use serde_json::json;

    #[test]
    fn commands_have_family_and_action_tags() {
        let command = Command::Rdf(RdfCommand::ListGraphs);
        let value = serde_json::to_value(&command).expect("serialize command");
        assert_eq!(
            value,
            json!({ "family": "rdf", "command": { "action": "listGraphs" } })
        );
        assert_eq!(
            serde_json::from_value::<Command>(value).expect("round trip"),
            command
        );
    }

    #[test]
    fn command_results_use_the_type_tag() {
        let report = CommandResult::Report(OperationReport {
            operation: "delete_instance".to_owned(),
            affected: 3,
            message: Some("removed".to_owned()),
        });
        let value = serde_json::to_value(report).expect("serialize report");
        assert_eq!(value["type"], "report");
        assert_eq!(value["operation"], "delete_instance");
        assert_eq!(value["affected"], 3);

        let query = CommandResult::Query(QueryResult::Solutions {
            variables: vec!["s".to_owned()],
            rows: Vec::new(),
        });
        let value = serde_json::to_value(query).expect("serialize query");
        assert_eq!(value["type"], "query");
        assert_eq!(value["resultType"], "solutions");
        assert_eq!(value["variables"], json!(["s"]));
        assert_eq!(value["rows"], json!([]));

        let query = CommandResult::Query(QueryResult::Boolean(true));
        let value = serde_json::to_value(&query).expect("serialize boolean query");
        assert_eq!(
            value,
            json!({ "type": "query", "resultType": "boolean", "value": true })
        );
        assert_eq!(
            serde_json::from_value::<CommandResult>(value).expect("deserialize boolean query"),
            query
        );
    }

    #[test]
    fn list_results_use_an_items_field() {
        let result = CommandResult::Classifications(Vec::new());
        let value = serde_json::to_value(&result).expect("serialize classifications");
        assert_eq!(value, json!({ "type": "classifications", "items": [] }));
        assert_eq!(
            serde_json::from_value::<CommandResult>(value).expect("deserialize classifications"),
            result
        );
    }

    #[test]
    fn collections_report_page_metadata() {
        let collection = CollectionResult {
            items: vec![json!({ "id": "urn:example:a" })],
            count: 1,
            limit: 20,
            offset: 40,
        };
        let value = serde_json::to_value(CommandResult::Collection(collection))
            .expect("serialize collection");
        assert_eq!(value["type"], "collection");
        assert_eq!(value["count"], 1);
        assert_eq!(value["limit"], 20);
        assert_eq!(value["offset"], 40);
    }

    #[test]
    fn external_and_job_families_round_trip() {
        for command in [
            Command::External(ExternalCommand::List),
            Command::Job(JobCommand::List),
            Command::Taxonomy(TaxonomyCommand::ListSchemes),
        ] {
            let encoded = serde_json::to_string(&command).expect("encode");
            let decoded: Command = serde_json::from_str(&encoded).expect("decode");
            assert_eq!(decoded, command);
        }
    }

    #[test]
    fn job_requests_use_stable_kind_names() {
        let request = JobRequest::RdfExport {
            format: "turtle".to_owned(),
            graph: None,
        };
        let value = serde_json::to_value(request).expect("serialize job");
        assert_eq!(value["kind"], "rdfExport");
        assert_eq!(value["request"]["format"], "turtle");
    }
}
