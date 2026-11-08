//! The shared application service used by every interface.

use std::sync::Arc;

use ontology_classify::ClassificationService;
use ontology_core::command::{
    CollectionResult, Command, CommandResult, ExternalCommand, JobCommand, OntologyCommand,
    OperationReport, RdfCommand, ResourceResult, SemanticCommand, TaxonomyCommand,
};
use ontology_core::error::{Error, Result};
use ontology_core::external::ExternalOntologyRegistry;
use ontology_core::id::Id;
use ontology_core::model::{GraphName, QuadPattern};
use ontology_core::namespace::NamespaceRegistry;
use ontology_core::owl::OntologyManager;
use ontology_core::rdf::{OntologyService, RdfStore};
use ontology_core::semantic::{EmbeddingProvider, InferenceEngine, VectorIndex};
use ontology_core::skos::TaxonomyManager;
use ontology_embedding::{EmbeddingService, HashingEmbeddingProvider, InMemoryVectorIndex};
use ontology_import::ExternalOntologyManager;
use ontology_inference::RuleInferenceEngine;
use ontology_owl::OwlManager;
use ontology_skos::SkosManager;
use serde::Serialize;

use crate::instance::InstanceService;
use crate::jobs::{JobContext, JobManager};

/// Default number of background jobs that may run at once.
const JOB_CONCURRENCY: usize = 4;

/// Orchestrates commands against the RDF store.
///
/// HTTP, CLI, and MCP translate their input into [`Command`] values and render
/// [`CommandResult`] values; they hold no business logic of their own.
///
/// Storage operations run through the engine-independent [`RdfStore`] trait, so
/// interfaces and the transport layer never depend on Oxigraph directly.
pub struct ApplicationService {
    store: Arc<dyn RdfStore>,
    instances: InstanceService,
    owl: Arc<OwlManager>,
    skos: Arc<SkosManager>,
    external: Arc<ExternalOntologyManager>,
    classify: Arc<ClassificationService>,
    embedding: Arc<EmbeddingService>,
    inference: Arc<RuleInferenceEngine>,
    jobs: JobManager,
    namespaces: NamespaceRegistry,
}

impl ApplicationService {
    /// Wraps a store with the built-in namespace registry.
    #[must_use]
    pub fn new(store: Arc<dyn RdfStore>) -> Self {
        let owl = Arc::new(OwlManager::new(Arc::clone(&store)));
        let skos = Arc::new(SkosManager::new(Arc::clone(&store)));
        let external = Arc::new(ExternalOntologyManager::new(Arc::clone(&store)));
        let provider: Arc<dyn EmbeddingProvider> = Arc::new(HashingEmbeddingProvider::new(128));
        let index: Arc<dyn VectorIndex> = Arc::new(InMemoryVectorIndex::new());
        let ontology: Arc<dyn OntologyManager> = owl.clone();
        let taxonomy: Arc<dyn TaxonomyManager> = skos.clone();
        let registry: Arc<dyn ExternalOntologyRegistry> = external.clone();
        let embedding = Arc::new(EmbeddingService::new(
            Arc::clone(&store),
            Arc::clone(&provider),
            Arc::clone(&index),
        ));
        let classify = Arc::new(
            ClassificationService::new(Arc::clone(&store), Arc::clone(&taxonomy))
                .with_embedding_provider(provider),
        );
        let inference = Arc::new(
            RuleInferenceEngine::new(Arc::clone(&store), ontology, taxonomy)
                .with_external(registry),
        );
        let jobs = JobManager::new(
            Arc::new(JobContext {
                classify: Arc::clone(&classify),
                embedding: Arc::clone(&embedding),
                inference: Arc::clone(&inference),
                store: Arc::clone(&store),
                external: Arc::clone(&external),
            }),
            crate::service::JOB_CONCURRENCY,
        );
        Self {
            instances: InstanceService::new(Arc::clone(&store)),
            owl,
            skos,
            external,
            classify,
            embedding,
            inference,
            jobs,
            store,
            namespaces: NamespaceRegistry::with_builtins(),
        }
    }

    /// Replaces the namespace registry.
    #[must_use]
    pub fn with_namespaces(mut self, namespaces: NamespaceRegistry) -> Self {
        self.namespaces = namespaces;
        self
    }

    /// Returns the namespace registry.
    #[must_use]
    pub fn namespaces(&self) -> &NamespaceRegistry {
        &self.namespaces
    }

