# Multi-Graph Backend — Design and Implementation Plan

Rust · Oxigraph (`basenote` fork) · RocksDB · SPARQL 1.1 · RDF/XML registry

Status: proposed
Scope: storage layout, graph registry, query routing, internal API
Supersedes: the single-directory assumption documented in `infodocs/rdf-storage.md`

---

## 1. Objective

Today one process opens exactly one RocksDB directory (`storage.path`), and every
managed graph (`urn:onto:system:schema`, `urn:onto:data:asserted`, …) is a logical
partition inside it. This plan introduces a **multi-graph backend**:

1. A **main graph directory** that keeps the reserved system graphs, the asserted
   data, and the authoritative copy of the graph registry.
2. **Satellite graph stores** — further RocksDB directories, each holding one or
   more graphs — declared in a **root configuration** that is loaded into the
   main graph.
3. The root configuration is authored as an **RDF/XML file** and **synchronised**
   into the main graph at startup (and on demand), giving the registry a
   queryable, versioned, RDF-native representation.
4. A **namespace map** binding namespaces to graphs to stores, so callers keep
   writing `ex:Product` and `GRAPH <urn:onto:external:dbpedia>` without knowing
   which directory answers.
5. **Dynamic query construction**: a router rewrites each SPARQL query into a
   single-host evaluation with in-process `SERVICE <oxigraph://…>` fan-out, so
   joins across directories stay inside one engine and keep SPARQL semantics.
6. An **internal API** (`GraphBackend`: catalog + router + admin) used by
   `ApplicationService`, `InstanceService`, the OWL/SKOS/classification/
   inference/embedding managers, job workers, and the HTTP/CLI/MCP adapters,
   for storage management and for connecting to the configured RDF instances.

Non-goals for this phase: per-graph *files* inside one engine, distributed
transactions across directories, and remote SPARQL federation as a supported
storage mode (the router leaves user-written `SERVICE` clauses untouched, but the
registry only manages local stores).

---

## 2. Starting Point and Constraints

| Fact | Consequence |
| --- | --- |
| `RdfStore` (`crates/ontology-core/src/rdf.rs`) is the only storage boundary used above the adapter | A composite implementation is a drop-in; no interface changes required |
| `OxigraphStore` (`crates/ontology-rdf/src/store.rs`) owns one `Store` and translates all Oxigraph types | The router must live above it and must not expose Oxigraph types |
| `ApplicationService::new(Arc<dyn RdfStore>)` | `Arc<MultiStore>` is passed in unchanged |
| `GraphKind` (`crates/ontology-core/src/graph.rs`) fixes the reserved graph IRIs | Main store keeps these; registry adds new graphs, never renames reserved ones |
| `Config` (`crates/ontology-core/src/config.rs`) has `storage`, `logging`, `namespaces`, `limits` | Adds one `[graphs]` table; `storage.path` remains the main directory |
| Oxigraph `Store` allows one read-write handle per directory (`LOCK`) | Exactly one writer per store; satellites are read-only while serving queries |
| `basenote` fork: `LocalStoreServiceHandler` resolves `oxigraph://<path>` and fuses `SERVICE` into a virtual `GRAPH` over a snapshot; fusion is skipped when the inner pattern contains `GRAPH`, is `SILENT`, or names the service by variable | Cross-store joins are cheap **only** for default-graph layouts; named-graph layouts fall back to the (correct but slower) handler path |
| Fusion reuses term encodings but does not remap blank nodes | Blank nodes must not cross store boundaries in fused queries |
| `spargebra 0.5.0-dev` is already in `Cargo.lock` from the same fork revision | Query rewriting uses the engine's own parser/AST, not a text scanner |

---

## 3. Target Architecture

```
                        HTTP API        CLI        MCP
                             \           |          /
                              ApplicationService (unchanged signature)
                                        |
                                  Arc<dyn RdfStore>
                                        |
                             +---------------------+
                             |  MultiStore (router) |   ontology-graph
                             |  - rewrite           |
                             |  - route             |
                             |  - merge/join        |
                             +----------+----------+
                                        |
                    +-------------------+-------------------+
                    |                                       |
            Main store (rw)                        Satellite stores (ro)
        ./data/ontology                        ./data/graphs/<name>
        urn:onto:system:schema                 urn:onto:external:dbpedia
        urn:onto:system:graphs   <-- registry  urn:onto:data:products
        urn:onto:data:asserted                 ...
        urn:onto:data:inferred
                    ^
                    | sync (RDF/XML -> RDF, diff, transaction)
        ./config/graphs.rdf  (root configuration, authored)
```

### 3.1 Terminology

| Term | Meaning |
| --- | --- |
| **Main store** | The RocksDB directory at `storage.path`; always writable, hosts the registry and reserved graphs |
| **Satellite store** | A registered RocksDB directory holding one or more graphs; opened read-only during query serving |
| **Graph** | A named graph IRI (`GraphName::Iri`) visible through `GRAPH <iri>`; exactly one owning store |
| **Store** | A physical RocksDB directory; identified by a registry subject (`urn:onto:store:<name>`) |
| **Logical layout** | How a satellite maps its registered graph onto the engine: `single` (data in the unnamed graph, fusion-friendly) or `named` (data in its own named graph) |
| **Registry** | The RDF description of stores, graphs, and namespace bindings, synced into `urn:onto:system:graphs` |
| **Generation** | Monotonic counter bumped on every successful registry sync; invalidates plans and cached handles |
| **Host** | The single store whose engine evaluates a given query |

---

## 4. Storage Layout and Graph Files

### 4.1 Directory layout

```
data/
├── ontology/                     # main graph dir (storage.path)
│   ├── CURRENT  MANIFEST-*  OPTIONS-*  *.sst  *.log  LOCK
│   └── graph.meta.json           # store marker written by this system
├── graphs/
│   ├── dbpedia/                  # satellite, layout = single
│   │   ├── … RocksDB files …
│   │   └── graph.meta.json
│   └── products/                 # satellite, layout = named
│       ├── … RocksDB files …
│       └── graph.meta.json
└── backups/
    └── 2026-11-08T12-00-00Z/
        ├── manifest.json         # generation, per-store digests
        ├── ontology/
        └── graphs/dbpedia/
```

