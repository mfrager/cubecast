# Application Orchestration (`ontology-application`)

`ontology-application` is the single execution boundary. It owns the concrete
managers, exposes them as shared `Arc` handles, and dispatches every `Command`
to the right subsystem. HTTP, CLI, and MCP do not call domain managers directly.

Source: `crates/ontology-application/src/`.

## Components

| Component | Responsibility |
| --- | --- |
| `ApplicationService` | Wires managers, namespaces, jobs, and command dispatch |
| `InstanceService` | Instance/statement operations and provenance |
| `JobManager` | Background execution and job registry |

## Construction

```rust
let service = ApplicationService::new(store)
    .with_namespaces(namespace_registry);
```

`ApplicationService::new` builds:

- `OwlManager` over the store (schema graph);
- `SkosManager` (taxonomy graph);
- `ExternalOntologyManager` (external graph);
- `HashingEmbeddingProvider` with 128 dimensions;
- `InMemoryVectorIndex`;
- `EmbeddingService`;
- `ClassificationService` with semantic matching enabled;
- `RuleInferenceEngine` with the external registry;
- `JobManager` with concurrency 4;
- `InstanceService`;
- built-in namespaces.

`with_namespaces` replaces the registry; `namespaces()` exposes it.

Getters: `instances()`, `owl()`, `skos()`, `external()`, `classify()`,
`embedding()`, `inference()`, `jobs()`, `store()`.

## Command dispatch

```rust
impl OntologyService for ApplicationService {
    async fn execute(&self, command: Command) -> Result<CommandResult> {
        match command {
            Command::Rdf(c) => self.handle(c),
            Command::Ontology(c) => self.handle_ontology(c).await,
            Command::Taxonomy(c) => self.handle_taxonomy(c).await,
            Command::External(c) => self.handle_external(c).await,
            Command::Semantic(c) => self.handle_semantic(c).await,
            Command::Job(c) => self.handle_job(c).await,
        }
    }
}
```

The RDF handler is synchronous; the others are async because they call
taxonomy, provider, import, or job APIs.

`execute_rdf(RdfCommand)` is exposed for interfaces that dispatch by family.
`resolve_graph(name)` expands a prefixed or absolute graph name.

## Result construction

The service consistently wraps manager returns:

| Manager return | Command result |
| --- | --- |
| single object | `Resource { id, kind, resource }` |
| list | `Collection { items, count, limit, offset }` |
| mutation count | `Report { operation, affected }` |
| OWL validation | `Validation` |
| taxonomy/mapping validation | `Taxonomy` / `Mapping` |
| classification/embedding/inference | typed semantic result |
| serialized RDF | `Export` |
| job status | `Job` |

Resource `kind` values include `instance`, `owl_class`, `owl_property`,
`ontology`, `concept_scheme`, `concept`, `concept_collection`,
`external_ontology`, `import_report`, `class_mapping`, and
`property_mapping`.

For collections produced by managers with a total result count, `count` is the
page size, not the total. Pagination metadata reflects the requested limit and
offset.

## RDF command behavior

### Instances

`InstanceService` uses `data:asserted` as the mutation default.

| Operation | Behavior |
| --- | --- |
| `create_instance` | Rejects if the subject has any quads in the target graph; needs at least one type/property; inserts all quads; records provenance; returns the instance |
| `get_instance` | If no graph is given, searches all graphs; the reported graph is the requested one or the first matching quad's graph |
| `list_instances` | Reads `rdf:type` quads, excludes system graphs and provenance, optionally filters by graph/type, sorts by IRI, then applies offset/limit |
| `update_instance` | Requires the instance in the target graph; replaces types if present; replaces each named predicate (`Some(None)` clears); commits in one transaction; records provenance |
| `delete_instance` | Without a graph, deletes the subject from every graph; records provenance once per graph |
| `statements` | Returns all statements for the subject in the requested/all graphs |
| `add_statements` | Requires the instance in the target graph; validates each quad; inserts batch; records provenance |
| `delete_statements` | Resolves each stable statement ID; any unknown ID fails the whole command |
| `relationships` | Returns outgoing and incoming edges across all graphs |
| `instance_graphs` | Distinct graphs containing statements about the subject |

