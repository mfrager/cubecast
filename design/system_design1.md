# RDF Ontology Management System — Expanded Technical Design

Rust · Oxigraph · RDF/OWL · SKOS · Semantic Classification · Embeddings · Async HTTP · CLI · MCP

## 1. System Architecture

Build a modular Rust knowledge management engine with Oxigraph as its native RDF storage and SPARQL execution layer. Expose the complete functionality through an asynchronous HTTP API, a full-featured CLI, and a compact MCP interface designed for LLM agents.

Core design principle: One ontology engine, three interfaces, shared capabilities.

```
                  LLM / AI AGENTS
                        |
                       MCP
                 Compact Tool Set
                        |
CLI -------------- HTTP API -------------- Applications
                        |
                 Async API Engine
                        |
                 Application Core
                        |
       +----------------+----------------+
       |                |                |
   RDF Engine       OWL / SKOS       Semantic Engine
       |                |                |
   Oxigraph         Taxonomies       Embeddings
       |                |                |
       +----------------+----------------+
                        |
               Inference / Enrichment
                        |
               Storage & Indexes
```

### Architectural layers

1. RDF Storage: Oxigraph-backed triple and named-graph management.
2. Ontology Engine: OWL classes, properties, constraints, and schema versions.
3. Taxonomy Engine: SKOS ConceptSchemes, concepts, labels, and hierarchical relationships.
4. Classification Engine: Rule-based, lexical, and semantic instance classification.
5. Import Engine: RDF standards, external ontologies, structured data, and bulk ingestion.
6. Embedding Engine: Attribute-level vectorization and semantic similarity.
7. Inference Engine: Automated class assignment, property application, and derived statements.
8. Application Engine: Transactions, orchestration, validation, provenance, and job execution.
9. Interface Layer: Async HTTP, CLI, and MCP.

All interfaces call the same application services. Neither the CLI nor MCP should implement independent business logic.

## 2. RDF Storage — Oxigraph

Use Oxigraph as the primary RDF implementation.

Oxigraph provides the standards-based RDF foundation rather than requiring a custom triple store.

### Storage responsibilities

- RDF triple and quad storage.
- Named graphs and graph-level operations.
- SPARQL 1.1 querying and updates.
- RDF serialization and parsing.
- Persistent and in-memory storage modes.
- RDF dataset management.
- Transactional updates.
- RDF term and datatype handling.

### Dataset organization

Use named graphs to separate different classes of information:

| Graph                 | Purpose                           |
| --------------------- | --------------------------------- |
| `system:schema`       | Internal OWL schema               |
| `system:taxonomy`     | Managed SKOS ConceptSchemes       |
| `system:external`     | Imported external ontologies      |
| `data:asserted`       | Explicit instance statements      |
| `data:inferred`       | Automatically inferred statements |
| `data:classification` | Taxonomy classification results   |
| `data:provenance`     | Source and derivation metadata    |

Use tenant, workspace, or project graph namespaces where isolation is required.

### Important storage distinction

Oxigraph handles RDF storage and SPARQL, but it is not a complete OWL reasoning engine or vector database.

Implement these capabilities in separate application services:

- OWL schema validation and selected reasoning.
- SKOS taxonomy operations.
- Semantic classification.
- Embedding generation.
- Vector similarity search.
- External ontology mapping.

This keeps RDF storage standards-compliant while allowing specialized processing.

## 3. Rust Workspace and Crate Organization

Use a Cargo workspace with independently testable modules and a shared application layer.

```
ontology/
├── Cargo.toml
├── crates/
│   ├── ontology-core/
│   ├── ontology-rdf/
│   ├── ontology-owl/
│   ├── ontology-skos/
│   ├── ontology-classify/
│   ├── ontology-embedding/
│   ├── ontology-import/
│   ├── ontology-inference/
│   ├── ontology-application/
│   ├── ontology-api/
│   ├── ontology-cli/
│   └── ontology-mcp/
├── migrations/
├── schemas/
├── ontologies/
├── tests/
└── config/
```

Module responsibilities:

