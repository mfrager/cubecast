# MCP Server (`ontology-mcp`)

`ontology-mcp` exposes the application through exactly four MCP tools over
stdio: `rdf`, `ontology`, `semantic`, and `jobs`. It translates actions into
`Command` values, enforces an access policy, executes locally or forwards to the
HTTP API, audits calls, and compacts large results.

Source: `crates/ontology-mcp/src/`.

## Runtime modes

### Local

Without `ONTO_MCP_SERVER`, the binary:

1. loads `ONTO_CONFIG`;
2. validates the config;
3. initializes tracing;
4. reads the policy and namespace registry;
5. opens `OxigraphStore` with the configured storage;
6. bootstraps the internal schema;
7. builds `ApplicationService`;
8. serves stdio.

Local mode owns the RocksDB directory. Do not run it against a directory already
opened by the API.

### Remote

With `ONTO_MCP_SERVER`:

- `HttpService` forwards each command to `<base>/api/v1/commands`;
- `ONTO_API_KEY` supplies the bearer token;
- remote 404/409/501 responses become the corresponding domain errors, other
  failures become `Invalid`/`Storage`;
- local storage is not opened.

This is the recommended mode when the API is running.

## Tools and actions

### `rdf`

| Action | Purpose |
| --- | --- |
| `get` | Retrieve an instance |
| `search` | Search/list instances by type |
| `create` | Create from a `NewInstance` body |
| `update` | Patch from an `InstancePatch` body |
| `delete` | Delete an instance (destructive) |
| `query` | SPARQL SELECT/ASK/CONSTRUCT/DESCRIBE |
| `sparql_update` | SPARQL Update (destructive) |
| `import` | Parse inline RDF |
| `export` | Serialize dataset/graph |
| `inspect` | Relationships, or graphs without a target |
| `list_instances` | Paginated instance list |
| `list_graphs` | Named graphs |
| `statements` | Instance statements |
| `add_statements` | Add statement bodies |
| `delete_statements` | Delete by statement IDs (destructive) |
| `clear_graph` | Clear a graph (destructive) |

### `ontology`

| Action | Purpose |
| --- | --- |
| `schema` | Class/property/ontology CRUD via `kind` + `operation` |
| `validate` | Validate schema or graph |
| `scheme` | ConceptScheme CRUD |
| `concept` | Concept CRUD |
| `taxonomy` | Hierarchy, relations, move, validation, collections |
| `search` | Classes, properties, concepts, schemes |
| `external_import` | Import external ontology |
| `external_sync` | Re-fetch external ontology |
| `mapping` | Class/property mapping operations |
| `merge` | Merge schemes |

Compound actions use `kind` (`class`, `property`, `ontology`, `scheme`,
`concept`, `collection`, etc.) and `operation` (`get`, `list`, `create`,
`update`, `delete`, `add_relation`, `remove_relation`, `tree`, `ancestors`,
`descendants`, `move`, `validate`, `map_class`, `map_property`,
`list_class_mappings`, `list_property_mappings`, `validate_mappings`).

### `semantic`

`classify`, `classify_batch`, `embed`, `similarity`, `infer`, `preview`,
`apply`, `explain`, `reclassify`, `reembed`, `classifications`, `embeddings`,
`delete_embeddings`, `review`, `override`.

`background: true` converts eligible batch work into a job where supported.

### `jobs`

`submit`, `list`, `get`, `results`, `cancel`, `retry`. Submission uses `kind`
plus the corresponding fields; see [background-jobs.md](background-jobs.md).

## Arguments

All tools flatten a shared `CommonArgs` object. Important fields:

- `target` — primary IRI;
- `id` — secondary IRI (job/classification);
- `graph`, `target_graph`;
- `scheme`, `parent`, `class`, `property`, `instance`, `concept`, `ontology`;
- `source`, `external`, `internal`;
- `kind`, `operation`, `property_kind`, `mapping_kind`, `relation`;
- `format`, `data`, `sparql`, `query`, `label`, `language`;
- `body` — full resource document for create/update and compound actions;
- `instances`, `properties`, `attributes`, `strategies`, `strategy`;
- `types`, `members`, `top_concepts`, `broader`, `narrower`;
- `statement_ids`;
- `limit`, `offset`, `threshold`, `max_results`;
- `policy`, `policy_threshold`, `confidence`, `approved`;
- inference/classification toggles;
- `background`, `confirm`, `dry_run`.

Names are accepted as absolute IRIs or prefixed names resolved through the
configured registry. Every CRUD payload can also be provided as `body`.

## Safety policy

Defaults:

| Setting | Default | Effect |
| --- | --- | --- |
| `ONTO_MCP_ACCESS` | `read-only` | Writes denied unless `read-write` |
| `ONTO_MCP_ALLOW_DESTRUCTIVE` | false | Enables deletes, SPARQL Update, clear graph |
| `ONTO_MCP_ALLOW_EXTERNAL` | false | Enables external import/sync |
| `ONTO_MCP_MAX_RESULTS` | 100 | Result cap |
| `ONTO_MCP_MAX_EXPORT_BYTES` | 2097152 | Inline export cap |
| `ONTO_MCP_AUDIT_LOG` | unset | JSONL audit path |

`ONTO_MCP_ACCESS` accepts `read-only`, `readonly`, `read_only`, `read-write`,
`readwrite`, `read_write`, or `write`. Flags accept `1`, `true`, `yes`, or `on`.

