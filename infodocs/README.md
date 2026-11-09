# Ontology Management System — Documentation

This directory documents the implemented system, not the design proposal. Each
file covers one major subsystem: its responsibilities, public types, storage
layout, control flow, invariants, configuration, tests, and known limits.

## What the system does

The system is a Rust workspace for managing RDF data, OWL schemas, SKOS
taxonomies, external ontologies, semantic classification, embeddings,
inference, and document extraction. Storage runs as one Oxigraph store or as an
optional routed multi-graph deployment, and MinerU extraction is opt-in. The
same application behavior is exposed through three interfaces:

- an asynchronous HTTP API (`ontology-api`);
- a remote administration CLI and interactive shell (`onto`);
- a four-tool MCP server for LLM agents (`ontology-mcp`).

Oxigraph over RocksDB is the only storage engine. It is isolated behind the
`RdfStore` trait and the `ontology-rdf` crate. `ontology-graph` optionally
composes several stores behind that same trait.

## Layering

```
HTTP endpoints        CLI commands        MCP tools
      \                   |                  /
       \                  |                 /
        +-------- Command / CommandResult --------+
                          |
                    ApplicationService
   (instances, OWL, SKOS, external, semantic, jobs, documents)
      /        |         |          |          \            \
 OwlManager SkosManager External Classification Inference  DocumentService
      \        |         |          |          /                 |
                        RdfStore trait                  ontology-document
                             |                                  |
                       GraphBackend                    ontology-mineru
                     /              \                  (optional PyO3)
              MultiStore        OxigraphStore                   |
            (satellites)      (single/RocksDB)            MinerU + shim
```

Rules that hold across the codebase:

1. `ontology-core` defines domain types, identifiers, graph conventions, errors,
   configuration, commands, and capability traits. It has no Oxigraph types.
2. Domain crates (`ontology-owl`, `ontology-skos`, `ontology-import`,
   `ontology-classify`, `ontology-embedding`, `ontology-inference`) implement
   behavior over `RdfStore`.
3. `ontology-application` is the single orchestration boundary. Interfaces only
   translate transport input to `Command` values and render `CommandResult`
   values.
4. Oxigraph is confined to `ontology-rdf`.
5. Storage is synchronous. Async interfaces call it from request or job tasks;
   the engine itself is not async.
6. Document extraction is engine-neutral above `ontology-mineru`; the Python
   boundary is optional and never leaks into core or the interfaces.

## Workspace map

| Crate | Responsibility | Documentation |
| --- | --- | --- |
| `ontology-core` | IDs, namespaces, graph conventions, model, commands, config, errors, jobs, traits | [core-domain.md](core-domain.md) |
| `ontology-rdf` | Oxigraph adapter, schema bootstrap, RDF formats, transactions, backup | [rdf-storage.md](rdf-storage.md) |
| `ontology-graph` | Optional multi-store registry, catalog, query router, administration | [multi-graph.md](multi-graph.md) |
| `ontology-owl` | OWL classes, properties, ontologies, versions, hierarchy, validation | [owl-schema.md](owl-schema.md) |
| `ontology-skos` | SKOS schemes, concepts, relations, collections, trees, validation, merge | [skos-taxonomy.md](skos-taxonomy.md) |
| `ontology-import` | External ontology registry, local-file import/sync, mappings | [external-ontologies.md](external-ontologies.md) |
| `ontology-classify` | Multi-strategy matching, persistence, review, overrides | [classification.md](classification.md) |
| `ontology-embedding` | Hashing provider, in-memory vector index, embedding metadata | [embeddings.md](embeddings.md) |
| `ontology-inference` | Subclass/domain/range/subproperty, mappings, classifications, rules | [inference.md](inference.md) |
| `ontology-document` | Schema dispatch, canonical document model, artefacts/cache, text projection, RDF mapping | [document-extraction.md](document-extraction.md) |
| `ontology-mineru` | Optional PyO3 bridge to the MinerU Python shim | [document-extraction.md](document-extraction.md) |
| `ontology-application` | Command dispatch, instance service, job manager | [application-service.md](application-service.md), [background-jobs.md](background-jobs.md) |
| `ontology-api` | Axum router, auth, metrics, rate limiting, OpenAPI, server bootstrap | [http-api.md](http-api.md) |
| `ontology-cli` | `onto` remote client, profiles, output formats, shell | [cli.md](cli.md) |
| `ontology-mcp` | Four MCP tools, access policy, audit, local/remote backends | [mcp-server.md](mcp-server.md) |
| `tests` | Cross-crate integration suites for every phase | [testing.md](testing.md) |

## Fundamental conventions

### Identifiers

`Id` is a validated absolute IRI. It is the identifier for instances, classes,
properties, schemes, concepts, graphs, jobs, classifications, and records.
`OperationId` is a generated ULID used to build record identifiers such as:

```
urn:onto:job:<ULID>
urn:onto:classification:<ULID>
urn:onto:embedding:<ULID>
urn:onto:provenance:<ULID>
urn:onto:inference:<ULID>
```

### Reserved graphs

