# Document Extractor — Design and Implementation Plan

Rust · MinerU · PyO3 · DoCO/BIBO/Schema.org · Oxigraph

Status: proposed
Scope: MinerU ingestion, the canonical document model, structured and text-only
extraction modes, the PyO3 boundary, artefact storage, RDF mapping
Basis: `design/document_extractor1.md` (canonical RDF model),
`design/document_extractor2.md` (single continuous text), `docs/operations.md`

---

## 1. Objective

Add a document extractor that turns a PDF into either:

1. **Structured output** — page → block (title, heading, paragraph, list,
   figure, table, formula, footnote, reference) → line → span, with PDF
   geometry, reading order, and a logical section tree.
2. **Single-block text** — one continuous, reading-order text string that keeps
   headings and paragraphs and drops visual blocks, as described in
   `document_extractor2.md`.
3. **Both**, derived from the same parse so the two views cannot disagree.

Extraction runs through **MinerU**, invoked from Rust through **PyO3**: the Rust
process embeds a CPython interpreter, imports a thin Python shim, and calls it.
The shim owns everything MinerU-specific; Rust owns the output contract, the
document model, mode selection, caching, and (Phase 4) RDF mapping.

The mode is always explicit in the request: `structured`, `text`, or `both`.
The result type carries only what was asked for, and a validation step enforces
that invariant.

Non-goals: replacing MinerU's OCR/layout models, running remote MinerU,
multi-engine consensus, and the scientific-claims ontology deferred by
`document_extractor1.md` §16.

---

## 2. Design Inputs and Decisions

| Decision | Choice | Reason |
| --- | --- | --- |
| Extraction engine | MinerU, pinned per release | Layout, OCR, tables, formulas, reading order |
| Rust ↔ Python | PyO3 0.30 embedding (`Python::initialize` / `Python::attach`) | In-process call, no shell parsing, no HTTP hop |
| MinerU API surface | A repository-owned Python shim (`python/onto_mineru`) | MinerU's Python API and output schemas change; the churn stays in one file |
| Primary MinerU artefact | `middle_json` (`docvortex.middle`) | Keeps hierarchy, spans, coordinates (`document_extractor1.md` §2) |
| Text source | MinerU NLP Markdown rendered from the same parse | One continuous string, images/tables skipped (`document_extractor2.md`) |
| Rust/Python payload | Shim writes artefacts to disk and returns a small JSON manifest | Large JSON never crosses the FFI boundary twice |
| Canonical model | Typed Rust structures in `ontology-core::document` | Interfaces see domain types, never PyO3 or MinerU types |
| Mode contract | `structured` / `text` / `both`, enforced by `ExtractionOutput::validate` | Requested explicitly; no silent extra work |
| Artefact store | Content-addressed directory per document + run | Deterministic, re-runnable, keeps extraction history |
| RDF mapping | DoCO + BIBO + Schema.org + PROV-O, application namespace for the rest | `document_extractor1.md` §3–§10 |
| RDF writing | Through `RdfStore`, never Oxigraph directly | Existing layering (`RdfStore` is the only storage boundary) |
| PyO3 crate | Optional `python` feature, off by default | `cargo test --workspace` stays buildable without libpython |

### Workspace constraints this plan inherits

- Rust 1.88, edition 2021, `unsafe_code = "forbid"` at workspace level. The
  PyO3 embedding API used here is safe Rust; no `#[pymodule]`, no `unsafe`
  block, so the lint stays satisfied.
- `ontology-core` owns shared types and capability traits; adapters are separate
  crates. `Error` is the single error type and gains a few coarse variants.
- Interfaces (HTTP/CLI/MCP) call `OntologyService`; they do not reach into
  adapters directly.
- Background work goes through `JobRequest`; job state is in-process.
- The repository has no SHACL processor. Validation in this plan is Rust-side
  shape checking plus a shipped `shapes.ttl` for external validators.

---

## 3. Extraction Contract

### 3.1 Request

```rust
// crates/ontology-core/src/document.rs

/// What the caller wants back from one extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionMode {
    /// Page/block/line hierarchy only.
    Structured,
    /// One continuous text block only.
    Text,
    /// Both, from a single MinerU parse.
    Both,
}

/// Format of [`TextOutput::content`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextFormat {
    /// MinerU NLP Markdown: headings and paragraphs retained, figures and
    /// tables omitted.
    #[default]
    Markdown,
    /// Markdown formatting stripped, plain UTF-8 text.
    Plain,
}

/// One extraction request, fully describing the work so a failed job can be
/// retried without the caller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractionRequest {
    /// PDF (or MinerU-supported image) to extract.
    pub source: PathBuf,
    /// Overrides `[extraction].mode`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<ExtractionMode>,
    /// Overrides `[extraction].text_format`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_format: Option<TextFormat>,
    /// Overrides `[extraction].output_dir`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<PathBuf>,
    /// Re-run even when a matching cached run exists.
    #[serde(default)]
    pub force: bool,
}
```