| Crate                  | Responsibility                                  |
| ---------------------- | ----------------------------------------------- |
| `ontology-core`        | Shared domain types, IDs, errors, configuration |
| `ontology-rdf`         | Oxigraph adapter, datasets, triples, SPARQL     |
| `ontology-owl`         | OWL schema, classes, properties, validation     |
| `ontology-skos`        | ConceptSchemes, concepts, taxonomy              |
| `ontology-classify`    | Matching, scoring, classification               |
| `ontology-embedding`   | Embedding models, vectors, similarity           |
| `ontology-import`      | RDF, OWL, structured data ingestion             |
| `ontology-inference`   | Class/property inference and enrichment         |
| `ontology-application` | Application services and orchestration          |
| `ontology-api`         | Async HTTP server                               |
| `ontology-cli`         | HTTP API client and interactive CLI             |
| `ontology-mcp`         | MCP server and tool definitions                 |

Keep the domain crates independent of HTTP and CLI dependencies.

## 4. Rust Trait Architecture

Define traits around capabilities rather than implementation details. This makes storage, embedding, classification, and reasoning implementations replaceable.

Use `async_trait` for dynamically dispatched asynchronous services. Keep Oxigraph-specific storage operations behind an adapter.

### 4.1 Core RDF traits

```
pub trait RdfStore: Send + Sync {
    fn get(&self, pattern: QuadPattern)
        -> Result<Vec<Quad>, RdfError>;

    fn insert(&self, quads: &[Quad])
        -> Result<(), RdfError>;

    fn delete(&self, quads: &[Quad])
        -> Result<(), RdfError>;

    fn query(&self, sparql: &str)
        -> Result<QueryResult, RdfError>;

    fn update(&self, sparql: &str)
        -> Result<(), RdfError>;

    fn graphs(&self)
        -> Result<Vec<GraphId>, RdfError>;
}
```

Implement as `OxigraphStore`, wrapping Oxigraph's `Store`.

Keep the underlying Oxigraph types inside the storage crate wherever possible. Expose application-level RDF types so the rest of the system isn't tightly coupled to Oxigraph.

### 4.2 Transaction trait

```
pub trait RdfTransaction: Send {
    fn insert(&mut self, quads: &[Quad])
        -> Result<(), RdfError>;

    fn delete(&mut self, quads: &[Quad])
        -> Result<(), RdfError>;

    fn commit(self)
        -> Result<(), RdfError>;
}
```

Add transaction creation to `RdfStore` when the implementation supports it.

Use Oxigraph transactions for atomic operations. For large imports, use its bulk-loading facilities rather than accumulating millions of changes in a transaction. Oxigraph documents repeatable-read isolation and explicitly cautions that transaction changes are held in memory.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://docs.rs\&sz=32)

Rust+1



### 4.3 OWL ontology traits

```
pub trait OntologyManager: Send + Sync {
    async fn create_class(&self, class: OwlClass)
        -> Result<Id, Error>;

    async fn create_property(&self, property: OwlProperty)
        -> Result<Id, Error>;

    async fn get_class(&self, id: &Id)
        -> Result<OwlClass, Error>;

    async fn get_property(&self, id: &Id)
        -> Result<OwlProperty, Error>;

    async fn validate(&self, scope: GraphScope)
        -> Result<ValidationReport, Error>;

    async fn hierarchy(&self, class: &Id)
        -> Result<ClassHierarchy, Error>;
}
```

Support:

- `owl:Class`, `owl:ObjectProperty`, `owl:DatatypeProperty`.
- Subclass and subproperty relationships.
- Domains, ranges, restrictions, and equivalent classes.
- Disjointness and property characteristics.
- Cardinality and datatype constraints.
- Schema versioning and change tracking.

### 4.4 SKOS traits

```
pub trait TaxonomyManager: Send + Sync {
    async fn create_scheme(&self, scheme: ConceptScheme)
        -> Result<Id, Error>;

    async fn create_concept(&self, concept: Concept)
        -> Result<Id, Error>;

    async fn add_relation(
        &self,
        source: &Id,
        relation: SkosRelation,
        target: &Id,
    ) -> Result<(), Error>;

    async fn get_concept(&self, id: &Id)
        -> Result<Concept, Error>;

    async fn descendants(&self, id: &Id)
        -> Result<Vec<Concept>, Error>;

    async fn search(&self, query: &str)
        -> Result<Vec<ConceptMatch>, Error>;
}
```

Support:

