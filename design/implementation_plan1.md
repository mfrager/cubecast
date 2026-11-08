# RDF Ontology Management System — 5-Phase Implementation Plan

Technology: Rust · Oxigraph · OWL · SKOS · Tokio · Axum · Clap · MCP

Architecture: Modular Rust workspace, Oxigraph-backed RDF storage, shared application services, asynchronous HTTP API, full-featured CLI, and four-tool MCP interface.

The five phases are organized around functional milestones. Each phase produces a usable, testable system and establishes the foundation for the next.

# Phase 1 — RDF Core & Storage Foundation

Foundation

Objective: Build the Rust workspace, RDF storage engine, data model, and foundational ontology infrastructure.

### 1.1 Workspace and infrastructure

- Establish Cargo workspace and crate organization.
- Define shared domain types, errors, identifiers, and configuration.
- Establish namespace and prefix management.
- Implement structured logging and tracing.
- Establish unit, integration, and benchmark testing.
- Define internal serialization conventions.

Crates:

- `ontology-core`
- `ontology-rdf`

### 1.2 Oxigraph integration

Implement the Oxigraph storage adapter.

- Persistent and in-memory storage.
- RDF triple and quad operations.
- Named graph management.
- RDF term and literal handling.
- SPARQL SELECT, ASK, CONSTRUCT, DESCRIBE.
- SPARQL Update.
- RDF serialization and parsing.
- Transaction management.
- Bulk loading.
- Graph enumeration and inspection.

Support:

- Turtle
- N-Triples
- N-Quads
- TriG
- RDF/XML
- JSON-LD

### 1.3 RDF instance management

Implement the complete instance CRUD layer.

- Create, retrieve, update, delete instances.
- Statement-level CRUD.
- Batch statement operations.
- Instance type management.
- Graph assignment.
- Resource relationships.
- Stable identifiers.
- Duplicate detection.
- Statement provenance.
- Asserted versus derived statement separation.

### 1.4 Internal OWL schema

Establish the system's own OWL ontology.

Define:

- Core entity classes.
- RDF instance representations.
- Ontology and schema resources.
- Property definitions.
- Metadata and provenance.
- Classification records.
- Embedding metadata.
- Import and processing job definitions.

Use the internal schema to describe managed resources, rather than maintaining a separate proprietary data model.

### 1.5 Shared application foundation

- Define `RdfStore` and transaction traits.
- Implement `OxigraphStore`.
- Implement namespace resolution.
- Establish graph naming conventions.
- Define application command and result types.
- Establish the service abstraction used by all interfaces.

### Phase 1 deliverables

- Functional Oxigraph-backed RDF engine.
- Complete RDF CRUD.
- Named graph management.
- SPARQL interface.
- Internal OWL schema.
- Rust library APIs.
- Persistent storage and backup strategy.
- Automated tests and benchmarks.

Acceptance criteria: The system can create, modify, query, validate basic RDF structures, and persist datasets across restarts.

# Phase 2 — Ontology, Taxonomy & External Schema Management

Semantic structure

Objective: Implement comprehensive OWL and SKOS management, including external ontology integration.

### 2.1 OWL ontology management

Implement `ontology-owl`.

Support:

- Ontology creation and registration.
- OWL class CRUD.
- Object property CRUD.
- Datatype property CRUD.
- Class inheritance.
- Property inheritance.
- Domain and range definitions.
- Equivalent classes and properties.
- Disjoint classes.
- Property characteristics.
- Restrictions and cardinality.
- Datatype definitions.
- Ontology versioning.
- Schema inspection and validation.

Build a schema query interface for discovering applicable classes and properties.

### 2.2 SKOS ConceptScheme management

Implement `ontology-skos`.

Support:

- Multiple ConceptSchemes.
- Scheme aggregation.
- Concept CRUD.
- Concept collections.
- Preferred labels.
- Alternative labels.
- Hidden labels.
- Language-tagged labels.
- Definitions and descriptions.
- Concept identifiers.
- Scheme membership.
- Concept-to-concept relationships.

### 2.3 Taxonomy management

Implement taxonomy operations:

- Broader/narrower relationships.
- Related concepts.
- Ancestor and descendant traversal.
- Hierarchical tree generation.
- Concept movement.
- Hierarchy restructuring.
- Taxonomy merging.
- Cross-scheme mappings.
- Duplicate concept detection.
- Cycle detection.
- Hierarchy validation.
- Taxonomy versioning.

