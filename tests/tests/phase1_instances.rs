//! Phase 1 instance CRUD, statement operations, relationships, provenance, and
//! asserted/inferred separation tests.

use std::sync::Arc;

use ontology_application::InstanceService;
use ontology_core::error::Error;
use ontology_core::graph::GraphKind;
use ontology_core::model::{
    GraphName, InstancePatch, InstanceQuery, NewInstance, Quad, QuadPattern, RelationshipDirection,
    StatementInput, Term,
};
use ontology_core::namespace::NamespaceRegistry;
use ontology_core::rdf::RdfStore;
use ontology_integration_tests::{asserted_graph, ex, inferred_graph, memory_store};

fn service() -> InstanceService {
    InstanceService::new(Arc::new(memory_store()))
}

#[test]
fn creates_reads_updates_and_deletes_instances() {
    let service = service();
    let product = ex("product/123");

    let instance = service
        .create_instance(
            NewInstance::new(product.clone())
                .with_type(ex("Product"))
                .with_property(ex("name"), Term::string("Example Product"))
                .with_property(
                    ex("description"),
                    Term::language_tagged("A product", "en").expect("literal"),
                )
                .with_source("unit-test"),
        )
        .expect("create");

    assert_eq!(instance.id, product);
    assert_eq!(instance.types, vec![ex("Product")]);
    assert_eq!(instance.graph, asserted_graph());
    assert_eq!(instance.statements.len(), 3);

    assert!(matches!(
        service.create_instance(NewInstance::new(product.clone()).with_type(ex("Product"))),
        Err(Error::Conflict { .. })
    ));

    let listed = service
        .list_instances(InstanceQuery::default())
        .expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, product);
    assert_eq!(listed[0].types, vec![ex("Product")]);

    let filtered = service
        .list_instances(InstanceQuery::default().with_type(ex("Widget")))
        .expect("filter");
    assert!(filtered.is_empty());

    let mut patch = InstancePatch::default();
    patch.types = Some(vec![ex("Widget")]);
    patch.properties.insert(ex("description"), None);
    patch
        .properties
        .insert(ex("name"), Some(vec![Term::string("Renamed Product")]));
    let updated = service
        .update_instance(&product, patch, None, Some("unit-test"))
        .expect("update");

    assert_eq!(updated.types, vec![ex("Widget")]);
    let names: Vec<_> = updated
        .statements
        .iter()
        .filter(|statement| statement.quad.predicate == ex("name"))
        .map(|statement| statement.quad.object.clone())
        .collect();
    assert_eq!(names, vec![Term::string("Renamed Product")]);
    assert!(!updated
        .statements
        .iter()
        .any(|statement| statement.quad.predicate == ex("description")));

    // After the update the instance holds one type and one name statement.
    let deleted = service.delete_instance(&product, None).expect("delete");
    assert_eq!(deleted, 2);
    assert!(matches!(
        service.get_instance(&product, None),
        Err(Error::NotFound { .. })
    ));
}

#[test]
fn manages_statement_crud_and_batch_operations() {
    let service = service();
    let id = ex("product/1");
    service
        .create_instance(NewInstance::new(id.clone()).with_type(ex("Product")))
        .expect("create");

    let added = service
        .add_statements(
            &id,
            &[
                StatementInput {
                    predicate: ex("sku"),
                    object: Term::string("ABC-1"),
                },
                StatementInput {
                    predicate: ex("stock"),
                    object: Term::Literal(ontology_core::model::Literal::integer(12)),
                },
            ],
            None,
            Some("erp"),
        )
        .expect("add statements");
    assert_eq!(added.len(), 3);

    let statements = service.statements(&id, None).expect("statements");
    assert_eq!(statements.len(), 3);

    let sku = statements
        .iter()
        .find(|statement| statement.quad.predicate == ex("sku"))
        .expect("sku statement")
        .clone();
    let removed = service
        .delete_statements(&id, &[sku.id.clone()], None)
        .expect("delete statement");
    assert_eq!(removed, 1);
    assert_eq!(service.statements(&id, None).expect("statements").len(), 2);

    assert!(matches!(
        service.delete_statements(&id, &[sku.id], None),
        Err(Error::NotFound { .. })
    ));
}

