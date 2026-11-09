# DCAT-based RDF Ontology Management System

I would design this as a semantic registry for ontologies, vocabularies, schemas, and their versions, using DCAT as the catalog foundation rather than inventing another catalog model.

The central design principle is to separate four things:

1. Catalog — discovers and organizes ontology resources.
2. Ontology — the actual RDF/OWL vocabulary, including classes, properties, shapes, and axioms.
3. Release — an immutable, versioned publication of an ontology.
4. Analysis — machine-generated metadata about ontology contents, dependencies, compatibility, quality, and usage.

DCAT describes the catalog and its distributions. OWL, RDFS, SKOS, SHACL, and other semantic languages describe the ontology itself. A Rust service backed by Oxigraph can connect these layers without conflating catalog metadata with ontology semantics.

## 1. Standards to build on

Use a small set of existing vocabularies rather than defining a complete registry ontology from scratch.

| Vocabulary   | Responsibility in the system                                                                  |
| ------------ | --------------------------------------------------------------------------------------------- |
| DCAT 3       | Catalogs, cataloged ontology assets, releases, distributions, services, version relationships |
| ADMS         | Semantic-asset lifecycle, asset types, status, version history                                |
| OWL 2 / RDFS | Ontology identity, classes, properties, axioms, imports                                       |
| SKOS         | Taxonomies, concept schemes, controlled classifications                                       |
| SHACL        | Validation rules for catalog records and ontology graphs                                      |
| PROV-O       | Import, transformation, validation, publication, and other activity provenance                |
| VoID         | RDF dataset statistics and structural descriptions                                            |
| PROF         | Profiles and conformance relationships                                                        |

Important: ADMS is particularly relevant. It was explicitly designed as a DCAT profile for semantic assets such as ontologies, schemas, taxonomies, and code lists. Its original W3C publication is retired, but the vocabulary is now maintained by the SEMIC community. I would use the current SEMIC-maintained vocabulary, checking its namespace and terms rather than treating the old W3C document as the current authority.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.w3.org\&sz=32)

W3C+1



DCAT 3 is the current W3C Recommendation. It adds versioning and dataset-series support while retaining the DCAT namespace and backward compatibility with DCAT 2.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.w3.org\&sz=32)

Version 3+1



A key modeling decision: An ontology is a semantic asset, not merely a file. One ontology release can have several distributions: Turtle, RDF/XML, JSON-LD, N-Triples, documentation HTML, and perhaps a packaged archive. These are different representations of the same release, not separate ontologies. This is precisely the distinction made by ADMS and DCAT.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.w3.org\&sz=32)

W3C+1



## 2. Conceptual architecture

Ontology Registry

Catalog discovery · lifecycle · access control · search

Catalog layer

DCAT · ADMS · SKOS

Ontology layer

OWL · RDFS · SHACL

Release layer

Immutable versions · distributions · checksums

Analysis layer

Imports · terms · dependencies · compatibility

Rust application services

Ingestion · parsing · validation · indexing · publishing · API

Oxigraph + artifact storage

Named RDF graphs · SPARQL · immutable files · content hashes

The registry should manage both externally sourced ontologies and locally authored ontologies. It should also support ontologies assembled from multiple modules, profiles that constrain other ontologies, and application-specific extensions.

### Main resource hierarchy

```
Catalog
 ├── CatalogRecord
 │    └── OntologyAsset
 │         ├── OntologyRelease 1.0
 │         │    ├── Turtle Distribution
 │         │    ├── RDF/XML Distribution
 │         │    ├── JSON-LD Distribution
 │         │    └── Documentation Distribution
 │         ├── OntologyRelease 1.1
 │         │    └── ...
 │         └── OntologyRelease 2.0
 │
 ├── CatalogRecord
 │    └── OntologyAsset
 │
 └── CatalogRecord
      └── VocabularyAsset
```

Use stable IRIs for the asset and separate IRIs for each immutable release. Each release has its own ontology version IRI, distributions, validation results, and provenance.

## 3. RDF data model

I would introduce a very small custom namespace, perhaps `oreg:` (`Ontology Registry`), for metadata that existing standards do not adequately express.

Avoid redefining DCAT or ADMS. Use their classes and properties wherever possible.

### 3.1 Core entities

