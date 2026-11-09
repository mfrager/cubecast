//! Translation from MCP tool arguments into application commands.
//!
//! This module is deliberately transport-free and synchronous so the mapping
//! can be unit tested without starting an MCP session.

use std::path::PathBuf;

use ontology_core::command::{
    Command, DocumentCommand, ExternalCommand, JobCommand, OntologyCommand, RdfCommand,
    SemanticCommand, TaxonomyCommand,
};
use ontology_core::document::{ExtractionMode, ExtractionRequest, TextFormat};
use ontology_core::error::{Error, Result};
use ontology_core::external::{
    ClassMapping, ImportRequest, MappingKind, MappingQuery, PropertyMapping,
};
use ontology_core::format::RdfFormat;
use ontology_core::id::Id;
use ontology_core::job::JobRequest;
use ontology_core::model::{
    GraphName, InstancePatch, InstanceQuery, NewInstance, StatementId, StatementInput,
};
use ontology_core::namespace::NamespaceRegistry;
use ontology_core::owl::{
    ClassQuery, GraphScope, Ontology, OwlClass, OwlProperty, PropertyKind, PropertyQuery,
};
use ontology_core::semantic::{
    ApplyPolicy, ClassificationOptions, ClassificationStrategy, InferenceOptions,
};
use ontology_core::skos::{Concept, ConceptScheme, SkosRelation, TaxonomyQuery};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::params::{CommonArgs, JobsAction, OntologyAction, RdfAction, SemanticAction};

/// Resolves prefixed names against the configured namespace registry.
#[derive(Debug, Clone)]
pub struct Resolver {
    namespaces: NamespaceRegistry,
}

impl Resolver {
    /// Creates a resolver.
    #[must_use]
    pub fn new(namespaces: NamespaceRegistry) -> Self {
        Self { namespaces }
    }

    /// Returns the underlying registry.
    #[must_use]
    pub fn namespaces(&self) -> &NamespaceRegistry {
        &self.namespaces
    }

    /// Resolves an IRI or prefixed name.
    ///
    /// # Errors
    ///
    /// Propagates namespace resolution failures.
    pub fn id(&self, value: &str) -> Result<Id> {
        self.namespaces.resolve(value)
    }

    /// Resolves a graph name; the literal `default` selects the default graph.
    ///
    /// # Errors
    ///
    /// Propagates namespace resolution failures.
    pub fn graph(&self, value: &str) -> Result<GraphName> {
        if value.trim() == "default" {
            Ok(GraphName::Default)
        } else {
            Ok(GraphName::Iri(self.id(value)?))
        }
    }

    fn optional_graph(&self, value: Option<&String>) -> Result<Option<GraphName>> {
        value.map(|value| self.graph(value)).transpose()
    }

    fn optional_id(&self, value: Option<&String>) -> Result<Option<Id>> {
        value.map(|value| self.id(value)).transpose()
    }
}

/// Limits and permissions used while translating a request.
pub struct DispatchContext<'a> {
    /// Namespace resolver.
    pub resolver: &'a Resolver,
    /// Maximum page size.
    pub max_results: usize,
    /// Whether classification may persist results.
    pub allow_write: bool,
}

impl DispatchContext<'_> {
    fn limit(&self, args: &CommonArgs) -> usize {
        args.limit.unwrap_or(self.max_results).min(self.max_results)
    }

    fn offset(&self, args: &CommonArgs) -> usize {
        args.offset.unwrap_or(0)
    }
}

fn required<'a>(value: Option<&'a String>, action: &str, field: &str) -> Result<&'a str> {
    value
        .map(String::as_str)
        .ok_or_else(|| Error::validation(format!("{action} requires {field:?}")))
}

fn body_with_id(body: Option<&Value>, target: Option<&String>, action: &str) -> Result<Value> {
    let Some(body) = body else {
        return Err(Error::validation(format!(
            "{action} requires a full resource in \"body\""
        )));
    };
    let mut body = body.clone();
    if let (Value::Object(map), Some(target)) = (&mut body, target) {
        if !map.contains_key("id") {
            map.insert("id".to_owned(), Value::String(target.clone()));
        }
    }
    Ok(body)
}

fn body_or_id<T: DeserializeOwned>(
    body: Option<&Value>,
    target: Option<&String>,
    action: &str,
) -> Result<T> {
    let value = body_with_id(body, target, action)?;
    serde_json::from_value(value).map_err(|error| Error::invalid(error.to_string()))
}

fn parse_body<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|error| Error::invalid(error.to_string()))
}

fn parse_ids(resolver: &Resolver, values: &[String]) -> Result<Vec<Id>> {
    values.iter().map(|value| resolver.id(value)).collect()
}

fn parse_strategy(value: &str) -> Result<ClassificationStrategy> {
    ClassificationStrategy::parse(value)
        .or_else(|| serde_json::from_value(Value::String(value.to_owned())).ok())
        .ok_or_else(|| Error::validation(format!("unknown classification strategy {value:?}")))
}

fn classification_options(
    args: &CommonArgs,
    context: &DispatchContext<'_>,
) -> Result<ClassificationOptions> {
    let mut strategies = Vec::new();
    for value in &args.strategies {
        strategies.push(parse_strategy(value)?);
    }
    if let Some(value) = &args.strategy {
        strategies.push(parse_strategy(value)?);
    }
    let threshold = args.threshold.unwrap_or(0.5);
    if !(0.0..=1.0).contains(&threshold) {
        return Err(Error::validation("threshold must be between 0.0 and 1.0"));
    }
    Ok(ClassificationOptions {
        scheme: context.resolver.optional_id(args.scheme.as_ref())?,
        strategies,
        threshold,
        max_results: args.max_results.unwrap_or(5).min(context.max_results),
        attributes: parse_ids(context.resolver, &args.attributes)?,
        language: args.language.clone(),
        include_ancestors: args.include_ancestors.unwrap_or(false),
        persist: context.allow_write && args.persist.unwrap_or(true),
        graph: context.resolver.optional_graph(args.graph.as_ref())?,
    })
}

