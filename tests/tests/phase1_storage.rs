//! Phase 1 storage acceptance tests: persistence, SPARQL, formats,
//! transactions, bulk loading, backup, and the internal schema.

use ontology_core::graph::GraphKind;
use ontology_core::id::Id;
use ontology_core::model::{GraphName, Quad, QuadPattern, QueryResult, Term};
use ontology_core::rdf::RdfStore;
use ontology_integration_tests::{asserted_graph, asserted_quad, ex, memory_store, sample_nquads};
use ontology_rdf::{OxigraphStore, RdfFormat};

#[test]
fn persists_across_restarts() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("store");
    let quad = asserted_quad("resource/1", "name", "Persisted");

    {
        let store = OxigraphStore::open(&path).expect("open store");
        assert!(store.is_empty().expect("empty"));
        store.insert(&[quad.clone()]).expect("insert");
        store.flush().expect("flush");
    }

    let store = OxigraphStore::open(&path).expect("reopen store");
    assert_eq!(store.quad_count().expect("count"), 1);
    assert!(store.contains(&quad).expect("contains"));
    store.validate().expect("valid store");
}

#[test]
fn manages_named_graphs() {
    let store = memory_store();
    store
        .insert(&[
            asserted_quad("a", "name", "A"),
            asserted_quad("b", "name", "B"),
        ])
        .expect("insert asserted");
    store
        .insert(&[Quad::in_graph(
            Term::Iri(ex("a")),
            ex("derivedFrom"),
            Term::string("rule"),
            GraphName::Iri(GraphKind::Inferred.iri()),
        )])
        .expect("insert inferred");

    assert_eq!(store.len().expect("len"), 3);
    let mut graphs = store.graphs().expect("graphs");
    graphs.sort();
    assert_eq!(
        graphs,
        vec![
            GraphName::Iri(GraphKind::Asserted.iri()),
            GraphName::Iri(GraphKind::Inferred.iri())
        ]
    );

    let asserted = store
        .get(QuadPattern::new().with_graph(asserted_graph()))
        .expect("get asserted");
    assert_eq!(asserted.len(), 2);

    store.delete(&asserted).expect("delete asserted contents");
    assert_eq!(store.len().expect("len"), 1);
}

#[test]
fn evaluates_sparql_queries_and_updates() {
    let store = memory_store();
    store
        .insert(&[
            asserted_quad("product/1", "name", "One"),
            asserted_quad("product/2", "name", "Two"),
        ])
        .expect("insert");

    let result = store
        .query(
            "SELECT ?s ?name WHERE { GRAPH <urn:onto:data:asserted> { ?s <https://example.org/name> ?name } } ORDER BY ?s",
        )
        .expect("select");
    match result {
        QueryResult::Solutions { variables, rows } => {
            assert_eq!(variables, vec!["s".to_owned(), "name".to_owned()]);
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0][0], Some(Term::Iri(ex("product/1"))));
            assert_eq!(rows[0][1], Some(Term::string("One")));
        }
        other => panic!("unexpected result: {other:?}"),
    }

    assert_eq!(
        store
            .query(
                "ASK { GRAPH <urn:onto:data:asserted> { <https://example.org/product/1> <https://example.org/name> \"One\" } }",
            )
            .expect("ask"),
        QueryResult::Boolean(true)
    );

    let constructed = store
        .query(
            "CONSTRUCT { ?s <https://example.org/label> ?name } WHERE { GRAPH <urn:onto:data:asserted> { ?s <https://example.org/name> ?name } }",
        )
        .expect("construct");
    assert_eq!(constructed.count(), 2);

    store
        .update(
            "INSERT DATA { GRAPH <urn:onto:data:inferred> { <https://example.org/product/1> <https://example.org/derivedFrom> \"rule-7\" } }",
        )
        .expect("update");
    let inferred = store
        .get(QuadPattern::new().with_graph(GraphName::Iri(GraphKind::Inferred.iri())))
        .expect("get inferred");
    assert_eq!(inferred.len(), 1);
    assert_eq!(inferred[0].object, Term::string("rule-7"));
}

