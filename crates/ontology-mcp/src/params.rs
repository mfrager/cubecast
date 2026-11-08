//! MCP tool input envelopes.
//!
//! Each tool accepts an explicit action plus structured fields. The shared
//! fields are flattened into every tool schema so clients see one predictable
//! envelope; the dispatcher validates which fields the chosen action requires.

use rmcp::schemars;
use serde::{Deserialize, Serialize};

/// RDF tool actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RdfAction {
    /// Retrieve one instance by IRI.
    Get,
    /// List instances, optionally filtered by type.
    Search,
    /// Create an instance from a full `NewInstance` body.
    Create,
    /// Update an instance from an `InstancePatch` body.
    Update,
    /// Delete an instance and its statements (destructive).
    Delete,
    /// Run a SPARQL SELECT, ASK, CONSTRUCT, or DESCRIBE query.
    Query,
    /// Run a SPARQL Update (destructive).
    SparqlUpdate,
    /// Parse an inline RDF document.
    Import,
    /// Serialize the dataset or one graph.
    Export,
    /// Inspect relationships, or list graphs when no target is given.
    Inspect,
    /// List instances with pagination.
    ListInstances,
    /// List named graphs.
    ListGraphs,
    /// List an instance's statements.
    Statements,
    /// Add statements to an instance.
    AddStatements,
    /// Delete statements by stable identifier (destructive).
    DeleteStatements,
    /// Remove every quad in a graph (destructive).
    ClearGraph,
}

impl RdfAction {
    /// Stable action name used in responses and audit records.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Search => "search",
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::Query => "query",
            Self::SparqlUpdate => "sparql_update",
            Self::Import => "import",
            Self::Export => "export",
            Self::Inspect => "inspect",
            Self::ListInstances => "list_instances",
            Self::ListGraphs => "list_graphs",
            Self::Statements => "statements",
            Self::AddStatements => "add_statements",
            Self::DeleteStatements => "delete_statements",
            Self::ClearGraph => "clear_graph",
        }
    }
}

/// Ontology tool actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OntologyAction {
    /// Class, property, or ontology CRUD selected by `kind`/`operation`.
    Schema,
    /// Validate the schema or a graph.
    Validate,
    /// ConceptScheme CRUD selected by `operation`.
    Scheme,
    /// Concept CRUD selected by `operation`.
    Concept,
    /// Hierarchy, relations, movement, and validation selected by `operation`.
    Taxonomy,
    /// Search classes, properties, concepts, or schemes.
    Search,
    /// Import an external ontology.
    ExternalImport,
    /// Re-fetch a registered external ontology.
    ExternalSync,
    /// Class/property mapping operations selected by `operation`.
    Mapping,
    /// Merge one scheme into another.
    Merge,
}

impl OntologyAction {
    /// Stable action name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Schema => "schema",
            Self::Validate => "validate",
            Self::Scheme => "scheme",
            Self::Concept => "concept",
            Self::Taxonomy => "taxonomy",
            Self::Search => "search",
            Self::ExternalImport => "external_import",
            Self::ExternalSync => "external_sync",
            Self::Mapping => "mapping",
            Self::Merge => "merge",
        }
    }
}

/// Semantic tool actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SemanticAction {
    /// Classify one instance.
    Classify,
    /// Classify several instances.
    ClassifyBatch,
    /// Generate attribute embeddings.
    Embed,
    /// Search for similar instances.
    Similarity,
    /// Preview inferred classes and properties.
    Infer,
    /// Preview proposed enrichment.
    Preview,
    /// Apply inferred statements under a policy.
    Apply,
    /// Explain a classification or inference.
    Explain,
    /// Recompute classifications.
    Reclassify,
    /// Regenerate embeddings.
    Reembed,
    /// List stored classifications.
    Classifications,
    /// List stored embedding metadata.
    Embeddings,
    /// Delete stored embeddings (destructive).
    DeleteEmbeddings,
    /// Approve or reject a classification.
    Review,
    /// Record a manual classification.
    Override,
}