fn inference_options(args: &CommonArgs, context: &DispatchContext<'_>) -> Result<InferenceOptions> {
    Ok(InferenceOptions {
        include_external: args.include_external.unwrap_or(true),
        include_classification: args.include_classification.unwrap_or(true),
        include_inherited: args.include_inherited.unwrap_or(true),
        rules: Vec::new(),
        graph: context.resolver.optional_graph(args.graph.as_ref())?,
        target_graph: context
            .resolver
            .optional_graph(args.target_graph.as_ref())?,
    })
}

fn apply_policy(args: &CommonArgs) -> Result<ApplyPolicy> {
    match args.policy.as_deref().unwrap_or("suggest") {
        "suggest" => Ok(ApplyPolicy::Suggest),
        "review" => Ok(ApplyPolicy::Review),
        "automatic" => Ok(ApplyPolicy::Automatic {
            threshold: args.policy_threshold.unwrap_or(0.8),
        }),
        other => Err(Error::validation(format!(
            "unknown inference policy {other:?}; use suggest, review, or automatic"
        ))),
    }
}

fn parse_relation(value: &str) -> Result<SkosRelation> {
    SkosRelation::parse(value)
        .or_else(|| serde_json::from_value(Value::String(value.to_owned())).ok())
        .ok_or_else(|| Error::validation(format!("unknown SKOS relation {value:?}")))
}

fn parse_mapping_kind(value: &str) -> Result<MappingKind> {
    MappingKind::parse(value)
        .or_else(|| serde_json::from_value(Value::String(value.to_owned())).ok())
        .ok_or_else(|| Error::validation(format!("unknown mapping kind {value:?}")))
}

fn parse_property_kind(value: &str) -> Result<PropertyKind> {
    match value.to_ascii_lowercase().as_str() {
        "object" | "objectproperty" => Ok(PropertyKind::Object),
        "datatype" | "datatypeproperty" => Ok(PropertyKind::Datatype),
        "annotation" | "annotationproperty" => Ok(PropertyKind::Annotation),
        other => Err(Error::validation(format!(
            "unknown property kind {other:?}; use object, datatype, or annotation"
        ))),
    }
}

/// Translates an `rdf` tool request.
///
/// # Errors
///
/// Returns [`Error::Validation`] for missing or malformed fields.
pub fn rdf_command(
    action: RdfAction,
    args: &CommonArgs,
    context: &DispatchContext<'_>,
) -> Result<Command> {
    let name = action.as_str();
    let command = match action {
        RdfAction::Get => RdfCommand::GetInstance {
            id: context
                .resolver
                .id(required(args.target.as_ref(), name, "target")?)?,
            graph: context.resolver.optional_graph(args.graph.as_ref())?,
        },
        RdfAction::Search | RdfAction::ListInstances => RdfCommand::ListInstances(InstanceQuery {
            graph: context.resolver.optional_graph(args.graph.as_ref())?,
            type_filter: context.resolver.optional_id(args.type_filter.as_ref())?,
            limit: context.limit(args),
            offset: context.offset(args),
        }),
        RdfAction::Create => {
            let new: NewInstance = if let Some(body) = &args.body {
                body_or_id(Some(body), args.target.as_ref(), name)?
            } else {
                let id = context
                    .resolver
                    .id(required(args.target.as_ref(), name, "target")?)?;
                let types = parse_ids(context.resolver, &args.types)?;
                let mut instance = NewInstance::new(id);
                instance.types = types;
                instance.source = args.source.clone();
                if let Some(graph) = context.resolver.optional_graph(args.graph.as_ref())? {
                    instance.graph = Some(graph);
                }
                instance
            };
            RdfCommand::CreateInstance(new)
        }
        RdfAction::Update => {
            let patch: InstancePatch = parse_body(body_with_id(args.body.as_ref(), None, name)?)?;
            RdfCommand::UpdateInstance {
                id: context
                    .resolver
                    .id(required(args.target.as_ref(), name, "target")?)?,
                patch,
                graph: context.resolver.optional_graph(args.graph.as_ref())?,
                source: args.source.clone(),
            }
        }
        RdfAction::Delete => RdfCommand::DeleteInstance {
            id: context
                .resolver
                .id(required(args.target.as_ref(), name, "target")?)?,
            graph: context.resolver.optional_graph(args.graph.as_ref())?,
        },
        RdfAction::Query => RdfCommand::Query {
            sparql: required(args.sparql.as_ref(), name, "sparql")?.to_owned(),
        },
        RdfAction::SparqlUpdate => RdfCommand::Update {
            sparql: required(args.sparql.as_ref(), name, "sparql")?.to_owned(),
        },
        RdfAction::Import => {
            let format = args.format.clone().unwrap_or_else(|| "turtle".to_owned());
            format.parse::<RdfFormat>()?;
            let data = required(args.data.as_ref(), name, "data")?.to_owned();
            let graph = context.resolver.optional_graph(args.graph.as_ref())?;
            if args.background {
                return Ok(Command::Job(JobCommand::Submit(JobRequest::RdfImport {
                    format,
                    data,
                    graph,
                })));
            }
            RdfCommand::Import {
                format,
                data,
                graph,
            }
        }
        RdfAction::Export => {
            let format = args.format.clone().unwrap_or_else(|| "turtle".to_owned());
            format.parse::<RdfFormat>()?;
            let graph = context.resolver.optional_graph(args.graph.as_ref())?;
            if args.background {
                return Ok(Command::Job(JobCommand::Submit(JobRequest::RdfExport {
                    format,
                    graph,
                })));
            }
            RdfCommand::Export { format, graph }
        }
        RdfAction::Inspect => {
            if let Some(target) = &args.target {
                RdfCommand::GetRelationships {
                    id: context.resolver.id(target)?,
                }
            } else {
                RdfCommand::ListGraphs
            }
        }
        RdfAction::ListGraphs => RdfCommand::ListGraphs,
        RdfAction::Statements => RdfCommand::ListStatements {
            id: context
                .resolver
                .id(required(args.target.as_ref(), name, "target")?)?,
            graph: context.resolver.optional_graph(args.graph.as_ref())?,
        },
        RdfAction::AddStatements => {
            let statements: Vec<StatementInput> =
                parse_body(body_with_id(args.body.as_ref(), None, name)?)?;
            RdfCommand::AddStatements {
                id: context
                    .resolver
                    .id(required(args.target.as_ref(), name, "target")?)?,
                statements,
                graph: context.resolver.optional_graph(args.graph.as_ref())?,
                source: args.source.clone(),
            }
        }
        RdfAction::DeleteStatements => RdfCommand::DeleteStatements {
            id: context
                .resolver
                .id(required(args.target.as_ref(), name, "target")?)?,
            statement_ids: args
                .statement_ids
                .iter()
                .map(|value| StatementId::new(value.clone()))
                .collect::<Result<Vec<_>>>()?,
            graph: context.resolver.optional_graph(args.graph.as_ref())?,
        },
        RdfAction::ClearGraph => RdfCommand::ClearGraph {
            graph: context
                .resolver
                .graph(required(args.graph.as_ref(), name, "graph")?)?,
        },
        RdfAction::ExtractDocument => {
            let source = required(args.source.as_ref(), name, "source")?;
            return Ok(Command::Document(DocumentCommand::Extract(
                ExtractionRequest {
                    source: PathBuf::from(source),
                    mode: parse_extraction_mode(args.mode.as_deref())?,
                    text_format: parse_text_format(args.text_format.as_deref())?,
                    output_dir: None,
                    force: false,
                },
            )));
        }
    };
    Ok(Command::Rdf(command))
}

