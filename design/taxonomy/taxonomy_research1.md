# Automatic Taxonomy Construction: Research and Architecture

Automatic taxonomy construction is the process of discovering concepts, organizing them into hierarchical categories, identifying parent-child relationships, and producing a machine-readable classification system from unstructured or semi-structured information.

For an RDF-based knowledge system using Oxigraph, this is particularly interesting because taxonomy construction can become an automated knowledge-engineering pipeline:

Documents → Concepts → Classification → Hierarchy → RDF/SKOS → Validation → Continuous refinement

The important distinction is between three related activities:

1. Taxonomy induction: discovering categories and hierarchical relationships from data.
2. Taxonomy enrichment: adding concepts, synonyms, definitions, and relationships to an existing taxonomy.
3. Taxonomy alignment: mapping independently constructed taxonomies to one another or to established vocabularies.

I would design a system that supports all three, rather than treating taxonomy construction as a one-time clustering operation.

## 1. Main approaches to automatic taxonomy construction

Research generally identifies several complementary approaches.

### A. Linguistic pattern extraction

Classical NLP

The most established approach is based on extracting is-a relationships from natural language.

The foundational work is Marti Hearst's 1992 paper, Automatic Acquisition of Hyponyms from Large Text Corpora.

It identifies linguistic patterns that indicate hierarchical relationships.

Examples:

| Text pattern         | Extracted relationship |
| -------------------- | ---------------------- |
| X such as A, B and C | A, B, C are types of X |
| X including A and B  | A and B are types of X |
| X is a type of Y     | X is a subtype of Y    |
| X and other Y        | X is a type of Y       |
| Y such as X          | X is a subtype of Y    |

For example:

> Databases such as PostgreSQL, MySQL and SQLite.

Produces:

```
PostgreSQL → Database
MySQL      → Database
SQLite     → Database
```

Advantages

- Relatively inexpensive.
- Explainable extraction.
- Works with conventional NLP pipelines.
- Extracted relationships can be tied to their source documents.

Disadvantages

- Requires linguistic evidence.
- Misses implicit relationships.
- Can confuse association with subclass relationships.
- Coverage varies considerably by corpus and language.

A major example of scaling this approach is Probase. The original work extracted millions of concepts and relationships from web documents.