Precedence is request → config → default. The resolved values are echoed in
`ExtractionOutput`, so a caller never has to reconstruct them.

### 3.2 Result

```rust
/// Result of one extraction run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractionOutput {
    /// `urn:sha256:<hex>` of the source bytes.
    pub document_id: String,
    /// `urn:onto:extraction:<hex>` of this run (engine + config + input).
    pub run_id: String,
    /// Mode that was actually executed.
    pub mode: ExtractionMode,
    /// Present for `structured` and `both`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<StructuredDocument>,
    /// Present for `text` and `both`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextOutput>,
    /// Files written for this run, with hashes.
    pub artefacts: ArtefactSet,
    /// Engine, versions, schema identifiers, timings.
    pub provenance: ExtractionProvenance,
    /// Non-fatal findings (uncaptioned figure, unparsed block, …).
    #[serde(default)]
    pub warnings: Vec<ExtractionWarning>,
}

/// One continuous text block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextOutput {
    pub content: String,
    pub format: TextFormat,
    pub char_count: usize,
}
```

### 3.3 Guarantees

| Mode | `document` | `text` | MinerU artefacts required |
| --- | --- | --- | --- |
| `structured` | `Some` | `None` | `middle.json`, assets |
| `text` | `None` | `Some` | `middle.json` + NLP Markdown |
| `both` | `Some` | `Some` | as above |

`ExtractionOutput::validate()` returns `Error::Extraction` when a required side
is missing, when a side is present that the mode did not ask for, or when
`mode` disagrees with the populated fields. Every constructor path calls it, so
"as specified" is a testable property rather than a convention.

For `both`, the text is rendered from the same MinerU parse result as the
structure. A test asserts `text` is byte-identical to the `text`-mode output for
the same source and engine configuration.

---

## 4. Canonical Document Model

The model mirrors `document_extractor1.md` but stops before RDF: it is pure
Rust/Serde, valid to serialize over HTTP and to map to RDF later.

Two hierarchies coexist and are never collapsed:

- **Physical**: `Page` → ordered `Block`s → `Line`s → `Span`s, with geometry
  and reading order.
- **Logical**: `Section` tree → block ids, with section level and order.
  Membership is a reference on the block, so a paragraph that starts on page 3
  and continues on page 4 stays one block with two geometry entries.

```rust
pub struct StructuredDocument {
    pub document_id: String,
    pub metadata: DocumentMetadata,
    pub pages: Vec<Page>,
    pub sections: Vec<Section>,
    pub references: Vec<Reference>,
    pub citations: Vec<Citation>,
    pub provenance: ExtractionProvenance,
}

pub struct DocumentMetadata {
    pub title: Option<String>,
    pub authors: Vec<Author>,
    /// Abstract as extracted, not generated.
    pub abstract_text: Option<String>,
    pub language: Option<String>,
    pub page_count: u32,
    pub source_file_name: Option<String>,
}

pub struct Page {
    pub index: u32,              // zero-based, as MinerU reports it
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub unit: Option<String>,    // e.g. "pt"
    pub blocks: Vec<Block>,      // physical reading order
}

pub struct Block {
    pub id: String,              // urn:onto:document:<doc>/block/<n>
    pub kind: BlockKind,
    pub text: String,            // normalized text; empty for pure images
    pub geometry: Option<Geometry>,
    pub reading_order: u32,      // within the page
    pub lines: Vec<Line>,
    pub section: Option<String>, // logical membership, if known
    pub caption: Option<Caption>,
    pub asset: Option<AssetRef>,
    pub table: Option<Table>,
    pub formula: Option<Formula>,
    pub list: Option<ListBlock>,
}

#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockKind {
    Title,
    Heading { level: u8 },
    Paragraph,
    List,
    ListItem,
    Figure,
    Table,
    Formula,
    Footnote,
    PageHeader,
    PageFooter,
    Reference,
    Code,
    /// Preserved so unknown MinerU types are never dropped.
    Unknown { label: String },
}

pub struct Line { pub text: String, pub geometry: Option<Geometry>, pub spans: Vec<Span> }
pub struct Span { pub text: String, pub geometry: Option<Geometry>, pub style: Option<String> }

pub struct Geometry {
    pub x0: f64, pub y0: f64, pub x1: f64, pub y1: f64,
    pub unit: Option<String>,
    pub origin: Option<String>,   // "top-left" for MinerU page coordinates
}

pub struct Section {
    pub id: String,
    pub title: Option<String>,
    pub level: u8,
    pub order: u32,
    pub parent: Option<String>,
    pub blocks: Vec<String>,
    pub page_start: Option<u32>,
}

pub struct Reference {
    pub id: String,
    pub index: u32,
    pub raw_text: String,
    pub resolved: Option<ResolvedWork>,
}

pub struct Citation {
    pub id: String,
    pub index: u32,
    pub raw_text: String,
    pub reference: Option<String>,
    pub block: Option<String>,
}
```