- Multiple aggregated ConceptSchemes.
- `skos:Concept`, `skos:Collection`, `skos:ConceptScheme`.
- `skos:broader`, `skos:narrower`, `skos:related`.
- Preferred, alternative, and hidden labels.
- Language-aware labels.
- Concept mappings between schemes.
- Taxonomy merging and synchronization.
- Cycle and hierarchy validation.

### 4.5 Classification traits

```
#[async_trait]
pub trait Classifier: Send + Sync {
    async fn classify(
        &self,
        instance: &Instance,
        options: ClassificationOptions,
    ) -> Result<Vec<Classification>, Error>;

    async fn classify_batch(
        &self,
        instances: &[InstanceId],
        options: ClassificationOptions,
    ) -> Result<ClassificationBatch, Error>;
}
```

Implement interchangeable classification strategies:

- Exact label matching.
- Normalized text matching.
- Token and keyword matching.
- Fuzzy matching.
- Attribute-based rules.
- Embedding similarity.
- Hybrid weighted scoring.

Every classification should retain:

- Concept identifier.
- Confidence score.
- Matching attributes.
- Matching strategy.
- Taxonomy version.
- Timestamp and provenance.
- Manual override status.

### 4.6 Embedding traits

```
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, texts: &[String])
        -> Result<Vec<Embedding>, Error>;

    fn dimensions(&self) -> usize;

    fn model_id(&self) -> &str;
}

#[async_trait]
pub trait VectorIndex: Send + Sync {
    async fn upsert(&self, items: &[VectorRecord])
        -> Result<(), Error>;

    async fn search(
        &self,
        vector: &[f32],
        limit: usize,
        filter: Option<VectorFilter>,
    ) -> Result<Vec<VectorMatch>, Error>;

    async fn delete(&self, ids: &[Id])
        -> Result<(), Error>;
}
```

Support multiple providers, model versions, dimensions, similarity metrics, and vector backends.

### 4.7 Import traits

```
#[async_trait]
pub trait DataImporter: Send + Sync {
    async fn inspect(
        &self,
        source: ImportSource,
    ) -> Result<ImportPreview, Error>;

    async fn import(
        &self,
        source: ImportSource,
        options: ImportOptions,
    ) -> Result<ImportReport, Error>;
}
```

Separate format parsers from import orchestration.

### 4.8 External ontology traits

```
#[async_trait]
pub trait OntologyResolver: Send + Sync {
    async fn resolve(
        &self,
        uri: &str,
    ) -> Result<ExternalOntology, Error>;

    async fn import(
        &self,
        ontology: ExternalOntology,
        options: OntologyImportOptions,
    ) -> Result<ImportReport, Error>;

    async fn map_class(
        &self,
        external: &Id,
        internal: &Id,
    ) -> Result<(), Error>;

    async fn map_property(
        &self,
        external: &Id,
        internal: &Id,
    ) -> Result<(), Error>;
}
```

Preserve external IRIs. Maintain explicit mapping records rather than rewriting external ontology identifiers.

### 4.9 Inference and enrichment traits

```
#[async_trait]
pub trait InferenceEngine: Send + Sync {
    async fn infer_classes(
        &self,
        instance: &InstanceId,
    ) -> Result<Vec<InferredClass>, Error>;

    async fn infer_properties(
        &self,
        instance: &InstanceId,
    ) -> Result<Vec<InferredProperty>, Error>;

    async fn apply(
        &self,
        instance: &InstanceId,
        policy: ApplyPolicy,
    ) -> Result<InferenceReport, Error>;
}
```

Keep inference separate from persistence. Inferred results should first be generated and validated, then explicitly applied according to policy.

### 4.10 Async application service trait

```
#[async_trait]
pub trait OntologyService: Send + Sync {
    async fn execute(
        &self,
        command: Command,
    ) -> Result<CommandResult, Error>;
}
```

This becomes the shared execution boundary for HTTP, CLI, and MCP.

Trait design rule: Prefer small capability-specific traits and compose them in application services. Avoid one enormous `OntologyEngine` trait containing every operation.

## 5. Data Model and Processing Rules

### Primary entities

- `Instance`
- `Statement`
- `Graph`
- `Ontology`
- `OntologyVersion`
- `OwlClass`
- `OwlProperty`
- `ConceptScheme`
- `Concept`
- `TaxonomyRelation`
- `ExternalOntology`
- `ClassMapping`
- `PropertyMapping`
- `Classification`
- `Embedding`
- `Inference`
- `ImportJob`
- `ProcessingJob`
- `ProvenanceRecord`

