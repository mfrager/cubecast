//! Command-line surface: Clap definitions and translation into [`Command`]s.

use std::io::Read;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use ontology_core::command::{
    Command, DocumentCommand, ExternalCommand, JobCommand, OntologyCommand, ProjectCommand,
    RdfCommand, RegistryCommand, SemanticCommand, TaxonomyCommand,
};
use ontology_core::document::{ExtractionMode, ExtractionRequest, TextFormat};
use ontology_core::external::{ClassMapping, ImportRequest, MappingQuery, PropertyMapping};
use ontology_core::id::Id;
use ontology_core::job::JobRequest;
use ontology_core::model::{
    GraphName, InstancePatch, InstanceQuery, NewInstance, StatementId, StatementInput,
};
use ontology_core::owl::{
    ClassQuery, GraphScope, Ontology, OwlClass, OwlProperty, PropertyKind, PropertyQuery,
};
use ontology_core::project::{TaskEvent, WorkKind};
use ontology_core::registry::IngestRequest;
use ontology_core::semantic::{
    ApplyPolicy, ClassificationOptions, ClassificationStrategy, InferenceOptions,
};
use ontology_core::skos::{Concept, ConceptCollection, ConceptScheme, SkosRelation, TaxonomyQuery};
use serde::de::DeserializeOwned;

use crate::client::{Invocation, OutputFormat};
use crate::CliResult;

/// Top-level CLI options.
#[derive(Debug, Parser)]
#[command(
    name = "onto",
    version,
    about = "Remote administration for the ontology management system"
)]
pub struct Cli {
    /// API base URL.
    #[arg(long, global = true, env = "ONTO_SERVER")]
    pub server: Option<String>,
    /// Bearer token for authenticated servers.
    #[arg(long, global = true, env = "ONTO_API_KEY")]
    pub api_key: Option<String>,
    /// Named connection profile from the CLI configuration.
    #[arg(long, global = true, env = "ONTO_PROFILE")]
    pub profile: Option<String>,
    /// Result rendering.
    #[arg(long, global = true, value_enum)]
    pub output: Option<OutputFormat>,
    /// Print the command instead of sending it.
    #[arg(long, global = true)]
    pub dry_run: bool,
    /// Skip interactive confirmation for destructive commands.
    #[arg(long, global = true)]
    pub yes: bool,
    /// Command to run.
    #[command(subcommand)]
    pub command: CommandArgs,
}

/// Every top-level command.
#[derive(Debug, Subcommand)]
pub enum CommandArgs {
    /// Start the HTTP API server.
    Server(ServerArgs),
    /// Show the resolved client configuration.
    Config,
    /// RDF instance and statement operations.
    Instance {
        /// Operation.
        #[command(subcommand)]
        command: InstanceCommand,
    },
    /// Dataset operations: SPARQL, graphs, import, and export.
    Rdf {
        /// Operation.
        #[command(subcommand)]
        command: RdfArgs,
    },
    /// OWL ontology operations.
    Ontology {
        /// Operation.
        #[command(subcommand)]
        command: OntologyArgs,
    },
    /// OWL class operations.
    Class {
        /// Operation.
        #[command(subcommand)]
        command: ClassArgs,
    },
    /// OWL property operations.
    Property {
        /// Operation.
        #[command(subcommand)]
        command: PropertyArgs,
    },
    /// SKOS concept scheme operations.
    Scheme {
        /// Operation.
        #[command(subcommand)]
        command: SchemeArgs,
    },
    /// SKOS concept operations.
    Concept {
        /// Operation.
        #[command(subcommand)]
        command: ConceptArgs,
    },
    /// Taxonomy trees, relations, collections, and validation.
    Taxonomy {
        /// Operation.
        #[command(subcommand)]
        command: TaxonomyArgs,
    },
    /// Classification and review.
    Classify {
        /// Operation.
        #[command(subcommand)]
        command: ClassifyArgs,
    },
    /// Embeddings and similarity search.
    Embedding {
        /// Operation.
        #[command(subcommand)]
        command: EmbeddingArgs,
    },
    /// External ontology registry and mappings.
    External {
        /// Operation.
        #[command(subcommand)]
        command: ExternalArgs,
    },
    /// Inference and schema application.
    Inference {
        /// Operation.
        #[command(subcommand)]
        command: InferenceArgs,
    },
    /// Document extraction through MinerU.
    Document {
        /// Operation.
        #[command(subcommand)]
        command: DocumentArgs,
    },
    /// Semantic registry: catalogs, assets, ingestion, releases, and search.
    Registry {
        /// Operation.
        #[command(subcommand)]
        command: RegistryArgs,
    },
    /// Project management and deliverable acceptance.
    Project {
        /// Operation.
        #[command(subcommand)]
        command: ProjectArgs,
    },
    /// Bulk RDF import.
    Import(ImportArgs),
    /// Bulk RDF export.
    Export(ExportArgs),
    /// Background job operations.
    Job {
        /// Operation.
        #[command(subcommand)]
        command: JobArgs,
    },
    /// Interactive persistent session.
    Shell,
}

/// Server options.
#[derive(Debug, Args)]
pub struct ServerArgs {
    /// Address to bind, overriding `ONTO_BIND`.
    #[arg(long)]
    pub bind: Option<String>,
    /// Configuration file, overriding `ONTO_CONFIG`.
    #[arg(long)]
    pub config: Option<PathBuf>,
}

/// JSON payload supplied inline or from a file.
#[derive(Debug, Args)]
pub struct PayloadArgs {
    /// Inline JSON document.
    #[arg(long)]
    pub json: Option<String>,
    /// Path to a JSON document.
    #[arg(long)]
    pub file: Option<PathBuf>,
}

impl PayloadArgs {
    fn read(&self) -> CliResult<String> {
        match (&self.json, &self.file) {
            (Some(json), None) => Ok(json.clone()),
            (None, Some(path)) => Ok(std::fs::read_to_string(path)?),
            (None, None) => Err("provide --json or --file".into()),
            (Some(_), Some(_)) => Err("provide either --json or --file, not both".into()),
        }
    }

    fn parse<T: DeserializeOwned>(&self) -> CliResult<T> {
        Ok(serde_json::from_str(&self.read()?)?)
    }
}

/// Document extraction operations.
#[derive(Debug, Subcommand)]
pub enum DocumentArgs {
    /// Extract a PDF or image through MinerU.
    Extract(DocumentExtractArgs),
    /// Load the latest stored extraction for a document id.
    Get {
        /// `urn:sha256:<hex>` document identifier.
        id: String,
    },
}

/// Extraction options.
#[derive(Debug, Args)]
pub struct DocumentExtractArgs {
    /// Path to the PDF or image.
    pub file: PathBuf,
    /// Extraction mode.
    #[arg(long, value_parser = ["structured", "text", "both"])]
    pub mode: Option<String>,
    /// Text format for `text` and `both` modes.
    #[arg(long, value_parser = ["markdown", "plain"])]
    pub format: Option<String>,
    /// Override the configured output directory.
    #[arg(long)]
    pub output_dir: Option<PathBuf>,
    /// Re-run even when a cached extraction exists.
    #[arg(long)]
    pub force: bool,
}

