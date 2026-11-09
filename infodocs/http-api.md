# HTTP API (`ontology-api`)

`ontology-api` is an Axum server over the shared application service. Resource
routes and the generic command route both translate to `Command` and return
`CommandResult` JSON. The crate also provides the server bootstrap used by the
`ontology-api` binary and `onto server`.

Source: `crates/ontology-api/src/`.

## Modules

| Module | Responsibility |
| --- | --- |
| `router` | Routes, auth, metrics, rate limit, handlers |
| `state` | `AppState`, `Metrics`, `RateLimiter` |
| `error` | Domain error → HTTP status/body |
| `openapi` | OpenAPI 3.0 document |
| `server` | Store open, schema bootstrap, bind, graceful shutdown |
| `main` | Binary entry point |

## Startup

`serve(ServeOptions)`:

1. `Config::validate()`.
2. `GraphBackend::from_config(&config)` (opens the single store or the
   multi-graph registry, and bootstraps the schema on the main store).
3. Build the namespace registry from config.
4. Build `ApplicationService`, including configured document extraction.
6. Build `AppState` with API key and optional rate limit.
7. Build the router.
8. Bind `options.bind` (default `127.0.0.1:8080`).
9. Serve with graceful shutdown on Ctrl-C/SIGTERM.

Environment:

| Variable | Default | Effect |
| --- | --- | --- |
| `ONTO_BIND` | `127.0.0.1:8080` | Listen address |
| `ONTO_API_KEY` | unset | Enables bearer auth |
| `ONTO_CONFIG` | unset | TOML config path |
| `ONTO_STORAGE_PATH` | unset | Persistent store override |
| `ONTO_RATE_LIMIT` | unset | Global requests/minute; invalid/zero is ignored with a warning |

`--bind` and `--config` on `onto server` override the corresponding settings.

## State

`AppState` is cloneable and holds:

- `service: Arc<ApplicationService>`
- optional `api_key`
- `started_at`
- `metrics: Arc<Metrics>`
- optional `rate_limit: Arc<RateLimiter>`

`AppState::open(service)` is unauthenticated. `with_rate_limit(Some(n))`
enables the limiter.

## Authentication

When `api_key` is set, every protected route requires exactly:

```
Authorization: Bearer <key>
```

Comparison is exact after stripping `Bearer `. Missing/invalid credentials
return `401` with:

```json
{"error":{"kind":"unauthorized","message":"missing or invalid bearer token"}}
```

Public routes:

- `GET /api/v1/system/health`
- `GET /api/v1/openapi.json`

Everything else is protected. There is no per-route authorization or tenant
scoping.

## Middleware order

The protected router is layered:

```
metrics (outer)
  authentication
    rate limit
      handler
```

Consequences:

- metrics count completed protected requests, including 401 and 429 responses;
- unauthenticated requests do not consume rate-limit budget;
- rate-limited requests are counted as errors;
- the metrics endpoint itself is counted only after its response completes, so
  its snapshot does not include that call.

`ONTO_RATE_LIMIT` is a process-wide fixed 60-second window, not per client.
Rejected requests get `429`, `Retry-After: 60`, and a `rate_limited` error body.

## Endpoint groups

