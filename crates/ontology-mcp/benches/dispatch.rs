//! Microbenchmark for MCP action translation.

use criterion::{criterion_group, criterion_main, Criterion};
use ontology_core::namespace::NamespaceRegistry;
use ontology_mcp::dispatch::{rdf_command, DispatchContext, Resolver};
use ontology_mcp::params::{CommonArgs, RdfAction};

fn dispatch_benchmark(criterion: &mut Criterion) {
    let resolver = Resolver::new(NamespaceRegistry::with_builtins());
    let context = DispatchContext {
        resolver: &resolver,
        max_results: 100,
        allow_write: true,
    };
    let args = CommonArgs {
        target: Some("ex:Product".to_owned()),
        graph: Some("data:asserted".to_owned()),
        ..CommonArgs::default()
    };

    criterion.bench_function("mcp_rdf_get_translation", |bencher| {
        bencher.iter(|| rdf_command(RdfAction::Get, &args, &context).expect("translate"));
    });
}

criterion_group!(benches, dispatch_benchmark);
criterion_main!(benches);