`Table` carries `rows: Vec<TableRow>` with explicit `row_index`,
`column_index`, `is_header`, spans, and the raw HTML/Markdown when MinerU
provides it. `Formula` carries `latex`, optional `mathml`, `formula_type`
(inline/interline), and the asset fallback. `Figure`/`Table` keep
`caption: Option<Caption>` and `asset: Option<AssetRef>`; `AssetRef` is a
content hash plus relative path, never a bare filesystem path.

Model invariants checked by a `StructuredDocument::validate()` pass:

- `pages` are ordered by `index` and contain no duplicate indices;
- every page block's `reading_order` is unique within the page;
- block/section/reference/citation ids are unique;
- `section` references resolve to a section that lists the block;
- `geometry` is non-inverted (`x0 <= x1`, `y0 <= y1`);
- table cells have unique `(row, column)` pairs within a table;
- a `Figure` has an asset or a caption (neither is a warning, not an error).

---

## 5. MinerU Boundary

### 5.1 One parse, three projections

The shim always runs **one** MinerU parse per request. Mode only selects which
projections Rust reads:

```
PDF ──► MinerU parse ──┬─► middle.json  ──► structured model
                        ├─► NLP Markdown ──► single text block
                        └─► image assets ──► content-addressed copies
```

For `both`, the Markdown is rendered from the same parse result (`render` over
the parse output), not by re-parsing. If a MinerU release can only render by
re-parsing, the shim renders from the already-saved `middle.json`; it never
invokes the layout/OCR pipeline twice.

### 5.2 Python shim

Repository path: `python/onto_mineru/`.

```python
# python/onto_mineru/bridge.py
def extract(source: str, options: dict) -> dict:
    """Run one MinerU parse and return a manifest.

    options keys: output_dir, mode, text_format, backend, tier,
                  model_source, timeout_seconds, extra
    Raises ExtractionError with a stable `code` on failure.
    """
```

The manifest is deliberately small and versioned:

```json
{
  "shim_version": "1",
  "mineru_version": "x.y.z",
  "python_version": "3.12.9",
  "backend": "pipeline",
  "tier": "default",
  "schema": "docvortex.middle",
  "schema_version": "2.0",
  "middle_json": "/abs/run/middle.json",
  "markdown": "/abs/run/text.md",
  "content_list": null,
  "assets": [
    {"path": "/abs/run/images/figure-1.png", "mime": "image/png", "page": 2}
  ],
  "warnings": []
}
```

Rust reads `middle_json`/`markdown` from disk and parses them. This keeps the
FFI payload to a few hundred bytes regardless of document size, and makes
fixture-based testing possible without Python.

### 5.3 Schema dispatch

`document_extractor1.md` §2 warns that a version number alone does not identify
the format. Rust therefore dispatches on an explicit discriminator, in this
order:

1. `middle_json` with `schema`/`schema_version` (or the documented
   `docvortex.middle` marker) → structured parser.
2. `structured_content.json` → simplified parser (fallback only).
3. `content_list.json` V2 (page-grouped) → flat fallback.
4. legacy middle (no schema marker) → legacy parser.
5. anything else → `Error::Unsupported` naming every format that was tried.

The parser selected and the observed schema/version are recorded in
`ExtractionProvenance` (`input_schema`, `input_schema_version`), matching the
provenance fields required by `document_extractor1.md` §8.

### 5.4 What the normaliser must preserve

- Original block order and page index, even when MinerU's inferred reading
  order differs.
- Unknown block types, under `BlockKind::Unknown`, with their raw text.
- Captions as separate resources linked to their figure/table.
- Raw reference text, independent of any resolved publication.
- Coordinates and page dimensions when present, without converting units or
  clamping to page bounds.
- Confidence values when MinerU exposes them, tagged with a source identifier.

Conversely it must not invent: a `text` block is not automatically a
`Paragraph` if it is in the header/footer/reference band; classification uses
context as `document_extractor1.md` §10 requires.

---

## 6. PyO3 Integration

### 6.1 Dependencies and build

```toml
# crates/ontology-mineru/Cargo.toml
[features]
default = []
python = ["dep:pyo3"]

[dependencies]
pyo3 = { version = "0.30", optional = true, features = ["auto-initialize"] }
```

PyO3 0.30 is current and provides `Python::initialize()` and
`Python::attach(|py| …)` (`with_gil` was renamed in 0.29). `auto-initialize`
covers tests; production code still calls `Python::initialize()` explicitly once
per process to control failures.