### Statement lifecycle

Maintain three distinct categories:

1. Asserted: Explicitly provided or imported RDF statements.
2. Inferred: Statements generated by reasoning or classification.
3. Derived: Embeddings, classifications, scores, and other computed artifacts.

Never silently overwrite asserted statements with inferred data.

### Automatic external schema application

Implement the following process:

1. Discover external ontologies and imported classes.
2. Match instance attributes to external properties.
3. Evaluate class requirements and property domains.
4. Identify applicable classes.
5. Evaluate inherited properties.
6. Validate the proposed statements.
7. Generate an application preview.
8. Apply accepted statements to the designated graph.
9. Record provenance and ontology version.

Use explicit policies for automatic application:

- `suggest`: Generate recommendations only.
- `review`: Require approval.
- `automatic`: Apply above a configured confidence threshold.

Do not confuse OWL entailment with application-specific heuristics. Track their provenance and reasoning method separately.

## 6. Async HTTP API

Use Tokio + Axum + Serde for the HTTP service.

Recommended dependencies:

- `tokio` — asynchronous runtime.
- `axum` — HTTP routing and middleware.
- `tower` / `tower-http` — middleware, tracing, CORS.
- `serde` / `serde_json` — serialization.
- `utoipa` — OpenAPI generation.
- `reqwest` — HTTP client.
- `tracing` — structured logging.
- `uuid` or ULID implementation — identifiers.
- `thiserror` — domain errors.

### 6.1 API design principles

- RESTful resource management.
- JSON request/response format.
- RDF-native content negotiation.
- Asynchronous long-running operations.
- Consistent pagination.
- Structured validation errors.
- OpenAPI documentation.
- Versioned endpoints (`/api/v1`).
- Streaming upload and export.
- Job tracking for bulk operations.
- Idempotency keys for repeatable writes.

### 6.2 Endpoint structure

| Endpoint                            | Operations                             |
| ----------------------------------- | -------------------------------------- |
| `/api/v1/instances`                 | Create, list, retrieve, update, delete |
| `/api/v1/instances/{id}/statements` | Statement CRUD                         |
| `/api/v1/graphs`                    | Named graph management                 |
| `/api/v1/sparql/query`              | SPARQL queries                         |
| `/api/v1/sparql/update`             | SPARQL updates                         |
| `/api/v1/ontologies`                | Ontology management                    |
| `/api/v1/classes`                   | OWL class management                   |
| `/api/v1/properties`                | OWL property management                |
| `/api/v1/schemas/validate`          | Schema validation                      |
| `/api/v1/schemes`                   | SKOS ConceptScheme management          |
| `/api/v1/concepts`                  | Concept CRUD and search                |
| `/api/v1/taxonomies`                | Taxonomy hierarchy operations          |
| `/api/v1/classifications`           | Instance classification                |
| `/api/v1/embeddings`                | Embedding generation and retrieval     |
| `/api/v1/similarity`                | Vector similarity search               |
| `/api/v1/external-ontologies`       | External ontology registry             |
| `/api/v1/mappings`                  | External class/property mappings       |
| `/api/v1/inference`                 | Inference and schema application       |
| `/api/v1/imports`                   | Bulk data and ontology imports         |
| `/api/v1/exports`                   | RDF dataset exports                    |
| `/api/v1/jobs`                      | Background job management              |
| `/api/v1/system`                    | Health, status, configuration          |

### 6.3 Instance API

Example:

```
POST /api/v1/instances
Content-Type: application/json
```

```
{
  "id": "urn:example:product:123",
  "types": ["ex:Product"],
  "properties": {
    "ex:name": "Example Product",
    "ex:description": "Product description"
  },
  "graph": "data:asserted"
}
```

Statement-level operations:

```
GET    /instances/{id}/statements
POST   /instances/{id}/statements
PUT    /instances/{id}/statements/{statement_id}
DELETE /instances/{id}/statements/{statement_id}
```

Provide batch statement operations to avoid excessive HTTP round trips.

### 6.4 SPARQL API

```
POST /api/v1/sparql/query
Content-Type: application/sparql-query
```

Accept standard SPARQL query syntax.

Return:

- JSON solutions.
- Boolean results.
- RDF graph results.
- CSV / TSV results.