Rules:

- `storage.path` **is** the main graph directory; `[graphs].root` (default
  `./data/graphs`) is the only allowed parent for satellite paths.
- Satellite paths are relative to `[graphs].root` or absolute; both are
  canonicalised at load and must resolve inside the root (symlink escape is an
  error).
- Every store directory contains `graph.meta.json`, written at creation and
  verified on every open:

```json
{
  "formatVersion": 1,
  "storeId": "urn:onto:store:dbpedia",
  "role": "satellite",
  "graphs": ["urn:onto:external:dbpedia"],
  "layout": "single",
  "createdAt": "2026-11-08T10:00:00Z",
  "engine": "oxigraph-0.6.0-dev+ae2a2ae"
}
```

A mismatch between marker, registry entry, and requested graph is **fail-closed**
(`Error::GraphOpen`): this prevents pointing the registry at the wrong directory
after a manual copy.

### 4.2 What stays a single directory

The RocksDB file set remains engine-owned and unused by application code. This
plan does **not** introduce a file-per-graph format; it introduces a
**directory-per-store** layout with multiple logical graphs per directory. That
keeps the tested backup/recovery procedure intact (`docs/operations.md`).

---

## 5. Root Configuration (RDF/XML) and Registry Sync

### 5.1 Location and activation

```toml
# config/ontology.toml
[storage]
backend = "persistent"
path = "./data/ontology"          # main graph dir

[graphs]
enabled = true
registry = "./config/graphs.rdf"  # RDF/XML root configuration
root = "./data/graphs"            # satellite parent directory
sync = "on_startup"               # on_startup | manual
update_policy = "strict"          # strict | split
```

- `[graphs]` absent or `enabled = false` → `SingleStore` backend, byte-for-byte
  today's behaviour.
- `sync = "manual"` skips the startup sync; the registry is then whatever the
  main store holds (still validated for internal consistency).
- `registry` must exist when `enabled = true`; a missing file is a hard error
  (never silently fall back to an empty registry).

`ontology-core` grows one struct (the file above is a proposal; the shipped
`config/ontology.toml` stays unchanged until Phase 7.3):

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub registry: Option<PathBuf>,
    #[serde(default = "default_graphs_root")]
    pub root: PathBuf,                 // default "./data/graphs"
    #[serde(default)]
    pub sync: GraphsSync,              // OnStartup | Manual
    #[serde(default)]
    pub update_policy: UpdatePolicy,   // Strict | Split
}
```

`Config::validate` gains: `enabled` requires `registry`; `root` must exist or be
creatable inside the working tree; `storage.path` must be a persistent backend
(the registry has no meaning for an in-memory store, so `enabled` + memory is a
config error).

### 5.2 Vocabulary

All terms live in the existing internal namespace `urn:onto:schema:`
(`ontology_core::namespace::ONTO`), so no new prefix is introduced.

| Class / property | Domain | Meaning |
| --- | --- | --- |
| `onto:GraphRegistry` | — | Registry document; subject is `urn:onto:system:graphs` |
| `onto:Store` | — | One physical store (directory) |
| `onto:GraphRegistration` | — | One graph hosted by a store |
| `onto:registryVersion` | Registry | Schema version of this document (`1`) |
| `onto:mainStore` | Registry | The store whose role is `main` |
| `onto:entry` | Registry | Store membership (repeatable) |
| `onto:role` | Store | `main` or `satellite` |
| `onto:storePath` | Store | Directory, relative to `[graphs].root` or absolute |
| `onto:access` | Store | `rw` or `ro` |
| `onto:layout` | Store | `single` or `named` (default `named`) |
| `onto:mountOrder` | Store | Integer tie-breaker for host election (lower wins) |
| `onto:registration` | Store | Graph registration (repeatable) |
| `onto:graphIri` | Registration | The logical graph IRI |
| `onto:namespace` | Registration / Store | Namespace owned through this graph (repeatable) |
| `onto:priority` | Registration | Integer; when two namespaces are prefixes of one IRI |
| `onto:enabled` | Registration | `false` keeps the entry but excludes it at runtime |
| `onto:required` | Registration | `false` allows a missing directory to degrade to empty |
| `onto:strictBlankNodes` | Registration | `true` refuses fused evaluation of a satellite that holds blank nodes |

### 5.3 Example root configuration

```xml
<?xml version="1.0" encoding="utf-8"?>
<rdf:RDF
    xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:dcterms="http://purl.org/dc/terms/"
    xmlns:onto="urn:onto:schema:">

  <onto:GraphRegistry rdf:about="urn:onto:system:graphs">
    <dcterms:title>Ontology graph registry</dcterms:title>
    <onto:registryVersion>1</onto:registryVersion>
    <onto:mainStore rdf:resource="urn:onto:store:main"/>
    <onto:entry rdf:resource="urn:onto:store:main"/>
    <onto:entry rdf:resource="urn:onto:store:products"/>
    <onto:entry rdf:resource="urn:onto:store:dbpedia"/>
  </onto:GraphRegistry>

  <onto:Store rdf:about="urn:onto:store:main">
    <onto:role>main</onto:role>
    <onto:storePath>./data/ontology</onto:storePath>
    <onto:access>rw</onto:access>
    <onto:mountOrder>0</onto:mountOrder>

    <onto:registration rdf:parseType="Resource">
      <onto:graphIri rdf:resource="urn:onto:system:schema"/>
      <onto:namespace>urn:onto:</onto:namespace>
      <onto:namespace>https://example.org/</onto:namespace>
    </onto:registration>

    <onto:registration rdf:parseType="Resource">
      <onto:graphIri rdf:resource="urn:onto:data:asserted"/>
      <onto:namespace>https://acme.example/</onto:namespace>
    </onto:registration>

    <onto:registration rdf:parseType="Resource">
      <onto:graphIri rdf:resource="urn:onto:data:inferred"/>
    </onto:registration>

    <onto:registration rdf:parseType="Resource">
      <onto:graphIri rdf:resource="urn:onto:data:classification"/>
    </onto:registration>
  </onto:Store>

  <onto:Store rdf:about="urn:onto:store:products">
    <onto:role>satellite</onto:role>
    <onto:storePath>products</onto:storePath>
    <onto:access>ro</onto:access>
    <onto:layout>named</onto:layout>
    <onto:mountOrder>10</onto:mountOrder>

    <onto:registration rdf:parseType="Resource">
      <onto:graphIri rdf:resource="urn:onto:data:products"/>
      <onto:namespace>https://products.example/</onto:namespace>
    </onto:registration>
  </onto:Store>

  <onto:Store rdf:about="urn:onto:store:dbpedia">
    <onto:role>satellite</onto:role>
    <onto:storePath>dbpedia</onto:storePath>
    <onto:access>ro</onto:access>
    <onto:layout>single</onto:layout>
    <onto:mountOrder>20</onto:mountOrder>

    <onto:registration rdf:parseType="Resource">
      <onto:graphIri rdf:resource="urn:onto:external:dbpedia"/>
      <onto:namespace>http://dbpedia.org/</onto:namespace>
    </onto:registration>
  </onto:Store>

