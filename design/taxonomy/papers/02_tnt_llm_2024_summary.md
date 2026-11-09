# Paper Summary: TnT-LLM: Text Mining at Scale with Large Language Models (2024)

**Authors:** Microsoft Research (GPT-4 based system)
**Venue:** Microsoft Research Publication / arXiv
**Source:** https://www.microsoft.com/en-us/research/publication/tnt-llm-text-mining-at-scale-with-large-language-models/
**Note:** HTML fetch returned rate-limit page; summary synthesized from research doc + public descriptions

---

## Paper Overview

TnT-LLM introduces a **two-phase architecture** that separates taxonomy construction from large-scale classification. This is the key architectural insight: **use LLMs for the hard semantic abstraction task once, then distill to smaller models for production classification.**

---

## Two-Phase Architecture

### Phase 1: Taxonomy Induction (LLM-Heavy, Run Infrequently)
```
Raw Documents
    ↓
LLM: Extract key concepts & generate initial label taxonomy
    ↓
LLM: Iteratively refine taxonomy (merge, split, rename, reorganize)
    ↓
Human-in-the-loop validation (optional but recommended)
    ↓
Stabilized Taxonomy v1.0
```

**Key operations the LLM performs:**
- Concept extraction from document samples
- Deduplication of near-synonymous labels
- Parent-child relationship inference
- Category naming and definition writing
- Hierarchy restructuring based on coverage analysis

### Phase 2: Large-Scale Classification (LLM-Light, Run Continuously)
```
Stabilized Taxonomy
    ↓
LLM: Generate few-shot examples for each category
    ↓
Train small supervised classifier (BERT, DistilBERT, logistic regression)
    ↓
Deploy classifier for high-throughput document classification
    ↓
Monitor drift → trigger Phase 1 re-run when needed
```

---

## Why This Matters for Our System

| Traditional Approach | TnT-LLM Approach |
|---------------------|------------------|
| LLM classifies every document | LLM builds taxonomy **once** |
| High latency, high cost | Low latency, low cost at scale |
| Hard to audit/debug | Taxonomy is explicit, inspectable |
| Implicit categories | Explicit, versioned SKOS taxonomy |

**Our research doc explicitly endorses this:** *"You don't need an LLM to classify every subsequent document. Once the taxonomy stabilizes, smaller models can handle much of the workload."*

---

## Implementation Plan for Cubecast

### Phase 1: Taxonomy Induction Service (Weeks 1-3)

```rust
// Core service: runs periodically or on-demand
struct TaxonomyInductionService {
    llm_client: LlmClient,
    embedder: Embedder,
    oxigraph: OxigraphStore,
}

impl TaxonomyInductionService {
    // Main entry point
    async fn induce_taxonomy(
        &self,
        corpus: &Corpus,
        seed_taxonomy: Option<Taxonomy>,  // for enrichment mode
        config: InductionConfig,
    ) -> Result<TaxonomyVersion> {
        // 1. Sample documents for LLM context
        let samples = self.sample_representative_docs(corpus, config.sample_size)?;
        
        // 2. Generate candidate concepts (batch LLM calls)
        let candidates = self.llm_extract_concepts(&samples).await?;
        
        // 3. Deduplicate & normalize
        let concepts = self.normalize_concepts(candidates).await?;
        
        // 4. Infer hierarchy (LLM + structural validation)
        let hierarchy = self.llm_infer_hierarchy(&concepts, seed_taxonomy).await?;
        
        // 5. Validate with SHACL + semantic checks
        let validated = self.validate_taxonomy(&hierarchy).await?;
        
        // 6. Publish to Oxigraph with versioning
        self.publish_taxonomy(validated).await
    }
}
```

### Phase 2: Classifier Distillation (Weeks 3-4)