The `python` feature is **off by default**: `cargo test --workspace` then needs
no libpython, no venv, and no MinerU. The crate compiles to a thin
`PyMinerU` that returns `Error::Unsupported` with the exact enable instructions.
CI adds a dedicated job that builds with `--features python` inside a prepared
venv; the production build always enables it.

Build-time interpreter selection uses `PYO3_PYTHON=/path/to/venv/bin/python`.
Runtime discovery is documented, not guessed.

### 6.2 Interpreter setup once per process

```rust
static INIT: OnceLock<Result<(), String>> = OnceLock::new();

fn ensure_initialized(venv: Option<&Path>, shim: &Path) -> Result<()> {
    INIT.get_or_init(|| {
        Python::initialize();          // idempotent
        Python::attach(|py| {
            let sys = py.import("sys")?;
            // Shim directory first, so `import onto_mineru` resolves here.
            add_sys_path(py, &sys, shim)?;
            // An embedded interpreter does not honour a venv's pyvenv.cfg;
            // add its site-packages explicitly.
            if let Some(venv) = venv {
                add_venv_site_packages(py, &sys, venv)?;
            }
            Ok(())
        })
    }).clone()
}
```

`add_venv_site_packages` imports `site` and calls `site.addsitedir` for each
`<venv>/lib/python*/site-packages` (and `<venv>/Lib/site-packages` on Windows).
This is the classic embedding pitfall: linking libpython from a venv does **not**
put that venv's packages on `sys.path`.

Failure modes are distinct and actionable:

| Failure | Error |
| --- | --- |
| No libpython at build/runtime | `Error::Python { detail }` with the `PYO3_PYTHON` hint |
| Shim directory or module missing | `Error::Python` naming the path and module |
| MinerU not installed in the venv | `Error::Python` naming the venv |
| MinerU raised | `Error::Extraction { stage: "mineru", detail }` with the Python traceback, truncated |
| Manifest unreadable/invalid | `Error::Extraction { stage: "manifest", detail }` |

### 6.3 Calling the shim

```rust
fn run_shim(python: &PyMinerU, request: &ResolvedRequest) -> Result<Manifest> {
    Python::attach(|py| {
        let module = py.import("onto_mineru.bridge")?;
        let options = to_py_dict(py, &request.shim_options())?;
        let manifest = module
            .getattr("extract")?
            .call1((request.source.to_string_lossy(), options))?;
        let json: String = py.import("json")?.call_method1("dumps", (manifest,))?
            .extract()?;
        serde_json::from_str(&json).map_err(Error::extraction_manifest)
    })
}
```

Rules:

- **GIL scope is one call.** No Python object or GIL token crosses an `.await`,
  a thread boundary, or a returned struct.
- The Python callback runs inside `tokio::task::spawn_blocking`; the async
  service never blocks a runtime worker.
- Extraction concurrency is capped by `[extraction].max_concurrency` (default
  `1`; MinerU is CPU/GPU-heavy) and by the existing blocking-task limit.
- The interpreter is process-wide; concurrent extractions serialize on the GIL
  anyway, which is why the semaphore defaults to one.
- No `detach` is needed: the shim is Python-bound (layout, OCR, table models).

### 6.4 Timeout and cancellation

PyO3 cannot interrupt a running Python call. The plan is honest about that:

- `timeout_seconds` is enforced by the shim **before** heavy phases where MinerU
  exposes checkpoints, and by Rust as a post-return check of elapsed time.
- A job cancellation marks the job `cancelled`; the result of an in-flight parse
  is discarded, but the parse itself runs to completion.
- Hard cancellation is out of scope for this phase. If it becomes necessary, the
  adapter can move to a supervised child process (still driving MinerU through a
  Python entry point) without changing `DocumentExtractor`; that is a Phase 6
  option, not part of this plan.

### 6.5 Untrusted input

- Reject files above `[extraction].max_input_bytes` (default 200 MiB) before
  initializing anything.
- Validate the extension/magic (`%PDF-` for PDFs) and pass only canonicalised
  paths to Python.
- Paths returned by the shim are canonicalised and must remain inside the
  per-run directory; otherwise `Error::Extraction` (path escape). Symlinks are
  resolved before the check.
- `model_source` defaults to the local pre-provisioned directory; network model
  download is opt-in per configuration, never on by default.
- Extraction happens in-process, so the service runs unprivileged with CPU/memory
  limits supplied by the deployment (documented in `docs/operations.md`).

---

## 7. Text Mode

`text` mode uses MinerU's NLP Markdown renderer (`MakeMode.NLP_MD` in the
versions described by `document_extractor2.md`):

