# Ontology Management System

Rust workspace for an RDF ontology management engine with Oxigraph as its
storage and SPARQL layer. Phases 1–3 implement the RDF foundation, OWL and SKOS
schema management, external ontologies, classification, embeddings, and
inference. Phase 4 adds the asynchronous HTTP API, background jobs, and the
remote administration CLI. Phase 5 adds the MCP server for LLM agents (now six tools),
with read-only defaults, explicit destructive/external gates, and audit logging.

## Layout

```
crates/
  ontology-core/         domain types, ids, namespaces, graphs, config, traits
  ontology-rdf/          Oxigraph adapter: quads, SPARQL, transactions, formats
  ontology-owl/          OWL classes, properties, restrictions, validation, versions
  ontology-skos/         SKOS schemes, concepts, relations, search, validation
  ontology-classify/     exact, normalized, token, fuzzy, rule, semantic matching
  ontology-embedding/    providers, vector index, similarity search
  ontology-inference/    OWL, mapping, classification, and rule enrichment
  ontology-import/       external ontology registry, import, sync, mappings
  ontology-application/  instance, statement, and schema services; command dispatch
  ontology-api/          Axum HTTP API, OpenAPI document, server binary
  ontology-cli/          Clap + Reqwest `onto` CLI and interactive shell
  ontology-mcp/          six-tool MCP server over stdio
ontologies/system-owl.ttl  internal schema describing managed resources
docs/mcp.md              MCP tools, safety model, and agent workflows
tests/                   integration tests (workspace member)
config/ontology.toml     example configuration
```

## Storage engine

RDF storage uses the internal Oxigraph library from
`https://github.com/mfrager/oxigraph` (branch `basenote`), built against the
system RocksDB through `pkg-config` (the same setup as `~/Scripts/rust_extract`).
Building therefore requires:

- RocksDB development files (`pkg-config --modversion rocksdb`)
- libclang for `bindgen`
- a C++20 compiler

Both persistent (RocksDB directory) and in-memory stores are supported.

## HTTP API

```sh
cargo run -p ontology-api                 # 127.0.0.1:8080, in-memory store
ONTO_STORAGE_PATH=./data cargo run -p ontology-api
ONTO_API_KEY=secret cargo run -p ontology-api
```

The server bootstraps the internal schema on start. Configuration is read from
`--config`/`ONTO_CONFIG` (TOML), and `ONTO_BIND` selects the listen address.
Every authenticated request carries `Authorization: Bearer <key>`.
`ONTO_RATE_LIMIT` enables a global requests-per-minute cap (429 with
`Retry-After` when exceeded). `GET /api/v1/system/metrics` reports completed
request/error counts and the active limit.

Endpoints cover instances and statements, graphs and SPARQL, OWL classes and
properties, SKOS schemes, concepts and taxonomies, classification and review,
embeddings and similarity, external ontology imports and mappings, inference,
RDF import/export, and background jobs. `GET /api/v1/openapi.json` returns the
OpenAPI document, and `POST /api/v1/commands` accepts any application command
for automation and clients that need identifiers containing `/`.

Background jobs are submitted to `POST /api/v1/jobs` with a `JobRequest`
(`classifyBatch`, `reclassifyBatch`, `embedBatch`, `inferBatch`, `rdfImport`,
`rdfExport`, `externalImport`, or `externalSync`), then inspected, cancelled,
or retried through `/api/v1/jobs/{id}`.

## CLI

```sh
cargo run -p ontology-cli --bin onto -- --help
onto --server http://127.0.0.1:8080 instance list
onto rdf query 'SELECT * WHERE { ?s ?p ?o } LIMIT 10'
onto import --format turtle --file data.ttl
onto export --format nquads --output data.nq
onto job watch urn:onto:job:01...
onto shell
```

Connection settings come from `--server`, `--api-key`, `ONTO_SERVER`,
`ONTO_API_KEY`, or named profiles in `~/.config/onto/cli.toml`
(`ONTO_CLI_CONFIG` overrides the path). Results render as JSON, YAML, or a
table (`--output`); `--dry-run` prints the command without sending it.

## MCP

```sh
cargo run -p ontology-mcp
```

The server speaks MCP over stdio and exposes exactly six tools: `rdf`,
`ontology`, `semantic`, `jobs`, `registry`, and `project`. It is read-only by
default. Set
`ONTO_MCP_ACCESS=read-write` for writes, `ONTO_MCP_ALLOW_DESTRUCTIVE=true` for
deletes and SPARQL Update, and `ONTO_MCP_ALLOW_EXTERNAL=true` for external
ontology imports. Destructive calls still require `"confirm": true`, and
`"dry_run": true` returns the translated command without executing it.

Set `ONTO_MCP_SERVER=http://127.0.0.1:8080` to forward MCP commands to a running
API instead of opening the local store. `ONTO_MCP_AUDIT_LOG` writes one JSONL
record per tool call. See [docs/mcp.md](docs/mcp.md) for tool actions, safety
configuration, and agent workflow examples, and
[docs/operations.md](docs/operations.md) for deployment, health, rate limiting,
backup, and recovery.

## Checks

```sh
cargo check --workspace --all-targets
cargo test --workspace
cargo bench -p ontology-rdf      # storage benchmarks
cargo bench -p ontology-mcp      # MCP action-translation benchmark
```
