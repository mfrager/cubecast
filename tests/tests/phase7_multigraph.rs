//! Phase 7 acceptance tests: registry sync, cross-store routing, degraded
//! stores, writable satellites, and backups.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ontology_core::config::Config;
use ontology_core::error::Error;
use ontology_core::graph::GraphKind;
use ontology_core::id::Id;
use ontology_core::model::{GraphName, Quad, QueryResult, Term};
use ontology_core::rdf::RdfStore;
use ontology_graph::{pool, GraphAdmin, GraphBackend, GraphStoreId, StoreLayout};
use ontology_rdf::OxigraphStore;

struct SatSpec {
    name: String,
    graph: String,
    namespace: String,
    layout: StoreLayout,
    read_write: bool,
    required: bool,
    create: bool,
    data: Vec<Quad>,
}

impl SatSpec {
    fn new(name: &str, graph: &str, namespace: &str, layout: StoreLayout) -> Self {
        Self {
            name: name.to_owned(),
            graph: graph.to_owned(),
            namespace: namespace.to_owned(),
            layout,
            read_write: false,
            required: true,
            create: true,
            data: Vec::new(),
        }
    }

    fn with_data(mut self, data: Vec<Quad>) -> Self {
        self.data = data;
        self
    }

    fn read_write(mut self) -> Self {
        self.read_write = true;
        self
    }

    fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    fn missing(mut self) -> Self {
        self.create = false;
        self
    }
}

struct Deployment {
    _directory: tempfile::TempDir,
    backend: GraphBackend,
    registry: PathBuf,
    main: PathBuf,
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn write_registry(path: &Path, main: &Path, satellites: &[SatSpec]) {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<rdf:RDF
    xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:onto="urn:onto:schema:">
"#,
    );
    xml.push_str(
        r#"
  <onto:GraphRegistry rdf:about="urn:onto:system:graphs">
    <onto:registryVersion>1</onto:registryVersion>
    <onto:mainStore rdf:resource="urn:onto:store:main"/>
    <onto:entry rdf:resource="urn:onto:store:main"/>
"#,
    );
    for satellite in satellites {
        xml.push_str(&format!(
            "    <onto:entry rdf:resource=\"urn:onto:store:{}\"/>\n",
            satellite.name
        ));
    }
    xml.push_str("  </onto:GraphRegistry>\n");
    xml.push_str(&format!(
        r#"
  <onto:Store rdf:about="urn:onto:store:main">
    <onto:role>main</onto:role>
    <onto:storePath>{}</onto:storePath>
    <onto:access>rw</onto:access>
    <onto:mountOrder>0</onto:mountOrder>
    <onto:registration rdf:parseType="Resource">
      <onto:graphIri rdf:resource="urn:onto:system:schema"/>
      <onto:namespace>urn:onto:</onto:namespace>
    </onto:registration>
    <onto:registration rdf:parseType="Resource">
      <onto:graphIri rdf:resource="urn:onto:data:asserted"/>
      <onto:namespace>https://acme.example/</onto:namespace>
    </onto:registration>
    <onto:registration rdf:parseType="Resource">
      <onto:graphIri rdf:resource="urn:onto:data:inferred"/>
    </onto:registration>
  </onto:Store>
"#,
        xml_escape(&main.display().to_string())
    ));
    for (index, satellite) in satellites.iter().enumerate() {
        xml.push_str(&format!(
            r#"
  <onto:Store rdf:about="urn:onto:store:{name}">
    <onto:role>satellite</onto:role>
    <onto:storePath>{name}</onto:storePath>
    <onto:access>{access}</onto:access>
    <onto:layout>{layout}</onto:layout>
    <onto:mountOrder>{mount}</onto:mountOrder>
    <onto:registration rdf:parseType="Resource">
      <onto:graphIri rdf:resource="{graph}"/>
      <onto:namespace>{namespace}</onto:namespace>
      <onto:required>{required}</onto:required>
    </onto:registration>
  </onto:Store>
"#,
            name = satellite.name,
            access = if satellite.read_write { "rw" } else { "ro" },
            layout = satellite.layout.as_str(),
            mount = 10 + index,
            graph = satellite.graph,
            namespace = xml_escape(&satellite.namespace),
            required = satellite.required,
        ));
    }
    xml.push_str("</rdf:RDF>\n");
    fs::write(path, xml).expect("write registry");
}