#[test]
fn tracks_relationships_and_provenance() {
    let store = Arc::new(memory_store());
    let service = InstanceService::new(Arc::clone(&store) as Arc<dyn RdfStore>);
    let source = ex("source");
    let target = ex("target");

    service
        .create_instance(NewInstance::new(source.clone()).with_type(ex("Thing")))
        .expect("create source");
    service
        .create_instance(
            NewInstance::new(target.clone())
                .with_type(ex("Thing"))
                .with_property(ex("relatedTo"), Term::Iri(source.clone()))
                .with_source("seed"),
        )
        .expect("create target");

    let relationships = service.relationships(&source).expect("relationships");
    assert!(relationships.iter().any(|relationship| {
        relationship.direction == RelationshipDirection::Incoming
            && relationship.predicate == ex("relatedTo")
            && relationship.target == Term::Iri(target.clone())
    }));
    assert!(relationships.iter().any(|relationship| {
        relationship.direction == RelationshipDirection::Outgoing
            && relationship.predicate.as_str() == ontology_core::namespace::RDF_TYPE
    }));

    let provenance = store
        .get(QuadPattern::new().with_graph(GraphName::Iri(GraphKind::Provenance.iri())))
        .expect("provenance graph");
    assert!(!provenance.is_empty());
    assert!(provenance
        .iter()
        .any(|quad| quad.predicate == NamespaceRegistry::onto("operation")));
    assert!(provenance
        .iter()
        .any(|quad| quad.predicate == NamespaceRegistry::onto("source")
            && quad.object == Term::string("seed")));
}

#[test]
fn keeps_asserted_and_inferred_statements_apart() {
    let store = Arc::new(memory_store());
    let service = InstanceService::new(Arc::clone(&store) as Arc<dyn RdfStore>);
    let id = ex("resource");

    service
        .create_instance(NewInstance::new(id.clone()).with_type(ex("Thing")))
        .expect("create");
    store
        .insert(&[Quad::in_graph(
            Term::Iri(id.clone()),
            ex("derivedFrom"),
            Term::string("rule-1"),
            inferred_graph(),
        )])
        .expect("insert inferred");

    let asserted = service
        .statements(&id, Some(&asserted_graph()))
        .expect("asserted statements");
    assert_eq!(asserted.len(), 1);
    assert_eq!(
        asserted[0].kind,
        ontology_core::model::StatementKind::Asserted
    );

    let inferred = service
        .statements(&id, Some(&inferred_graph()))
        .expect("inferred statements");
    assert_eq!(inferred.len(), 1);
    assert_eq!(
        inferred[0].kind,
        ontology_core::model::StatementKind::Inferred
    );

    let all = service
        .get_instance(&id, None)
        .expect("instance across graphs");
    assert_eq!(all.statements.len(), 2);
    assert_eq!(service.instance_graphs(&id).expect("graphs").len(), 2);
}

#[test]
fn rejects_conflicting_duplicate_creation_in_the_same_graph() {
    let service = service();
    let id = ex("duplicate");
    service
        .create_instance(NewInstance::new(id.clone()).with_type(ex("Thing")))
        .expect("first create");
    assert!(matches!(
        service.create_instance(NewInstance::new(id.clone()).with_type(ex("Thing"))),
        Err(Error::Conflict { .. })
    ));

    // A different graph is not a conflict.
    service
        .create_instance(
            NewInstance::new(id.clone())
                .with_type(ex("Thing"))
                .with_graph(GraphName::Iri(GraphKind::Classification.iri())),
        )
        .expect("create in another graph");
}