| Entity            | RDF representation                             | Meaning                                          |
| ----------------- | ---------------------------------------------- | ------------------------------------------------ |
| Registry          | `dcat:Catalog`                                 | A collection of ontology assets                  |
| Catalog entry     | `dcat:CatalogRecord`                           | A record describing an asset                     |
| Ontology family   | `dcat:DatasetSeries`                           | A sequence of releases belonging to one ontology |
| Ontology release  | `adms:Asset` / `dcat:Dataset`                  | A specific immutable semantic release            |
| Distribution      | `dcat:Distribution` / `adms:AssetDistribution` | A serialization or package                       |
| Ontology document | `owl:Ontology`                                 | The ontology described by a release              |
| Ontology module   | Custom `oreg:Module`                           | A logical subdivision of an ontology             |
| Import dependency | `owl:imports`                                  | A dependency on another ontology                 |
| Validation result | `sh:ValidationReport`                          | Results of validating a release                  |
| Analysis report   | Custom `oreg:AnalysisReport`                   | Computed metadata about the ontology             |

I would distinguish an ontology family from its releases explicitly. The family has a stable identity, while each release is immutable and has its own version-specific identity.

For OWL ontologies, use the ontology IRI and `owl:versionIRI` conventions. OWL 2 recommends that the unversioned ontology IRI resolve to the current version, while older releases remain accessible at their version IRIs.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.w3.org\&sz=32)

W3C



### 3.2 Custom registry vocabulary

These are proposed application terms, not established standard terms.

Classes

| Class                      | Purpose                                          |
| -------------------------- | ------------------------------------------------ |
| `oreg:Ontology`            | An ontology family                               |
| `oreg:OntologyRelease`     | An immutable ontology version                    |
| `oreg:Module`              | A logical module within an ontology              |
| `oreg:OntologyTerm`        | A catalog-indexed class, property, or individual |
| `oreg:ImportDependency`    | A resolved or unresolved dependency record       |
| `oreg:CompatibilityReport` | Compatibility assessment between releases        |
| `oreg:AnalysisReport`      | A machine-generated ontology analysis            |
| `oreg:ValidationRun`       | A validation execution                           |
| `oreg:ReleaseCandidate`    | A proposed release awaiting publication          |

Properties

| Property                      | Purpose                                         |
| ----------------------------- | ----------------------------------------------- |
| `oreg:hasRelease`             | Links ontology family to release                |
| `oreg:currentRelease`         | Identifies the selected current release         |
| `oreg:hasModule`              | Links release to modules                        |
| `oreg:moduleOf`               | Reverse module relationship                     |
| `oreg:namespaceIRI`           | Namespace of an ontology                        |
| `oreg:ontologyIRI`            | Logical ontology identifier                     |
| `oreg:versionIRI`             | Version-specific ontology identifier            |
| `oreg:importsOntology`        | Resolved ontology dependency                    |
| `oreg:unresolvedImport`       | Import that could not be resolved               |
| `oreg:hasTerm`                | Links a release or module to indexed terms      |
| `oreg:termKind`               | Class, object property, datatype property, etc. |
| `oreg:hasValidationReport`    | Validation results                              |
| `oreg:hasCompatibilityReport` | Compatibility analysis                          |
| `oreg:contentHash`            | Immutable artifact digest                       |
| `oreg:releaseChannel`         | Stable, beta, experimental, archived            |
| `oreg:breakingChange`         | Records a breaking change                       |
| `oreg:generatedBy`            | Analysis or transformation provenance           |

Use `owl:versionIRI` on the actual ontology document. Use `dcat:version`, `dcat:previousVersion`, `dcat:hasVersion`, and `dcat:hasCurrentVersion` for catalog-level version relationships. These properties complement rather than replace OWL's ontology-specific versioning.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.w3.org\&sz=32)

Version 3+1



### 3.3 Example RDF

This example shows an ontology family, one release, its distributions, and an import dependency.

