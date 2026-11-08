//! Phase 3 tests: multi-strategy classification, classification review and
//! overrides, embedding generation and similarity search, inference policies,
//! external mapping enrichment, and explanation output.

use std::sync::Arc;

use ontology_application::ApplicationService;
use ontology_classify::ClassificationService;
use ontology_core::command::{Command, CommandResult, SemanticCommand};
use ontology_core::external::{
    ClassMapping, ExternalOntologyRegistry, MappingKind, PropertyMapping,
};
use ontology_core::graph::GraphKind;
use ontology_core::id::Id;
use ontology_core::model::{GraphName, Quad, QuadPattern, Term};
use ontology_core::owl::{OntologyManager, OwlClass, OwlProperty, PropertyKind};
use ontology_core::rdf::{OntologyService, RdfStore};
use ontology_core::semantic::{
    ApplyPolicy, ClassificationOptions, ClassificationRule, ClassificationStrategy,
    EmbeddingProvider, InferenceEngine, InferenceMethod, InferenceOptions, ReviewStatus,
    RuleOperator, SimilarityMetric, VectorFilter, VectorIndex,
};
use ontology_core::skos::{Concept, ConceptScheme, TaxonomyManager};
use ontology_embedding::{EmbeddingService, HashingEmbeddingProvider, InMemoryVectorIndex};
use ontology_import::ExternalOntologyManager;
use ontology_inference::RuleInferenceEngine;
use ontology_integration_tests::{ex, memory_store_handle};
use ontology_owl::OwlManager;
use ontology_skos::SkosManager;

fn rdf_type() -> Id {
    Id::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#type").expect("rdf:type")
}

fn asserted_graph() -> GraphName {
    GraphName::Iri(GraphKind::Asserted.iri())
}

fn inferred_graph() -> GraphName {
    GraphName::Iri(GraphKind::Inferred.iri())
}

fn external(local: &str) -> Id {
    Id::new(format!("https://external.example.org/{local}")).expect("external IRI")
}

fn insert_instance(store: &Arc<dyn RdfStore>, id: &Id, class: &Id, values: &[(Id, &str)]) {
    let graph = asserted_graph();
    let mut quads = vec![Quad::in_graph(
        Term::Iri(id.clone()),
        rdf_type(),
        Term::Iri(class.clone()),
        graph.clone(),
    )];
    for (predicate, value) in values {
        quads.push(Quad::in_graph(
            Term::Iri(id.clone()),
            predicate.clone(),
            Term::string(*value),
            graph.clone(),
        ));
    }
    store.insert(&quads).expect("insert instance");
}

async fn setup_taxonomy(store: &Arc<dyn RdfStore>) -> Arc<SkosManager> {
    let skos = Arc::new(SkosManager::new(Arc::clone(store)));
    let scheme = ex("categories");
    skos.create_scheme(
        ConceptScheme::new(scheme.clone())
            .with_label("en", "Categories")
            .with_version("1.0"),
    )
    .await
    .expect("create scheme");
    let concepts = [
        ("electronics", "Electronics", None),
        ("networking", "Networking Equipment", Some("network gear")),
        ("storage", "Storage Devices", Some("hard drives")),
    ];
    for (local, label, alternative) in concepts {
        let mut concept = Concept::new(ex(local))
            .with_scheme(scheme.clone())
            .with_pref_label(label, "en");
        if let Some(alternative) = alternative {
            concept = concept.with_alt_label(alternative, "en");
        }
        skos.create_concept(concept).await.expect("concept");
    }
    skos
}

