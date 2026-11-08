# Core Domain (`ontology-core`)

`ontology-core` is the dependency root. It contains no storage engine, network,
or interface code. Everything here is shared by the domain crates,
`ontology-application`, and the three interfaces.

Source: `crates/ontology-core/src/`.

## Modules

| Module | Contents |
| --- | --- |
| `id` | `Id`, `OperationId` |
| `namespace` | Built-in prefixes, `NamespaceRegistry` |
| `graph` | `GraphKind`, reserved graph IRIs, tenant graphs |
| `model` | Terms, literals, quads, instances, statements, queries, results |
| `rdf` | `RdfStore`, `RdfTransaction`, `OntologyService`, `load_instance` |
| `format` | `RdfFormat` and syntax detection |
| `command` | `Command`, command families, `CommandResult` and payloads |
| `owl` | OWL schema types and `OntologyManager` trait |
| `skos` | SKOS types and `TaxonomyManager` trait |
| `external` | External registry types and `ExternalOntologyRegistry` trait |
| `semantic` | Classification, embedding, inference types and traits |
| `job` | `JobRequest`, `JobState`, `JobStatus` |
| `config` | TOML configuration and environment overrides |
| `error` | `Error`, `Result` |
| `tracing` | Tracing initialisation |

## Identifiers

### `Id`

`Id` wraps a non-empty absolute IRI validated by `oxiri::Iri::parse`. Empty or
relative values fail with `Error::InvalidIri`. It implements ordering, hashing,
display, and serde as a string.

```rust
let id = Id::new("https://example.org/Product")?;
```

No implicit prefix expansion happens in `Id`. Prefix expansion is a namespace
registry concern.

### `OperationId`

`OperationId` is a generated ULID. `OperationId::generate()` is used for
records that have no natural IRI:

- jobs: `urn:onto:job:<ULID>`
- classifications: `urn:onto:classification:<ULID>`
- embeddings: `urn:onto:embedding:<ULID>`
- provenance: `urn:onto:provenance:<ULID>`
- inference records: `urn:onto:inference:<ULID>`

ULIDs sort by creation time, which gives record identifiers a useful stable
order.

## Namespaces

`NamespaceRegistry` holds prefix-to-namespace mappings. Built-ins include the
usual vocabularies (`rdf`, `rdfs`, `owl`, `xsd`, `skos`, `dcterms`, `prov`) and
the internal `onto`/graph namespaces.

Methods:

- `resolve("ex:Product")` expands a prefixed name;
- `insert(prefix, namespace)` validates and adds an entry;
- absolute IRIs pass through unchanged.

The HTTP/CLI/MCP layers use this when a user supplies a prefixed name. The
registry is constructed from `Config::namespace_registry()`, which starts with
built-ins and applies configured overrides.

## Graphs

`GraphKind` defines seven reserved graph families:

| Variant | Local name | IRI | Role |
| --- | --- | --- | --- |
| `Schema` | `schema` | `urn:onto:system:schema` | Internal and managed OWL schema |
| `Taxonomy` | `taxonomy` | `urn:onto:system:taxonomy` | SKOS data |
| `External` | `external` | `urn:onto:system:external` | External registry metadata |
| `Asserted` | `asserted` | `urn:onto:data:asserted` | Explicit data |
| `Inferred` | `inferred` | `urn:onto:data:inferred` | Derived statements |
| `Classification` | `classification` | `urn:onto:data:classification` | Classification records |
| `Provenance` | `provenance` | `urn:onto:data:provenance` | Provenance and embeddings |

Helpers: `iri_str()`, `iri()`, `from_iri_str()`, `from_iri()`, `is_system()`,
`is_data()`, `local_name()`.

`tenant_graph(tenant, kind)` validates the tenant name (alphanumeric, `-`, `_`)
and returns `urn:onto:tenant:<tenant>:<local-name>`.

## RDF data model

Defined in `model.rs`:

- `Term` — `Iri(Id)`, `BlankNode(String)`, or `Literal(Literal)`.
- `Literal` — value plus optional language tag or datatype. Constructors include
  `simple`, `typed`, `language_tagged`, `integer`, `boolean`, and `datetime`.
  A literal cannot carry both a language and a datatype.
