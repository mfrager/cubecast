# Related research: Fact ontologies and automated fact engineering

The research landscape is broader than traditional ontology engineering. There are several overlapping fields that are particularly relevant:

1. Assertion and evidence modeling.
2. Automated knowledge graph construction.
3. Ontology learning and schema induction.
4. Fact-level provenance and trust.
5. Temporal and event-centric knowledge representation.
6. Automated reasoning, contradiction detection and knowledge validation.
7. Knowledge integration and entity resolution.

I investigated these areas with an emphasis on research published through November 2026, RDF compatibility, and applicability to an Oxigraph-based knowledge system.

The most important finding is that fact extraction, fact representation, and fact verification are separate research problems. A good architecture needs to solve all three.

## 1. Assertion, evidence and provenance research

This is probably the closest research area to your proposed fact ontology.

### 1.1 Bridging the Scientific Knowledge Gap and Reproducibility (2025)

Read first

Chhetri et al., ACM Web Conference Companion 2025.

[Full paper – PMC](https://pmc.ncbi.nlm.nih.gov/articles/PMC12376154/?utm_source=chatgpt.com) · [ACM publication](https://doi.org/10.1145%2F3701716.3715483?utm_source=chatgpt.com)

This is an especially valuable survey because it systematically examines 23 ontologies:

- 13 focused on assertions and evidence.
- 10 focused on provenance.

It compares how different ontologies represent claims, evidence, sources, and the processes that generate knowledge.

Some important observations:

- SEPIO explicitly models assertions and evidence.
- ECO distinguishes evidence types and assertion methods.
- Nanopublications provide identifiable, citable assertions.
- PROV-O provides general-purpose provenance.
- Other ontologies can represent hypotheses, observations, and information artifacts.

Why it matters: This is an excellent starting point for deciding which existing vocabularies to reuse instead of reinventing them.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://pubmed.ncbi.nlm.nih.gov\&sz=32)

PubMed+1



### 1.2 Provenance-driven nanopublications (2025)

Menotti et al., International Journal on Digital Libraries, October 2025.

[Provenance-driven nanopublications: representing source lineage and trust networks for multi-source assertions](https://link.springer.com/article/10.1007/s00799-025-00431-x?utm_source=chatgpt.com)

This is one of the most interesting papers for your architecture.

The authors address a limitation of traditional nanopublications: representing a factual assertion supported by multiple sources, including sources that contradict each other.

Their solution introduces:

- An extended nanopublication model.
- A knowledge provenance graph.
- PROV-K, an ontology for knowledge provenance.
- Trust relationships between agents and propositions.
- Support and refutation relationships between evidence and assertions.

Their experiments include 197,511 assertions and a trust network involving more than 45,000 facts.

The critical idea is that provenance must be maintained through aggregation, not just at initial extraction.

For example:

```
Fact: Organization A acquired Organization B

Evidence:
    Document 1: Supports
    Document 2: Supports
    Document 3: Refutes
    Database 4: Supports

Aggregation:
    Knowledge provenance
        ├── Source lineage
        ├── Supporting evidence
        ├── Contradictory evidence
        └── Agent trust relationships
```

I would study PROV-K particularly closely. It may eliminate the need to design some of your provenance extensions from scratch.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://link.springer.com\&sz=32)

Springer Nature Link



### 1.3 SEPIO and the evidence ontology ecosystem

Resources:

- [SEPIO](https://monarchinitiative.org/ontologies/sepio?utm_source=chatgpt.com)
- [ECO](https://evidenceontology.org/?utm_source=chatgpt.com)
- [SIO](https://bioportal.bioontology.org/ontologies/SIO?utm_source=chatgpt.com)
- [PROV-O](https://www.w3.org/TR/prov-o/?utm_source=chatgpt.com)

These are complementary rather than competing approaches.

My recommendation is to study SEPIO's assertion/evidence model, ECO's evidence classification, and PROV-O's activity/entity/agent model together.

## 2. Automated knowledge graph construction

This field is concerned with transforming unstructured information into structured factual knowledge.

### 2.1 A Comprehensive Survey on Automatic Knowledge Graph Construction (2023)

Zhong et al., ACM Computing Surveys.

[A Comprehensive Survey on Automatic Knowledge Graph Construction](https://doi.org/10.1145/3618295?utm_source=chatgpt.com)

This is an important foundational survey.

It examines the evolution of automatic knowledge graph construction, including:

- Entity extraction.
- Relation extraction.
- Knowledge acquisition.
- Entity alignment.
- Knowledge fusion.
- Schema and ontology construction.

One important distinction is between extracting individual factual relationships and constructing more conceptualized knowledge.

The latter is much closer to your goal of automatically constructing fact ontologies.

I would use this as the general research map for the extraction subsystem.

### 2.2 LMKG: A Flexible Method for Ontology-Aware Knowledge Graph Extraction from Text (2026)

Pappacoda and Torroni, AKBC 2026.

Highly relevant implementation research

[LMKG paper – ACL Anthology](https://aclanthology.org/2026.akbc-1.23/?utm_source=chatgpt.com)

LMKG is particularly relevant because it treats fact extraction as a complete knowledge graph construction problem rather than just triple generation.

Its architecture supports:

- Open extraction.
- Ontology-constrained extraction.
- Entity reconciliation.
- Relation vocabulary control.
- Cross-passage integration.
- Source traceability.
- Graph refinement.

It proposes two strategies for long documents:

Fold: progressively accumulate a knowledge graph over document chunks.

Enrich: combine whole-document and chunk-level extraction before graph refinement.

The authors report 90.80% fact recovery on MINE-1.

An especially important observation is that providing an extracted graph to a question-answering system did not always outperform providing the original passages.

That suggests a useful architectural rule: retain source documents and their original text alongside extracted facts. A knowledge graph should complement, not necessarily replace, source material.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://aclanthology.org\&sz=32)

ACL Anthology



### 2.3 Ontology-Aware Prompting for Knowledge Graph Construction from Text (2026)

Tiwari et al., 2026.

[Full research paper](https://journals.sagepub.com/doi/full/10.3233/SSW260009?utm_source=chatgpt.com) · [Open-source implementation](https://github.com/dice-group/ontology-aware-kg-construction?utm_source=chatgpt.com)

This paper addresses two particularly serious problems:

1. LLM-generated facts that are not actually supported by source text.
2. Extracted relationships that do not conform to the target ontology.

Its approach combines:

- Multiple complementary extraction prompts.
- Ontology-aware generation.
- Schema-constrained OpenIE.
- Evidence grounding.
- Cross-prompt agreement.
- Surface-form validation.

It reports improvements over LLM-based baselines and ontology conformance above 0.90 on most DBpedia-WebNLG domains.

I particularly like the separation between candidate generation and candidate acceptance.

Rather than allowing an LLM to directly write authoritative RDF, it produces candidate facts that pass through independent verification stages.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://journals.sagepub.com\&sz=32)

Sage Journals



### 2.4 Axiom Generation for Automated Ontology Construction from Texts Through Schema Mapping (2026)

Zengeya, Fonou-Dombeu and Gwetu.

[Research paper – Machine Learning and Knowledge Extraction](https://www.mdpi.com/2504-4990/8/2/29?utm_source=chatgpt.com)

This is more directly concerned with constructing the ontology itself.

It addresses the conversion of extracted entities and relationships into formal OWL axioms.

The approach includes:

- Concept extraction.
- Relation extraction.
- Lexical relationship normalization.
- Schema mapping.
- Entity typing.
- Hierarchical induction.
- Domain and range inference.
- ABox and TBox generation.
- Logical consistency checking.

The reported experiments use CONLL04 and NYT datasets, with expert acceptance above 95% and zero inconsistencies under their reasoner checks.

This is an important direction for automatically discovering the structure of fact types, rather than simply populating an existing schema with facts.

One qualification: logical consistency does not establish factual correctness. A logically consistent ontology can still contain unsupported or incorrect claims.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.mdpi.com\&sz=32)

MDPI



## 3. Temporal facts and event-centric knowledge graphs

This is a particularly important area if your fact ontology is intended to represent real-world knowledge rather than static database records.

### 3.1 ChronoGrapher: Event-Centric Knowledge Graph Construction (2025)

Blin et al., Semantic Web: Interoperability, Usability, Applicability.

[ChronoGrapher research paper](https://journals.sagepub.com/doi/10.1177/22104968251377247?utm_source=chatgpt.com)

This paper constructs event-centric knowledge graphs from existing generic knowledge graphs.

Its approach:

1. Retrieve relevant subgraphs using informed graph traversal.
2. Prioritize event-related nodes.
3. Enrich the resulting graph using structured triples and text-derived information.
4. Establish temporal and causal connections.

It uses concepts from the Simple Event Model (SEM).

This is particularly useful because many factual statements are really descriptions of events, rather than simple relationships between static entities.

For example:

```
Company Acquisition
    ├── Acquirer
    ├── Acquired Organization
    ├── Date
    ├── Location
    ├── Purchase Price
    ├── Participants
    └── Consequences
```

This is much more expressive than:

```
ex:CompanyA ex:acquired ex:CompanyB .
```

I would study SEM alongside ChronoGrapher.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://journals.sagepub.com\&sz=32)

Sage Journals



### 3.2 Temporal knowledge graphs

Another important research area is Temporal Knowledge Graph Completion (TKGC).

Traditional knowledge graphs represent facts such as:

```
Person A → worksFor → Company B
```

Temporal knowledge graphs add a time dimension:

```
Person A → worksFor → Company B
    validFrom: 2020
    validUntil: 2024
```

Research problems include:

- Temporal fact extraction.
- Temporal entity resolution.
- Temporal relation prediction.
- Historical fact correction.
- Event sequencing.
- Temporal consistency.
- Future fact prediction.

Relevant model families include:

| Model     | Research focus                          |
| --------- | --------------------------------------- |
| TTransE   | Temporal knowledge graph embeddings     |
| HyTE      | Temporal embedding and entity relations |
| DE-SimplE | Temporal representation learning        |
| TeRo      | Temporal rotation embeddings            |
| TLogic    | Temporal logical rule learning          |

These are primarily prediction and completion approaches, not RDF ontology standards.

I would keep them separate from the authoritative fact representation. They can propose missing relationships or identify anomalies, while the ontology and provenance layer determines what is actually recorded.

### 3.3 Event and process ontologies

These are particularly useful for representing complex factual structures.

| Vocabulary  | Application                             |
| ----------- | --------------------------------------- |
| SEM         | General event representation            |
| CIDOC CRM   | Cultural heritage and historical events |
| LODE        | Linking open descriptions of events     |
| OWL-Time    | Temporal relationships                  |
| PROV-O      | Activities and derivation               |
| DOLCE / DUL | General entities, events and situations |
| SOSA/SSN    | Observations and measurements           |

Useful resources:

-
-
-
-

## 4. Observation, measurement and scientific fact modeling

This is another important branch of research because not all facts are extracted from prose.

Some are produced by:

- Sensors.
- Experiments.
- Measurements.
- Calculations.
- Simulations.
- Statistical analysis.

### SOSA/SSN

Recommended vocabulary

SOSA/SSN

The SOSA ontology provides a general model for observations.

Its core concepts include:

| Concept                   | Purpose                 |
| ------------------------- | ----------------------- |
| `sosa:Observation`        | Observation activity    |
| `sosa:Sensor`             | Observation agent       |
| `sosa:FeatureOfInterest`  | Thing being observed    |
| `sosa:ObservableProperty` | Property being observed |
| `sosa:ObservedProperty`   | Observed characteristic |
| `sosa:hasSimpleResult`    | Observation result      |
| `sosa:usedProcedure`      | Procedure used          |

For example:

```
Observation
    ├── Observed entity
    ├── Observed property
    ├── Procedure
    ├── Sensor
    ├── Result
    ├── Timestamp
    └── Unit
```

This is a valuable distinction: an observation is an activity, while its result is information produced by that activity.

### QUDT

QUDT

QUDT is useful for representing quantitative facts.

It models:

- Quantities.
- Units.
- Dimensions.
- Measurement scales.
- Quantity kinds.
- Numerical values.

For example, a statement that a machine operates at 2.4 GHz should not be represented merely as a floating-point number.

It should preserve the quantity, unit, and dimensional semantics.

### STATO

STATO

STATO describes statistical methods, measurements, and results.

It is especially interesting for facts derived from statistical analysis.

For a general fact engine, I would consider SOSA, QUDT and STATO as separate extensions.

## 5. Fact validation, truth discovery and contradiction detection

This is one of the most important research areas that is often overlooked in knowledge graph architecture.

Extracting a statement does not establish that the statement is true.

There are several distinct research problems:

| Problem                 | Description                                                   |
| ----------------------- | ------------------------------------------------------------- |
| Fact verification       | Is a claim supported by evidence?                             |
| Truth discovery         | Which value is most likely correct among conflicting sources? |
| Knowledge fusion        | How should multiple sources be combined?                      |
| Contradiction detection | Which assertions cannot simultaneously hold?                  |
| Source reliability      | How trustworthy is the source?                                |
| Temporal reconciliation | Are apparently conflicting facts valid at different times?    |
| Logical consistency     | Does the knowledge violate ontology constraints?              |

### A. Truth discovery

Research on truth discovery attempts to determine the most likely truth when multiple sources provide conflicting values.

A useful foundational research direction is the work of Dong, Berti-Équille and Srivastava on truth discovery.

A common model considers:

- Source reliability.
- Agreement between sources.
- Value frequency.
- Independence of sources.
- Historical accuracy.

This is more sophisticated than simply choosing the most frequently repeated fact.

For example, ten websites copying one incorrect source should not necessarily outweigh one independently verified primary source.

### B. Knowledge graph fact checking

Relevant research systems include:

- FactBench.
- FEVER.
- SciFact.
- HoVer.

These explore different aspects of claim verification.

SciFact is particularly relevant for your document-processing pipeline because it evaluates scientific claims against scientific abstracts and evidence.

.

### C. Contradiction representation

I would explicitly distinguish:

1. Logical contradiction.
2. Empirical disagreement.
3. Temporal disagreement.
4. Terminological disagreement.
5. Measurement disagreement.
6. Source disagreement.

For example:

```
Assertion A:
    Company revenue = $10M
    Period = 2024
    Source = Annual report

Assertion B:
    Company revenue = $12M
    Period = 2024
    Source = Financial database
```

These conflict, but they may use different accounting definitions.

A contradiction detector should therefore examine qualifiers, units, temporal scope and semantic definitions before flagging a genuine contradiction.

### D. Belief and uncertainty representation

I would also investigate:

- Bayesian networks.
- Dempster-Shafer evidence theory.
- Probabilistic soft logic.
- Markov logic networks.
- Multi-valued logics.
- Paraconsistent reasoning.

These approaches are useful because ordinary OWL reasoning is based on an open-world, monotonic logical model and is not designed to express all forms of uncertainty or conflicting evidence.

My preference would be to retain explicit evidence and assertion records in RDF and use probabilistic or statistical reasoning as a separate evaluation layer.

## 6. Automatic discovery of fact types and ontology structures

This is perhaps the most interesting area for building a genuinely autonomous fact ontology engine.

Rather than requiring a pre-existing ontology, the system discovers recurring structures from data.

For example, imagine processing 100,000 company-related documents.

The system discovers repeated patterns:

```
Company acquired Company
Company founded Company
Person founded Company
Company employed Person
Company generated Revenue
Company operates In Location
```

It could then infer candidate ontology structures:

```
Company
    ├── acquisition
    │     ├── acquiredOrganization
    │     ├── acquisitionDate
    │     └── purchasePrice
    │
    ├── founding
    │     ├── founder
    │     └── foundingDate
    │
    └── financialPerformance
          ├── revenue
          ├── reportingPeriod
          └── currency
```

This is called ontology learning, schema induction, or ontology population depending on which aspects of the process are being studied.

### Research worth examining

| Research                                                                                                          | Main contribution                                        |
| ----------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| [Axiom Generation (2026)](https://www.mdpi.com/2504-4990/8/2/29?utm_source=chatgpt.com)                           | Convert extracted relationships into OWL axioms          |
| [Ontology-Aware Prompting (2026)](https://journals.sagepub.com/doi/full/10.3233/SSW260009?utm_source=chatgpt.com) | Ground extraction in existing ontology structures        |
| [Bootstrapping Ontology Graphs from Textbooks](https://openreview.net/pdf?id=nDe2D8DDXKR\&utm_source=chatgpt.com) | Induce conceptual relationships from educational content |
| [Automatic KG Construction Survey](https://doi.org/10.1145/3618295?utm_source=chatgpt.com)                        | Broader construction methods                             |

I would distinguish three levels of automated schema discovery:

Level 1 — Predicate discovery

Discover relationships such as `owns`, `acquired`, `employs`, and `manufactures`.

Level 2 — Fact structure discovery

Discover that acquisitions typically have an acquirer, acquired organization, date, value and transaction type.

Level 3 — Ontology axiom discovery

Infer that:

- `acquiredOrganization` has domain `Acquisition`.
- `acquiredOrganization` has range `Organization`.
- `Acquisition` is a subclass of `BusinessEvent`.
- An acquisition has at least one acquiring organization.

The third level is where ontology learning becomes formal knowledge engineering.

## 7. A particularly interesting direction: fact-level knowledge provenance

I would investigate this separately because it could become one of the most important parts of your system.

Traditional knowledge graphs store facts:

```
ex:CompanyA ex:owns ex:CompanyB .
```

A provenance-aware knowledge graph stores the fact together with its origin and derivation:

```
Fact
  ├── Subject
  ├── Predicate
  ├── Object
  ├── Source documents
  ├── Extraction activities
  ├── Supporting assertions
  ├── Contradicting assertions
  ├── Derivation history
  └── Evaluation results
```

The important research concepts are:

- Statement-level provenance.
- Deep provenance.
- Provenance semirings.
- Data lineage.
- Knowledge provenance.
- Trust networks.
- Multi-source truth discovery.

The 2025 PROV-K nanopublication paper is particularly important here because it connects source lineage, multiple assertions, trust relationships and evidence aggregation.

I would also investigate:

-
-
-
-

## 8. Research synthesis: what I would actually build

Based on these research directions, I would organize the system into six independent capabilities.

## Automated fact engineering architecture

Source ingestion

Documents · APIs · RDF · Databases · Observations

Fact extraction

Entities, relations, events, measurements, assertions

Schema induction

Fact types, predicates, qualifiers, ontology axioms

Fact normalization and reconciliation

Entity resolution · Deduplication · Temporal normalization · Semantic alignment

Evidence evaluation

Provenance · Source reliability · Support · Refutation

Logical validation

SHACL · OWL · Temporal consistency · Contradictions

Versioned RDF knowledge base

Facts · Assertions · Evidence · Ontologies · Provenance · Taxonomies

### Suggested module boundaries

| Module            | Responsibility                            |
| ----------------- | ----------------------------------------- |
| `fact-extract`    | Extract factual statements                |
| `fact-normalize`  | Canonicalize entities and predicates      |
| `fact-schema`     | Discover fact structures and types        |
| `fact-provenance` | Record sources and derivation             |
| `fact-evidence`   | Supporting and refuting evidence          |
| `fact-temporal`   | Valid time and transaction time           |
| `fact-reconcile`  | Resolve duplicates and conflicts          |
| `fact-verify`     | Evaluate factual correctness              |
| `fact-reason`     | Apply logical and probabilistic reasoning |
| `fact-publish`    | Generate RDF and nanopublications         |

### Suggested vocabulary stack

| Concern                           | Primary vocabulary |
| --------------------------------- | ------------------ |
| General assertions                | SEPIO              |
| Evidence classification           | ECO                |
| General entities and attributes   | SIO                |
| Information artifacts             | IAO                |
| Provenance                        | PROV-O             |
| Multi-source knowledge provenance | PROV-K             |
| Fact publication                  | Nanopublications   |
| Events                            | SEM                |
| Temporal relationships            | OWL-Time           |
| Observations                      | SOSA/SSN           |
| Quantities and units              | QUDT               |
| Statistics                        | STATO              |
| Taxonomy                          | SKOS               |
| Constraints                       | SHACL              |

I would not force all these vocabularies into a single ontology. Use modular ontology imports and mappings, and define a small application-level ontology to provide a consistent interface.

## 9. My prioritized reading list

If I were investigating this for implementation, I would read these in roughly this order:

## Essential research

1

Assertion, evidence and provenance ontology survey (2025)

Understand the existing vocabulary landscape.

[Read paper](https://pmc.ncbi.nlm.nih.gov/articles/PMC12376154/?utm_source=chatgpt.com)

2

Provenance-driven nanopublications (2025)

Multi-source assertions, trust and evidence aggregation.

[Read paper](https://link.springer.com/article/10.1007/s00799-025-00431-x?utm_source=chatgpt.com)

3

LMKG (2026)

Ontology-aware extraction and long-document graph construction.

[Read paper](https://aclanthology.org/2026.akbc-1.23/?utm_source=chatgpt.com)

4

Ontology-Aware Prompting (2026)

Grounding extracted facts and preventing ontology violations.

[Read paper](https://journals.sagepub.com/doi/full/10.3233/SSW260009?utm_source=chatgpt.com)

5

Axiom Generation (2026)

Discovering formal ontology structures from extracted facts.

[Read paper](https://www.mdpi.com/2504-4990/8/2/29?utm_source=chatgpt.com)

6

ChronoGrapher (2025)

Event-centric and temporal knowledge construction.

[Read paper](https://journals.sagepub.com/doi/10.1177/22104968251377247?utm_source=chatgpt.com)

7

Automatic Knowledge Graph Construction Survey (2023)

Broad background on knowledge acquisition and integration.

[Read paper](https://doi.org/10.1145/3618295?utm_source=chatgpt.com)

## 10. Three additional research directions I would prioritize

There are three areas that I think deserve more attention than they usually receive in conventional knowledge graph systems.

1\. Automatic fact-type induction

Discovering reusable structures such as acquisitions, employment relationships, measurements, financial transactions and scientific findings from collections of extracted facts.

This is more interesting than ordinary relation extraction because it can generate new ontology classes, properties and constraints.

2\. Multi-source evidence aggregation

Treating facts as hypotheses supported or refuted by independent sources, with explicit lineage and trust relationships.

PROV-K and extended nanopublications are particularly promising here.

3\. Symbolic and statistical knowledge reconciliation

Combining formal ontology constraints with probabilistic source evaluation, temporal reasoning and contradiction detection.

This would avoid relying on an LLM to decide whether two statements are true or false.

My strongest recommendation: investigate the combination of SEPIO + PROV-K + LMKG + automatic axiom generation.

Together, these cover four fundamental problems:

- What is being asserted?
- Where did the assertion come from?
- How can facts be extracted and integrated?
- How can the system discover the ontology needed to represent them?

That combination is a strong foundation for a self-extending RDF knowledge system in which both the factual knowledge and the schema used to represent that knowledge can evolve independently.