#[tokio::test]
async fn classifies_with_label_strategies() {
    let store = memory_store_handle();
    let skos = setup_taxonomy(&store).await;
    let service = ClassificationService::new(Arc::clone(&store), skos);

    let product = ex("product-1");
    insert_instance(
        &store,
        &product,
        &ex("Product"),
        &[
            (ex("name"), "Network Switch"),
            (ex("description"), "network gear"),
        ],
    );

    let results = service
        .classify(
            &product,
            ClassificationOptions::new()
                .with_scheme(ex("categories"))
                .with_threshold(0.5),
        )
        .await
        .expect("classify");
    let networking = results
        .iter()
        .find(|result| result.concept == ex("networking"))
        .expect("networking match");
    assert!(networking.confidence > 0.7, "{}", networking.confidence);
    assert_eq!(networking.strategy, ClassificationStrategy::Hybrid);
    assert_eq!(networking.status, ReviewStatus::Pending);
    assert_eq!(networking.taxonomy_version.as_deref(), Some("1.0"));
    assert!(!networking.explanation.matched.is_empty());

    // Explicit strategies.
    let normalized = service
        .classify(
            &product,
            ClassificationOptions::new()
                .with_strategies(vec![ClassificationStrategy::Normalized])
                .with_attributes(vec![ex("description")]),
        )
        .await
        .expect("normalized");
    assert!(normalized
        .iter()
        .any(|result| result.strategy == ClassificationStrategy::Normalized));

    let fuzzy_instance = ex("product-2");
    insert_instance(
        &store,
        &fuzzy_instance,
        &ex("Product"),
        &[(ex("name"), "Electronix")],
    );
    let fuzzy = service
        .classify(
            &fuzzy_instance,
            ClassificationOptions::new()
                .with_strategies(vec![ClassificationStrategy::Fuzzy])
                .with_threshold(0.8),
        )
        .await
        .expect("fuzzy");
    assert!(fuzzy
        .iter()
        .any(|result| result.concept == ex("electronics")
            && result.strategy == ClassificationStrategy::Fuzzy));

    let empty = service
        .classify(
            &fuzzy_instance,
            ClassificationOptions::new()
                .with_strategies(vec![ClassificationStrategy::Fuzzy])
                .with_threshold(0.999),
        )
        .await
        .expect("threshold");
    assert!(empty.is_empty());
}

#[tokio::test]
async fn classifies_with_rules_and_semantics() {
    let store = memory_store_handle();
    let skos = setup_taxonomy(&store).await;
    let provider: Arc<dyn EmbeddingProvider> = Arc::new(HashingEmbeddingProvider::new(64));
    let service = ClassificationService::new(Arc::clone(&store), skos.clone())
        .with_embedding_provider(provider)
        .with_rules(vec![ClassificationRule::new(
            "storage-rule",
            ex("category"),
            RuleOperator::Equals,
            "storage",
            ex("storage"),
            0.93,
        )]);

    let rule_instance = ex("rule-1");
    insert_instance(
        &store,
        &rule_instance,
        &ex("Product"),
        &[(ex("category"), "Storage")],
    );
    let rules = service
        .classify(
            &rule_instance,
            ClassificationOptions::new()
                .with_strategies(vec![ClassificationStrategy::Rules])
                .with_threshold(0.5),
        )
        .await
        .expect("rules");
    let storage = rules
        .iter()
        .find(|result| result.concept == ex("storage"))
        .expect("rule match");
    assert_eq!(storage.strategy, ClassificationStrategy::Rules);
    assert!((storage.confidence - 0.93).abs() < f64::EPSILON);
    assert!(storage
        .explanation
        .rules
        .contains(&"storage-rule".to_owned()));

    let semantic_instance = ex("semantic-1");
    insert_instance(
        &store,
        &semantic_instance,
        &ex("Product"),
        &[(ex("name"), "hard drives")],
    );
    let semantic = service
        .classify(
            &semantic_instance,
            ClassificationOptions::new()
                .with_strategies(vec![ClassificationStrategy::Semantic])
                .with_threshold(0.5),
        )
        .await
        .expect("semantic");
    assert!(semantic.iter().any(|result| result.concept == ex("storage")
        && result.strategy == ClassificationStrategy::Semantic
        && result.confidence > 0.9));
}

