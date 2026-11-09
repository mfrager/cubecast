# Paper Summary: SKOS Simple Knowledge Organization System Reference (W3C Recommendation, 2009)

**Authors:** Alistair Miles, Sean Bechhofer (W3C Semantic Web Deployment Working Group)
**Status:** W3C Recommendation, 18 August 2009
**Source:** https://www.w3.org/TR/skos-reference/
**Current Version:** https://www.w3.org/TR/skos-reference/

---

## Overview

SKOS is the **standard RDF vocabulary for representing knowledge organization systems** (thesauri, classification schemes, taxonomies, subject heading systems). It is the **foundational representation layer** for our taxonomy engine — all generated taxonomies publish as SKOS.

---

## Core SKOS Model

### Primary Classes

| Class | Purpose | Example |
|-------|---------|---------|
| `skos:ConceptScheme` | A taxonomy / classification scheme / thesaurus | `ex:MethodologyTaxonomy` |
| `skos:Concept` | A category / concept / unit of thought | `ex:Transformer` |

### Lexical Labels

| Property | Purpose | Cardinality | Example |
|----------|---------|-------------|---------|
| `skos:prefLabel` | Preferred label (one per language) | 1 per lang | "Transformer"@en |
| `skos:altLabel` | Alternative/synonym labels | 0..* | "Attention-based Model"@en |
| `skos:hiddenLabel` | Labels for search only (misspellings, obsolete) | 0..* | "transformer model"@en |

### Semantic Relations

| Property | Direction | Inverse | Purpose |
|----------|-----------|---------|---------|
| `skos:broader` | Narrower → Broader | `skos:narrower` | Immediate parent |
| `skos:narrower` | Broader → Narrower | `skos:broader` | Immediate child |
| `skos:related` | Associative (non-hierarchical) | `skos:related` | Related concepts |
| `skos:broaderTransitive` | Transitive broader | `skos:narrowerTransitive` | All ancestors |
| `skos:narrowerTransitive` | Transitive narrower | `skos:broaderTransitive` | All descendants |

### Documentation Properties

| Property | Purpose |
|----------|---------|
| `skos:definition` | Full definition |
| `skos:scopeNote` | Scope/usage restrictions |
| `skos:example` | Example usage |
| `skos:historyNote` | Change history |
| `skos:editorialNote` | Internal notes |

### Mapping Properties (Alignment)

| Property | Strength | Use Case |
|----------|----------|----------|
| `skos:exactMatch` | Exact equivalence | Same concept, different URI |
| `skos:closeMatch` | Close but not exact | Cross-walk between taxonomies |
| `skos:broadMatch` | Broader concept in other scheme | |
| `skos:narrowMatch` | Narrower concept in other scheme | |
| `skos:relatedMatch` | Associative match | |

### Scheme Membership

| Property | Purpose |
|----------|---------|
| `skos:inScheme` | Concept belongs to scheme |
| `skos:topConceptOf` | Concept is root of scheme |
| `skos:hasTopConcept` | Scheme has root concept |

---

## SKOS-XL (Extension for Labels)

For rich label metadata (language variants, provenance, notation):

```turtle
ex:transformerLabel a skosxl:Label ;
    skosxl:literalForm "Transformer"@en ;
    tx:source "CAM-Taxo generation" ;
    tx:confidence 0.92 .

ex:Transformer a skos:Concept ;
    skosxl:prefLabel ex:transformerLabel ;
    skos:inScheme ex:MethodologyTaxonomy .
```

---

## SKOS vs. OWL: Critical Distinction

> **SKOS deliberately separates concept organization from formal OWL class semantics.**

| Aspect | SKOS | OWL |
|--------|------|-----|
| `skos:Concept` | Unit of thought, informal | Formal class with logical constraints |
| `skos:broader` | Suggests hierarchy, no entailment | `rdfs:subClassOf` entails inference |
| `skos:related` | Associative, no logic | `owl:ObjectProperty` with characteristics |
| Reasoning | None required | Full DL reasoning |

**Why this matters for us:** Our taxonomies are **pragmatic knowledge organization**, not formal ontologies. SKOS allows:
- Polyhierarchy (multiple `skos:broader`)
- Provisional relationships (candidates not yet validated)
- Mapping to external vocabularies without logical commitment
- Versioning and provenance without logical inconsistency