#[test]
fn round_trips_supported_formats() {
    let store = memory_store();
    let count = store
        .load_from_slice(
            RdfFormat::Turtle,
            ontology_integration_tests::sample_turtle().as_bytes(),
            None,
            None,
        )
        .expect("load turtle");
    assert_eq!(count, 5);

    for format in RdfFormat::ALL {
        let serialized = store
            .dump_to_string(format)
            .unwrap_or_else(|error| panic!("serialize {format}: {error}"));
        assert!(!serialized.trim().is_empty(), "{format} produced no output");

        let target = memory_store();
        let reparsed = target
            .load_from_slice(format, serialized.as_bytes(), None, None)
            .unwrap_or_else(|error| panic!("reparse {format}: {error}"));
        assert_eq!(reparsed, count, "{format} lost statements");
    }
}

#[test]
fn loads_datasets_into_an_explicit_graph() {
    let store = memory_store();
    let graph = GraphName::Iri(GraphKind::Classification.iri());
    let count = store
        .load_from_slice(
            RdfFormat::Turtle,
            b"<https://example.org/x> <https://example.org/classifiedAs> <https://example.org/Thing> .",
            None,
            Some(&graph),
        )
        .expect("load into graph");
    assert_eq!(count, 1);
    assert_eq!(
        store
            .get(QuadPattern::new().with_graph(graph))
            .expect("get")
            .len(),
        1
    );
}

#[test]
fn bulk_loads_and_restores_a_backup() {
    let directory = tempfile::tempdir().expect("temp dir");
    let store = OxigraphStore::open(directory.path().join("live")).expect("open store");
    let loaded = store
        .bulk_load_from_slice(RdfFormat::NQuads, sample_nquads().as_bytes())
        .expect("bulk load");
    assert_eq!(loaded, 3);
    assert_eq!(store.quad_count().expect("count"), 3);
    store.optimize().expect("optimize");

    let backup = directory.path().join("backup");
    store.backup(&backup).expect("backup");

    let restored = OxigraphStore::open_read_only(&backup).expect("open backup");
    assert_eq!(restored.quad_count().expect("count"), 3);
    assert_eq!(
        restored
            .get(QuadPattern::new().with_graph(asserted_graph()))
            .expect("get")
            .len(),
        2
    );
}

#[test]
fn rolls_back_uncommitted_transactions() {
    let store = memory_store();
    let quad = asserted_quad("resource/1", "name", "Transactional");

    {
        let mut transaction = store.transaction().expect("begin");
        transaction.insert(&[quad.clone()]).expect("insert");
        // Dropped without commit.
    }
    assert_eq!(store.quad_count().expect("count"), 0);

    {
        let mut transaction = store.transaction().expect("begin");
        transaction.insert(&[quad.clone()]).expect("insert");
        transaction.commit().expect("commit");
    }
    assert!(store.contains(&quad).expect("contains"));
}

#[test]
fn bootstraps_the_internal_schema_once() {
    let store = memory_store();
    let inserted = store.bootstrap_schema().expect("bootstrap");
    assert!(inserted > 20, "schema should define the core vocabulary");
    assert_eq!(store.bootstrap_schema().expect("bootstrap again"), 0);

    let schema = store
        .get(QuadPattern::new().with_graph(GraphName::Iri(GraphKind::Schema.iri())))
        .expect("read schema");
    assert!(!schema.is_empty());
    assert!(ontology_rdf::schema::is_initialized(&store).expect("initialized"));

    let ontology = Id::new("urn:onto:schema:Vocabulary").expect("valid IRI");
    let header = store
        .get(QuadPattern::new().with_subject(Term::Iri(ontology)))
        .expect("read ontology header");
    assert!(!header.is_empty(), "ontology header should be stored");
}
