# Semantic Registry and Project Layer — Design and Implementation Plan

Rust · Oxigraph (`basenote` fork) · DCAT 3 · ADMS · OWL 2 · SKOS · SHACL · PROV-O

Status: proposed
Scope: ontology catalog, immutable releases, analysis, validation, publication, project
management, and the HTTP/CLI/MCP surfaces
Design inputs: `semantic_registry1.md`, `semantic_registry2.md`

---

## 1. Objective

Build a semantic registry that manages ontologies, vocabularies, schemas, and their
releases as first-class RDF resources, then add a project-management layer that
connects the work of producing those assets to the assets themselves.

The registry has four separate concerns, and the implementation must keep them
separate:

1. **Catalog** — DCAT catalog, records, assets, and discovery metadata.
2. **Ontology** — the authoritative RDF content of each release.
3. **Release** — an immutable, versioned publication with distributions and hashes.
4. **Analysis** — derived metadata about terms, structure, imports, compatibility,
   validation, and provenance.

The project layer adds a fifth concern:

5. **Work** — projects, goals, requirements, tasks, deliverables, milestones,
   resources, risks, decisions, and acceptance evidence.

A project task is not an ontology release. A deliverable is not a release. A
deliverable *realizes* an asset or targets a release. That distinction is enforced
in the types, the RDF model, and the graph layout.

### 1.1 Definition of success

- A user can register a local or HTTP ontology source.
- The registry identifies the ontology family and version, parses all supported RDF
  syntaxes, and creates a release candidate without publishing it.
- The candidate carries a term index, statistics, import graph, structural diff,
  validation report, and provenance.
- Publishing produces immutable release content, distributions, hashes, and a
  catalog entry; re-publishing the same bytes is idempotent, and publishing
  different bytes under an existing version is a conflict.
- A project can require an ontology capability, create work to produce it, link a
  deliverable to the release, accept it against evidence, and record a snapshot.
- HTTP, CLI, and MCP all use the same application commands and permissions.

### 1.2 Non-goals for the first delivery

- A general OWL reasoner. Compatibility is structural unless a profile validation
  explicitly checks an application graph.
- Unrestricted recursive import fetching. Imports resolve against registered
  releases first; remote fallback is allow-listed and opt-in.
- Distributed transactions across multi-graph satellite stores.
- A general workflow engine, scheduler, or optimization solver. Project plans are
  RDF descriptions; execution evidence is PROV-O.
- Per-user or per-asset ACLs. The existing bearer-token boundary remains the
  authentication model for this plan.
- S3/object storage. The artifact store starts on the filesystem behind a trait.
- Rewriting existing external ontology imports. They remain available and are
  bridged later.

---

## 2. Starting Point and Constraints

| Fact | Consequence |
| --- | --- |
| `ontology-core` owns all engine-independent domain types and the `Command`/`CommandResult` boundary | Registry and project types belong in core modules; transports do not get new business logic |
| `RdfStore` is the storage boundary; Oxigraph types never escape `ontology-rdf` | Registry repositories are written against `RdfStore` and `RdfTransaction` |
| `ontology-graph` routes registered graphs to the main store or satellites | Registry graphs created at runtime need an explicit main-owned dynamic-prefix rule |
| `ontology-import` already parses RDF, discovers `owl:Ontology`, counts classes/properties, and stores external imports | Extract reusable parsing/statistics helpers instead of writing a second RDF parser |
| `ontology-import` does not fetch HTTP sources (`read_source` returns `Unsupported`) | Remote ingestion is new work and must be opt-in, capped, and allow-listed |
| `ontology-owl` manages classes/properties in `system:schema`, not arbitrary ontology files | Do not load releases into `system:schema`; release graphs are separate and immutable |
| The workspace has no SHACL processor | Validation starts with deterministic Rust checks; SHACL is an adapter selected by a Phase R0 spike |
| `RdfFormat` already supports Turtle, N-Triples, N-Quads, TriG, RDF/XML, and JSON-LD | Distribution generation can cover the full set without a new parser |
| `ApplicationService` already composes OWL, SKOS, external, classification, embedding, inference, documents, and jobs | Registry and project services join that composition root |
| HTTP is Axum 0.8 without multipart; CLI is a remote client; MCP has four tools | Uploads require an Axum feature and a CLI upload path; MCP gains tools rather than overloading `rdf` indefinitely |
| API path parameters currently hold raw IRIs | Registry routes use opaque keys for path segments because release IRIs commonly contain `/` |

### 2.1 Decisions taken by this plan

1. **New domain crates, not a second application.** Add `ontology-registry` and
   `ontology-project`; do not create the twelve-crate `registry-*`/`project-*`
   tree from the designs. Modules inside each crate are split when they earn it.
2. **Registry metadata and project data live in the main store.** All registry and
   project graphs are main-owned, including runtime-created release graphs. The
   multi-graph backend remains available for external data stores.
3. **Registry content is authoritative in RDF; artifacts are immutable files.**
   The catalog graph never absorbs the ontology's triples, and analysis graphs never
   masquerade as the release.
4. **The existing external import registry is not the release registry.** It stays
   for compatibility; registry ingestion creates immutable releases. A later bridge
   can import existing external records as catalog assets.
5. **Use standard vocabularies where they fit.** DCAT 3, ADMS, OWL, SKOS, PROV-O,
   VoID, PROF, and SHACL carry their normal meanings. A small `oreg:` namespace is
   only for missing registry concepts; `pm:` is only for missing project concepts.
6. **IRIs are public, keys are for paths.** API paths address assets/releases by
   stable hash keys; bodies and results carry full IRIs. This avoids slash-encoded
   version IRIs breaking routing.

---

## 3. Target Architecture

```
HTTP API              CLI `onto`              MCP `registry` / `project`
     \                   |                         /
      \                  |                        /
       +-------- ApplicationService -------------+
       |  Command::Registry / Command::Project   |
       |  + existing OWL/SKOS/RDF/semantic/jobs  |
       +------------------+----------------------+
                          |
          +---------------+----------------+
          |                                |
  ontology-registry                 ontology-project
  catalog, ingest, analysis,        projects, work, goals,
  validation, publish               deliverables, governance
          |                                |
          +---------------+----------------+
                          |
                    Arc<dyn RdfStore>
                          |
             ontology-graph (main + satellites)
                          |
             +------------+-------------+
             |                          |
        main store                 artifact store
   registry/project graphs       sha256 files on disk
```

### 3.1 Dependency direction

```
ontology-core
   ↑          ↑
ontology-rdf  |
   ↑          |
ontology-graph
   ↑
ontology-import     ontology-owl     ontology-skos
   ↑                    ↑                ↑
   +-------- ontology-registry ---------+
                    ↑
             ontology-project
                    ↑
          ontology-application
             ↑        ↑        ↑
      ontology-api  ontology-cli  ontology-mcp
```

- `ontology-registry` may use `ontology-rdf` for parsing/serialization helpers and
  `ontology-import` only for extracted source/statistics helpers.