</rdf:RDF>
```

`rdf:parseType="Resource"` keeps each registration inline and anonymous; named
registrations (`rdf:about="urn:onto:registration:dbpedia"`) are also accepted,
which is what a write-back would produce if that is added later.

### 5.4 Sync algorithm

`GraphAdmin::sync_registry()`:

1. **Read** the file bytes; record `sourceDigest = sha256(bytes)`
   (`sha2` is already a dependency of `ontology-core`).
2. **Parse** with `RdfFormat::RdfXml` and base IRI = `file://<absolute path>`,
   so `rdf:resource="./products"` resolves deterministically. Any parse error
   aborts without touching the store.
3. **Extract** a `RegistryModel` from the quads (see `ontology-graph/src/registry.rs`),
   rejecting entries outside the document subject graph.
4. **Validate** (all failures reported together, not one at a time):
   - exactly one `onto:mainStore`; exactly one store with `onto:role "main"`;
   - `main.storePath`, canonicalised, equals `storage.path`;
   - every `onto:graphIri` is absolute and unique across stores;
   - reserved graphs (`GraphKind::ALL`) are owned by the main store only;
   - `layout = "single"` requires exactly one registration on that store;
   - every `onto:namespace` is an absolute IRI and owned by exactly one
     registration (no namespace may span two graphs);
   - satellite `storePath` is inside `[graphs].root` after canonicalisation;
   - `mountOrder` unique; no duplicate store ids;
   - `onto:registryVersion` is a version this build understands.
5. **Open** every satellite read-only, verify `graph.meta.json`, and compute a
   content digest (`quad_count` + `store.len()` + first/last graph IRI is too
   weak; use the engine's directory `CURRENT`/`MANIFEST` identity plus
   `quad_count`, recorded as `contentFingerprint`). A missing directory is an
   error unless `onto:required` is `false`, in which case the store is marked
   `Degraded` and every query routing to it returns empty results (SERVICE
   `SILENT` semantics) with a warning event.
6. **Diff** against the current snapshot: added / changed / removed / unchanged,
   keyed by store id and graph IRI.
7. **Persist** in the main store inside one transaction:
   - delete the previous contents of `urn:onto:system:graphs`, insert the new
     registry quads, set `onto:generation = previous + 1`, `dcterms:modified`,
     `onto:sourceDigest`, and `prov:wasGeneratedBy` provenance;
   - commit. A failed commit leaves both the store and the published snapshot
     untouched.
8. **Publish** a new `Arc<CatalogSnapshot>` under the router's `RwLock`; the old
   snapshot stays alive for in-flight queries.
9. **Close** retired stores once their `Arc` count drops; log a
   `GraphSyncReport { generation, added, changed, removed, unchanged, degraded }`.

Sync is one-way: **the file is authoritative**. The copy in
`urn:onto:system:graphs` is a materialised view with the source digest, so
runtime code never has to parse XML and can join registry data with ordinary
SPARQL. Mutating the registry through the API is rejected in this phase
(`Error::Unsupported { operation: "registry mutation" }`); a later phase may add
`graph registry export --write`.

### 5.5 Bootstrapping order

```
Config::load + validate
  └─ open main store (storage.path)
       └─ bootstrap_schema()                     # existing, unchanged
            └─ if graphs.enabled: sync_registry()
                 ├─ open satellites (read-only, marker check)
                 ├─ validate + persist registry in main
                 └─ publish snapshot (generation 1)
                      └─ ApplicationService::new(Arc<MultiStore>)
```

The cycle "registry lives in the store whose path the registry declares" is
broken by taking the main path from TOML, never from RDF, and validating the two
against each other.

---

## 6. Namespace Mapping

### 6.1 Model

A **binding** is `(namespace, graph, store, priority)`. Resolution is
longest-namespace-prefix match over the registry:

```rust
let store = catalog.resolve_namespace(&Id::new("https://products.example/Widget")?)?;
// -> urn:onto:store:products, graph urn:onto:data:products
let graph = catalog.graph_for_namespace("https://products.example/")?;
```

Rules:

1. **Exclusive ownership** — a namespace belongs to exactly one graph. This is
   what makes write routing unambiguous.
2. **Reserved first** — `urn:onto:` is owned by the main store; a satellite may
   not claim it or any `urn:onto:system:` graph.
3. **Longest prefix wins**, then higher `onto:priority`, then `mountOrder`;
   remaining ties are a validation error rather than a runtime guess.
4. **Unmapped namespaces** resolve to the main store's asserted graph (today's
   behaviour), and a warning counter is incremented so operators can find gaps.
5. `NamespaceRegistry` (`ontology-core`) keeps handing out prefixes for
   rendering; the catalog owns the *storage* mapping. The two are linked at
   startup: every configured `[namespaces]` entry is registered in the
   `NamespaceRegistry`, while storage ownership comes only from the RDF registry.