```rust
struct ClassifierDistillation {
    llm_client: LlmClient,
    trainer: ClassifierTrainer,
}

impl ClassifierDistillation {
    async fn distill(&self, taxonomy: &Taxonomy) -> Result<DeployedClassifier> {
        // For each leaf category, generate training examples
        let mut training_data = Vec::new();
        for category in taxonomy.leaf_categories() {
            let examples = self.llm_generate_examples(category, n=50).await?;
            training_data.extend(examples.into_iter().map(|ex| (ex.text, category.id)));
        }
        
        // Train lightweight classifier
        let model = self.trainer.train(training_data).await?;
        
        // Validate on held-out set
        let metrics = self.evaluate(&model, &taxonomy).await?;
        
        Ok(DeployedClassifier { model, taxonomy_version: taxonomy.version(), metrics })
    }
}
```

### Phase 3: Drift Detection & Re-induction Trigger (Week 4-5)

```rust
struct DriftMonitor {
    classifier: DeployedClassifier,
    embedder: Embedder,
}

impl DriftMonitor {
    fn check_drift(&self, new_documents: &[Document]) -> DriftSignal {
        // 1. Classification confidence distribution shift
        let confidence_shift = self.measure_confidence_shift(new_documents);
        
        // 2. Embedding space coverage gap
        let coverage_gap = self.measure_coverage_gap(new_documents);
        
        // 3. "Other" category growth rate
        let other_rate = self.measure_other_category_growth(new_documents);
        
        DriftSignal {
            confidence_shift,
            coverage_gap,
            other_rate,
            should_reinduce: confidence_shift > 0.15 || coverage_gap > 0.2 || other_rate > 0.1,
        }
    }
}
```

---

## RDF/Provenance Integration

| Artifact | Graph | Key Properties |
|----------|-------|----------------|
| Taxonomy version | `taxonomy/versions` | `tx:version`, `prov:generatedAtTime`, `tx:inductionConfig` |
| LLM prompts/responses | `taxonomy/provenance` | `prov:Activity`, `tx:promptTemplate`, `tx:modelVersion` |
| Classifier model | `taxonomy/models` | `tx:Classifier`, `tx:trainingDataHash`, `tx:metrics` |
| Drift signals | `taxonomy/evaluation` | `tx:DriftSignal`, `tx:timestamp`, `tx:triggeredReinduction` |

---

## Critical Design Decisions from TnT-LLM

1. **Separate induction from classification** — They are different workloads with different SLAs
2. **LLM for abstraction, not classification** — Use LLM semantic reasoning where it shines
3. **Explicit taxonomy as artifact** — Not latent in model weights; inspectable, versionable, auditable
4. **Human-in-the-loop optional but valuable** — Especially for root categories and cross-cutting dimensions
5. **Monitor → Re-induce loop** — Taxonomy is not static; build the feedback loop from day one

---

## Integration with Our Pipeline (from Research Doc)

```
Our Pipeline Stage          TnT-LLM Correspondence
─────────────────────────    ──────────────────────
1. Corpus ingestion          →  Document sampling
2. Concept extraction        →  LLM concept extraction
3. Concept normalization     →  LLM deduplication + our canonicalization
4. Semantic analysis         →  LLM hierarchy inference
5. Concept clustering        →  (Our clustering supplements LLM)
6. Hierarchy induction       →  LLM hierarchy + our validation
7. Semantic refinement       →  LLM refinement passes
8. Taxonomy validation       →  SHACL + our metrics (EEG from TaxCE)
9. RDF publication           →  SKOS + PROV-O + versioning
10. Classifier distillation  →  Phase 2 (NEW - not in our pipeline yet)
11. Drift monitoring         →  Phase 3 (NEW)
```

---

## Open Questions for Implementation

1. **LLM Provider Abstraction** — Need trait for OpenAI/Anthropic/local (llama.cpp) to avoid vendor lock-in
2. **Prompt Versioning** — Prompts are code; store in git, reference by hash in provenance
3. **Cost Controls** — Token budget per induction run; sampling strategy for large corpora
4. **Partial Re-induction** — Can we extend taxonomy without full rebuild? (TaxoAdapt addresses this)
5. **Multi-dimensional Taxonomies** — TnT-LLM assumes single hierarchy; we need facet support (TaxoAdapt/CAM-Taxo)

---

## Next Steps

1. Implement `TaxonomyInductionService` with mock LLM for testing
2. Add classifier distillation as separate module (`distill` crate)
3. Build drift monitoring into cron job that checks nightly
4. Design prompt templates as versioned assets (not hardcoded strings)