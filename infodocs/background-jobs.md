# Background Jobs (`ontology-application::jobs`)

Jobs move long-running batch/semantic work off the request path. The registry
and worker pool are in-process, bounded, cancellable, and retryable.

Source: `crates/ontology-application/src/jobs.rs`, with the job model in
`crates/ontology-core/src/job.rs`.

## Job types

`JobRequest` is tagged by `kind` and carries its complete payload:

| Kind | Payload |
| --- | --- |
| `classifyBatch` | `instances`, `ClassificationOptions` |
| `embedBatch` | `instances`, `attributes` |
| `inferBatch` | `instances`, `InferenceOptions`, `ApplyPolicy` |
| `reclassifyBatch` | `instances`, `ClassificationOptions` |
| `rdfImport` | `format`, `data`, optional `graph` |
| `rdfExport` | `format`, optional `graph` |
| `externalImport` | `ImportRequest` |
| `externalSync` | external ontology `id` |

Because the payload is stored with the job, retry can re-run it without the
original request.

## States

```
queued -> running -> completed
                  -> failed
                  -> cancelled
       -> cancelled (cancelled before a worker starts)
failed/cancelled -> queued (retry)
```

`JobState::is_terminal()` is true for completed, failed, and cancelled.

`JobStatus`:

| Field | Meaning |
| --- | --- |
| `id` | `urn:onto:job:<ULID>` |
| `kind` | stable request kind |
| `state` | lifecycle state |
| `progress` | `0.0..=1.0` |
| `processed`, `total` | batch items processed / expected |
| `attempts` | retry count |
| `message` | queued/running/completed/cancellation detail |
| `error` | failure detail |
| `result` | serialized result |
| `created_at`, `started_at`, `finished_at` | timestamps |

## Manager

```rust
let manager = JobManager::new(context, concurrency);
```

`JobContext` holds shared handles to classification, embedding, inference, the
RDF store, and the external registry. The application service uses concurrency
**4**.

The manager stores `JobRecord { status, request, cancel }` in a
`RwLock<BTreeMap<Id, JobRecord>>`. A Tokio `Semaphore` bounds concurrent
execution.

`submit` requires a Tokio runtime; outside one it returns `Unsupported`.
It creates the ID/status, inserts the record, and spawns a task. The task waits
for a permit, marks running, executes, and writes the terminal state.

## Progress and results

Batch jobs report after each instance: `processed = index + 1`,
`progress = processed / total`, capped at 1.0. Single-unit jobs
(import/export/external) report `total = 1`; completion sets
`processed = total = 1`, `progress = 1.0`.

Results are serialized JSON:

| Job | Result |
| --- | --- |
| classify/reclassify | flattened classification records |
| embed | embedding records |
| infer | inference reports |
| RDF import | `{ "imported": n }` |
| RDF export | `{ "format": ..., "data": ... }` |
| external import/sync | `ImportReport` |

`get`, `list`, and `results` all return statuses. `results` does not extract only
the `result` field; the CLI and callers read the field from the status.

## Cancellation

`cancel(id)`:

- unknown → `NotFound`;
- already terminal → `Conflict`;
- otherwise sets the cancel flag and immediately marks the status `cancelled`
  with `finished_at` and `"cancellation requested"`.

Workers check the flag before each item and before starting after a permit.
Cancellation is cooperative: a long single operation cannot be interrupted
mid-call. A running job that completes before observing the flag can overwrite
the cancelled terminal state with completed.

## Retry

`retry(id)` requires a failed or cancelled job:

- resets progress/processed/error/result/timestamps;
- increments `attempts`;
- installs a fresh cancel flag;
- sets state `queued` with message `"queued"`;
- re-spawns using the stored request.

`created_at` is retained. Completed/running jobs cannot be retried.

## Listing and lookup

- `get` returns a cloned status or `NotFound`.
- `list` returns all statuses oldest first by `created_at`, then ID.
- There is no pruning, retention limit, or persistence.

## Storage and concurrency invariants

- Job IDs are generated ULIDs.
- The registry lock is poisoned-safe only in the sense that errors are returned
  for get/list/cancel/retry; worker `mark` silently skips a poisoned lock.
- Concurrency is at least one (`concurrency.max(1)`).
- Jobs share the application's store and engines; there is no isolation or
  priority.
- Two writes to the same graph can interleave at the operation level.

## HTTP, CLI, and MCP surfaces

HTTP:

- `GET /api/v1/jobs`
- `POST /api/v1/jobs`
- `GET /api/v1/jobs/{id}`
- `DELETE /api/v1/jobs/{id}` (same as cancel)
- `GET /api/v1/jobs/{id}/results`
- `POST /api/v1/jobs/{id}/cancel`
- `POST /api/v1/jobs/{id}/retry`

CLI: `onto job` with submit helpers, list/get/results/cancel/retry/watch.

MCP: the `jobs` tool with submit/list/get/results/cancel/retry. Submissions of
external import/sync are separately gated and confirmed; RDF import is a write;
RDF export is allowed read-only.

## Tests

`tests/tests/phase4_jobs.rs` covers submission, progress, results, cancellation,
retry, and all job kinds. MCP stdio tests cover export job polling.

```sh
cargo test -p ontology-application
cargo test -p ontology-integration-tests --test phase4_jobs
```

## Limitations

- In-process only; restart loses all jobs and results.
- No durable queue, worker recovery, leases, or distributed execution.
- Cooperative cancellation only.
- No automatic retry/backoff; retry is manual.
- No retention or cleanup policy.
- Results can be large and are held in memory.
- A cancelled running job can race with completion.

## Related

- Job model: [core-domain.md](core-domain.md)
- Semantic commands: [application-service.md](application-service.md)
- HTTP endpoints: [http-api.md](http-api.md)
- Operations: [../docs/operations.md](../docs/operations.md)