    /// Resolves a prefixed name or absolute IRI.
    ///
    /// # Errors
    ///
    /// Propagates resolution failures.
    pub fn resolve(&self, name: &str) -> Result<Id> {
        self.namespaces.resolve(name)
    }

    /// Returns the instance service.
    #[must_use]
    pub fn instances(&self) -> &InstanceService {
        &self.instances
    }

    /// Returns the OWL schema manager.
    #[must_use]
    pub fn owl(&self) -> &OwlManager {
        self.owl.as_ref()
    }

    /// Returns the SKOS taxonomy manager.
    #[must_use]
    pub fn skos(&self) -> &SkosManager {
        self.skos.as_ref()
    }

    /// Returns the external ontology registry.
    #[must_use]
    pub fn external(&self) -> &ExternalOntologyManager {
        self.external.as_ref()
    }

    /// Returns the classification engine.
    #[must_use]
    pub fn classify(&self) -> &ClassificationService {
        self.classify.as_ref()
    }

    /// Returns the embedding service.
    #[must_use]
    pub fn embedding(&self) -> &EmbeddingService {
        self.embedding.as_ref()
    }

    /// Returns the inference engine.
    #[must_use]
    pub fn inference(&self) -> &RuleInferenceEngine {
        self.inference.as_ref()
    }

    /// Returns the job manager.
    #[must_use]
    pub fn jobs(&self) -> &JobManager {
        &self.jobs
    }

    /// Returns the underlying store.
    #[must_use]
    pub fn store(&self) -> &Arc<dyn RdfStore> {
        &self.store
    }

