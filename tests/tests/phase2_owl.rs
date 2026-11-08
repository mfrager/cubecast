//! Phase 2.1 tests: OWL class and property management, restrictions,
//! hierarchy, validation, versioning, schema discovery, and command dispatch.

use std::sync::Arc;

use ontology_application::ApplicationService;
use ontology_core::command::{Command, CommandResult, OntologyCommand};
use ontology_core::error::Error;
use ontology_core::id::Id;
use ontology_core::owl::{
    ClassQuery, GraphScope, Ontology, OntologyManager, OwlClass, OwlProperty,
    PropertyCharacteristic, PropertyKind, PropertyQuery, Restriction, Severity,
};
use ontology_core::rdf::{OntologyService, RdfStore};
use ontology_integration_tests::{ex, memory_store_handle};
use ontology_owl::OwlManager;

fn manager() -> OwlManager {
    OwlManager::new(memory_store_handle())
}

#[tokio::test]
async fn creates_and_reads_classes() {
    let manager = manager();
    let product = ex("Product");

    manager
        .create_class(
            OwlClass::new(product.clone())
                .with_label("en", "Product")
                .with_label("", "Produkt")
                .with_comment("en", "A sellable item")
                .with_superclass(ex("Thing"))
                .with_equivalent_class(ex("Item"))
                .with_disjoint_class(ex("Service")),
        )
        .await
        .expect("create class");

    let class = manager.get_class(&product).await.expect("get class");
    assert_eq!(class.labels.get("en").map(String::as_str), Some("Product"));
    assert_eq!(class.labels.get("").map(String::as_str), Some("Produkt"));
    assert_eq!(
        class.comments.get("en").map(String::as_str),
        Some("A sellable item")
    );
    assert_eq!(class.sub_class_of, vec![ex("Thing")]);
    assert_eq!(class.equivalent_classes, vec![ex("Item")]);
    assert_eq!(class.disjoint_with, vec![ex("Service")]);

    assert!(matches!(
        manager.create_class(OwlClass::new(product)).await,
        Err(Error::Conflict { .. })
    ));

    let listed = manager
        .list_classes(ClassQuery::default())
        .await
        .expect("list classes");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, ex("Product"));

    let by_label = manager
        .list_classes(ClassQuery::default().with_label("prod"))
        .await
        .expect("label search");
    assert_eq!(by_label.len(), 1);
    assert!(manager
        .list_classes(ClassQuery::default().with_label("nothing"))
        .await
        .expect("label search")
        .is_empty());
}

#[tokio::test]
async fn updates_replace_definitions_and_restrictions() {
    let manager = manager();
    let animal = ex("Animal");
    let plant = ex("Plant");
    manager
        .create_class(OwlClass::new(animal.clone()).with_superclass(ex("Thing")))
        .await
        .expect("create");

    manager
        .update_class(
            OwlClass::new(animal.clone())
                .with_superclass(plant.clone())
                .with_restriction(Restriction::MinCardinality {
                    property: ex("legs"),
                    cardinality: 2,
                    class: None,
                }),
        )
        .await
        .expect("update");

    let class = manager.get_class(&animal).await.expect("get");
    assert_eq!(class.sub_class_of, vec![plant]);
    assert_eq!(class.restrictions.len(), 1);
    assert!(matches!(
        class.restrictions[0],
        Restriction::MinCardinality { cardinality: 2, .. }
    ));

    // A second update replaces the restriction instead of accumulating it.
    manager
        .update_class(OwlClass::new(animal.clone()))
        .await
        .expect("clear update");
    let cleared = manager.get_class(&animal).await.expect("get");
    assert!(cleared.restrictions.is_empty());
    assert!(cleared.sub_class_of.is_empty());
}