- `GraphName` — `Default`, `Iri(Id)`, or `BlankNode(String)`.
- `Quad` — subject, predicate, object, graph. `Quad::new` uses the default graph;
  `Quad::in_graph` uses a named graph. `validate()` rejects a literal subject.
- `QuadPattern` — optional filters for each component; `with_*` builders and
  `with_quad`.
- `Instance` — `id`, `graph`, `types`, `statements`.
- `Statement` — a quad plus a stable statement identifier.
- `StatementInput` — predicate and object for adding a statement.
- `InstanceQuery` — graph, type filter, limit, offset.
- `NewInstance` — IRI, optional graph, types, properties, source.
- `InstancePatch` — optional type replacement and per-predicate property
  replacement (`Some(None)` clears a predicate, `None` leaves it unchanged).
- `Relationship` / `RelationshipDirection` — incoming and outgoing edges.
- `QueryResult` — `Solutions { variables, rows }`, `Boolean(bool)`, or
  `Graph(Vec<Quad>)`.

Serialization uses camelCase tags. The important external tags are:

```json
{ "resultType": "solutions", "variables": ["s"], "rows": [[...]] }
{ "resultType": "boolean", "value": true }
{ "resultType": "graph", "quads": [...] }
```

## Storage capability traits

`rdf.rs` defines the only storage abstraction:

```rust
pub trait RdfStore: Send + Sync {
    fn get(&self, pattern: QuadPattern) -> Result<Vec<Quad>>;
    fn insert(&self, quads: &[Quad]) -> Result<()>;
    fn delete(&self, quads: &[Quad]) -> Result<()>;
    fn query(&self, sparql: &str) -> Result<QueryResult>;
    fn update(&self, sparql: &str) -> Result<()>;
    fn graphs(&self) -> Result<Vec<GraphName>>;
    // Optional capabilities with Unsupported defaults:
    fn load_from_slice(...) -> Result<u64>;
    fn serialize(...) -> Result<String>;
    fn transaction(...) -> Result<Box<dyn RdfTransaction + '_>>;
    // Provided helpers:
    fn contains(&self, quad: &Quad) -> Result<bool>;
    fn len(&self) -> Result<usize>;
    fn is_empty(&self) -> Result<bool>;
}
```

`RdfTransaction` supports `insert`, `delete`, and consuming `commit`. Dropping an
uncommitted transaction rolls it back.

`load_instance(store, id, graph)` is the shared loader used by classification,
embeddings, and inference. It defaults to `data:asserted`, collects `rdf:type`
objects, and returns an `Instance`.

`OntologyService` is the async boundary:

```rust
async fn execute(&self, command: Command) -> Result<CommandResult>;
```

## Commands and results

`Command` is tagged by `family` and `command`:

```json
{ "family": "rdf", "command": { "action": "getInstance", "id": "..." } }
```

Families:

- `RdfCommand` — create/get/list/update/delete instances, statements,
  relationships, SPARQL query/update, graphs, import/export, clear graph.
- `OntologyCommand` — class/property CRUD, hierarchy, validation, ontology CRUD,
  versions, applicable properties/classes.
- `TaxonomyCommand` — scheme/concept CRUD, search, relations, ancestors,
  descendants, tree, move, merge, validation, collections.
- `ExternalCommand` — import, registry get/list/sync/delete, class/property
  mappings, mapping validation.
- `SemanticCommand` — classify, batch classify, list/override/review/reclassify,
  embeddings, similarity, inference preview/apply/explain.
- `JobCommand` — submit, get, list, results, cancel, retry.

`CommandResult` is tagged by `type`:

| Type | Payload |
| --- | --- |
| `resource` | `id`, `kind`, `resource` |
| `collection` | `items`, `count`, `limit`, `offset` |
| `query` | `QueryResult` |
| `report` | `operation`, `affected`, optional `message` |
| `validation` | OWL `ValidationReport` |
| `taxonomy` | `TaxonomyReport` |
| `mapping` | `MappingReport` |
| `classifications` | `Vec<Classification>` |
| `embeddings` | `Vec<EmbeddingRecord>` |
| `similarity` | `Vec<VectorMatch>` |
| `preview` | `InferencePreview` |
| `inference` | `InferenceReport` |
| `export` | `format`, `data` |
| `job` | `JobStatus` |

Collections always include the requested `limit` and `offset`, even when the
page is empty. `count` is the size of the returned page, not the total dataset.

