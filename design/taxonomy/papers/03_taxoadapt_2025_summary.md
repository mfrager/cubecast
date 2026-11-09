# Paper Summary: TaxoAdapt: Aligning LLM-Based Multidimensional Taxonomy Construction to Evolving Research Corpora (2025)

**Authors:** Priyanka Kargupta, Nan Zhang, Yunyi Zhang, Rui Zhang, Prasenjit Mitra, Jiawei Han
**Venue:** ACL 2025 (Long Paper)
**DOI:** 10.18653/v1/2025.acl-long.1442
**Source:** https://aclanthology.org/2025.acl-long.1442/

---

## Paper Overview

TaxoAdapt solves a critical gap: **most taxonomy methods either overfit to a specific corpus (losing generalizability) or rely solely on LLM pre-training knowledge (ignoring corpus evolution).** It introduces a framework that **dynamically adapts an LLM-generated taxonomy to a given corpus across multiple dimensions**, with iterative hierarchical classification that expands width and depth based on topical distribution.

---

## Core Problem: Multidimensional Scientific Literature

A single paper belongs to **multiple independent classification dimensions simultaneously**:

| Dimension | Example Categories |
|-----------|-------------------|
| Research Problem | Machine Translation, Summarization, QA |
| Methodology | Transformer, RNN, Diffusion, Retrieval-Augmented |
| Dataset | WMT, CNN/DailyMail, SQuAD, Custom |
| Evaluation Metric | BLEU, ROUGE, BERTScore, Human Eval |
| Application Domain | Biomedical, Legal, Code, Dialogue |

**Key insight:** These are **separate taxonomies**, not branches of one universal hierarchy. Forcing them into one tree creates semantic confusion.

---

## TaxoAdapt Architecture

```
Input: Corpus D (e.g., ACL papers 2020-2024) + Seed Taxonomy T₀ (LLM-generated)
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│  FOR EACH DIMENSION d ∈ {Problem, Method, Dataset, ...}    │
├─────────────────────────────────────────────────────────────┤
│  1. LLM generates initial taxonomy T_d⁰ for dimension d    │
│  2. Iterative Hierarchical Classification:                 │
│     For iteration i = 1..N:                                │
│       a) Classify each doc in D into T_dⁱ⁻¹                │
│       b) Identify:                                         │
│          - Unclassified docs (need new categories)         │
│          - Overpopulated categories (need splitting)       │
│          - Underpopulated categories (need merging)        │
│       c) LLM proposes taxonomy modifications               │
│       d) Apply modifications → T_dⁱ                        │
│       e) Check convergence (stability threshold)           │
│  3. Output: Dimension-specific taxonomy T_d*               │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
Output: Set of aligned dimension taxonomies {T_Problem*, T_Method*, T_Dataset*, ...}
        + Cross-dimension mappings for each document
```

---

## Key Algorithmic Components

### 1. Dimension-Aware LLM Prompting
```python
# Pseudocode for dimension-specific taxonomy generation
prompt = f"""
You are organizing {dimension} categories for {domain} research papers.
Current taxonomy: {current_taxonomy}
Sample papers: {sample_papers}
Unclassified papers: {unclassified_papers}

Propose taxonomy changes:
- ADD new categories for uncovered topics
- SPLIT overbroad categories
- MERGE redundant categories
- REORGANIZE hierarchy for better granularity

Output JSON: {{"add": [...], "split": [...], "merge": [...], "reorganize": [...]}}
"""
```

### 2. Iterative Hierarchical Classification
- Uses LLM to classify papers into current taxonomy
- Tracks classification confidence per paper per node
- Papers with low confidence → signal for taxonomy expansion

### 3. Corpus-Grounded Expansion Criteria
| Signal | Action |
|--------|--------|
| > θ% papers unclassified at depth k | Add children to relevant parents |
| Category has > θ papers & high intra-variance | Split category |
| Sibling categories semantically similar (embedding) | Merge siblings |
| Category has < θ papers for N iterations | Mark for review/removal |

### 4. Convergence Detection
- Taxonomy structure stability (edit distance < ε)
- Classification confidence plateau
- Coverage saturation