#[tokio::test]
async fn manages_object_and_datatype_properties() {
    let manager = manager();
    manager
        .create_class(OwlClass::new(ex("Product")))
        .await
        .unwrap();
    manager
        .create_class(OwlClass::new(ex("Part")))
        .await
        .unwrap();

    let has_part = ex("hasPart");
    manager
        .create_property(
            OwlProperty::new(has_part.clone(), PropertyKind::Object)
                .with_label("en", "has part")
                .with_domain(ex("Product"))
                .with_range(ex("Part"))
                .with_inverse(ex("partOf"))
                .with_characteristic(PropertyCharacteristic::Transitive)
                .with_characteristic(PropertyCharacteristic::Asymmetric),
        )
        .await
        .expect("create object property");

    let property = manager.get_property(&has_part).await.expect("get property");
    assert_eq!(property.kind, PropertyKind::Object);
    assert_eq!(property.domain, vec![ex("Product")]);
    assert_eq!(property.range, vec![ex("Part")]);
    assert_eq!(property.inverse_of, Some(ex("partOf")));
    assert!(property
        .characteristics
        .contains(&PropertyCharacteristic::Transitive));
    assert!(property
        .characteristics
        .contains(&PropertyCharacteristic::Asymmetric));

    let price = ex("price");
    let decimal = Id::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap();
    manager
        .create_property(
            OwlProperty::new(price.clone(), PropertyKind::Datatype)
                .with_domain(ex("Product"))
                .with_range(decimal.clone()),
        )
        .await
        .expect("create datatype property");

    let datatypes = manager
        .list_properties(PropertyQuery::new().with_kind(PropertyKind::Datatype))
        .await
        .expect("list datatype properties");
    assert_eq!(datatypes.len(), 1);
    assert_eq!(datatypes[0].id, price);
    assert_eq!(datatypes[0].range, vec![decimal]);

    // Updating replaces the whole definition.
    manager
        .update_property(
            OwlProperty::new(price.clone(), PropertyKind::Datatype)
                .with_domain(ex("Part"))
                .with_range(Id::new("http://www.w3.org/2001/XMLSchema#string").unwrap()),
        )
        .await
        .expect("update property");
    let updated = manager.get_property(&price).await.expect("get property");
    assert_eq!(updated.domain, vec![ex("Part")]);

    assert!(matches!(
        manager
            .create_property(OwlProperty::new(price, PropertyKind::Datatype))
            .await,
        Err(Error::Conflict { .. })
    ));
}

#[tokio::test]
async fn computes_class_hierarchy() {
    let manager = manager();
    manager
        .create_class(OwlClass::new(ex("Thing")))
        .await
        .unwrap();
    for (child, parent) in [("Dog", "Mammal"), ("Mammal", "Animal"), ("Animal", "Thing")] {
        manager
            .create_class(OwlClass::new(ex(child)).with_superclass(ex(parent)))
            .await
            .unwrap();
    }

    let dog = manager.hierarchy(&ex("Dog")).await.expect("hierarchy");
    assert_eq!(dog.direct_superclasses, vec![ex("Mammal")]);
    assert!(dog.ancestors.contains(&ex("Animal")));
    assert!(dog.ancestors.contains(&ex("Thing")));
    assert!(dog.descendants.is_empty());

    let animal = manager.hierarchy(&ex("Animal")).await.expect("hierarchy");
    assert_eq!(animal.direct_subclasses, vec![ex("Mammal")]);
    assert!(animal.descendants.contains(&ex("Dog")));

    assert!(matches!(
        manager.hierarchy(&ex("Missing")).await,
        Err(Error::NotFound { .. })
    ));
}

#[tokio::test]
async fn round_trips_restrictions() {
    let manager = manager();
    let contract = ex("Contract");
    manager
        .create_property(OwlProperty::new(ex("party"), PropertyKind::Object))
        .await
        .unwrap();
    manager
        .create_property(OwlProperty::new(ex("status"), PropertyKind::Datatype))
        .await
        .unwrap();
    manager
        .create_class(OwlClass::new(ex("Party")))
        .await
        .unwrap();

    manager
        .create_class(
            OwlClass::new(contract.clone())
                .with_restriction(Restriction::SomeValuesFrom {
                    property: ex("party"),
                    class: ex("Party"),
                })
                .with_restriction(Restriction::MinCardinality {
                    property: ex("party"),
                    cardinality: 1,
                    class: Some(ex("Party")),
                })
                .with_restriction(Restriction::HasValue {
                    property: ex("status"),
                    value: ontology_core::model::Term::string("active"),
                }),
        )
        .await
        .expect("create class with restrictions");

    let loaded = manager.get_class(&contract).await.expect("get");
    assert_eq!(loaded.restrictions.len(), 3);
    assert!(loaded.restrictions.iter().any(|restriction| matches!(
        restriction,
        Restriction::SomeValuesFrom { class, .. } if class == &ex("Party")
    )));
    assert!(loaded.restrictions.iter().any(|restriction| matches!(
        restriction,
        Restriction::MinCardinality {
            cardinality: 1,
            class: Some(_),
            ..
        }
    )));
    assert!(loaded.restrictions.iter().any(|restriction| matches!(
        restriction,
        Restriction::HasValue { value, .. }
            if value == &ontology_core::model::Term::string("active")
    )));
}