```
@prefix dcat:    <http://www.w3.org/ns/dcat#> .
@prefix dcterms: <http://purl.org/dc/terms/> .
@prefix adms:    <http://www.w3.org/ns/adms#> .
@prefix owl:     <http://www.w3.org/2002/07/owl#> .
@prefix oreg:    <https://example.org/ontology/registry#> .
@prefix ex:      <https://example.org/ontology/project#> .

<https://example.org/catalog>
    a dcat:Catalog ;
    dcterms:title "Ontology Registry" ;
    dcat:dataset ex:series .

ex:series
    a dcat:DatasetSeries ;
    a oreg:Ontology ;
    dcterms:title "Project Ontology" ;
    oreg:namespaceIRI
        "https://example.org/ontology/project#" ;
    dcat:hasCurrentVersion ex:release-1.0 ;
    dcat:hasVersion ex:release-1.0 .

ex:release-1.0
    a adms:Asset ;
    a oreg:OntologyRelease ;
    dcterms:title "Project Ontology 1.0" ;
    dcat:isVersionOf ex:series ;
    dcat:version "1.0" ;
    dcterms:issued "2026-11-08"^^<http://www.w3.org/2001/XMLSchema#date> ;
    oreg:ontologyIRI
        "https://example.org/ontology/project" ;
    oreg:versionIRI
        "https://example.org/ontology/project/1.0" ;
    oreg:contentHash "sha256:..." ;
    dcat:distribution ex:release-1.0-ttl,
                      ex:release-1.0-jsonld .

ex:release-1.0-ttl
    a dcat:Distribution ;
    dcterms:title "Turtle serialization" ;
    dcat:downloadURL
        <https://example.org/ontology/project/1.0.ttl> ;
    dcat:mediaType
        <https://www.iana.org/assignments/media-types/text/turtle> .

ex:release-1.0-jsonld
    a dcat:Distribution ;
    dcterms:title "JSON-LD serialization" ;
    dcat:downloadURL
        <https://example.org/ontology/project/1.0.jsonld> ;
    dcat:mediaType
        <https://www.iana.org/assignments/media-types/application/ld+json> .

<https://example.org/ontology/project/1.0>
    a owl:Ontology ;
    owl:versionIRI
        <https://example.org/ontology/project/1.0> ;
    owl:imports
        <http://www.w3.org/ns/prov#> .
```

In production, use named IRIs for every important resource, including distributions, validation runs, modules, and catalog records. DCAT 3 explicitly recommends globally identifiable resources rather than relying on blank nodes when resources may need to be independently linked or annotated.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.w3.org\&sz=32)

Version 3



## 4. Ontology ingestion and publication

The most important part of the system is the ingestion pipeline. It should convert arbitrary ontology files into a consistent, analyzable, versioned registry representation.

1. Register source

   Accept a file upload, Git repository, URL, package, or existing RDF resource.
2. Parse and normalize

   Detect syntax, parse RDF, resolve prefixes, identify ontology IRIs and namespaces.
3. Identify ontology structure

   Discover classes, properties, individuals, restrictions, annotations, imports, and modules.
4. Resolve dependencies

   Fetch or match imported ontologies against known registry releases.
5. Analyze and validate

   Run SHACL checks, ontology consistency checks, structural analysis, and dependency checks.
6. Create release candidate

   Generate metadata, distributions, checksums, and reports.
7. Publish

   Assign immutable release identity, update the current-release pointer, and expose distributions.

### Ingestion sources

Support these source types:

- Local RDF files: Turtle, N-Triples, N-Quads, RDF/XML, JSON-LD.
- HTTP-accessible ontology documents.
- Git repositories and tagged releases.
- ZIP/TAR packages containing multiple ontologies.
- Existing ontology registries and DCAT catalogs.
- Generated ontologies from the user's own modeling tools.

For external imports, maintain a configurable resolver. An import IRI should first be matched against registered ontology identifiers and versions, then against approved external sources. Avoid unrestricted recursive fetching from arbitrary URLs.

### Analysis outputs

Every ingestion should produce structured metadata, not just a successful/failed result.

| Analysis      | Information produced                                   |
| ------------- | ------------------------------------------------------ |
| Identity      | Ontology IRI, version IRI, namespace, preferred prefix |
| Structure     | Classes, properties, individuals, restrictions         |
| Taxonomy      | Subclass trees, subproperties, SKOS concept schemes    |
| Dependencies  | Imports, dependency graph, missing imports             |
| Statistics    | Triple count, named graph count, term counts           |
| Documentation | Labels, definitions, comments, examples                |
| Validation    | SHACL conformance and validation messages              |
| Compatibility | Added, removed, changed terms and axioms               |
| Provenance    | Source, author, importer, timestamp, transformations   |

The ontology analysis should be reproducible. Store the tool version, configuration, input content hash, and execution provenance alongside every report.

## 5. Versioning, dependencies, and compatibility

This deserves its own subsystem because ontology versioning is more complicated than simply storing files with version numbers.