fn try_setup(satellites: &[SatSpec], policy: &str) -> Result<Deployment, Error> {
    let directory = tempfile::tempdir().expect("temp dir");
    let main = directory.path().join("ontology");
    let root = directory.path().join("graphs");
    fs::create_dir_all(&main).expect("main dir");
    fs::create_dir_all(&root).expect("graph root");

    for satellite in satellites {
        if !satellite.create {
            continue;
        }
        let path = root.join(&satellite.name);
        let id = GraphStoreId::new(format!("urn:onto:store:{}", satellite.name)).expect("store id");
        let graph = Id::new(satellite.graph.clone()).expect("graph iri");
        pool::create_satellite(&path, &id, satellite.layout, std::slice::from_ref(&graph))
            .expect("create satellite");
        if !satellite.data.is_empty() {
            let store = OxigraphStore::open(&path).expect("open satellite");
            store.insert(&satellite.data).expect("seed satellite");
            store.flush().expect("flush satellite");
        }
    }

    let registry = directory.path().join("graphs.rdf");
    write_registry(&registry, &main, satellites);

    let config = Config::from_toml_str(&format!(
        r#"
[storage]
backend = "persistent"
path = "{}"

[graphs]
enabled = true
registry = "{}"
root = "{}"
sync = "on_startup"
update_policy = "{}"
"#,
        main.display(),
        registry.display(),
        root.display(),
        policy
    ))
    .expect("config");

    let backend = GraphBackend::from_config(&config)?;
    Ok(Deployment {
        _directory: directory,
        backend,
        registry,
        main,
    })
}

fn setup(satellites: &[SatSpec], policy: &str) -> Deployment {
    try_setup(satellites, policy).expect("multi-graph backend")
}

fn ex(local: &str) -> Id {
    Id::new(format!("https://example.org/{local}")).expect("example IRI")
}

fn asserted_quad(local: &str, predicate: &str, value: &str) -> Quad {
    Quad::in_graph(
        Term::Iri(ex(local)),
        ex(predicate),
        Term::string(value),
        GraphName::Iri(GraphKind::Asserted.iri()),
    )
}

fn named_quad(graph: &str, subject: &str, predicate: &str, value: &str) -> Quad {
    Quad::in_graph(
        Term::Iri(ex(subject)),
        Id::new(format!("https://products.example/{predicate}")).expect("predicate"),
        Term::string(value),
        GraphName::Iri(Id::new(graph).expect("graph")),
    )
}

const PRODUCTS: &str = "urn:onto:data:products";
const DBPEDIA: &str = "urn:onto:external:dbpedia";

fn products_satellite(data: Vec<Quad>) -> SatSpec {
    SatSpec::new(
        "products",
        PRODUCTS,
        "https://products.example/",
        StoreLayout::Named,
    )
    .with_data(data)
}

fn dbpedia_satellite(data: Vec<Quad>) -> SatSpec {
    SatSpec::new(
        "dbpedia",
        DBPEDIA,
        "http://dbpedia.org/",
        StoreLayout::Single,
    )
    .with_data(data)
}

fn rows(result: QueryResult) -> Vec<Vec<Option<Term>>> {
    match result {
        QueryResult::Solutions { rows, .. } => rows,
        other => panic!("expected solutions, got {other:?}"),
    }
}

