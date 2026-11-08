//! Phase 2.4 and 2.5 tests: external ontology import, metadata discovery,
//! synchronization, deletion, and class/property mappings.

use std::fs;
use std::sync::Arc;

use ontology_application::ApplicationService;
use ontology_core::command::{Command, CommandResult, ExternalCommand};
use ontology_core::error::Error;
use ontology_core::external::{
    ClassMapping, ExternalOntologyRegistry, ImportRequest, MappingKind, MappingQuery,
    PropertyMapping,
};
use ontology_core::id::Id;
use ontology_core::model::{GraphName, QuadPattern, Term};
use ontology_core::owl::{OntologyManager, OwlClass, OwlProperty, PropertyKind};
use ontology_core::rdf::OntologyService;
use ontology_import::ExternalOntologyManager;
use ontology_integration_tests::{ex, memory_store_handle};
use ontology_owl::OwlManager;

const CATALOG: &str = r#"
@prefix ex: <https://external.example.org/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

<https://external.example.org/onto> a owl:Ontology ;
    rdfs:label "External Catalog" ;
    owl:versionInfo "2.1" .

ex:Book a owl:Class .
ex:Author a owl:Class .
ex:hasAuthor a owl:ObjectProperty ;
    rdfs:domain ex:Book ;
    rdfs:range ex:Author .
"#;

fn external(local: &str) -> Id {
    Id::new(format!("https://external.example.org/{local}")).expect("external IRI")
}

fn request() -> ImportRequest {
    ImportRequest::new("catalog.ttl").with_format("turtle")
}

#[tokio::test]
async fn imports_and_preserves_iris() {
    let store = memory_store_handle();
    let registry = ExternalOntologyManager::new(Arc::clone(&store));
    let report = registry
        .import_from_slice(request(), CATALOG.as_bytes())
        .await
        .expect("import");

    assert_eq!(report.ontology, external("onto"));
    assert_eq!(report.class_count, 2);
    assert_eq!(report.property_count, 1);
    assert!(report.inserted >= 8);
    assert!(!report.already_present);

    let record = registry.get(&report.ontology).await.expect("get");
    assert_eq!(record.title.as_deref(), Some("External Catalog"));
    assert_eq!(record.version.as_deref(), Some("2.1"));
    assert_eq!(
        record.namespace.as_deref(),
        Some("https://external.example.org/")
    );
    assert_eq!(record.format.as_deref(), Some("turtle"));
    assert_eq!(record.quad_count, report.inserted as usize);
    assert_eq!(registry.list().await.expect("list").len(), 1);

    // The published IRI is preserved inside the dedicated import graph.
    let owl_class = Id::new("http://www.w3.org/2002/07/owl#Class").expect("owl:Class");
    let quads = store
        .get(
            QuadPattern::new()
                .with_subject(Term::Iri(external("Book")))
                .with_graph(record.graph.clone()),
        )
        .expect("subject quads");
    assert!(quads
        .iter()
        .any(|quad| quad.object == Term::Iri(owl_class.clone())));
}

#[tokio::test]
async fn reimports_idempotently_and_rejects_source_changes() {
    let store = memory_store_handle();
    let registry = ExternalOntologyManager::new(Arc::clone(&store));
    let first = registry
        .import_from_slice(request(), CATALOG.as_bytes())
        .await
        .expect("import");

    let again = registry
        .import_from_slice(request(), CATALOG.as_bytes())
        .await
        .expect("re-import");
    assert!(again.already_present);
    assert_eq!(again.inserted, first.inserted);

    let record = registry.get(&first.ontology).await.expect("get");
    let quads = store
        .get(QuadPattern::new().with_graph(record.graph.clone()))
        .expect("graph quads");
    assert_eq!(quads.len(), record.quad_count);
    assert_eq!(quads.len(), first.inserted as usize);

    let conflict = registry
        .import_from_slice(
            ImportRequest::new("elsewhere.ttl").with_format("turtle"),
            CATALOG.as_bytes(),
        )
        .await;
    assert!(matches!(conflict, Err(Error::Conflict { .. })));
}

#[tokio::test]
async fn syncs_from_the_recorded_source() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("catalog.ttl");
    fs::write(&path, CATALOG).expect("write ontology");

    let store = memory_store_handle();
    let registry = ExternalOntologyManager::new(Arc::clone(&store));
    let imported = registry
        .import(ImportRequest::new(path.to_str().expect("path")))
        .await
        .expect("import file");
    assert_eq!(imported.class_count, 2);

    fs::write(&path, format!("{CATALOG}\nex:Publisher a owl:Class .\n")).expect("update ontology");
    let synced = registry
        .sync(&imported.ontology)
        .await
        .expect("synchronize");
    assert!(synced.already_present);
    assert_eq!(synced.class_count, 3);

    let record = registry.get(&imported.ontology).await.expect("get");
    assert_eq!(record.class_count, 3);
    assert_eq!(record.quad_count, synced.inserted as usize);
    let quads = store
        .get(QuadPattern::new().with_graph(record.graph.clone()))
        .expect("graph quads");
    assert_eq!(quads.len(), record.quad_count);

    let remote = registry
        .import(ImportRequest::new("https://example.org/onto.ttl"))
        .await;
    assert!(matches!(remote, Err(Error::Unsupported { .. })));
}