impl SemanticAction {
    /// Stable action name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Classify => "classify",
            Self::ClassifyBatch => "classify_batch",
            Self::Embed => "embed",
            Self::Similarity => "similarity",
            Self::Infer => "infer",
            Self::Preview => "preview",
            Self::Apply => "apply",
            Self::Explain => "explain",
            Self::Reclassify => "reclassify",
            Self::Reembed => "reembed",
            Self::Classifications => "classifications",
            Self::Embeddings => "embeddings",
            Self::DeleteEmbeddings => "delete_embeddings",
            Self::Review => "review",
            Self::Override => "override",
        }
    }
}

/// Job tool actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobsAction {
    /// Submit a background job.
    Submit,
    /// List jobs.
    List,
    /// Retrieve one job's status.
    Get,
    /// Retrieve one job's result.
    Results,
    /// Request cancellation.
    Cancel,
    /// Retry a failed or cancelled job.
    Retry,
}

impl JobsAction {
    /// Stable action name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Submit => "submit",
            Self::List => "list",
            Self::Get => "get",
            Self::Results => "results",
            Self::Cancel => "cancel",
            Self::Retry => "retry",
        }
    }
}

/// Fields shared by all four tools.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommonArgs {
    /// Primary resource IRI (instance, class, property, concept, scheme, job,
    /// or classification depending on the action).
    #[serde(default)]
    pub target: Option<String>,
    /// Secondary identifier, used by job and classification actions.
    #[serde(default)]
    pub id: Option<String>,
    /// Target or scope graph.
    #[serde(default)]
    pub graph: Option<String>,
    /// Graph receiving inferred statements.
    #[serde(default)]
    pub target_graph: Option<String>,
    /// Concept scheme IRI.
    #[serde(default)]
    pub scheme: Option<String>,
    /// Parent class or broader concept IRI.
    #[serde(default)]
    pub parent: Option<String>,
    /// Ontology IRI.
    #[serde(default)]
    pub ontology: Option<String>,
    /// OWL class IRI.
    #[serde(default)]
    pub class: Option<String>,
    /// OWL property IRI.
    #[serde(default)]
    pub property: Option<String>,
    /// Instance IRI.
    #[serde(default)]
    pub instance: Option<String>,
    /// Concept IRI.
    #[serde(default)]
    pub concept: Option<String>,
    /// External document path or URI.
    #[serde(default)]
    pub source: Option<String>,
    /// External term IRI.
    #[serde(default)]
    pub external: Option<String>,
    /// Internal term IRI.
    #[serde(default)]
    pub internal: Option<String>,
    /// SKOS relation name.
    #[serde(default)]
    pub relation: Option<String>,
    /// Schema object kind (`class`, `property`, `ontology`) or search kind.
    #[serde(default)]
    pub kind: Option<String>,
    /// Operation within a compound action (`get`, `list`, `create`, ...).
    #[serde(default)]
    pub operation: Option<String>,
    /// OWL property kind (`object`, `datatype`, `annotation`).
    #[serde(default)]
    pub property_kind: Option<String>,
    /// External mapping kind.
    #[serde(default)]
    pub mapping_kind: Option<String>,
    /// RDF syntax name.
    #[serde(default)]
    pub format: Option<String>,
    /// Inline RDF document.
    #[serde(default)]
    pub data: Option<String>,
    /// SPARQL query or update text.
    #[serde(default)]
    pub sparql: Option<String>,
    /// Free-text search or list filter.
    #[serde(default)]
    pub query: Option<String>,
    /// Label filter.
    #[serde(default)]
    pub label: Option<String>,
    /// Label or content language.
    #[serde(default)]
    pub language: Option<String>,
    /// Full resource body for create/update and complex actions.
    #[serde(default)]
    pub body: Option<serde_json::Value>,
    /// Instance IRIs for batch work or applicable-class queries.
    #[serde(default)]
    pub instances: Vec<String>,
    /// Additional instance property IRIs.
    #[serde(default)]
    pub properties: Vec<String>,
    /// Attribute IRIs to classify or embed.
    #[serde(default)]
    pub attributes: Vec<String>,
    /// Classification strategies.
    #[serde(default)]
    pub strategies: Vec<String>,
    /// Single classification strategy shorthand.
    #[serde(default)]
    pub strategy: Option<String>,
    /// Type filter for instance listings.
    #[serde(default)]
    pub type_filter: Option<String>,
    /// `rdf:type` values used with a create shorthand.
    #[serde(default)]
    pub types: Vec<String>,
    /// Concept collection members.
    #[serde(default)]
    pub members: Vec<String>,
    /// Scheme top concepts.
    #[serde(default)]
    pub top_concepts: Vec<String>,
    /// Broader concept IRIs.
    #[serde(default)]
    pub broader: Vec<String>,
    /// Narrower concept IRIs.
    #[serde(default)]
    pub narrower: Vec<String>,
    /// Stable statement identifiers.
    #[serde(default)]
    pub statement_ids: Vec<String>,
    /// Maximum results.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Results to skip.
    #[serde(default)]
    pub offset: Option<usize>,
    /// Classification confidence threshold.
    #[serde(default)]
    pub threshold: Option<f64>,
    /// Maximum classifications per instance.
    #[serde(default)]
    pub max_results: Option<usize>,
    /// Inference policy (`suggest`, `review`, `automatic`) or job kind.
    #[serde(default)]
    pub policy: Option<String>,
    /// Automatic inference threshold.
    #[serde(default)]
    pub policy_threshold: Option<f64>,
    /// Manual classification confidence.
    #[serde(default)]
    pub confidence: Option<f64>,
    /// Review decision.
    #[serde(default)]
    pub approved: Option<bool>,
    /// Include external mappings in inference.
    #[serde(default)]
    pub include_external: Option<bool>,
    /// Include classification results in inference.
    #[serde(default)]
    pub include_classification: Option<bool>,
    /// Include inherited/applicable properties in inference.
    #[serde(default)]
    pub include_inherited: Option<bool>,
    /// Include ancestor concepts in classification.
    #[serde(default)]
    pub include_ancestors: Option<bool>,
    /// Persist classification results.
    #[serde(default)]
    pub persist: Option<bool>,
    /// Submit expensive work as a background job.
    #[serde(default)]
    pub background: bool,
    /// Explicit confirmation for destructive or consequential actions.
    #[serde(default)]
    pub confirm: bool,
    /// Return the translated command without executing it.
    #[serde(default)]
    pub dry_run: bool,
}