---

## SKOS for Our Taxonomy Engine

### Concept Representation

```turtle
# Canonical concept
ex:LinearAttention a skos:Concept ;
    skos:prefLabel "Linear Attention"@en ;
    skos:altLabel "Linear Attention Mechanism"@en, "Kernel-based Attention"@en ;
    skos:definition "An attention mechanism that reduces quadratic complexity to linear via kernel feature maps, enabling efficient long-sequence modeling."@en ;
    skos:scopeNote "Applies to autoregressive and bidirectional models. Distinct from sparse attention."@en ;
    skos:broader ex:EfficientAttention ;
    skos:narrower ex:Performer, ex:LinearTransformer, ex:RWKV ;
    skos:related ex:SparseAttention, ex:FlashAttention ;
    skos:inScheme ex:MethodologyTaxonomy ;
    skos:topConceptOf ex:MethodologyTaxonomy ;  # if root
    tx:facet ex:MethodologyFacet ;
    tx:eegExclusivity 0.85 ;
    tx:eegExhaustivity 0.91 ;
    tx:eegGranularity 0.78 ;
    tx:sourceCluster "cluster_42" ;
    tx:llmGenerated true .
```

### Faceted Taxonomy as Multiple ConceptSchemes

```turtle
# Each facet = separate ConceptScheme
ex:MethodologyTaxonomy a skos:ConceptScheme ;
    skos:prefLabel "Research Methodology Taxonomy"@en ;
    tx:facet ex:MethodologyFacet ;
    tx:version 3 ;
    tx:generatedBy ex:taxonomyRun42 ;
    dcterms:created "2026-11-08T10:00:00Z"^^xsd:dateTime .

ex:DatasetTaxonomy a skos:ConceptScheme ;
    skos:prefLabel "Dataset Taxonomy"@en ;
    tx:facet ex:DatasetFacet ;
    tx:version 2 .

ex:EvaluationTaxonomy a skos:ConceptScheme ;
    skos:prefLabel "Evaluation Metric Taxonomy"@en ;
    tx:facet ex:EvaluationFacet ;
    tx:version 1 .
```

### Provenance Integration (PROV-O + Custom)

```turtle
# Generation activity
ex:taxonomyRun42 a prov:Activity, tx:TaxonomyGeneration ;
    prov:startedAtTime "2026-11-08T10:00:00Z"^^xsd:dateTime ;
    prov:endedAtTime "2026-11-08T10:45:00Z"^^xsd:dateTime ;
    prov:used ex:corpus_v17, ex:camTaxoBenchmark ;
    prov:generated ex:MethodologyTaxonomy, ex:DatasetTaxonomy, ex:EvaluationTaxonomy ;
    prov:wasAssociatedWith ex:TaxonomyAgent ;
    tx:method "CAM-Taxo" ;
    tx:llmModel "gpt-4o-2024-11-20" ;
    tx:embeddingModel "bge-large-en-v1.5" .

# Concept provenance
ex:LinearAttention a skos:Concept ;
    # ... SKOS properties ...
    prov:wasGeneratedBy ex:conceptGen_42 .

ex:conceptGen_42 a tx:ConceptGeneration ;
    prov:used ex:cluster_42, ex:llmPrompt_v3 ;
    prov:wasGeneratedBy ex:taxonomyRun42 ;
    tx:confidence 0.92 .
```

### Candidate vs. Validated Separation

```turtle
# Candidate concepts (not yet validated)
ex:candidate_123 a tx:ConceptCandidate ;
    skos:prefLabel "Mixture of Experts"@en ;
    skos:definition "..." ;
    tx:proposedBroader ex:TransformerArchitecture ;
    tx:confidence 0.78 ;
    tx:sourceCluster "cluster_88" ;
    tx:status tx:PendingValidation .

# Validated concept (promoted)
ex:MixtureOfExperts a skos:Concept ;
    skos:prefLabel "Mixture of Experts"@en ;
    skos:broader ex:TransformerArchitecture ;
    skos:inScheme ex:MethodologyTaxonomy ;
    tx:promotedFrom ex:candidate_123 ;
    tx:validationRun ex:validationRun_7 .
```

