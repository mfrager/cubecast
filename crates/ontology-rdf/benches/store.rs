//! Throughput benchmarks for the Oxigraph adapter.

use std::sync::Arc;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use ontology_core::graph::GraphKind;
use ontology_core::id::Id;
use ontology_core::model::{GraphName, Quad, QuadPattern, Term};
use ontology_core::rdf::RdfStore;
use ontology_rdf::OxigraphStore;

fn ex(local: &str) -> Id {
    Id::new(format!("https://example.org/{local}")).expect("valid test IRI")
}

fn sample_quads(count: usize) -> Vec<Quad> {
    let graph = GraphName::Iri(GraphKind::Asserted.iri());
    (0..count)
        .map(|index| {
            Quad::in_graph(
                Term::Iri(ex(&format!("resource/{index}"))),
                ex("name"),
                Term::string(format!("Resource {index}")),
                graph.clone(),
            )
        })
        .collect()
}

fn bench_insert(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("insert");
    for count in [1_000_usize, 10_000] {
        group.bench_function(format!("{count}_quads"), |bencher| {
            bencher.iter_batched(
                || {
                    let store = OxigraphStore::in_memory().expect("in-memory store");
                    let quads = sample_quads(count);
                    (store, quads)
                },
                |(store, quads)| {
                    store.insert(&quads).expect("insert");
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_pattern_lookup(criterion: &mut Criterion) {
    let store = OxigraphStore::in_memory().expect("in-memory store");
    store.insert(&sample_quads(10_000)).expect("insert");
    let subject = Term::Iri(ex("resource/5000"));

    criterion.bench_function("lookup_by_subject", |bencher| {
        bencher.iter(|| {
            store
                .get(QuadPattern::new().with_subject(subject.clone()))
                .expect("lookup");
        });
    });
}

fn bench_sparql_select(criterion: &mut Criterion) {
    let store = Arc::new(OxigraphStore::in_memory().expect("in-memory store"));
    store.insert(&sample_quads(10_000)).expect("insert");

    criterion.bench_function("select_ordered_limit", |bencher| {
        bencher.iter(|| {
            store
                .query(
                    "SELECT ?s ?name WHERE { ?s <https://example.org/name> ?name } ORDER BY ?s LIMIT 50",
                )
                .expect("query");
        });
    });
}

criterion_group!(
    benches,
    bench_insert,
    bench_pattern_lookup,
    bench_sparql_select
);
criterion_main!(benches);