### 6.2 Graph file → namespace → store table

`GraphAdmin::status()` renders exactly this table, which is also the operator's
mental model:

| Graph | Namespace(s) | Store | Path | Layout | Access |
| --- | --- | --- | --- | --- | --- |
| `urn:onto:system:schema` | `urn:onto:`, `https://example.org/` | `urn:onto:store:main` | `./data/ontology` | named | rw |
| `urn:onto:data:asserted` | `https://acme.example/` | `urn:onto:store:main` | `./data/ontology` | named | rw |
| `urn:onto:data:products` | `https://products.example/` | `urn:onto:store:products` | `./data/graphs/products` | named | ro |
| `urn:onto:external:dbpedia` | `http://dbpedia.org/` | `urn:onto:store:dbpedia` | `./data/graphs/dbpedia` | single | ro |

---

## 7. Dynamic Query Construction

### 7.1 Principles

1. **One host, one evaluation.** The query is never split into per-store result
   sets and merged in Rust: joins, `OPTIONAL`, aggregation, `ORDER BY`,
   `DISTINCT`, and solution ordering must stay inside a SPARQL engine.
2. **Cross-store data is reached through `SERVICE <oxigraph://…>`**, which the
   `basenote` fork evaluates in-process (fused virtual `GRAPH` or handler
   fallback). No HTTP hop, no per-tuple boundary cost on the fused path.
3. **Rewriting is structural** (parsed AST), never textual; the plan cache key is
   `(query_text, generation)`.
4. **Main is never opened read-only** while its writer is live; the router never
   emits a `SERVICE` clause targeting the main store.

### 7.2 Host election

Given the set `G` of graphs referenced by the query:

| Condition | Host |
| --- | --- |
| `G` empty (default-graph only) | main |
| main ∈ `G` | main |
| `G` all owned by one satellite | that satellite |
| otherwise | satellite owning most of `G`; tie → lowest `mountOrder` |

Consequences: single-store queries on a satellite are evaluated by that
satellite's own read-only handle (fast, no `SERVICE`); anything touching main is
evaluated by main with `SERVICE` clauses to satellites.

### 7.3 Referenced-graph analysis

The router walks the parsed query (`spargebra::algebra::GraphPattern`) and
classifies:

- `GRAPH <iri>` / `GRAPH <iri> { … }` with a constant IRI → a concrete graph;
- `GRAPH ?var` → every enabled registered graph (the variable is bound per
  branch in the rewrite);
- `FROM` / `FROM NAMED` → **rejected** in this phase
  (`Error::Unsupported { operation: "query dataset clause" }`), because the
  dataset clause would silently deselect satellite data;
- user-written `SERVICE` → left untouched (resolved by the handler if it is
  `oxigraph://`, otherwise by whatever handler the caller configured; the
  default evaluator has none, so it errors as today);
- `GRAPH ?g` combined with `GRAPH <iri>` → handled by the same union rewrite,
  with constants taking precedence.

### 7.4 Rewrite rules

Let `H` be the host and `S(g)` the store owning graph `g`.

**R1 — graph in host store.** `GRAPH <g> { P }` stays as written.

**R2 — graph in a satellite, layout `named`.**

```sparql
GRAPH <g> { P }
```
becomes
```sparql
SERVICE <oxigraph:///abs/data/graphs/products> { GRAPH <g> { P } }
```

The `GRAPH` inside the SERVICE disables fusion (by design in the fork), so this
uses the handler path: correct, streaming, but with thread + channel overhead per
call. Acceptable for this layout.

**R3 — graph in a satellite, layout `single`.**

```sparql
GRAPH <g> { P }
```
becomes
```sparql
SERVICE <oxigraph:///abs/data/graphs/dbpedia> { P }
```

No inner `GRAPH`, so `fuse_query` turns the `SERVICE` into a virtual `GRAPH`
over the satellite snapshot and the join runs on the fused dataset — the fast
path. The registry guarantees a `single` store hosts exactly one graph, so the
mapping `iri ↔ unnamed graph` is unambiguous.

**R4 — variable graph.** For `GRAPH ?g { P }`:

```sparql
{
  BIND(<urn:onto:data:asserted> AS ?g) GRAPH <urn:onto:data:asserted> { P' }
}
UNION
{
  BIND(<urn:onto:data:products> AS ?g) GRAPH <urn:onto:data:products> { P' }
}
UNION
{
  BIND(<urn:onto:external:dbpedia> AS ?g) GRAPH <urn:onto:external:dbpedia> { P' }
}
```

Branches that route to the host keep `GRAPH`; branches in satellites become
`SERVICE` per R2/R3. Graph ownership is disjoint, so the union introduces no
duplicates. Empty registrations are omitted.

**R5 — default graph (no `GRAPH`).** The host's unnamed graph is queried as
today. Satellite data is *not* implicitly in the default graph: callers must
name a graph or use `GRAPH ?g`. This keeps `data:asserted` semantics stable and
makes fan-out explicit.

**R6 — property paths, sub-selects, `VALUES`, `EXISTS`.** No special casing;
`P` is rewritten recursively, so nested patterns inside `EXISTS`, `NOT EXISTS`,
sub-`SELECT`s, and `MINUS` are routed the same way. The virtual graph name is
the `oxigraph://…` IRI itself, which can never equal a registered graph IRI
(those are `urn:` or `http(s):`), so the fork's outer-collision guard cannot
trigger on generated plans.

**R7 — blank nodes.** `SERVICE` evaluation re-encodes terms; blank nodes are not
remapped by fusion. The router refuses to fuse a branch whose satellite contains
blank nodes only if `onto:strictBlankNodes` is set (default `false`); writes to
satellites reject blank-node subjects (`Error::Validation`). Queries that need
them use the handler path.

### 7.5 Worked example

Query (as authored by a caller):

