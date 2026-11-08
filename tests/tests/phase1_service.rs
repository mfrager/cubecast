//! Phase 1 shared-service tests: command dispatch and namespace resolution.

use std::sync::Arc;

use ontology_application::ApplicationService;
use ontology_core::command::{Command, CommandResult, RdfCommand};
use ontology_core::id::Id;
use ontology_core::model::{InstanceQuery, NewInstance, QueryResult, StatementInput, Term};
use ontology_core::rdf::{OntologyService, RdfStore};
use ontology_integration_tests::{ex, memory_store_handle};

fn service() -> (Arc<dyn RdfStore>, ApplicationService) {
    let store = memory_store_handle();
    let service = ApplicationService::new(Arc::clone(&store));
    (store, service)
}

#[test]
fn resolves_prefixed_names_and_graphs() {
    let (_, service) = service();
    assert_eq!(
        service.resolve("ex:Product").expect("resolve"),
        ex("Product")
    );
    assert_eq!(
        service
            .resolve_graph("data:asserted")
            .expect("graph")
            .to_string(),
        "urn:onto:data:asserted"
    );
    assert!(service.resolve("nope:Thing").is_err());
}

#[tokio::test]
async fn executes_rdf_commands() {
    let (_, service) = service();
    let id = ex("service/1");

    let created = service
        .execute(Command::Rdf(RdfCommand::CreateInstance(
            NewInstance::new(id.clone())
                .with_type(ex("Thing"))
                .with_property(ex("name"), Term::string("Service Instance")),
        )))
        .await
        .expect("create");
    match created {
        CommandResult::Resource(resource) => {
            assert_eq!(resource.id, id.as_str());
            assert_eq!(resource.kind, "instance");
            assert_eq!(
                resource.resource["types"][0],
                serde_json::json!(id_to_string(&ex("Thing")))
            );
        }
        other => panic!("unexpected result: {other:?}"),
    }

    let fetched = service
        .execute(Command::Rdf(RdfCommand::GetInstance {
            id: id.clone(),
            graph: None,
        }))
        .await
        .expect("get");
    assert!(matches!(fetched, CommandResult::Resource(_)));

    let listed = service
        .execute(Command::Rdf(RdfCommand::ListInstances(
            InstanceQuery::default(),
        )))
        .await
        .expect("list");
    match listed {
        CommandResult::Collection(collection) => {
            assert_eq!(collection.count, 1);
            assert_eq!(collection.limit, 100);
        }
        other => panic!("unexpected result: {other:?}"),
    }

    let added = service
        .execute(Command::Rdf(RdfCommand::AddStatements {
            id: id.clone(),
            statements: vec![StatementInput {
                predicate: ex("sku"),
                object: Term::string("SVC-1"),
            }],
            graph: None,
            source: Some("service-test".to_owned()),
        }))
        .await
        .expect("add statements");
    match added {
        CommandResult::Collection(collection) => assert_eq!(collection.count, 3),
        other => panic!("unexpected result: {other:?}"),
    }

    let asked = service
        .execute(Command::Rdf(RdfCommand::Query {
            sparql: "ASK { GRAPH <urn:onto:data:asserted> { <https://example.org/service/1> a <https://example.org/Thing> } }"
                .to_owned(),
        }))
        .await
        .expect("query");
    assert_eq!(asked, CommandResult::Query(QueryResult::Boolean(true)));

    service
        .execute(Command::Rdf(RdfCommand::Update {
            sparql: "INSERT DATA { GRAPH <urn:onto:data:inferred> { <https://example.org/service/1> <https://example.org/derivedFrom> \"rule-9\" } }"
                .to_owned(),
        }))
        .await
        .expect("update");

    let graphs = service
        .execute(Command::Rdf(RdfCommand::ListGraphs))
        .await
        .expect("graphs");
    match graphs {
        CommandResult::Collection(collection) => {
            // asserted, provenance, and the inferred graph added above
            assert_eq!(collection.count, 3);
        }
        other => panic!("unexpected result: {other:?}"),
    }

    let relationships = service
        .execute(Command::Rdf(RdfCommand::GetRelationships {
            id: id.clone(),
        }))
        .await
        .expect("relationships");
    match relationships {
        // type, name, sku, and the inferred derivedFrom statement
        CommandResult::Collection(collection) => assert_eq!(collection.count, 4),
        other => panic!("unexpected result: {other:?}"),
    }

    let cleared = service
        .execute(Command::Rdf(RdfCommand::ClearGraph {
            graph: ontology_integration_tests::inferred_graph(),
        }))
        .await
        .expect("clear graph");
    match cleared {
        CommandResult::Report(report) => assert_eq!(report.affected, 1),
        other => panic!("unexpected result: {other:?}"),
    }

    let deleted = service
        .execute(Command::Rdf(RdfCommand::DeleteInstance {
            id: id.clone(),
            graph: None,
        }))
        .await
        .expect("delete");
    match deleted {
        CommandResult::Report(report) => assert!(report.affected >= 1),
        other => panic!("unexpected result: {other:?}"),
    }
}

fn id_to_string(id: &Id) -> String {
    id.as_str().to_owned()
}