---

## Evaluation Results (From Paper)

| Metric | TaxoAdapt | Best Baseline | Improvement |
|--------|-----------|---------------|-------------|
| Granularity Preservation | **0.78** | 0.62 | **+26.51%** |
| Coherence (LLM-judged) | **0.82** | 0.55 | **+50.41%** |
| Coverage | 0.91 | 0.87 | +4.6% |
| Stability (across years) | 0.85 | 0.71 | +19.7% |

Tested on CS conference papers across multiple years (ACL, CVPR, NeurIPS, ICML, KDD).

---

## Why This Is Critical for Cubecast

Our research doc explicitly states: *"TaxoAdapt is particularly relevant to this approach"* (bottom-up + top-down reconciliation) and *"These are different taxonomies, not necessarily branches of one universal hierarchy."*

**Direct architectural implications:**

1. **Faceted taxonomy = First-class citizen** — Not a hack on top of single hierarchy
2. **Seed taxonomy from LLM, adapt to corpus** — Don't choose one or the other
3. **Iterative refinement loop** — Build into our pipeline as standard operation
4. **Dimension-specific evaluation** — Can't evaluate "the taxonomy" as single score

---

## Implementation Plan for Cubecast

### Phase 1: Faceted Taxonomy Data Model (Week 1)

```rust
// Core types for multidimensional taxonomies
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FacetedTaxonomy {
    pub dimensions: HashMap<DimensionId, DimensionTaxonomy>,
    pub cross_mappings: Vec<CrossDimensionMapping>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DimensionTaxonomy {
    pub dimension_id: DimensionId,
    pub dimension_name: String,  // "methodology", "dataset", "problem"
    pub concepts: HashMap<ConceptId, Concept>,
    pub hierarchy: Hierarchy,  // DAG, not necessarily tree
    pub version: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrossDimensionMapping {
    pub document_id: DocumentId,
    pub assignments: HashMap<DimensionId, ConceptId>,  // One concept per dimension
    pub confidence: HashMap<DimensionId, f32>,
}
```

### Phase 2: Dimension-Specific Induction Engine (Weeks 2-3)

```rust
struct DimensionInductionEngine {
    llm: LlmClient,
    embedder: Embedder,
    classifier: HierarchicalClassifier,
}

impl DimensionInductionEngine {
    async fn adapt_dimension(
        &self,
        dimension: &DimensionSpec,
        corpus: &Corpus,
        seed_taxonomy: Option<&DimensionTaxonomy>,
    ) -> Result<DimensionTaxonomy> {
        // 1. Generate or load seed taxonomy for this dimension
        let mut taxonomy = seed_taxonomy.cloned()
            .unwrap_or_else(|| self.llm_generate_seed_taxonomy(dimension).await?);
        
        // 2. Iterative adaptation loop
        for iteration in 0..MAX_ITERATIONS {
            // Classify all documents
            let classifications = self.classifier.classify_corpus(&corpus, &taxonomy).await?;
            
            // Analyze classification results
            let signals = self.analyze_classification_signals(&classifications, &taxonomy);
            
            // Check convergence
            if signals.converged {
                break;
            }
            
            // LLM proposes modifications
            let modifications = self.llm_propose_modifications(&taxonomy, &signals).await?;
            
            // Apply with validation
            taxonomy = self.apply_modifications(taxonomy, modifications).await?;
            
            // Validate structure (SHACL, cycle check, etc.)
            self.validate_taxonomy(&taxonomy)?;
        }
        
        Ok(taxonomy)
    }
}
```

### Phase 3: Cross-Dimension Document Classification (Week 3-4)

```rust
struct MultiDimensionalClassifier {
    dimension_classifiers: HashMap<DimensionId, HierarchicalClassifier>,
}

impl MultiDimensionalClassifier {
    async fn classify_document(&self, doc: &Document) -> Result<DocumentClassification> {
        let mut assignments = HashMap::new();
        let mut confidences = HashMap::new();
        
        for (dim_id, classifier) in &self.dimension_classifiers {
            let result = classifier.classify(doc).await?;
            assignments.insert(dim_id.clone(), result.concept_id);
            confidences.insert(dim_id.clone(), result.confidence);
        }
        
        Ok(DocumentClassification {
            document_id: doc.id,
            assignments,
            confidences,
            timestamp: Utc::now(),
        })
    }
}
```

