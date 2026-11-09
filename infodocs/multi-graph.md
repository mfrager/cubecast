# Multi-Graph Backend (`ontology-graph`)

The multi-graph backend serves one logical dataset from a main store plus
optional satellite stores. It is disabled by default: without a `[graphs]`
section the process opens one `OxigraphStore` and behaves exactly as before.

`ontology-graph` owns three pieces:

- the **registry** — an RDF/XML root configuration that is parsed, validated,
  materialised into `urn:onto:system:graphs`, and diffed against the previous
  generation;
- the **catalog** — immutable snapshots mapping graph IRIs and namespaces to
  store handles;
- the **router** — a `MultiStore` that implements `ontology_core::rdf::RdfStore`
  and evaluates queries across stores.

Source: `crates/ontology-graph/src/`.

## Activation

```toml
[graphs]
enabled = true
registry = "./config/graphs.rdf"
root = "./data/graphs"
sync = "on_startup"       # "on_startup" | "manual"
update_policy = "strict"  # "strict" | "split"
```

`config/graphs.rdf` is the example registry. Enabling the section requires:

- `storage.backend = "persistent"` and `storage.path`;
- `graphs.registry` pointing at an RDF/XML file;
- `graphs.root` without `..` components;
- exactly one writable process per store directory.

When `enabled = false` or the section is absent, `GraphsConfig::default()`
selects single-store mode.

## Registry model

The file is authoritative. It contains a `GraphRegistry`, a `mainStore`
reference, and one `Store` per directory:

```xml
<onto:GraphRegistry rdf:about="urn:onto:system:graphs">
  <onto:registryVersion>1</onto:registryVersion>
  <onto:mainStore rdf:resource="urn:onto:store:main"/>
  <onto:entry rdf:resource="urn:onto:store:main"/>
</onto:GraphRegistry>

<onto:Store rdf:about="urn:onto:store:products">
  <onto:role>satellite</onto:role>
  <onto:storePath>./products</onto:storePath>
  <onto:access>ro</onto:access>
  <onto:mountOrder>10</onto:mountOrder>
  <onto:registration rdf:parseType="Resource">
    <onto:graphIri rdf:resource="urn:onto:data:asserted"/>
    <onto:namespace>https://products.example/</onto:namespace>
    <onto:required>true</onto:required>
    <onto:priority>10</onto:priority>
  </onto:registration>
</onto:Store>
```

| Field | Meaning |
| --- | --- |
| `role` | `main` (exactly one) or `satellite` |
| `storePath` | Directory; the main store uses `storage.path` instead |
| `access` | `rw` or `ro`; satellites default to read-only in practice |
| `mountOrder` | Ordering hint for operators and status output |
| `registration` | One or more graph IRIs, optional namespaces, `required`, `priority` |

A graph may be registered on only one store, and namespace prefixes must not
overlap ambiguously. Satellite paths resolve under `graphs.root`; paths that
escape it or point at a mismatched directory are rejected. Registry failures
surface as `Error::GraphRegistry`.

Each store directory holds a `graph.meta.json` marker with its identifier,
role, layout, graph list, and engine revision. Opening a directory whose marker
does not match the registry entry fails with `Error::GraphOpen`; this prevents
silently attaching an unrelated RocksDB directory.

## Startup

`GraphBackend::from_config(&config)` is the composition root used by the API,
the CLI server, and local MCP mode.

### Single mode

`GraphBackend::single(&storage)` opens one store, bootstraps the schema, and
builds a catalog snapshot with a single `urn:onto:store:main` handle.

### Registry mode, `sync = "on_startup"`

1. Open the main store at `storage.path` and bootstrap the schema.
2. Parse and validate the registry file against `graphs.root`.
3. Persist the registry into `urn:onto:system:graphs` (one transaction,
   including a content digest and generation).
4. Open every satellite read-only and build a catalog snapshot.
5. Publish the snapshot; the generation increments.

A missing optional store is recorded as degraded and is not created implicitly;
a missing *required* store aborts startup with `Error::GraphOpen`.

### Registry mode, `sync = "manual"`

Startup reads the registry already materialised in the main store. If the graph
is empty, startup fails with `Error::GraphRegistry`. Operators call
`GraphAdmin::sync_registry()` to load a changed file.

Optional unavailable stores are marked `Degraded` and their graphs yield empty
results; required missing stores fail the sync with `Error::GraphOpen`.

## Catalog

`CatalogSnapshot` is immutable for one generation. A `GraphCatalogImpl` holds
the current snapshot behind a lock and `publish` swaps it atomically.

| Type | Purpose |
| --- | --- |
| `GraphStoreId` | Validated store identifier, e.g. `urn:onto:store:products` |
| `StoreRole` | `main` or `satellite` |
| `StoreAccess` | `rw` or `ro` |
| `StoreLayout` | `single` (default graph only) or `named` |
| `StoreHealth` | `ready` or `degraded { reason }` |
| `StoreHandle` | Path, role, access, layout, graphs, namespaces, engine handle |
| `GraphRoute` | `{ graph, store }` pair used by query plans |