#[tokio::test]
async fn deletes_imported_ontologies() {
    let store = memory_store_handle();
    let registry = ExternalOntologyManager::new(Arc::clone(&store));
    let report = registry
        .import_from_slice(request(), CATALOG.as_bytes())
        .await
        .expect("import");
    let record = registry.get(&report.ontology).await.expect("get");

    let affected = registry.delete(&report.ontology).await.expect("delete");
    assert!(affected > 0);
    assert!(matches!(
        registry.get(&report.ontology).await,
        Err(Error::NotFound { .. })
    ));
    assert!(registry.list().await.expect("list").is_empty());
    assert!(store
        .get(QuadPattern::new().with_graph(record.graph))
        .expect("graph quads")
        .is_empty());
}

#[tokio::test]
async fn maps_classes_and_properties() {
    let store = memory_store_handle();
    let registry = ExternalOntologyManager::new(Arc::clone(&store));
    let owl = OwlManager::new(Arc::clone(&store));
    let report = registry
        .import_from_slice(request(), CATALOG.as_bytes())
        .await
        .expect("import");

    owl.create_class(OwlClass::new(ex("Book")))
        .await
        .expect("internal class");
    owl.create_property(OwlProperty::new(ex("hasAuthor"), PropertyKind::Object))
        .await
        .expect("internal property");

    let mapping = ClassMapping::new(external("Book"), ex("Book"), MappingKind::EquivalentClass)
        .expect("class mapping")
        .with_confidence(0.9)
        .expect("confidence");
    let id = registry
        .map_class(mapping.clone())
        .await
        .expect("record mapping");
    assert_eq!(id, mapping.id);
    assert!(matches!(
        registry
            .map_class(
                ClassMapping::new(external("Book"), ex("Book"), MappingKind::EquivalentClass)
                    .expect("duplicate")
            )
            .await,
        Err(Error::Conflict { .. })
    ));

    let listed = registry
        .list_class_mappings(MappingQuery::default())
        .await
        .expect("list class mappings");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].external, external("Book"));
    assert_eq!(listed[0].internal, ex("Book"));
    assert_eq!(listed[0].confidence, Some(0.9));

    registry
        .map_property(
            PropertyMapping::new(
                external("hasAuthor"),
                ex("hasAuthor"),
                MappingKind::EquivalentProperty,
            )
            .expect("property mapping"),
        )
        .await
        .expect("record property mapping");
    assert_eq!(
        registry
            .list_property_mappings(MappingQuery::default())
            .await
            .expect("list property mappings")
            .len(),
        1
    );

    let validation = registry
        .validate_mappings()
        .await
        .expect("validate mappings");
    assert_eq!(validation.checked, 2);
    assert!(validation.issues.is_empty());
    assert!(validation.is_valid());

    // A mapping to an undeclared internal class is reported.
    registry
        .map_class(
            ClassMapping::new(
                external("Author"),
                ex("Writer"),
                MappingKind::EquivalentClass,
            )
            .expect("mapping"),
        )
        .await
        .expect("record mapping");
    let validation = registry
        .validate_mappings()
        .await
        .expect("validate mappings");
    assert_eq!(validation.checked, 3);
    assert!(validation
        .issues
        .iter()
        .any(|issue| issue.message.contains("internal class")));
    assert!(validation.is_valid());

    // Deleting the import leaves the mapping in place but flags the external
    // term as undeclared.
    registry
        .delete(&report.ontology)
        .await
        .expect("delete ontology");
    let validation = registry
        .validate_mappings()
        .await
        .expect("validate mappings");
    assert!(validation
        .issues
        .iter()
        .any(|issue| issue.message.contains("external class")));
}

#[tokio::test]
async fn dispatches_external_commands() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("catalog.ttl");
    fs::write(&path, CATALOG).expect("write ontology");
    let service = ApplicationService::new(memory_store_handle());

    let imported = service
        .execute(Command::External(ExternalCommand::Import(
            ImportRequest::new(path.to_str().expect("path")),
        )))
        .await
        .expect("import command");
    match imported {
        CommandResult::Resource(resource) => {
            assert_eq!(resource.kind, "import_report");
            assert_eq!(resource.id, external("onto").as_str());
        }
        other => panic!("unexpected result: {other:?}"),
    }

    let listed = service
        .execute(Command::External(ExternalCommand::List))
        .await
        .expect("list command");
    match listed {
        CommandResult::Collection(collection) => assert_eq!(collection.count, 1),
        other => panic!("unexpected result: {other:?}"),
    }

    let mapping = ClassMapping::new(external("Book"), ex("Book"), MappingKind::EquivalentClass)
        .expect("mapping");
    let mapped = service
        .execute(Command::External(ExternalCommand::MapClass(mapping)))
        .await
        .expect("map class command");
    match mapped {
        CommandResult::Resource(resource) => assert_eq!(resource.kind, "class_mapping"),
        other => panic!("unexpected result: {other:?}"),
    }

    let validated = service
        .execute(Command::External(ExternalCommand::ValidateMappings))
        .await
        .expect("validate command");
    assert!(matches!(validated, CommandResult::Mapping(_)));
}

#[tokio::test]
async fn keeps_import_graphs_separate() {
    let store = memory_store_handle();
    let registry = ExternalOntologyManager::new(Arc::clone(&store));
    let first = registry
        .import_from_slice(request(), CATALOG.as_bytes())
        .await
        .expect("import");

    let second_doc = CATALOG.replace(
        "https://external.example.org/onto",
        "https://external.example.org/other",
    );
    let second = registry
        .import_from_slice(
            ImportRequest::new("other.ttl")
                .with_format("turtle")
                .with_ontology(external("other")),
            second_doc.as_bytes(),
        )
        .await
        .expect("second import");

    assert_ne!(first.graph, second.graph);
    assert!(matches!(first.graph, GraphName::Iri(_)));
    assert_eq!(registry.list().await.expect("list").len(), 2);
}