/// Instance operations.
#[derive(Debug, Subcommand)]
pub enum InstanceCommand {
    /// Create an instance from a `NewInstance` JSON document.
    Create {
        /// Payload.
        #[command(flatten)]
        payload: PayloadArgs,
    },
    /// Retrieve one instance.
    Get {
        /// Instance IRI.
        id: String,
        /// Restrict to a graph.
        #[arg(long)]
        graph: Option<String>,
    },
    /// List instances.
    List {
        /// Restrict to a graph.
        #[arg(long)]
        graph: Option<String>,
        /// Restrict to an `rdf:type`.
        #[arg(long = "type")]
        r#type: Option<String>,
        /// Page size.
        #[arg(long, default_value_t = 100)]
        limit: usize,
        /// Rows to skip.
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
    /// Apply a partial update from an `InstancePatch` document.
    Update {
        /// Instance IRI.
        id: String,
        /// Payload.
        #[command(flatten)]
        payload: PayloadArgs,
        /// Restrict to a graph.
        #[arg(long)]
        graph: Option<String>,
    },
    /// Delete an instance and its statements.
    Delete {
        /// Instance IRI.
        id: String,
        /// Restrict to a graph.
        #[arg(long)]
        graph: Option<String>,
    },
    /// List an instance's statements.
    Statements {
        /// Instance IRI.
        id: String,
        /// Restrict to a graph.
        #[arg(long)]
        graph: Option<String>,
    },
    /// Add statements from a JSON array of `StatementInput`.
    AddStatements {
        /// Instance IRI.
        id: String,
        /// Payload.
        #[command(flatten)]
        payload: PayloadArgs,
        /// Target graph.
        #[arg(long)]
        graph: Option<String>,
        /// Provenance source.
        #[arg(long)]
        source: Option<String>,
    },
    /// Delete statements by identifier.
    DeleteStatements {
        /// Instance IRI.
        id: String,
        /// Statement identifiers.
        statement_ids: Vec<String>,
        /// Restrict to a graph.
        #[arg(long)]
        graph: Option<String>,
    },
    /// List incoming and outgoing relationships.
    Relationships {
        /// Resource IRI.
        id: String,
    },
}

/// SPARQL text source.
#[derive(Debug, Args)]
pub struct QueryArgs {
    /// Query text.
    pub sparql: Option<String>,
    /// Read the query from a file.
    #[arg(long)]
    pub file: Option<PathBuf>,
    /// Read the query from standard input.
    #[arg(long)]
    pub stdin: bool,
}

impl QueryArgs {
    fn text(&self) -> CliResult<String> {
        read_text(&self.sparql, &self.file, self.stdin)
    }
}

/// RDF import arguments.
#[derive(Debug, Args)]
pub struct ImportArgs {
    /// RDF syntax (`turtle`, `ntriples`, `nquads`, `trig`, `rdfxml`, `jsonld`).
    #[arg(long, default_value = "turtle")]
    pub format: String,
    /// Inline document.
    pub data: Option<String>,
    /// Read the document from a file.
    #[arg(long)]
    pub file: Option<PathBuf>,
    /// Read the document from standard input.
    #[arg(long)]
    pub stdin: bool,
    /// Target graph.
    #[arg(long)]
    pub graph: Option<String>,
}

/// RDF export arguments.
#[derive(Debug, Args)]
pub struct ExportArgs {
    /// RDF syntax name.
    #[arg(long, default_value = "turtle")]
    pub format: String,
    /// Restrict to a graph.
    #[arg(long)]
    pub graph: Option<String>,
    /// Write the serialized document to this path.
    #[arg(long)]
    pub output: Option<PathBuf>,
}

/// Dataset operations.
#[derive(Debug, Subcommand)]
pub enum RdfArgs {
    /// Evaluate a SPARQL query.
    Query(QueryArgs),
    /// Evaluate a SPARQL update.
    Update(QueryArgs),
    /// List named graphs.
    Graphs,
    /// Import an RDF document.
    Import(ImportArgs),
    /// Export the dataset or one graph.
    Export(ExportArgs),
    /// Remove every quad in a graph.
    ClearGraph {
        /// Graph to clear.
        graph: String,
    },
}

/// OWL ontology operations.
#[derive(Debug, Subcommand)]
pub enum OntologyArgs {
    /// List registered ontologies.
    List,
    /// Retrieve an ontology.
    Get {
        /// Ontology IRI.
        id: String,
    },
    /// Create an ontology from an `Ontology` document.
    Create {
        /// Payload.
        #[command(flatten)]
        payload: PayloadArgs,
    },
    /// Replace an ontology definition.
    Update {
        /// Ontology IRI.
        id: String,
        /// Payload.
        #[command(flatten)]
        payload: PayloadArgs,
    },
    /// Delete an ontology definition.
    Delete {
        /// Ontology IRI.
        id: String,
    },
    /// List recorded versions.
    Versions {
        /// Ontology IRI.
        id: String,
    },
    /// Validate the schema or a graph.
    Validate {
        /// Graph to validate; the managed schema when omitted.
        #[arg(long)]
        graph: Option<String>,
    },
    /// List properties applicable to a class.
    ApplicableProperties {
        /// Class IRI.
        class: String,
    },
    /// List classes whose restrictions reference the given properties.
    ApplicableClasses {
        /// Property IRIs.
        properties: Vec<String>,
    },
}

/// OWL class operations.
#[derive(Debug, Subcommand)]
pub enum ClassArgs {
    /// List classes.
    List {
        /// Only direct subclasses.
        #[arg(long)]
        parent: Option<String>,
        /// Substring match against labels.
        #[arg(long)]
        label: Option<String>,
        /// Page size.
        #[arg(long, default_value_t = 100)]
        limit: usize,
        /// Rows to skip.
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
    /// Retrieve a class.
    Get {
        /// Class IRI.
        id: String,
    },
    /// Create a class from an `OwlClass` document.
    Create {
        /// Payload.
        #[command(flatten)]
        payload: PayloadArgs,
    },
    /// Replace a class definition.
    Update {
        /// Class IRI.
        id: String,
        /// Payload.
        #[command(flatten)]
        payload: PayloadArgs,
    },
    /// Delete a class definition.
    Delete {
        /// Class IRI.
        id: String,
    },
    /// Show the subclass hierarchy around a class.
    Hierarchy {
        /// Class IRI.
        id: String,
    },
}

/// OWL property operations.
#[derive(Debug, Subcommand)]
pub enum PropertyArgs {
    /// List properties.
    List {
        /// Restrict by kind (`object`, `datatype`, `annotation`).
        #[arg(long)]
        kind: Option<String>,
        /// Substring match against labels.
        #[arg(long)]
        label: Option<String>,
        /// Page size.
        #[arg(long, default_value_t = 100)]
        limit: usize,
        /// Rows to skip.
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
    /// Retrieve a property.
    Get {
        /// Property IRI.
        id: String,
    },
    /// Create a property from an `OwlProperty` document.
    Create {
        /// Payload.
        #[command(flatten)]
        payload: PayloadArgs,
    },
    /// Replace a property definition.
    Update {
        /// Property IRI.
        id: String,
        /// Payload.
        #[command(flatten)]
        payload: PayloadArgs,
    },
    /// Delete a property definition.
    Delete {
        /// Property IRI.
        id: String,
    },
}

/// SKOS scheme operations.
#[derive(Debug, Subcommand)]
pub enum SchemeArgs {
    /// List schemes.
    List,
    /// Retrieve a scheme.
    Get {
        /// Scheme IRI.
        id: String,
    },
    /// Create a scheme from a `ConceptScheme` document.
    Create {
        /// Payload.
        #[command(flatten)]
        payload: PayloadArgs,
    },
    /// Replace a scheme definition.
    Update {
        /// Scheme IRI.
        id: String,
        /// Payload.
        #[command(flatten)]
        payload: PayloadArgs,
    },
    /// Delete a scheme.
    Delete {
        /// Scheme IRI.
        id: String,
    },
    /// List recorded versions.
    Versions {
        /// Scheme IRI.
        id: String,
    },
}

/// SKOS concept operations.
#[derive(Debug, Subcommand)]
pub enum ConceptArgs {
    /// List concepts.
    List {
        /// Restrict to a scheme.
        #[arg(long)]
        scheme: Option<String>,
        /// Substring match against labels.
        #[arg(long)]
        label: Option<String>,
        /// Page size.
        #[arg(long, default_value_t = 100)]
        limit: usize,
        /// Rows to skip.
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
    /// Retrieve a concept.
    Get {
        /// Concept IRI.
        id: String,
    },
    /// Create a concept from a `Concept` document.
    Create {
        /// Payload.
        #[command(flatten)]
        payload: PayloadArgs,
    },
    /// Replace a concept definition.
    Update {
        /// Concept IRI.
        id: String,
        /// Payload.
        #[command(flatten)]
        payload: PayloadArgs,
    },
    /// Delete a concept.
    Delete {
        /// Concept IRI.
        id: String,
    },
    /// Search labels and notations.
    Search {
        /// Search text.
        query: String,
        /// Restrict to a scheme.
        #[arg(long)]
        scheme: Option<String>,
    },
    /// List relations involving a concept.
    Relations {
        /// Concept IRI.
        id: String,
    },
    /// List transitive narrower concepts.
    Descendants {
        /// Concept IRI.
        id: String,
    },
    /// List transitive broader concepts.
    Ancestors {
        /// Concept IRI.
        id: String,
    },
    /// Move a concept under a new parent and/or scheme.
    Move {
        /// Concept IRI.
        id: String,
        /// New parent.
        #[arg(long)]
        broader: Option<String>,
        /// New scheme.
        #[arg(long)]
        scheme: Option<String>,
    },
}

/// Taxonomy operations.
#[derive(Debug, Subcommand)]
pub enum TaxonomyArgs {
    /// Show a scheme tree.
    Tree {
        /// Scheme IRI.
        scheme: String,
    },
    /// Merge one scheme into another.
    Merge {
        /// Scheme to merge from.
        source: String,
        /// Scheme to merge into.
        target: String,
    },
    /// Validate a scheme or the whole taxonomy store.
    Validate {
        /// Scheme IRI.
        #[arg(long)]
        scheme: Option<String>,
    },
    /// Add a relation.
    AddRelation {
        /// Source concept.
        source: String,
        /// Relation (`broader`, `related`, `exactMatch`, …).
        relation: String,
        /// Target concept.
        target: String,
    },
    /// Remove a relation.
    RemoveRelation {
        /// Source concept.
        source: String,
        /// Relation.
        relation: String,
        /// Target concept.
        target: String,
    },
    /// List collections.
    Collections {
        /// Restrict to a scheme.
        #[arg(long)]
        scheme: Option<String>,
    },
    /// Create a collection from a `ConceptCollection` document.
    CreateCollection {
        /// Payload.
        #[command(flatten)]
        payload: PayloadArgs,
    },
    /// Retrieve a collection.
    GetCollection {
        /// Collection IRI.
        id: String,
    },
    /// Delete a collection.
    DeleteCollection {
        /// Collection IRI.
        id: String,
    },
}

/// Classification options shared by single and batch runs.
#[derive(Debug, Args)]
pub struct ClassificationArgs {
    /// Full `ClassificationOptions` JSON document.
    #[arg(long)]
    pub options: Option<String>,
    /// Minimum confidence.
    #[arg(long)]
    pub threshold: Option<f64>,
    /// Maximum results per instance.
    #[arg(long)]
    pub max_results: Option<usize>,
    /// Restrict to a scheme.
    #[arg(long)]
    pub scheme: Option<String>,
    /// Strategy (`exact`, `normalized`, `token`, `fuzzy`, `semantic`, `hybrid`).
    #[arg(long = "strategy")]
    pub strategies: Vec<String>,
    /// Do not persist the results.
    #[arg(long)]
    pub no_persist: bool,
}

impl ClassificationArgs {
    fn options(&self) -> CliResult<ClassificationOptions> {
        let mut options: ClassificationOptions = match &self.options {
            Some(json) => serde_json::from_str(json)?,
            None => ClassificationOptions::default(),
        };
        if let Some(threshold) = self.threshold {
            options.threshold = threshold;
        }
        if let Some(max_results) = self.max_results {
            options.max_results = max_results;
        }
        if let Some(scheme) = &self.scheme {
            options.scheme = Some(parse_id(scheme)?);
        }
        for strategy in &self.strategies {
            options.strategies.push(
                ClassificationStrategy::parse(strategy)
                    .ok_or_else(|| format!("unknown classification strategy `{strategy}`"))?,
            );
        }
        if self.no_persist {
            options.persist = false;
        }
        Ok(options)
    }
}

/// Classification operations.
#[derive(Debug, Subcommand)]
pub enum ClassifyArgs {
    /// Classify one instance.
    Run {
        /// Instance IRI.
        instance: String,
        /// Options.
        #[command(flatten)]
        options: ClassificationArgs,
    },
    /// Classify several instances.
    Batch {
        /// Instance IRIs.
        instances: Vec<String>,
        /// Options.
        #[command(flatten)]
        options: ClassificationArgs,
    },
    /// List stored classifications.
    List {
        /// Instance IRI.
        instance: String,
    },
    /// Record a manual classification.
    Override {
        /// Instance IRI.
        instance: String,
        /// Concept IRI.
        concept: String,
        /// Confidence.
        #[arg(long, default_value_t = 1.0)]
        confidence: f64,
    },
    /// Approve or reject a classification.
    Review {
        /// Classification record IRI.
        classification: String,
        /// Reject instead of approve.
        #[arg(long)]
        reject: bool,
    },
    /// Recompute automatic classifications.
    Reclassify {
        /// Instance IRI.
        instance: String,
        /// Options.
        #[command(flatten)]
        options: ClassificationArgs,
    },
}

/// Embedding operations.
#[derive(Debug, Subcommand)]
pub enum EmbeddingArgs {
    /// Generate embeddings for an instance.
    Generate {
        /// Instance IRI.
        instance: String,
        /// Attributes to embed.
        #[arg(long)]
        attributes: Vec<String>,
    },
    /// List stored embedding metadata.
    List {
        /// Instance IRI.
        instance: String,
    },
    /// Search for similar instances.
    Search {
        /// Instance IRI.
        instance: String,
        /// Maximum results.
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Delete stored embeddings.
    Delete {
        /// Instance IRI.
        instance: String,
    },
}

/// External ontology operations.
#[derive(Debug, Subcommand)]
pub enum ExternalArgs {
    /// Import an external ontology.
    Import {
        /// Path or URI.
        source: String,
        /// RDF syntax name.
        #[arg(long)]
        format: Option<String>,
        /// Ontology IRI override.
        #[arg(long)]
        ontology: Option<String>,
        /// Target graph.
        #[arg(long)]
        graph: Option<String>,
    },
    /// List registered external ontologies.
    List,
    /// Retrieve one.
    Get {
        /// Ontology IRI.
        id: String,
    },
    /// Re-fetch and synchronize.
    Sync {
        /// Ontology IRI.
        id: String,
    },
    /// Delete an external ontology.
    Delete {
        /// Ontology IRI.
        id: String,
    },
    /// Record a class mapping from a `ClassMapping` document.
    MapClass {
        /// Payload.
        #[command(flatten)]
        payload: PayloadArgs,
    },
    /// Record a property mapping from a `PropertyMapping` document.
    MapProperty {
        /// Payload.
        #[command(flatten)]
        payload: PayloadArgs,
    },
    /// List class mappings.
    ClassMappings {
        /// Restrict by external IRI.
        #[arg(long)]
        external: Option<String>,
        /// Restrict by internal IRI.
        #[arg(long)]
        internal: Option<String>,
        /// Page size.
        #[arg(long, default_value_t = 100)]
        limit: usize,
        /// Rows to skip.
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
    /// List property mappings.
    PropertyMappings {
        /// Restrict by external IRI.
        #[arg(long)]
        external: Option<String>,
        /// Restrict by internal IRI.
        #[arg(long)]
        internal: Option<String>,
        /// Page size.
        #[arg(long, default_value_t = 100)]
        limit: usize,
        /// Rows to skip.
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
    /// Validate the mapping registry.
    ValidateMappings,
}

/// Inference options.
#[derive(Debug, Args)]
pub struct InferenceOptionsArgs {
    /// Full `InferenceOptions` JSON document.
    #[arg(long)]
    pub options: Option<String>,
    /// Do not apply external mappings.
    #[arg(long)]
    pub no_external: bool,
    /// Do not use classifications.
    #[arg(long)]
    pub no_classification: bool,
}

impl InferenceOptionsArgs {
    fn options(&self) -> CliResult<InferenceOptions> {
        let mut options: InferenceOptions = match &self.options {
            Some(json) => serde_json::from_str(json)?,
            None => InferenceOptions::default(),
        };
        if self.no_external {
            options.include_external = false;
        }
        if self.no_classification {
            options.include_classification = false;
        }
        Ok(options)
    }
}

/// Inference operations.
#[derive(Debug, Subcommand)]
pub enum InferenceArgs {
    /// Preview inferred statements.
    Preview {
        /// Instance IRI.
        instance: String,
        /// Options.
        #[command(flatten)]
        options: InferenceOptionsArgs,
    },
    /// Apply inferred statements.
    Apply {
        /// Instance IRI.
        instance: String,
        /// Options.
        #[command(flatten)]
        options: InferenceOptionsArgs,
        /// Policy (`suggest`, `review`, `automatic`).
        #[arg(long, default_value = "suggest")]
        policy: String,
        /// Automatic policy threshold.
        #[arg(long, default_value_t = 0.8)]
        threshold: f64,
    },
    /// Explain inferred statements.
    Explain {
        /// Instance IRI.
        instance: String,
        /// Options.
        #[command(flatten)]
        options: InferenceOptionsArgs,
    },
}

/// Job operations.
#[derive(Debug, Subcommand)]
pub enum JobArgs {
    /// Submit a `JobRequest` JSON document.
    Submit {
        /// Payload.
        #[command(flatten)]
        payload: PayloadArgs,
    },
    /// Submit a classification batch.
    Classify {
        /// Instance IRIs.
        instances: Vec<String>,
        /// Options.
        #[command(flatten)]
        options: ClassificationArgs,
    },
    /// Submit an embedding batch.
    Embed {
        /// Instance IRIs.
        instances: Vec<String>,
        /// Attributes to embed.
        #[arg(long)]
        attributes: Vec<String>,
    },
    /// Submit an inference batch.
    Infer {
        /// Instance IRIs.
        instances: Vec<String>,
        /// Options.
        #[command(flatten)]
        options: InferenceOptionsArgs,
        /// Policy (`suggest`, `review`, `automatic`).
        #[arg(long, default_value = "suggest")]
        policy: String,
        /// Automatic policy threshold.
        #[arg(long, default_value_t = 0.8)]
        threshold: f64,
    },
    /// Submit a taxonomy reclassification batch.
    Reclassify {
        /// Instance IRIs.
        instances: Vec<String>,
        /// Options.
        #[command(flatten)]
        options: ClassificationArgs,
    },
    /// Submit a bulk RDF import.
    Import(ImportArgs),
    /// Submit a bulk RDF export.
    Export(ExportArgs),
    /// Submit an external ontology import.
    ExternalImport {
        /// Path or URI.
        source: String,
        /// RDF syntax name.
        #[arg(long)]
        format: Option<String>,
        /// Ontology IRI override.
        #[arg(long)]
        ontology: Option<String>,
        /// Target graph.
        #[arg(long)]
        graph: Option<String>,
    },
    /// Submit an external ontology synchronization.
    ExternalSync {
        /// External ontology IRI.
        id: String,
    },
    /// List jobs.
    List,
    /// Show one job.
    Get {
        /// Job IRI.
        id: String,
    },
    /// Retrieve a job's result.
    Results {
        /// Job IRI.
        id: String,
    },
    /// Request cancellation.
    Cancel {
        /// Job IRI.
        id: String,
    },
    /// Re-enqueue a failed or cancelled job.
    Retry {
        /// Job IRI.
        id: String,
    },
    /// Poll a job until it finishes.
    Watch {
        /// Job IRI.
        id: String,
        /// Poll interval in seconds.
        #[arg(long, default_value_t = 1)]
        interval_secs: u64,
    },
}

/// What the CLI should do after parsing.
#[derive(Debug)]
pub enum Action {
    /// Execute one application command.
    Command(Invocation),
    /// Poll a job.
    Watch {
        /// Job IRI.
        id: Id,
        /// Poll interval in seconds.
        interval_secs: u64,
    },
    /// Download a registry distribution artifact.
    Download {
        /// Release key.
        key: String,
        /// RDF syntax name.
        format: String,
        /// Destination path.
        output: PathBuf,
    },
    /// Start the HTTP server.
    Server(ServerArgs),
    /// Print the resolved configuration.
    Config,
    /// Start the interactive shell.
    Shell,
}

/// Registry operations.
#[derive(Debug, Subcommand)]
pub enum RegistryArgs {
    /// Catalog operations.
    Catalog {
        /// Operation.
        #[command(subcommand)]
        command: RegistryCatalogArgs,
    },
    /// Asset operations.
    Asset {
        /// Operation.
        #[command(subcommand)]
        command: RegistryAssetArgs,
    },
    /// Ingest a local file or URL.
    Ingest(RegistryIngestArgs),
    /// Candidate operations.
    Candidate {
        /// Operation.
        #[command(subcommand)]
        command: RegistryCandidateArgs,
    },
    /// Release operations.
    Release {
        /// Operation.
        #[command(subcommand)]
        command: RegistryReleaseArgs,
    },
    /// Profile operations.
    Profile {
        /// Operation.
        #[command(subcommand)]
        command: RegistryProfileArgs,
    },
    /// Search catalog metadata, terms, namespaces, and dependencies.
    Search(RegistrySearchArgs),
    /// Download a distribution artifact.
    Download(RegistryDownloadArgs),
}

/// Catalog operations.
#[derive(Debug, Subcommand)]
pub enum RegistryCatalogArgs {
    /// List catalogs.
    List,
    /// Show one catalog.
    Show {
        /// Catalog IRI.
        id: String,
    },
    /// Create a catalog.
    Create {
        /// Catalog title.
        title: String,
        /// Optional description.
        #[arg(long)]
        description: Option<String>,
    },
}

/// Asset operations.
#[derive(Debug, Subcommand)]
pub enum RegistryAssetArgs {
    /// List assets.
    List {
        /// Restrict to a catalog IRI.
        #[arg(long)]
        catalog: Option<String>,
    },
    /// Register an asset family.
    Register {
        /// Family IRI.
        asset_iri: String,
        /// Owning catalog IRI.
        #[arg(long)]
        catalog: Option<String>,
        /// Display title.
        #[arg(long)]
        title: Option<String>,
        /// Optional description.
        #[arg(long)]
        description: Option<String>,
        /// Declared namespace.
        #[arg(long)]
        namespace: Option<String>,
    },
    /// Show an asset by key or IRI.
    Show {
        /// Asset key or IRI.
        key: String,
    },
}

/// Ingestion options.
#[derive(Debug, Args)]
pub struct RegistryIngestArgs {
    /// Local path or HTTP(S) URL.
    pub source: String,
    /// Release version.
    #[arg(long)]
    pub version: Option<String>,
    /// Explicit family IRI.
    #[arg(long)]
    pub asset: Option<String>,
    /// RDF syntax name.
    #[arg(long)]
    pub format: Option<String>,
    /// Optional display title.
    #[arg(long)]
    pub title: Option<String>,
    /// Owning catalog IRI.
    #[arg(long)]
    pub catalog: Option<String>,
    /// Inline RDF document; otherwise the local file is read for local servers.
    #[arg(long = "data")]
    pub data: Option<String>,
}

/// Candidate operations.
#[derive(Debug, Subcommand)]
pub enum RegistryCandidateArgs {
    /// Show a candidate.
    Show { id: String },
    /// List candidates, optionally for an asset.
    List {
        #[arg(long)]
        asset: Option<String>,
    },
    /// Recompute analysis.
    Analyze { id: String },
    /// Resolve imports (currently the same as analysis).
    Resolve { id: String },
    /// Validate a candidate.
    Validate {
        id: String,
        #[arg(long)]
        profile: Option<String>,
    },
    /// Publish a candidate.
    Publish {
        id: String,
        #[arg(long)]
        channel: Option<String>,
    },
}

/// Release operations.
#[derive(Debug, Subcommand)]
pub enum RegistryReleaseArgs {
    /// List releases, optionally for an asset.
    List {
        #[arg(long)]
        asset: Option<String>,
    },
    /// Show one release.
    Show { key: String },
    /// List or search terms.
    Terms {
        key: String,
        #[arg(long)]
        query: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
    /// Show one term.
    Term { release: String, term: String },
    /// List imports.
    Imports { release: String },
    /// Show statistics.
    Stats { release: String },
    /// Compare two releases.
    Diff { left: String, right: String },
    /// List validation reports.
    Reports { release: String },
    /// Archive a release.
    Archive {
        key: String,
        #[arg(long)]
        successor: Option<String>,
    },
}

/// Profile operations.
#[derive(Debug, Subcommand)]
pub enum RegistryProfileArgs {
    /// List profiles.
    List,
    /// Create a profile.
    Create {
        id: String,
        title: String,
        #[arg(long)]
        description: Option<String>,
    },
}

/// Registry search options.
#[derive(Debug, Args)]
pub struct RegistrySearchArgs {
    /// Search text.
    pub query: String,
    /// Result kind.
    #[arg(long)]
    pub kind: Option<String>,
    #[arg(long, default_value_t = 100)]
    pub limit: usize,
    #[arg(long, default_value_t = 0)]
    pub offset: usize,
}

/// Distribution download options.
#[derive(Debug, Args)]
pub struct RegistryDownloadArgs {
    /// Release key.
    pub key: String,
    /// RDF syntax name.
    #[arg(long, default_value = "turtle")]
    pub format: String,
    /// Output path.
    #[arg(long)]
    pub output: PathBuf,
}

/// Project operations.
#[derive(Debug, Subcommand)]
pub enum ProjectArgs {
    /// Create a project from a `NewProject` JSON document.
    Create {
        #[command(flatten)]
        payload: PayloadArgs,
    },
    /// List projects.
    List,
    /// Show a project.
    Show { project: String },
    /// Update project metadata.
    Update {
        project: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        owner: Option<String>,
    },
    /// Goal operations.
    Goal {
        #[command(subcommand)]
        command: ProjectGoalArgs,
    },
    /// Requirement operations.
    Requirement {
        #[command(subcommand)]
        command: ProjectRequirementArgs,
    },
    /// Work operations.
    Work {
        #[command(subcommand)]
        command: ProjectWorkArgs,
    },
    /// Deliverable operations.
    Deliverable {
        #[command(subcommand)]
        command: ProjectDeliverableArgs,
    },
    /// Create a snapshot.
    Snapshot { project: String },
    /// Show the overview.
    Overview { project: String },
    /// Show the timeline.
    Timeline { project: String },
    /// Show dependency edges.
    Dependencies { project: String },
}

/// Goal operations.
#[derive(Debug, Subcommand)]
pub enum ProjectGoalArgs {
    /// Add a goal.
    Add {
        project: String,
        title: String,
        #[arg(long)]
        description: Option<String>,
    },
    /// List goals.
    List { project: String },
}

/// Requirement operations.
#[derive(Debug, Subcommand)]
pub enum ProjectRequirementArgs {
    /// Add a requirement.
    Add {
        project: String,
        title: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        kind: Option<String>,
    },
    /// List requirements.
    List { project: String },
}

/// Work operations.
#[derive(Debug, Subcommand)]
pub enum ProjectWorkArgs {
    /// Add a work item.
    Add {
        project: String,
        title: String,
        /// `task`, `work_package`, or `milestone`.
        #[arg(long, default_value = "task")]
        kind: String,
        #[arg(long)]
        parent: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        assignee: Option<String>,
        #[arg(long)]
        due: Option<chrono::DateTime<chrono::Utc>>,
    },
    /// List work items.
    List { project: String },
    /// Transition a work item.
    Transition {
        work: String,
        /// `start`, `block`, `unblock`, `complete`, or `cancel`.
        event: String,
    },
    /// Assign a work item.
    Assign { work: String, assignee: String },
}

/// Deliverable operations.
#[derive(Debug, Subcommand)]
pub enum ProjectDeliverableArgs {
    /// Add a deliverable.
    Add {
        project: String,
        title: String,
        #[arg(long = "kind")]
        kinds: Vec<String>,
        #[arg(long)]
        asset: Option<String>,
        #[arg(long)]
        release: Option<String>,
        #[arg(long = "criterion")]
        acceptance_criteria: Vec<String>,
    },
    /// List deliverables.
    List { project: String },
    /// Link to a published release.
    Link {
        deliverable: String,
        release: String,
    },
    /// Accept against registry evidence.
    Accept {
        deliverable: String,
        #[arg(long)]
        evidence: Option<String>,
        #[arg(long)]
        criterion: Option<String>,
    },
}

/// Translates parsed arguments into an action.
///
/// # Errors
///
/// Returns an error for malformed identifiers, payloads, or option values.
pub fn action(args: CommandArgs) -> CliResult<Action> {
    let invocation = match args {
        CommandArgs::Server(args) => return Ok(Action::Server(args)),
        CommandArgs::Config => return Ok(Action::Config),
        CommandArgs::Shell => return Ok(Action::Shell),
        CommandArgs::Instance { command } => Invocation::new(instance_command(command)?),
        CommandArgs::Rdf { command } => rdf_action(command)?,
        CommandArgs::Ontology { command } => Invocation::new(ontology_command(command)?),
        CommandArgs::Class { command } => Invocation::new(class_command(command)?),
        CommandArgs::Property { command } => Invocation::new(property_command(command)?),
        CommandArgs::Scheme { command } => Invocation::new(scheme_command(command)?),
        CommandArgs::Concept { command } => Invocation::new(concept_command(command)?),
        CommandArgs::Taxonomy { command } => Invocation::new(taxonomy_command(command)?),
        CommandArgs::Classify { command } => Invocation::new(classify_command(command)?),
        CommandArgs::Embedding { command } => Invocation::new(embedding_command(command)?),
        CommandArgs::External { command } => Invocation::new(external_command(command)?),
        CommandArgs::Inference { command } => Invocation::new(inference_command(command)?),
        CommandArgs::Document { command } => Invocation::new(document_command(command)?),
        CommandArgs::Registry { command } => return registry_action(command),
        CommandArgs::Project { command } => Invocation::new(project_command(command)?),
        CommandArgs::Import(args) => Invocation::new(import_command(args)?),
        CommandArgs::Export(args) => {
            let output = args.output.clone();
            Invocation::with_output(export_command(args)?, output)
        }
        CommandArgs::Job { command } => return job_action(command),
    };
    Ok(Action::Command(invocation))
}

fn registry_action(command: RegistryArgs) -> CliResult<Action> {
    match command {
        RegistryArgs::Download(args) => Ok(Action::Download {
            key: args.key,
            format: args.format,
            output: args.output,
        }),
        other => Ok(Action::Command(Invocation::new(registry_command(other)?))),
    }
}

fn registry_command(command: RegistryArgs) -> CliResult<Command> {
    let command = match command {
        RegistryArgs::Catalog { command } => match command {
            RegistryCatalogArgs::List => RegistryCommand::ListCatalogs,
            RegistryCatalogArgs::Show { id } => RegistryCommand::GetCatalog { id: parse_id(&id)? },
            RegistryCatalogArgs::Create { title, description } => {
                RegistryCommand::CreateCatalog { title, description }
            }
        },
        RegistryArgs::Asset { command } => match command {
            RegistryAssetArgs::List { catalog } => RegistryCommand::ListAssets {
                catalog: catalog.as_deref().map(parse_id).transpose()?,
            },
            RegistryAssetArgs::Register {
                asset_iri,
                catalog,
                title,
                description,
                namespace,
            } => RegistryCommand::RegisterAsset {
                catalog: catalog.as_deref().map(parse_id).transpose()?,
                asset_iri: parse_id(&asset_iri)?,
                title,
                description,
                namespace,
            },
            RegistryAssetArgs::Show { key } => RegistryCommand::GetAsset { key },
        },
        RegistryArgs::Ingest(args) => {
            let data = match args.data {
                Some(data) => Some(data),
                None if std::path::Path::new(&args.source).is_file() => {
                    Some(std::fs::read_to_string(&args.source)?)
                }
                None => None,
            };
            RegistryCommand::Ingest(IngestRequest {
                catalog: args.catalog.as_deref().map(parse_id).transpose()?,
                source: args.source,
                data,
                asset_iri: args.asset.as_deref().map(parse_id).transpose()?,
                version: args.version,
                format: args.format,
                title: args.title,
            })
        }
        RegistryArgs::Candidate { command } => match command {
            RegistryCandidateArgs::Show { id } => {
                RegistryCommand::GetCandidate { id: parse_id(&id)? }
            }
            RegistryCandidateArgs::List { asset } => RegistryCommand::ListCandidates {
                asset: asset.as_deref().map(parse_id).transpose()?,
            },
            RegistryCandidateArgs::Analyze { id } => RegistryCommand::Analyze {
                candidate: parse_id(&id)?,
            },
            RegistryCandidateArgs::Resolve { id } => RegistryCommand::Analyze {
                candidate: parse_id(&id)?,
            },
            RegistryCandidateArgs::Validate { id, profile } => RegistryCommand::Validate {
                candidate: parse_id(&id)?,
                profile: profile.as_deref().map(parse_id).transpose()?,
            },
            RegistryCandidateArgs::Publish { id, channel } => RegistryCommand::Publish {
                candidate: parse_id(&id)?,
                channel,
            },
        },
        RegistryArgs::Release { command } => match command {
            RegistryReleaseArgs::List { asset } => RegistryCommand::ListReleases { asset },
            RegistryReleaseArgs::Show { key } => RegistryCommand::GetRelease { key },
            RegistryReleaseArgs::Terms {
                key,
                query,
                limit,
                offset,
            } => RegistryCommand::ReleaseTerms {
                release: key,
                query,
                limit,
                offset,
            },
            RegistryReleaseArgs::Term { release, term } => {
                RegistryCommand::GetTerm { release, term }
            }
            RegistryReleaseArgs::Imports { release } => RegistryCommand::ReleaseImports { release },
            RegistryReleaseArgs::Stats { release } => {
                RegistryCommand::ReleaseStatistics { release }
            }
            RegistryReleaseArgs::Diff { left, right } => RegistryCommand::Diff { left, right },
            RegistryReleaseArgs::Reports { release } => RegistryCommand::ListReports { release },
            RegistryReleaseArgs::Archive { key, successor } => {
                RegistryCommand::ArchiveRelease { key, successor }
            }
        },
        RegistryArgs::Profile { command } => match command {
            RegistryProfileArgs::List => RegistryCommand::ListProfiles,
            RegistryProfileArgs::Create {
                id,
                title,
                description,
            } => RegistryCommand::CreateProfile {
                id: parse_id(&id)?,
                title,
                description,
            },
        },
        RegistryArgs::Search(args) => RegistryCommand::Search {
            query: args.query,
            kind: args.kind,
            limit: args.limit,
            offset: args.offset,
        },
        RegistryArgs::Download(_) => return Err("download is handled by the HTTP client".into()),
    };
    Ok(Command::Registry(command))
}

fn project_command(command: ProjectArgs) -> CliResult<Command> {
    let command = match command {
        ProjectArgs::Create { payload } => ProjectCommand::Create(payload.parse()?),
        ProjectArgs::List => ProjectCommand::List,
        ProjectArgs::Show { project } => ProjectCommand::Get {
            project: parse_id(&project)?,
        },
        ProjectArgs::Update {
            project,
            title,
            description,
            owner,
        } => ProjectCommand::Update {
            project: parse_id(&project)?,
            title,
            description,
            owner,
        },
        ProjectArgs::Goal { command } => match command {
            ProjectGoalArgs::Add {
                project,
                title,
                description,
            } => ProjectCommand::AddGoal {
                project: parse_id(&project)?,
                title,
                description,
            },
            ProjectGoalArgs::List { project } => ProjectCommand::ListGoals {
                project: parse_id(&project)?,
            },
        },
        ProjectArgs::Requirement { command } => match command {
            ProjectRequirementArgs::Add {
                project,
                title,
                description,
                kind,
            } => ProjectCommand::AddRequirement {
                project: parse_id(&project)?,
                title,
                description,
                kind,
            },
            ProjectRequirementArgs::List { project } => ProjectCommand::ListRequirements {
                project: parse_id(&project)?,
            },
        },
        ProjectArgs::Work { command } => match command {
            ProjectWorkArgs::Add {
                project,
                title,
                kind,
                parent,
                description,
                assignee,
                due,
            } => ProjectCommand::AddWork {
                project: parse_id(&project)?,
                parent: parent.as_deref().map(parse_id).transpose()?,
                kind: WorkKind::parse(&kind)
                    .ok_or_else(|| format!("unknown work kind `{kind}`"))?,
                title,
                description,
                assignee,
                due,
            },
            ProjectWorkArgs::List { project } => ProjectCommand::ListWork {
                project: parse_id(&project)?,
            },
            ProjectWorkArgs::Transition { work, event } => ProjectCommand::Transition {
                work: parse_id(&work)?,
                event: TaskEvent::parse(&event)
                    .ok_or_else(|| format!("unknown task event `{event}`"))?,
            },
            ProjectWorkArgs::Assign { work, assignee } => ProjectCommand::Assign {
                work: parse_id(&work)?,
                assignee,
            },
        },
        ProjectArgs::Deliverable { command } => match command {
            ProjectDeliverableArgs::Add {
                project,
                title,
                kinds,
                asset,
                release,
                acceptance_criteria,
            } => ProjectCommand::AddDeliverable {
                project: parse_id(&project)?,
                title,
                kinds,
                asset: asset.as_deref().map(parse_id).transpose()?,
                release: release.as_deref().map(parse_id).transpose()?,
                acceptance_criteria,
            },
            ProjectDeliverableArgs::List { project } => ProjectCommand::ListDeliverables {
                project: parse_id(&project)?,
            },
            ProjectDeliverableArgs::Link {
                deliverable,
                release,
            } => ProjectCommand::LinkDeliverable {
                deliverable: parse_id(&deliverable)?,
                release,
            },
            ProjectDeliverableArgs::Accept {
                deliverable,
                evidence,
                criterion,
            } => ProjectCommand::Accept {
                deliverable: parse_id(&deliverable)?,
                evidence,
                criterion,
            },
        },
        ProjectArgs::Snapshot { project } => ProjectCommand::Snapshot {
            project: parse_id(&project)?,
        },
        ProjectArgs::Overview { project } => ProjectCommand::Overview {
            project: parse_id(&project)?,
        },
        ProjectArgs::Timeline { project } => ProjectCommand::Timeline {
            project: parse_id(&project)?,
        },
        ProjectArgs::Dependencies { project } => ProjectCommand::Dependencies {
            project: parse_id(&project)?,
        },
    };
    Ok(Command::Project(command))
}

fn rdf_action(command: RdfArgs) -> CliResult<Invocation> {
    match command {
        RdfArgs::Query(args) => Ok(Invocation::new(Command::Rdf(RdfCommand::Query {
            sparql: args.text()?,
        }))),
        RdfArgs::Update(args) => Ok(Invocation::new(Command::Rdf(RdfCommand::Update {
            sparql: args.text()?,
        }))),
        RdfArgs::Graphs => Ok(Invocation::new(Command::Rdf(RdfCommand::ListGraphs))),
        RdfArgs::Import(args) => Ok(Invocation::new(import_command(args)?)),
        RdfArgs::Export(args) => Ok(Invocation::with_output(
            export_command(ExportArgs {
                format: args.format,
                graph: args.graph,
                output: args.output.clone(),
            })?,
            args.output,
        )),
        RdfArgs::ClearGraph { graph } => {
            Ok(Invocation::new(Command::Rdf(RdfCommand::ClearGraph {
                graph: GraphName::Iri(parse_id(&graph)?),
            })))
        }
    }
}

fn document_command(command: DocumentArgs) -> CliResult<Command> {
    match command {
        DocumentArgs::Extract(args) => Ok(Command::Document(DocumentCommand::Extract(
            ExtractionRequest {
                source: args.file,
                mode: parse_extraction_mode(args.mode.as_deref())?,
                text_format: parse_text_format(args.format.as_deref())?,
                output_dir: args.output_dir,
                force: args.force,
            },
        ))),
        DocumentArgs::Get { id } => Ok(Command::Document(DocumentCommand::Get { id })),
    }
}

fn parse_extraction_mode(value: Option<&str>) -> CliResult<Option<ExtractionMode>> {
    value
        .map(|value| match value.to_ascii_lowercase().as_str() {
            "structured" => Ok(ExtractionMode::Structured),
            "text" => Ok(ExtractionMode::Text),
            "both" => Ok(ExtractionMode::Both),
            other => Err(format!("unknown mode {other:?}; use structured, text, or both").into()),
        })
        .transpose()
}

fn parse_text_format(value: Option<&str>) -> CliResult<Option<TextFormat>> {
    value
        .map(|value| match value.to_ascii_lowercase().as_str() {
            "markdown" => Ok(TextFormat::Markdown),
            "plain" => Ok(TextFormat::Plain),
            other => Err(format!("unknown format {other:?}; use markdown or plain").into()),
        })
        .transpose()
}

fn import_command(args: ImportArgs) -> CliResult<Command> {
    let data = read_text(&args.data, &args.file, args.stdin)?;
    Ok(Command::Rdf(RdfCommand::Import {
        format: args.format,
        data,
        graph: parse_graph(&args.graph)?,
    }))
}

fn export_command(args: ExportArgs) -> CliResult<Command> {
    Ok(Command::Rdf(RdfCommand::Export {
        format: args.format,
        graph: parse_graph(&args.graph)?,
    }))
}

fn instance_command(command: InstanceCommand) -> CliResult<Command> {
    let command = match command {
        InstanceCommand::Create { payload } => {
            let new: NewInstance = payload.parse()?;
            RdfCommand::CreateInstance(new)
        }
        InstanceCommand::Get { id, graph } => RdfCommand::GetInstance {
            id: parse_id(&id)?,
            graph: parse_graph(&graph)?,
        },
        InstanceCommand::List {
            graph,
            r#type,
            limit,
            offset,
        } => {
            let mut query = InstanceQuery::default();
            query.graph = parse_graph(&graph)?;
            if let Some(value) = r#type {
                query.type_filter = Some(parse_id(&value)?);
            }
            query.limit = limit;
            query.offset = offset;
            RdfCommand::ListInstances(query)
        }
        InstanceCommand::Update { id, payload, graph } => {
            let patch: InstancePatch = payload.parse()?;
            RdfCommand::UpdateInstance {
                id: parse_id(&id)?,
                patch,
                graph: parse_graph(&graph)?,
                source: None,
            }
        }
        InstanceCommand::Delete { id, graph } => RdfCommand::DeleteInstance {
            id: parse_id(&id)?,
            graph: parse_graph(&graph)?,
        },
        InstanceCommand::Statements { id, graph } => RdfCommand::ListStatements {
            id: parse_id(&id)?,
            graph: parse_graph(&graph)?,
        },
        InstanceCommand::AddStatements {
            id,
            payload,
            graph,
            source,
        } => {
            let statements: Vec<StatementInput> = payload.parse()?;
            RdfCommand::AddStatements {
                id: parse_id(&id)?,
                statements,
                graph: parse_graph(&graph)?,
                source,
            }
        }
        InstanceCommand::DeleteStatements {
            id,
            statement_ids,
            graph,
        } => RdfCommand::DeleteStatements {
            id: parse_id(&id)?,
            statement_ids: statement_ids
                .iter()
                .map(|value| parse_statement_id(value))
                .collect::<CliResult<Vec<StatementId>>>()?,
            graph: parse_graph(&graph)?,
        },
        InstanceCommand::Relationships { id } => {
            RdfCommand::GetRelationships { id: parse_id(&id)? }
        }
    };
    Ok(Command::Rdf(command))
}

fn ontology_command(command: OntologyArgs) -> CliResult<Command> {
    let command = match command {
        OntologyArgs::List => OntologyCommand::ListOntologies,
        OntologyArgs::Get { id } => OntologyCommand::GetOntology { id: parse_id(&id)? },
        OntologyArgs::Create { payload } => {
            OntologyCommand::CreateOntology(payload.parse::<Ontology>()?)
        }
        OntologyArgs::Update { id, payload } => {
            let mut ontology: Ontology = payload.parse()?;
            ontology.id = parse_id(&id)?;
            OntologyCommand::UpdateOntology(ontology)
        }
        OntologyArgs::Delete { id } => OntologyCommand::DeleteOntology { id: parse_id(&id)? },
        OntologyArgs::Versions { id } => OntologyCommand::OntologyVersions { id: parse_id(&id)? },
        OntologyArgs::Validate { graph } => OntologyCommand::Validate {
            scope: graph_scope(&graph)?,
        },
        OntologyArgs::ApplicableProperties { class } => OntologyCommand::ApplicableProperties {
            class: parse_id(&class)?,
        },
        OntologyArgs::ApplicableClasses { properties } => OntologyCommand::ApplicableClasses {
            properties: properties
                .iter()
                .map(|value| parse_id(value))
                .collect::<CliResult<Vec<Id>>>()?,
        },
    };
    Ok(Command::Ontology(command))
}

fn class_command(command: ClassArgs) -> CliResult<Command> {
    let command = match command {
        ClassArgs::List {
            parent,
            label,
            limit,
            offset,
        } => {
            let mut query = ClassQuery::default();
            if let Some(parent) = parent {
                query.parent = Some(parse_id(&parent)?);
            }
            query.label_contains = label;
            query.limit = limit;
            query.offset = offset;
            OntologyCommand::ListClasses(query)
        }
        ClassArgs::Get { id } => OntologyCommand::GetClass { id: parse_id(&id)? },
        ClassArgs::Create { payload } => OntologyCommand::CreateClass(payload.parse::<OwlClass>()?),
        ClassArgs::Update { id, payload } => {
            let mut class: OwlClass = payload.parse()?;
            class.id = parse_id(&id)?;
            OntologyCommand::UpdateClass(class)
        }
        ClassArgs::Delete { id } => OntologyCommand::DeleteClass { id: parse_id(&id)? },
        ClassArgs::Hierarchy { id } => OntologyCommand::ClassHierarchy { id: parse_id(&id)? },
    };
    Ok(Command::Ontology(command))
}

fn property_command(command: PropertyArgs) -> CliResult<Command> {
    let command = match command {
        PropertyArgs::List {
            kind,
            label,
            limit,
            offset,
        } => {
            let mut query = PropertyQuery::new();
            if let Some(kind) = kind {
                query.kind = Some(parse_property_kind(&kind)?);
            }
            query.label_contains = label;
            query.limit = limit;
            query.offset = offset;
            OntologyCommand::ListProperties(query)
        }
        PropertyArgs::Get { id } => OntologyCommand::GetProperty { id: parse_id(&id)? },
        PropertyArgs::Create { payload } => {
            OntologyCommand::CreateProperty(payload.parse::<OwlProperty>()?)
        }
        PropertyArgs::Update { id, payload } => {
            let mut property: OwlProperty = payload.parse()?;
            property.id = parse_id(&id)?;
            OntologyCommand::UpdateProperty(property)
        }
        PropertyArgs::Delete { id } => OntologyCommand::DeleteProperty { id: parse_id(&id)? },
    };
    Ok(Command::Ontology(command))
}

fn scheme_command(command: SchemeArgs) -> CliResult<Command> {
    let command = match command {
        SchemeArgs::List => TaxonomyCommand::ListSchemes,
        SchemeArgs::Get { id } => TaxonomyCommand::GetScheme { id: parse_id(&id)? },
        SchemeArgs::Create { payload } => {
            TaxonomyCommand::CreateScheme(payload.parse::<ConceptScheme>()?)
        }
        SchemeArgs::Update { id, payload } => {
            let mut scheme: ConceptScheme = payload.parse()?;
            scheme.id = parse_id(&id)?;
            TaxonomyCommand::UpdateScheme(scheme)
        }
        SchemeArgs::Delete { id } => TaxonomyCommand::DeleteScheme { id: parse_id(&id)? },
        SchemeArgs::Versions { id } => TaxonomyCommand::SchemeVersions { id: parse_id(&id)? },
    };
    Ok(Command::Taxonomy(command))
}

fn concept_command(command: ConceptArgs) -> CliResult<Command> {
    let command = match command {
        ConceptArgs::List {
            scheme,
            label,
            limit,
            offset,
        } => {
            let mut query = TaxonomyQuery::new();
            if let Some(scheme) = scheme {
                query.scheme = Some(parse_id(&scheme)?);
            }
            query.label_contains = label;
            query.limit = limit;
            query.offset = offset;
            TaxonomyCommand::ListConcepts(query)
        }
        ConceptArgs::Get { id } => TaxonomyCommand::GetConcept { id: parse_id(&id)? },
        ConceptArgs::Create { payload } => {
            TaxonomyCommand::CreateConcept(payload.parse::<Concept>()?)
        }
        ConceptArgs::Update { id, payload } => {
            let mut concept: Concept = payload.parse()?;
            concept.id = parse_id(&id)?;
            TaxonomyCommand::UpdateConcept(concept)
        }
        ConceptArgs::Delete { id } => TaxonomyCommand::DeleteConcept { id: parse_id(&id)? },
        ConceptArgs::Search { query, scheme } => TaxonomyCommand::SearchConcepts {
            query,
            scheme: scheme.as_deref().map(parse_id).transpose()?,
        },
        ConceptArgs::Relations { id } => TaxonomyCommand::ListRelations { id: parse_id(&id)? },
        ConceptArgs::Descendants { id } => TaxonomyCommand::Descendants { id: parse_id(&id)? },
        ConceptArgs::Ancestors { id } => TaxonomyCommand::Ancestors { id: parse_id(&id)? },
        ConceptArgs::Move {
            id,
            broader,
            scheme,
        } => TaxonomyCommand::MoveConcept {
            id: parse_id(&id)?,
            broader: broader.as_deref().map(parse_id).transpose()?,
            scheme: scheme.as_deref().map(parse_id).transpose()?,
        },
    };
    Ok(Command::Taxonomy(command))
}

fn taxonomy_command(command: TaxonomyArgs) -> CliResult<Command> {
    let command = match command {
        TaxonomyArgs::Tree { scheme } => TaxonomyCommand::ConceptTree {
            scheme: parse_id(&scheme)?,
        },
        TaxonomyArgs::Merge { source, target } => TaxonomyCommand::MergeSchemes {
            source: parse_id(&source)?,
            target: parse_id(&target)?,
        },
        TaxonomyArgs::Validate { scheme } => TaxonomyCommand::ValidateTaxonomy {
            scheme: scheme.as_deref().map(parse_id).transpose()?,
        },
        TaxonomyArgs::AddRelation {
            source,
            relation,
            target,
        } => TaxonomyCommand::AddRelation {
            source: parse_id(&source)?,
            relation: parse_relation(&relation)?,
            target: parse_id(&target)?,
        },
        TaxonomyArgs::RemoveRelation {
            source,
            relation,
            target,
        } => TaxonomyCommand::RemoveRelation {
            source: parse_id(&source)?,
            relation: parse_relation(&relation)?,
            target: parse_id(&target)?,
        },
        TaxonomyArgs::Collections { scheme } => TaxonomyCommand::ListCollections {
            scheme: scheme.as_deref().map(parse_id).transpose()?,
        },
        TaxonomyArgs::CreateCollection { payload } => {
            TaxonomyCommand::CreateCollection(payload.parse::<ConceptCollection>()?)
        }
        TaxonomyArgs::GetCollection { id } => TaxonomyCommand::GetCollection { id: parse_id(&id)? },
        TaxonomyArgs::DeleteCollection { id } => {
            TaxonomyCommand::DeleteCollection { id: parse_id(&id)? }
        }
    };
    Ok(Command::Taxonomy(command))
}

fn classify_command(command: ClassifyArgs) -> CliResult<Command> {
    let command = match command {
        ClassifyArgs::Run { instance, options } => SemanticCommand::Classify {
            instance: parse_id(&instance)?,
            options: options.options()?,
        },
        ClassifyArgs::Batch { instances, options } => SemanticCommand::ClassifyBatch {
            instances: parse_ids(&instances)?,
            options: options.options()?,
        },
        ClassifyArgs::List { instance } => SemanticCommand::ListClassifications {
            instance: parse_id(&instance)?,
        },
        ClassifyArgs::Override {
            instance,
            concept,
            confidence,
        } => SemanticCommand::OverrideClassification {
            instance: parse_id(&instance)?,
            concept: parse_id(&concept)?,
            confidence,
        },
        ClassifyArgs::Review {
            classification,
            reject,
        } => SemanticCommand::ReviewClassification {
            classification: parse_id(&classification)?,
            approved: !reject,
        },
        ClassifyArgs::Reclassify { instance, options } => SemanticCommand::Reclassify {
            instance: parse_id(&instance)?,
            options: options.options()?,
        },
    };
    Ok(Command::Semantic(command))
}

fn embedding_command(command: EmbeddingArgs) -> CliResult<Command> {
    let command = match command {
        EmbeddingArgs::Generate {
            instance,
            attributes,
        } => SemanticCommand::GenerateEmbeddings {
            instance: parse_id(&instance)?,
            attributes: parse_ids(&attributes)?,
        },
        EmbeddingArgs::List { instance } => SemanticCommand::ListEmbeddings {
            instance: parse_id(&instance)?,
        },
        EmbeddingArgs::Search { instance, limit } => SemanticCommand::SearchSimilar {
            instance: parse_id(&instance)?,
            limit,
            filter: None,
        },
        EmbeddingArgs::Delete { instance } => SemanticCommand::DeleteEmbeddings {
            instance: parse_id(&instance)?,
        },
    };
    Ok(Command::Semantic(command))
}

fn external_command(command: ExternalArgs) -> CliResult<Command> {
    let command = match command {
        ExternalArgs::Import {
            source,
            format,
            ontology,
            graph,
        } => {
            let mut request = ImportRequest::new(source);
            request.format = format;
            if let Some(ontology) = ontology {
                request.ontology = Some(parse_id(&ontology)?);
            }
            request.graph = parse_graph(&graph)?;
            ExternalCommand::Import(request)
        }
        ExternalArgs::List => ExternalCommand::List,
        ExternalArgs::Get { id } => ExternalCommand::Get { id: parse_id(&id)? },
        ExternalArgs::Sync { id } => ExternalCommand::Sync { id: parse_id(&id)? },
        ExternalArgs::Delete { id } => ExternalCommand::Delete { id: parse_id(&id)? },
        ExternalArgs::MapClass { payload } => {
            ExternalCommand::MapClass(payload.parse::<ClassMapping>()?)
        }
        ExternalArgs::MapProperty { payload } => {
            ExternalCommand::MapProperty(payload.parse::<PropertyMapping>()?)
        }
        ExternalArgs::ClassMappings {
            external,
            internal,
            limit,
            offset,
        } => {
            let mut query = mapping_query(external, internal)?;
            query.limit = limit;
            query.offset = offset;
            ExternalCommand::ListClassMappings(query)
        }
        ExternalArgs::PropertyMappings {
            external,
            internal,
            limit,
            offset,
        } => {
            let mut query = mapping_query(external, internal)?;
            query.limit = limit;
            query.offset = offset;
            ExternalCommand::ListPropertyMappings(query)
        }
        ExternalArgs::ValidateMappings => ExternalCommand::ValidateMappings,
    };
    Ok(Command::External(command))
}

fn mapping_query(external: Option<String>, internal: Option<String>) -> CliResult<MappingQuery> {
    let mut query = MappingQuery::default();
    if let Some(value) = external {
        query.external = Some(parse_id(&value)?);
    }
    if let Some(value) = internal {
        query.internal = Some(parse_id(&value)?);
    }
    Ok(query)
}

fn inference_command(command: InferenceArgs) -> CliResult<Command> {
    let command = match command {
        InferenceArgs::Preview { instance, options } => SemanticCommand::PreviewInference {
            instance: parse_id(&instance)?,
            options: options.options()?,
        },
        InferenceArgs::Apply {
            instance,
            options,
            policy,
            threshold,
        } => SemanticCommand::ApplyInference {
            instance: parse_id(&instance)?,
            options: options.options()?,
            policy: parse_policy(&policy, threshold)?,
        },
        InferenceArgs::Explain { instance, options } => SemanticCommand::ExplainInference {
            instance: parse_id(&instance)?,
            options: options.options()?,
        },
    };
    Ok(Command::Semantic(command))
}

fn job_action(command: JobArgs) -> CliResult<Action> {
    let invocation = match command {
        JobArgs::Submit { payload } => {
            Invocation::new(Command::Job(JobCommand::Submit(payload.parse()?)))
        }
        JobArgs::Classify { instances, options } => Invocation::new(Command::Job(
            JobCommand::Submit(JobRequest::ClassifyBatch {
                instances: parse_ids(&instances)?,
                options: options.options()?,
            }),
        )),
        JobArgs::Embed {
            instances,
            attributes,
        } => Invocation::new(Command::Job(JobCommand::Submit(JobRequest::EmbedBatch {
            instances: parse_ids(&instances)?,
            attributes: parse_ids(&attributes)?,
        }))),
        JobArgs::Infer {
            instances,
            options,
            policy,
            threshold,
        } => Invocation::new(Command::Job(JobCommand::Submit(JobRequest::InferBatch {
            instances: parse_ids(&instances)?,
            options: options.options()?,
            policy: parse_policy(&policy, threshold)?,
        }))),
        JobArgs::Reclassify { instances, options } => Invocation::new(Command::Job(
            JobCommand::Submit(JobRequest::ReclassifyBatch {
                instances: parse_ids(&instances)?,
                options: options.options()?,
            }),
        )),
        JobArgs::Import(args) => {
            let data = read_text(&args.data, &args.file, args.stdin)?;
            Invocation::new(Command::Job(JobCommand::Submit(JobRequest::RdfImport {
                format: args.format,
                data,
                graph: parse_graph(&args.graph)?,
            })))
        }
        JobArgs::Export(args) => {
            Invocation::new(Command::Job(JobCommand::Submit(JobRequest::RdfExport {
                format: args.format,
                graph: parse_graph(&args.graph)?,
            })))
        }
        JobArgs::ExternalImport {
            source,
            format,
            ontology,
            graph,
        } => {
            let mut request = ImportRequest::new(source);
            request.format = format;
            if let Some(ontology) = ontology {
                request.ontology = Some(parse_id(&ontology)?);
            }
            request.graph = parse_graph(&graph)?;
            Invocation::new(Command::Job(JobCommand::Submit(
                JobRequest::ExternalImport(request),
            )))
        }
        JobArgs::ExternalSync { id } => {
            Invocation::new(Command::Job(JobCommand::Submit(JobRequest::ExternalSync {
                id: parse_id(&id)?,
            })))
        }
        JobArgs::List => Invocation::new(Command::Job(JobCommand::List)),
        JobArgs::Get { id } => {
            Invocation::new(Command::Job(JobCommand::Get { id: parse_id(&id)? }))
        }
        JobArgs::Results { id } => {
            Invocation::new(Command::Job(JobCommand::Results { id: parse_id(&id)? }))
        }
        JobArgs::Cancel { id } => {
            Invocation::new(Command::Job(JobCommand::Cancel { id: parse_id(&id)? }))
        }
        JobArgs::Retry { id } => {
            Invocation::new(Command::Job(JobCommand::Retry { id: parse_id(&id)? }))
        }
        JobArgs::Watch { id, interval_secs } => {
            return Ok(Action::Watch {
                id: parse_id(&id)?,
                interval_secs: interval_secs.max(1),
            })
        }
    };
    Ok(Action::Command(invocation))
}

fn parse_policy(value: &str, threshold: f64) -> CliResult<ApplyPolicy> {
    match value {
        "suggest" => Ok(ApplyPolicy::Suggest),
        "review" => Ok(ApplyPolicy::Review),
        "automatic" => Ok(ApplyPolicy::Automatic { threshold }),
        other => Err(format!("unknown apply policy `{other}`").into()),
    }
}

fn parse_property_kind(value: &str) -> CliResult<PropertyKind> {
    match value {
        "object" => Ok(PropertyKind::Object),
        "datatype" => Ok(PropertyKind::Datatype),
        "annotation" => Ok(PropertyKind::Annotation),
        other => Err(format!("unknown property kind `{other}`").into()),
    }
}

fn parse_relation(value: &str) -> CliResult<SkosRelation> {
    SkosRelation::parse(value).ok_or_else(|| format!("unknown SKOS relation `{value}`").into())
}

fn parse_graph(value: &Option<String>) -> CliResult<Option<GraphName>> {
    match value {
        None => Ok(None),
        Some(value) if value == "default" => Ok(Some(GraphName::Default)),
        Some(value) => Ok(Some(GraphName::Iri(parse_id(value)?))),
    }
}

fn graph_scope(value: &Option<String>) -> CliResult<GraphScope> {
    match value {
        None => Ok(GraphScope::Schema),
        Some(value) => Ok(GraphScope::Graph(GraphName::Iri(parse_id(value)?))),
    }
}

fn parse_id(value: &str) -> CliResult<Id> {
    Ok(Id::new(value)?)
}

fn parse_ids(values: &[String]) -> CliResult<Vec<Id>> {
    values.iter().map(|value| parse_id(value)).collect()
}

fn parse_statement_id(value: &str) -> CliResult<StatementId> {
    Ok(serde_json::from_value(serde_json::Value::String(
        value.to_owned(),
    ))?)
}

fn read_text(inline: &Option<String>, file: &Option<PathBuf>, stdin: bool) -> CliResult<String> {
    match (inline, file, stdin) {
        (Some(text), None, false) => Ok(text.clone()),
        (None, Some(path), false) => Ok(std::fs::read_to_string(path)?),
        (None, None, true) => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text)?;
            Ok(text)
        }
        _ => Err("provide exactly one of inline text, --file, or --stdin".into()),
    }
}

/// Resolved client settings.
#[derive(Debug, Clone)]
pub struct Connection {
    /// API base URL.
    pub server: String,
    /// Optional bearer token.
    pub api_key: Option<String>,
    /// Result format.
    pub output: OutputFormat,
    /// Print commands instead of sending them.
    pub dry_run: bool,
    /// Skip destructive confirmations.
    pub assume_yes: bool,
}

impl Connection {
    /// Resolves settings from flags, environment, and the profile file.
    ///
    /// # Errors
    ///
    /// Returns an error when an explicitly selected profile is unknown or the
    /// profile file is malformed.
    pub fn resolve(cli: &Cli) -> CliResult<Self> {
        let profile_name = cli.profile.clone().unwrap_or_else(|| "default".to_owned());
        let profile = read_profile(&profile_name)?;
        if cli.profile.is_some() && profile.is_none() {
            return Err(format!("unknown profile `{profile_name}`").into());
        }
        let profile = profile.unwrap_or_default();
        Ok(Self {
            server: cli
                .server
                .clone()
                .or(profile.server)
                .unwrap_or_else(|| "http://127.0.0.1:8080".to_owned()),
            api_key: cli.api_key.clone().or(profile.api_key),
            output: cli.output.or(profile.output).unwrap_or(OutputFormat::Json),
            dry_run: cli.dry_run,
            assume_yes: cli.yes,
        })
    }
}

#[derive(Debug, Default, serde::Deserialize)]
struct ProfileFile {
    #[serde(default)]
    profiles: std::collections::BTreeMap<String, Profile>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct Profile {
    server: Option<String>,
    api_key: Option<String>,
    output: Option<OutputFormat>,
}

fn read_profile(name: &str) -> CliResult<Option<Profile>> {
    let path = match std::env::var_os("ONTO_CLI_CONFIG") {
        Some(path) => PathBuf::from(path),
        None => match std::env::var_os("HOME") {
            Some(home) => PathBuf::from(home).join(".config/onto/cli.toml"),
            None => return Ok(None),
        },
    };
    if !path.exists() {
        return Ok(None);
    }
    let document = std::fs::read_to_string(&path)?;
    let file: ProfileFile = toml::from_str(&document)?;
    Ok(file.profiles.get(name).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_inline_payloads() {
        let payload = PayloadArgs {
            json: Some(r#"{"id":"urn:example:x"}"#.to_owned()),
            file: None,
        };
        assert_eq!(payload.read().expect("read"), r#"{"id":"urn:example:x"}"#);
        let value: serde_json::Value = payload.parse().expect("parse");
        assert_eq!(value["id"], "urn:example:x");
    }

    #[test]
    fn rejects_missing_and_duplicate_payload_sources() {
        let missing = PayloadArgs {
            json: None,
            file: None,
        };
        assert!(missing.read().is_err());
        let duplicate = PayloadArgs {
            json: Some("{}".to_owned()),
            file: Some(PathBuf::from("payload.json")),
        };
        assert!(duplicate.read().is_err());
    }

    #[test]
    fn rejects_malformed_json() {
        let payload = PayloadArgs {
            json: Some("{".to_owned()),
            file: None,
        };
        assert!(payload.parse::<serde_json::Value>().is_err());
    }
}