#[test]
fn round_trips_across_two_stores() {
    let deployment = setup(&[products_satellite(Vec::new()).read_write()], "strict");
    let store = deployment.backend.rdf_store();

    store
        .insert(&[
            asserted_quad("product-1", "name", "One"),
            Quad::in_graph(
                Term::Iri(ex("product-1")),
                Id::new("https://products.example/price").expect("predicate"),
                Term::string("9.99"),
                GraphName::Iri(Id::new(PRODUCTS).expect("graph")),
            ),
        ])
        .expect("insert");

    let graphs = store.graphs().expect("graphs");
    assert!(graphs.contains(&GraphName::Iri(GraphKind::Asserted.iri())));
    assert!(graphs.contains(&GraphName::Iri(Id::new(PRODUCTS).expect("graph"))));

    let satellite = store
        .get(
            ontology_core::model::QuadPattern::new()
                .with_graph(GraphName::Iri(Id::new(PRODUCTS).expect("graph"))),
        )
        .expect("get products");
    assert_eq!(satellite.len(), 1);
    assert_eq!(
        satellite[0].graph,
        GraphName::Iri(Id::new(PRODUCTS).expect("graph"))
    );

    let all = store
        .get(ontology_core::model::QuadPattern::new())
        .expect("all");
    assert!(all
        .iter()
        .any(|quad| quad.subject == Term::Iri(ex("product-1"))));
}

#[test]
fn joins_graphs_across_stores() {
    let deployment = setup(
        &[products_satellite(vec![named_quad(
            PRODUCTS,
            "product-1",
            "price",
            "9.99",
        )])],
        "strict",
    );
    let store = deployment.backend.rdf_store();
    store
        .insert(&[asserted_quad("product-1", "name", "One")])
        .expect("insert main");

    let query = format!(
        "SELECT ?name ?price WHERE {{ GRAPH <{}> {{ ?p <https://example.org/name> ?name }} GRAPH <{}> {{ ?p <https://products.example/price> ?price }} }}",
        GraphKind::Asserted.iri_str(),
        PRODUCTS
    );
    let solutions = rows(store.query(&query).expect("join"));
    assert_eq!(solutions.len(), 1);
    assert_eq!(solutions[0][0], Some(Term::string("One")));
    assert_eq!(solutions[0][1], Some(Term::string("9.99")));

    let plan = deployment.backend.store().plan_text(&query).expect("plan");
    assert!(plan.fallbacks.iter().any(|fallback| matches!(
        fallback,
        ontology_graph::FallbackReason::NamedLayoutService { .. }
    )));
    let rewritten = plan.query.to_string();
    assert!(rewritten.contains("SERVICE <oxigraph://"));
    assert!(rewritten.contains("GRAPH <urn:onto:data:products>"));
}

#[test]
fn fuses_single_layout_satellites() {
    let deployment = setup(
        &[dbpedia_satellite(vec![Quad::new(
            Term::Iri(Id::new("http://dbpedia.org/resource/X").expect("iri")),
            Id::new("http://dbpedia.org/ontology/name").expect("iri"),
            Term::string("X"),
        )])],
        "strict",
    );
    let store = deployment.backend.rdf_store();

    let query = format!(
        "SELECT ?s ?o WHERE {{ GRAPH <{}> {{ ?s ?p ?o }} }}",
        DBPEDIA
    );
    let plan = deployment.backend.store().plan_text(&query).expect("plan");
    assert_eq!(
        plan.host,
        GraphStoreId::new("urn:onto:store:dbpedia").expect("id")
    );
    assert!(plan.fallbacks.is_empty(), "{:?}", plan.fallbacks);

    let solutions = rows(store.query(&query).expect("query"));
    assert_eq!(solutions.len(), 1);

    let cross = format!(
        "SELECT ?s WHERE {{ GRAPH <{}> {{ ?s <https://example.org/name> ?n }} GRAPH <{}> {{ ?s ?p ?o }} }}",
        GraphKind::Asserted.iri_str(),
        DBPEDIA
    );
    let plan = deployment.backend.store().plan_text(&cross).expect("plan");
    assert!(plan.fallbacks.is_empty(), "{:?}", plan.fallbacks);
    let rewritten = plan.query.to_string();
    assert!(rewritten.contains("SERVICE <oxigraph://"));
    assert!(!rewritten.contains("GRAPH <urn:onto:external:dbpedia>"));
}