### System

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/api/v1/system/health` | Public liveness, version |
| GET | `/api/v1/system/status` | Auth, uptime, job count |
| GET | `/api/v1/system/metrics` | Requests, errors, active rate limit, uptime |
| GET | `/api/v1/config` | Version, auth flag, namespace prefixes |
| GET | `/api/v1/openapi.json` | OpenAPI document |

### Generic commands

| Method | Path | Body |
| --- | --- | --- |
| POST | `/api/v1/commands` | Any `Command` |
| POST | `/api/v1/rdf` | `RdfCommand` |
| POST | `/api/v1/ontology` | `OntologyCommand` |
| POST | `/api/v1/taxonomy` | `TaxonomyCommand` |
| POST | `/api/v1/external` | `ExternalCommand` |
| POST | `/api/v1/semantic` | `SemanticCommand` |
| POST | `/api/v1/import`, `/api/v1/rdf/import` | RDF import command |
| POST | `/api/v1/export`, `/api/v1/rdf/export` | RDF export command |

### Instances and dataset

`GET/POST /api/v1/instances`; `GET/PUT/DELETE /api/v1/instances/{id}`;
`GET/POST/DELETE /api/v1/instances/{id}/statements`;
`GET /api/v1/instances/{id}/relationships`; `GET /api/v1/graphs`;
`POST /api/v1/sparql/query`; `POST /api/v1/sparql/update`.

Query parameters commonly include `graph`, `type`, `limit`, and `offset`.
`graph=default` maps to the default graph; empty means no restriction for reads
and the asserted graph for mutations.

### OWL

`GET/POST /api/v1/classes`; `GET/PUT/DELETE /api/v1/classes/{id}`;
`GET /api/v1/classes/{id}/hierarchy`;
`GET /api/v1/applicable-properties/{class}`;
`POST /api/v1/applicable-classes`;
`POST /api/v1/ontology/validate`;
`GET/POST /api/v1/properties`; `GET/PUT/DELETE /api/v1/properties/{id}`;
`GET/POST /api/v1/ontologies`; `GET/DELETE /api/v1/ontologies/{id}`;
`GET /api/v1/ontologies/{id}/versions`.

### Taxonomy

`GET/POST /api/v1/schemes`; `GET/PUT/DELETE /api/v1/schemes/{id}`;
`GET /api/v1/schemes/{id}/versions`;
`GET/POST /api/v1/concepts`; `GET/PUT/DELETE /api/v1/concepts/{id}`;
`GET /api/v1/concepts/{id}/relations|descendants|ancestors`;
`POST /api/v1/concepts/{id}/move`; `GET /api/v1/concepts/search`;
`GET /api/v1/taxonomies/{scheme}/tree`; `POST /api/v1/taxonomies/merge`;
`POST /api/v1/taxonomies/validate`; `POST/DELETE /api/v1/relations`;
`GET/POST /api/v1/collections`; `GET/DELETE /api/v1/collections/{id}`.

### Semantic

`POST /api/v1/classifications/run|batch|override|review|reclassify`;
`GET /api/v1/classifications/{instance}`;
`POST /api/v1/embeddings/generate`;
`GET/DELETE /api/v1/embeddings/{instance}`;
`POST /api/v1/similarity/search`;
`POST /api/v1/inference/preview|apply|explain`.

### External and mappings

`GET/POST /api/v1/external-ontologies`;
`GET/DELETE /api/v1/external-ontologies/{id}`;
`POST /api/v1/external-ontologies/{id}/sync`;
`GET/POST /api/v1/mappings/classes`;
`GET/POST /api/v1/mappings/properties`;
`POST /api/v1/mappings/validate`.

### Jobs

`GET/POST /api/v1/jobs`;
`GET/DELETE /api/v1/jobs/{id}` (DELETE cancels);
`GET /api/v1/jobs/{id}/results`;
`POST /api/v1/jobs/{id}/cancel|retry`.

Job bodies are `JobRequest` values. There is no separate request shape per job
kind; the `kind` tag selects the payload.

### Document extraction

| Method | Path | Purpose |
| --- | --- | --- |
| POST | `/api/v1/documents/extractions` | Run `ExtractionRequest` and return `ExtractionOutput` |
| GET | `/api/v1/documents/{id}` | Load the latest stored run for `urn:sha256:<hex>` |

Extraction is available only when the server was built with
`ontology-mineru`'s `python` feature and `[extraction].enabled = true`.
Otherwise the command returns `501` with `unsupported`. The POST body is the
same request used by the CLI, jobs, and generic command route; `mode` and
`text_format` override the configured defaults, and `output_dir` and `force`
are accepted.

## Response shape

Handlers return `Json<CommandResult>` on success. Example collection:

```json
{
  "type": "collection",
  "items": [{"id": "https://example.org/Product", "types": ["..."]}],
  "count": 1,
  "limit": 20,
  "offset": 0
}
```

The exact `CommandResult` tags are documented in [core-domain.md](core-domain.md).

## Errors

`ApiError` maps domain errors:

| Domain | HTTP |
| --- | --- |
| Invalid IRI, prefix, validation, parse, query, update, config, invalid request, graph registry | 400 |
| NotFound | 404 |
| Conflict | 409 |
| GraphReadOnly | 403 |
| Extraction | 422 |
| Python | 503 |
| Unsupported | 501 |
| StoreOpen, Storage, Serialization, Io, GraphOpen, GraphPartial | 500 |

Body:

```json
{"error":{"kind":"not_found","message":"..."}}
```

Malformed JSON and extractor failures are handled by Axum's default rejection,
not this error shape.

## OpenAPI

`GET /api/v1/openapi.json` returns an OpenAPI 3.0.3 document with:

- most routes and methods in the main API;
- generic 200/400/401/404/409/500 responses;
- a bearer security scheme;
- skeletal `CommandResult` and `Error` schemas.

Limitations: request bodies/parameters are not described per route, response
bodies are generic, `/api/v1/system/metrics` is served but not listed, and the
document extraction routes are served but not described in `openapi.rs`.

## Metrics and rate limiting

`Metrics` uses relaxed atomic counters for completed requests and non-2xx
responses. `GET /api/v1/system/metrics` returns:

```json
{
  "requests": 12,
  "errors": 1,
  "rateLimitPerMinute": 60,
  "uptimeSeconds": 300
}
```

`RateLimiter` is a mutex-protected fixed window. Capacity is `max(1, limit)`.
After 60 seconds it resets. It is not distributed and not per API key.

## Tests

`tests/tests/phase4_api.rs` covers health/status, CRUD, commands, auth, errors,
and routes. `tests/tests/phase5_api.rs` covers metrics and rate limiting.
`tests/tests/phase8_extraction.rs` drives the document POST and GET routes end
to end with a fixture extractor.

```sh
cargo test -p ontology-api
cargo test -p ontology-integration-tests --test phase4_api
cargo test -p ontology-integration-tests --test phase5_api
```

## Limitations

- No TLS, CORS, compression, or request-size limit in the binary.
- No per-client or distributed rate limiting.
- No tenant-aware authorization.
- OpenAPI is descriptive rather than a full request/response contract.
- Document routes need a separately built Python feature and MinerU; a default
  build serves them as `501`.
- Synchronous storage work runs on the async request task.
- System graphs are not protected from generic SPARQL/clear-graph requests.

## Related

- Services: [application-service.md](application-service.md)
- Jobs: [background-jobs.md](background-jobs.md)
- Operations: [../docs/operations.md](../docs/operations.md)
- MCP remote mode: [mcp-server.md](mcp-server.md)