#[tokio::test]
async fn persists_reviews_and_overrides() {
    let store = memory_store_handle();
    let skos = setup_taxonomy(&store).await;
    let service = ClassificationService::new(Arc::clone(&store), skos);

    let product = ex("review-1");
    insert_instance(
        &store,
        &product,
        &ex("Product"),
        &[(ex("name"), "network gear")],
    );
    let results = service
        .classify(&product, ClassificationOptions::new().with_threshold(0.5))
        .await
        .expect("classify");
    assert!(!results.is_empty());
    let stored = service.list(&product).await.expect("list");
    assert_eq!(stored.len(), results.len());

    let rejected = service.review(&results[0].id, false).await.expect("review");
    assert_eq!(rejected.status, ReviewStatus::Rejected);
    let stored = service.list(&product).await.expect("list");
    assert!(stored
        .iter()
        .any(|record| record.id == results[0].id && record.status == ReviewStatus::Rejected));

    let manual = service
        .override_classification(&product, &ex("custom"), 0.8)
        .await
        .expect("override");
    assert!(manual.manual);
    assert_eq!(manual.status, ReviewStatus::Approved);
    assert!(service
        .list(&product)
        .await
        .expect("list")
        .iter()
        .any(|record| record.concept == ex("custom") && record.manual));

    service
        .reclassify(&product, ClassificationOptions::new().with_threshold(0.5))
        .await
        .expect("reclassify");
    let stored = service.list(&product).await.expect("list");
    assert!(stored
        .iter()
        .any(|record| record.concept == ex("custom") && record.manual));

    let removed = service
        .delete_for_instance(&product)
        .await
        .expect("delete classifications");
    assert!(removed > 0);
    assert!(service.list(&product).await.expect("list").is_empty());
}

#[tokio::test]
async fn generates_embeddings_and_searches_similar_instances() {
    let store = memory_store_handle();
    let provider: Arc<dyn EmbeddingProvider> = Arc::new(HashingEmbeddingProvider::new(64));
    let index: Arc<dyn VectorIndex> = Arc::new(InMemoryVectorIndex::new());
    let service = EmbeddingService::new(Arc::clone(&store), provider, index);

    let switch = ex("switch");
    let router = ex("router");
    let disk = ex("disk");
    insert_instance(
        &store,
        &switch,
        &ex("Product"),
        &[(ex("name"), "network switch")],
    );
    insert_instance(
        &store,
        &router,
        &ex("Product"),
        &[(ex("name"), "network router")],
    );
    insert_instance(&store, &disk, &ex("Product"), &[(ex("name"), "hard drive")]);

    let records = service
        .generate(&switch, &[ex("name")], None)
        .await
        .expect("generate");
    assert_eq!(records.len(), 2);
    assert!(records.iter().any(|record| record.attribute.is_none()));
    assert!(records
        .iter()
        .any(|record| record.attribute == Some(ex("name"))));
    assert!(records
        .iter()
        .all(|record| record.dimensions == 64 && !record.vector.is_empty()));

    let listed = service.list(&switch).await.expect("list embeddings");
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().all(|record| record.vector.is_empty()));

    service
        .generate(&router, &[ex("name")], None)
        .await
        .expect("generate router");
    service
        .generate(&disk, &[ex("name")], None)
        .await
        .expect("generate disk");

    let hits = service
        .search(&switch, 10, None)
        .await
        .expect("search similar");
    let router_score = hits
        .iter()
        .find(|hit| hit.instance == router)
        .expect("router hit")
        .score;
    let disk_score = hits
        .iter()
        .find(|hit| hit.instance == disk)
        .expect("disk hit")
        .score;
    assert!(router_score > disk_score);

    let filtered = service
        .search(
            &switch,
            10,
            Some(VectorFilter {
                instance: Some(router.clone()),
                attribute: None,
                model_id: None,
            }),
        )
        .await
        .expect("filtered search");
    assert!(filtered.iter().all(|hit| hit.instance == router));

    let vector = vec![0.0_f32; 64];
    let metric_hits = service
        .search_vector(&vector, 5, None, SimilarityMetric::Euclidean)
        .await
        .expect("vector search");
    assert!(!metric_hits.is_empty());

    let removed = service.delete(&switch).await.expect("delete embeddings");
    assert!(removed > 0);
    assert!(service.list(&switch).await.expect("list").is_empty());
}