### Phase 4: RDF Representation (Week 4-5)

```turtle
# Separate SKOS ConceptScheme per dimension
ex:methodologyScheme a skos:ConceptScheme ;
    skos:prefLabel "Research Methodology"@en ;
    tx:dimension "methodology" ;
    tx:version 3 .

ex:transformer a skos:Concept ;
    skos:prefLabel "Transformer"@en ;
    skos:broader ex:neuralMethods ;
    skos:inScheme ex:methodologyScheme ;
    tx:dimension "methodology" .

ex:datasetScheme a skos:ConceptScheme ;
    skos:prefLabel "Dataset"@en ;
    tx:dimension "dataset" ;
    tx:version 2 .

# Document classified across dimensions
ex:paper123 a tx:ResearchPaper ;
    tx:hasMethodology ex:transformer ;
    tx:hasDataset ex:wmt2020 ;
    tx:hasProblem ex:machineTranslation ;
    tx:classifiedBy ex:classificationRun42 .

ex:classificationRun42 a tx:MultiDimensionalClassification ;
    prov:generatedAtTime "2026-11-08T10:00:00Z"^^xsd:dateTime ;
    tx:methodologyVersion 3 ;
    tx:datasetVersion 2 ;
    tx:problemVersion 1 .
```

### Phase 5: Oxigraph Named Graphs per Dimension

| Graph | Contents |
|-------|----------|
| `taxonomy/dimensions/methodology` | Methodology concept scheme + concepts |
| `taxonomy/dimensions/dataset` | Dataset concept scheme + concepts |
| `taxonomy/dimensions/problem` | Problem concept scheme + concepts |
| `taxonomy/dimensions/evaluation` | Evaluation metric scheme |
| `taxonomy/classifications` | Document → concept assignments per dimension |
| `taxonomy/cross-mappings` | Explicit cross-dimension relations (optional) |

---

## Integration with Our Pipeline

| Our Pipeline Stage | TaxoAdapt Enhancement |
|-------------------|----------------------|
| 5. Concept clustering | → Cluster **per dimension**, not globally |
| 6. Hierarchy induction | → Induce **per dimension** with dimension-specific prompts |
| 7. Semantic refinement | → Refine **per dimension**; cross-dimension consistency check |
| 8. Taxonomy validation | → Validate **per dimension** + cross-dimension coherence |
| 9. RDF publication | → Publish **multiple ConceptSchemes** + cross-mappings |

---

## Critical Design Decisions

1. **Dimensions are discovered, not fixed** — Start with known dimensions (method, dataset, problem) but allow LLM to propose new dimensions from corpus
2. **Seed taxonomy per dimension** — LLM generates from general knowledge; corpus adapts it
3. **Independent versioning per dimension** — Methodology taxonomy v3, Dataset taxonomy v1
4. **Cross-dimension mappings are explicit** — Not inferred; stored as first-class RDF
5. **Classifier per dimension** — Different feature importance per dimension (TnT-LLM distillation applies per dimension)

---

## Open Questions

1. **Dimension discovery** — How to automatically identify classification dimensions from corpus?
2. **Cross-dimension constraints** — Some combinations invalid (e.g., "BLEU" metric for "Code Generation" problem)? Encode as SHACL?
3. **Shared concepts across dimensions** — "Transformer" appears as method AND as architecture component; same URI or different?
4. **Temporal evolution per dimension** — Methodology evolves faster than problem types; different re-induction schedules?

---

## Next Papers to Complement

| Aspect | Paper |
|--------|-------|
| Aspect-specific clustering + benchmark | CAM-Taxo (2025) — Paper #4 |
| Evaluation metrics for refinement loop | TaxCE (2026) — Paper #6 |
| Semantic consistency constraints | SC-Taxo (2026) — Paper #7 |
| LLM fine-tuning for taxonomy tasks | TaxoLLaMA (2026) — Paper #5 |