| Kind | Graph IRI | Contents |
| --- | --- | --- |
| Schema | `urn:onto:system:schema` | Internal OWL schema and managed class/property/ontology definitions |
| Taxonomy | `urn:onto:system:taxonomy` | SKOS schemes, concepts, relations, collections, versions |
| External | `urn:onto:system:external` | External ontology registry metadata and mappings |
| Asserted | `urn:onto:data:asserted` | Explicit instance statements |
| Inferred | `urn:onto:data:inferred` | Inference output |
| Classification | `urn:onto:data:classification` | Classification records |
| Provenance | `urn:onto:data:provenance` | Provenance records and embedding metadata |
| Registry | `urn:onto:system:graphs` | Materialised multi-graph registry and generation metadata |

Structured extraction also creates one graph per document and facet:
`urn:onto:data:document:<hex>:{structure,layout,bibliography,provenance}`.
These are created on demand and are not `GraphKind` variants; in registry mode
they must be registered before the RDF mapping can write them.

`tenant_graph(tenant, kind)` produces `urn:onto:tenant:<tenant>:<kind>` for
isolated deployments. The built-in graph constructors keep the reserved IRIs.

### Command boundary

Every interface sends one of seven command families:

```
Rdf | Ontology | Taxonomy | External | Semantic | Job | Document
```

`ApplicationService::execute` returns a tagged `CommandResult`:
`resource`, `collection`, `query`, `report`, `validation`, `taxonomy`,
`mapping`, `classifications`, `embeddings`, `similarity`, `preview`,
`inference`, `export`, `job`, or `document`. See [core-domain.md](core-domain.md)
for the JSON shapes.

## Request flow

### HTTP and CLI

```
request body / CLI arguments
  -> build Command
  -> ApplicationService::execute
  -> domain manager
  -> RdfStore
  -> Oxigraph
  -> CommandResult
  -> JSON endpoint / formatted CLI output
```

The CLI always posts to `POST /api/v1/commands`; it does not open the database.
The HTTP API has both ergonomic resource routes and a generic command route.

### MCP

```
tool + action + arguments
  -> action dispatcher
  -> access/destructive/external policy
  -> Command
  -> local ApplicationService OR POST /api/v1/commands
  -> CommandResult
  -> compact MCP envelope
```

MCP never contains domain logic. It translates, authorizes, executes, audits,
and truncates large results.

## Quick start

```sh
# In-memory API on 127.0.0.1:8080
cargo run -p ontology-api

# Persistent API
ONTO_STORAGE_PATH=./data cargo run -p ontology-api

# CLI
cargo run -p ontology-cli --bin onto -- --help
cargo run -p ontology-cli --bin onto -- instance list

# Document extraction (needs a python-enabled build and MinerU)
cargo run -p ontology-cli --bin onto -- document extract --help

# Multi-graph mode: add [graphs] to the config; see multi-graph.md

# MCP over stdio, read-only
cargo run -p ontology-mcp

# Checks
cargo check --workspace --all-targets
cargo test --workspace
```

The API bootstraps `ontologies/system-owl.ttl` into the schema graph on start.
Configuration comes from `ONTO_CONFIG`/`--config`, with environment overrides.

## Interface entry points

| Interface | Binary / library | Transport | Auth |
| --- | --- | --- | --- |
| HTTP | `ontology-api` | JSON over HTTP | Optional bearer token |
| CLI | `onto` | HTTP to API | Bearer token or profiles |
| MCP | `ontology-mcp` | MCP over stdio | Environment policy gates |
| Library | `ontology-application` | Direct Rust calls | Caller-controlled |

## Operational summary

- `GET /api/v1/system/health` is public; status, metrics, and all data routes
  require auth when `ONTO_API_KEY` is set.
- `ONTO_RATE_LIMIT` enables a process-wide fixed-window per-minute limit.
- `ONTO_MCP_ACCESS`, `ONTO_MCP_ALLOW_DESTRUCTIVE`, and
  `ONTO_MCP_ALLOW_EXTERNAL` gate MCP behavior. Defaults are read-only.
- Jobs are in-process, bounded to four concurrent workers, and do not survive a
  restart.
- `[graphs]` enables the multi-store registry and router; without it the
  process opens one store. `GraphAdmin` is library-only for now.
- `[extraction]` is disabled by default. Enabling it requires a
  `--features python` build and a Python environment with MinerU.
- Backup is available through `GraphBackend::admin().backup` (all stores) or
  `OxigraphStore::backup` (one store), plus logical RDF export.
- See [../docs/operations.md](../docs/operations.md) for deployment,
  backup/recovery, and the security checklist.

## Reading guide

Start with [core-domain.md](core-domain.md), then [rdf-storage.md](rdf-storage.md).
Read [multi-graph.md](multi-graph.md) when several store directories are in
play, and [document-extraction.md](document-extraction.md) for MinerU. For a
particular interface, read [application-service.md](application-service.md)
first, followed by the relevant interface file. For data modeling, read the OWL,
SKOS, external, and inference files in that order.

## Known system-level limitations

- Single-process job registry and vector index.
- One RocksDB writer per database directory.
- MCP currently speaks stdio only.
- The API rate limiter is global to the process, not per client.
- External ontology resolution accepts local paths and `file://`; HTTP(S)
  fetching is not implemented.
- The built-in embedding provider is a deterministic hashing baseline, not a
  trained model.
- MinerU is not installed in the development environment, so extraction is
  verified only against fixtures and the Python shim contract.
- Per-document RDF graphs are not routable in multi-graph registry mode.
- Multi-graph administration has no HTTP, CLI, or MCP surface.