```sparql
PREFIX ex: <https://acme.example/>
PREFIX p:  <https://products.example/>
SELECT ?name ?price WHERE {
  GRAPH <urn:onto:data:asserted> { ?p ex:sku ?sku . ?p ex:name ?name }
  GRAPH <urn:onto:data:products> { ?p p:price ?price }
}
```

Plan: `G = { asserted, products }`, main ∈ `G` → host = main. Rewrite:

```sparql
SELECT ?name ?price WHERE {
  GRAPH <urn:onto:data:asserted> { ?p ex:sku ?sku . ?p ex:name ?name }
  SERVICE <oxigraph:///srv/onto/data/graphs/products> {
    GRAPH <urn:onto:data:products> { ?p p:price ?price }
  }
}
```

Evaluated once by main; the `SERVICE` is resolved by
`LocalStoreServiceHandler` against the satellite's read-only snapshot.

### 7.6 Plan cache

```rust
struct PlanCache {
    generation: Generation,
    entries: HashMap<String, Arc<QueryPlan>>, // bounded, e.g. 512, FIFO eviction
}

struct QueryPlan {
    host: GraphStoreId,
    rewritten: String,
    routes: Vec<GraphRoute>,       // graph -> store, for diagnostics
    fallbacks: Vec<FallbackReason>,// e.g. NamedLayoutService, VariableGraphUnion
    referenced: BTreeSet<Id>,
}
```

Cache hit requires the same generation; a sync clears the cache wholesale. This
keeps invalidation trivially correct.

### 7.7 Update routing