/// Input for the `rdf` tool.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RdfArgs {
    /// RDF operation.
    pub action: RdfAction,
    /// Operation parameters.
    #[serde(flatten)]
    pub args: CommonArgs,
}

/// Input for the `ontology` tool.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OntologyArgs {
    /// Ontology operation.
    pub action: OntologyAction,
    /// Operation parameters.
    #[serde(flatten)]
    pub args: CommonArgs,
}

/// Input for the `semantic` tool.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SemanticArgs {
    /// Semantic operation.
    pub action: SemanticAction,
    /// Operation parameters.
    #[serde(flatten)]
    pub args: CommonArgs,
}

/// Input for the `jobs` tool.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct JobsArgs {
    /// Job operation.
    pub action: JobsAction,
    /// Operation parameters.
    #[serde(flatten)]
    pub args: CommonArgs,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flattening_keeps_the_top_level_envelope() {
        let args: RdfArgs = serde_json::from_value(serde_json::json!({
            "action": "query",
            "sparql": "ASK {}",
            "limit": 5
        }))
        .unwrap();
        assert_eq!(args.action, RdfAction::Query);
        assert_eq!(args.args.sparql.as_deref(), Some("ASK {}"));
        assert_eq!(args.args.limit, Some(5));
    }

    #[test]
    fn actions_use_snake_case_names() {
        let value = serde_json::to_value(RdfAction::SparqlUpdate).unwrap();
        assert_eq!(value, serde_json::json!("sparql_update"));
        assert_eq!(RdfAction::ListGraphs.as_str(), "list_graphs");
    }
}