Ensure relationships follow SKOS semantics and distinguish hierarchical relationships from associative relationships.

### 2.4 External ontology integration

Implement `ontology-import` and external ontology registry functionality.

Support importing:

- OWL ontologies.
- RDF vocabularies.
- Standard RDF schema projects.
- External SKOS schemes.
- Domain-specific ontologies.

Implement:

- Namespace discovery.
- Ontology metadata extraction.
- Dependency resolution.
- Import tracking.
- Original IRI preservation.
- Version tracking.
- External source synchronization.
- Duplicate and conflict detection.

### 2.5 External class/property mapping

Build explicit mapping functionality.

For example:

```
External schema:
  schema:Product
  schema:name
  schema:description

Internal schema:
  ex:Product
  ex:displayName
  ex:summary
```

Support:

- Class equivalence mappings.
- Subclass mappings.
- Property equivalence mappings.
- Property transformation mappings.
- Domain/range compatibility.
- Mapping confidence and provenance.
- Mapping validation.

Do not automatically assume similarly named properties are equivalent.

### Phase 2 deliverables

- Full OWL management.
- Full SKOS ConceptScheme management.
- Taxonomy management.
- External ontology registry.
- External ontology import and synchronization.
- Class and property mapping system.
- Schema validation and versioning.

Acceptance criteria: Users can manage multiple internal and external ontologies, build taxonomies, inspect class/property relationships, and establish validated mappings between schemas.

# Phase 3 — Classification, Inference & Semantic Enrichment

Intelligence engine

Objective: Turn RDF instances into semantically classified, enriched, searchable knowledge resources.

### 3.1 Classification engine

Implement `ontology-classify`.

Support multiple independent classification strategies:

| Strategy   | Function                                        |
| ---------- | ----------------------------------------------- |
| Exact      | Exact label/property matching                   |
| Normalized | Case, whitespace, and punctuation normalization |
| Token      | Keyword and token matching                      |
| Fuzzy      | Approximate text matching                       |
| Rules      | Configurable attribute-based rules              |
| Semantic   | Embedding similarity                            |
| Hybrid     | Weighted combination of strategies              |

Classification should work against multiple SKOS ConceptSchemes.

### 3.2 Instance classification

Implement:

- Single-instance classification.
- Batch classification.
- Attribute selection.
- Multiple concept assignment.
- Confidence scoring.
- Hierarchical classification.
- Classification provenance.
- Manual classification overrides.
- Classification review.
- Reclassification when taxonomies change.

Store classifications as RDF statements and associated metadata in dedicated classification graphs.

### 3.3 Embedding subsystem

Implement `ontology-embedding`.

Define pluggable `EmbeddingProvider` and `VectorIndex` traits.

Support:

- Local embedding models.
- Remote embedding APIs.
- Configurable model selection.
- Attribute-level embeddings.
- Aggregate instance embeddings.
- Batch embedding generation.
- Incremental updates.
- Embedding version management.
- Similarity metrics.
- Metadata filtering.
- Vector similarity search.

Associate embeddings with:

- Instance IRI.
- Source attribute.
- Source text hash.
- Model identifier.
- Model version.
- Embedding dimensions.
- Generation timestamp.

Keep vectors in a specialized vector backend rather than forcing them into Oxigraph.

### 3.4 Inference engine

Implement `ontology-inference`.

Support:

- OWL subclass inference.
- OWL subproperty inference.
- Domain/range-based type inference.
- Applicable class discovery.
- Applicable property discovery.
- Inherited property identification.
- External ontology application.
- Rule-based enrichment.
- Classification-driven enrichment.

Separate formal OWL entailments from heuristic recommendations.

Consider implementing a configurable reasoning profile rather than attempting complete OWL 2 DL reasoning initially.

### 3.5 Automatic schema application

Build the automated enrichment pipeline.

```
RDF Instance
     |
     v
Attribute Extraction
     |
     v
Internal / External Schema Matching
     |
     v
Class Identification
     |
     v
Property Identification
     |
     v
Validation
     |
     v
Application Preview
     |
     v
Approval / Automatic Policy
     |
     v
RDF Enrichment
     |
     v
Provenance Recording
```

Implement three application policies:

- Suggest only.
- Approval required.
- Automatic application above configured confidence thresholds.

Track asserted, inferred, and derived statements independently.

### 3.6 Explanation subsystem

Every classification and inference should be explainable.