| Update form | Routing |
| --- | --- |
| `INSERT DATA { GRAPH <g> { … } }` | group quads by owning store |
| `DELETE DATA { … }` | same, with graph/namespace fallback to main |
| `DELETE { … } INSERT { … } WHERE { … }` with explicit `GRAPH` in templates | one host (the store owning the template's `WITH`/`GRAPH`), `WHERE` rewritten per §7.4 |
| `DELETE/INSERT` without `GRAPH`, `WITH <g>` given | host = owner of `g`; patterns over other graphs use `SERVICE` |
| `LOAD <url> INTO GRAPH <g>` | owner of `g` only |
| `CLEAR`/`DROP`/`CREATE`/`COPY`/`MOVE`/`ADD` | owner(s) of the referenced graphs; cross-store `COPY`/`MOVE` rejected |

Policy `update_policy`:

- `strict` (default): an update touching more than one store is rejected before
  any write, with the offending graphs and stores in the error.
- `split`: groups execute sequentially in `mountOrder`; each group is its own
  engine transaction. On failure, remaining groups are not attempted and the
  error carries `applied`/`failed`. There is **no cross-store rollback** —
  documented in §10 and surfaced as `Error::GraphPartial`.

---

## 8. Internal API

New crate `ontology-graph`, depending on `ontology-core`, `ontology-rdf`,
`spargebra`, `serde`, `tracing`, and `sha2`. `ontology-application` depends on it
and keeps receiving `Arc<dyn RdfStore>`.

The `spargebra` dependency must resolve to the **same fork revision** as
`oxigraph`, not crates.io, or the AST types will not match the engine's parser:

```toml
# workspace Cargo.toml
spargebra = { git = "https://github.com/mfrager/oxigraph", branch = "basenote", package = "spargebra" }
```

`Cargo.lock` already contains `spargebra 0.5.0-dev` from
`ae2a2ae6cf7acf7f82dc4f69fe886b138c0f4625`, so this adds no new source.

### 8.1 Module layout

```
crates/ontology-graph/
├── src/lib.rs          # GraphBackend facade, re-exports
├── src/catalog.rs      # CatalogSnapshot, GraphStoreId, StoreHandle, resolution
├── src/registry.rs     # RegistryModel, RDF/XML parse, validation, diff
├── src/sync.rs         # sync_registry, generation, registry persistence
├── src/pool.rs         # open/close, graph.meta.json, with_writable_store
├── src/rewrite.rs      # spargebra-based query/update rewriting, PlanCache
├── src/router.rs       # MultiStore: impl RdfStore
├── src/admin.rs        # GraphAdmin: status, validate, backup, reload
└── src/error.rs        # GraphError -> ontology_core::Error conversions
```

### 8.2 Core types

```rust
/// Registry subject local name, e.g. `urn:onto:store:dbpedia`.
pub struct GraphStoreId(String);

pub struct Generation(pub u64);

pub enum StoreRole { Main, Satellite }
pub enum StoreAccess { ReadWrite, ReadOnly }
pub enum StoreLayout { Single, Named }
pub enum StoreHealth { Ready, Degraded { reason: String }, Closed }

/// One open engine handle plus its registry metadata.
pub struct StoreHandle {
    id: GraphStoreId,
    path: PathBuf,
    role: StoreRole,
    access: StoreAccess,
    layout: StoreLayout,
    generation: Generation,
    opened_at: SystemTime,
    store: Arc<OxigraphStore>,
}

/// Immutable view of the registry at one generation.
pub struct CatalogSnapshot {
    generation: Generation,
    main: Arc<StoreHandle>,
    stores: BTreeMap<GraphStoreId, Arc<StoreHandle>>,
    graph_index: BTreeMap<Id, Arc<StoreHandle>>,   // logical graph -> store
    namespace_index: NamespaceIndex,               // longest-prefix lookup
    layout_index: BTreeMap<Id, StoreLayout>,
}

pub struct GraphRoute { pub graph: Id, pub store: GraphStoreId }
```

### 8.3 Traits

```rust
/// Read-side discovery. Cheap, lock-free after one RwLock read.
pub trait GraphCatalog: Send + Sync {
    fn snapshot(&self) -> Arc<CatalogSnapshot>;
    fn generation(&self) -> Generation;

    fn resolve_graph(&self, graph: &Id) -> Result<Arc<StoreHandle>>;
    fn resolve_namespace(&self, iri: &Id) -> Result<Arc<StoreHandle>>;
    fn graph_for_namespace(&self, namespace: &str) -> Result<Id>;
    fn routes_for(&self, graphs: &BTreeSet<Id>) -> Vec<GraphRoute>;
    fn status(&self) -> Vec<StoreStatus>;
}

/// Write/administrative side. All mutating calls bump the generation when they
/// change the catalog, and never run concurrently with a sync.
pub trait GraphAdmin: Send + Sync {
    /// Parse the RDF/XML registry, diff, persist into the main store, publish.
    fn sync_registry(&self) -> Result<GraphSyncReport>;

    /// Re-open all stores at the published generation (after out-of-band edits).
    fn reload(&self) -> Result<GraphSyncReport>;

    fn status(&self) -> Vec<StoreStatus>;

    /// Per-store index validation plus registry invariants.
    fn validate(&self) -> Result<Vec<StoreValidation>>;

    /// Consistent backup of every store at the current generation.
    fn backup(&self, target: &Path) -> Result<GraphBackupReport>;

    /// Quiesce a satellite, reopen read-write, run `operation`, flush, reopen
    /// read-only, bump the store's local generation. The only supported way to
    /// write a satellite.
    fn with_writable_store<T>(
        &self,
        id: &GraphStoreId,
        operation: impl FnOnce(&OxigraphStore) -> Result<T>,
    ) -> Result<T>;
}
```

`with_writable_store` is the answer to the read-only/writer conflict: satellites
are opened read-only for queries, so a writer never coexists with a reader on
the same directory. The operation holds an exclusive guard on that store; if any
query plan is currently using it, the call waits (`RwLock`), then reopens.

### 8.4 `MultiStore`

```rust
pub struct MultiStore {
    catalog: Arc<GraphCatalogImpl>,   // snapshot in RwLock
    admin: Arc<GraphAdminImpl>,
    handler: LocalStoreServiceHandler,
    plans: Mutex<PlanCache>,
}

impl RdfStore for MultiStore {
    fn get(&self, pattern: QuadPattern) -> Result<Vec<Quad>>;
    fn insert(&self, quads: &[Quad]) -> Result<()>;
    fn delete(&self, quads: &[Quad]) -> Result<()>;
    fn query(&self, sparql: &str) -> Result<QueryResult>;
    fn update(&self, sparql: &str) -> Result<()>;
    fn graphs(&self) -> Result<Vec<GraphName>>;
    fn load_from_slice(…, graph: Option<&GraphName>) -> Result<u64>;
    fn serialize(…, graph: Option<&GraphName>) -> Result<String>;
    fn transaction(&self) -> Result<Box<dyn RdfTransaction + '_>>;
    fn len(&self) -> Result<usize>;
}
```

Semantics per method:

| Method | Behaviour |
| --- | --- |
| `get` | Resolve graph (or all stores when `None`); translate `Iri(g)` → engine graph (`Default` for `single` layout); merge, keeping the logical graph label on each quad |
| `insert` / `delete` | Group by owning store; reject writes to `ro` stores (`Error::GraphReadOnly`); translate layout; `Default`-graph quads go to main |
| `query` | Plan, rewrite, evaluate on the host with `query_with_services(…)` |
| `update` | Partition per §7.7, then execute per store |
| `graphs` | Return the **registry's** enabled graphs (stable logical view), not a raw engine scan; `graphs` in a `single` store surface under their logical IRI |
| `load_from_slice` / `serialize` | Require an explicit graph; route accordingly; `None` means main |
| `transaction` | `Error::Unsupported`; use `admin.transaction_for(&graph)` for a single-store transaction |
| `len` | Sum of store lengths with `SERVICE`-free `quad_count` |

`OxigraphStore` gains one method so the router can evaluate with federation
without leaking the handler:

```rust
impl OxigraphStore {
    pub fn query_with_services(
        &self,
        sparql: &str,
        handler: LocalStoreServiceHandler,
    ) -> Result<QueryResult>;
}
```

`RdfStore::query` keeps its current behaviour (no handler). Only `MultiStore`
uses the new method.

### 8.5 Facade

```rust
pub struct GraphBackend {
    store: Arc<MultiStore>,
    admin: Arc<dyn GraphAdmin>,
    catalog: Arc<dyn GraphCatalog>,
}

impl GraphBackend {
    /// Single-store compatibility mode: one directory, no registry.
    pub fn single(config: &StorageConfig) -> Result<Self>;

    /// Registry mode: open main, sync `[graphs].registry`, open satellites.
    pub fn from_config(config: &Config) -> Result<Self>;

    pub fn rdf_store(&self) -> Arc<dyn RdfStore>;      // for ApplicationService
    pub fn admin(&self) -> Arc<dyn GraphAdmin>;       // for maintenance commands
    pub fn catalog(&self) -> Arc<dyn GraphCatalog>;   // for diagnostics
}
```

Consumers change in exactly one place:

```rust
let backend = GraphBackend::from_config(&config)?;
let service = ApplicationService::new(backend.rdf_store())
    .with_namespaces(config.namespace_registry()?);
```

`InstanceService`, `OwlManager`, `SkosManager`, `ClassificationService`,
`RuleInferenceEngine`, `EmbeddingService`, `ExternalOntologyManager`, and
`JobContext` continue to take `Arc<dyn RdfStore>` and need no edits.

### 8.6 Admin surface (later transport wiring)

The internal API is what HTTP/CLI/MCP will call, but this plan does not add new
routes. Planned commands, expressed as `RdfCommand`/`GraphCommand` variants in a
follow-up phase:

| Operation | CLI shape | API shape |
| --- | --- | --- |
| Registry status | `onto graph status` | `GET /api/v1/graphs/status` |
| Sync registry | `onto graph sync` | `POST /api/v1/graphs/sync` |
| Validate stores | `onto graph validate` | `POST /api/v1/graphs/validate` |
| Backup all stores | `onto graph backup --to DIR` | `POST /api/v1/graphs/backup` |
| Inspect one graph | `onto graph describe <iri>` | `GET /api/v1/graphs/{iri}` |

---

## 9. Consistency, Durability, and Backup

1. **Writer isolation** — one writable handle per directory. Main is the only
   long-lived writer; satellites are written only through `with_writable_store`,
   which quiesces readers first.
2. **Registry atomicity** — registry replacement is a single transaction in the
   main store. Either the new generation is fully visible or the previous one is.
3. **Read consistency** — each query sees one satellite snapshot per `SERVICE`
   call (the handler takes a snapshot), and the host's own snapshot for its
   branch. Cross-store reads are therefore *not* one global snapshot; this is
   documented and acceptable for read-mostly satellites. A query that needs a
   frozen view must run while no writes are in flight (`update_policy` +
   maintenance window).
4. **Backups** — `GraphAdmin::backup` iterates stores in `mountOrder`, calls
   `OxigraphStore::backup` into `<target>/<store>` (target must not exist), and
   writes `<target>/manifest.json` with `generation`, per-store
   `contentFingerprint`, and engine revision. Restore = stop the process, copy
   the directories back, keep the registry file, start; the marker check refuses
   a mismatched pairing.
5. **Failure of a satellite at startup** — aborts unless `onto:required false`,
   in which case the store is `Degraded` and its graphs read as empty.
6. **Failure mid-query** — a `SERVICE` error fails the query (as today); no
   partial results are returned.

---

## 10. Security and Validation

- Registry files are **trusted configuration**, but still validated: absolute
  IRIs only, no `urn:onto:system:` ownership outside main, no path escaping
  `[graphs].root`.
- Path handling uses canonicalised paths everywhere, including the
  `oxigraph://` IRIs emitted by the rewriter. Paths containing whitespace, `#`,
  `?`, `%`, or non-ASCII are rejected at registry load, because the handler
  treats the IRI's path substring literally (`PathBuf::from`) and would not
  decode percent-encoding.
- Satellite writes require `onto:access "rw"` **and** go through
  `with_writable_store`; the HTTP API cannot write a satellite in this phase.
- `SERVICE` rewriting only targets local `oxigraph://` IRIs derived from the
  registry — never a path supplied by a query string.
- Blank-node subjects are rejected on satellite writes; `onto:strictBlankNodes`
  additionally refuses fused evaluation for satellites containing them.
- Registry mutations through the API are rejected (§5.4), so the file remains
  the only source of truth.

---

## 11. Failure Modes

| Failure | Detection | Response |
| --- | --- | --- |
| Registry file missing/unparsable | startup sync | abort startup, `Error::GraphRegistry` with line/column |
| `storage.path` ≠ registry main path | validation | abort; message names both canonical paths |
| Two stores claim one graph or namespace | validation | abort, list all conflicts |
| Satellite directory missing | marker/open | abort, or `Degraded` when `onto:required false` |
| `graph.meta.json` mismatch | open | abort; protects against restored/moved directories |
| Second writer on a satellite | RocksDB `LOCK` | `with_writable_store` quiesces first; a foreign process is reported as `Error::GraphOpen` |
| Cross-store update under `strict` | update plan | reject before writing, list graphs/stores |
| Partial cross-store update under `split` | execution | `Error::GraphPartial { applied, failed, reason }` |
| Registry drift after manual edits | `onto graph status` | `contentFingerprint` mismatch is reported, not auto-repaired |
| Query references a `Degraded` store | plan | empty branch + `tracing::warn!`, counted in metrics |
| Query uses `FROM`/`FROM NAMED` | plan | `Error::Unsupported` with guidance to use `GRAPH` |
| Blank nodes cross stores | write validation | rejected |

---

## 12. Testing Strategy

Unit (`crates/ontology-graph/src/**/tests`):

1. Registry model extraction from the RDF/XML fixture, including anonymous and
   named registrations.
2. Validation: duplicate graph, duplicate namespace, reserved graph outside
   main, path escape, `single` with two registrations, bad registry version.
3. Namespace index: longest-prefix resolution, priority tie-break, unmapped
   fallback to main.
4. Rewriter golden tests (`insta`-style inline snapshots preferred; otherwise
   `assert_eq!` on normalised strings) for R1–R6, including `GRAPH ?g` union,
   nested `EXISTS`, and rejection of `FROM`.
5. Host election table, including the "all graphs in one satellite" case.
6. Update partitioning: data inserts/deletes, `WITH`, cross-store rejection,
   `split` partial failure.
7. Marker file: read/verify/mismatch.

Integration (`tests/tests/phase7_multigraph.rs`, using `tempfile`):

8. Two-store round trip: write to main, load a satellite fixture, `GET`
   both graphs through `MultiStore`, verify `graphs()` returns the logical set.
9. Cross-store join through the API-facing service: `ApplicationService` over a
   `MultiStore`, `GRAPH <asserted>` ⋈ `GRAPH <products>` returns joined rows.
10. Fused path is actually used for `layout = single` (assert no fallback) and the
    handler path for `layout = named`.
11. `SERVICE` results equal a single-store equivalent (load the same data into
    one store and compare sorted solutions).
12. Registry sync: add, change path of, and retire a satellite across
    generations; verify registry triples, `onto:generation`, and that retired
    handles are closed.
13. Degraded store: unregister directory with `onto:required false` → empty
    branch, no error; `required true` → startup error.
14. `with_writable_store`: write, flush, reopen read-only, query sees the data.
15. Backup/manifest: backup all stores, reopen the backup as a standalone main
    store, verify counts.

End-to-end:

16. Start the HTTP server with a two-store config; `POST /api/v1/sparql/query`
    with a cross-store join returns rows; `GET /api/v1/graphs` includes satellite
    graphs.
17. CLI `rdf graphs` and `rdf query` against that server.

Performance smoke (bench, not a gate): 5k-row join across two stores, compare
`layout = single` (fused) against `layout = named` (handler) to confirm the
expected order-of-magnitude difference and to catch regressions in routing.

---

## 13. Implementation Phases

### Phase 7.1 — Registry model and validation (no runtime behaviour change)

- `ontology-graph` skeleton, `RegistryModel`, RDF/XML parsing with base IRI,
  validation with aggregated errors.
- Fixtures under `crates/ontology-graph/tests/fixtures/`.
- Acceptance: `cargo test -p ontology-graph` covers §12.1–12.3; no other crate
  changes.

### Phase 7.2 — Catalog, pool, marker files

- `CatalogSnapshot`, namespace index, `StoreHandle`, open/close, marker
  read/write, `single`/`named` layout mapping.
- `GraphBackend::single` reproduces today's behaviour exactly.
- Acceptance: a main store plus two fixtures open, resolve graphs and
  namespaces, and report status; `single` mode passes the existing phase 1
  storage tests unchanged.

### Phase 7.3 — Sync into the main graph

- Diff + transactional registry persistence, generation, digests, provenance.
- Startup wiring in `ontology-api` and `ontology-mcp` (`GraphBackend::from_config`).
- Acceptance: `tests/tests/phase7_multigraph.rs` §12.8, §12.12, §12.13.

### Phase 7.4 — `MultiStore` and rewriting

- `get`/`insert`/`delete`/`graphs`/`load`/`serialize` routing; `query_with_services`
  on `OxigraphStore`; spargebra rewriting; plan cache; host election.
- Acceptance: §12.9–12.11; existing 168-test suite still green.

### Phase 7.5 — Update routing and write promotion

- Update partitioning, `strict`/`split`, `with_writable_store`, partial-failure
  error, blank-node rules.
- Acceptance: §12.14 plus update-routing units (§12.6).

### Phase 7.6 — Admin surface and operations

- `status`/`validate`/`backup`/`reload`, manifest, health reporting; HTTP and CLI
  wiring in a follow-up if desired.
- Acceptance: §12.15, §12.16, §12.17; `docs/operations.md` updated with the
  multi-store backup/restore procedure.

Each phase must leave `cargo test --workspace`, `cargo clippy --workspace`, and
`cargo fmt --all -- --check` clean.

---

## 14. Migration and Compatibility

1. **No registry, no change.** `[graphs]` absent → `SingleStore`, identical file
   layout, identical behaviour, identical tests.
2. **Adopting multi-store on an existing deployment.**
   - Keep `storage.path` as-is; it becomes the main directory.
   - Write `config/graphs.rdf` with the main store claiming every graph that
     currently exists in that directory (`GraphKind::ALL` plus any tenant
     graphs).
   - Start with `sync = "on_startup"`, `[graphs].root` empty; verify
     `onto graph status` lists exactly the pre-existing graphs.
   - Add satellites one at a time; each new store starts empty and is populated
     by an import (`load_from_slice` with an explicit graph).
3. **Rollback.** Delete `[graphs]`; the main store still contains every graph it
   ever held, because satellite adoption never moves existing main-store data.
   This is a deliberate constraint: the plan **does not** implement graph
   *migration* between stores (that needs a copy + delete + generation dance and
   is out of scope).
4. **Data already in `data:asserted`.** Unaffected: it stays owned by main.

---

## 15. Open Decisions

| # | Decision | Recommendation |
| --- | --- | --- |
| 1 | Registry authority: file vs. store | File authoritative, store is a materialised view (simplest invariants) |
| 2 | `single` vs `named` default for satellites | `named` (uniform semantics); `single` opt-in for read-only external datasets that need the fused path |
| 3 | Cross-store update policy default | `strict`; `split` available but requires explicit opt-in |
| 4 | Whether to add `onto:endpoint` (remote SPARQL) stores | Defer; the router's `SERVICE` machinery makes it feasible, but it changes the trust model |
| 5 | Registry write-back (`graph registry export`) | Defer until an operator workflow needs it |
| 6 | File watching for live registry reload | Defer; `graph sync` + restart is enough for the first release |

## 16. Risks

| Risk | Impact | Mitigation |
| --- | --- | --- |
| `basenote` fusion changes upstream | Rewriter assumptions (R3) break | Pin the fork revision; add a test that asserts a `single`-layout plan has no fallback reason, so a behaviour change fails loudly |
| Read-only satellite + concurrent writer | Undefined engine behaviour | Satellites are read-only while queries run; writes only via `with_writable_store` |
| Cross-store reads are not one snapshot | Skew in long reports | Document; recommend maintenance window for reports; per-query `SERVICE` snapshot is consistent |
| Namespace ownership collisions across teams | Wrong-store writes | Fail-closed validation; exclusivity is enforced at sync, not at write time |
| Blank nodes silently collide in fused joins | Wrong results | Reject bnodes on satellite writes; strict flag for legacy data |
| Plan cache staleness | Stale routing | Cache keyed by generation; sync clears it |
| Query rewrite bugs | Incorrect results | Golden tests per rule + equivalence tests against a single-store dataset; reject constructs that cannot be routed (`FROM`) instead of guessing |
| Directory proliferation (one store per graph) | Many open RocksDB handles, fd pressure | `max_open_files` per store, `mountOrder`, and a documented recommendation to group graphs per store; `status` reports handle counts |

---

## 17. Appendix — Plan Shape

For a two-store deployment, one query flows as follows:

```
MultiStore::query(text)
  ├─ cache.lookup(text, generation) -> miss
  ├─ spargebra parse
  ├─ collect referenced graphs: {urn:onto:data:asserted, urn:onto:data:products}
  ├─ elect host: main (main is referenced)
  ├─ rewrite:
  │    asserted   -> kept (host)
  │    products   -> SERVICE <oxigraph:///srv/onto/data/graphs/products> { GRAPH <…products> { … } }
  ├─ cache.insert
  ├─ main.handle.query_with_services(rewritten, shared_handler)
  │    └─ LocalStoreServiceHandler: open_read_only(/srv/onto/data/graphs/products), cached
  │         ├─ fusion guard: inner GRAPH  -> not fused -> handler path
  │         └─ evaluate sub-pattern on satellite snapshot
  └─ convert_query_results -> QueryResult
```

With `layout = "single"` on the satellite, the rewrite emits
`SERVICE <oxigraph://…> { … }` with no inner `GRAPH`; `fuse_query` then rewrites
it to a virtual `GRAPH` over the fused dataset and the join runs without the
service boundary — the same query, one engine, two directories.