- all pages in reading order, one string;
- headings and paragraphs retained;
- images, tables, and other visual blocks skipped;
- formulas retained as text/LaTeX where MinerU extracted them.

`TextFormat::Plain` then strips Markdown deterministically:

- heading markers (`#`, `##`, …) and setext underlines removed;
- emphasis/strong/code markers removed, their text kept;
- link syntax reduced to the label;
- consecutive blank lines collapsed to one;
- no sentence re-flowing or re-wrapping, so character offsets stay meaningful.

The raw Markdown is always kept as `text.md` in the run directory, even when the
requested `format` is `Plain`, so the conversion can be audited and re-run.

No page-text concatenation is performed by Rust; MinerU's renderer is
authoritative, which is exactly the option `document_extractor2.md` recommends.

---

## 8. Artefact Store, Caching, Determinism

### 8.1 Layout

```
<output_dir>/
└── sha256/<doc-hex>/
    ├── source.json                 # original path, size, hash, mime
    └── <run-hex>/
        ├── manifest.json           # ExtractionOutput without the model body
        ├── middle.json             # MinerU raw, unchanged
        ├── text.md                 # only for text/both
        ├── text.txt                # only for Plain
        ├── images/…                # copied assets, content-addressed names
        └── structured.json         # canonical model (structured/both)
```

`structured.json` is a convenience for retrieval; the model is also returned in
memory and (Phase 4) written to RDF.

### 8.2 Identity

- `document_id = "urn:sha256:" + hex(sha256(source bytes))`
- `run_id = "urn:onto:extraction:" + hex(sha256(
      document_id ‖ mineru_version ‖ shim_version ‖ backend ‖ tier ‖
      schema_version ‖ mode ‖ text_format ))`

Re-extraction with a new MinerU version or different mode produces a new
`run_id`; old runs are retained, matching `document_extractor1.md` §9. The same
input and configuration always produce the same ids and the same model bytes
(JSON object keys are stable; collections are ordered explicitly).

### 8.3 Cache

Before invoking Python, the service looks for `<doc>/<run>/manifest.json`. A hit
returns the stored output unless `force` is set. A cache hit is reported in
provenance (`cached: true`), so callers can distinguish it from a parse. A
missing artefact referenced by an existing manifest is a cache miss, not an
error.

Temporary MinerU output is written under the run directory and removed on
failure, so a failed run never leaves a partially valid cache entry.

---

## 9. RDF Mapping (Phase 4)

The mapping follows `document_extractor1.md`; this section fixes the names and
the integration points the workspace already has.

### 9.1 Namespaces

| Prefix | Namespace |
| --- | --- |
| `doco` | `http://purl.org/spar/doco/` |
| `bibo` | `http://purl.org/ontology/bibo/` |
| `schema` | `https://schema.org/` |
| `dcterms` | `http://purl.org/dc/terms/` |
| `foaf` | `http://xmlns.com/foaf/0.1/` |
| `prov` | `http://www.w3.org/ns/prov#` |
| `oa` | `http://www.w3.org/ns/oa#` |
| `doc` | `urn:onto:document:` (application extension) |

`doc:` replaces the example domain in `document_extractor1.md`; it is stable and
under this system's control.

### 9.2 Resource IRIs

| Resource | IRI |
| --- | --- |
| Document | `urn:onto:document:<doc-hex>` |
| Page | `urn:onto:document:<doc-hex>/page/<n>` |
| Block | `urn:onto:document:<doc-hex>/block/<n>` |
| Section | `urn:onto:document:<doc-hex>/section/<n>` |
| Figure / table / formula | `…/figure/<n>`, `…/table/<n>`, `…/formula/<n>` |
| Reference / citation | `…/reference/<n>`, `…/citation/<n>` |
| Extraction run | `urn:onto:extraction:<run-hex>` |
| Geometry | `…/block/<n>/geometry/<revision>` |

Numbers are the stable extraction indices, not RDF triple order. Multiple runs
share the document IRI and separate run resources, so history is additive.

### 9.3 Graphs

| Graph | Contents |
| --- | --- |
| `urn:onto:data:document:<doc-hex>:structure` | DoCO sections, paragraphs, figures, tables |
| `urn:onto:data:document:<doc-hex>:bibliography` | BIBO/Schema.org metadata, authors, references, citations |
| `urn:onto:data:document:<doc-hex>:layout` | Pages, ordering, geometry, lines, spans |
| `urn:onto:data:document:<doc-hex>:provenance` | PROV-O activity, versions, confidence, warnings |

Revision separation uses `extraction/runs` links rather than graph-name suffixes
in the first version: one structure graph holds the accepted run, and provenance
records supersession. If competing revisions must coexist, the `run-hex` is
appended to the graph name; that is a documented option, not the default.

All writes go through `RdfStore::transaction()` when available, so a document is
either fully mapped or not mapped at all.

