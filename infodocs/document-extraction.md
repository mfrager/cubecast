# Document Extraction (`ontology-document`, `ontology-mineru`)

Document extraction turns a PDF or image into either a structured document, one
continuous text block, or both. The engine is MinerU, reached through an
optional PyO3 bridge; the canonical model and all storage logic stay in Rust.

The feature is off by default. Enabling it requires a build of `ontology-mineru`
with `--features python` and a Python environment containing MinerU.

## Crates and boundary

| Crate | Responsibility |
| --- | --- |
| `ontology-core` (`document`) | Modes, request/context/output types, canonical model, `DocumentExtractor` trait, cache identity |
| `ontology-document` | Schema dispatch, artefact parsing, normalisation, section/reference extraction, text projection, artefact store/cache, RDF mapping, `DocumentService` |
| `ontology-mineru` | Optional PyO3 bridge, manifest validation, path containment, shim invocation |
| `python/onto_mineru` | Python shim: one MinerU parse, artefact files, small JSON manifest |

No MinerU or PyO3 type appears in core or in the interfaces. The shim writes
artefacts to disk; Rust parses those files. Document JSON never crosses the FFI
boundary.

## Modes

`ExtractionMode` has exactly three values:

| Mode | Returns |
| --- | --- |
| `structured` | `document` only |
| `text` | `text` only |
| `both` (default) | Both projections from one parse |

`TextFormat` selects how `text.content` is rendered: `markdown` (headings and
paragraphs retained) or `plain` (formatting removed). `both` must return the
same text block as `text` for the same input and configuration.

`ExtractionOutput::validate()` enforces the contract after every run: the
requested projection is present, an unrequested projection is absent, and
provenance records the executed mode and format. A mismatch is
`Error::Extraction` and is never returned to a caller as a partial result.

## Canonical model

`StructuredDocument` is the loss-minimising representation:

```
StructuredDocument
  document_id       urn:sha256:<hex> of the source bytes
  metadata          title, authors, abstract, language, page count, file name
  pages             Page { index, width, height, unit, blocks }
  sections          Section { id, title, level, order, parent, blocks, page_start }
  references        Reference { id, index, raw_text, resolved? }
  citations         Citation { id, index, raw_text, reference?, block? }
  provenance        engine, versions, backend, tier, input schema, timing
```

Each `Page` contains `Block`s in reading order. A block has a stable IRI, a
`BlockKind`, normalised text, optional geometry, reading-order index, lines and
spans, section membership, caption, asset, table, formula, and code payload.
`Line` and `Span` preserve inline structure and style hints. Table cells carry
row/column coordinates, spans, and header flags; duplicate coordinates fail
validation.

Block kinds: title, heading (level 1–6), paragraph, list, figure, table,
formula, footnote, page header, page footer, reference, code, and unknown.
Unclassified MinerU blocks are preserved as `Unknown` with the original text,
not dropped.

## Request pipeline

`DocumentService::extract`:

1. Validate the source: readable file, non-empty, within
   `max_input_bytes`, supported extension (`pdf`, `png`, `jpg`, `jpeg`,
   `webp`, `tif`, `tiff`), and `%PDF-` magic for PDFs. Failures are
   `Error::Invalid`.
2. Hash the bytes and derive `document_id = urn:sha256:<hex>`.
3. Resolve mode, text format, and output directory from the request, then the
   configured defaults.
4. Unless `force` is set, look for a cached run matching document, mode,
   format, and (when known) engine version.
5. Acquire the `max_concurrency` semaphore and run the blocking extractor on
   `spawn_blocking`.
6. Persist the result through the artefact store, validate the mode, and write
   the RDF mapping when a store is attached.
7. Return `ExtractionOutput`.

`DocumentService::get` loads the most recent run for a document id on a blocking
task and returns `NotFound` when none exists.

## Artefacts and caching

Runs are content-addressed:

```
<output_dir>/sha256/<document-hex>/<run-hex>/
  manifest.json       # output without the model/text bodies
  middle.json         # raw MinerU artefact
  structured.json     # structured/both
  text.md | text.txt  # text/both, Markdown or plain
  images/...          # copied figures
```

`run_id` is deterministic: `urn:onto:extraction:<run-hex>`, derived from the
document id plus engine and shim versions, backend, tier, schema identifiers,
mode, and format. Identical requests therefore reuse the same run directory.

The manifest records the shim version, MinerU and Python versions, backend,
tier, schema identifiers, artefact paths, warnings, and timing. `force` bypasses
the lookup and re-runs the engine. Cache matching uses the observed MinerU
version only after the interpreter has run in the process; after an upgrade,
pass `force` once (see [../docs/operations.md](../docs/operations.md)).

## Text projection

The text projection is one continuous block, not a per-page concatenation with
page markers:

- headings become ATX headings with their level;
- paragraphs, list items, captions, and table Markdown are retained in reading
  order;
- figures and tables without text do not produce placeholder prose;
- `plain` strips the Markdown syntax after rendering.

The renderer is deterministic for the same canonical document, which is why
`both` and `text` can be asserted equal.

## Parsing and schema dispatch

`ontology-document` accepts the artefact families MinerU has emitted:

| Schema | Detected by |
| --- | --- |
| `docvortex.middle` | `schema` marker plus `schema_version` |
| `docvortex.middle.legacy` | `pdf_info` without a version marker |
| `docvortex.structured_content` | `pages[].blocks` / `para_blocks` |
| `docvortex.content_list` | flat array or `content_list` key |

Unknown schemas fail closed with `Error::Unsupported` naming the formats tried;
they are never guessed from a version string alone. The parser tolerates missing
optional fields, keeps unknown blocks, reconstructs table rows from HTML when
needed, and emits warnings such as `unknown_block_type`, `uncaptioned_figure`,
and `empty_table`.

## RDF mapping

A structured result maps into four named graphs:

```
urn:onto:data:document:<document-hex>:structure
urn:onto:data:document:<document-hex>:layout
urn:onto:data:document:<document-hex>:bibliography
urn:onto:data:document:<document-hex>:provenance
```

The mapping uses DOCO, BIBO, Schema.org, and PROV-O terms: document and pages,
block hierarchy, geometry and reading order, lines and spans, figures/assets,
tables/cells, sections, references, citations, and provenance. Mapping is
idempotent: an existing run is deleted and replaced in one `RdfStore`
transaction, with a direct delete/insert fallback for stores without
transactions.

In multi-graph registry mode these graph IRIs must already be registered on a
store; because they are allocated per document, enabling both features
currently makes the RDF-mapping step fail with `Error::GraphRegistry`. See
[multi-graph.md](multi-graph.md).

## Python boundary

`MinerUExtractor` implements `DocumentExtractor`. With the `python` feature it:

1. initialises the interpreter with the configured `venv` (prepending its
   `site-packages` when needed);
2. adds `shim_path` to `sys.path`;
3. calls `onto_mineru.bridge.extract(source, options)` exactly once;
4. validates the returned manifest;
5. resolves every reported path inside the temporary run directory, rejecting
   absolute paths outside it and `..` components;
6. parses the artefacts from disk.

Without the feature, `MinerUExtractor` exists but returns `Unsupported`, and
`ApplicationService::with_configured_documents` refuses to start when extraction
is enabled.

The shim contract is documented in [../docs/mineru-contract.md](../docs/mineru-contract.md).
`bridge.py` adapts to several MinerU entry points and filters keyword arguments
through `inspect.signature`, so a renamed parameter is reported instead of
crashing. The exact MinerU release and entry-point signature are still Phase 0
items because MinerU is not installed on the development host.

## Configuration

```toml
[extraction]
enabled = false
mode = "both"
text_format = "markdown"
output_dir = "./data/extractions"
shim_path = "./python"
# venv = "./.venv"
# model_source = "./models/mineru"
backend = "pipeline"
tier = "default"
max_input_bytes = 209715200
max_concurrency = 1
timeout_seconds = 900
keep_intermediates = false
max_text_bytes = 5242880
```

