//! Snapshot fixtures shared by the unit tests.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use ontology_core::graph::GraphKind;
use ontology_core::id::Id;
use ontology_rdf::OxigraphStore;

use crate::catalog::{
    CatalogSnapshot, Generation, GraphStoreId, NamespaceBinding, StoreAccess, StoreHandle,
    StoreHealth, StoreLayout, StoreRole,
};

fn handle(
    id: &str,
    role: StoreRole,
    layout: StoreLayout,
    path: &str,
    graphs: &[&str],
    mount_order: i64,
) -> Arc<StoreHandle> {
    Arc::new(StoreHandle::new(
        GraphStoreId::new(id).expect("test store id"),
        PathBuf::from(path),
        PathBuf::from(path),
        role,
        if role == StoreRole::Main {
            StoreAccess::ReadWrite
        } else {
            StoreAccess::ReadOnly
        },
        layout,
        mount_order,
        graphs
            .iter()
            .map(|graph| Id::new(*graph).expect("test graph"))
            .collect(),
        Vec::new(),
        StoreHealth::Ready,
        Some(Arc::new(
            OxigraphStore::in_memory().expect("in-memory store"),
        )),
    ))
}

pub(crate) fn snapshot() -> CatalogSnapshot {
    let asserted = Id::new(GraphKind::Asserted.iri_str()).expect("asserted");
    let main = handle(
        "urn:onto:store:main",
        StoreRole::Main,
        StoreLayout::Named,
        "/srv/onto/data/ontology",
        &[GraphKind::Schema.iri_str(), GraphKind::Asserted.iri_str()],
        0,
    );
    let products = handle(
        "urn:onto:store:products",
        StoreRole::Satellite,
        StoreLayout::Named,
        "/srv/onto/data/graphs/products",
        &["urn:onto:data:products"],
        10,
    );
    let dbpedia = handle(
        "urn:onto:store:dbpedia",
        StoreRole::Satellite,
        StoreLayout::Single,
        "/srv/onto/data/graphs/dbpedia",
        &["urn:onto:external:dbpedia"],
        20,
    );
    let products_graph = Id::new("urn:onto:data:products").expect("products");
    let dbpedia_graph = Id::new("urn:onto:external:dbpedia").expect("dbpedia");

    let mut stores = BTreeMap::new();
    for handle in [&main, &products, &dbpedia] {
        stores.insert(handle.id.clone(), Arc::clone(handle));
    }
    let mut graph_index = BTreeMap::new();
    graph_index.insert(
        Id::new(GraphKind::Schema.iri_str()).expect("schema"),
        Arc::clone(&main),
    );
    graph_index.insert(asserted.clone(), Arc::clone(&main));
    graph_index.insert(products_graph.clone(), Arc::clone(&products));
    graph_index.insert(dbpedia_graph.clone(), Arc::clone(&dbpedia));
    let bindings = vec![
        NamespaceBinding {
            namespace: "https://acme.example/".to_owned(),
            graph: asserted.clone(),
            store: main.id.clone(),
            priority: 0,
        },
        NamespaceBinding {
            namespace: "https://acme.example/team/".to_owned(),
            graph: asserted,
            store: main.id.clone(),
            priority: 0,
        },
        NamespaceBinding {
            namespace: "https://products.example/".to_owned(),
            graph: products_graph,
            store: products.id.clone(),
            priority: 0,
        },
        NamespaceBinding {
            namespace: "http://dbpedia.org/".to_owned(),
            graph: dbpedia_graph,
            store: dbpedia.id.clone(),
            priority: 0,
        },
    ];
    CatalogSnapshot::assemble(
        Generation(1),
        main,
        stores,
        graph_index,
        BTreeMap::new(),
        bindings,
        crate::registry::RegistryModel {
            version: 1,
            main: GraphStoreId::new("urn:onto:store:main").expect("main"),
            stores: Vec::new(),
        },
    )
}