### 9.4 Mapping rules

| Model | RDF |
| --- | --- |
| `StructuredDocument` | `a bibo:AcademicArticle, schema:ScholarlyArticle`; `dcterms:title`, `schema:headline`, `schema:abstract`, `schema:datePublished` when known |
| `Author` | `schema:Person, foaf:Person`; `schema:name`; ordered via `doc:authorList` + `doc:position` |
| `Page` | `doc:Page`; `doc:pageIndex`, `doc:pageWidth`, `doc:pageHeight`, `doc:coordinateUnit` |
| `Block::Paragraph` | `doco:Paragraph`; `doc:text`; `doc:belongsToSection`, `doc:appearsOnPage`, `doc:readingOrderIndex` |
| `Block::Heading` | `doco:Section`; `dcterms:title`, `doc:sectionLevel`, `doc:sectionIndex` |
| `Block::Figure` | `doco:Figure`; caption (`doco:Caption`), `schema:ImageObject` representation |
| `Block::Table` | `doco:Table, schema:Table`; `doc:TableStructure`, rows and cells with explicit indices |
| `Block::Formula` | `doc:Formula`; `doc:latex`, `doc:mathml`, `doc:formulaType`, image fallback |
| `Block::Footnote` | `doco:Footnote` |
| `Block::List` | `doco:List` + `doco:ListItem`, ordered |
| `Reference` | `doco:BibliographicReference`; `doc:rawReferenceText`; optional `doc:referencesPublication` |
| `Citation` | `doc:CitationOccurrence`; `doc:citesReference`, `doc:appearsInParagraph` |
| Geometry | `doc:BoundingBox` with `doc:x0/y0/x1/y1`, unit, origin, page |
| Provenance | `prov:Activity`, `prov:SoftwareAgent`, `prov:used`, `prov:generated`; version/schema fields as `doc:` |

Per-element provenance is added only when it differs from the run's activity
(corrected caption, second engine), which avoids four extra triples per
paragraph while keeping the audit path intact.

### 9.5 Validation

Rust checks, in this order, before writing:

1. `StructuredDocument::validate()` (model invariants above).
2. Mapping completeness: every block produced at least one quad; every asset
   reference resolves to a file and a hash.
3. Reference/citation integrity: citations point at an existing or explicitly
   unresolved reference.
4. Duplicate IRI detection across the four graphs.

`shapes.ttl` is shipped next to the mapping code for external SHACL validators;
the service does not claim to run SHACL itself. Hard failures abort the
transaction; warnings (uncaptioned figure, unresolvable reference) are kept in
provenance and returned in `ExtractionOutput::warnings`.

---

## 10. Configuration

```toml
[extraction]
enabled = true
mode = "both"                    # structured | text | both
text_format = "markdown"         # markdown | plain
output_dir = "./data/extractions"
shim_path = "./python"
venv = "./.venv"                 # optional; site-packages added explicitly
backend = "pipeline"
tier = "default"
model_source = "./models/mineru" # local; no download by default
max_input_bytes = 209715200      # 200 MiB
max_concurrency = 1
timeout_seconds = 900
keep_intermediates = false

[documents.extraction]           # optional UI/API defaults
max_text_bytes = 5242880         # cap on inline text in HTTP responses
```

`ExtractionConfig` lives in `ontology-core::config`, is `#[serde(default)]`, and
`Config::validate` gains:

- `enabled` requires an existing `shim_path`, an existing `output_dir` (or a
  creatable parent), and a usable `venv` when set;
- `max_input_bytes`, `max_concurrency`, `timeout_seconds` must be non-zero;
- `mode`/`text_format` must deserialize (Serde enforces this);
- `model_source` must exist when `enabled` and `backend` requires local models.

Absent `[extraction]` keeps the feature off and changes nothing else, exactly
like `[graphs]` in the multi-graph plan.

---

## 11. Crate and File Layout

```
crates/ontology-core/src/
  document.rs          # modes, request/output, model, DocumentExtractor trait
  config.rs            # ExtractionConfig + validation
  error.rs             # Extraction / Python variants

crates/ontology-document/
  src/lib.rs
  src/parse/mod.rs
  src/parse/schema.rs          # discriminator + dispatch
  src/parse/middle_json.rs     # docvortex.middle -> model
  src/parse/structured_content.rs
  src/parse/content_list.rs
  src/parse/legacy_middle.rs
  src/normalize.rs             # block classification, section tree, ordering
  src/text.rs                  # NLP Markdown -> plain text
  src/artefacts.rs             # run dirs, manifest, hashing, cache
  src/service.rs               # async DocumentService (spawn_blocking, semaphore)
  src/rdf/mod.rs               # Phase 4 mapping
  src/rdf/namespaces.rs
  src/rdf/validate.rs
  assets/shapes.ttl
  tests/fixtures/mineru/…      # anonymised middle.json, text.md, assets

crates/ontology-mineru/
  src/lib.rs                   # PyMinerU: DocumentExtractor impl
  src/pyo3_bridge.rs           # interpreter init, sys.path, call, manifest
  src/manifest.rs              # manifest schema (serde)

python/onto_mineru/
  __init__.py
  bridge.py                    # extract(source, options) -> manifest
  errors.py
  selftest.py                  # contract check without a PDF

tests/tests/phase8_extraction.rs
```