Also expose SPARQL Update separately with appropriate authorization.

### 6.5 Taxonomy API

```
POST /schemes
POST /concepts
GET  /concepts/{id}
GET  /concepts/{id}/broader
GET  /concepts/{id}/narrower
GET  /concepts/{id}/related
GET  /taxonomies/{id}/tree
POST /concepts/search
POST /taxonomies/merge
```

Support concept movement, hierarchy changes, cross-scheme mapping, and taxonomy versioning.

### 6.6 Classification API

```
POST /classifications/run
POST /classifications/batch
GET  /classifications/{id}
POST /classifications/{id}/approve
POST /classifications/{id}/reject
POST /classifications/{id}/apply
```

Allow configuration of:

- Matching strategies.
- Similarity thresholds.
- Target ConceptSchemes.
- Instance attributes.
- Maximum classifications per instance.
- Hierarchical expansion.
- Automatic application policy.

### 6.7 Embedding API

```
POST   /embeddings/generate
POST   /embeddings/batch
GET    /embeddings/{instance_id}
DELETE /embeddings/{instance_id}
POST   /similarity/search
```

Allow model selection and attribute selection.

### 6.8 External ontology API

```
POST /external-ontologies/import
GET  /external-ontologies
GET  /external-ontologies/{id}
POST /external-ontologies/{id}/sync
POST /mappings/classes
POST /mappings/properties
GET  /mappings
POST /inference/preview
POST /inference/apply
```

### 6.9 Asynchronous job API

Bulk imports, classification, embedding generation, and inference should be executable as background jobs.

```
POST   /api/v1/jobs
GET    /api/v1/jobs/{id}
GET    /api/v1/jobs/{id}/results
POST   /api/v1/jobs/{id}/cancel
DELETE /api/v1/jobs/{id}
```

Job states:

`queued → running → completed | failed | cancelled`

Include progress, counters, errors, timestamps, and execution parameters.

Use bounded worker pools and configurable concurrency.

### 6.10 Async execution and Oxigraph

Important implementation consideration: Oxigraph's `Store` APIs are synchronous. Do not execute substantial storage operations directly on Tokio's asynchronous executor.

Use:

- `Arc<Store>` for shared storage access.
- `tokio::task::spawn_blocking` for blocking operations.
- A bounded semaphore to limit concurrent blocking work.
- Streaming interfaces for large imports and exports.
- Dedicated workers for CPU-intensive embedding and classification.

The HTTP API remains asynchronous even though the underlying RDF engine is synchronous.

## 7. Full-Featured CLI

Build a CLI named `onto`, using `clap` and `reqwest`.

The CLI should be a complete remote administration client for the HTTP API, not a separate RDF engine.

### 7.1 Command hierarchy

```
onto
├── server
│   ├── status
│   ├── health
│   └── config
│
├── instance
│   ├── create
│   ├── get
│   ├── list
│   ├── update
│   ├── delete
│   ├── statements
│   └── classify
│
├── rdf
│   ├── query
│   ├── update
│   ├── insert
│   ├── delete
│   ├── import
│   ├── export
│   └── graphs
│
├── ontology
│   ├── list
│   ├── get
│   ├── import
│   ├── sync
│   ├── validate
│   └── versions
│
├── class
│   ├── create
│   ├── get
│   ├── list
│   ├── update
│   ├── delete
│   └── hierarchy
│
├── property
│   ├── create
│   ├── get
│   ├── list
│   ├── update
│   └── delete
│
├── scheme
│   ├── create
│   ├── list
│   ├── get
│   ├── update
│   ├── delete
│   └── merge
│
├── concept
│   ├── create
│   ├── get
│   ├── search
│   ├── update
│   ├── delete
│   ├── move
│   └── relations
│
├── taxonomy
│   ├── tree
│   ├── descendants
│   ├── ancestors
│   ├── validate
│   ├── classify
│   └── merge
│
├── classify
│   ├── instance
│   ├── batch
│   ├── preview
│   ├── approve
│   ├── reject
│   └── apply
│
├── embedding
│   ├── generate
│   ├── batch
│   ├── search
│   ├── inspect
│   └── delete
│
├── external
│   ├── search
│   ├── import
│   ├── list
│   ├── inspect
│   ├── sync
│   └── mappings
│
├── inference
│   ├── preview
│   ├── classes
│   ├── properties
│   ├── apply
│   └── explain
│
├── import
│   ├── file
│   ├── directory
│   ├── url
│   ├── csv
│   └── json
│
├── export
│   ├── graph
│   ├── instance
│   ├── ontology
│   └── dataset
│
└── job
    ├── list
    ├── get
    ├── results
    ├── watch
    └── cancel
```