fn parse_extraction_mode(value: Option<&str>) -> Result<Option<ExtractionMode>> {
    value
        .map(|value| match value.to_ascii_lowercase().as_str() {
            "structured" => Ok(ExtractionMode::Structured),
            "text" => Ok(ExtractionMode::Text),
            "both" => Ok(ExtractionMode::Both),
            other => Err(Error::validation(format!(
                "unknown extraction mode {other:?}; use structured, text, or both"
            ))),
        })
        .transpose()
}

fn parse_text_format(value: Option<&str>) -> Result<Option<TextFormat>> {
    value
        .map(|value| match value.to_ascii_lowercase().as_str() {
            "markdown" => Ok(TextFormat::Markdown),
            "plain" => Ok(TextFormat::Plain),
            other => Err(Error::validation(format!(
                "unknown text format {other:?}; use markdown or plain"
            ))),
        })
        .transpose()
}

/// Translates an `ontology` tool request.
///
/// # Errors
///
/// Returns [`Error::Validation`] for missing or malformed fields.
pub fn ontology_command(
    action: OntologyAction,
    args: &CommonArgs,
    context: &DispatchContext<'_>,
) -> Result<Command> {
    match action {
        OntologyAction::Schema => schema_command(args, context),
        OntologyAction::Validate => {
            let scope = match context.resolver.optional_graph(args.graph.as_ref())? {
                Some(graph) => GraphScope::Graph(graph),
                None => GraphScope::Schema,
            };
            Ok(Command::Ontology(OntologyCommand::Validate { scope }))
        }
        OntologyAction::Scheme => scheme_command(args, context),
        OntologyAction::Concept => concept_command(args, context),
        OntologyAction::Taxonomy => taxonomy_command(args, context),
        OntologyAction::Search => search_command(args, context),
        OntologyAction::ExternalImport => {
            let mut request =
                ImportRequest::new(required(args.source.as_ref(), "external_import", "source")?);
            request.ontology = context.resolver.optional_id(args.ontology.as_ref())?;
            request.format = args.format.clone();
            request.graph = context.resolver.optional_graph(args.graph.as_ref())?;
            if args.background {
                return Ok(Command::Job(JobCommand::Submit(
                    JobRequest::ExternalImport(request),
                )));
            }
            Ok(Command::External(ExternalCommand::Import(request)))
        }
        OntologyAction::ExternalSync => {
            let id =
                context
                    .resolver
                    .id(required(args.target.as_ref(), "external_sync", "target")?)?;
            if args.background {
                return Ok(Command::Job(JobCommand::Submit(JobRequest::ExternalSync {
                    id,
                })));
            }
            Ok(Command::External(ExternalCommand::Sync { id }))
        }
        OntologyAction::Mapping => mapping_command(args, context),
        OntologyAction::Merge => {
            let source = context
                .resolver
                .id(required(args.source.as_ref(), "merge", "source")?)?;
            let target = context
                .resolver
                .id(required(args.target.as_ref(), "merge", "target")?)?;
            Ok(Command::Taxonomy(TaxonomyCommand::MergeSchemes {
                source,
                target,
            }))
        }
    }
}