#[test]
fn expands_variable_graphs_and_rejects_dataset_clauses() {
    let deployment = setup(&[products_satellite(Vec::new())], "strict");
    let store = deployment.backend.rdf_store();

    let query = "SELECT ?g ?s WHERE { GRAPH ?g { ?s ?p ?o } }";
    let plan = deployment.backend.store().plan_text(query).expect("plan");
    assert!(plan
        .fallbacks
        .iter()
        .any(|fallback| matches!(fallback, ontology_graph::FallbackReason::VariableGraphUnion)));
    assert!(plan.query.to_string().contains("BIND("));

    let error = store
        .query("SELECT * FROM <urn:onto:data:asserted> WHERE { ?s ?p ?o }")
        .expect_err("dataset clause is rejected");
    assert!(matches!(error, Error::Unsupported { .. }));
}

#[test]
fn resynchronises_and_retires_stores() {
    let deployment = setup(&[products_satellite(Vec::new())], "strict");
    let store = deployment.backend.rdf_store();
    let products = GraphName::Iri(Id::new(PRODUCTS).expect("graph"));
    assert!(store.graphs().expect("graphs").contains(&products));

    write_registry(&deployment.registry, &deployment.main, &[]);
    let report = deployment
        .backend
        .administrator()
        .sync_registry()
        .expect("sync");
    assert_eq!(report.generation, 2);
    assert_eq!(report.removed, vec!["urn:onto:store:products".to_owned()]);
    assert!(!store.graphs().expect("graphs").contains(&products));

    let error = store
        .query(&format!(
            "SELECT * WHERE {{ GRAPH <{PRODUCTS}> {{ ?s ?p ?o }} }}"
        ))
        .expect_err("retired graph is unknown");
    assert!(matches!(error, Error::GraphRegistry { .. }));
}

#[test]
fn degrades_optional_stores_and_fails_missing_required_ones() {
    let deployment = setup(
        &[dbpedia_satellite(Vec::new()).optional().missing()],
        "strict",
    );
    let store = deployment.backend.rdf_store();
    let query = format!("SELECT ?s WHERE {{ GRAPH <{DBPEDIA}> {{ ?s ?p ?o }} }}");
    assert!(rows(store.query(&query).expect("query")).is_empty());

    let error = match try_setup(&[dbpedia_satellite(Vec::new()).missing()], "strict") {
        Ok(_) => panic!("required store should be missing"),
        Err(error) => error,
    };
    assert!(matches!(error, Error::GraphOpen { .. }));
}

#[test]
fn writes_satellites_through_admin_only() {
    let read_only = setup(&[products_satellite(Vec::new())], "strict");
    let quad = named_quad(PRODUCTS, "product-9", "price", "1.00");
    let direct = read_only
        .backend
        .rdf_store()
        .insert(std::slice::from_ref(&quad))
        .expect_err("read-only");
    assert!(matches!(direct, Error::GraphReadOnly { .. }));

    let deployment = setup(&[products_satellite(Vec::new()).read_write()], "strict");
    let id = GraphStoreId::new("urn:onto:store:products").expect("id");
    deployment
        .backend
        .administrator()
        .write(&id, |satellite| satellite.insert(&[quad]))
        .expect("admin write");

    let solutions = rows(
        deployment
            .backend
            .rdf_store()
            .query(&format!(
                "SELECT ?price WHERE {{ GRAPH <{PRODUCTS}> {{ ?p <https://products.example/price> ?price }} }}"
            ))
            .expect("query"),
    );
    assert_eq!(solutions.len(), 1);
}