The stable statement identifier is derived from the quad, so deleting by ID
requires the same statement content still to exist.

### Dataset

- `query` / `update` call the store directly.
- `list_graphs` enumerates non-empty named graphs.
- `clear_graph` reads and deletes every quad in one graph.
- `import` parses an inline document and inserts, optionally forcing a graph.
- `export` serializes the dataset or one graph.

`clear_graph` and SPARQL Update can affect system graphs if the caller names
them. The HTTP/MCP layers expose these as destructive operations, but the
application service itself does not enforce graph protection.

## Provenance

`InstanceService` writes provenance records to `data:provenance` after
create/update/delete/add/delete-statements. A record includes:

- type `urn:onto:schema:ProvenanceRecord`
- operation name
- target graph
- affected quad count
- timestamp
- optional source

Provenance failures fail the command if the main mutation has already
completed. There is no outbox or retry.

## OWL, taxonomy, and external dispatch

Ontology commands delegate to `OwlManager` and return class/property/ontology
resources or validation reports.

Taxonomy commands delegate to `SkosManager`; merge and validation return
`CommandResult::Taxonomy`; tree returns a `concept_tree` resource; relation and
move mutations return reports.

External import/sync return `import_report` resources. Mapping creation returns
`class_mapping` or `property_mapping` resources. Mapping validation returns
`CommandResult::Mapping`.

## Semantic dispatch

| Command | Result |
| --- | --- |
| `Classify`, `ClassifyBatch` | `Classifications` |
| `ListClassifications` | `Classifications` |
| `OverrideClassification` | one `Classifications` item |
| `ReviewClassification` | one `Classifications` item |
| `Reclassify` | `Classifications` |
| `GenerateEmbeddings` | `Embeddings` |
| `SearchSimilar` | `Similarity` |
| `ListEmbeddings` | `Embeddings` |
| `DeleteEmbeddings` | `Report` |
| `PreviewInference` | `Preview` |
| `ApplyInference` | `Inference` |
| `ExplainInference` | `Preview` |

`ClassifyBatch` discards the batch's rejected list at this boundary and returns
only accepted classifications.

`GenerateEmbeddings` always passes `graph: None`, so generation reads the
asserted graph. Inference options carry explicit graph choices.

## Job dispatch

`JobCommand::Submit` returns the initial `JobStatus`. `Get` and `Results` are
currently the same operation: both return the full status including `result`.
`List` returns a collection with `limit = count` and `offset = 0`. `Cancel` and
`Retry` return updated statuses.

See [background-jobs.md](background-jobs.md).

## Error behavior

Domain errors propagate unchanged. The service does not retry, translate, or
partially succeed. If serialization of a result fails, the command fails with
`Error::Serialization`.

No authorization or rate limiting exists at this layer; those are interface
concerns (HTTP auth/rate limit, MCP policy).

## Tests

The service is exercised indirectly by all integration suites. Phase 1 covers
instance/statement behavior and provenance; later phases cover schema, taxonomy,
external, semantic, jobs, CLI, API, and MCP flows.

```sh
cargo test -p ontology-application
cargo test --workspace
```

## Limitations

- No unit-of-work spanning multiple commands.
- No transactional boundary across a main mutation and its provenance write.
- No graph protection for `clear_graph`/SPARQL Update.
- Batch classification drops rejected details at the command boundary.
- Job results are retained only in process.

## Related

- Core contracts: [core-domain.md](core-domain.md)
- Storage: [rdf-storage.md](rdf-storage.md)
- Interfaces: [http-api.md](http-api.md), [cli.md](cli.md),
  [mcp-server.md](mcp-server.md)