`ontology-document` has no PyO3 dependency and no MinerU types. `ontology-mineru`
depends on `ontology-document`; `ontology-application` depends on both.

The capability trait stays next to the other capability traits:

```rust
// ontology-core::document
pub trait DocumentExtractor: Send + Sync {
    /// Runs one extraction. Callers execute this in a blocking worker.
    fn extract(&self, request: &ExtractionRequest) -> Result<ExtractionOutput>;
}
```

---

## 12. Implementation Phases

### Phase 0 — MinerU contract spike (timeboxed)

The plan deliberately does not hard-code MinerU's current Python API. Before
writing the bridge, verify on the target machine:

1. Pin a MinerU release in `python/constraints-mineru.txt`; record
   `mineru --version`.
2. Confirm the parse entry point and that `middle.json` matches the
   `docvortex.middle` shape from `document_extractor1.md` (schema identifier,
   version, pages, blocks, spans, bboxes).
3. Capture a two-page PDF extraction as anonymised fixtures for
   `crates/ontology-document/tests/fixtures/mineru/`.
4. Confirm the NLP Markdown renderer name and that it excludes figures/tables.
5. Record the supported CPython range; pick the venv interpreter accordingly and
   set `PYO3_PYTHON`.

Deliverable: `docs/mineru-contract.md` with the versions, the shim signature,
the manifest keys, and the fixtures. If a step fails, only the shim and the
parser dispatch change; the model and modes do not.

**Acceptance:** a shell command in the contract note reproduces the fixture from
the PDF with the pinned MinerU version.

### Phase 1 — Domain model and modes

- Add `ontology-core::document` (modes, request/output, model, trait, errors).
- Add `ExtractionConfig` and validation.
- Add `ontology-document` with model validation and the JSON serialization shape.
- Tests: mode invariants, model invariants, config validation.

**Acceptance:** `ExtractionOutput::validate()` rejects each wrong mode/field
combination; no Python is involved.

### Phase 2 — MinerU ingestion (no Python)

- Implement the schema dispatcher and the `middle_json` parser.
- Normalise blocks, lines, spans, pages, sections, tables, formulas and
  references from the Phase 0 fixtures.
- Implement unknown-block preservation and warning collection.
- Fixture tests assert the full model for `structured` mode.

**Acceptance:** `cargo test -p ontology-document` parses the fixture into a
document with correct page/block/line counts, order, and geometry, and round-trips
`structured.json` unchanged.

### Phase 3 — PyO3 bridge and text mode

- `ontology-mineru` with optional `python` feature, interpreter init, venv
  `site-packages` handling, shim call, manifest validation, path containment.
- `python/onto_mineru/bridge.py`: one parse, write artefacts, return manifest.
- Markdown/plain text conversion and the "text is identical in `text` and
  `both`" test.
- Run through `DocumentService` with the concurrency semaphore and cache.
- Contract test: `python -m onto_mineru.selftest` (imports MinerU, prints the
  version, validates the manifest schema) — gated, not part of `--workspace`.

**Acceptance:** with `--features python` and a prepared venv, a real PDF extracts
in all three modes; without the feature, the workspace still builds and tests,
and `PyMinerU` returns `Error::Unsupported` with enable instructions.

### Phase 4 — Artefacts, caching and RDF mapping

- Content-addressed run layout, manifest, cache hit/miss, `force`.
- Deterministic IRIs and the four-graph mapping through `RdfStore`.
- Mapping validation and `shapes.ttl`.
- Goldens: canonical N-Quads stable across runs; repeated extraction produces
  no duplicate triples.

**Acceptance:** extracting the fixture twice yields identical quads and a cache
hit; a corrupted manifest is a miss, not an error; every block maps.

### Phase 5 — Application, API, CLI, jobs, MCP

- `DocumentCommand::Extract` in `ontology-core::command`; `Command::Document`.
- `JobRequest::DocumentExtract`, `total_items = 1`, cancellation semantics as in
  §6.4; result is the `ExtractionOutput` summary.
- HTTP: `POST /api/v1/documents/extractions` (multipart path or server-side
  path), `GET /api/v1/documents/{id}` for stored metadata. Text returned inline
  is capped by `max_text_bytes`; artefacts are exposed through an authenticated
  download route with an allow-listed root.