### 5.1 Identity model

Use three different identities:

| Identity            | Example                                        | Stability |
| ------------------- | ---------------------------------------------- | --------- |
| Ontology family IRI | `https://example.org/ontology/project`         | Permanent |
| Version IRI         | `https://example.org/ontology/project/1.0`     | Permanent |
| Distribution URL    | `https://example.org/ontology/project/1.0.ttl` | Permanent |

A release should never be overwritten. A correction produces another release, even if the correction is small. A mutable family IRI resolves to the selected current release.

For a release, preserve:

- Original source files.
- Normalized RDF representation.
- Content-addressed artifact hashes.
- Ontology IRI and version IRI.
- Release notes and status.
- Import lock information.
- Validation and analysis reports.
- Provenance.

### 5.2 Dependency graph

Model imports as a first-class graph, rather than just a list of `owl:imports` statements.

```
Project Ontology 1.2
 ├── PROV-O
 ├── OWL-Time
 ├── SKOS
 └── Organization Ontology
      └── FOAF
```

The dependency subsystem should provide:

1. Import discovery.
2. IRI resolution.
3. Version selection.
4. Dependency graph construction.
5. Circular dependency detection.
6. Missing dependency detection.
7. Transitive dependency calculation.
8. Dependency pinning for reproducible builds.

Keep the original `owl:imports` statements in the ontology. Add registry metadata recording which exact release satisfied each import. This gives you both standards-compatible ontology content and deterministic application-level dependency resolution.

A release manifest could contain:

```
ontology: https://example.org/ontology/project
version: 1.2.0
imports:
  - ontology: http://www.w3.org/ns/prov#
    version: registry:prov/2013
  - ontology: http://www.w3.org/2004/02/skos/core
    version: registry:skos/2009
```

This is an application manifest, not a replacement for RDF import declarations.

### 5.3 Compatibility analysis

Do not treat every RDF difference as a breaking change. Analyze changes by semantic category.

| Change                       | Suggested classification |
| ---------------------------- | ------------------------ |
| Add class                    | Usually compatible       |
| Add annotation property      | Usually compatible       |
| Add optional property        | Usually compatible       |
| Remove class                 | Breaking                 |
| Remove property              | Breaking                 |
| Rename an IRI                | Breaking                 |
| Change domain or range       | Potentially breaking     |
| Narrow a class restriction   | Potentially breaking     |
| Widen a class restriction    | Context-dependent        |
| Change equivalence axioms    | Requires semantic review |
| Change labels or definitions | Documentation change     |

The analyzer should produce a report with three levels:

- Structural compatibility: Are existing terms and references still present?
- Logical compatibility: Do axioms or inferred relationships change?
- Application compatibility: Do existing data graphs and SHACL profiles still validate?

Logical compatibility requires care: RDF graph diffs alone cannot establish semantic equivalence. Make the reasoner and supported OWL profile explicit.

### 5.4 Profiles and conformance

An ontology can be used with one or more application profiles.

For example:

```
Project Ontology
 ├── Core Ontology
 ├── Planning Profile
 ├── Workflow Profile
 └── Catalog Profile
```

A profile describes constraints on how a vocabulary is used. PROF is useful for documenting profiles and their conformance relationships; SHACL provides executable graph validation. The newer W3C Profiles Vocabulary draft explicitly discusses the relationship between PROF, ADMS, and DCAT.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.w3.org\&sz=32)

W3C



This lets the registry answer questions such as:

- Which profiles use this ontology?
- Which ontology versions conform to this profile?
- Which application datasets pass the profile's SHACL shapes?
- What breaks if a profile changes?

## 6. Oxigraph storage design

I would use Oxigraph as the semantic source of truth, with a separate artifact store for the original files and generated distributions.

### 6.1 Named graph partitioning

Do not put everything into one undifferentiated default graph.

Use named graphs with clear ownership and lifecycle boundaries:

| Graph                              | Contents                                          |
| ---------------------------------- | ------------------------------------------------- |
| `registry:catalog`                 | DCAT catalog, catalog records, asset descriptions |
| `registry:ontology:{id}:{version}` | Parsed ontology RDF                               |
| `registry:analysis:{id}:{version}` | Indexed terms, statistics, dependency analysis    |
| `registry:validation:{run}`        | SHACL validation reports                          |
| `registry:provenance:{run}`        | PROV-O activities and agents                      |
| `registry:profiles:{id}:{version}` | Profile descriptions and SHACL shapes             |
| `registry:staging:{id}`            | Unpublished candidate data                        |

