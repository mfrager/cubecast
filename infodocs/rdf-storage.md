# RDF Storage (`ontology-rdf`)

`ontology-rdf` is the only crate allowed to depend on Oxigraph. It implements
the `RdfStore` and `RdfTransaction` traits from `ontology-core`, adds RDF
parsing/serialization, bootstraps the internal schema, and exposes maintenance
operations such as flush, optimize, validate, and backup.

Source: `crates/ontology-rdf/src/`.

## Modules

| Module | Responsibility |
| --- | --- |
| `store` | `OxigraphStore`, open modes, query/update, load/serialize, maintenance |
| `convert` | Translation between `ontology_core::model` and OxRDF |
| `format` | Mapping `RdfFormat` to Oxigraph parsers/serializers |
| `schema` | Embedded internal schema and idempotent bootstrap |
| `transaction` | `OxigraphTransaction` |
| `lib` | Public exports, including `parse_quads` |

## Engine and dependencies

The workspace builds against the internal Oxigraph fork:

```
https://github.com/mfrager/oxigraph (branch basenote)
```

It uses system RocksDB through `pkg-config`, plus `bindgen`/libclang and a C++20
compiler. Both in-memory and persistent stores are supported.

The engine is synchronous. The async API and MCP layers call it from their
request/job tasks. There is no separate blocking thread pool in the current
implementation.

## Opening a store

`OxigraphStore` constructors:

| Constructor | Behavior |
| --- | --- |
| `in_memory()` | New volatile store |
| `open(path)` | Open or create persistent store; creates parent directories |
| `open_with_max_open_files(path, n)` | Persistent store with RocksDB `max_open_files` |
| `open_read_only(path)` | Open existing store without writes |
| `open_with(&StorageConfig)` | Select memory/persistent, read-only, and file limit |

`open` and `open_with_max_open_files` map engine failures to
`Error::StoreOpen { path, detail }`. A second writer opening the same RocksDB
directory fails at the engine lock.

`OxigraphStore` owns the engine handle and does not implement `Clone`. Services
share it as `Arc<dyn RdfStore>` (alias `DynStore`).

## Composition: `GraphBackend`

Interfaces do not open `OxigraphStore` directly. They call
`ontology_graph::GraphBackend::from_config(&config)`:

- with `[graphs].enabled = false`, it opens one store, bootstraps the schema,
  and `rdf_store()` returns that `OxigraphStore`;
- with `[graphs].enabled = true`, it loads the RDF/XML registry, opens the main
  store plus satellites, persists the registry into `urn:onto:system:graphs`,
  and returns a `MultiStore` that implements `RdfStore` by routing.

The application service, the HTTP server, the CLI `server` subcommand, and
local MCP mode all consume the resulting trait object. Domain managers never
know whether one store or a router is behind it. See
[multi-graph.md](multi-graph.md) for the registry, catalog, routing rules, and
`GraphAdmin` (sync, status, validation, backup).

## RDF conversion

`convert.rs` translates:

- `Id` ↔ `NamedNode`;
- `Term` ↔ `OxTerm`, including blank nodes and literals;
- `GraphName` ↔ `OxGraphName`;
- `Quad` ↔ `OxQuad`;
- `QuadPattern` → optional OxRDF filter components.

Rules:

- Literal subjects are rejected before reaching Oxigraph.
- A literal cannot have both a language tag and a datatype.
- `xsd:string` literals are normalized to simple literals on read, so simple
  strings round-trip without gaining an explicit datatype.
- RDF-star quoted triples are not represented and return `Unsupported`.
- Blank-node identifiers are preserved as strings but are scoped by the engine.

## Parsing and serialization

`RdfFormat` supports:

| Format | Name | Media type | Named graphs |
| --- | --- | --- | --- |
| Turtle | `turtle` | `text/turtle` | no |
| N-Triples | `ntriples` | `application/n-triples` | no |
| N-Quads | `nquads` | `application/n-quads` | yes |
| TriG | `trig` | `application/trig` | yes |
| RDF/XML | `rdfxml` | `application/rdf+xml` | no |
| JSON-LD | `jsonld` | `application/ld+json` | yes |

Format detection is available from extension, canonical name, media type, and
path. `parse_quads(format, data, base)` is used by external import to parse an
entire document before staging registry metadata. It parses into
`Vec<Quad>` without inserting.

`OxigraphStore::load_from_reader/load_from_slice`:

- parses with an optional base IRI;
- optionally forces every parsed quad into a target graph;
- buffers 10,000 quads before each engine `extend`;
- returns the number of parsed quads (not the number newly inserted).