fn schema_command(args: &CommonArgs, context: &DispatchContext<'_>) -> Result<Command> {
    let kind = required(args.kind.as_ref(), "schema", "kind")?.to_ascii_lowercase();
    let operation = args.operation.as_deref().unwrap_or("get");
    match (kind.as_str(), operation) {
        ("class", "get") => Ok(Command::Ontology(OntologyCommand::GetClass {
            id: context
                .resolver
                .id(required(args.target.as_ref(), "schema", "target")?)?,
        })),
        ("class", "list") => Ok(Command::Ontology(OntologyCommand::ListClasses(
            ClassQuery {
                parent: context.resolver.optional_id(args.parent.as_ref())?,
                label_contains: args.query.clone().or_else(|| args.label.clone()),
                limit: context.limit(args),
                offset: context.offset(args),
            },
        ))),
        ("class", "create") => {
            let class: OwlClass = if args.body.is_some() {
                body_or_id(args.body.as_ref(), args.target.as_ref(), "schema create")?
            } else {
                OwlClass::new(context.resolver.id(required(
                    args.target.as_ref(),
                    "schema create",
                    "target",
                )?)?)
            };
            Ok(Command::Ontology(OntologyCommand::CreateClass(class)))
        }
        ("class", "update") => {
            let class: OwlClass =
                body_or_id(args.body.as_ref(), args.target.as_ref(), "schema update")?;
            Ok(Command::Ontology(OntologyCommand::UpdateClass(class)))
        }
        ("class", "delete") => Ok(Command::Ontology(OntologyCommand::DeleteClass {
            id: context
                .resolver
                .id(required(args.target.as_ref(), "schema delete", "target")?)?,
        })),
        ("class", "hierarchy") => Ok(Command::Ontology(OntologyCommand::ClassHierarchy {
            id: context.resolver.id(required(
                args.target.as_ref(),
                "schema hierarchy",
                "target",
            )?)?,
        })),
        ("class", "applicable_properties" | "applicable-properties") => {
            Ok(Command::Ontology(OntologyCommand::ApplicableProperties {
                class: context.resolver.id(required(
                    args.class.as_ref().or(args.target.as_ref()),
                    "schema applicable_properties",
                    "class",
                )?)?,
            }))
        }
        ("class", "applicable_classes" | "applicable-classes") => {
            Ok(Command::Ontology(OntologyCommand::ApplicableClasses {
                properties: parse_ids(context.resolver, &args.properties)?,
            }))
        }
        ("property", "get") => Ok(Command::Ontology(OntologyCommand::GetProperty {
            id: context
                .resolver
                .id(required(args.target.as_ref(), "schema", "target")?)?,
        })),
        ("property", "list") => Ok(Command::Ontology(OntologyCommand::ListProperties(
            PropertyQuery {
                kind: args
                    .property_kind
                    .as_deref()
                    .map(parse_property_kind)
                    .transpose()?,
                label_contains: args.query.clone().or_else(|| args.label.clone()),
                limit: context.limit(args),
                offset: context.offset(args),
            },
        ))),
        ("property", "create") => {
            let property: OwlProperty = if args.body.is_some() {
                body_or_id(args.body.as_ref(), args.target.as_ref(), "schema create")?
            } else {
                let id = context.resolver.id(required(
                    args.target.as_ref(),
                    "schema create",
                    "target",
                )?)?;
                let kind = args
                    .property_kind
                    .as_deref()
                    .map(parse_property_kind)
                    .transpose()?
                    .unwrap_or(PropertyKind::Object);
                OwlProperty::new(id, kind)
            };
            Ok(Command::Ontology(OntologyCommand::CreateProperty(property)))
        }
        ("property", "update") => {
            let property: OwlProperty =
                body_or_id(args.body.as_ref(), args.target.as_ref(), "schema update")?;
            Ok(Command::Ontology(OntologyCommand::UpdateProperty(property)))
        }
        ("property", "delete") => Ok(Command::Ontology(OntologyCommand::DeleteProperty {
            id: context
                .resolver
                .id(required(args.target.as_ref(), "schema delete", "target")?)?,
        })),
        ("ontology", "get") => Ok(Command::Ontology(OntologyCommand::GetOntology {
            id: context
                .resolver
                .id(required(args.target.as_ref(), "schema", "target")?)?,
        })),
        ("ontology", "list") => Ok(Command::Ontology(OntologyCommand::ListOntologies)),
        ("ontology", "create") => {
            let ontology: Ontology = if args.body.is_some() {
                body_or_id(args.body.as_ref(), args.target.as_ref(), "schema create")?
            } else {
                Ontology::new(context.resolver.id(required(
                    args.target.as_ref(),
                    "schema create",
                    "target",
                )?)?)
            };
            Ok(Command::Ontology(OntologyCommand::CreateOntology(ontology)))
        }
        ("ontology", "update") => {
            let ontology: Ontology =
                body_or_id(args.body.as_ref(), args.target.as_ref(), "schema update")?;
            Ok(Command::Ontology(OntologyCommand::UpdateOntology(ontology)))
        }
        ("ontology", "delete") => Ok(Command::Ontology(OntologyCommand::DeleteOntology {
            id: context
                .resolver
                .id(required(args.target.as_ref(), "schema delete", "target")?)?,
        })),
        ("ontology", "versions") => Ok(Command::Ontology(OntologyCommand::OntologyVersions {
            id: context.resolver.id(required(
                args.target.as_ref(),
                "schema versions",
                "target",
            )?)?,
        })),
        (kind, operation) => Err(Error::validation(format!(
            "unsupported schema operation {operation:?} for kind {kind:?}"
        ))),
    }
}

fn scheme_command(args: &CommonArgs, context: &DispatchContext<'_>) -> Result<Command> {
    let operation = args.operation.as_deref().unwrap_or("list");
    match operation {
        "list" => Ok(Command::Taxonomy(TaxonomyCommand::ListSchemes)),
        "get" => Ok(Command::Taxonomy(TaxonomyCommand::GetScheme {
            id: context
                .resolver
                .id(required(args.target.as_ref(), "scheme", "target")?)?,
        })),
        "create" => {
            let scheme: ConceptScheme = if args.body.is_some() {
                body_or_id(args.body.as_ref(), args.target.as_ref(), "scheme create")?
            } else {
                let id = context.resolver.id(required(
                    args.target.as_ref(),
                    "scheme create",
                    "target",
                )?)?;
                let mut scheme = ConceptScheme::new(id);
                if let Some(label) = &args.label {
                    scheme
                        .labels
                        .insert(args.language.clone().unwrap_or_default(), label.clone());
                }
                scheme.top_concepts = parse_ids(context.resolver, &args.top_concepts)?;
                scheme
            };
            Ok(Command::Taxonomy(TaxonomyCommand::CreateScheme(scheme)))
        }
        "update" => {
            let scheme: ConceptScheme =
                body_or_id(args.body.as_ref(), args.target.as_ref(), "scheme update")?;
            Ok(Command::Taxonomy(TaxonomyCommand::UpdateScheme(scheme)))
        }
        "delete" => Ok(Command::Taxonomy(TaxonomyCommand::DeleteScheme {
            id: context
                .resolver
                .id(required(args.target.as_ref(), "scheme delete", "target")?)?,
        })),
        "versions" => Ok(Command::Taxonomy(TaxonomyCommand::SchemeVersions {
            id: context.resolver.id(required(
                args.target.as_ref(),
                "scheme versions",
                "target",
            )?)?,
        })),
        other => Err(Error::validation(format!(
            "unsupported scheme operation {other:?}"
        ))),
    }
}