The exact IRI patterns are implementation conventions, not standard DCAT requirements.

Keep ontology content separate from registry-generated metadata. For example, an ontology author should not have their original RDF silently modified by the registry to add labels, catalog records, or analysis results.

### 6.2 Storage lifecycle

```
Original file
     │
     ▼
Artifact store ──────── immutable
     │
     ▼
RDF parser
     │
     ▼
Staging named graph
     │
     ├── Structural analysis
     ├── Dependency resolution
     ├── SHACL validation
     └── Compatibility checks
     │
     ▼
Release transaction
     │
     ├── Immutable ontology graph
     ├── DCAT catalog update
     ├── Analysis graph
     └── Provenance graph
```

Use a content-addressed artifact store for source files and distributions, with SHA-256 hashes. This could initially be a filesystem directory structure and later S3-compatible object storage.

### 6.3 Important Oxigraph design considerations

- Use SPARQL for catalog and graph queries.
- Use RDF graph parsing and serialization for ingestion and distribution generation.
- Keep staging graphs separate from published release graphs.
- Publish metadata and release graph changes atomically where possible.
- Do not depend on blank nodes for persistent registry identity.
- Use deterministic IRIs for generated term-index records.
- Use graph-level access control in the application layer; do not assume named graphs alone enforce authorization.
- Keep derived analysis graphs disposable and regenerable.

One subtle but important distinction: the original ontology graph should contain the ontology's actual RDF. The term index should contain registry-generated descriptions and search metadata. This prevents the registry from confusing its interpretation of an ontology with the ontology's authoritative contents.

## 7. Rust service architecture

I would structure the backend as a small set of domain services, all using shared RDF types and a common repository abstraction.

```
ontology-registry/
├── crates/
│   ├── registry-core/
│   ├── registry-rdf/
│   ├── registry-store/
│   ├── registry-catalog/
│   ├── registry-ontology/
│   ├── registry-import/
│   ├── registry-analysis/
│   ├── registry-validation/
│   ├── registry-versioning/
│   ├── registry-publish/
│   ├── registry-api/
│   └── registry-cli/
```

### 7.1 Responsibilities

| Crate                 | Responsibility                                   |
| --------------------- | ------------------------------------------------ |
| `registry-core`       | IDs, domain types, errors, traits                |
| `registry-rdf`        | RDF parsing, serialization, vocabulary constants |
| `registry-store`      | Oxigraph transactions, named graphs, SPARQL      |
| `registry-catalog`    | DCAT and ADMS catalog operations                 |
| `registry-ontology`   | Ontology identity, terms, modules, axioms        |
| `registry-import`     | Source fetching, parsing, import resolution      |
| `registry-analysis`   | Statistics, term index, dependency graph, diffs  |
| `registry-validation` | SHACL and ontology validation                    |
| `registry-versioning` | Release identity and version relationships       |
| `registry-publish`    | Immutable distributions and publication          |
| `registry-api`        | Async HTTP API                                   |
| `registry-cli`        | User-facing command line                         |

### 7.2 Core Rust traits

Keep the domain interfaces small. A possible initial design:

```
pub trait CatalogRepository {
    async fn get_catalog(&self, id: &CatalogId)
        -> Result<Catalog, RegistryError>;

    async fn list_assets(&self, query: AssetQuery)
        -> Result<Vec<OntologyAsset>, RegistryError>;

    async fn insert_asset(&self, asset: OntologyAsset)
        -> Result<(), RegistryError>;
}

pub trait OntologyRepository {
    async fn load_release(&self, id: &ReleaseId)
        -> Result<OntologyRelease, RegistryError>;

    async fn load_graph(&self, id: &ReleaseId)
        -> Result<OntologyGraph, RegistryError>;

    async fn store_candidate(&self, candidate: Candidate)
        -> Result<(), RegistryError>;
}

pub trait OntologyAnalyzer {
    async fn analyze(&self, graph: &OntologyGraph)
        -> Result<AnalysisReport, RegistryError>;
}

pub trait DependencyResolver {
    async fn resolve(
        &self,
        ontology: &OntologyGraph,
    ) -> Result<DependencyGraph, RegistryError>;
}

pub trait Validator {
    async fn validate(
        &self,
        graph: &OntologyGraph,
        profile: &ValidationProfile,
    ) -> Result<ValidationReport, RegistryError>;
}

pub trait ReleasePublisher {
    async fn publish(
        &self,
        candidate: CandidateId,
    ) -> Result<ReleaseId, RegistryError>;
}
```

