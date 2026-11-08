# MCP server

`ontology-mcp` is the agent-facing interface. It exposes the whole engine
through four tools instead of dozens of narrow endpoints, and every action is
translated into the same `Command` values used by the HTTP API and the CLI.

## Running

```sh
cargo run -p ontology-mcp
```

The server speaks MCP over stdio, so it is normally launched by an MCP client:

```json
{
  "mcpServers": {
    "onto": {
      "command": "/absolute/path/to/target/debug/ontology-mcp",
      "env": {
        "ONTO_CONFIG": "/absolute/path/to/config/ontology.toml",
        "ONTO_MCP_ACCESS": "read-only"
      }
    }
  }
}
```

Two execution modes are available:

- **Local** (default): open the configured RDF store directly.
- **Remote**: set `ONTO_MCP_SERVER=http://127.0.0.1:8080` to forward commands to
  a running API. This avoids a second process opening the same RocksDB
  directory while the API is running.

`ONTO_CONFIG` selects the configuration file, `ONTO_STORAGE_PATH` selects a
persistent store, and `ONTO_API_KEY` is used only in remote mode.

## Tools

All tools accept a common envelope:

```json
{
  "action": "classify",
  "target": "urn:example:product:123",
  "graph": "data:asserted",
  "scheme": "ex:ProductCategories",
  "options": {
    "strategy": "hybrid",
    "threshold": 0.82,
    "limit": 5
  }
}
```

The fields are flat; `target`, `graph`, `scheme`, `limit`, `offset`, `dry_run`,
and `confirm` are shared by every action. Each tool also exposes the fields its
actions need, such as `body`, `sparql`, `data`, `format`, `instances`,
`attributes`, `policy`, and `statement_ids`.

### `rdf`

RDF instances, statements, graphs, SPARQL, and bulk import/export.

| Action | Purpose |
| --- | --- |
| `get` | Retrieve one instance. |
| `search`, `list_instances` | List instances, optionally filtered by `type_filter`. |
| `create` | Create an instance from a `NewInstance` body. |
| `update` | Apply an `InstancePatch` body. |
| `delete` | Delete an instance and its statements. |
| `query` | Run SPARQL SELECT, ASK, CONSTRUCT, or DESCRIBE. |
| `sparql_update` | Run SPARQL Update. |
| `import` | Parse inline RDF; `background: true` submits a job. |
| `export` | Serialize the dataset or graph; `background: true` submits a job. |
| `inspect` | Relationships for a target, or all graphs without one. |
| `list_graphs`, `statements` | Inspect graphs and instance statements. |
| `add_statements`, `delete_statements` | Statement-level changes. |
| `clear_graph` | Remove every quad in a graph. |

### `ontology`

OWL, SKOS, taxonomies, external ontologies, and mappings.

| Action | Purpose |
| --- | --- |
| `schema` | Class/property/ontology CRUD via `kind` and `operation`. |
| `validate` | Validate the schema or a specific graph. |
| `scheme` | ConceptScheme CRUD and version history. |
| `concept` | Concept CRUD and search. |
| `taxonomy` | Tree, ancestors, descendants, relations, move, validate, merge. |
| `search` | Search classes, properties, or concepts. |
| `external_import`, `external_sync` | External ontology work. |
| `mapping` | Class/property mappings and validation. |
| `merge` | Merge a scheme into another. |

For `schema`, `kind` is `class`, `property`, or `ontology`, and `operation` is
one of `get`, `list`, `create`, `update`, `delete`, `hierarchy`, `versions`,
`applicable_properties`, or `applicable_classes`.

### `semantic`

Classification, embeddings, inference, enrichment, and explanations.

| Action | Purpose |
| --- | --- |
| `classify`, `classify_batch` | Assign SKOS concepts. |
| `reclassify` | Recompute automatic classifications. |
| `embed`, `reembed` | Generate attribute/aggregate embeddings. |
| `similarity` | Search for similar instances. |
| `infer`, `preview` | Preview inferred classes and properties. |
| `apply` | Apply inference under `suggest`, `review`, or `automatic`. |
| `explain` | Retrieve inference explanations. |
| `classifications`, `embeddings` | Inspect stored derived data. |
| `delete_embeddings` | Remove stored vectors. |
| `review`, `override` | Approve/reject or manually assign a classification. |