---

## Oxigraph Named Graph Design (from Research Doc)

| Graph | Contents | SKOS Alignment |
|-------|----------|----------------|
| `taxonomy/catalog` | Registered taxonomies (ConceptSchemes) | `skos:ConceptScheme` |
| `taxonomy/concepts` | Canonical concepts | `skos:Concept` + `skos:inScheme` |
| `taxonomy/relations` | Accepted hierarchical relationships | `skos:broader`/`skos:narrower` |
| `taxonomy/candidates` | Unapproved concepts & relationships | `tx:ConceptCandidate`, `tx:RelationCandidate` |
| `taxonomy/evidence` | Extraction & relationship evidence | `tx:RelationEvidence`, `tx:sourceDocument` |
| `taxonomy/provenance` | PROV-O generation history | `prov:Activity`, `prov:Entity` |
| `taxonomy/versions` | Published revisions | `tx:TaxonomyVersion`, `tx:RevisionOperation` |
| `taxonomy/evaluation` | Quality measurements | `tx:EEGMetrics`, `tx:ConsistencyReport` |

---

## Implementation: SKOS Publisher Module

```rust
struct SkosPublisher {
    store: OxigraphStore,
    base_iri: String,
}

impl SkosPublisher {
    async fn publish_taxonomy(&self, taxonomy: &ValidatedTaxonomy) -> Result<TaxonomyVersion> {
        let version_id = Uuid::new_v4();
        let version_iri = format!("{}/versions/{}", self.base_iri, version_id);
        
        // 1. Create ConceptScheme
        let scheme_iri = format!("{}/schemes/{}", self.base_iri, taxonomy.facet_id);
        self.add_quad(Quad::new(
            NamedNode::new(&scheme_iri)?,
            rdf::TYPE,
            SKOS_CONCEPT_SCHEME,
            NamedNode::new("taxonomy/catalog")?,
        ))?;
        
        self.add_quad(Quad::new(
            NamedNode::new(&scheme_iri)?,
            SKOS_PREF_LABEL,
            Literal::new_simple_literal(&taxonomy.facet_name),
            NamedNode::new("taxonomy/catalog")?,
        ))?;
        
        // 2. Publish concepts
        for concept in &taxonomy.concepts {
            let concept_iri = format!("{}/concepts/{}", self.base_iri, concept.id);
            
            self.add_quad(Quad::new(
                NamedNode::new(&concept_iri)?,
                rdf::TYPE,
                SKOS_CONCEPT,
                NamedNode::new("taxonomy/concepts")?,
            ))?;
            
            self.add_quad(Quad::new(
                NamedNode::new(&concept_iri)?,
                SKOS_PREF_LABEL,
                Literal::new_language_tagged_literal(&concept.pref_label, "en"),
                NamedNode::new("taxonomy/concepts")?,
            ))?;
            
            for alt in &concept.alt_labels {
                self.add_quad(Quad::new(
                    NamedNode::new(&concept_iri)?,
                    SKOS_ALT_LABEL,
                    Literal::new_language_tagged_literal(alt, "en"),
                    NamedNode::new("taxonomy/concepts")?,
                ))?;
            }
            
            self.add_quad(Quad::new(
                NamedNode::new(&concept_iri)?,
                SKOS_DEFINITION,
                Literal::new_language_tagged_literal(&concept.definition, "en"),
                NamedNode::new("taxonomy/concepts")?,
            ))?;
            
            self.add_quad(Quad::new(
                NamedNode::new(&concept_iri)?,
                SKOS_IN_SCHEME,
                NamedNode::new(&scheme_iri)?,
                NamedNode::new("taxonomy/concepts")?,
            ))?;
            
            // Hierarchy
            for parent in &concept.broader {
                let parent_iri = format!("{}/concepts/{}", self.base_iri, parent);
                self.add_quad(Quad::new(
                    NamedNode::new(&concept_iri)?,
                    SKOS_BROADER,
                    NamedNode::new(&parent_iri)?,
                    NamedNode::new("taxonomy/relations")?,
                ))?;
            }
            
            // EEG metrics as custom properties
            if let Some(eeg) = &concept.eeg_metrics {
                self.add_quad(Quad::new(
                    NamedNode::new(&concept_iri)?,
                    TX_EEG_EXCLUSIVITY,
                    Literal::new_typed_literal(eeg.exclusivity.to_string(), XSD_FLOAT),
                    NamedNode::new("taxonomy/evaluation")?,
                ))?;
                // ... exhaustivity, granularity
            }
        }
        
        // 3. Top concepts
        for root in taxonomy.root_concepts() {
            let root_iri = format!("{}/concepts/{}", self.base_iri, root);
            self.add_quad(Quad::new(
                NamedNode::new(&scheme_iri)?,
                SKOS_HAS_TOP_CONCEPT,
                NamedNode::new(&root_iri)?,
                NamedNode::new("taxonomy/catalog")?,
            ))?;
            self.add_quad(Quad::new(
                NamedNode::new(&root_iri)?,
                SKOS_TOP_CONCEPT_OF,
                NamedNode::new(&scheme_iri)?,
                NamedNode::new("taxonomy/concepts")?,
            ))?;
        }
        
        // 4. Provenance
        self.publish_provenance(taxonomy, &version_iri).await?;
        
        Ok(TaxonomyVersion { id: version_iri, scheme_iri, timestamp: Utc::now() })
    }
}
```