`bulk_load_from_slice` uses the engine bulk loader. It is only valid for an
empty store and returns the resulting total quad count.

`dump_to_writer/dump_to_string` serialize the whole dataset; the graph variants
serialize one `GraphName`. Non-UTF-8 output becomes `Error::Serialization`.

## Query and update

`RdfStore::query` uses `SparqlEvaluator` and converts results:

- `QueryResults::Solutions` → `QueryResult::Solutions { variables, rows }`.
  Each row has one entry per variable, `None` for unbound values.
- `QueryResults::Boolean` → `QueryResult::Boolean`.
- `QueryResults::Graph` → `QueryResult::Graph`, with triples placed in the
  default graph.

The default graph is the unnamed graph. Managed data lives in named graphs, so
queries must use `GRAPH <urn:onto:...> { ... }` unless the data was written to
the default graph.

SPARQL parse and execution failures map to `Error::Query` or `Error::Update`.

In multi-graph mode, a `MultiStore` plans each query once per generation. A
single-layout satellite can be fused into the host query; a named-layout one is
reached through in-process `SERVICE <oxigraph:///absolute/path>` clauses served
by `LocalFederation` in this crate. `GRAPH ?g` expands to a union of registered
graphs, unregistered graphs yield empty branches, and `FROM`/`FROM NAMED` are
rejected.

## Transactions and atomicity

`RdfStore::transaction()` returns `Box<dyn RdfTransaction + '_>`. The Oxigraph
implementation holds an engine transaction:

- changes are invisible until `commit`;
- dropping an uncommitted transaction discards changes;
- `commit(self: Box<Self>)` consumes the transaction.

Transactions are used for multi-quad replacements where partial application
would be observable: instance updates, OWL/SKOS replacement, classification
review, and external import replacement. Single insert/delete batches rely on
the engine's own atomic operations.

## Schema bootstrap

`ontologies/system-owl.ttl` is embedded with `include_str!`.

`bootstrap(store)`:

1. checks whether `system:schema` has any quads;
2. if empty, parses the Turtle document into that graph;
3. returns inserted count (0 on subsequent calls).

`is_initialized(store)` exposes the check. The API calls `bootstrap_schema()` at
startup, so every deployment has the internal schema before serving requests.

## Maintenance

`OxigraphStore` exposes:

- `inner()` — engine handle for maintenance and benchmarks;
- `quad_count()`;
- `flush()` — persist pending writes;
- `optimize()` — engine compaction/optimization;
- `validate()` — index corruption check;
- `backup(target)` — consistent engine backup, creating the parent directory.

`backup` is a library operation, not an HTTP endpoint. Operational procedure is
documented in [../docs/operations.md](../docs/operations.md).

## Concurrency and access

- `RdfStore: Send + Sync`; the engine handle is shared through `Arc`.
- Reads can run concurrently where the engine allows it.
- Exactly one process should own a persistent directory for writes.
- Read-only opens reject writes at the engine level.
- MCP local mode and the API must not open the same directory simultaneously;
  use MCP remote mode when the API is running.

## Tests and benchmarks

Coverage in `tests/tests/phase1_storage.rs` includes:

- memory and persistent stores;
- reopen and data retention;
- read-only behavior;
- quad patterns, graph enumeration, transactions and rollback;
- all supported formats;
- import/export round trips;
- schema bootstrap idempotence;
- backup/open validation.

`tests/tests/phase7_multigraph.rs` covers the composed backend: registry sync,
cross-store queries and joins, fusion, degraded stores, update policies, and
per-store backup. `cargo test -p ontology-graph` covers the graph crate itself.

`crates/ontology-rdf/benches/store.rs` measures insertion and query paths.

```sh
cargo test -p ontology-rdf
cargo bench -p ontology-rdf
```

## Limitations

- No RDF-star support.
- No dataset-level access control; graph isolation is by convention.
- SPARQL queries are evaluated directly against the engine, so expensive
  queries can consume request time.
- Backup/recovery is engine-level; there is no scheduling or retention policy.
- Read-only stores still require the engine's read-only open path.
- Per-document extraction graphs are not registered in multi-graph mode, so the
  document RDF mapping cannot write them without prior registration.
- Multi-store updates have no cross-store transaction; `strict` rejects them
  and `split` can leave earlier groups committed.

## Related

- Domain traits: [core-domain.md](core-domain.md)
- Multi-store composition: [multi-graph.md](multi-graph.md)
- Schema bootstrap content: [owl-schema.md](owl-schema.md)
- Operations: [../docs/operations.md](../docs/operations.md)