In read-only mode classification runs with persistence disabled. `apply` with a
non-`suggest` policy requires `confirm: true`.

### `jobs`

Submit and monitor long-running work.

| Action | Purpose |
| --- | --- |
| `submit` | Submit one of the eight job kinds. |
| `list`, `get`, `results` | Inspect progress and outputs. |
| `cancel`, `retry` | Manage non-terminal or failed work. |

Job kinds: `classify_batch`, `reclassify_batch`, `embed_batch`, `infer_batch`,
`rdf_import`, `rdf_export`, `external_import`, and `external_sync`.

## Response shape

```json
{
  "success": true,
  "tool": "semantic",
  "action": "classify",
  "target": "urn:example:product:123",
  "result": [
    {
      "concept": "ex:NetworkingEquipment",
      "confidence": 0.94,
      "strategy": "hybrid"
    }
  ],
  "metadata": {
    "count": 1
  }
}
```

Large query results and collections are truncated to the configured limit and
carry `metadata.truncated: true` plus the original `metadata.count`. Large
exports become a preview instead of an invalid partial RDF document; use a
background export job to retrieve the full document.

Errors are tool-level structured errors, not protocol failures:

```json
{
  "success": false,
  "error": {
    "kind": "validation",
    "message": "rdf create requires a full resource in \"body\""
  }
}
```

## Safety

The server is read-only by default.

| Variable | Default | Effect |
| --- | --- | --- |
| `ONTO_MCP_ACCESS` | `read-only` | `read-write` enables graph changes. |
| `ONTO_MCP_ALLOW_DESTRUCTIVE` | `false` | Enables deletes and SPARQL Update. |
| `ONTO_MCP_ALLOW_EXTERNAL` | `false` | Enables external imports and syncs. |
| `ONTO_MCP_MAX_RESULTS` | `100` | Maximum returned rows/items. |
| `ONTO_MCP_MAX_EXPORT_BYTES` | `2097152` | Inline export size cap. |
| `ONTO_MCP_AUDIT_LOG` | unset | JSONL file receiving every tool call. |

Even with destructive operations enabled, a request must include
`"confirm": true`. `dry_run: true` returns the translated command without
executing it, which is the recommended way for an agent to inspect a change
before asking for approval. Inference `apply` with `review` or `automatic`
also requires confirmation.

External imports and syncs also require `"confirm": true`. Audit records
contain the timestamp, tool, action, target, rendered command, and outcome
(`ok`, `denied`, `invalid`, `dry_run`, or an error).

## Agent workflows

### Ontology discovery

```json
{"action": "search", "kind": "class", "query": "Product", "limit": 20}
```

Call the `ontology` tool, then inspect a class and its applicable properties:

```json
{"action": "schema", "kind": "class", "operation": "hierarchy", "target": "ex:Product"}
```

```json
{"action": "schema", "kind": "class", "operation": "applicable_properties", "class": "ex:Product"}
```

### Instance enrichment

```json
{"action": "get", "target": "urn:example:product:123"}
```

```json
{"action": "classify", "target": "urn:example:product:123", "scheme": "ex:ProductCategories", "threshold": 0.6}
```

```json
{"action": "infer", "target": "urn:example:product:123", "include_external": true}
```

```json
{"action": "apply", "target": "urn:example:product:123", "policy": "review"}
```

The last call returns the approval requirement; only a request with
`"policy": "automatic"` and `"confirm": true` writes statements.

### Taxonomy management

```json
{"action": "scheme", "operation": "get", "target": "ex:ProductCategories"}
```

```json
{"action": "concept", "operation": "create", "target": "ex:Laptops", "scheme": "ex:ProductCategories", "label": "Laptops", "language": "en", "broader": ["ex:Computers"]}
```

```json
{"action": "taxonomy", "operation": "validate", "scheme": "ex:ProductCategories"}
```

### Bulk processing

```json
{"action": "submit", "kind": "rdf_import", "format": "turtle", "data": "<urn:s> <urn:p> \"v\" ."}
```

Poll with `{"action": "get", "id": "urn:onto:job:..."}` until the state is
terminal, then use `results` for the output.