- `ontology-project` depends on a core trait `SemanticAssetCatalog`, implemented by
  `ontology-registry`; it never depends on the registry crate directly.
- `ontology-application` owns the concrete composition and exposes commands.
- No transport crate parses registry files or writes registry graphs directly.

### 3.2 Crate and module layout

```
crates/ontology-core/src/
├── registry.rs          # catalog/release/analysis/validation/project types
├── registry_graph.rs    # fixed and dynamic graph IRI constructors
├── project.rs           # project/work/goal/deliverable types and traits
├── command.rs           # RegistryCommand, ProjectCommand, Command variants
├── job.rs               # RegistryIngest, RegistryAnalyze, ProjectSnapshot
├── config.rs            # RegistryConfig, ProjectConfig
├── error.rs             # Registry and Project variants
├── namespace.rs         # dcat, adms, prof, void, oreg, pm prefixes
└── lib.rs               # re-exports

crates/ontology-registry/src/
├── lib.rs               # RegistryService, re-exports
├── catalog.rs           # catalog/asset/release metadata repository
├── identity.rs          # family/version/release keys and IRI rules
├── artifact.rs          # content-addressed store, atomic writes
├── source.rs            # local/HTTP source readers and limits
├── ingest.rs            # parse, identify, candidate creation
├── terms.rs             # term extraction and deterministic term records
├── analysis.rs          # statistics, modules, reports
├── dependencies.rs      # owl:imports resolution, lock manifest, cycles
├── diff.rs              # structural release comparison
├── validation.rs        # structural checks, Validator trait, profile records
├── compatibility.rs     # change classification and report generation
├── publish.rs           # immutable publication, distributions, current pointer
├── query.rs             # catalog/term/release search
└── service.rs           # RegistryService command implementation

crates/ontology-project/src/
├── lib.rs               # ProjectService, re-exports
├── repository.rs        # RDF encoding/decoding per project graph
├── projects.rs          # project, goal, requirement CRUD
├── work.rs              # work packages, tasks, milestones, dependencies
├── deliverables.rs      # deliverable links, acceptance, evidence
├── plans.rs             # P-Plan records and task-to-plan links
├── governance.rs        # risks, issues, decisions, baselines
├── snapshots.rs         # immutable project snapshots
└── report.rs            # overview, timeline, impact queries
```

If `ontology-registry` becomes too large, split analysis and publication into
separate crates only after the interfaces stabilize. Do not split on day one.

### 3.3 Storage partitions

All IRIs below are main-owned. Fixed graphs are registered at startup; release,
analysis, validation, and staging graphs are runtime-created under a main-owned
dynamic prefix.

| Graph | Contents | Mutability |
| --- | --- | --- |
| `urn:onto:registry:catalog` | `dcat:Catalog`, `dcat:CatalogRecord`, families, releases, distributions, candidates, profiles, ingestion records | mutable catalog metadata; releases never deleted |
| `urn:onto:registry:release:<key>` | parsed RDF of one published release | immutable |
| `urn:onto:registry:analysis:<key>` | term index, statistics, module records, dependency report, diff reports | replaceable derived data |
| `urn:onto:registry:validation:<run>` | validation run and report | append-only |
| `urn:onto:registry:profiles` | profile descriptions and pointers to shapes artifacts | mutable |
| `urn:onto:registry:provenance` | PROV-O activities for ingest, analyze, validate, publish | append-only |
| `urn:onto:registry:staging:<candidate>` | candidate RDF before publication | temporary |
| `urn:onto:project:core` | projects and project metadata | mutable |
| `urn:onto:project:goals` | goals, objectives, success criteria | mutable |
| `urn:onto:project:requirements` | requirements, acceptance criteria, constraints | mutable |
| `urn:onto:project:work` | phases, work packages, tasks, subtasks, milestones, dependencies | mutable |
| `urn:onto:project:deliverables` | deliverables, asset links, acceptance evidence | mutable |
| `urn:onto:project:resources` | teams, roles, assignments, allocations | mutable |
| `urn:onto:project:governance` | risks, issues, decisions, approvals | mutable |
| `urn:onto:project:plans` | project plans, P-Plan structures, plan approvals | mutable |
| `urn:onto:project:events` | snapshots and project event records | append-only |

`<key>` is the first 16 hex characters of a SHA-256 over the stable release
identity (`family IRI | version`). `<run>` and `<candidate>` use ULIDs. None of
these keys is user-controlled path data.

### 3.4 Main-owned dynamic graphs in multi-graph mode

`ontology-graph` currently routes only graphs present in the registry snapshot. A
runtime-created release graph would fall outside the catalog. Phase R1 adds a
`onto:dynamicPrefix` property to the main store registration (and a built-in
default for `urn:onto:registry:` and `urn:onto:project:`), with these rules:

- An unknown graph IRI under a main-owned dynamic prefix routes to the main store.
- Dynamic prefixes may not overlap `urn:onto:system:` or another store's namespace.
- `GraphAdmin::status()` reports dynamic graph counts separately.
- `validate()` checks that every persisted registry/project graph matches a
  declared prefix.
- Single-store mode is unaffected; every graph already lives in main.

This is a prerequisite for multi-graph deployments, not an optional polish item.

---

## 4. Vocabulary and Data Model

### 4.1 Namespaces

Add to `NamespaceRegistry::with_builtins`:

| Prefix | Namespace | Use |
| --- | --- | --- |
| `dcat` | `http://www.w3.org/ns/dcat#` | catalogs, datasets, series, distributions |
| `adms` | `http://www.w3.org/ns/adms#` | semantic asset lifecycle and versions |
| `prof` | `http://www.w3.org/ns/prof#` | profiles and conformance |
| `void` | `http://rdfs.org/ns/void#` | dataset statistics |
| `oreg` | `urn:onto:registry:` | registry-only extension terms |
| `pm` | `urn:onto:project:` | project-only extension terms |

The `oreg:` and `pm:` namespaces are internal IRIs. The API can dereference
vocabulary terms through `/api/v1/rdf/{resource}`; publishing them at stable HTTP
IRIs is a separate decisions (see §12).

### 4.2 Registry entities

| Concept | RDF type(s) | Identity |
| --- | --- | --- |
| Catalog | `dcat:Catalog` | catalog IRI or `urn:onto:registry:catalog` |
| Catalog record | `dcat:CatalogRecord` | generated from asset key |
| Ontology family | `oreg:Ontology`, `dcat:DatasetSeries` | stable ontology IRI |
| Release | `oreg:OntologyRelease`, `dcat:Dataset`, `adms:Asset` | version IRI |
| Release candidate | `oreg:ReleaseCandidate` | `urn:onto:registry:candidate:<ulid>` |
| Distribution | `dcat:Distribution`, `adms:AssetDistribution` | `urn:onto:registry:distribution:<key>:<format>` |
| Module | `oreg:Module` | `urn:onto:registry:module:<key>:<slug>` |
| Term record | `oreg:OntologyTerm` | `urn:onto:registry:term:<key>:<term-hash>` |
| Import dependency | `oreg:ImportDependency` | `urn:onto:registry:import:<key>:<dep-hash>` |
| Analysis report | `oreg:AnalysisReport`, `prov:Entity` | `urn:onto:registry:analysis-report:<ulid>` |
| Validation run | `oreg:ValidationRun`, `prov:Activity` | `urn:onto:registry:validation-run:<ulid>` |
| Compatibility report | `oreg:CompatibilityReport` | `urn:onto:registry:compatibility:<ulid>` |
| Profile | `prof:Profile`, `oreg:Profile` | profile IRI |