These are illustrative interfaces; adapt async trait support and concrete error types to the chosen Rust stack. The important point is that the registry domain should not depend directly on Oxigraph's storage types. The Oxigraph adapter implements the repository traits.

### 7.3 Typed domain objects

Prefer explicit types instead of passing arbitrary strings throughout the system:

```
pub struct OntologyId(pub Iri);
pub struct ReleaseId(pub Iri);
pub struct ModuleId(pub Iri);
pub struct CatalogId(pub Iri);

pub struct OntologyRelease {
    pub id: ReleaseId,
    pub ontology: OntologyId,
    pub version: String,
    pub version_iri: Iri,
    pub distributions: Vec<Distribution>,
    pub imports: Vec<OntologyDependency>,
    pub status: ReleaseStatus,
}

pub struct OntologyDependency {
    pub ontology: OntologyId,
    pub requested_version: Option<String>,
    pub resolved_release: Option<ReleaseId>,
}

pub enum ReleaseStatus {
    Draft,
    Candidate,
    Stable,
    Deprecated,
    Withdrawn,
    Archived,
}
```

I would make the distinction between a family and a release impossible to accidentally erase in the type system. It is a common source of subtle bugs in registries.

## 8. HTTP API

Use an async HTTP API with REST endpoints for resource lifecycle operations and SPARQL for flexible semantic queries.

Suggested implementation: Axum + Tokio + Oxigraph.

### 8.1 Catalog API

| Method  | Endpoint                                        | Purpose                 |
| ------- | ----------------------------------------------- | ----------------------- |
| `GET`   | `/catalogs`                                     | List catalogs           |
| `POST`  | `/catalogs`                                     | Create catalog          |
| `GET`   | `/catalogs/{id}`                                | Retrieve catalog        |
| `GET`   | `/catalogs/{id}/assets`                         | List ontology assets    |
| `POST`  | `/catalogs/{id}/assets`                         | Register an asset       |
| `GET`   | `/assets/{id}`                                  | Retrieve asset metadata |
| `PATCH` | `/assets/{id}`                                  | Update asset metadata   |
| `GET`   | `/assets/{id}/releases`                         | List releases           |
| `GET`   | `/assets/{id}/releases/{version}`               | Retrieve release        |
| `GET`   | `/assets/{id}/releases/{version}/distributions` | List representations    |

### 8.2 Ontology API

| Method | Endpoint                        | Purpose                       |
| ------ | ------------------------------- | ----------------------------- |
| `GET`  | `/ontologies/{id}/terms`        | Search classes and properties |
| `GET`  | `/ontologies/{id}/terms/{term}` | Retrieve term description     |
| `GET`  | `/ontologies/{id}/modules`      | List modules                  |
| `GET`  | `/releases/{id}/imports`        | Dependency graph              |
| `GET`  | `/releases/{id}/statistics`     | Ontology statistics           |
| `GET`  | `/releases/{id}/diff/{other}`   | Compare releases              |
| `POST` | `/releases/{id}/validate`       | Validate against a profile    |
| `GET`  | `/releases/{id}/reports`        | List analysis reports         |

### 8.3 Ingestion and publication API

| Method | Endpoint                             | Purpose                  |
| ------ | ------------------------------------ | ------------------------ |
| `POST` | `/ingestions`                        | Start an ingestion       |
| `GET`  | `/ingestions/{id}`                   | Get ingestion status     |
| `POST` | `/ingestions/{id}/analyze`           | Run analysis             |
| `POST` | `/ingestions/{id}/validate`          | Run validation           |
| `POST` | `/ingestions/{id}/resolve-imports`   | Resolve dependencies     |
| `POST` | `/ingestions/{id}/release-candidate` | Create release candidate |
| `POST` | `/candidates/{id}/publish`           | Publish release          |

### 8.4 Semantic query API

Expose:

```
POST /sparql
GET  /sparql
GET  /rdf/{resource}
GET  /download/{release}/{format}
```

The SPARQL endpoint should support read-only queries for ordinary users. Update operations should go through authorized application services, not unrestricted SPARQL Update.