Resolution rules:

- `resolve_graph(graph)` returns the owning store or `GraphRegistry` when the
  graph is not registered;
- `resolve_namespace(iri)` chooses the longest matching namespace binding and
  falls back to the main store;
- `graph_for_namespace(namespace)` returns the bound graph, or the asserted
  graph when there is no exact binding;
- `engine_graph(graph)` maps a logical graph to `Default` for a `single`
  layout store and to the named graph for a `named` layout store;
- `routes_for`, `enabled_graphs`, and `status` support diagnostics.

## Query routing

`MultiStore` is a drop-in `RdfStore`. Query planning happens once per query
text and generation, cached for 512 entries:

- a graph that lives on a `single`-layout satellite is fused into the host
  query where possible;
- a `named`-layout satellite is reached through an in-process
  `SERVICE <oxigraph:///absolute/path>` clause, served by
  `ontology_rdf::LocalFederation`;
- `GRAPH ?g` expands to a union over every registered graph;
- an unregistered graph yields an empty branch and a fallback record;
- degraded stores contribute no solutions;
- `FROM` and `FROM NAMED` dataset clauses are rejected.

The host store evaluates the rewritten query; no external HTTP endpoint or
SPARQL protocol server is involved. `QueryPlan` records the host, rewritten
query, routes, referenced graphs, and any `FallbackReason`.

## Writes and update policy

Direct quad inserts/deletes are grouped by owning store. Writes to a read-only
satellite fail with `Error::GraphReadOnly`, and blank-node subjects cannot be
written to a satellite because their scope would change.

For SPARQL updates that touch more than one store:

| `update_policy` | Behavior |
| --- | --- |
| `strict` (default) | Reject before writing; `Error::Validation` names the stores |
| `split` | Execute one group per store; only `INSERT DATA`/`DELETE DATA` are supported |

Cross-store `DELETE/INSERT` is `Unsupported`. In `split` mode a later failure
returns `Error::GraphPartial { applied, failed, reason }`; earlier groups stay
committed, so there is no cross-store rollback. Direct quad batches are also
not atomic across stores.

Satellite maintenance writes go through `GraphAdmin::write` or
`with_writable_store`, which reopens the store read-write for the closure.
There is no HTTP, CLI, or MCP surface for this today.

## Administration

`GraphBackend` exposes:

```rust
let backend = GraphBackend::from_config(&config)?;
let store = backend.rdf_store();      // Arc<dyn RdfStore>
let catalog = backend.catalog();      // read-side discovery
let admin = backend.admin();          // Arc<dyn GraphAdmin>
```

`GraphAdmin`:

| Method | Effect |
| --- | --- |
| `sync_registry` | Re-read, diff, persist, and publish a new generation |
| `reload` | Same as sync for the current file |
| `status` | Per-store identifier, path, role, access, layout, health, graphs, quads |
| `validate` | Registry invariants plus each store's index validation |
| `backup` | Copy every store into a target directory and write `manifest.json` |
| `with_writable_store` | Run one closure against a satellite opened read-write |

`backup` refuses an existing target with `Conflict` and includes the generation,
engine revision, store identifiers, directories, and quad counts in the
manifest. Restoration is manual, as with the single-store path.

## Compatibility and limits

- Domain managers are unchanged: they receive `Arc<dyn RdfStore>` and do not
  know whether a router or one store is behind it.
- Document extraction maps into `urn:onto:data:document:<hex>:*` graphs. Those
  IRIs are created per document and are not in the registry, so with
  `[graphs].enabled` an extraction that writes the RDF mapping fails with
  `GraphRegistry` unless those graphs are registered in advance. Extraction
  without the RDF mapping is unaffected.
- There is no online rebalancing, graph migration, or store removal workflow;
  sync retires a store from future routing but does not move its data.
- Only Oxigraph directories are supported; a satellite is not a generic SPARQL
  endpoint.
- Registry administration is library-only at present.

## Tests

`cargo test -p ontology-graph` covers registry parsing/validation, paths and
markers, catalog resolution, query planning, fallbacks, single mode, and
backup. `tests/tests/phase7_multigraph.rs` adds 12 end-to-end tests: cross-store
round trips and joins, fusion, variable graphs, resync/retire, degraded and
missing stores, read-only writes, strict and split updates, backup, namespace
resolution, and single-mode parity.

## Related

- Storage engine: [rdf-storage.md](rdf-storage.md)
- Configuration: [core-domain.md](core-domain.md)
- Operations: [../docs/operations.md](../docs/operations.md)
- Document graphs: [document-extraction.md](document-extraction.md)