A family is not a release. `oreg:hasRelease` links them; `oreg:currentRelease`
points at the selected current release. `dcat:hasVersion`,
`dcat:hasCurrentVersion`, `dcat:previousVersion`, and `dcat:isVersionOf` mirror the
catalog view. OWL's `owl:versionIRI` remains on the release document itself.
Catalog records carry `adms:status`, `dcterms:issued`, `dcterms:modified`, and
version notes; status changes append PROV records rather than erasing history.

### 4.3 Custom registry terms (minimum)

Classes:

`oreg:Ontology`, `oreg:OntologyRelease`, `oreg:ReleaseCandidate`, `oreg:Module`,
`oreg:OntologyTerm`, `oreg:ImportDependency`, `oreg:AnalysisReport`,
`oreg:ValidationRun`, `oreg:CompatibilityReport`, `oreg:Profile`.

Properties:

`oreg:hasRelease`, `oreg:currentRelease`, `oreg:hasModule`, `oreg:moduleOf`,
`oreg:namespaceIRI`, `oreg:ontologyIRI`, `oreg:versionIRI`, `oreg:importsOntology`,
`oreg:unresolvedImport`, `oreg:hasTerm`, `oreg:termIri`, `oreg:termKind`,
`oreg:hasValidationReport`, `oreg:hasCompatibilityReport`, `oreg:contentHash`,
`oreg:sourceHash`, `oreg:releaseChannel`, `oreg:breakingChange`,
`oreg:generatedBy`, `oreg:releaseKey`, `oreg:assetKey`, `oreg:distributionFormat`.

Use `dcat:downloadURL`, `dcat:mediaType`, `dcat:byteSize`, and
`dcat:packageFormat` for distributions. Do not create custom duplicates.

### 4.4 Project entities

Phase P1 introduces the minimum set; later phases extend it.

| Concept | Type | Notes |
| --- | --- | --- |
| Project | `pm:Project` | stable project IRI |
| Goal / objective / criterion | `pm:Goal`, `pm:Objective`, `pm:SuccessCriterion` | goal decomposition |
| Requirement | `pm:Requirement` (+ subtypes) | capability or constraint |
| Work package / task | `pm:WorkPackage`, `pm:Task` | task is a unit of accountability |
| Milestone | `pm:Milestone` | significant completion point |
| Deliverable | `pm:Deliverable` | bridge to an asset/release |
| Acceptance criterion | `pm:AcceptanceCriterion` | testable completion condition |
| Evidence | `pm:Evidence` | validation report, diff, publication record |
| Decision / risk / issue | `pm:Decision`, `pm:Risk`, `pm:Issue` | governance |
| Snapshot | `pm:ProjectSnapshot` | pins plans and release IRIs |

Core properties: `pm:hasGoal`, `pm:hasWorkPackage`, `pm:hasTask`,
`pm:parentWorkItem`, `pm:contributesTo`, `pm:dependsOn`, `pm:blocks`,
`pm:produces`, `pm:fulfills`, `pm:realizesAsset`, `pm:targetsRelease`,
`pm:managesAsset`, `pm:adopts`, `pm:consumes`, `pm:dependsOnAsset`,
`pm:contributesToAsset`, `pm:hasEvidence`, `pm:acceptedThrough`,
`pm:hasAcceptanceCriterion`.

Deliverable kinds are a SKOS concept scheme (`pm:DeliverableType`) covering
semantic assets, documentation, software, datasets, workflows, validation
reports, and research outputs; a deliverable may carry more than one kind.

`pm:WorkItem` remains separate from `p-plan:Step`. A task may have one or more
execution plans. A deliverable points at a release; it does not duplicate release
metadata.

Vocabulary reuse is deliberate. PROMCODE supplies scope, work item, artifact,
risk, measure, and plan concepts; ORG/FOAF supply agents and teams; OWL-Time
supplies dates, intervals, and durations; QUDT supplies estimates and costs; SKOS
supplies statuses, priorities, and deliverable types; P-Plan supplies execution
plans. Phase R0 records the exact PROMCODE terms and definitions, and P1 maps to
them where the semantics match. `pm:` is the stable application vocabulary; a
mapping is never asserted as `owl:equivalentClass` without review.

### 4.5 Project-to-registry bridge

The bridge is a small core trait so the project crate does not depend on the
registry crate:

```rust
#[async_trait::async_trait]
pub trait SemanticAssetCatalog: Send + Sync {
    async fn asset(&self, id: &Id) -> Result<AssetSummary>;
    async fn release(&self, id: &Id) -> Result<ReleaseSummary>;
    async fn releases_for_asset(&self, id: &Id) -> Result<Vec<ReleaseSummary>>;
    async fn validation_reports(&self, release: &Id) -> Result<Vec<ReportSummary>>;
}
```

`ProjectService` uses this for `pm:realizesAsset`, `pm:targetsRelease`,
acceptance evidence, and impact reports. The registry implements it over catalog
and analysis graphs.

### 4.6 Example

```turtle
@prefix dcat:    <http://www.w3.org/ns/dcat#> .
@prefix dcterms: <http://purl.org/dc/terms/> .
@prefix adms:    <http://www.w3.org/ns/adms#> .
@prefix oreg:    <urn:onto:registry:> .
@prefix pm:      <urn:onto:project:> .
@prefix ex:      <https://example.org/> .

ex:planning
    a oreg:Ontology, dcat:DatasetSeries ;
    dcterms:title "Planning Ontology" ;
    oreg:currentRelease ex:planning-1.2 .

ex:planning-1.2
    a oreg:OntologyRelease, dcat:Dataset, adms:Asset ;
    dcat:isVersionOf ex:planning ;
    dcat:version "1.2.0" ;
    oreg:contentHash "sha256:..." ;
    oreg:releaseKey "4f2a9c1d7e803b66" .

ex:project-registry
    a pm:Project ;
    pm:hasGoal ex:goal-planning ;
    pm:hasWorkPackage ex:wp-model .

ex:task-publish
    a pm:Task ;
    pm:produces ex:deliverable-planning .
```

---

## 5. Configuration, Identity, and Storage Rules

### 5.1 Configuration

Add two optional sections. Absence means the services are available with the
defaults below; no existing command changes behaviour until a registry or project
command is called.

```toml
[registry]
artifact_dir = "./data/registry"
max_source_bytes = 52428800
max_upload_bytes = 52428800
max_sync_bytes = 4194304
allow_remote = false
remote_hosts = []
require_resolved_imports = false
public_base_url = "http://127.0.0.1:8080"

[project]
base_iri = "urn:onto:project:"
```