    fn handle(&self, command: RdfCommand) -> Result<CommandResult> {
        match command {
            RdfCommand::CreateInstance(new) => {
                let instance = self.instances.create_instance(new)?;
                Ok(CommandResult::Resource(Self::resource(
                    &instance.id,
                    "instance",
                    &instance,
                )?))
            }
            RdfCommand::GetInstance { id, graph } => {
                let instance = self.instances.get_instance(&id, graph.as_ref())?;
                Ok(CommandResult::Resource(Self::resource(
                    &instance.id,
                    "instance",
                    &instance,
                )?))
            }
            RdfCommand::ListInstances(query) => {
                let limit = query.limit;
                let offset = query.offset;
                let summaries = self.instances.list_instances(query)?;
                let items = summaries
                    .iter()
                    .map(|summary| serde_json::to_value(summary).map_err(Error::serialization))
                    .collect::<Result<Vec<_>>>()?;
                Ok(CommandResult::Collection(CollectionResult {
                    count: items.len(),
                    items,
                    limit,
                    offset,
                }))
            }
            RdfCommand::UpdateInstance {
                id,
                patch,
                graph,
                source,
            } => {
                let instance = self.instances.update_instance(
                    &id,
                    patch,
                    graph.as_ref(),
                    source.as_deref(),
                )?;
                Ok(CommandResult::Resource(Self::resource(
                    &instance.id,
                    "instance",
                    &instance,
                )?))
            }
            RdfCommand::DeleteInstance { id, graph } => {
                let affected = self.instances.delete_instance(&id, graph.as_ref())?;
                Ok(CommandResult::Report(OperationReport {
                    operation: "delete_instance".to_owned(),
                    affected,
                    message: None,
                }))
            }
            RdfCommand::ListStatements { id, graph } => {
                let statements = self.instances.statements(&id, graph.as_ref())?;
                let items = statements
                    .iter()
                    .map(|statement| serde_json::to_value(statement).map_err(Error::serialization))
                    .collect::<Result<Vec<_>>>()?;
                Ok(CommandResult::Collection(CollectionResult {
                    count: items.len(),
                    limit: items.len(),
                    offset: 0,
                    items,
                }))
            }
            RdfCommand::AddStatements {
                id,
                statements,
                graph,
                source,
            } => {
                let statements = self.instances.add_statements(
                    &id,
                    &statements,
                    graph.as_ref(),
                    source.as_deref(),
                )?;
                let items = statements
                    .iter()
                    .map(|statement| serde_json::to_value(statement).map_err(Error::serialization))
                    .collect::<Result<Vec<_>>>()?;
                Ok(CommandResult::Collection(CollectionResult {
                    count: items.len(),
                    limit: items.len(),
                    offset: 0,
                    items,
                }))
            }
            RdfCommand::DeleteStatements {
                id,
                statement_ids,
                graph,
            } => {
                let affected =
                    self.instances
                        .delete_statements(&id, &statement_ids, graph.as_ref())?;
                Ok(CommandResult::Report(OperationReport {
                    operation: "delete_statements".to_owned(),
                    affected,
                    message: None,
                }))
            }
            RdfCommand::GetRelationships { id } => {
                let relationships = self.instances.relationships(&id)?;
                let items = relationships
                    .iter()
                    .map(|relationship| {
                        serde_json::to_value(relationship).map_err(Error::serialization)
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(CommandResult::Collection(CollectionResult {
                    count: items.len(),
                    limit: items.len(),
                    offset: 0,
                    items,
                }))
            }
            RdfCommand::Query { sparql } => Ok(CommandResult::Query(self.store.query(&sparql)?)),
            RdfCommand::Update { sparql } => {
                self.store.update(&sparql)?;
                Ok(CommandResult::Report(OperationReport {
                    operation: "update".to_owned(),
                    affected: 0,
                    message: None,
                }))
            }
            RdfCommand::ListGraphs => {
                let graphs = self.store.graphs()?;
                let items = graphs
                    .iter()
                    .map(|graph| serde_json::to_value(graph).map_err(Error::serialization))
                    .collect::<Result<Vec<_>>>()?;
                Ok(CommandResult::Collection(CollectionResult {
                    count: items.len(),
                    limit: items.len(),
                    offset: 0,
                    items,
                }))
            }
            RdfCommand::ClearGraph { graph } => {
                let quads = self.store.get(QuadPattern::new().with_graph(graph))?;
                self.store.delete(&quads)?;
                Ok(CommandResult::Report(OperationReport {
                    operation: "clear_graph".to_owned(),
                    affected: quads.len(),
                    message: None,
                }))
            }
            RdfCommand::Import {
                format,
                data,
                graph,
            } => {
                let format = format.parse::<ontology_core::format::RdfFormat>()?;
                let inserted =
                    self.store
                        .load_from_slice(format, data.as_bytes(), None, graph.as_ref())?;
                Ok(CommandResult::Report(OperationReport {
                    operation: "import".to_owned(),
                    affected: inserted as usize,
                    message: Some(format!("{inserted} quads parsed")),
                }))
            }
            RdfCommand::Export { format, graph } => {
                let format = format.parse::<ontology_core::format::RdfFormat>()?;
                let data = self.store.serialize(format, graph.as_ref())?;
                Ok(CommandResult::Export {
                    format: format.as_str().to_owned(),
                    data,
                })
            }
        }
    }

    async fn handle_job(&self, command: JobCommand) -> Result<CommandResult> {
        match command {
            JobCommand::Submit(request) => Ok(CommandResult::Job(self.jobs.submit(request).await?)),
            JobCommand::Get { id } | JobCommand::Results { id } => {
                Ok(CommandResult::Job(self.jobs.get(&id).await?))
            }
            JobCommand::List => {
                let statuses = self.jobs.list().await?;
                let limit = statuses.len();
                Ok(CommandResult::Collection(Self::collection(
                    &statuses, limit, 0,
                )?))
            }
            JobCommand::Cancel { id } => Ok(CommandResult::Job(self.jobs.cancel(&id).await?)),
            JobCommand::Retry { id } => Ok(CommandResult::Job(self.jobs.retry(&id).await?)),
        }
    }

    async fn handle_ontology(&self, command: OntologyCommand) -> Result<CommandResult> {
        match command {
            OntologyCommand::CreateClass(class) => {
                let id = self.owl.create_class(class).await?;
                self.class_resource(&id).await
            }
            OntologyCommand::GetClass { id } => self.class_resource(&id).await,
            OntologyCommand::ListClasses(query) => {
                let limit = query.limit;
                let offset = query.offset;
                let classes = self.owl.list_classes(query).await?;
                Ok(CommandResult::Collection(Self::collection(
                    &classes, limit, offset,
                )?))
            }
            OntologyCommand::UpdateClass(class) => {
                let id = self.owl.update_class(class).await?;
                self.class_resource(&id).await
            }
            OntologyCommand::DeleteClass { id } => {
                let affected = self.owl.delete_class(&id).await?;
                Ok(CommandResult::Report(OperationReport {
                    operation: "delete_class".to_owned(),
                    affected,
                    message: None,
                }))
            }
            OntologyCommand::CreateProperty(property) => {
                let id = self.owl.create_property(property).await?;
                self.property_resource(&id).await
            }
            OntologyCommand::GetProperty { id } => self.property_resource(&id).await,
            OntologyCommand::ListProperties(query) => {
                let limit = query.limit;
                let offset = query.offset;
                let properties = self.owl.list_properties(query).await?;
                Ok(CommandResult::Collection(Self::collection(
                    &properties,
                    limit,
                    offset,
                )?))
            }
            OntologyCommand::UpdateProperty(property) => {
                let id = self.owl.update_property(property).await?;
                self.property_resource(&id).await
            }
            OntologyCommand::DeleteProperty { id } => {
                let affected = self.owl.delete_property(&id).await?;
                Ok(CommandResult::Report(OperationReport {
                    operation: "delete_property".to_owned(),
                    affected,
                    message: None,
                }))
            }
            OntologyCommand::ClassHierarchy { id } => {
                let hierarchy = self.owl.hierarchy(&id).await?;
                Ok(CommandResult::Resource(Self::resource(
                    &hierarchy.class,
                    "class_hierarchy",
                    &hierarchy,
                )?))
            }
            OntologyCommand::Validate { scope } => {
                Ok(CommandResult::Validation(self.owl.validate(scope).await?))
            }
            OntologyCommand::CreateOntology(ontology) => {
                let id = self.owl.create_ontology(ontology).await?;
                self.ontology_resource(&id).await
            }
            OntologyCommand::GetOntology { id } => self.ontology_resource(&id).await,
            OntologyCommand::ListOntologies => {
                let ontologies = self.owl.list_ontologies().await?;
                let limit = ontologies.len();
                Ok(CommandResult::Collection(Self::collection(
                    &ontologies,
                    limit,
                    0,
                )?))
            }
            OntologyCommand::UpdateOntology(ontology) => {
                let id = self.owl.update_ontology(ontology).await?;
                self.ontology_resource(&id).await
            }
            OntologyCommand::DeleteOntology { id } => {
                let affected = self.owl.delete_ontology(&id).await?;
                Ok(CommandResult::Report(OperationReport {
                    operation: "delete_ontology".to_owned(),
                    affected,
                    message: None,
                }))
            }
            OntologyCommand::OntologyVersions { id } => {
                let versions = self.owl.ontology_versions(&id).await?;
                let limit = versions.len();
                Ok(CommandResult::Collection(Self::collection(
                    &versions, limit, 0,
                )?))
            }
            OntologyCommand::ApplicableProperties { class } => {
                let properties = self.owl.applicable_properties(&class).await?;
                let limit = properties.len();
                Ok(CommandResult::Collection(Self::collection(
                    &properties,
                    limit,
                    0,
                )?))
            }
            OntologyCommand::ApplicableClasses { properties } => {
                let classes = self.owl.applicable_classes(&properties).await?;
                let limit = classes.len();
                Ok(CommandResult::Collection(Self::collection(
                    &classes, limit, 0,
                )?))
            }
        }
    }

    async fn class_resource(&self, id: &Id) -> Result<CommandResult> {
        let class = self.owl.get_class(id).await?;
        Ok(CommandResult::Resource(Self::resource(
            &class.id,
            "owl_class",
            &class,
        )?))
    }

    async fn property_resource(&self, id: &Id) -> Result<CommandResult> {
        let property = self.owl.get_property(id).await?;
        Ok(CommandResult::Resource(Self::resource(
            &property.id,
            "owl_property",
            &property,
        )?))
    }

    async fn ontology_resource(&self, id: &Id) -> Result<CommandResult> {
        let ontology = self.owl.get_ontology(id).await?;
        Ok(CommandResult::Resource(Self::resource(
            &ontology.id,
            "ontology",
            &ontology,
        )?))
    }

    async fn handle_taxonomy(&self, command: TaxonomyCommand) -> Result<CommandResult> {
        match command {
            TaxonomyCommand::CreateScheme(scheme) => {
                let id = self.skos.create_scheme(scheme).await?;
                self.scheme_resource(&id).await
            }
            TaxonomyCommand::GetScheme { id } => self.scheme_resource(&id).await,
            TaxonomyCommand::ListSchemes => {
                let schemes = self.skos.list_schemes().await?;
                let limit = schemes.len();
                Ok(CommandResult::Collection(Self::collection(
                    &schemes, limit, 0,
                )?))
            }
            TaxonomyCommand::UpdateScheme(scheme) => {
                let id = self.skos.update_scheme(scheme).await?;
                self.scheme_resource(&id).await
            }
            TaxonomyCommand::DeleteScheme { id } => {
                let affected = self.skos.delete_scheme(&id).await?;
                Ok(CommandResult::Report(OperationReport {
                    operation: "delete_scheme".to_owned(),
                    affected,
                    message: None,
                }))
            }
            TaxonomyCommand::SchemeVersions { id } => {
                let versions = self.skos.scheme_versions(&id).await?;
                let limit = versions.len();
                Ok(CommandResult::Collection(Self::collection(
                    &versions, limit, 0,
                )?))
            }
            TaxonomyCommand::CreateConcept(concept) => {
                let id = self.skos.create_concept(concept).await?;
                self.concept_resource(&id).await
            }
            TaxonomyCommand::GetConcept { id } => self.concept_resource(&id).await,
            TaxonomyCommand::ListConcepts(query) => {
                let limit = query.limit;
                let offset = query.offset;
                let concepts = self.skos.list_concepts(query).await?;
                Ok(CommandResult::Collection(Self::collection(
                    &concepts, limit, offset,
                )?))
            }
            TaxonomyCommand::UpdateConcept(concept) => {
                let id = self.skos.update_concept(concept).await?;
                self.concept_resource(&id).await
            }
            TaxonomyCommand::DeleteConcept { id } => {
                let affected = self.skos.delete_concept(&id).await?;
                Ok(CommandResult::Report(OperationReport {
                    operation: "delete_concept".to_owned(),
                    affected,
                    message: None,
                }))
            }
            TaxonomyCommand::SearchConcepts { query, scheme } => {
                let matches = self.skos.search(&query, scheme.as_ref()).await?;
                let limit = matches.len();
                Ok(CommandResult::Collection(Self::collection(
                    &matches, limit, 0,
                )?))
            }
            TaxonomyCommand::AddRelation {
                source,
                relation,
                target,
            } => {
                self.skos.add_relation(&source, relation, &target).await?;
                Ok(CommandResult::Report(OperationReport {
                    operation: "add_relation".to_owned(),
                    affected: 1,
                    message: None,
                }))
            }
            TaxonomyCommand::RemoveRelation {
                source,
                relation,
                target,
            } => {
                self.skos
                    .remove_relation(&source, relation, &target)
                    .await?;
                Ok(CommandResult::Report(OperationReport {
                    operation: "remove_relation".to_owned(),
                    affected: 1,
                    message: None,
                }))
            }
            TaxonomyCommand::ListRelations { id } => {
                let relations = self.skos.relations(&id).await?;
                let limit = relations.len();
                Ok(CommandResult::Collection(Self::collection(
                    &relations, limit, 0,
                )?))
            }
            TaxonomyCommand::Descendants { id } => {
                let concepts = self.skos.descendants(&id).await?;
                let limit = concepts.len();
                Ok(CommandResult::Collection(Self::collection(
                    &concepts, limit, 0,
                )?))
            }
            TaxonomyCommand::Ancestors { id } => {
                let concepts = self.skos.ancestors(&id).await?;
                let limit = concepts.len();
                Ok(CommandResult::Collection(Self::collection(
                    &concepts, limit, 0,
                )?))
            }
            TaxonomyCommand::ConceptTree { scheme } => {
                let trees = self.skos.tree(&scheme).await?;
                Ok(CommandResult::Resource(Self::resource(
                    &scheme,
                    "concept_tree",
                    &trees,
                )?))
            }
            TaxonomyCommand::MoveConcept {
                id,
                broader,
                scheme,
            } => {
                self.skos
                    .move_concept(&id, broader.as_ref(), scheme.as_ref())
                    .await?;
                Ok(CommandResult::Report(OperationReport {
                    operation: "move_concept".to_owned(),
                    affected: 1,
                    message: None,
                }))
            }
            TaxonomyCommand::MergeSchemes { source, target } => Ok(CommandResult::Taxonomy(
                self.skos.merge_schemes(&source, &target).await?,
            )),
            TaxonomyCommand::ValidateTaxonomy { scheme } => Ok(CommandResult::Taxonomy(
                self.skos.validate(scheme.as_ref()).await?,
            )),
            TaxonomyCommand::CreateCollection(collection) => {
                let id = self.skos.create_collection(collection).await?;
                self.collection_resource(&id).await
            }
            TaxonomyCommand::GetCollection { id } => self.collection_resource(&id).await,
            TaxonomyCommand::ListCollections { scheme } => {
                let collections = self.skos.list_collections(scheme.as_ref()).await?;
                let limit = collections.len();
                Ok(CommandResult::Collection(Self::collection(
                    &collections,
                    limit,
                    0,
                )?))
            }
            TaxonomyCommand::DeleteCollection { id } => {
                let affected = self.skos.delete_collection(&id).await?;
                Ok(CommandResult::Report(OperationReport {
                    operation: "delete_collection".to_owned(),
                    affected,
                    message: None,
                }))
            }
        }
    }

    async fn handle_external(&self, command: ExternalCommand) -> Result<CommandResult> {
        match command {
            ExternalCommand::Import(request) => {
                let report = self.external.import(request).await?;
                Ok(CommandResult::Resource(Self::resource(
                    &report.ontology,
                    "import_report",
                    &report,
                )?))
            }
            ExternalCommand::Get { id } => self.external_resource(&id).await,
            ExternalCommand::List => {
                let ontologies = self.external.list().await?;
                let limit = ontologies.len();
                Ok(CommandResult::Collection(Self::collection(
                    &ontologies,
                    limit,
                    0,
                )?))
            }
            ExternalCommand::Sync { id } => {
                let report = self.external.sync(&id).await?;
                Ok(CommandResult::Resource(Self::resource(
                    &report.ontology,
                    "import_report",
                    &report,
                )?))
            }
            ExternalCommand::Delete { id } => {
                let affected = self.external.delete(&id).await?;
                Ok(CommandResult::Report(OperationReport {
                    operation: "delete_external_ontology".to_owned(),
                    affected,
                    message: None,
                }))
            }
            ExternalCommand::MapClass(mapping) => {
                let value = mapping.clone();
                let id = self.external.map_class(mapping).await?;
                Ok(CommandResult::Resource(Self::resource(
                    &id,
                    "class_mapping",
                    &value,
                )?))
            }
            ExternalCommand::MapProperty(mapping) => {
                let value = mapping.clone();
                let id = self.external.map_property(mapping).await?;
                Ok(CommandResult::Resource(Self::resource(
                    &id,
                    "property_mapping",
                    &value,
                )?))
            }
            ExternalCommand::ListClassMappings(query) => {
                let limit = query.limit;
                let offset = query.offset;
                let mappings = self.external.list_class_mappings(query).await?;
                Ok(CommandResult::Collection(Self::collection(
                    &mappings, limit, offset,
                )?))
            }
            ExternalCommand::ListPropertyMappings(query) => {
                let limit = query.limit;
                let offset = query.offset;
                let mappings = self.external.list_property_mappings(query).await?;
                Ok(CommandResult::Collection(Self::collection(
                    &mappings, limit, offset,
                )?))
            }
            ExternalCommand::ValidateMappings => Ok(CommandResult::Mapping(
                self.external.validate_mappings().await?,
            )),
        }
    }

    async fn scheme_resource(&self, id: &Id) -> Result<CommandResult> {
        let scheme = self.skos.get_scheme(id).await?;
        Ok(CommandResult::Resource(Self::resource(
            &scheme.id,
            "concept_scheme",
            &scheme,
        )?))
    }

    async fn concept_resource(&self, id: &Id) -> Result<CommandResult> {
        let concept = self.skos.get_concept(id).await?;
        Ok(CommandResult::Resource(Self::resource(
            &concept.id,
            "concept",
            &concept,
        )?))
    }

    async fn collection_resource(&self, id: &Id) -> Result<CommandResult> {
        let collection = self.skos.get_collection(id).await?;
        Ok(CommandResult::Resource(Self::resource(
            &collection.id,
            "concept_collection",
            &collection,
        )?))
    }

    async fn external_resource(&self, id: &Id) -> Result<CommandResult> {
        let ontology = self.external.get(id).await?;
        Ok(CommandResult::Resource(Self::resource(
            &ontology.id,
            "external_ontology",
            &ontology,
        )?))
    }

    async fn handle_semantic(&self, command: SemanticCommand) -> Result<CommandResult> {
        match command {
            SemanticCommand::Classify { instance, options } => Ok(CommandResult::Classifications(
                self.classify.classify(&instance, options).await?,
            )),
            SemanticCommand::ClassifyBatch { instances, options } => {
                let batch = self.classify.classify_batch(&instances, options).await?;
                Ok(CommandResult::Classifications(batch.classifications))
            }
            SemanticCommand::ListClassifications { instance } => Ok(
                CommandResult::Classifications(self.classify.list(&instance).await?),
            ),
            SemanticCommand::OverrideClassification {
                instance,
                concept,
                confidence,
            } => Ok(CommandResult::Classifications(vec![
                self.classify
                    .override_classification(&instance, &concept, confidence)
                    .await?,
            ])),
            SemanticCommand::ReviewClassification {
                classification,
                approved,
            } => Ok(CommandResult::Classifications(vec![
                self.classify.review(&classification, approved).await?,
            ])),
            SemanticCommand::Reclassify { instance, options } => Ok(
                CommandResult::Classifications(self.classify.reclassify(&instance, options).await?),
            ),
            SemanticCommand::GenerateEmbeddings {
                instance,
                attributes,
            } => Ok(CommandResult::Embeddings(
                self.embedding
                    .generate(&instance, &attributes, None)
                    .await?,
            )),
            SemanticCommand::SearchSimilar {
                instance,
                limit,
                filter,
            } => Ok(CommandResult::Similarity(
                self.embedding.search(&instance, limit, filter).await?,
            )),
            SemanticCommand::ListEmbeddings { instance } => Ok(CommandResult::Embeddings(
                self.embedding.list(&instance).await?,
            )),
            SemanticCommand::DeleteEmbeddings { instance } => {
                let affected = self.embedding.delete(&instance).await?;
                Ok(CommandResult::Report(OperationReport {
                    operation: "delete_embeddings".to_owned(),
                    affected,
                    message: None,
                }))
            }
            SemanticCommand::PreviewInference { instance, options } => Ok(CommandResult::Preview(
                self.inference.preview(&instance, options).await?,
            )),
            SemanticCommand::ApplyInference {
                instance,
                options,
                policy,
            } => Ok(CommandResult::Inference(
                self.inference.apply(&instance, policy, options).await?,
            )),
            SemanticCommand::ExplainInference { instance, options } => Ok(CommandResult::Preview(
                self.inference.preview(&instance, options).await?,
            )),
        }
    }

    fn collection<T: Serialize>(
        values: &[T],
        limit: usize,
        offset: usize,
    ) -> Result<CollectionResult> {
        let items = values
            .iter()
            .map(|value| serde_json::to_value(value).map_err(Error::serialization))
            .collect::<Result<Vec<_>>>()?;
        Ok(CollectionResult {
            count: items.len(),
            items,
            limit,
            offset,
        })
    }

    fn resource<T: Serialize>(id: &Id, kind: &str, value: &T) -> Result<ResourceResult> {
        Ok(ResourceResult {
            id: id.to_string(),
            kind: kind.to_owned(),
            resource: serde_json::to_value(value).map_err(Error::serialization)?,
        })
    }
}

impl ApplicationService {
    /// Executes one RDF command. Exposed for interfaces that dispatch by family.
    ///
    /// # Errors
    ///
    /// Propagates domain errors.
    pub fn execute_rdf(&self, command: RdfCommand) -> Result<CommandResult> {
        self.handle(command)
    }

    /// Returns a graph name from a prefixed or absolute name.
    ///
    /// # Errors
    ///
    /// Propagates resolution failures.
    pub fn resolve_graph(&self, name: &str) -> Result<GraphName> {
        Ok(GraphName::Iri(self.namespaces.resolve(name)?))
    }
}

#[async_trait::async_trait]
impl OntologyService for ApplicationService {
    async fn execute(&self, command: Command) -> Result<CommandResult> {
        match command {
            Command::Rdf(command) => self.handle(command),
            Command::Ontology(command) => self.handle_ontology(command).await,
            Command::Taxonomy(command) => self.handle_taxonomy(command).await,
            Command::External(command) => self.handle_external(command).await,
            Command::Semantic(command) => self.handle_semantic(command).await,
            Command::Job(command) => self.handle_job(command).await,
        }
    }
}
