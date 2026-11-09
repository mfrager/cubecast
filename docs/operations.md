# Operations

Deployment, health, backup, and the production limits of the current build.

## Deployment

```sh
cargo build --release
```

The workspace produces two service binaries:

- `target/release/ontology-api` — HTTP API and background jobs.
- `target/release/ontology-mcp` — stdio MCP server.

Use one writer per RocksDB directory. If the API is running against a
persistent store, run MCP in remote mode:

```sh
ONTO_MCP_SERVER=http://127.0.0.1:8080 \
ONTO_API_KEY=secret \
target/release/ontology-mcp
```

Otherwise MCP can own the store directly:

```sh
ONTO_CONFIG=/etc/onto/ontology.toml \
ONTO_MCP_ACCESS=read-only \
target/release/ontology-mcp
```

Configuration is TOML; `ONTO_CONFIG` selects the file. `Config::validate` runs
at startup and rejects zero page/blocking limits, invalid namespace prefixes,
and a persistent store without a path.

## Health and readiness

| Endpoint | Purpose |
| --- | --- |
| `GET /api/v1/system/health` | Public liveness probe. |
| `GET /api/v1/system/status` | Authenticated status: version, uptime, auth, job count. |
| `GET /api/v1/system/metrics` | Authenticated completed-request/error counters and uptime. |
| `GET /api/v1/openapi.json` | Public OpenAPI document. |

The API shuts down gracefully on `SIGINT`/`SIGTERM`; the MCP server exits when
its stdio transport closes.

## Rate limiting and quotas

`ONTO_RATE_LIMIT=<requests per minute>` enables a global fixed-window limit on
authenticated API routes. Rejected requests get `429` and `Retry-After: 60`.
MCP has its own result limits:

- `ONTO_MCP_MAX_RESULTS` (default 100) caps query rows and collection pages.
- `ONTO_MCP_MAX_EXPORT_BYTES` (default 2 MiB) caps inline RDF exports.

The API limiter is process-local. Deployments with multiple API processes need
a shared limiter at the gateway.

## Backup and recovery

The storage adapter exposes `OxigraphStore::backup(path)`, which uses the
engine's consistent backup operation. The simplest operational procedure is:

1. Stop the API/MCP writer.
2. Run a backup (or copy the RocksDB directory while no process has it open).
3. Store the backup with the configuration file and any external ontology
   sources used by the registry.
4. To recover, restore the directory to `storage.path` and start one process.

For portable exports, use the HTTP API or CLI:

```sh
onto export --format nquads --output backup.nq
onto import --format nquads --file backup.nq
```

RDF export is a logical backup; it does not preserve RocksDB engine state or
in-process job history.

## Multi-store backup and restore

When `[graphs]` is enabled, `storage.path` is the main store directory and every
satellite has its own directory under `[graphs].root`. `GraphAdmin::backup`
backs up the main store and every ready satellite into one target directory:

```text
backup/
  manifest.json          # store ids, roles, layouts, quad counts, engine revision
  00-main/
  01-products/
```

The RDF/XML registry file is authoritative and is **not** copied into the
backup; archive it alongside the backup directory. A restore is:

1. Stop the API/MCP process so no store is open.
2. Restore `00-main` to `storage.path` and each satellite directory to its
   registered path, keeping the `graph.meta.json` marker inside each directory.
   Marker mismatches are rejected at startup, which catches directories moved
   between stores by mistake.
3. Restore the registry file that produced the backup and start one process.
4. Verify with `graph status`/`GraphAdmin::status`: every store should report
   `ready` and the counts should match `manifest.json`.

Satellites marked `onto:required false` may be restored later; the backend
reports them as `degraded` and queries return an empty branch for their graphs
until the directory is back. Required satellites abort startup when missing.

## Jobs

Jobs run in-process with bounded concurrency, cancellation, progress, retry,
and result retrieval. A restart clears job history because the registry is
memory-backed; completed exports should be retrieved before shutdown. Retry is
available for failed and cancelled jobs while the process is running.

## Security checklist

- Set `ONTO_API_KEY` on any non-loopback API listener.
- Leave MCP at `read-only` unless the agent workflow needs writes.
- Enable `ONTO_MCP_ALLOW_DESTRUCTIVE` only for trusted agents and require
  `confirm: true` on destructive calls.
- Enable `ONTO_MCP_ALLOW_EXTERNAL` only when imports are expected.
- Set `ONTO_MCP_AUDIT_LOG` to a durable path and ship the JSONL records.
- Put TLS and network access control in front of the API; the binary serves
  plain HTTP.

## Benchmarks

```sh
cargo bench -p ontology-rdf      # storage insertion/query
cargo bench -p ontology-mcp      # MCP action translation
```

The MCP benchmark measures translation overhead, not model or network latency.
Measure end-to-end tool latency from the client when tuning agent workflows.

## Known limitations

- API rate limiting is a single-process fixed window.
- Job state and history are in-process.
- MCP local mode and the API cannot open the same RocksDB directory at once.
- MCP export responses are truncated with a preview above the byte limit; use a
  background export job for the full document.
- In registry mode, a transaction is buffered and routed on commit; a crash
  between stores can leave a cross-store transaction partially applied.
  Keep related graphs in one store when that matters.
- Cross-store queries are not one snapshot: each store is read independently.

## Document extraction (MinerU)

Extraction is off unless `[extraction].enabled = true`. It needs a build of
`ontology-mineru` with the `python` feature and a Python environment containing
the pinned MinerU release:

```bash
python3 -m venv .venv
./.venv/bin/pip install -r python/constraints-mineru.txt
PYO3_PYTHON=./.venv/bin/python cargo build --release -p ontology-mineru --features python
```

The embedded interpreter does not inherit a venv's `pyvenv.cfg`; the bridge
adds `<venv>/lib/python*/site-packages` explicitly. Set `PYO3_PYTHON` to the same
interpreter used to create the venv.

Model weights are not downloaded implicitly. Provision them and point
`[extraction].model_source` at the directory. `python -m onto_mineru.selftest`
checks the interpreter, MinerU version, and manifest contract.

Artefacts are written to `output_dir/sha256/<document>/<run>/` with
`manifest.json`, `middle.json`, `structured.json`, and `text.md`. Re-running an
unchanged document with the same configuration is a cache hit; `force = true`
(or `--force`) re-parses and replaces that run.

Limitations:

- MinerU runs in-process through PyO3. A running parse cannot be interrupted;
  cancelling a job discards its result but the parse still finishes.
- Extraction concurrency defaults to one because MinerU is CPU/GPU heavy.
- Without the `python` feature, document commands return `unsupported` with the
  rebuild instructions above.

Cache entries are keyed by document, mode, text format, and (once the
interpreter has run at least once in the process) the MinerU version. After an
upgrade, pass `--force` once to avoid reusing a run produced by the previous
version; the cached manifest always records the version that produced it.