Support RDF content negotiation for resource IRIs:

- `text/turtle`
- `application/ld+json`
- `application/rdf+xml`
- `application/n-triples`
- `application/n-quads`

The RDF endpoint should serve the registry description separately from the actual ontology distribution. This is important because the catalog description and ontology document are different resources.

## 9. CLI design

The CLI should expose the entire registry through a compact command hierarchy.

I would call it `oreg`.

```
oreg catalog list
oreg catalog show main
oreg catalog create main

oreg ontology register ./project.ttl
oreg ontology import https://example.org/ontology/project
oreg ontology search "project task"
oreg ontology show project
oreg ontology terms project
oreg ontology term project:Task

oreg release list project
oreg release show project 1.0
oreg release diff project 1.0 1.1
oreg release validate project 1.1
oreg release publish project

oreg imports show project 1.0
oreg imports resolve project 1.0
oreg imports graph project 1.0

oreg analysis run project 1.0
oreg analysis statistics project 1.0
oreg analysis dependencies project 1.0
oreg analysis compatibility project 1.0

oreg profile list
oreg profile validate project 1.0 planning-profile

oreg query "SELECT ..."
oreg export project 1.0 --format turtle
```

### Useful CLI behavior

- `--json` for machine-readable output.
- `--quiet` for scripting.
- `--server` to select a registry instance.
- `--catalog` to select a catalog.
- `--version` to pin a release.
- `--format` to select RDF serialization.
- `--dry-run` for ingestion and publication.
- `--recursive` for import resolution.
- `--include-dependencies` for exports.
- `--explain` for compatibility and validation diagnostics.

The CLI should work against the HTTP API rather than implementing a separate storage path. This keeps local scripting, remote operation, and agent access consistent.

## 10. Ontology discovery and search

A registry should do more than search titles and descriptions. It should provide semantic discovery.

I would implement several complementary indexes.

| Search type          | Example                                          |
| -------------------- | ------------------------------------------------ |
| Metadata search      | Find ontologies about planning                   |
| Namespace search     | Find resources in a namespace                    |
| Term search          | Find classes named `Task`                        |
| Definition search    | Find properties describing task dependencies     |
| Type search          | Find all OWL object properties                   |
| Dependency search    | Find ontologies importing PROV-O                 |
| Usage search         | Find ontologies using `schema:Action`            |
| Structural search    | Find ontologies with a class hierarchy           |
| Compatibility search | Find releases compatible with a selected profile |

### Term index

Each indexed term should be independently addressable.

For example:

```
https://example.org/ontology/project#Task
```

The registry should capture:

- IRI
- Local name
- Preferred label
- Definition
- Comments
- Ontology family
- Release
- Module
- RDF type
- Superclasses
- Superproperties
- Domains and ranges
- Deprecation status
- Equivalent and related terms

Do not copy term definitions into the catalog graph as if they were original ontology statements. The index should reference the authoritative ontology graph and record its source release.

### Cross-ontology mapping

An especially useful feature would be a mapping registry:

```
Project Task
   ├── closeMatch → Schema.org Action
   ├── relatedMatch → P-Plan Step
   └── exactMatch → Internal Workflow Task
```

Use SKOS mapping properties when their semantics are appropriate. Use OWL equivalence only when the concepts really have equivalent formal meaning. A loose conceptual similarity should never automatically become `owl:equivalentClass`.

This feature could eventually become one of the registry's most valuable capabilities: discovering how different ontology families can work together.

## 11. LLM and MCP interface

I would make the registry particularly useful to agents by exposing a very small set of high-level tools.

Rather than giving an LLM unrestricted SPARQL access as its primary interface, expose semantic operations:

| Tool       | Purpose                                                 |
| ---------- | ------------------------------------------------------- |
| `catalog`  | Search and inspect catalogs and ontology assets         |
| `ontology` | Inspect ontology metadata, terms, modules, and releases |
| `query`    | Run constrained SPARQL queries                          |
| `analyze`  | Analyze ontology structure and dependencies             |
| `validate` | Validate an ontology or data graph against a profile    |
| `compare`  | Compare ontology releases                               |
| `ingest`   | Register and process ontology sources                   |
| `publish`  | Create and publish ontology releases                    |

For example, an agent could ask:

```
catalog:
  search: "workflow planning"

ontology:
  inspect: "project-ontology"

analyze:
  release: "project-ontology/1.0"
  include:
    - imports
    - classes
    - properties
    - dependencies
```