#[tokio::test]
async fn validates_schema_structure() {
    let manager = manager();
    manager
        .create_class(OwlClass::new(ex("A")).with_superclass(ex("B")))
        .await
        .unwrap();
    manager
        .create_class(OwlClass::new(ex("B")).with_superclass(ex("A")))
        .await
        .unwrap();
    manager
        .create_class(OwlClass::new(ex("C")).with_superclass(ex("Missing")))
        .await
        .unwrap();
    manager
        .create_property(
            OwlProperty::new(ex("weird"), PropertyKind::Datatype).with_range(ex("NotADatatype")),
        )
        .await
        .unwrap();

    let report = manager
        .validate(GraphScope::Schema)
        .await
        .expect("validate schema");
    assert!(!report.is_valid());
    assert!(report.checked_classes >= 3);
    assert!(report.checked_properties >= 1);
    assert!(report
        .issues
        .iter()
        .any(|issue| issue.code == "subclass-cycle" && issue.severity == Severity::Error));
    assert!(report.issues.iter().any(|issue| {
        issue.code == "unknown-superclass" && issue.severity == Severity::Warning
    }));
    assert!(report
        .issues
        .iter()
        .any(|issue| issue.code == "non-datatype-range"));
    assert!(report.errors().count() >= 2);
}

#[tokio::test]
async fn tracks_ontology_versions() {
    let manager = manager();
    let ontology = ex("catalog");
    manager
        .create_ontology(
            Ontology::new(ontology.clone())
                .with_label("en", "Catalog")
                .with_version(ex("catalog/1.0"), "1.0"),
        )
        .await
        .expect("create ontology");
    manager
        .update_ontology(Ontology::new(ontology.clone()).with_version(ex("catalog/1.1"), "1.1"))
        .await
        .expect("update ontology");

    let loaded = manager.get_ontology(&ontology).await.expect("get ontology");
    assert_eq!(loaded.version_info.as_deref(), Some("1.1"));
    assert_eq!(loaded.version_iri, Some(ex("catalog/1.1")));

    let versions = manager
        .ontology_versions(&ontology)
        .await
        .expect("versions");
    assert_eq!(versions.len(), 2);
    assert!(versions
        .iter()
        .any(|version| version.version_info.as_deref() == Some("1.0")));
    assert!(versions
        .iter()
        .any(|version| version.version_info.as_deref() == Some("1.1")));

    assert_eq!(manager.list_ontologies().await.expect("list").len(), 1);
    assert!(matches!(
        manager.create_ontology(Ontology::new(ontology)).await,
        Err(Error::Conflict { .. })
    ));
}

#[tokio::test]
async fn discovers_applicable_schema() {
    let manager = manager();
    manager
        .create_class(OwlClass::new(ex("Animal")))
        .await
        .unwrap();
    manager
        .create_class(OwlClass::new(ex("Dog")).with_superclass(ex("Animal")))
        .await
        .unwrap();
    manager
        .create_class(OwlClass::new(ex("Plant")))
        .await
        .unwrap();
    manager
        .create_property(
            OwlProperty::new(ex("eats"), PropertyKind::Object).with_domain(ex("Animal")),
        )
        .await
        .unwrap();
    manager
        .create_property(
            OwlProperty::new(ex("photosynthesizes"), PropertyKind::Datatype)
                .with_domain(ex("Plant")),
        )
        .await
        .unwrap();

    let applicable = manager
        .applicable_properties(&ex("Dog"))
        .await
        .expect("applicable properties");
    let ids: Vec<Id> = applicable
        .iter()
        .map(|property| property.id.clone())
        .collect();
    assert!(ids.contains(&ex("eats")));
    assert!(!ids.contains(&ex("photosynthesizes")));

    manager
        .create_class(
            OwlClass::new(ex("Pet")).with_restriction(Restriction::SomeValuesFrom {
                property: ex("eats"),
                class: ex("Animal"),
            }),
        )
        .await
        .unwrap();
    let classes = manager
        .applicable_classes(&[ex("eats")])
        .await
        .expect("applicable classes");
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].id, ex("Pet"));
}