fn concept_command(args: &CommonArgs, context: &DispatchContext<'_>) -> Result<Command> {
    let operation = args.operation.as_deref().unwrap_or("list");
    match operation {
        "list" => Ok(Command::Taxonomy(TaxonomyCommand::ListConcepts(
            TaxonomyQuery {
                scheme: context.resolver.optional_id(args.scheme.as_ref())?,
                label_contains: args.query.clone().or_else(|| args.label.clone()),
                limit: context.limit(args),
                offset: context.offset(args),
            },
        ))),
        "get" => Ok(Command::Taxonomy(TaxonomyCommand::GetConcept {
            id: context
                .resolver
                .id(required(args.target.as_ref(), "concept", "target")?)?,
        })),
        "create" => {
            let concept: Concept = if args.body.is_some() {
                body_or_id(args.body.as_ref(), args.target.as_ref(), "concept create")?
            } else {
                let id = context.resolver.id(required(
                    args.target.as_ref(),
                    "concept create",
                    "target",
                )?)?;
                let mut concept = Concept::new(id);
                concept.scheme = context.resolver.optional_id(args.scheme.as_ref())?;
                if let Some(label) = &args.label {
                    concept
                        .pref_labels
                        .push(ontology_core::skos::LocalizedText::new(
                            label,
                            args.language.as_deref().unwrap_or(""),
                        ));
                }
                concept.broader = parse_ids(context.resolver, &args.broader)?;
                concept
            };
            Ok(Command::Taxonomy(TaxonomyCommand::CreateConcept(concept)))
        }
        "update" => {
            let concept: Concept =
                body_or_id(args.body.as_ref(), args.target.as_ref(), "concept update")?;
            Ok(Command::Taxonomy(TaxonomyCommand::UpdateConcept(concept)))
        }
        "delete" => Ok(Command::Taxonomy(TaxonomyCommand::DeleteConcept {
            id: context
                .resolver
                .id(required(args.target.as_ref(), "concept delete", "target")?)?,
        })),
        "search" => Ok(Command::Taxonomy(TaxonomyCommand::SearchConcepts {
            query: required(args.query.as_ref(), "concept search", "query")?.to_owned(),
            scheme: context.resolver.optional_id(args.scheme.as_ref())?,
        })),
        other => Err(Error::validation(format!(
            "unsupported concept operation {other:?}"
        ))),
    }
}

fn taxonomy_command(args: &CommonArgs, context: &DispatchContext<'_>) -> Result<Command> {
    let operation = args.operation.as_deref().unwrap_or("tree");
    match operation {
        "tree" => Ok(Command::Taxonomy(TaxonomyCommand::ConceptTree {
            scheme: context.resolver.id(required(
                args.scheme.as_ref().or(args.target.as_ref()),
                "taxonomy tree",
                "scheme",
            )?)?,
        })),
        "ancestors" => Ok(Command::Taxonomy(TaxonomyCommand::Ancestors {
            id: context.resolver.id(required(
                args.target.as_ref(),
                "taxonomy ancestors",
                "target",
            )?)?,
        })),
        "descendants" => Ok(Command::Taxonomy(TaxonomyCommand::Descendants {
            id: context.resolver.id(required(
                args.target.as_ref(),
                "taxonomy descendants",
                "target",
            )?)?,
        })),
        "relations" => Ok(Command::Taxonomy(TaxonomyCommand::ListRelations {
            id: context.resolver.id(required(
                args.target.as_ref(),
                "taxonomy relations",
                "target",
            )?)?,
        })),
        "add_relation" => Ok(Command::Taxonomy(TaxonomyCommand::AddRelation {
            source: context.resolver.id(required(
                args.source.as_ref(),
                "taxonomy add_relation",
                "source",
            )?)?,
            relation: parse_relation(required(
                args.relation.as_ref(),
                "taxonomy add_relation",
                "relation",
            )?)?,
            target: context.resolver.id(required(
                args.target.as_ref(),
                "taxonomy add_relation",
                "target",
            )?)?,
        })),
        "remove_relation" => Ok(Command::Taxonomy(TaxonomyCommand::RemoveRelation {
            source: context.resolver.id(required(
                args.source.as_ref(),
                "taxonomy remove_relation",
                "source",
            )?)?,
            relation: parse_relation(required(
                args.relation.as_ref(),
                "taxonomy remove_relation",
                "relation",
            )?)?,
            target: context.resolver.id(required(
                args.target.as_ref(),
                "taxonomy remove_relation",
                "target",
            )?)?,
        })),
        "move" => Ok(Command::Taxonomy(TaxonomyCommand::MoveConcept {
            id: context
                .resolver
                .id(required(args.target.as_ref(), "taxonomy move", "target")?)?,
            broader: context.resolver.optional_id(args.broader.first())?,
            scheme: context.resolver.optional_id(args.scheme.as_ref())?,
        })),
        "validate" => Ok(Command::Taxonomy(TaxonomyCommand::ValidateTaxonomy {
            scheme: context.resolver.optional_id(args.scheme.as_ref())?,
        })),
        "merge" => Ok(Command::Taxonomy(TaxonomyCommand::MergeSchemes {
            source: context.resolver.id(required(
                args.source.as_ref(),
                "taxonomy merge",
                "source",
            )?)?,
            target: context.resolver.id(required(
                args.target.as_ref(),
                "taxonomy merge",
                "target",
            )?)?,
        })),
        other => Err(Error::validation(format!(
            "unsupported taxonomy operation {other:?}"
        ))),
    }
}