`Config::validate()` requires a non-zero `max_input_bytes`,
`max_concurrency`, `timeout_seconds`, and `max_text_bytes` when enabled, and
checks that `shim_path` exists plus any configured `venv` or `model_source`
directory. Concurrency defaults to one because MinerU is CPU/GPU heavy.

## Interfaces

| Surface | Shape |
| --- | --- |
| HTTP | `POST /api/v1/documents/extractions` with `ExtractionRequest`; `GET /api/v1/documents/{id}` |
| CLI | `onto document extract <file> [--mode …] [--format …] [--output-dir …] [--force]`; `onto document get <id>` |
| MCP | `rdf` tool, action `extract_document`, with `source`, `mode`, and `text_format` |
| Jobs | `JobRequest::DocumentExtract(ExtractionRequest)` → kind `document_extract` |

HTTP and CLI accept all request fields. MCP exposes only `source`, `mode`, and
`text_format`; it does not expose `output_dir` or `force`. Extraction is
classified as a write because it may write the RDF mapping, so MCP requires
`ONTO_MCP_ACCESS=read-write`; it is not destructive and needs no `confirm`.

## Errors

| Error | Meaning | HTTP |
| --- | --- | --- |
| `Invalid` | Unreadable, empty, too large, or unsupported source | 400 |
| `Extraction` | Parse, validation, artefact, or mapping stage failed | 422 |
| `Python` | Interpreter or MinerU unavailable | 503 |
| `Unsupported` | No extractor configured or feature not built | 501 |
| `Config` | Invalid extraction settings | 400 |

Failures are stage-tagged (`worker`, `parse`, `store`, `mode`, `mapping`, and
bridge stages), include the Python traceback when there is one, and never leave
a partially validated result visible as a successful run.

## Tests

- `ontology-document` unit tests (7): schema detection, content-list grouping,
  heading nesting, unknown-block preservation, text rendering, and model
  invariants.
- `crates/ontology-document/tests/extraction.rs` (6): fixture extraction in all
  modes, mode exclusion, caching/force, RDF mapping and idempotence, warnings.
- `tests/tests/phase8_extraction.rs` (5): application command path, text-only
  mode, jobs, HTTP extract/fetch, and the unsupported-when-disabled path.

No test runs a live MinerU parse. `python -m onto_mineru.selftest` is the
manual check once MinerU is installed, and `PYO3_PYTHON=… cargo check -p
ontology-mineru --features python` verifies the bridge compiles.

## Limitations

- MinerU is not installed on the development host; the engine version range and
  shim entry point are placeholders pending Phase 0 confirmation.
- A running Python parse cannot be cancelled or interrupted; cancellation is
  only observed between jobs.
- Cross-process cache matching cannot see the MinerU version until the
  interpreter has run, so an upgrade can reuse a stale run unless `force` is
  passed.
- MinerU's NLP Markdown is authoritative when the shim supplies it; otherwise
  `text.rs` renders a deterministic fallback from the canonical model. Both
  projections still come from one parse.
- Extraction concurrency defaults to one and there is no GPU scheduling or
  priority.
- The RDF mapping is incompatible with multi-graph registry mode for
  per-document graph IRIs (see above).

## Operations

Provisioning, the shim contract, and the Phase 0 checklist are in
[../docs/operations.md](../docs/operations.md) and
[../docs/mineru-contract.md](../docs/mineru-contract.md).

## Related

- Modes and command shapes: [core-domain.md](core-domain.md)
- Dispatch and jobs: [application-service.md](application-service.md),
  [background-jobs.md](background-jobs.md)
- Interfaces: [http-api.md](http-api.md), [cli.md](cli.md),
  [mcp-server.md](mcp-server.md)
- Graph caveat: [multi-graph.md](multi-graph.md)