## Configuration

`Config` is a serde TOML document with four sections:

```toml
[storage]
backend = "memory"          # "memory" | "persistent"
path = "./data"             # required for persistent
read_only = false
max_open_files = 512

[logging]
filter = "info"
json = false

[namespaces]
ex = "https://example.org/"

[limits]
max_page_size = 1000
max_blocking_tasks = 8
```

`Config::from_toml_str`, `Config::from_file`, and `Config::load` build it.
`load` calls `apply_env()`, which currently supports:

- `ONTO_STORAGE_PATH`: use a persistent store at this directory;
- `ONTO_LOG`: tracing filter.

The binaries additionally read `ONTO_CONFIG` to select the TOML file, and
`ServeOptions::from_env` reads `ONTO_BIND`, `ONTO_API_KEY`, and
`ONTO_RATE_LIMIT`.

`Config::validate()` rejects:

- persistent storage without `storage.path`;
- zero `limits.max_blocking_tasks`;
- zero `limits.max_page_size`;
- invalid namespace prefixes/namespaces.

`StorageConfig::database_path()` returns `None` for memory and errors when a
persistent store has no path.

## Errors

`Error` is a `thiserror` enum:

| Variant | Meaning | HTTP status |
| --- | --- | --- |
| `InvalidIri` | Malformed absolute IRI | 400 |
| `UnknownPrefix` | Unresolvable prefix | 400 |
| `StoreOpen` | Database open/lock failure | 500 |
| `Storage` | Engine operation failure | 500 |
| `Parse` | RDF/SPARQL parse failure | 400 |
| `Serialization` | Serialization or non-UTF-8 output | 500 |
| `Query` | SPARQL query failure | 400 |
| `Update` | SPARQL update failure | 400 |
| `NotFound` | Missing resource | 404 |
| `Conflict` | Duplicate or conflicting state | 409 |
| `Validation` | Domain rule violation | 400 |
| `Invalid` | Invalid request shape | 400 |
| `Config` | Invalid configuration | 400 |
| `Io` | Filesystem/network/server I/O | 500 |
| `Unsupported` | Capability not available | 501 |

`Result<T>` is `std::result::Result<T, Error>`.

## Jobs model

`job.rs` defines `JobState` (`queued`, `running`, `completed`, `failed`,
`cancelled`), `JobRequest`, and `JobStatus`.

`JobRequest` variants and stable kind names:

- `classifyBatch` → `classify_batch`
- `embedBatch` → `embed_batch`
- `inferBatch` → `infer_batch`
- `reclassifyBatch` → `reclassify_batch`
- `rdfImport` → `rdf_import`
- `rdfExport` → `rdf_export`
- `externalImport` → `external_import`
- `externalSync` → `external_sync`

`JobStatus` carries `id`, `kind`, `state`, `progress`, `processed`, `total`,
`attempts`, optional `message`, `error`, `result`, and timestamps.

## Domain contracts in core

The traits that domain crates implement live here:

- `OntologyManager` — OWL class/property/ontology operations.
- `TaxonomyManager` — SKOS operations.
- `ExternalOntologyRegistry` — import/sync/registry/mapping operations.
- `Classifier`, `EmbeddingProvider`, `VectorIndex`, `InferenceEngine` — semantic
  capabilities.

The corresponding value objects (`OwlClass`, `OwlProperty`, `Concept`,
`ConceptScheme`, `Classification`, `InferencePreview`, etc.) are also in core,
so every interface can serialize the same contract without depending on an
implementation crate.

## Design invariants

- No core module imports Oxigraph or Axum.
- `Id` is always absolute; there is no relative IRI state.
- Prefix expansion is explicit and registry-driven.
- Mutations default to `data:asserted`; reads may span graphs.
- Store writes are idempotent for quads (inserting an existing quad is a no-op).
- No command silently returns a partial success: an error aborts the command.

## Tests

`cargo test -p ontology-core` covers identifiers, namespaces, graphs, literals,
quads, formats, configuration validation, and command serialization. The final
workspace run had 24 core tests passing.

## Related

- Storage implementation: [rdf-storage.md](rdf-storage.md)
- Command orchestration: [application-service.md](application-service.md)
- Interface translation: [http-api.md](http-api.md), [cli.md](cli.md),
  [mcp-server.md](mcp-server.md)