| Setting | Rule |
| --- | --- |
| `artifact_dir` | Must not contain `..`; created on first write; canonicalised; symlinks rejected when resolving stored paths |
| `max_source_bytes` | Greater than zero; applies to local files and HTTP bodies |
| `max_upload_bytes` | Greater than zero; independent multipart limit |
| `max_sync_bytes` | Greater than zero and at most `max_source_bytes`; larger sources use the background job path |
| `allow_remote` | `false` by default; when true, `remote_hosts` must be non-empty unless explicitly `["*"]` with a warning |
| `remote_hosts` | Exact host names, no wildcards except the explicit `*`; redirects are re-checked against the same list |
| `require_resolved_imports` | Publication gate; default `false` so offline catalogs can publish with unresolved imports recorded |
| `public_base_url` | Used only to build human-facing download URLs; never used to fetch |
| `project.base_iri` | Absolute IRI prefix for generated project IDs |

`Config::validate` gains the path, size, host-list, and IRI checks. `Config::load`
continues to tolerate a missing file; a missing `[registry]` section is not an
error. The shipped `config/ontology.toml` gets commented examples, not enabled
remote access.

### 5.2 Identity rules

| Identity | Derivation | Stability |
| --- | --- | --- |
| Family IRI | `owl:Ontology` subject, `skos:ConceptScheme` IRI, or explicit `asset_iri` | permanent |
| Release version | explicit request, then `owl:versionInfo`, else error | permanent label |
| Version IRI | declared `owl:versionIRI`, else `urn:onto:registry:release:<key>` | permanent |
| Release key | first 16 hex of SHA-256 of `family_iri + "|" + version` | stable, path-safe |
| Asset key | first 16 hex of SHA-256 of the family IRI | stable, path-safe |
| Source hash | SHA-256 of the exact source bytes | per artifact |
| Content hash | SHA-256 of sorted canonical N-Quads for the parsed release | per release |

Rules:

- A source that declares no identity requires an explicit `asset_iri`; never
  invent a family silently.
- A missing version is an error; do not use a timestamp, because it makes repeat
  ingestion non-deterministic.
- Publishing the same family and version with the same source hash (or the same
  canonical content hash once RDFC-1.0 is available) is idempotent and returns the
  existing release.
- Publishing the same family and version with different content is
  `Error::Conflict`; releases are immutable.
- Blank-node labels are not stable across parsers. `content_hash` is therefore
  informational until RDF Dataset Canonicalization (RDFC-1.0) is implemented;
  `source_hash` plus the family/version uniqueness rule is authoritative.
- Release and asset keys never contain `/`, `#`, or `?`, so they are safe in path
  segments.

### 5.3 Artifact store

```
data/registry/
├── sha256/
│   ├── 4f/2a9c1d7e803b66...     # exact source bytes
│   └── 91/7c2e0d4a1f55aa...     # Turtle distribution
├── manifests/
│   └── 4f2a9c1d7e803b66.json    # optional import lock / distribution manifest
└── tmp/                        # same filesystem, cleaned on startup
```

- Files are content-addressed and never overwritten. A second write of identical
  bytes is a no-op.
- Write to `tmp/<random>`, flush, `rename` into `sha256/<aa>/<hash>`; a failed
  rename leaves only a temp file.
- The extension is metadata, not part of the lookup key; the catalog records the
  media type and original filename.
- Reads resolve the hash, canonicalise the path, and verify it stays under
  `artifact_dir`. No caller-supplied path is joined directly.
- The artifact store is a trait (`ArtifactStore`) with a filesystem implementation;
  S3 is a later implementation, not a second code path in the registry service.
- Backup includes both the main store and `artifact_dir` in one manifest; a
  database backup without its artifacts is incomplete.

### 5.4 Publication ordering

There is no reliable cross-store transaction for files plus RocksDB. Publication
uses an ordered protocol that never exposes a catalog entry before its content is
readable:

1. Parse, analyse, validate, and diff in staging.
2. Write the release graph (immutable, not referenced by the catalog yet).
3. Generate and write every distribution artifact; verify hashes by reading back.
4. In one catalog transaction, insert the release, distributions, analysis report
   link, validation link, and provenance; update `oreg:currentRelease` and
   `dcat:hasCurrentVersion`.
5. Commit; a failure before step 4 leaves an unreferenced release graph and
   artifacts that a later garbage-collection pass may remove.
6. Remove the staging graph only after the catalog commit.

If `RdfStore::transaction()` is unsupported, perform step 4 last with the release
record as the final insert. A crash can leave an orphan, but never a visible release
without content. The registry never deletes a published release graph; archive
only changes the channel and current pointer.

---

## 6. Services, Commands, and Interfaces

### 6.1 Service traits

Keep the transport-neutral boundary small:

```rust
pub trait RegistryServiceApi: Send + Sync {
    async fn ingest(&self, request: IngestRequest) -> Result<IngestionResult>;
    async fn analyze(&self, candidate: &Id) -> Result<AnalysisReport>;
    async fn validate(
        &self,
        candidate: &Id,
        profile: Option<&Id>,
    ) -> Result<ValidationReport>;
    async fn publish(&self, candidate: &Id) -> Result<ReleaseSummary>;
    async fn compare(&self, left: &Id, right: &Id) -> Result<CompatibilityReport>;
    async fn search_terms(&self, query: TermQuery) -> Result<Vec<TermSummary>>;
}

pub trait ProjectServiceApi: Send + Sync {
    async fn create(&self, project: NewProject) -> Result<Id>;
    async fn transition(&self, task: &Id, event: TaskEvent) -> Result<TaskStatus>;
    async fn link_deliverable(
        &self,
        deliverable: &Id,
        release: &Id,
    ) -> Result<()>;
    async fn accept(&self, deliverable: &Id) -> Result<AcceptanceReport>;
    async fn snapshot(&self, project: &Id) -> Result<SnapshotSummary>;
}
```

The concrete implementations are `RegistryService` and `ProjectService`.
`ApplicationService` holds `Arc<RegistryService>` and `Arc<ProjectService>` and
implements the existing `OntologyService` command boundary.

### 6.2 Command additions

Add two variants to `Command`:

```rust
Registry(RegistryCommand),
Project(ProjectCommand),
```

`RegistryCommand` actions: `create_catalog`, `get_catalog`, `list_catalogs`,
`register_asset`, `get_asset`, `list_assets`, `update_asset`, `search`, `ingest`,
`get_ingestion`, `analyze`, `resolve_imports`, `validate`, `create_candidate`,
`publish`, `list_releases`, `get_release`, `release_terms`, `get_term`,
`release_imports`, `release_statistics`, `diff`, `list_reports`, `create_profile`,
`list_profiles`.

`ProjectCommand` actions: `create`, `get`, `list`, `update`, `add_goal`,
`list_goals`, `add_requirement`, `add_work_package`, `add_task`, `update_work`,
`transition`, `assign`, `add_deliverable`, `link_asset`, `accept`, `snapshot`,
`timeline`, `overview`, `dependencies`, `list_risks`, `list_decisions`.