#[test]
fn rejects_cross_store_updates_under_strict_policy() {
    let deployment = setup(&[products_satellite(Vec::new()).read_write()], "strict");
    let store = deployment.backend.rdf_store();
    let error = store
        .update(&format!(
            "INSERT DATA {{ GRAPH <{}> {{ <https://example.org/a> <https://example.org/p> \"1\" }} GRAPH <{}> {{ <https://example.org/b> <https://example.org/p> \"2\" }} }}",
            GraphKind::Asserted.iri_str(),
            PRODUCTS
        ))
        .expect_err("cross-store update rejected");
    assert!(matches!(error, Error::Validation { .. }));
}

#[test]
fn splits_data_updates_and_backs_up_every_store() {
    let deployment = setup(&[products_satellite(Vec::new()).read_write()], "split");
    let store = deployment.backend.rdf_store();
    store
        .update(&format!(
            "INSERT DATA {{ GRAPH <{}> {{ <https://example.org/a> <https://example.org/p> \"1\" }} GRAPH <{}> {{ <https://example.org/b> <https://products.example/p> \"2\" }} }}",
            GraphKind::Asserted.iri_str(),
            PRODUCTS
        ))
        .expect("split update");

    let all = store
        .get(ontology_core::model::QuadPattern::new())
        .expect("all");
    assert!(all.iter().any(|quad| quad.subject == Term::Iri(ex("a"))));
    assert!(all.iter().any(|quad| quad.subject == Term::Iri(ex("b"))));

    let target = deployment._directory.path().join("backup");
    let report = deployment.backend.admin().backup(&target).expect("backup");
    assert_eq!(report.generation, 1);
    assert!(target.join("manifest.json").exists());
    assert!(report.stores.len() >= 2);
    let manifest = fs::read_to_string(target.join("manifest.json")).expect("manifest");
    assert!(manifest.contains("urn:onto:store:products"));
}

#[test]
fn exposes_namespace_and_status_resolution() {
    let deployment = setup(&[products_satellite(Vec::new())], "strict");
    let catalog = deployment.backend.catalog();
    let widget = Id::new("https://products.example/Widget").expect("iri");
    let handle = catalog.resolve_namespace(&widget).expect("namespace");
    assert_eq!(handle.id.as_str(), "urn:onto:store:products");
    assert_eq!(
        catalog
            .graph_for_namespace("https://products.example/")
            .expect("graph")
            .as_str(),
        PRODUCTS
    );

    let status = catalog.status();
    assert_eq!(status.len(), 2);
    assert!(status.iter().all(|store| store.health.is_ready()));
    assert!(status
        .iter()
        .find(|store| store.id == "urn:onto:store:products")
        .is_some_and(|store| store.graphs.contains(&PRODUCTS.to_owned())));
}

#[test]
fn single_mode_matches_a_plain_store() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("store");
    let backend = GraphBackend::single(&ontology_core::config::StorageConfig {
        backend: ontology_core::config::StorageBackend::Persistent,
        path: Some(path),
        read_only: false,
        max_open_files: None,
    })
    .expect("single backend");
    let store = backend.rdf_store();
    store
        .insert(&[asserted_quad("product-1", "name", "One")])
        .expect("insert");
    let result = store
        .query("SELECT ?name WHERE { GRAPH <urn:onto:data:asserted> { ?s <https://example.org/name> ?name } }")
        .expect("query");
    assert_eq!(rows(result).len(), 1);
}

#[test]
fn shares_the_store_between_handles() {
    let deployment = setup(&[products_satellite(Vec::new())], "strict");
    let service = Arc::new(ontology_application::ApplicationService::new(
        deployment.backend.rdf_store(),
    ));
    service
        .execute_rdf(ontology_core::command::RdfCommand::Query {
            sparql: "SELECT * WHERE { GRAPH <urn:onto:data:asserted> { ?s ?p ?o } }".to_owned(),
        })
        .expect("query through the application service");
}