#[tokio::test]
async fn deletes_classes_and_properties() {
    let manager = manager();
    manager
        .create_property(OwlProperty::new(ex("part"), PropertyKind::Object))
        .await
        .unwrap();
    manager
        .create_class(
            OwlClass::new(ex("Whole")).with_restriction(Restriction::SomeValuesFrom {
                property: ex("part"),
                class: ex("Whole"),
            }),
        )
        .await
        .unwrap();

    let affected = manager
        .delete_class(&ex("Whole"))
        .await
        .expect("delete class");
    assert!(affected >= 2);
    assert!(matches!(
        manager.get_class(&ex("Whole")).await,
        Err(Error::NotFound { .. })
    ));

    let affected = manager
        .delete_property(&ex("part"))
        .await
        .expect("delete property");
    assert!(affected >= 1);
    assert!(matches!(
        manager.get_property(&ex("part")).await,
        Err(Error::NotFound { .. })
    ));
}

#[tokio::test]
async fn persists_schema_across_restarts() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("store");

    {
        let store: Arc<dyn RdfStore> =
            Arc::new(ontology_rdf::OxigraphStore::open(&path).expect("open store"));
        let manager = OwlManager::new(store);
        manager
            .create_class(OwlClass::new(ex("Persisted")))
            .await
            .unwrap();
        manager
            .create_property(OwlProperty::new(
                ex("persistedProperty"),
                PropertyKind::Datatype,
            ))
            .await
            .unwrap();
    }

    let store: Arc<dyn RdfStore> =
        Arc::new(ontology_rdf::OxigraphStore::open(&path).expect("reopen store"));
    let manager = OwlManager::new(store);
    assert_eq!(
        manager.get_class(&ex("Persisted")).await.expect("class").id,
        ex("Persisted")
    );
    assert_eq!(
        manager
            .get_property(&ex("persistedProperty"))
            .await
            .expect("property")
            .id,
        ex("persistedProperty")
    );
}

#[tokio::test]
async fn dispatches_ontology_commands() {
    let service = ApplicationService::new(memory_store_handle());

    let created = service
        .execute(Command::Ontology(OntologyCommand::CreateClass(
            OwlClass::new(ex("Widget")).with_label("en", "Widget"),
        )))
        .await
        .expect("create class command");
    match created {
        CommandResult::Resource(resource) => {
            assert_eq!(resource.kind, "owl_class");
            assert_eq!(resource.id, ex("Widget").as_str());
        }
        other => panic!("unexpected result: {other:?}"),
    }

    let fetched = service
        .execute(Command::Ontology(OntologyCommand::GetClass {
            id: ex("Widget"),
        }))
        .await
        .expect("get class command");
    assert!(matches!(fetched, CommandResult::Resource(_)));

    let listed = service
        .execute(Command::Ontology(OntologyCommand::ListClasses(
            ClassQuery::default(),
        )))
        .await
        .expect("list classes command");
    match listed {
        CommandResult::Collection(collection) => assert_eq!(collection.count, 1),
        other => panic!("unexpected result: {other:?}"),
    }

    let report = service
        .execute(Command::Ontology(OntologyCommand::Validate {
            scope: GraphScope::Schema,
        }))
        .await
        .expect("validate command");
    assert!(matches!(report, CommandResult::Validation(_)));

    let deleted = service
        .execute(Command::Ontology(OntologyCommand::DeleteClass {
            id: ex("Widget"),
        }))
        .await
        .expect("delete class command");
    match deleted {
        CommandResult::Report(report) => assert!(report.affected >= 1),
        other => panic!("unexpected result: {other:?}"),
    }
}