Add `CommandResult::Registry(RegistryResult)` and
`CommandResult::Project(ProjectResult)` for rich responses (candidate with
reports, release detail, project overview). The existing `Resource`, `Collection`,
`Report`, and `Validation` variants remain the default for simple operations.
Update `CommandResultRepr` and its serialization tests in the same change.

### 6.3 Jobs

Ingestion and analysis are synchronous when the source is at most
`max_sync_bytes`; this keeps the common path simple and deterministic. Add
`JobRequest::RegistryIngest` and `JobRequest::RegistryAnalyze` for sources above
that threshold, with the same result serialization. A job holds a candidate id, so
cancellation leaves an unpublishable candidate and cleans staging. Publication is
never a background job.

### 6.4 HTTP API

Use opaque keys in path segments and full IRIs in bodies/results:

| Method | Endpoint | Purpose |
| --- | --- | --- |
| `GET/POST` | `/api/v1/registry/catalogs` | list/create catalogs |
| `GET` | `/api/v1/registry/catalogs/{id}` | catalog detail |
| `GET/POST` | `/api/v1/registry/catalogs/{id}/assets` | list/register assets |
| `GET/PATCH` | `/api/v1/registry/assets/{key}` | asset metadata |
| `GET` | `/api/v1/registry/assets/{key}/releases` | release history |
| `POST` | `/api/v1/registry/ingestions` | multipart upload, local path, or URL |
| `GET` | `/api/v1/registry/ingestions/{id}` | candidate status and reports |
| `POST` | `/api/v1/registry/ingestions/{id}/analyze` | recompute analysis |
| `POST` | `/api/v1/registry/ingestions/{id}/resolve-imports` | resolve/pin imports |
| `POST` | `/api/v1/registry/ingestions/{id}/validate` | structural/profile validation |
| `POST` | `/api/v1/registry/ingestions/{id}/release-candidate` | freeze a candidate |
| `POST` | `/api/v1/registry/candidates/{id}/publish` | publish; requires confirmation |
| `POST` | `/api/v1/registry/releases/{key}/archive` | archive a release and move the current pointer |
| `GET` | `/api/v1/registry/releases/{key}` | release metadata |
| `GET` | `/api/v1/registry/releases/{key}/terms` | term search |
| `GET` | `/api/v1/registry/releases/{key}/terms/{term}` | term detail |
| `GET` | `/api/v1/registry/releases/{key}/imports` | dependency graph |
| `GET` | `/api/v1/registry/releases/{key}/statistics` | statistics/VoID view |
| `GET` | `/api/v1/registry/releases/{key}/diff/{other}` | structural comparison |
| `GET` | `/api/v1/registry/releases/{key}/reports` | validation/analysis/compatibility |
| `GET` | `/api/v1/registry/download/{key}/{format}` | authenticated distribution |
| `GET/POST` | `/api/v1/registry/profiles` | profiles and shape pointers |
| `GET` | `/api/v1/registry/search?q=&kind=` | catalog, term, namespace, and dependency search |

Project endpoints follow the design's resource model, with safe slugs for
`{project}` and generated ids for work items:

```
POST/GET  /api/v1/projects
GET       /api/v1/projects/{project}
POST/GET  /api/v1/projects/{project}/goals
POST/GET  /api/v1/projects/{project}/requirements
POST/GET  /api/v1/projects/{project}/work
PATCH     /api/v1/work/{id}
POST      /api/v1/work/{id}/transition
POST      /api/v1/work/{id}/assignments
GET       /api/v1/projects/{project}/dependencies
GET/POST  /api/v1/projects/{project}/deliverables
POST      /api/v1/deliverables/{id}/accept
POST      /api/v1/projects/{project}/snapshots
GET       /api/v1/projects/{project}/overview
GET       /api/v1/projects/{project}/timeline
GET       /api/v1/projects/{project}/assets
POST      /api/v1/projects/{project}/assets
GET       /api/v1/projects/{project}/compatibility
```

Implementation notes:

- Add the `multipart` feature to Axum for uploads. Spool to a temporary file while
  enforcing `max_upload_bytes`; never buffer unbounded bodies in memory.
- The ingestion body is either multipart (`file`, `format`, `version`) or JSON with
  `source`, `asset_iri`, `version`, and `format`.
- Download routes stream from the artifact store, set `Content-Type` and
  `Content-Disposition`, and reject unknown keys before touching the filesystem.
- Term detail paths use the deterministic term key (a hash), not the raw term IRI;
  the response always includes the IRI. Release keys follow the same rule.
- OpenAPI is generated manually today; update `openapi.rs` for every route and add
  a test that route count and documented count agree.
- The existing `/api/v1/sparql/update` must not be a bypass around release
  immutability. Before executing an update, inspect its target graphs with the
  `spargebra` AST; reject writes to `urn:onto:registry:release:*`,
  `urn:onto:registry:analysis:*`, `urn:onto:registry:validation:*`, and
  `urn:onto:registry:provenance` from user commands. Registry internals use the
  raw store, not this guarded path.

### 6.5 CLI

Keep the binary name `onto`; add two top-level groups:

```
onto registry catalog list|show|create
onto registry asset register|list|show
onto registry ingest <file-or-url> --version <v> [--asset <iri>] [--format <f>]
onto registry candidate show|analyze|resolve|validate|publish
onto registry release list|show|terms|term|imports|stats|diff|reports|archive
onto registry profile list|validate
onto registry search <query> [--kind catalog|term|namespace|dependency]
onto registry download <release-key> --format turtle --output <path>

onto project create|list|show|overview
onto project goal add|list
onto project requirement add|list
onto project work add|list|assign|transition
onto project deliverable add|list|link|accept
onto project snapshot create|show
onto project timeline
onto project risk list
onto project decision list
```

- `--json`, `--quiet`, `--server`, `--catalog`, `--version`, `--format`,
  `--dry-run`, `--explain`, `--include-dependencies` follow the design's CLI
  recommendations where they map to an existing concept.
- `onto registry ingest ./file.ttl` uploads the file as multipart when a remote
  server is configured; the server does not read the client's filesystem. This is
  the same rule as document extraction.
- Publication and acceptance print the evidence and require `--yes` unless the
  command is non-interactive with `--dry-run`.

### 6.6 MCP

Add two tools, `registry` and `project`, keeping the existing four-tool contract
intact for current agents:

| Tool | Actions |
| --- | --- |
| `registry` | `catalog`, `asset`, `ingest`, `candidate`, `analyze`, `validate`, `publish`, `release`, `terms`, `imports`, `diff`, `reports`, `profile` |
| `project` | `project`, `goal`, `requirement`, `work`, `assign`, `transition`, `deliverable`, `accept`, `snapshot`, `overview`, `timeline`, `risk`, `decision` |

Rules:

- Reads are allowed in read-only mode. Ingestion, publication, assignment, and
  acceptance are writes.
- Publication and acceptance require confirmation; both are non-destructive in the
  storage sense (they append), but they change what the organization treats as
  current.