fn search_command(args: &CommonArgs, context: &DispatchContext<'_>) -> Result<Command> {
    let kind = required(args.kind.as_ref(), "search", "kind")?.to_ascii_lowercase();
    let text = args.query.clone().or_else(|| args.label.clone());
    match kind.as_str() {
        "class" => Ok(Command::Ontology(OntologyCommand::ListClasses(
            ClassQuery {
                parent: context.resolver.optional_id(args.parent.as_ref())?,
                label_contains: text,
                limit: context.limit(args),
                offset: context.offset(args),
            },
        ))),
        "property" => Ok(Command::Ontology(OntologyCommand::ListProperties(
            PropertyQuery {
                kind: args
                    .property_kind
                    .as_deref()
                    .map(parse_property_kind)
                    .transpose()?,
                label_contains: text,
                limit: context.limit(args),
                offset: context.offset(args),
            },
        ))),
        "concept" => Ok(Command::Taxonomy(TaxonomyCommand::SearchConcepts {
            query: text.unwrap_or_default(),
            scheme: context.resolver.optional_id(args.scheme.as_ref())?,
        })),
        "scheme" => Ok(Command::Taxonomy(TaxonomyCommand::ListSchemes)),
        other => Err(Error::validation(format!(
            "unsupported search kind {other:?}; use class, property, concept, or scheme"
        ))),
    }
}

fn mapping_query(args: &CommonArgs, context: &DispatchContext<'_>) -> Result<MappingQuery> {
    Ok(MappingQuery {
        external: context.resolver.optional_id(args.external.as_ref())?,
        internal: context.resolver.optional_id(args.internal.as_ref())?,
        limit: context.limit(args),
        offset: context.offset(args),
    })
}

fn mapping_command(args: &CommonArgs, context: &DispatchContext<'_>) -> Result<Command> {
    let operation = args.operation.as_deref().unwrap_or("validate");
    match operation {
        "map_class" => {
            let mapping: ClassMapping = if let Some(body) = &args.body {
                parse_body(body.clone())?
            } else {
                let external = context.resolver.id(required(
                    args.external.as_ref(),
                    "mapping map_class",
                    "external",
                )?)?;
                let internal = context.resolver.id(required(
                    args.internal.as_ref(),
                    "mapping map_class",
                    "internal",
                )?)?;
                let kind = parse_mapping_kind(required(
                    args.mapping_kind.as_ref(),
                    "mapping map_class",
                    "mapping_kind",
                )?)?;
                let mapping = ClassMapping::new(external, internal, kind)?;
                match args.confidence {
                    Some(confidence) => mapping.with_confidence(confidence)?,
                    None => mapping,
                }
            };
            Ok(Command::External(ExternalCommand::MapClass(mapping)))
        }
        "map_property" => {
            let mapping: PropertyMapping = if let Some(body) = &args.body {
                parse_body(body.clone())?
            } else {
                let external = context.resolver.id(required(
                    args.external.as_ref(),
                    "mapping map_property",
                    "external",
                )?)?;
                let internal = context.resolver.id(required(
                    args.internal.as_ref(),
                    "mapping map_property",
                    "internal",
                )?)?;
                let kind = parse_mapping_kind(required(
                    args.mapping_kind.as_ref(),
                    "mapping map_property",
                    "mapping_kind",
                )?)?;
                let mapping = PropertyMapping::new(external, internal, kind)?;
                match args.confidence {
                    Some(confidence) => mapping.with_confidence(confidence)?,
                    None => mapping,
                }
            };
            Ok(Command::External(ExternalCommand::MapProperty(mapping)))
        }
        "list_class" | "list_classes" => Ok(Command::External(ExternalCommand::ListClassMappings(
            mapping_query(args, context)?,
        ))),
        "list_property" | "list_properties" => Ok(Command::External(
            ExternalCommand::ListPropertyMappings(mapping_query(args, context)?),
        )),
        "validate" => Ok(Command::External(ExternalCommand::ValidateMappings)),
        other => Err(Error::validation(format!(
            "unsupported mapping operation {other:?}"
        ))),
    }
}