Provide:

- Matched attributes.
- Relevant concepts.
- Matching strategies.
- Confidence calculations.
- Applicable ontology rules.
- Inherited properties.
- External mappings used.
- Validation results.
- Reasons for rejection.

### Phase 3 deliverables

- Classification engine.
- Multi-strategy taxonomy matching.
- Embedding generation and similarity search.
- OWL and application-specific inference.
- External schema enrichment.
- Automated property application.
- Classification and inference explanations.

Acceptance criteria: An RDF instance can be analyzed, classified against multiple taxonomies, matched to external schemas, enriched with applicable properties, and semantically searched.

# Phase 4 — Async HTTP API & Full-Featured CLI

Operational platform

Objective: Expose the complete engine through a stable asynchronous HTTP interface and comprehensive remote administration CLI.

### 4.1 Async application architecture

Implement the shared application execution layer.

- Command and result definitions.
- Service orchestration.
- Dependency injection.
- Error translation.
- Transaction boundaries.
- Authorization checks.
- Background task submission.
- Processing cancellation.
- Progress reporting.

Ensure the application layer does not depend on Axum, Clap, or MCP.

### 4.2 HTTP server

Implement `ontology-api` using:

- Tokio.
- Axum.
- Tower.
- Serde.
- Utoipa.
- Tracing.

Provide REST endpoints for:

1. RDF instances and statements.
2. Named graphs and SPARQL.
3. OWL ontologies, classes, and properties.
4. SKOS schemes, concepts, and taxonomies.
5. Classification and review.
6. Embeddings and similarity.
7. External ontology imports and mappings.
8. Inference and schema application.
9. Bulk import and export.
10. Asynchronous jobs.
11. System configuration and status.

Generate OpenAPI documentation automatically.

### 4.3 Async job processing

Implement a background execution system for:

- Bulk RDF imports.
- External ontology imports.
- Classification batches.
- Embedding generation.
- Taxonomy reclassification.
- Inference batches.
- RDF exports.
- Ontology synchronization.

Support:

- Job queues.
- Bounded concurrency.
- Cancellation.
- Retry policies.
- Progress tracking.
- Error reporting.
- Job history.
- Idempotency.
- Result retrieval.

Use Tokio blocking workers for synchronous Oxigraph operations and CPU-heavy tasks where appropriate.

### 4.4 Full-featured CLI

Implement `ontology-cli` using Clap and Reqwest.

Build the complete command hierarchy:

```
onto
├── server
├── config
├── instance
├── rdf
├── ontology
├── class
├── property
├── scheme
├── concept
├── taxonomy
├── classify
├── embedding
├── external
├── inference
├── import
├── export
└── job
```

Support:

- Complete CRUD.
- SPARQL execution.
- File and stdin input.
- RDF format selection.
- Bulk operations.
- JSON/YAML/table output.
- Pagination.
- Named profiles.
- Authentication.
- Dry-run.
- Interactive confirmations.
- Progress reporting.
- Job monitoring.
- Shell scripting and automation.

### 4.5 Interactive shell

Add an optional persistent CLI session.

```
onto shell
```

Provide:

- History.
- Autocomplete.
- Prefix management.
- SPARQL multiline editor.
- Persistent API connection.
- Contextual resource inspection.
- Job monitoring.

### Phase 4 deliverables

- Fully functional asynchronous HTTP API.
- OpenAPI documentation.
- Background job system.
- Complete CLI.
- Interactive shell.
- API authentication and authorization.
- Integration tests.

Acceptance criteria: All engine capabilities can be operated remotely through HTTP and CLI, including long-running tasks, without direct access to the RDF database.

# Phase 5 — MCP, Agent Integration & Production Hardening

Agent platform

Objective: Expose the complete semantic knowledge engine to LLM agents through a minimal MCP interface and prepare the platform for production use.

### 5.1 MCP server

Implement `ontology-mcp` using a Rust MCP SDK.

Expose exactly four top-level tools.

| Tool       | Scope                                                           |
| ---------- | --------------------------------------------------------------- |
| `rdf`      | RDF CRUD, statements, graphs, SPARQL, imports, exports          |
| `ontology` | OWL, SKOS, taxonomies, external ontologies, mappings            |
| `semantic` | Classification, embeddings, inference, enrichment, explanations |
| `jobs`     | Submit, inspect, monitor, retrieve, cancel, retry               |

### 5.2 MCP action dispatch