- Remote ingestion is additionally gated as an external operation.
- Results include stable keys, full IRIs, release versions, and report summaries;
  agents fetch full term trees by key rather than receiving the whole ontology.
- Update `mcp-server.md` (and `infodocs/mcp-server.md` when implementation lands)
  from four tools to six.

### 6.7 Errors and HTTP status mapping

Add two coarse variants rather than a variant per registry operation:

```rust
Registry { stage: &'static str, detail: String },
Project { stage: &'static str, detail: String },
Limit { limit: u64, actual: u64 },
```

| Domain result | HTTP |
| --- | --- |
| unknown key/resource | `404` |
| duplicate version with different content | `409` |
| malformed source, version, profile, or transition | `422` |
| oversized source/upload | `413` |
| unsupported format, feature, or SHACL engine | `501` |
| artifact I/O, storage, or internal invariant failure | `500` |

No registry operation returns a bare `400` for a semantic problem; `Invalid` is
reserved for malformed request envelopes.

---

## 7. Operational Workflows

### 7.1 Ingesting an external ontology

```
source (file/URL/upload)
  -> size + host policy
  -> source hash
  -> parse (explicit format > extension > content type)
  -> identify family/version/versionIRI
  -> release key + collision check
  -> staging graph + source artifact
  -> term index + statistics + imports
  -> structural validation + diff against current
  -> candidate (status = ready | needs_review | rejected)
```

A candidate is not visible in catalog listings or release queries. It can be
re-analysed and re-validated; its staging graph and artifacts are removed when it
is discarded.

### 7.2 Publishing

```
candidate
  -> re-run validation and import policy
  -> write immutable release graph
  -> materialize Turtle/N-Triples/N-Quads/RDF-XML/JSON-LD artifacts
  -> verify each artifact by reading back its hash
  -> catalog transaction: release + distributions + reports + provenance
  -> current release pointer
  -> delete staging
```

Publishing is all-or-nothing at the catalog boundary. A release is never edited;
new content means a new version. Archiving keeps the content and sets
`oreg:releaseChannel` to `archived` and moves the current pointer to the selected
successor.

### 7.3 Updating a family

1. Ingest the new candidate.
2. Diff structural terms and axioms against the current release.
3. Classify additions/removals/domain-range changes/restriction changes.
4. Resolve imports and pin selected release IRIs.
5. Validate against the family's profile, if any.
6. Publish; the previous release remains addressable.
7. Emit a compatibility report and a project-impact report when projects adopt the
   family.

### 7.4 Producing an ontology through a project

```
project goal
  -> requirement (capability or profile)
  -> work package
  -> task
  -> deliverable
  -> registry asset/release
  -> validation evidence
  -> acceptance
  -> goal progress
```

A deliverable is accepted only when its required evidence exists: a passing
validation report, a publication record, and any explicit acceptance criterion.
Acceptance does not mutate the release; it records a project decision with
provenance.

### 7.5 Impact analysis

Given a release or family, traverse:

```
release -> imports -> dependent releases -> profiles
        -> project adoptions -> deliverables -> affected tasks
```

Return affected projects, unaccepted deliverables, validation evidence, and
breaking-change counts. This is a dedicated query service, not an LLM-composed
SPARQL query.

### 7.6 Discovery and search

Start with SPARQL-backed search over the catalog and term graphs; add a dedicated
index only after measurement shows it is needed.

| Search | Initial query |
| --- | --- |
| Metadata | labels, descriptions, keywords, and release notes in the catalog graph |
| Namespace | `oreg:namespaceIRI` and prefix matching |
| Term | local name, preferred label, and alternate label |
| Definition | `skos:definition`, `rdfs:comment`, and `dcterms:description` |
| Type | `oreg:termKind` (`class`, `object_property`, `datatype_property`, `annotation_property`, `individual`) |
| Dependency | `owl:imports` and resolved `oreg:importsOntology` edges |
| Usage | incoming references from registered releases and profiles |
| Compatibility | releases whose latest report matches a requested profile or change level |
| Structural | releases containing a class hierarchy or a selected module |

Rules:

- All search results are paged with the existing `limit`/`offset` limits and
  return stable keys plus full IRIs.
- Text matching is case-insensitive `CONTAINS` initially; it is not ranked. Add
  ranking or full-text indexing only with a benchmarked requirement.
- Term results reference the authoritative release graph; the analysis graph holds
  summaries and provenance links, never a competing definition.
- Cross-ontology mappings (`skos:closeMatch`, `skos:relatedMatch`, and reviewed
  `owl:equivalentClass`) are an R7 feature and are not inferred from labels.

---

## 8. Testing Strategy

### 8.1 Unit tests

- Identity: family/version/key derivation, collision, missing version, slash-safe
  keys, same-content idempotence.
- Artifact store: atomic write, hash verification, symlink/traversal rejection,
  duplicate write no-op, cleanup.
- Source policy: size cap, host allowlist, redirect re-check, blocked private IPs.
- Parser/identity: Turtle, RDF/XML, JSON-LD, N-Triples, N-Quads, TriG, base IRI,
  blank nodes.
- Term index: classes/properties/individuals, labels/definitions, superclass and
  domain/range links, deprecation, deterministic record IRIs.
- Dependencies: resolved/unresolved, version pin, transitive closure, cycle.
- Diff: added/removed/renamed, domain/range change, restriction change, labels-only
  change, breaking classification.
- Validation: every structural rule has a passing and failing fixture.
- Project: state transitions, illegal transitions, acceptance evidence, snapshot
  pinning, dependency cycle.

### 8.2 Integration tests

Add `tests/tests/phase9_registry.rs` and `tests/tests/phase10_projects.rs`, using
`tempfile` for stores and artifacts:

1. Catalog CRUD and asset registration.
2. Ingest `planning-1.0.ttl` into a candidate; assert term/stat/import counts.
3. Re-ingest identical bytes; assert idempotent candidate and no duplicate quads.
4. Publish; assert immutable release graph, current pointer, distributions, and
   hashes.
5. Ingest `planning-1.1.ttl`; assert diff and compatibility categories.
6. Import resolution against a registered dependency; unresolved import with and
   without the publication gate.
7. Validation with a profile fixture; assert report graph and evidence link.
8. HTTP multipart upload, candidate detail, publish, download, and error statuses.
9. CLI upload/list/publish/download against an in-process server.
10. MCP `registry` read/write policy and `project` acceptance confirmation.
11. Project CRUD, work breakdown, dependency cycle, deliverable link, acceptance,
    snapshot, overview.
12. Impact report across a project adopting a release that imports another family.

### 8.3 Fixtures

Check in small, anonymised fixtures under `tests/fixtures/registry/`:

- `planning-1.0.ttl`, `planning-1.1.ttl` (labels, class, object property, import).
- `planning.rdf` (same family, RDF/XML).
- `planning.jsonld` (same family, JSON-LD).
- `skos-vocabulary.ttl` (a `skos:ConceptScheme` with no `owl:Ontology`).
- `dependency.ttl` (import target with a fixed version IRI).
- `profile-shapes.ttl` (a minimal profile shape artifact).
- `project.ttl` (project, task, deliverable, evidence references).