---

## Validation with SHACL

```turtle
# SHACL shapes for SKOS taxonomy validation
ex:TaxonomyShape a sh:NodeShape ;
    sh:targetClass skos:ConceptScheme ;
    sh:property [
        sh:path skos:hasTopConcept ;
        sh:minCount 1 ;
        sh:message "Every taxonomy must have at least one top concept" ;
    ] .

ex:ConceptShape a sh:NodeShape ;
    sh:targetClass skos:Concept ;
    sh:property [
        sh:path skos:prefLabel ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:languageIn ("en") ;
        sh:message "Every concept must have exactly one English prefLabel" ;
    ] ;
    sh:property [
        sh:path skos:inScheme ;
        sh:minCount 1 ;
        sh:message "Every concept must belong to at least one scheme" ;
    ] ;
    sh:property [
        sh:path skos:broader ;
        sh:nodeKind sh:IRI ;
        sh:message "broader must be a URI" ;
    ] ;
    sh:sparql [
        sh:message "No cycles in broader hierarchy" ;
        sh:select """
            PREFIX skos: <http://www.w3.org/2004/02/skos/core#>
            SELECT $this WHERE {
                $this skos:broaderTransitive* $this .
            }
        """ ;
    ] .
```

---

## Critical Design Decisions

1. **SKOS as canonical output** — All taxonomies publish as SKOS; internal representation may differ
2. **One ConceptScheme per facet** — Clean separation; enables independent versioning
3. **Candidate/Validated separation** — Use custom `tx:` classes for unapproved content
4. **PROV-O for full provenance** — Every concept/relation traces to generation activity
5. **EEG metrics as custom properties** — Not SKOS-standard; use `tx:` namespace
6. **Named graphs for access control** — Candidates separate from validated; evidence separate

---

## Integration with Pipeline

| Pipeline Output | SKOS Representation |
|-----------------|---------------------|
| Faceted taxonomies | Multiple `skos:ConceptScheme` with `tx:facet` |
| Hierarchy edges | `skos:broader`/`skos:narrower` in `taxonomy/relations` |
| Concept labels/definitions | `skos:prefLabel`, `skos:altLabel`, `skos:definition` |
| Synonyms | `skos:altLabel` |
| Cross-facet mappings | `skos:exactMatch`/`skos:closeMatch` between schemes |
| Provenance | `prov:Activity` in `taxonomy/provenance` |
| EEG metrics | `tx:eegExclusivity` etc. in `taxonomy/evaluation` |
| Validation results | `tx:ValidationReport` in `taxonomy/evaluation` |

---

## Next Steps

1. Define `tx:` vocabulary formally (Turtle file in repo)
2. Implement `SkosPublisher` with Oxigraph named graphs
3. Write SHACL shapes for structural validation
4. Add SKOS round-trip test (publish → query → verify)
5. Implement concept alignment (`skos:exactMatch`) to external vocabularies (WordNet, MeSH, ACM CCS)