### Write classification

Read-only-allowed examples:

- RDF get/list/query/export/inspect/statements/relationships;
- OWL/SKOS/external reads and validation;
- classify/classify_batch with `persist: false`;
- list classifications, similarity search, list embeddings;
- inference preview/explain;
- inference `apply` with `suggest`;
- job list/get/results/cancel/retry;
- submitting an RDF export job.

Writes include instance/schema/taxonomy/external changes, SPARQL Update, RDF
import, embedding generation/deletion, classification persistence, review,
override, reclassify, and inference apply with `review`/`automatic`.

In read-only mode, dispatch forces `ClassificationOptions.persist = false` even
if the caller omitted it or sent `true`.

### Destructive actions

Destructive means:

- RDF delete instance/statements, SPARQL Update, clear graph;
- OWL delete class/property/ontology;
- SKOS delete scheme/concept/collection, merge schemes;
- external delete/import/sync;
- delete embeddings.

These require **both** read-write and `ONTO_MCP_ALLOW_DESTRUCTIVE=true`, plus
inline `confirm: true`.

### External actions

External import/sync (direct or as a job) require
`ONTO_MCP_ALLOW_EXTERNAL=true` in addition to write/destructive permission.

### Confirmation

`confirm: true` is required for:

- every destructive command;
- inference `apply` with `review` or `automatic`;
- external import/sync jobs.

`review`/`automatic` inference is a write but is not in the "destructive" list;
it is confirmation-gated separately.

### Dry run

`dry_run: true` is evaluated **after** authorization. It returns the translated
command without executing it, but a write in read-only mode is still denied.
Use dry-run in read-write mode to inspect a command before adding `confirm`.

## Responses

Success:

```json
{
  "success": true,
  "tool": "rdf",
  "action": "get",
  "target": "urn:example:Product",
  "result": { "type": "resource", "..." : "..." },
  "metadata": {}
}
```

Dry run:

```json
{
  "success": true,
  "tool": "rdf",
  "action": "create",
  "target": "urn:example:Product",
  "dryRun": true,
  "command": { "family": "rdf", "command": { "..." : "..." } }
}
```

Error:

```json
{
  "success": false,
  "tool": "rdf",
  "action": "delete",
  "target": "urn:example:Product",
  "error": { "kind": "validation", "message": "..." }
}
```

The same JSON is returned as MCP structured content and as pretty-printed text
content. Tool-level errors set `is_error`.

Error kinds match `Error`: `invalid_iri`, `unknown_prefix`, `store_open`,
`storage`, `parse`, `serialization`, `query`, `update`, `not_found`, `conflict`,
`validation`, `invalid_request`, `config`, `io`, `unsupported`.

## Result compaction

| Result | Behavior | Metadata |
| --- | --- | --- |
| SPARQL solutions | First `max_results` rows | `count`, `returned`, `truncated` |
| SPARQL graph | First `max_results` quads | `count`, `returned`, `truncated` |
| Collection | First `max_results` items; `count` set to returned | `count` (original), `returned`, `truncated`, `limit`, `offset` |
| Export | Preview at `max_export_bytes` | `bytes`, `truncated`, `previewBytes` |
| Job | Full status | `job`, `state`, `progress` |
| Classifications/embeddings/similarity | Full list | `count` |

Classifications, embeddings, and similarity results are **not** truncated by
`max_results`; only their count is added.

Export truncation respects UTF-8 boundaries. A truncated export returns a
`preview` field instead of `data`.

## Audit

When `ONTO_MCP_AUDIT_LOG` is set, one JSON object per line is appended with:

- timestamp;
- tool and action;
- target;
- `command` as a debug-rendered string;
- outcome: `ok`, `dry_run`, `denied: ...`, `invalid: ...`, `error: ...`.

Audit failures are logged with `tracing::warn` and never fail the request.
There is no rotation, locking, or redaction. Because the debug rendering can
include inline RDF or SPARQL, protect the log as sensitive data.

## Protocol and server

- MCP transport: stdio only.
- Server name `onto-mcp`, version `0.1.0`.
- Exactly four tools are registered; the stdio test asserts this.
- Tool annotations are coarse: `rdf`, `ontology`, and `semantic` are marked
  destructive/non-read-only because they mix actions; `jobs` is marked
  idempotent and not destructive.
- MCP resources, prompts, sampling, and HTTP transport are not implemented.

## Tests

`crates/ontology-mcp/tests/stdio.rs` launches the binary and verifies:

- exactly four tools;
- read-only query success and write denial;
- write create/get;
- export job submission and polling;
- destructive confirmation refusal and success.

Unit tests cover dispatch, policy, compaction, and parameter flattening.

```sh
cargo test -p ontology-mcp
cargo bench -p ontology-mcp
```

## Limitations

- Stdio transport only.
- Environment-only policy; no per-client or per-tool grants.
- No OAuth/resource-server integration.
- Coarse tool annotations.
- `target` is an echo of the input, not a verified result target.
- Large classification/embedding lists bypass the result cap.
- Audit is best-effort and unredacted.
- Remote mode depends on the API's auth, limits, and error mapping.

## Related

- Safety/usage examples: [../docs/mcp.md](../docs/mcp.md)
- Commands: [application-service.md](application-service.md)
- HTTP remote mode: [http-api.md](http-api.md)
- Jobs: [background-jobs.md](background-jobs.md)