No test performs real network I/O. Remote-source tests use a local Axum listener
with a temporary allowlist entry.

### 8.4 Determinism and performance

- Sort serialized N-Quads and all term/import/report collections before hashing or
  returning them.
- Assert repeated ingest of the same bytes produces identical catalog metadata
  (apart from timestamps).
- Record benchmarks for: 1 MB Turtle parse + term index, 10 MB source, 1k-term
  search, and a 100-release catalog listing. These are reports, not hard gates
  until the implementation stabilizes.

---

## 9. Implementation Phases

The phases are ordered so every phase leaves a testable system and no phase
requires a later phase's interface. Registry phases are prefixed `R`; project
phases are prefixed `P`.

### Phase R0 — Standards and contract spike (timeboxed)

**Work.**

1. Pin the exact vocabulary documents and namespaces for DCAT 3, ADMS (SEMIC
   edition), OWL 2, SKOS, SHACL, PROV-O, VoID, and PROF under `ontologies/vocab/`
   or a documented cache; record the retrieval date.
2. Verify the fork's parser/serializer round-trips every `RdfFormat`, including
   JSON-LD and RDF/XML.
3. Evaluate SHACL options: a Rust engine compatible with the pinned Oxigraph fork,
   or a bounded internal evaluator. If neither is viable, adopt the structural
   validator plus external SHACL artifact policy and record it as a limitation.
4. Write `docs/registry-contract.md`: source resolution, identity, candidate JSON,
   artifact layout, report shapes, error codes, and the immutable-release protocol.
5. Capture the project graph example and the release/term query shapes as fixtures.

**Deliverable.** `docs/registry-contract.md` plus vocabulary/licence notes.

**Acceptance.** The contract document parses every fixture and names the chosen
SHACL path; no production crate has changed yet.

### Phase R1 — Domain model, configuration, storage partitions

**Work.**

- Add `ontology-core::{registry, registry_graph, project}` types, request/result
  structs, command variants, config structs, namespace constants, and error
  variants.
- Add fixed graph constructors and the dynamic-prefix rule; extend the graph
  registry parser/validator and router.
- Create `ontology-registry` with catalog repository and `ontology-project`
  skeleton; wire neither into interfaces yet.
- Add a `protected_graphs` guard helper used later by the application boundary.
- Update `Cargo.toml`, exports, and config validation tests.

**Acceptance.** Catalog/asset/release metadata CRUD works against in-memory and
persistent stores; a dynamic graph under `urn:onto:registry:` routes to main in
multi-graph mode; all existing workspace tests pass unchanged.

### Phase R2 — Artifacts, source policy, and candidates

**Work.**

- Implement `ArtifactStore`, atomic writes, hash verification, and path checks.
- Implement local file and opt-in HTTP sources with size/host/redirect policy.
- Implement format detection, parsing, identity, release keys, staging graphs, and
  candidate records.
- Add a minimal term/statistics pass sufficient to populate the candidate summary.
- Add conflict and idempotence handling before publication exists.

**Acceptance.** Ingest Turtle, RDF/XML, and JSON-LD fixtures; identical bytes create
no duplicate candidate; a different body for the same version is `409`; traversal
and oversized-source tests fail closed.

### Phase R3 — Analysis, term index, dependencies, and diffs

**Work.**

- Extract classes, properties, individuals, restrictions, labels, definitions,
  comments, domains/ranges, superclasses, equivalents, and deprecation.
- Write deterministic term records and VoID-style statistics.
- Resolve `owl:imports` against registered releases; record unresolved imports;
  build the transitive graph, detect cycles, and write the lock manifest.
- Implement module records (default module plus `rdfs:isDefinedBy`/explicit module
  annotations).
- Implement structural diff and compatibility classification.

**Acceptance.** Fixtures produce asserted counts and term/import graphs; an
unresolved dependency is visible but publishable by default; a cycle is reported;
1.0 vs 1.1 classifies removals as breaking and additions as compatible.

### Phase R4 — Validation, profiles, and conformance

**Work.**

- Implement the `Validator` trait with a structural validator.
- Add the SHACL adapter if R0 selected one; otherwise ship shapes artifacts and
  mark conformance as externally evaluated.
- Add profile records, shape artifact pointers, and profile validation of a release
  or data graph.
- Add compatibility reports combining structural diff and optional profile result.
- Define the exact validation report RDF (`sh:ValidationReport`, `sh:conforms`,
  `sh:result`).

**Acceptance.** Every structural rule has pass/fail tests; profile validation
stores a report and links it to the release; no `sh:conforms true` is emitted
without an actual evaluator.

### Phase R5 — Publication and lifecycle

**Work.**

- Materialize release graph, five distribution formats, hashes, and byte sizes.
- Implement the ordered catalog transaction, current pointer, provenance, and
  staging cleanup.
- Add archive/channel operations and release history.
- Add the SPARQL update guard for immutable registry graphs.
- Add idempotent publish and conflict tests.

**Acceptance.** Publish once and twice; verify one release, stable content hash,
readable distributions, current pointer, provenance, and no staging graph. A
failed distribution write leaves no catalog entry. A user SPARQL update cannot
modify a published release.

### Phase R6 — Application service and interfaces

**Work.**

- Implement `RegistryService` commands, results, and job thresholds.
- Wire registry into `ApplicationService`; expose `RegistryResult` serialization.
- Add HTTP routes, multipart upload, downloads, OpenAPI entries, and error mapping.
- Add CLI groups and MCP `registry` tool/policy.
- Add end-to-end registry tests.

**Acceptance.** A client can upload, inspect, validate, publish, list, diff,
search terms, and download through HTTP, CLI, and MCP; read-only MCP mode blocks
all writes; oversized/malformed requests return the documented statuses.

### Phase R7 — Advanced sources and mappings (optional)

- Git repository/tag sources, ZIP/TAR packages, and DCAT catalog import.
- Cross-ontology SKOS/OWL mappings in a dedicated mapping graph.
- S3-compatible artifacts, full-text search index, and federated catalog queries.
- Bridge existing `external_ontologies` records into registry assets/releases.

Do not start this phase until R0–R6 are stable; each item is independently
releasable.

### Phase P1 — Project core and deliverable bridge

**Work.**

- Add project/work/goal/requirement/deliverable types and RDF encodings.
- Implement project CRUD, work breakdown, milestones, deliveries, and links to
  assets/releases through `SemanticAssetCatalog`.
- Add the `pm:DeliverableType` SKOS scheme and task/deliverable state transitions
  with SKOS statuses.
- Add minimal HTTP/CLI/MCP project operations and structural integrity checks
  (SHACL shapes are shipped when the R0 evaluator decision permits).

**Acceptance.** Create a project, add a requirement and work package, link a
deliverable to a published release, record acceptance evidence, and query the
overview. Deleting a linked release is impossible; archiving leaves the link.

### Phase P2 — Planning, execution, and provenance