#[tokio::test]
async fn infers_classes_properties_and_applies_policies() {
    let store = memory_store_handle();
    let owl = Arc::new(OwlManager::new(Arc::clone(&store)));
    owl.create_class(OwlClass::new(ex("Animal")))
        .await
        .expect("animal");
    owl.create_class(OwlClass::new(ex("Dog")).with_superclass(ex("Animal")))
        .await
        .expect("dog");
    owl.create_property(
        OwlProperty::new(ex("eats"), PropertyKind::Object).with_domain(ex("Animal")),
    )
    .await
    .expect("eats");
    let skos = setup_taxonomy(&store).await;
    let engine = RuleInferenceEngine::new(Arc::clone(&store), owl, skos);

    let dog = ex("dog-1");
    insert_instance(&store, &dog, &ex("Dog"), &[(ex("eats"), "meat")]);

    let preview = engine
        .preview(&dog, InferenceOptions::new())
        .await
        .expect("preview");
    assert!(preview
        .classes
        .iter()
        .any(|class| class.class == ex("Animal")
            && class.method == InferenceMethod::Subclass
            && class.confidence == 1.0));
    assert!(preview
        .statements
        .iter()
        .any(|statement| statement.quad.predicate == rdf_type()
            && statement.quad.object == Term::Iri(ex("Animal"))));
    assert!(preview.statements.iter().all(|statement| {
        matches!(&statement.quad.graph, GraphName::Iri(graph) if graph == &GraphKind::Inferred.iri())
    }));
    assert!(!preview.explanation.summary.is_empty());
    assert!(!preview.explanation.validation.is_empty());

    let suggested = engine
        .apply(&dog, ApplyPolicy::Suggest, InferenceOptions::new())
        .await
        .expect("suggest");
    assert!(!suggested.applied);
    assert!(!suggested.requires_approval);
    assert!(store
        .get(QuadPattern::new().with_graph(inferred_graph()))
        .expect("inferred graph")
        .is_empty());

    let applied = engine
        .apply(
            &dog,
            ApplyPolicy::Automatic { threshold: 0.9 },
            InferenceOptions::new(),
        )
        .await
        .expect("apply");
    assert!(applied.applied);
    assert!(applied.applied_count > 0);
    assert!(!store
        .get(QuadPattern::new().with_graph(inferred_graph()))
        .expect("inferred graph")
        .is_empty());

    let review = engine
        .apply(&dog, ApplyPolicy::Review, InferenceOptions::new())
        .await
        .expect("review");
    assert!(review.requires_approval);
    assert!(!review.applied);
}

#[tokio::test]
async fn enriches_from_mappings_classifications_and_rules() {
    let store = memory_store_handle();
    let owl = Arc::new(OwlManager::new(Arc::clone(&store)));
    owl.create_class(OwlClass::new(ex("Product")))
        .await
        .expect("product");
    owl.create_property(OwlProperty::new(ex("title"), PropertyKind::Datatype))
        .await
        .expect("title");
    let skos = setup_taxonomy(&store).await;
    let registry = Arc::new(ExternalOntologyManager::new(Arc::clone(&store)));
    registry
        .map_class(
            ClassMapping::new(
                external("EProduct"),
                ex("Product"),
                MappingKind::EquivalentClass,
            )
            .expect("class mapping"),
        )
        .await
        .expect("map class");
    registry
        .map_property(
            PropertyMapping::new(
                external("extName"),
                ex("title"),
                MappingKind::EquivalentProperty,
            )
            .expect("property mapping"),
        )
        .await
        .expect("map property");
    let engine =
        RuleInferenceEngine::new(Arc::clone(&store), owl, skos.clone()).with_external(registry);

    let mapped = ex("mapped-1");
    insert_instance(
        &store,
        &mapped,
        &external("EProduct"),
        &[(external("extName"), "Widget")],
    );
    let preview = engine
        .preview(&mapped, InferenceOptions::new())
        .await
        .expect("mapping preview");
    assert!(preview
        .classes
        .iter()
        .any(|class| class.class == ex("Product") && class.method == InferenceMethod::Mapping));
    assert!(preview.properties.iter().any(|property| {
        property.property == ex("title")
            && property.method == InferenceMethod::Mapping
            && property.value == Term::string("Widget")
    }));
    assert!(!preview.explanation.mappings.is_empty());

    let classify = ClassificationService::new(Arc::clone(&store), skos);
    let classified = ex("classified-1");
    insert_instance(
        &store,
        &classified,
        &ex("Product"),
        &[(ex("name"), "network gear")],
    );
    let results = classify
        .classify(
            &classified,
            ClassificationOptions::new().with_threshold(0.5),
        )
        .await
        .expect("classify");
    classify
        .review(&results[0].id, true)
        .await
        .expect("approve classification");
    let preview = engine
        .preview(&classified, InferenceOptions::new())
        .await
        .expect("classification preview");
    assert!(preview.classes.iter().any(|class| {
        class.class == results[0].concept && class.method == InferenceMethod::Classification
    }));
    assert!(preview.properties.iter().any(|property| {
        property.property == Id::new("urn:onto:schema:classifiedAs").expect("classifiedAs")
            && property.method == InferenceMethod::Classification
    }));

    let ruled = ex("ruled-1");
    insert_instance(
        &store,
        &ruled,
        &ex("Product"),
        &[(ex("name"), "network switch")],
    );
    let preview = engine
        .preview(
            &ruled,
            InferenceOptions::new().with_rule(ClassificationRule::new(
                "network-rule",
                ex("name"),
                RuleOperator::Contains,
                "network",
                ex("networking"),
                0.88,
            )),
        )
        .await
        .expect("rule preview");
    assert!(preview.classes.iter().any(|class| {
        class.class == ex("networking")
            && class.method == InferenceMethod::Rule
            && (class.confidence - 0.88).abs() < f64::EPSILON
    }));
    assert!(preview
        .explanation
        .rules
        .contains(&"network-rule".to_owned()));
}