/// Translates a `semantic` tool request.
///
/// # Errors
///
/// Returns [`Error::Validation`] for missing or malformed fields.
pub fn semantic_command(
    action: SemanticAction,
    args: &CommonArgs,
    context: &DispatchContext<'_>,
) -> Result<Command> {
    let name = action.as_str();
    match action {
        SemanticAction::Classify => {
            let instance = context
                .resolver
                .id(required(args.target.as_ref(), name, "target")?)?;
            let options = classification_options(args, context)?;
            if args.background {
                return Ok(Command::Job(JobCommand::Submit(
                    JobRequest::ClassifyBatch {
                        instances: vec![instance],
                        options,
                    },
                )));
            }
            Ok(Command::Semantic(SemanticCommand::Classify {
                instance,
                options,
            }))
        }
        SemanticAction::ClassifyBatch => {
            let instances = parse_ids(context.resolver, &args.instances)?;
            if instances.is_empty() {
                return Err(Error::validation("classify_batch requires \"instances\""));
            }
            let options = classification_options(args, context)?;
            if args.background {
                return Ok(Command::Job(JobCommand::Submit(
                    JobRequest::ClassifyBatch { instances, options },
                )));
            }
            Ok(Command::Semantic(SemanticCommand::ClassifyBatch {
                instances,
                options,
            }))
        }
        SemanticAction::Embed | SemanticAction::Reembed => {
            let instance = context
                .resolver
                .id(required(args.target.as_ref(), name, "target")?)?;
            let attributes = parse_ids(context.resolver, &args.attributes)?;
            if args.background {
                return Ok(Command::Job(JobCommand::Submit(JobRequest::EmbedBatch {
                    instances: vec![instance],
                    attributes,
                })));
            }
            Ok(Command::Semantic(SemanticCommand::GenerateEmbeddings {
                instance,
                attributes,
            }))
        }
        SemanticAction::Similarity => Ok(Command::Semantic(SemanticCommand::SearchSimilar {
            instance: context
                .resolver
                .id(required(args.target.as_ref(), name, "target")?)?,
            limit: context.limit(args),
            filter: None,
        })),
        SemanticAction::Infer | SemanticAction::Preview => {
            Ok(Command::Semantic(SemanticCommand::PreviewInference {
                instance: context
                    .resolver
                    .id(required(args.target.as_ref(), name, "target")?)?,
                options: inference_options(args, context)?,
            }))
        }
        SemanticAction::Apply => {
            let instance = context
                .resolver
                .id(required(args.target.as_ref(), name, "target")?)?;
            let options = inference_options(args, context)?;
            let policy = apply_policy(args)?;
            if args.background {
                return Ok(Command::Job(JobCommand::Submit(JobRequest::InferBatch {
                    instances: vec![instance],
                    options,
                    policy,
                })));
            }
            Ok(Command::Semantic(SemanticCommand::ApplyInference {
                instance,
                options,
                policy,
            }))
        }
        SemanticAction::Explain => Ok(Command::Semantic(SemanticCommand::ExplainInference {
            instance: context
                .resolver
                .id(required(args.target.as_ref(), name, "target")?)?,
            options: inference_options(args, context)?,
        })),
        SemanticAction::Reclassify => {
            let instances = if args.instances.is_empty() {
                vec![context
                    .resolver
                    .id(required(args.target.as_ref(), name, "target")?)?]
            } else {
                parse_ids(context.resolver, &args.instances)?
            };
            let options = classification_options(args, context)?;
            if args.background || instances.len() > 1 {
                return Ok(Command::Job(JobCommand::Submit(
                    JobRequest::ReclassifyBatch { instances, options },
                )));
            }
            Ok(Command::Semantic(SemanticCommand::Reclassify {
                instance: instances.into_iter().next().expect("one instance"),
                options,
            }))
        }
        SemanticAction::Classifications => {
            Ok(Command::Semantic(SemanticCommand::ListClassifications {
                instance: context
                    .resolver
                    .id(required(args.target.as_ref(), name, "target")?)?,
            }))
        }
        SemanticAction::Embeddings => Ok(Command::Semantic(SemanticCommand::ListEmbeddings {
            instance: context
                .resolver
                .id(required(args.target.as_ref(), name, "target")?)?,
        })),
        SemanticAction::DeleteEmbeddings => {
            Ok(Command::Semantic(SemanticCommand::DeleteEmbeddings {
                instance: context
                    .resolver
                    .id(required(args.target.as_ref(), name, "target")?)?,
            }))
        }
        SemanticAction::Review => Ok(Command::Semantic(SemanticCommand::ReviewClassification {
            classification: context.resolver.id(required(
                args.id.as_ref().or(args.target.as_ref()),
                name,
                "id",
            )?)?,
            approved: args
                .approved
                .ok_or_else(|| Error::validation("review requires \"approved\""))?,
        })),
        SemanticAction::Override => {
            Ok(Command::Semantic(SemanticCommand::OverrideClassification {
                instance: context
                    .resolver
                    .id(required(args.target.as_ref(), name, "target")?)?,
                concept: context
                    .resolver
                    .id(required(args.concept.as_ref(), name, "concept")?)?,
                confidence: args.confidence.unwrap_or(1.0),
            }))
        }
    }
}

/// Translates a `jobs` tool request.
///
/// # Errors
///
/// Returns [`Error::Validation`] for missing or malformed fields.
pub fn jobs_command(
    action: JobsAction,
    args: &CommonArgs,
    context: &DispatchContext<'_>,
) -> Result<Command> {
    let name = action.as_str();
    match action {
        JobsAction::Submit => Ok(Command::Job(JobCommand::Submit(job_request(
            args, context,
        )?))),
        JobsAction::List => Ok(Command::Job(JobCommand::List)),
        JobsAction::Get => Ok(Command::Job(JobCommand::Get {
            id: context.resolver.id(required(
                args.id.as_ref().or(args.target.as_ref()),
                name,
                "id",
            )?)?,
        })),
        JobsAction::Results => Ok(Command::Job(JobCommand::Results {
            id: context.resolver.id(required(
                args.id.as_ref().or(args.target.as_ref()),
                name,
                "id",
            )?)?,
        })),
        JobsAction::Cancel => Ok(Command::Job(JobCommand::Cancel {
            id: context.resolver.id(required(
                args.id.as_ref().or(args.target.as_ref()),
                name,
                "id",
            )?)?,
        })),
        JobsAction::Retry => Ok(Command::Job(JobCommand::Retry {
            id: context.resolver.id(required(
                args.id.as_ref().or(args.target.as_ref()),
                name,
                "id",
            )?)?,
        })),
    }
}