**Work.**

- Add `pm:hasExecutionPlan` and P-Plan records (plan, steps, variables,
  dependencies) without equating a task with a step.
- Record task assignment, transition, validation, publication, and acceptance as
  PROV activities in `urn:onto:project:events` and registry provenance.
- Add plan approval and execution linkage; allow multiple plans per task.
- Add task dependency cycle detection and critical-path listing (no scheduler yet).

**Acceptance.** A task can reference an approved plan, a transition produces a PROV
record, history is append-only, and dependency cycles are rejected.

### Phase P3 — Scheduling, governance, baselines, and dashboards

**Work.**

- Add assignments, teams, roles, estimates, risks, issues, decisions, and reviews.
- Add milestones, schedule baselines, project snapshots, and release pinning.
- Implement `/projects/{id}/overview`, `/timeline`, `/risks`, `/decisions`, and the
  explicit weighted progress calculation. Store the method and value
  (`pm:progressMethod`, `pm:progressValue`) and test the documented weights rather
  than averaging task percentages.
- Add project integrity checks and project profile shapes, including identity,
  owner, self-dependency, cycle, milestone ownership, deliverable output,
  acceptance evidence, immutable publication reference, baseline pinning, and
  completion criteria.

**Acceptance.** A snapshot reconstructs the project's plan and exact release set;
overview totals match the underlying work; baselines reject mutable release
references; risk/decision history survives a snapshot.

### Phase P4 — Impact analysis and portfolio (optional)

- Release/family impact traversal over imports, profiles, adoptions, and tasks.
- Compatibility-driven work generation and portfolio dashboards.
- Automated planning suggestions; never auto-publish.
- Cross-project dependency reports.

---

## 10. Migration and Compatibility

1. **No registry use, no change.** Existing commands, graphs, API routes, CLI, and
   MCP tools keep their current behaviour. New graphs are created only when a
   registry/project command writes.
2. **Existing external ontologies.** `/external-ontologies` remains. A bridge can
   later create catalog assets from those records; no automatic rewrite of their
   dedicated graphs.
3. **Existing OWL schema.** Registry releases do not populate `system:schema` and
   do not replace `OwlManager`. Authored classes remain in the schema graph.
4. **Multi-graph deployments.** Add the dynamic-prefix declaration to the main
   store registration before enabling registry writes. Without it, the router must
   fail closed rather than write to an unknown graph.
5. **Backups.** Update the operations procedure to back up the main store and
   `artifact_dir` together; a store-only restore loses distributions.
6. **API compatibility.** All additions are additive. The six-tool MCP surface is
   a documented change; existing tool names and actions are unchanged.
7. **Rollback.** Stop issuing registry/project commands and restore the previous
   binary. Registry graphs and artifacts remain readable by the new binary; no
   destructive migration is required. Do not delete release graphs during rollback.

---

## 11. Risks and Mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| No SHACL engine in the workspace | Validation claims exceed reality | R0 selects or rejects an engine; structural validator and explicit `sh:conforms` policy |
| Blank-node labels vary | Content hashes differ for equivalent graphs | Source hash is authoritative; RDFC-1.0 deferred and documented |
| SPARQL update bypasses immutability | Published releases overwritten | AST-level protected-graph guard; tests with direct update attempts |
| Dynamic graphs not routed | Multi-graph registry writes fail | `onto:dynamicPrefix` validation and router tests in R1 |
| Remote fetch SSRF | Internal network access | Disabled by default, exact allowlist, redirect/IP checks, size/time limits |
| Artifact path traversal | Arbitrary file read/write | Content-hash lookup only, canonicalisation, symlink rejection |
| Version collisions | Ambiguous catalog | Family+version uniqueness; conflict on different content |
| Large ontologies exhaust memory | Process instability | Byte caps, streaming upload, synchronous threshold, job path |
| Release IRIs contain `/` | Broken routes | Opaque path keys; IRIs in bodies |
| Term index explodes | Slow queries/storage | Index summaries only, page limits, deterministic records, optional search index later |
| Structural diff overclaims compatibility | Unsafe upgrades | Report categories and require profile/application validation for breaking changes |
| Project model scope creep | Registry delayed | P phases start only after R6; P1 is the minimum bridge |
| Graph proliferation | Many handles/graphs | Main-owned fixed project graphs; release graphs are logical main graphs |
| Artifact/store divergence | Missing downloads | Ordered publish, read-back verification, backup manifest |
| Project status and PROV diverge | Lost history | Update status and append activity in one service call; never overwrite events |

---

## 12. Open Decisions

| # | Decision | Recommendation |
| --- | --- | --- |
| 1 | SHACL engine or bounded internal evaluator | Decide in R0; do not claim conformance without one |
| 2 | Public HTTP IRIs for `oreg:`/`pm:` vocabularies | Keep `urn:` internally; publish stable HTTPS aliases only when a domain is owned |
| 3 | External registry convergence | Keep both through R6; bridge in R7 |
| 4 | Artifact retention/GC | Keep all published artifacts; GC only unreferenced staging/temp files |
| 5 | Per-user roles/ACLs | Defer; document the single-token limitation |
| 6 | Git/ZIP sources | Defer to R7; local/HTTP covers the first delivery |
| 7 | Module inference | Default module plus explicit annotations; no heuristic module split in P1 |
| 8 | Logical compatibility | Structural only until a supported reasoner/profile is chosen |
| 9 | Full-text search | Start with SPARQL `CONTAINS`; add an index only after measurement |
| 10 | Project progress formula | Version and store the weighted formula; do not average task percentages |

---

## 13. Definition of Done

### Registry

- A family can be registered, ingested, analysed, validated, and published from a
  local file and an allow-listed HTTP source.
- All six RDF syntaxes round-trip; unsupported syntax fails with `Unsupported`.
- A release is immutable and addressable by version IRI and path-safe key.
- Publishing is idempotent for identical content and a conflict for changed
  content under an existing version.
- Terms, statistics, imports, dependencies, diffs, validation, and compatibility
  reports are queryable through HTTP, CLI, and MCP.
- Downloads are authenticated and path-safe; no directory listing exists.
- Multi-graph mode routes all registry graphs to main with a validated dynamic
  prefix.

### Project layer

- A project can express goals, requirements, work, deliverables, milestones, and
  acceptance against exact release IRIs.
- Task and deliverable transitions are validated and recorded with PROV.
- Acceptance requires evidence and never mutates the release.
- A snapshot reconstructs the project's plans and release set.
- Overview/timeline/impact endpoints agree with the underlying RDF.

### Quality gates

- `cargo test --workspace`, `cargo clippy --workspace --all-targets`, and
  `cargo fmt --all -- --check` pass at every phase boundary.
- New public types and traits have doc comments; every new route appears in
  OpenAPI and has an HTTP test.
- `docs/registry-contract.md`, `docs/registry-operations.md`, and the relevant
  `infodocs/` pages are updated before the feature is considered shipped.
- Existing phase 1–8 tests remain green, including multi-graph and extraction
  compatibility tests.