- CLI: `onto document extract --mode structured|text|both --format markdown|plain`.
- MCP: `rdf` tool gains action `extract_document`, returning the mode, ids, and
  a preview; the full text is fetched by id if needed.
- Wire `DocumentService` into `ApplicationService` as `Option<Arc<DocumentService>>`
  so `[extraction]` disabled keeps today's behaviour.
- `docs/operations.md`: venv/model provisioning, artefact retention, limits.

**Acceptance:** an end-to-end integration test extracts a fixture PDF via the
service and reads it back through the API and CLI; MCP returns a job handle for
slow runs.

### Phase 6 (optional) — hardening

- Supervised worker process for hard timeout/cancellation, if operationally
  required.
- Page-range extraction and batch imports, if requested later.
- Benchmark: pages/second for the pinned model set; memory ceiling per document.

---

## 13. Testing Strategy

| Layer | Test | Needs Python |
| --- | --- | --- |
| Model | mode invariants, id uniqueness, geometry bounds, table cell indices | no |
| Parsers | golden `middle.json` → expected model; unknown blocks; missing bbox; legacy dispatch | no |
| Text | Markdown → plain conversion table; reading order; no figures/tables leaked | no |
| Cache | run-id stability, hit/miss, corrupt manifest, partial run cleanup | no |
| PyO3 | interpreter init, missing venv, missing shim, Python exception → `Error`, path escape | yes |
| End-to-end | fixture PDF in all three modes; `both` text equals `text` | yes |
| RDF | N-Quads goldens, determinism, transactional abort on invalid mapping | no |
| Interfaces | API extract + fetch, CLI flags, MCP action, job cancel/retry | yes (or mocked trait) |

Fakes: `DocumentService` takes `Arc<dyn DocumentExtractor>`, so interface tests
use a fixture-backed fake and run without Python. The real `PyMinerU` is covered
by the gated job only.

Fixture policy: every fixture is anonymised, checked in, and small (two pages);
the PDF itself is generated from the fixture text, not a real paper.

---

## 14. Error Additions

```rust
/// Document extraction failed.
#[error("document extraction failed at {stage}: {detail}")]
Extraction { stage: &'static str, detail: String },

/// The Python runtime or MinerU installation is unavailable.
#[error("Python/MinerU unavailable: {detail}")]
Python { detail: String },
```

No new variant per MinerU error; the shim's stable `code` and the traceback go in
`detail`. `Error::Unsupported` continues to mean "feature not compiled in".

---

## 15. Risks and Mitigations

| Risk | Mitigation |
| --- | --- |
| MinerU schema/API drift | Shim isolation, schema discriminator, fixture replay, pinned version, fail-closed dispatch |
| Python version mismatch (e.g. interpreter newer than MinerU supports) | Phase 0 records the supported range; `PYO3_PYTHON` pins it; CI uses the same venv |
| libpython missing on developer machines | `python` feature off by default; actionable `Unsupported` message |
| Large PDFs exhaust memory | `max_input_bytes`, one concurrent parse, per-run temp cleanup, deployment limits |
| No hard cancellation | Documented; job marks cancelled and discards the result; Phase 6 subprocess option |
| Model weights downloaded unexpectedly | Local `model_source` by default; download is opt-in |
| Text and structure disagree | Both derived from one parse; equality test |
| RDF duplication on re-extraction | Transactional replace per graph + deterministic IRIs; tests assert idempotence |
| Interface layer reaching into PyO3 | `DocumentExtractor` trait in core; only `ontology-mineru` imports PyO3 |

---

## 16. Definition of Done

- `structured`, `text`, and `both` all work on the fixture PDF and on a real
  paper; each result validates against its mode.
- `both` returns byte-identical text to `text` mode for the same configuration.
- Unknown MinerU block types survive into the model and produce warnings.
- Re-running extraction is a cache hit with stable ids; `force` creates a new
  run without deleting the old one.
- With `--features python` the workspace tests and clippy pass in the prepared
  venv; without the feature, `cargo test --workspace`, `cargo clippy
  --workspace --all-targets`, and `cargo fmt --all -- --check` pass unchanged.
- `docs/operations.md` documents venv/model provisioning, artefact retention,
  and the no-hard-cancel limitation.

## 17. Open Questions for Phase 0

1. Which MinerU release and CPython version are the supported pair for this
   deployment? (Pin both in `python/constraints-mineru.txt`.)
2. Does the pinned release expose `docvortex.middle` directly, or only the
   legacy/structured-content variants? (Selects the primary parser.)
3. Does NLP Markdown rendering reuse the parse result in that release, or does
   it require a second pass? (Affects the `both` cost, not the contract.)
4. Are model weights provisioned at build time or mounted at runtime?