The API should return compact, structured results with stable IRIs and source-release references. The agent should be able to discover a vocabulary, inspect its terms, understand dependencies, and select an appropriate version without downloading and parsing the entire ontology itself.

For write operations, require explicit permissions and publication approval. An agent should be able to create a candidate without automatically making it a stable release.

## 12. Operational workflows

The system should support several distinct workflows.

### A. Importing an external ontology

```
URL / Git / file
      ↓
Register asset
      ↓
Parse RDF
      ↓
Identify ontology and version
      ↓
Resolve imports
      ↓
Analyze and validate
      ↓
Create immutable release
      ↓
Publish catalog entry
```

### B. Developing an internal ontology

```
Create ontology family
      ↓
Create draft
      ↓
Add modules and terms
      ↓
Validate against profile
      ↓
Compare with previous release
      ↓
Review breaking changes
      ↓
Publish version
```

### C. Updating an ontology

```
New upstream version
      ↓
Import into staging
      ↓
Calculate structural diff
      ↓
Check dependent ontologies
      ↓
Validate dependent profiles
      ↓
Generate compatibility report
      ↓
Approve and publish
```

### D. Building an ontology collection

This would let you create curated groups for particular domains:

```
Semantic Systems Catalog
 ├── Document Processing
 │    ├── DoCO
 │    ├── BIBO
 │    └── PROV-O
 ├── Planning
 │    ├── P-Plan
 │    ├── PROV-O
 │    └── OWL-Time
 ├── Workflow
 │    ├── FnO
 │    ├── Wfdesc
 │    └── WFPROV
 └── Project Management
      ├── PROMCODE
      ├── ORG
      └── QUDT
```

These collections should be represented as DCAT catalogs or curated catalog subsets, not hard-coded application categories.

## 13. Suggested implementation phases

I would build this in six phases, with a working registry after the first two.

| Phase                           | Scope                                                     | Deliverable                       |
| ------------------------------- | --------------------------------------------------------- | --------------------------------- |
| 1. Catalog foundation           | DCAT, ADMS, RDF storage, catalog CRUD                     | Working ontology catalog          |
| 2. Ingestion                    | RDF parsers, source registration, distributions, releases | Import and publish ontologies     |
| 3. Ontology analysis            | Term extraction, statistics, imports, dependency graphs   | Searchable ontology knowledgebase |
| 4. Validation and versioning    | SHACL, diffs, compatibility, release management           | Managed ontology lifecycle        |
| 5. API and agent access         | Axum API, CLI, MCP                                        | Full remote and agent interface   |
| 6. Advanced semantic management | Profiles, mappings, modularization, federated catalogs    | Ontology development platform     |

### My recommended initial technology choices

| Component          | Choice                                    |
| ------------------ | ----------------------------------------- |
| Language           | Rust                                      |
| RDF engine         | Oxigraph                                  |
| RDF serialization  | Oxigraph RDF parsers/serializers          |
| API                | Axum + Tokio                              |
| Validation         | SHACL validation adapter                  |
| Metadata model     | DCAT 3 + ADMS + Dublin Core               |
| Ontology semantics | OWL 2 + RDFS + SKOS                       |
| Provenance         | PROV-O                                    |
| Constraints        | SHACL                                     |
| Artifact storage   | Filesystem initially, S3-compatible later |
| CLI                | Clap                                      |
| Agent interface    | MCP over the HTTP service                 |

## 14. The most important architectural decision

I would make the system a registry of semantic assets with a semantic knowledgebase behind it, rather than a file repository with RDF metadata attached.

That means the registry itself understands:

- What ontologies exist.
- What each ontology defines.
- Which releases are available.
- Which ontologies depend on which other ontologies.
- Which profiles constrain them.
- Which terms overlap or correspond.
- Which applications use which ontology releases.
- Which releases are compatible with existing data and applications.

This naturally creates a second layer above DCAT: an ontology knowledge graph describing the ontology ecosystem itself.

The DCAT catalog remains the interoperable public-facing catalog. The registry's analysis graph provides the richer internal semantic capabilities.

One extension I would prioritize early: a first-class dependency and compatibility graph. Once you can traverse `application → profile → ontology release → imported ontology → term`, the registry becomes useful not only for finding ontologies, but also for selecting and maintaining the right ontology stack for a project.