The 2017 survey by Wang, He and Zhou is a particularly useful starting point for understanding this research area. [A Short Survey on Taxonomy Learning from Text Corpora](https://chywang.github.io/papers/emnlp2017b.pdf?utm_source=chatgpt.com).&#x20;

[image](https://www.google.com/s2/favicons?domain=https://chywang.github.io\&sz=32)

chywang.github.io+1



### B. Distributional semantics and embedding clustering

Unsupervised ML

This approach uses the semantic similarity of words, phrases, or documents to discover categories.

A typical process:

1. Extract terms and phrases.
2. Generate embeddings.
3. Calculate semantic similarities.
4. Cluster related terms.
5. Organize clusters hierarchically.
6. Generate category labels.

Example:

Embedding-based discovery

PostgreSQL

MySQL, SQLite, MariaDB

Redis

Memcached, Valkey

Neo4j

JanusGraph, Neptune

Hierarchical clustering

Database Systems

Relational

Key-value

Graph

Useful algorithms include:

| Algorithm                             | Application                                |
| ------------------------------------- | ------------------------------------------ |
| Hierarchical Agglomerative Clustering | Bottom-up taxonomy construction            |
| BIRCH                                 | Large-scale incremental clustering         |
| HDBSCAN                               | Discovering clusters with variable density |
| K-means                               | Flat category discovery                    |
| Spectral Clustering                   | Complex similarity structures              |
| Leiden                                | Graph community detection                  |

One particularly relevant paper is Automated Learning of Domain Taxonomies from Text Using Background Knowledge (2016).

It combines hierarchical agglomerative clustering, syntactic matching, semantic relatedness, and automated dendrogram pruning.

Its reported experiments achieved up to 0.71 average precision and 0.96 average recall for learned taxonomic relationships.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.sciencedirect.com\&sz=32)

ScienceDirect



Important limitation: semantic similarity does not imply an is-a relationship.

For example:

- PostgreSQL and MySQL are similar.
- PostgreSQL is a relational database.
- Relational database is a database.

The embedding space can identify related concepts, but additional reasoning is necessary to establish the hierarchy.

### C. Hypernym and hyponym discovery

This is a more specialized approach that attempts to identify the actual direction of taxonomic relationships.

- Hypernym: a more general concept.
- Hyponym: a more specific concept.

For example:

```
Animal (hypernym)
  Mammal (hyponym)
    Dog (hyponym)
      Labrador (hyponym)
```

Research approaches include:

- Lexico-syntactic pattern extraction.
- Distributional inclusion methods.
- Hypernym-specific embedding models.
- Transformer-based relation classifiers.
- Contextual entailment prediction.
- LLM-based parent candidate generation.

This distinction is important because a similarity model can tell you that two concepts are related, but a hypernym model attempts to tell you which concept is broader.

I would implement hypernym prediction as a separate subsystem, rather than treating it as part of clustering.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://chywang.github.io\&sz=32)

chywang.github.io+1



### D. LLM-generated taxonomies

Most interesting modern approach

LLMs can perform several tasks that were previously difficult to automate:

- Identify concepts.
- Deduplicate terminology.
- Discover implicit categories.
- Infer parent-child relationships.
- Generate category names.
- Write definitions.
- Identify classification dimensions.
- Refine existing hierarchies.

There are several particularly interesting research projects.

#### 1. TnT-LLM (2024)

[TnT-LLM: Text Mining at Scale with Large Language Models](https://www.microsoft.com/en-us/research/publication/tnt-llm-text-mining-at-scale-with-large-language-models/?msockid=036c56ec4c186aba0d18407e4d1f6bd6\&utm_source=chatgpt.com)

This is a two-phase system:

1. Iteratively generate and refine a label taxonomy.
2. Use LLMs to classify data and produce training examples for smaller supervised classifiers.

The interesting architectural idea is separating taxonomy construction from large-scale classification.

You don't need an LLM to classify every subsequent document. Once the taxonomy stabilizes, smaller models can handle much of the workload.

#### 2. TaxoAdapt (ACL 2025)

[TaxoAdapt paper](https://aclanthology.org/2025.acl-long.1442/?utm_source=chatgpt.com)

This is particularly relevant to dynamically evolving knowledge bases.

Its approach:

- Starts with an LLM-generated taxonomy.
- Uses iterative hierarchical classification.
- Expands taxonomy depth and width according to corpus distribution.
- Handles multiple dimensions of classification.

An important insight is that research papers can belong to multiple classification dimensions simultaneously.

For example, a paper can be categorized by:

- Research problem.
- Methodology.
- Dataset.
- Evaluation technique.

These are different taxonomies, not necessarily branches of one universal hierarchy.

#### 3. CAM-Taxo (EMNLP 2025)

[Context-Aware Hierarchical Taxonomy Generation](https://aclanthology.org/2025.emnlp-main.788/?utm_source=chatgpt.com)

This combines LLM-guided multi-aspect encoding with dynamic clustering.

Its process is especially interesting:

1. Extract different aspects of each document.
2. Generate aspect-specific summaries.
3. Embed each aspect independently.
4. Cluster documents within each aspect.
5. Construct hierarchical taxonomies.

The authors introduce a benchmark containing 156 expert-created taxonomies covering 11,600 papers.

This provides a useful model for constructing taxonomies from heterogeneous document collections.

#### 4. TaxoLLaMA (2026)

[Large Language Models for Creation, Enrichment and Evaluation of Taxonomic Graphs](https://journals.sagepub.com/doi/full/10.1177/22104968251404186?utm_source=chatgpt.com)

This work introduces a taxonomy-specific instruction dataset derived from WordNet and a fine-tuned model called TaxoLLaMA.

It targets multiple taxonomy operations:

- Taxonomy construction.
- Taxonomy enrichment.
- Hypernym discovery.
- Lexical entailment.

The interesting aspect is that these operations can be treated as a common family of related model tasks rather than requiring unrelated models.

#### 5. TaxCE (EMNLP 2026)

Especially worth studying

[TaxCE: A Framework for Automated Taxonomy Construction and Evaluation at Scale](https://aclanthology.org/2026.emnlp-industry.76/?utm_source=chatgpt.com)

This is one of the most relevant recent developments.

TaxCE builds taxonomies from raw text using progressive condensation:

```
Raw documents
     ↓
Actionable segments
     ↓
Deduplicated semantic units
     ↓
Granular topics + definitions
     ↓
Bottom-up hierarchical construction
     ↓
Iterative evaluation and refinement
```

Its evaluation framework introduces three metrics:

- Exclusivity: categories should be semantically distinct.
- Exhaustivity: the taxonomy should cover the underlying corpus.
- Granularity: categories should be at an appropriate level of detail.

The interesting feature is its metrics-in-the-loop refinement process. Rather than accepting the first generated hierarchy, the system diagnoses weaknesses and makes targeted corrections.

This is a very good architectural direction for an RDF taxonomy-generation system.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://aclanthology.org\&sz=32)

ACL Anthology



#### 6. SC-Taxo (October 2026)

[SC-Taxo: Hierarchical Taxonomy Generation Under Semantic Consistency Constraints](https://www.sciengine.com/doi/10.3724/jtke-20260022?utm_source=chatgpt.com)

This is particularly interesting for ensuring the logical coherence of the resulting hierarchy.

It addresses two separate problems:

- Vertical consistency: children must be semantically compatible with their parent.
- Horizontal consistency: sibling categories should be distinct and appropriately granular.

Its approach combines bottom-up abstraction with top-down semantic constraints.

This is a useful principle: a taxonomy must be evaluated across relationships between levels, not merely by the quality of individual category labels.

The paper reports improvements on TaxoBench using structural and semantic evaluation metrics.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.sciengine.com\&sz=32)

Sciengine+1



## 2. Comparison of construction strategies

| Method                      | Strength                    | Weakness                   | Best application       |
| --------------------------- | --------------------------- | -------------------------- | ---------------------- |
| Hearst patterns             | Explainable relationships   | Limited coverage           | Text extraction        |
| Embedding clustering        | Scalable discovery          | Similarity isn't hierarchy | Concept discovery      |
| Hypernym prediction         | Directed semantic relations | Requires training/evidence | Parent inference       |
| Topic modeling              | Corpus-wide themes          | Often shallow              | Initial categorization |
| LLM generation              | Semantic abstraction        | Hallucinations, cost       | Hierarchy synthesis    |
| LLM + clustering            | Grounded structure          | More engineering           | Production systems     |
| Existing taxonomy expansion | Consistency                 | Inherited biases           | Enrichment             |

My preference would be a hybrid approach:

Embeddings for discovery, linguistic extraction for evidence, LLMs for abstraction, and symbolic validation for correctness.

## 3. Taxonomy construction should be multidimensional

This is one of the most important design considerations.

A conventional taxonomy is usually represented as a tree:

```
Technology
  ├── Software
  │    ├── Databases
  │    └── Operating Systems
  └── Hardware
       ├── Processors
       └── Storage
```

But real-world knowledge rarely fits into one hierarchy.

Consider PostgreSQL:

- Software → Database Systems → Relational Databases
- Technology → Open Source Software
- Infrastructure → Data Storage
- Software → Server Applications

These are different classification perspectives.

I recommend supporting three distinct structures.

### A. Single-inheritance taxonomy

A concept has one immediate broader concept.

Useful for:

- Product categories.
- Document navigation.
- Basic classification.
- Catalog organization.

### B. Polyhierarchical taxonomy

A concept has multiple broader concepts.

Useful for:

- Knowledge management.
- Scientific literature.
- Technical terminology.
- Cross-domain classification.

SKOS supports this through multiple `skos:broader` relationships.

### C. Faceted taxonomy

A concept can be classified independently along multiple dimensions.

For example:

| Dimension    | Example      |
| ------------ | ------------ |
| Technology   | Database     |
| Architecture | Distributed  |
| License      | Open source  |
| Deployment   | Self-hosted  |
| Function     | Data storage |

I would represent these as separate SKOS concept schemes, connected through explicit concept mappings.

This is generally better than forcing every classification dimension into one large hierarchy.

## 4. RDF vocabularies for automatic taxonomy construction

The natural foundation is SKOS.

[W3C SKOS Reference](https://www.w3.org/TR/skos-reference/?lang=en\&utm_source=chatgpt.com)

### Core SKOS model

| Property             | Purpose                  |
| -------------------- | ------------------------ |
| `skos:ConceptScheme` | Taxonomy                 |
| `skos:Concept`       | Category/concept         |
| `skos:prefLabel`     | Preferred label          |
| `skos:altLabel`      | Synonyms                 |
| `skos:hiddenLabel`   | Search variants          |
| `skos:definition`    | Definition               |
| `skos:scopeNote`     | Scope restrictions       |
| `skos:broader`       | Immediate parent         |
| `skos:narrower`      | Immediate child          |
| `skos:related`       | Associative relationship |
| `skos:topConceptOf`  | Root concept             |
| `skos:inScheme`      | Scheme membership        |
| `skos:exactMatch`    | Equivalent concept       |
| `skos:closeMatch`    | Similar concept          |

SKOS deliberately separates concept organization from formal OWL class semantics.

This is useful because a taxonomy category doesn't necessarily represent a formal class with strict logical constraints.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.w3.org\&sz=32)

W3C



### Additional vocabularies

| Vocabulary | Role                                               |
| ---------- | -------------------------------------------------- |
| SKOS-XL    | Rich lexical labels and label metadata             |
| PROV-O     | Provenance of extracted concepts and relationships |
| DCAT       | Cataloging taxonomy datasets and versions          |
| DCTERMS    | Metadata and descriptions                          |
| OWL        | Formal concept relationships and constraints       |
| SHACL      | Taxonomy validation                                |
| RDF-star   | Potentially compact statement-level metadata       |

For taxonomy generation, provenance is particularly important.

You should know not only that:

```
ex:PostgreSQL skos:broader ex:RelationalDatabase .
```

but also:

- Which document supported the relationship?
- Which extraction method discovered it?
- Was it inferred by an LLM?
- What was the confidence?
- Was it independently verified?
- Which taxonomy generation version introduced it?

I would use a separate provenance graph for this information.

## 5. Recommended RDF extension for taxonomy learning

I would introduce a small custom vocabulary for taxonomy construction, using SKOS for the actual taxonomy and PROV-O for the history.

For example:

```
@prefix tx: <https://example.org/taxonomy#> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
@prefix prov: <http://www.w3.org/ns/prov#> .

ex:Database
    a skos:Concept ;
    skos:prefLabel "Database" ;
    skos:definition "A system for organizing and managing data." .

ex:PostgreSQL
    a skos:Concept ;
    skos:prefLabel "PostgreSQL" ;
    skos:broader ex:RelationalDatabase .

ex:RelationEvidence1
    a tx:RelationEvidence ;
    tx:subject ex:PostgreSQL ;
    tx:predicate skos:broader ;
    tx:object ex:RelationalDatabase ;
    tx:confidence 0.98 ;
    tx:method tx:HearstExtraction ;
    tx:source ex:Document123 .

ex:Generation1
    a prov:Activity, tx:TaxonomyGeneration ;
    prov:used ex:Document123 ;
    prov:generated ex:PostgreSQL ;
    prov:wasAssociatedWith ex:TaxonomyAgent .
```

I would add these classes:

| Class                   | Purpose                        |
| ----------------------- | ------------------------------ |
| `tx:TaxonomyGeneration` | Generation run                 |
| `tx:ConceptCandidate`   | Proposed concept               |
| `tx:RelationCandidate`  | Proposed relationship          |
| `tx:RelationEvidence`   | Supporting evidence            |
| `tx:Classification`     | Document-to-concept assignment |
| `tx:TaxonomyRevision`   | Versioned changes              |
| `tx:TaxonomyEvaluation` | Quality assessment             |
| `tx:ExtractionMethod`   | Extraction technique           |
| `tx:ConceptCluster`     | Intermediate semantic cluster  |

An important distinction: a proposed relationship should not automatically become an asserted `skos:broader` relationship.

Keep candidate relationships separate until they pass validation.

## 6. Proposed automatic taxonomy construction pipeline

For an Oxigraph-based knowledge system, I would build this as a multi-stage pipeline with independently executable operations.

## Taxonomy induction engine

1. Corpus ingestion

   Documents, RDF graphs, existing taxonomies, catalogs, databases and APIs.
2. Concept extraction

   Named entities, noun phrases, terminology, keywords and candidate concepts.
3. Concept normalization

   Deduplication, synonyms, canonical labels, entity resolution and disambiguation.
4. Semantic analysis

   Embeddings, contextual similarity, hypernym extraction and relationship candidates.
5. Concept clustering

   Discover groups, subgroups and independent classification dimensions.
6. Hierarchy induction

   Infer broader/narrower relationships and construct hierarchical structures.
7. Semantic refinement

   LLM-based abstraction, category naming, definitions, sibling consistency and parent-child validation.
8. Taxonomy validation

   SHACL constraints, cycle detection, coverage, exclusivity and granularity checks.
9. RDF publication

   Publish SKOS concepts, concept schemes, provenance and taxonomy versions into Oxigraph.

### Important architectural decision: bottom-up and top-down induction

I would run two complementary processes.

Bottom-up:

```
Documents
   ↓
Extracted concepts
   ↓
Semantic clusters
   ↓
Subcategories
   ↓
Parent categories
   ↓
Root concepts
```

Top-down:

```
Existing taxonomy
   ↓
Existing categories
   ↓
Classify new concepts
   ↓
Identify missing categories
   ↓
Expand taxonomy
```

The two processes should reconcile their results.

Bottom-up discovery prevents an existing taxonomy from restricting what the system can discover. Top-down classification prevents the system from constantly reinventing categories that already exist.

TaxoAdapt is particularly relevant to this approach.

## 7. Evaluation and quality control

A taxonomy that looks reasonable is not necessarily a good taxonomy.

I would implement explicit quality measurements.

| Metric                   | What it measures                                     |
| ------------------------ | ---------------------------------------------------- |
| Coverage                 | Percentage of concepts classified                    |
| Exclusivity              | Whether categories are distinct                      |
| Exhaustivity             | Whether the hierarchy covers the corpus              |
| Granularity              | Whether categories are appropriately specific        |
| Hierarchical consistency | Whether parent-child relationships make sense        |
| Semantic coherence       | Whether sibling concepts belong together             |
| Redundancy               | Duplicate or overlapping categories                  |
| Depth balance            | Whether hierarchy depth is reasonable                |
| Stability                | Whether repeated generation produces similar results |
| Provenance completeness  | Whether relationships have supporting evidence       |

### Three particularly useful metrics

1\. Exclusivity

A category should not substantially duplicate another category at the same level.

2\. Exhaustivity

The taxonomy should account for the relevant material in the corpus.

3\. Granularity

A category should not be excessively broad or unnecessarily specific.

These three metrics from TaxCE are particularly useful for automated iterative refinement.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://aclanthology.org\&sz=32)

ACL Anthology+1



I would also introduce a confidence model:

```
Relationship confidence =
    lexical evidence
  + embedding evidence
  + contextual evidence
  + LLM judgment
  + structural consistency
```

These should remain separately recorded signals, not simply averaged into an unexplained number.

A high-confidence semantic similarity score does not establish a valid taxonomic relationship.

## 8. Incremental taxonomy maintenance

One area that deserves special attention is maintaining taxonomies as new information arrives.

A production system should support:

- Adding new concepts.
- Adding narrower categories.
- Merging duplicate categories.
- Splitting overly broad categories.
- Moving concepts between categories.
- Introducing new classification dimensions.
- Detecting obsolete concepts.
- Reconciling competing taxonomies.
- Tracking changes across versions.

I would use a proposal-and-approval model:

```
New documents
     ↓
New concept candidates
     ↓
Existing taxonomy matching
     |
     +---- Match found → Classify
     |
     +---- No match → Candidate category
                         ↓
                   Relationship inference
                         ↓
                   Semantic validation
                         ↓
                   Taxonomy revision
                         ↓
                   Publish new version
```

Never silently rewrite an existing taxonomy when a new generation suggests a different hierarchy.

Instead, produce a revision containing explicit operations:

```
ADD_CONCEPT
ADD_BROADER_RELATION
REMOVE_BROADER_RELATION
MERGE_CONCEPTS
SPLIT_CONCEPT
RENAME_CONCEPT
RECLASSIFY_CONCEPT
```

Each operation should have provenance, supporting evidence, and an approval status.

## 9. Implementation recommendations for Rust and Oxigraph

I would separate the engine into these modules:

| Module      | Responsibility                           |
| ----------- | ---------------------------------------- |
| `corpus`    | Document and knowledge-source ingestion  |
| `extract`   | Terminology and concept extraction       |
| `normalize` | Canonicalization and deduplication       |
| `embed`     | Semantic vector generation               |
| `cluster`   | Clustering and grouping                  |
| `relations` | Hypernym and semantic relation discovery |
| `induce`    | Hierarchy construction                   |
| `refine`    | LLM-based semantic refinement            |
| `evaluate`  | Quality measurement                      |
| `validate`  | SHACL and structural validation          |
| `align`     | Mapping to external taxonomies           |
| `publish`   | SKOS/RDF generation                      |
| `revision`  | Incremental changes and versioning       |

I would keep the clustering and hierarchy algorithms independent of the LLM implementation.

For example:

```
trait ConceptExtractor {
    async fn extract(
        &self,
        document: &Document,
    ) -> Result<Vec<ConceptCandidate>>;
}

trait ConceptNormalizer {
    async fn normalize(
        &self,
        concepts: Vec<ConceptCandidate>,
    ) -> Result<Vec<CanonicalConcept>>;
}

trait RelationInducer {
    async fn infer(
        &self,
        concepts: &[CanonicalConcept],
    ) -> Result<Vec<RelationCandidate>>;
}

trait TaxonomyInducer {
    async fn construct(
        &self,
        concepts: &[CanonicalConcept],
        relations: &[RelationCandidate],
    ) -> Result<TaxonomyCandidate>;
}

trait TaxonomyEvaluator {
    async fn evaluate(
        &self,
        taxonomy: &TaxonomyCandidate,
    ) -> Result<TaxonomyMetrics>;
}

trait TaxonomyPublisher {
    async fn publish(
        &self,
        taxonomy: &ValidatedTaxonomy,
    ) -> Result<TaxonomyVersion>;
}
```

The actual implementation could use synchronous CPU-oriented algorithms internally, with asynchronous interfaces for external models, storage and API operations.

### Oxigraph storage design

I would use named graphs:

| Graph                 | Contents                              |
| --------------------- | ------------------------------------- |
| `taxonomy/catalog`    | Registered taxonomies                 |
| `taxonomy/concepts`   | Canonical concepts                    |
| `taxonomy/relations`  | Accepted hierarchical relationships   |
| `taxonomy/candidates` | Unapproved concepts and relationships |
| `taxonomy/evidence`   | Extraction and relationship evidence  |
| `taxonomy/provenance` | PROV-O generation history             |
| `taxonomy/versions`   | Published revisions                   |
| `taxonomy/evaluation` | Quality measurements                  |

For performance, maintain derived indexes for:

- Concept-to-parent.
- Parent-to-children.
- Concept-to-scheme.
- Document-to-concept.
- Candidate relationship scores.

Oxigraph should hold the authoritative RDF representation; Rust should handle expensive clustering, embedding similarity and hierarchy algorithms.

## 10. Research priorities and resources

These are the papers and standards I would prioritize studying.

| Priority | Research                                                                                                                                                                                         | Why it matters                             |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------ |
| 1        | [Taxonomy Learning Survey (2017)](https://chywang.github.io/papers/emnlp2017b.pdf?utm_source=chatgpt.com)                                                                                        | Foundational taxonomy induction methods    |
| 2        | [TnT-LLM (2024)](https://www.microsoft.com/en-us/research/publication/tnt-llm-text-mining-at-scale-with-large-language-models/?msockid=036c56ec4c186aba0d18407e4d1f6bd6\&utm_source=chatgpt.com) | LLM taxonomy generation and classification |
| 3        | [TaxoAdapt (2025)](https://aclanthology.org/2025.acl-long.1442/?utm_source=chatgpt.com)                                                                                                          | Incremental multidimensional taxonomies    |
| 4        | [CAM-Taxo (2025)](https://aclanthology.org/2025.emnlp-main.788/?utm_source=chatgpt.com)                                                                                                          | Aspect-specific clustering                 |
| 5        | [TaxoLLaMA (2026)](https://journals.sagepub.com/doi/full/10.1177/22104968251404186?utm_source=chatgpt.com)                                                                                       | Taxonomy-specific model training           |
| 6        | [TaxCE (2026)](https://aclanthology.org/2026.emnlp-industry.76/?utm_source=chatgpt.com)                                                                                                          | Corpus-grounded generation and evaluation  |
| 7        | [SC-Taxo (2026)](https://www.sciengine.com/doi/10.3724/jtke-20260022?utm_source=chatgpt.com)                                                                                                     | Semantic consistency constraints           |
| 8        | [W3C SKOS](https://www.w3.org/TR/skos-reference/?lang=en\&utm_source=chatgpt.com)                                                                                                                | RDF representation                         |

Useful benchmarks and resources:

- [TaxoBench](https://www.sciengine.com/doi/10.3724/jtke-20260022?utm_source=chatgpt.com) — hierarchical taxonomy evaluation.
- [CAM-Taxo benchmark](https://aclanthology.org/2025.emnlp-main.788/?utm_source=chatgpt.com) — scientific literature taxonomies.
- [SKOS](https://www.w3.org/TR/skos-reference/?lang=en\&utm_source=chatgpt.com) — standardized RDF taxonomy representation.
- [Automated domain taxonomy learning](https://www.sciencedirect.com/science/article/pii/S1532046416301149?utm_source=chatgpt.com) — classical clustering-based methods.
- [Contrastive learning for taxonomy induction](https://www.sciencedirect.com/science/article/pii/S0950705124010396?utm_source=chatgpt.com) — learning hierarchical structure.

## 11. My recommended direction

For your broader RDF/ontology system, I would build a general-purpose Taxonomy Induction Engine rather than a narrowly scoped text classifier.

Its central capabilities should be:

1. Discover concepts from arbitrary data sources, not just documents.
2. Normalize concepts across documents and existing RDF graphs.
3. Discover relationships using linguistic, statistical and semantic techniques.
4. Construct both single-parent and multi-parent hierarchies.
5. Generate multiple independent classification dimensions.
6. Use LLMs for semantic abstraction and refinement, not as the sole source of truth.
7. Validate hierarchies using structural and semantic constraints.
8. Publish versioned SKOS taxonomies with full PROV-O provenance.
9. Continuously enrich existing taxonomies without destroying their history.

One particularly important architectural insight: taxonomy construction should be a reusable knowledge-engineering capability, not a special-purpose document-processing feature.

The same engine could construct taxonomies from:

- MinerU-extracted documents.
- Product and service catalogs.
- RDF ontologies.
- API specifications.
- Software packages and dependencies.
- Research papers.
- Business processes.
- Organizational knowledge.
- Agent-generated knowledge graphs.

It could even construct taxonomies of your own RDF ontology catalog, automatically discovering related vocabularies and organizing them by domain, purpose, semantic scope and application.

That last capability would make it particularly useful alongside DCAT, SKOS, P-Plan, SCXML and your existing ontology-management architecture.