Each tool accepts an explicit action and structured parameters.

```
{
  "action": "infer",
  "target": "urn:example:product:123",
  "scope": {
    "graph": "data:asserted"
  },
  "options": {
    "include_external": true,
    "include_inherited": true,
    "policy": "suggest"
  }
}
```

Implement:

- Action-specific input schemas.
- Parameter validation.
- Consistent output structures.
- Pagination.
- Result limits.
- Compact summaries.
- Provenance and explanation retrieval.
- Job handles for expensive work.

The MCP tools should expose the same capabilities as the HTTP API without duplicating application logic.

### 5.3 Agent workflow support

Optimize common multi-step agent workflows.

Examples:

Ontology discovery

```
Search ontology
    → Inspect classes
    → Inspect properties
    → Identify applicable schema
```

Instance enrichment

```
Retrieve instance
    → Inspect attributes
    → Classify
    → Infer external classes
    → Preview properties
    → Apply approved enrichment
```

Taxonomy management

```
Inspect scheme
    → Search concepts
    → Create or update concepts
    → Establish relationships
    → Validate hierarchy
```

Bulk processing

```
Submit import
    → Monitor job
    → Review classification
    → Generate embeddings
    → Apply enrichment
```

### 5.4 Agent safety and permissions

- Read-only and write permissions.
- Explicit destructive-operation controls.
- Approval-required inference policies.
- Restricted SPARQL Update.
- Job cancellation.
- External import restrictions.
- Audit trails.
- Resource-level authorization.
- Configurable agent execution limits.

### 5.5 Performance and production hardening

Benchmark and optimize:

- RDF insertion throughput.
- SPARQL query latency.
- Graph traversal.
- Taxonomy searches.
- Batch classification.
- Embedding throughput.
- Vector retrieval.
- Concurrent HTTP requests.
- MCP tool latency.
- Large dataset import/export.

Implement:

- Database backup and recovery.
- Graceful shutdown.
- Worker recovery.
- Health and readiness endpoints.
- Structured metrics.
- Distributed tracing where needed.
- API rate limiting.
- Resource quotas.
- Configuration validation.
- Compatibility and migration tests.

### 5.6 Documentation and deployment

Deliver:

- Rust library documentation.
- HTTP API reference.
- CLI manual.
- MCP tool documentation.
- Ontology extension guide.
- Embedding provider integration guide.
- Deployment configuration.
- Example ontology projects.
- Example taxonomies.
- Agent workflow examples.
- Operational and backup documentation.

### Phase 5 deliverables

- Four-tool MCP server.
- Full LLM agent access.
- Agent workflow examples.
- Permission and approval controls.
- Performance benchmarks.
- Production deployment.
- Operational documentation.
- Release-ready system.

Acceptance criteria: An LLM agent can independently discover schemas, inspect and modify RDF, manage taxonomies, classify instances, generate embeddings, import external ontologies, preview inference, apply approved enrichment, and monitor background processing through four MCP tools.

## 5-Phase Milestone Summary

| Phase | Primary outcome        | Completion milestone                  |
| ----- | ---------------------- | ------------------------------------- |
| 1     | RDF foundation         | Working Oxigraph engine               |
| 2     | Ontology management    | OWL, SKOS, external schemas           |
| 3     | Semantic intelligence  | Classification, embeddings, inference |
| 4     | Application interfaces | Async API and complete CLI            |
| 5     | Agent integration      | Four-tool MCP and production release  |

## Recommended implementation priorities

A few architectural decisions are particularly important to preserve throughout development:

1. Oxigraph remains authoritative. Avoid introducing a parallel relational entity store for RDF-managed resources.
2. Keep reasoning modular. OWL reasoning, SKOS classification, and heuristic enrichment are different capabilities.
3. Make derived data reproducible. Retain model versions, configuration, source hashes, and provenance.
4. Design for incremental processing. Changes to one instance or taxonomy should not require recomputing the entire knowledge graph.
5. Build the application service layer before the interfaces. HTTP, CLI, and MCP should all share the same operations.
6. Treat MCP as an agent-oriented interface. Four broad tools should expose a large set of structured operations without sacrificing safety or discoverability.

Suggested build order: Complete Phases 1–3 as the core Rust library, then Phase 4 as the operational platform, and Phase 5 as the agent integration and production release. This minimizes interface rework while ensuring the semantic engine is independently testable and useful before MCP is introduced.