fn job_request(args: &CommonArgs, context: &DispatchContext<'_>) -> Result<JobRequest> {
    if let Some(body) = &args.body {
        if body.get("kind").is_some() {
            return parse_body(body.clone());
        }
    }
    let kind = required(args.kind.as_ref(), "jobs submit", "kind")?.to_ascii_lowercase();
    match kind.as_str() {
        "classify_batch" => Ok(JobRequest::ClassifyBatch {
            instances: parse_ids(context.resolver, &args.instances)?,
            options: classification_options(args, context)?,
        }),
        "reclassify_batch" => Ok(JobRequest::ReclassifyBatch {
            instances: parse_ids(context.resolver, &args.instances)?,
            options: classification_options(args, context)?,
        }),
        "embed_batch" => Ok(JobRequest::EmbedBatch {
            instances: parse_ids(context.resolver, &args.instances)?,
            attributes: parse_ids(context.resolver, &args.attributes)?,
        }),
        "infer_batch" => Ok(JobRequest::InferBatch {
            instances: parse_ids(context.resolver, &args.instances)?,
            options: inference_options(args, context)?,
            policy: apply_policy(args)?,
        }),
        "rdf_import" => {
            let format = args.format.clone().unwrap_or_else(|| "turtle".to_owned());
            format.parse::<RdfFormat>()?;
            Ok(JobRequest::RdfImport {
                format,
                data: required(args.data.as_ref(), "jobs submit", "data")?.to_owned(),
                graph: context.resolver.optional_graph(args.graph.as_ref())?,
            })
        }
        "rdf_export" => {
            let format = args.format.clone().unwrap_or_else(|| "turtle".to_owned());
            format.parse::<RdfFormat>()?;
            Ok(JobRequest::RdfExport {
                format,
                graph: context.resolver.optional_graph(args.graph.as_ref())?,
            })
        }
        "external_import" => {
            let mut request =
                ImportRequest::new(required(args.source.as_ref(), "jobs submit", "source")?);
            request.ontology = context.resolver.optional_id(args.ontology.as_ref())?;
            request.format = args.format.clone();
            request.graph = context.resolver.optional_graph(args.graph.as_ref())?;
            Ok(JobRequest::ExternalImport(request))
        }
        "external_sync" => Ok(JobRequest::ExternalSync {
            id: context.resolver.id(required(
                args.target.as_ref().or(args.ontology.as_ref()),
                "jobs submit",
                "target",
            )?)?,
        }),
        other => Err(Error::validation(format!(
            "unknown job kind {other:?}; use classify_batch, reclassify_batch, embed_batch, \
             infer_batch, rdf_import, rdf_export, external_import, or external_sync"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Resolver {
        Resolver::new(NamespaceRegistry::with_builtins())
    }

    fn common() -> CommonArgs {
        CommonArgs::default()
    }

    #[test]
    fn translates_rdf_queries_and_instances() {
        let resolver = context();
        let context = DispatchContext {
            resolver: &resolver,
            max_results: 50,
            allow_write: false,
        };
        let mut args = common();
        args.target = Some("ex:Product".to_owned());
        args.type_filter = Some("ex:Thing".to_owned());
        let command = rdf_command(RdfAction::Search, &args, &context).unwrap();
        match command {
            Command::Rdf(RdfCommand::ListInstances(query)) => {
                assert_eq!(query.limit, 50);
                assert_eq!(
                    query.type_filter.unwrap().as_str(),
                    "https://example.org/Thing"
                );
            }
            other => panic!("unexpected command: {other:?}"),
        }

        args.sparql = Some("ASK {}".to_owned());
        let command = rdf_command(RdfAction::Query, &args, &context).unwrap();
        assert!(matches!(command, Command::Rdf(RdfCommand::Query { .. })));
    }

    #[test]
    fn read_only_classification_does_not_persist() {
        let resolver = context();
        let context = DispatchContext {
            resolver: &resolver,
            max_results: 50,
            allow_write: false,
        };
        let mut args = common();
        args.target = Some("ex:Product".to_owned());
        args.threshold = Some(0.7);
        let command = semantic_command(SemanticAction::Classify, &args, &context).unwrap();
        match command {
            Command::Semantic(SemanticCommand::Classify { options, .. }) => {
                assert!(!options.persist);
                assert_eq!(options.threshold, 0.7);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn background_import_becomes_a_job() {
        let resolver = context();
        let context = DispatchContext {
            resolver: &resolver,
            max_results: 50,
            allow_write: true,
        };
        let mut args = common();
        args.data = Some("<urn:s> <urn:p> \"v\" .".to_owned());
        args.format = Some("ntriples".to_owned());
        args.background = true;
        let command = rdf_command(RdfAction::Import, &args, &context).unwrap();
        assert!(matches!(
            command,
            Command::Job(JobCommand::Submit(JobRequest::RdfImport { .. }))
        ));
    }

    #[test]
    fn translates_taxonomy_relations() {
        let resolver = context();
        let context = DispatchContext {
            resolver: &resolver,
            max_results: 50,
            allow_write: true,
        };
        let mut args = common();
        args.operation = Some("add_relation".to_owned());
        args.source = Some("ex:Child".to_owned());
        args.target = Some("ex:Parent".to_owned());
        args.relation = Some("broader".to_owned());
        let command = ontology_command(OntologyAction::Taxonomy, &args, &context).unwrap();
        assert!(matches!(
            command,
            Command::Taxonomy(TaxonomyCommand::AddRelation {
                relation: SkosRelation::Broader,
                ..
            })
        ));
    }

    #[test]
    fn translates_job_submission() {
        let resolver = context();
        let context = DispatchContext {
            resolver: &resolver,
            max_results: 50,
            allow_write: true,
        };
        let mut args = common();
        args.kind = Some("embed_batch".to_owned());
        args.instances = vec!["ex:A".to_owned(), "ex:B".to_owned()];
        let command = jobs_command(JobsAction::Submit, &args, &context).unwrap();
        match command {
            Command::Job(JobCommand::Submit(JobRequest::EmbedBatch { instances, .. })) => {
                assert_eq!(instances.len(), 2);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_operations() {
        let resolver = context();
        let context = DispatchContext {
            resolver: &resolver,
            max_results: 50,
            allow_write: true,
        };
        let mut args = common();
        args.operation = Some("explode".to_owned());
        assert!(ontology_command(OntologyAction::Scheme, &args, &context).is_err());
    }
}