### 7.2 CLI functionality

Support:

- Configurable API URL and authentication.
- Environment variables and configuration files.
- Named server profiles.
- JSON, YAML, table, and plain-text output.
- Machine-readable output for shell pipelines.
- Input from stdin.
- File-based request payloads.
- RDF format autodetection.
- Pagination and filtering.
- Progress indicators.
- Interactive confirmation for destructive operations.
- Dry-run and preview modes.
- Batch execution.
- Job monitoring.
- Exit codes suitable for automation.

### 7.3 Examples

```
# Connect to an API
onto config set endpoint http://localhost:8080

# Create an RDF instance
onto instance create --type ex:Product \
  --property ex:name="Example"

# Query RDF
onto rdf query --file query.rq

# Import RDF
onto import file products.ttl --format turtle

# Import external ontology
onto external import https://example.org/schema.ttl

# Create a taxonomy
onto scheme create Products

# Search concepts
onto concept search "network storage"

# Classify instances
onto classify batch --graph data:asserted

# Generate embeddings
onto embedding batch --attributes ex:name,ex:description

# Preview inferred properties
onto inference preview --instance urn:example:product:123

# Apply inferred properties
onto inference apply --instance urn:example:product:123

# Export dataset
onto export dataset --format trig --output dataset.trig

# Monitor background jobs
onto job watch <job-id>
```

### 7.4 Interactive mode

Optionally provide:

```
onto shell
```

A persistent interactive session with:

- Command history.
- Autocomplete.
- API connection context.
- Prefix management.
- SPARQL multiline input.
- Table rendering.
- Job monitoring.

This is an additional CLI convenience, not a separate execution interface.

## 8. Compact MCP Tool Interface

Recommended: 4 top-level tools

The MCP server should expose nearly all application functionality through four broad, semantically organized tools.

The objective is to avoid dozens of narrowly scoped MCP tools while retaining discoverability, structured inputs, and precise operations.

Use the HTTP application's service layer directly, or an internal API client if MCP runs as a separate process.

### Tool 1: `rdf`

Purpose: Manage and query RDF datasets and instances.

Operations:

| Action          | Capabilities                                      |
| --------------- | ------------------------------------------------- |
| `get`           | Retrieve instances, statements, graphs            |
| `search`        | Search instances and RDF resources                |
| `create`        | Create instances and statements                   |
| `update`        | Modify statements and metadata                    |
| `delete`        | Delete statements, instances, graphs              |
| `query`         | Execute SPARQL SELECT, ASK, CONSTRUCT, DESCRIBE   |
| `sparql_update` | Execute SPARQL Update                             |
| `import`        | Import RDF and structured data                    |
| `export`        | Export RDF datasets and resources                 |
| `inspect`       | Inspect graph contents and resource relationships |

### Tool 2: `ontology`

Purpose: Manage OWL schemas, SKOS taxonomies, and external ontology integrations.

Operations:

| Action            | Capabilities                                               |
| ----------------- | ---------------------------------------------------------- |
| `schema`          | Create, inspect, update, delete OWL classes and properties |
| `validate`        | Validate schema and instance conformance                   |
| `scheme`          | Manage ConceptSchemes                                      |
| `concept`         | Create, retrieve, update, delete concepts                  |
| `taxonomy`        | Manage hierarchies, relations, and taxonomy trees          |
| `search`          | Search classes, properties, concepts, and schemes          |
| `external_import` | Import external RDF/OWL projects                           |
| `external_sync`   | Synchronize imported ontologies                            |
| `mapping`         | Map external classes and properties                        |
| `merge`           | Merge taxonomies or ontology definitions                   |

### Tool 3: `semantic`

Purpose: Classification, embeddings, reasoning, and automated enrichment.

Operations:

| Action           | Capabilities                               |
| ---------------- | ------------------------------------------ |
| `classify`       | Classify instances against SKOS schemes    |
| `classify_batch` | Batch classification                       |
| `embed`          | Generate instance and attribute embeddings |
| `similarity`     | Semantic similarity search                 |
| `infer`          | Infer applicable classes and properties    |
| `preview`        | Preview proposed enrichment                |
| `apply`          | Apply approved inferred statements         |
| `explain`        | Explain classifications and inferences     |
| `reclassify`     | Recompute classifications                  |
| `reembed`        | Regenerate embeddings                      |

### Tool 4: `jobs`

Purpose: Manage long-running and asynchronous operations.

Operations:

| Action    | Capabilities                                                  |
| --------- | ------------------------------------------------------------- |
| `submit`  | Submit imports, exports, classification, embedding, inference |
| `list`    | List jobs                                                     |
| `get`     | Retrieve job status and progress                              |
| `results` | Retrieve job outputs and errors                               |
| `cancel`  | Cancel running jobs                                           |
| `retry`   | Retry eligible failed jobs                                    |

### 8.1 MCP input design

Each tool should use a common envelope:

```
{
  "action": "classify",
  "target": "urn:example:product:123",
  "scope": {
    "graph": "data:asserted",
    "scheme": "ex:ProductCategories"
  },
  "options": {
    "strategy": "hybrid",
    "threshold": 0.82,
    "limit": 5
  }
}
```

Use action-specific JSON Schema validation for the individual operations.

Avoid a completely unrestricted generic `execute` tool that accepts arbitrary code or arbitrary internal function names.

### 8.2 MCP response design

Return compact, structured results:

```
{
  "success": true,
  "action": "classify",
  "target": "urn:example:product:123",
  "results": [
    {
      "concept": "ex:NetworkingEquipment",
      "confidence": 0.94,
      "method": "hybrid"
    }
  ],
  "metadata": {
    "scheme": "ex:ProductCategories",
    "count": 1
  }
}
```

For large outputs, return:

- A concise summary.
- Result count.
- A paginated result handle or job ID.
- An optional explanation.
- Relevant provenance.

### 8.3 MCP design rules

1. Four tools, not dozens.
2. Stable tool names and predictable input schemas.
3. Action enums with explicit valid operations.
4. Structured parameters rather than natural-language command strings.
5. Consistent output schemas.
6. Explicit graph and ontology scope.
7. Pagination and result limits.
8. Dry-run support for destructive or consequential changes.
9. Approval policies for automated enrichment.
10. Job submission for expensive operations.

Use MCP tool annotations to distinguish read-only operations from destructive or externally consequential operations. Keep human approval available for sensitive writes. MCP tools use JSON Schema for their input definitions and expose invocation through `tools/call`.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

GitHub+1



Why four tools? They correspond to four natural operational domains: graph data, ontology structure, semantic processing, and background execution. An LLM can perform complex workflows without needing to discover and select from a large collection of individual tools.

## 9. API, CLI, and MCP Shared Execution Model

All three interfaces should translate requests into a common set of application commands.

```
HTTP Request ─┐
              │
CLI Request ──┼──> Command Router
              │         |
MCP Request ──┘         |
                        v
                Application Services
                        |
                 Domain Operations
                        |
             +----------+----------+
             |          |          |
           RDF       Ontology    Semantic
             |          |          |
             +----------+----------+
                        |
                   Oxigraph
```

Define a shared command model:

```
pub enum Command {
    Rdf(RdfCommand),
    Ontology(OntologyCommand),
    Semantic(SemanticCommand),
    Job(JobCommand),
}

pub enum CommandResult {
    Resource(ResourceResult),
    Collection(CollectionResult),
    Query(QueryResult),
    Report(OperationReport),
    Job(JobStatus),
}
```

This is particularly useful for MCP: the four tools become adapters over four command families.

The CLI and HTTP API can expose more granular operations without increasing the number of application-level capabilities.

## 10. Cross-Cutting Infrastructure

### 10.1 Configuration

Centralized configuration for:

- Oxigraph database location.
- Named graph conventions.
- Namespace and prefix registry.
- External ontology sources.
- Embedding providers and models.
- Vector storage.
- Classification policies.
- Inference policies.
- HTTP server settings.
- Authentication and authorization.
- Background worker concurrency.
- Logging and tracing.

### 10.2 Namespace management

Maintain a global prefix registry.

Examples:

```
rdf:
rdfs:
owl:
xsd:
skos:
dcterms:
schema:
foaf:
prov:
sh:
```

Support custom namespaces, aliases, and project-specific prefixes.

All interfaces should accept prefixed identifiers and resolve them into full IRIs.

### 10.3 Validation

Implement validation at multiple levels:

- RDF structural validity.
- OWL schema consistency.
- SKOS hierarchy consistency.
- Datatype validation.
- Domain/range validation.
- External ontology compatibility.
- Instance classification validity.
- Inference validation.

Consider optional SHACL support for declarative graph constraints. Keep SHACL validation distinct from OWL reasoning.

### 10.4 Provenance and versioning

Use RDF provenance metadata and dedicated graph records.

Track:

- Source of imported statements.
- Ontology version.
- Classification model and strategy.
- Embedding model and version.
- Inference rules and execution.
- Manual modifications.
- Timestamps and operation IDs.

Provide reproducible processing by retaining configuration and model versions.

### 10.5 Security

- API key or JWT authentication.
- Role-based permissions.
- Graph-level access controls.
- Read/write separation.
- Restricted SPARQL Update access.
- External URL import restrictions.
- Configurable limits on query execution and imports.
- Audit logs.
- Explicit approval for automated schema application.

Treat external ontology documents and imported RDF as untrusted data.

## 11. Recommended Technology Stack

| Area            | Technology                |
| --------------- | ------------------------- |
| Language        | Rust                      |
| RDF storage     | Oxigraph                  |
| RDF model       | OxRDF                     |
| RDF parsing     | OxRDF I/O                 |
| Query language  | SPARQL 1.1                |
| Schema          | OWL                       |
| Taxonomy        | SKOS                      |
| Constraints     | SHACL                     |
| Async runtime   | Tokio                     |
| HTTP            | Axum                      |
| Serialization   | Serde                     |
| CLI             | Clap                      |
| HTTP client     | Reqwest                   |
| MCP             | Rust MCP SDK              |
| OpenAPI         | Utoipa                    |
| Logging         | Tracing                   |
| Background jobs | Tokio workers             |
| Embeddings      | Pluggable model providers |
| Vector storage  | Pluggable vector index    |
| Identifiers     | ULID                      |
| Error handling  | Thiserror                 |

Oxigraph currently provides the core `Store`, RDF model, parsing/serialization, SPARQL execution, and bulk-loading functionality. Its current documented release is 0.5.13; pin a tested version rather than using an unrestricted dependency version.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://docs.rs\&sz=32)

Rust+2



## 12. Implementation Roadmap

Build incrementally, ensuring every phase produces a usable system.

| Phase | Deliverable                                  |
| ----- | -------------------------------------------- |
| 1     | Rust workspace, core types, Oxigraph storage |
| 2     | RDF CRUD, named graphs, SPARQL               |
| 3     | OWL schema management and validation         |
| 4     | SKOS ConceptSchemes and taxonomy management  |
| 5     | RDF import/export and bulk ingestion         |
| 6     | External ontology registry and imports       |
| 7     | Classification and taxonomy matching         |
| 8     | Embedding providers and vector search        |
| 9     | Inference and automatic schema application   |
| 10    | Async HTTP API and OpenAPI                   |
| 11    | Full-featured CLI                            |
| 12    | Four-tool MCP server                         |
| 13    | Integration testing, optimization, security  |

### Final architectural priorities

1. Oxigraph is the authoritative RDF storage layer.
2. All domain capabilities operate on RDF rather than maintaining duplicate entity databases.
3. OWL and SKOS are first-class management subsystems.
4. Classification and inference are separate, composable services.
5. External ontologies retain their original IRIs and provenance.
6. Embeddings are derived data, not replacements for RDF.
7. HTTP, CLI, and MCP share one application execution engine.
8. Expensive operations are asynchronous and job-based.
9. MCP exposes broad capabilities through four compact tools.
10. Every automated modification is traceable and controllable.

End goal: A self-contained Rust ontology management platform that serves as an extensible semantic knowledge engine, combining RDF storage, ontology administration, taxonomy management, automated classification, semantic search, external schema integration, and agent-accessible operations.

The key architectural decision is to make the application service layer the central abstraction. Oxigraph supplies the RDF foundation, while OWL, SKOS, classification, embeddings, and inference become composable capabilities accessible through a unified API.
