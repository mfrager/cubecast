# Testing and Verification

The workspace uses unit tests inside crates and a separate integration-test
member (`tests`) that exercises multiple crates together.

## Current result

Last full verification:

```text
cargo check --workspace --all-targets          exit 0
cargo test --workspace                         218 passed, 0 failed
cargo fmt --all -- --check                     exit 0
cargo clippy --workspace --all-targets         exit 0
cargo bench -p ontology-mcp -- --test          exit 0
PYO3_PYTHON=... cargo check -p ontology-mineru --features python  exit 0
```

The 218 passing tests break down as:

| Area | Tests |
| --- | --- |
| `ontology-core` unit tests | 31 |
| `ontology-rdf` unit tests | 7 |
| `ontology-owl` unit tests | 4 |
| `ontology-skos` unit tests | 3 |
| `ontology-graph` unit tests | 19 |
| `ontology-classify` unit tests | 5 |
| `ontology-embedding` unit tests | 8 |
| `ontology-inference` unit tests | 4 |
| `ontology-document` unit tests | 7 |
| `ontology-application` unit tests | 6 |
| `ontology-api` unit tests | 6 |
| `ontology-cli` unit tests | 7 |
| `ontology-mcp` unit tests | 16 |
| Phase 1 integration | 15 |
| Phase 2 integration | 26 |
| Phase 3 integration | 7 |
| Phase 4 integration | 15 |
| Phase 5 integration | 2 |
| Phase 6 end-to-end integration | 3 |
| Phase 7 multi-graph integration | 12 |
| Phase 8 extraction integration | 5 |
| `ontology-document` fixture integration | 6 |
| `ontology-cli` live HTTP integration | 1 |
| `ontology-mcp` stdio integration | 2 |
| `ontology-mcp` end-to-end integration | 1 |

## Commands

```sh
# Compile every library, binary, test, and benchmark
cargo check --workspace --all-targets

# Full suite
cargo test --workspace

# Focused areas
cargo test -p ontology-core
cargo test -p ontology-rdf
cargo test -p ontology-owl
cargo test -p ontology-skos
cargo test -p ontology-import
cargo test -p ontology-classify
cargo test -p ontology-embedding
cargo test -p ontology-inference
cargo test -p ontology-graph
cargo test -p ontology-document
cargo test -p ontology-mineru
cargo test -p ontology-application
cargo test -p ontology-api
cargo test -p ontology-cli
cargo test -p ontology-mcp

# Integration suites
cargo test -p ontology-integration-tests --test phase1_storage
cargo test -p ontology-integration-tests --test phase1_instances
cargo test -p ontology-integration-tests --test phase1_service
cargo test -p ontology-integration-tests --test phase2_owl
cargo test -p ontology-integration-tests --test phase2_skos
cargo test -p ontology-integration-tests --test phase2_external
cargo test -p ontology-integration-tests --test phase3_semantic
cargo test -p ontology-integration-tests --test phase4_api
cargo test -p ontology-integration-tests --test phase4_cli
cargo test -p ontology-integration-tests --test phase4_jobs
cargo test -p ontology-integration-tests --test phase5_api
cargo test -p ontology-integration-tests --test phase6_e2e
cargo test -p ontology-integration-tests --test phase7_multigraph
cargo test -p ontology-integration-tests --test phase8_extraction

# Python bridge compile check (requires an interpreter)
PYO3_PYTHON=./.venv/bin/python cargo check -p ontology-mineru --features python

# Benchmarks
cargo bench -p ontology-rdf
cargo bench -p ontology-mcp
```

Use `set -o pipefail` when filtering output through `head`/`tail` so a failing
test cannot be hidden.

## Integration layout

`tests` is a workspace member:

- `tests/src/lib.rs` — shared fixtures;
- `tests/tests/*.rs` — independent test binaries.

Fixtures include:

- `ex(local)` — example IRI builder;
- `asserted_graph()`, `inferred_graph()`;
- `memory_store()`, `memory_store_handle()`;
- `asserted_quad(subject, predicate, value)`;
- `sample_turtle()`, `sample_nquads()`.

Most tests use in-memory stores; multi-graph tests use temporary persistent
directories because satellites must reopen on disk. API tests mostly use
`tower::ServiceExt::oneshot` against the router, while the CLI integration test
binds an ephemeral TCP listener and drives the real `onto` binary. MCP stdio
tests spawn the real `ontology-mcp` binary with `TokioChildProcess`.

## What each suite covers

### Phase 1 — storage and instances

`phase1_storage.rs`:

- memory and persistent stores;
- reopen/retention;
- read-only open;
- patterns, graphs, transactions and rollback;
- every RDF format;
- import/export round trips;
- schema bootstrap idempotence;
- backup and validation.

`phase1_instances.rs`:

- create/get/list/update/delete;
- statements and stable IDs;
- relationships;
- graph selection;
- provenance.

`phase1_service.rs`:

- command execution and result shapes;
- namespace resolution;
- serialization.

### Phase 2 — schema and external ontologies

`phase2_owl.rs`:

- class/property CRUD and queries;
- hierarchy and applicable queries;
- restrictions, disjointness, domains/ranges;
- validation issues and cycles;
- ontology versions.

`phase2_skos.rs`:

- schemes/concepts/collections;
- relation canonicalization;
- cycles and label uniqueness;
- trees, ancestors, descendants, search;
- merge and validation;
- versions.

`phase2_external.rs`:

- local import and metadata;
- re-import replacement;
- sync and delete;
- counts/namespace discovery;
- class/property mappings;
- mapping validation.

### Phase 3 — semantic

`phase3_semantic.rs`:

- exact/normalized/token/fuzzy/hybrid;
- thresholds and ancestors;
- persistence, list, override, review, reclassify;
- embeddings, metadata, similarity, filters, deletion;
- inference methods, policies, target graph, provenance.

### Phase 4 — API, CLI, jobs

`phase4_api.rs`:

- health/status and OpenAPI;
- instance and schema routes;
- generic command route;
- auth and error statuses.

`phase4_cli.rs`:

- command construction and dry-run;
- profiles and output;
- confirmations;
- end-to-end server calls.

`phase4_jobs.rs`:

- the original job kinds;
- listing/get/results;
- cancellation and retry;
- progress/state transitions.

### Phase 5 — hardening and MCP

`phase5_api.rs`:

- completed-request/error metrics;
- global rate limit and `Retry-After`.

`crates/ontology-mcp/tests/stdio.rs`:

- exactly four tools;
- read-only denial;
- create/get;
- export-job polling;
- confirmation requirement;
- successful destructive delete with `confirm`.

Unit tests in `ontology-mcp` cover action translation, policy, result
compaction, schema flattening, and UTF-8 truncation.

### Phase 6 — end-to-end feature paths

`phase6_e2e.rs`:

- full OWL/SKOS/instance/classification/review/inference/embedding lifecycle;
- background RDF export and SPARQL assertions;
- external ontology import, class/property mapping, and mapping inference;
- HTTP routing for schema, taxonomy, semantics, export, and SPARQL.

`crates/ontology-cli/tests/e2e.rs` drives the real `onto` binary against a live
HTTP server: config, instance create/get, graph listing, export submission, and
job watching.

`crates/ontology-mcp/tests/e2e.rs` spans the MCP tools: class hierarchy, scheme
and concept creation, instance creation, classification, review, inference
preview, and export-job polling. It does not run `extract_document`.

### Phase 7 — multi-graph

`phase7_multigraph.rs`:

- registry sync, generations, resync/retire;
- cross-store round trips and joins, satellite fusion;
- variable `GRAPH` expansion and dataset-clause rejection;
- degraded optional stores and missing required stores;
- read-only satellite writes and administrator writes;
- strict/split update policies, per-store backup;
- namespace/status resolution and single-mode parity.

### Phase 8 — document extraction

`phase8_extraction.rs`:

- application `document extract` and `get` through the service;
- text-only mode (structured data absent, text present);
- `document_extract` job submission and result;
- HTTP POST/GET document routes;
- `Unsupported` when extraction is not configured.

`crates/ontology-document/tests/extraction.rs` adds fixture-based structured,
text, and both modes, mode exclusion, cache/force behavior, RDF mapping and
idempotence. No test performs a live MinerU parse.

## Benchmarks

`crates/ontology-rdf/benches/store.rs` measures storage operations.

`crates/ontology-mcp/benches/dispatch.rs` measures MCP action translation. It
does not include model, network, or storage latency. Run with `-- --test` for a
single verification pass:

```sh
cargo bench -p ontology-mcp --bench dispatch -- --test
```

## Test design conventions

- Use in-memory stores unless persistence itself is under test.
- Use unique example IRIs; tests share no database.
- Test observable behavior through public traits/commands, not private fields.
- Assert error variants where behavior depends on the failure class.
- Keep async tests on Tokio; MCP protocol tests use a multi-thread runtime.
- Use `oneshot` for HTTP rather than binding ports.
- Spawn the real MCP binary only for protocol-level coverage.

## What is not covered

- Concurrency/race testing beyond basic parallel API requests.
- Fault injection for storage, I/O, or provider failures.
- Crash/restart recovery for jobs.
- Backup restore verification.
- Load, soak, and performance regression thresholds.
- Property-based/fuzz testing of RDF and SPARQL inputs.
- Security testing (auth bypass, SPARQL abuse, audit tampering).
- TLS/proxy deployment behavior.
- Live MinerU extraction, model provisioning, and Python failure modes;
  only the shim contract, fixtures, and feature compile are covered.
- Multi-store crash recovery, online rebalancing, and cross-store rollback
  (degraded/missing stores and policy rejection are covered).
- Cross-process extraction cache behavior after a MinerU upgrade.
- Cross-platform behavior; CI runs on Linux.

Passing tests establish functional behavior, not production capacity or
security.

## Adding tests

1. Put reusable fixtures in `tests/src/lib.rs`.
2. Add a focused `tests/tests/phaseN_*.rs` binary for cross-crate behavior.
3. Keep unit-level checks in the owning crate.
4. For a new HTTP route, add a route test and, if it is public, an auth test.
5. For a new MCP action, add a dispatch unit test and extend the stdio test when
   policy behavior changes.
6. For extraction changes, keep fixtures under
   `crates/ontology-document/tests/fixtures/` and do not require MinerU; gate
   live checks behind the Python feature.
7. For multi-graph changes, extend `phase7_multigraph.rs` with a temporary
   registry and store directories rather than relying on operator files.
8. Run the focused test, then the full workspace check/test.

## Related

- Operations: [../docs/operations.md](../docs/operations.md)
- Storage: [rdf-storage.md](rdf-storage.md)
- MCP: [mcp-server.md](mcp-server.md)