#[tokio::test]
async fn dispatches_semantic_commands() {
    let service = ApplicationService::new(memory_store_handle());
    let scheme = ex("command-categories");
    service
        .skos()
        .create_scheme(ConceptScheme::new(scheme.clone()))
        .await
        .expect("create scheme");
    service
        .skos()
        .create_concept(
            Concept::new(ex("networking"))
                .with_scheme(scheme.clone())
                .with_pref_label("Networking Equipment", "en"),
        )
        .await
        .expect("create concept");
    let store = Arc::clone(service.store());
    insert_instance(
        &store,
        &ex("command-1"),
        &ex("Product"),
        &[(ex("name"), "Networking Equipment")],
    );

    let classified = service
        .execute(Command::Semantic(SemanticCommand::Classify {
            instance: ex("command-1"),
            options: ClassificationOptions::new().with_threshold(0.5),
        }))
        .await
        .expect("classify command");
    match &classified {
        CommandResult::Classifications(results) => assert_eq!(results.len(), 1),
        other => panic!("unexpected result: {other:?}"),
    }

    let listed = service
        .execute(Command::Semantic(SemanticCommand::ListClassifications {
            instance: ex("command-1"),
        }))
        .await
        .expect("list classifications");
    match listed {
        CommandResult::Classifications(results) => assert_eq!(results.len(), 1),
        other => panic!("unexpected result: {other:?}"),
    }

    let embedded = service
        .execute(Command::Semantic(SemanticCommand::GenerateEmbeddings {
            instance: ex("command-1"),
            attributes: vec![ex("name")],
        }))
        .await
        .expect("embedding command");
    match embedded {
        CommandResult::Embeddings(records) => assert_eq!(records.len(), 2),
        other => panic!("unexpected result: {other:?}"),
    }

    let similar = service
        .execute(Command::Semantic(SemanticCommand::SearchSimilar {
            instance: ex("command-1"),
            limit: 5,
            filter: None,
        }))
        .await
        .expect("similarity command");
    assert!(matches!(similar, CommandResult::Similarity(_)));

    let preview = service
        .execute(Command::Semantic(SemanticCommand::PreviewInference {
            instance: ex("command-1"),
            options: InferenceOptions::new(),
        }))
        .await
        .expect("preview command");
    assert!(matches!(preview, CommandResult::Preview(_)));

    let applied = service
        .execute(Command::Semantic(SemanticCommand::ApplyInference {
            instance: ex("command-1"),
            options: InferenceOptions::new(),
            policy: ApplyPolicy::Automatic { threshold: 0.8 },
        }))
        .await
        .expect("apply command");
    match applied {
        CommandResult::Inference(report) => {
            assert!(report.applied || report.preview.statements.is_empty())
        }
        other => panic!("unexpected result: {other:?}"),
    }
}
