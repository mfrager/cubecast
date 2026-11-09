# Paper Summary: TaxCE: A Framework for Automated Taxonomy Construction and Evaluation at Scale (2026)

**Authors:** Sandeep Sricharan Mukku, Albert Aristotle Nanda, Rohit Pyati
**Venue:** EMNLP 2026 Industry Track
**DOI:** 10.18653/2026.emnlp-industry.76
**Source:** https://aclanthology.org/2026.emnlp-industry.76/

---

## Paper Overview

TaxCE is **the most directly applicable paper for our system**. It presents a fully automated framework that constructs multi-level hierarchical taxonomies from raw text through **progressive condensation**, and introduces **three corpus-grounded evaluation metrics (EEG)** integrated into a **metrics-in-the-loop iterative refinement mechanism**.

---

## TaxCE Pipeline: Progressive Condensation

```
Raw Documents
    ↓
[Stage 1] Actionable Segments
    - Split documents into semantically coherent segments
    - Filter noise (boilerplate, headers, footers)
    - Output: Segment corpus with document provenance
    ↓
[Stage 2] Deduplicated Semantic Units
    - Embed segments → cluster near-duplicates
    - Merge paraphrases, near-duplicates
    - Output: Unique semantic units with support counts
    ↓
[Stage 3] Granular Topics + Definitions
    - LLM generates topic label + definition per unit
    - Quality filter: specificity, coherence, actionability
    - Output: (topic, definition, evidence_segments) tuples
    ↓
[Stage 4] Bottom-Up Hierarchical Construction
    - Embed topic definitions
    - Hierarchical clustering (agglomerative)
    - LLM validates/naming parent nodes
    - Output: Draft taxonomy tree
    ↓
[Stage 5] Iterative Evaluation & Refinement (EEG Loop)
    - Compute Exclusivity, Exhaustivity, Granularity
    - Diagnose deficiencies → targeted corrections
    - Repeat until convergence
    ↓
Final Taxonomy
```

---

## The EEG Metrics (Core Contribution)

### 1. Exclusivity — *Categories should be semantically distinct*
> "A category should not substantially duplicate another category at the same level."

**Computation:**
- For each sibling pair (c₁, c₂) at same level under parent p:
  - Embed definitions → cosine similarity
  - Exclusivity(c₁, c₂) = 1 - similarity(def₁, def₂)
- Parent exclusivity = average over all sibling pairs
- **Taxonomy exclusivity** = weighted average over all parents

**Threshold:** Target > 0.7 (siblings clearly distinct)

### 2. Exhaustivity — *Taxonomy should cover the underlying corpus*
> "The taxonomy should account for the relevant material in the corpus."

**Computation:**
- For each source segment, find best-matching leaf category (embedding similarity)
- Exhaustivity = % segments assigned to a category with similarity > θ
- Tracks long-tail coverage (not just head concepts)

**Threshold:** Target > 0.85

### 3. Granularity — *Categories at appropriate level of detail*
> "A category should not be excessively broad or unnecessarily specific."

**Computation:**
- For each category, compute:
  - **Breadth:** Number of descendant leaves / total leaves
  - **Depth:** Distance from root
  - **Support:** Number of source segments mapped
- Granularity score = f(breadth, depth, support) — penalizes:
  - Too broad: high breadth, low depth, high support → should split
  - Too specific: low breadth, high depth, low support → should merge

**Threshold:** Balanced distribution across levels

---

## Metrics-in-the-Loop Refinement

```
While not converged:
    1. Compute EEG metrics for current taxonomy
    2. Diagnose:
       - Low exclusivity siblings → MERGE or RELABEL
       - Low exhaustivity segments → ADD categories
       - Poor granularity nodes → SPLIT (broad) or MERGE (narrow)
    3. Generate correction proposals (LLM + rules)
    4. Apply corrections → new taxonomy version
    5. Re-compute EEG
    6. Check convergence (ΔEEG < ε for N iterations)
```

**Correction operators:**
| Deficiency | Operator | Trigger |
|------------|----------|---------|
| Sibling similarity > 0.8 | `MERGE_SIBLINGS` | Exclusivity |
| Segment cluster unassigned | `ADD_CATEGORY` | Exhaustivity |
| Category support > θ & breadth > 0.5 | `SPLIT_CATEGORY` | Granularity |
| Category support < θ & depth > 3 | `MERGE_INTO_PARENT` | Granularity |
| Orphan category (no segments) | `REMOVE_CATEGORY` | Exhaustivity |

---

## Why This Is Critical for Cubecast

Our research doc explicitly endorses: *"This is a very good architectural direction for an RDF taxonomy-generation system"* and *"The interesting feature is its metrics-in-the-loop refinement process. Rather than accepting the first generated hierarchy, the system diagnoses weaknesses and makes targeted corrections."*

**Direct adoption items:**
1. **EEG metrics** → Our evaluation framework (Section 7 of research doc)
2. **Progressive condensation stages** → Our pipeline stages 1-4
3. **Iterative refinement loop** → Our validation + revision system (Section 8)
4. **Corpus-grounded evaluation** → Not just structural; tied to source evidence

---

## Implementation Plan for Cubecast

### Phase 1: Progressive Condensation Pipeline (Weeks 1-3)

```rust
struct ProgressiveCondensationPipeline {
    segmenter: DocumentSegmenter,
    deduplicator: SemanticDeduplicator,
    topic_generator: TopicGenerator,
    hierarchy_builder: HierarchyBuilder,
}

impl ProgressiveCondensationPipeline {
    async fn run(&self, corpus: &Corpus) -> Result<DraftTaxonomy> {
        // Stage 1: Actionable Segments
        let segments = self.segmenter.segment_corpus(corpus).await?;
        
        // Stage 2: Deduplicated Semantic Units
        let semantic_units = self.deduplicator.deduplicate(segments).await?;
        
        // Stage 3: Granular Topics + Definitions
        let topics = self.topic_generator.generate_topics(semantic_units).await?;
        
        // Stage 4: Bottom-Up Hierarchy
        let draft_taxonomy = self.hierarchy_builder.build(topics).await?;
        
        Ok(draft_taxonomy)
    }
}

// Stage 1: Document Segmenter
struct DocumentSegmenter {
    llm: LlmClient,
    min_segment_length: usize,
    max_segment_length: usize,
}

impl DocumentSegmenter {
    async fn segment_corpus(&self, corpus: &Corpus) -> Result<Vec<Segment>> {
        let mut all_segments = Vec::new();
        
        for doc in &corpus.documents {
            // LLM-based semantic segmentation (better than fixed-window)
            let segments = self.llm_segment_document(doc).await?;
            
            // Filter noise
            let filtered: Vec<_> = segments.into_iter()
                .filter(|s| s.text.len() >= self.min_segment_length)
                .filter(|s| !self.is_boilerplate(s))
                .collect();
            
            all_segments.extend(filtered.into_iter().map(|s| Segment {
                id: Uuid::new_v4(),
                document_id: doc.id,
                text: s.text,
                position: s.position,
                metadata: s.metadata,
            }));
        }
        
        Ok(all_segments)
    }
}

// Stage 2: Semantic Deduplicator
struct SemanticDeduplicator {
    embedder: Embedder,
    similarity_threshold: f32,  // 0.85-0.95
}

impl SemanticDeduplicator {
    async fn deduplicate(&self, segments: Vec<Segment>) -> Result<Vec<SemanticUnit>> {
        // Embed all segments
        let texts: Vec<_> = segments.iter().map(|s| s.text.as_str()).collect();
        let embeddings = self.embedder.embed_batch(texts).await?;
        
        // Cluster near-duplicates (HDBSCAN or agglomerative with high threshold)
        let clusters = self.cluster_embeddings(&embeddings, self.similarity_threshold)?;
        
        // Merge each cluster into semantic unit
        let mut units = Vec::new();
        for cluster in clusters {
            let representative = self.select_representative(&cluster, &segments);
            let evidence_segments: Vec<_> = cluster.iter().map(|idx| segments[*idx].id).collect();
            
            units.push(SemanticUnit {
                id: Uuid::new_v4(),
                representative_text: representative.text,
                evidence_segment_ids: evidence_segments,
                support_count: evidence_segments.len(),
                source_documents: evidence_segments.iter()
                    .map(|sid| segments[*sid].document_id)
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect(),
            });
        }
        
        Ok(units)
    }
}

// Stage 3: Topic Generator (LLM)
struct TopicGenerator {
    llm: LlmClient,
}

impl TopicGenerator {
    async fn generate_topics(&self, units: Vec<SemanticUnit>) -> Result<Vec<Topic>> {
        let mut topics = Vec::new();
        
        for unit in units {
            let prompt = format!(
                "Generate a concise topic label and clear definition for this content:\n\n{}",
                unit.representative_text
            );
            
            let response = self.llm.generate(&prompt, GenerationConfig::structured()).await?;
            let (label, definition) = self.parse_topic_response(&response)?;
            
            // Quality filter
            if self.is_specific_enough(&label, &definition) && self.is_coherent(&label, &definition) {
                topics.push(Topic {
                    id: Uuid::new_v4(),
                    label,
                    definition,
                    source_unit_id: unit.id,
                    evidence_segments: unit.evidence_segment_ids,
                    support_count: unit.support_count,
                });
            }
        }
        
        Ok(topics)
    }
}

// Stage 4: Hierarchy Builder
struct HierarchyBuilder {
    embedder: Embedder,
    llm: LlmClient,
    clusterer: HierarchicalClusterer,
}

impl HierarchyBuilder {
    async fn build(&self, topics: Vec<Topic>) -> Result<DraftTaxonomy> {
        // Embed topic definitions
        let definitions: Vec<_> = topics.iter().map(|t| t.definition.as_str()).collect();
        let embeddings = self.embedder.embed_batch(definitions).await?;
        
        // Hierarchical agglomerative clustering
        let dendrogram = self.clusterer.build_hierarchy(&embeddings)?;
        
        // Extract clusters at multiple levels
        let level_clusters = self.clusterer.extract_levels(&dendrogram, max_levels=5)?;
        
        // LLM naming for internal nodes
        let mut named_nodes = Vec::new();
        for level_clusters in level_clusters {
            for cluster in level_clusters {
                if cluster.is_leaf {
                    named_nodes.push(NamedNode::Leaf(cluster.topic_id));
                } else {
                    let label = self.llm_name_cluster(&cluster, &topics).await?;
                    let definition = self.llm_define_cluster(&cluster, &topics).await?;
                    named_nodes.push(NamedNode::Internal {
                        id: Uuid::new_v4(),
                        label,
                        definition,
                        children: cluster.child_ids,
                    });
                }
            }
        }
        
        // Build tree structure
        let taxonomy = self.assemble_tree(named_nodes)?;
        
        Ok(DraftTaxonomy { taxonomy, topics, version: 1 })
    }
}
```

### Phase 2: EEG Metrics Engine (Week 3-4)

```rust
struct EEGMetricsEngine {
    embedder: Embedder,
    llm: LlmClient,  // For semantic judgment when embeddings insufficient
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EEGMetrics {
    pub exclusivity: f32,
    pub exhaustivity: f32,
    pub granularity: f32,
    pub per_level: HashMap<u32, LevelMetrics>,
    pub per_category: HashMap<ConceptId, CategoryMetrics>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CategoryMetrics {
    pub exclusivity: f32,      // Avg distinctness from siblings
    pub exhaustivity: f32,     // Coverage of assigned segments
    pub granularity: f32,      // Appropriateness of specificity
    pub support_count: usize,  // Number of source segments
    pub sibling_similarities: Vec<(ConceptId, f32)>,  // For debugging
}

impl EEGMetricsEngine {
    async fn compute(&self, taxonomy: &Taxonomy, corpus: &Corpus, topics: &[Topic]) -> Result<EEGMetrics> {
        // Exclusivity: sibling definition embeddings
        let exclusivity = self.compute_exclusivity(taxonomy).await?;
        
        // Exhaustivity: segment-to-leaf coverage
        let exhaustivity = self.compute_exhaustivity(taxonomy, corpus, topics).await?;
        
        // Granularity: breadth/depth/support balance
        let granularity = self.compute_granularity(taxonomy, topics).await?;
        
        Ok(EEGMetrics {
            exclusivity,
            exhaustivity,
            granularity,
            per_level: self.compute_per_level(taxonomy).await?,
            per_category: self.compute_per_category(taxonomy, topics).await?,
            timestamp: Utc::now(),
        })
    }
    
    async fn compute_exclusivity(&self, taxonomy: &Taxonomy) -> Result<f32> {
        let mut parent_scores = Vec::new();
        
        for parent_id in taxonomy.internal_nodes() {
            let children: Vec<_> = taxonomy.children(parent_id).collect();
            if children.len() < 2 { continue; }
            
            let mut pairwise_scores = Vec::new();
            for (i, c1) in children.iter().enumerate() {
                for c2 in &children[i+1..] {
                    let def1 = taxonomy.get_definition(c1);
                    let def2 = taxonomy.get_definition(c2);
                    let sim = self.embedder.similarity(def1, def2).await?;
                    pairwise_scores.push(1.0 - sim);  // Exclusivity = 1 - similarity
                }
            }
            
            parent_scores.push(pairwise_scores.iter().sum::<f32>() / pairwise_scores.len() as f32);
        }
        
        Ok(parent_scores.iter().sum::<f32>() / parent_scores.len() as f32)
    }
    
    async fn compute_exhaustivity(&self, taxonomy: &Taxonomy, corpus: &Corpus, topics: &[Topic]) -> Result<f32> {
        // Get all source segments from topics
        let all_segments: Vec<_> = topics.iter().flat_map(|t| &t.evidence_segments).collect();
        
        // Embed all leaf category definitions
        let leaves: Vec<_> = taxonomy.leaves().collect();
        let leaf_defs: Vec<_> = leaves.iter().map(|l| taxonomy.get_definition(l)).collect();
        let leaf_embeddings = self.embedder.embed_batch(leaf_defs).await?;
        
        let mut covered = 0;
        for segment_id in all_segments {
            let segment_text = corpus.get_segment(segment_id).unwrap().text;
            let seg_embedding = self.embedder.embed(segment_text).await?;
            
            // Find best matching leaf
            let max_sim = leaf_embeddings.iter()
                .map(|le| cosine_similarity(&seg_embedding, le))
                .fold(0.0, f32::max);
            
            if max_sim > 0.7 {  // Threshold from paper
                covered += 1;
            }
        }
        
        Ok(covered as f32 / all_segments.len() as f32)
    }
}
```

### Phase 3: Iterative Refinement Loop (Week 4-5)

```rust
struct IterativeRefinementEngine {
    eeg_engine: EEGMetricsEngine,
    corrector: TaxonomyCorrector,
    convergence_config: ConvergenceConfig,
}

#[derive(Clone, Debug)]
pub struct ConvergenceConfig {
    pub max_iterations: u32,           // 10-20
    pub exclusivity_threshold: f32,    // 0.7
    pub exhaustivity_threshold: f32,   // 0.85
    pub granularity_threshold: f32,    // 0.7
    pub delta_threshold: f32,          // 0.01
    pub stable_iterations: u32,        // 3
}

impl IterativeRefinementEngine {
    async fn refine(&self, draft: DraftTaxonomy, corpus: &Corpus) -> Result<RefinedTaxonomy> {
        let mut current = draft;
        let mut history = Vec::new();
        let mut stable_count = 0;
        
        for iteration in 0..self.convergence_config.max_iterations {
            // 1. Compute EEG metrics
            let metrics = self.eeg_engine.compute(&current.taxonomy, corpus, &current.topics).await?;
            history.push((iteration, metrics.clone()));
            
            // 2. Check convergence
            if self.is_converged(&metrics, &history, stable_count) {
                break;
            }
            
            // 3. Diagnose deficiencies
            let deficiencies = self.diagnose(&metrics, &current.taxonomy);
            
            // 4. Generate corrections
            let corrections = self.corrector.propose_corrections(&deficiencies, &current, corpus).await?;
            
            // 5. Apply corrections
            current = self.corrector.apply_corrections(current, corrections).await?;
            
            // 6. Validate structure (cycles, connectivity, etc.)
            self.validate_structure(&current.taxonomy)?;
        }
        
        Ok(RefinedTaxonomy {
            taxonomy: current.taxonomy,
            topics: current.topics,
            eeg_history: history,
            final_metrics: history.last().unwrap().1.clone(),
            iterations: history.len(),
        })
    }
    
    fn diagnose(&self, metrics: &EEGMetrics, taxonomy: &Taxonomy) -> Vec<Deficiency> {
        let mut deficiencies = Vec::new();
        
        // Low exclusivity siblings
        for (cat_id, cat_metrics) in &metrics.per_category {
            for (sibling_id, similarity) in &cat_metrics.sibling_similarities {
                if similarity > 0.8 {  // Low exclusivity
                    deficiencies.push(Deficiency {
                        type: DeficiencyType::LowExclusivity,
                        primary: cat_id.clone(),
                        related: vec![sibling_id.clone()],
                        severity: similarity,
                        suggested_action: CorrectionAction::MergeSiblings,
                    });
                }
            }
        }
        
        // Low exhaustivity (uncovered segments)
        if metrics.exhaustivity < self.convergence_config.exhaustivity_threshold {
            // Find uncovered segments via per-category exhaustivity
            for (cat_id, cat_metrics) in &metrics.per_category {
                if cat_metrics.exhaustivity < 0.5 {
                    deficiencies.push(Deficiency {
                        type: DeficiencyType::LowExhaustivity,
                        primary: cat_id.clone(),
                        related: vec![],
                        severity: 1.0 - cat_metrics.exhaustivity,
                        suggested_action: CorrectionAction::AddCategory,
                    });
                }
            }
        }
        
        // Poor granularity
        for (cat_id, cat_metrics) in &metrics.per_category {
            if cat_metrics.support_count > 100 && cat_metrics.granularity < 0.5 {
                deficiencies.push(Deficiency {
                    type: DeficiencyType::TooBroad,
                    primary: cat_id.clone(),
                    related: vec![],
                    severity: 1.0 - cat_metrics.granularity,
                    suggested_action: CorrectionAction::SplitCategory,
                });
            } else if cat_metrics.support_count < 5 && cat_metrics.granularity < 0.5 {
                deficiencies.push(Deficiency {
                    type: DeficiencyType::TooNarrow,
                    primary: cat_id.clone(),
                    related: vec![],
                    severity: 1.0 - cat_metrics.granularity,
                    suggested_action: CorrectionAction::MergeIntoParent,
                });
            }
        }
        
        deficiencies
    }
}
```

### Phase 4: Taxonomy Corrector (LLM + Rules)

```rust
struct TaxonomyCorrector {
    llm: LlmClient,
    embedder: Embedder,
}

impl TaxonomyCorrector {
    async fn propose_corrections(
        &self,
        deficiencies: &[Deficiency],
        taxonomy: &DraftTaxonomy,
        corpus: &Corpus,
    ) -> Result<Vec<Correction>> {
        let mut corrections = Vec::new();
        
        for deficiency in deficiencies {
            match deficiency.suggested_action {
                CorrectionAction::MergeSiblings => {
                    let (c1, c2) = (&deficiency.primary, &deficiency.related[0]);
                    let merge_proposal = self.llm_propose_merge(c1, c2, taxonomy).await?;
                    corrections.push(Correction::MergeSiblings {
                        concepts: vec![c1.clone(), c2.clone()],
                        new_label: merge_proposal.label,
                        new_definition: merge_proposal.definition,
                        confidence: merge_proposal.confidence,
                    });
                }
                CorrectionAction::SplitCategory => {
                    let split_proposal = self.llm_propose_split(&deficiency.primary, taxonomy, corpus).await?;
                    corrections.push(Correction::SplitCategory {
                        concept: deficiency.primary.clone(),
                        new_children: split_proposal.children,
                        confidence: split_proposal.confidence,
                    });
                }
                CorrectionAction::AddCategory => {
                    let add_proposal = self.llm_propose_new_category(&deficiency.primary, taxonomy, corpus).await?;
                    corrections.push(Correction::AddCategory {
                        parent: deficiency.primary.clone(),
                        new_concept: add_proposal.concept,
                        confidence: add_proposal.confidence,
                    });
                }
                CorrectionAction::MergeIntoParent => {
                    corrections.push(Correction::MergeIntoParent {
                        concept: deficiency.primary.clone(),
                        confidence: 0.8,
                    });
                }
            }
        }
        
        Ok(corrections)
    }
}
```

### Phase 5: RDF Integration with Provenance

```turtle
# Each refinement iteration as provenance activity
ex:taxceRun1 a tx:TaxCEGeneration ;
    prov:startedAtTime "2026-11-08T10:00:00Z"^^xsd:dateTime ;
    prov:endedAtTime "2026-11-08T10:45:00Z"^^xsd:dateTime ;
    tx:corpus ex:corpus_v1 ;
    tx:iterations 7 ;
    tx:finalEEG ex:eegMetrics_v7 .

ex:eegMetrics_v7 a tx:EEGMetrics ;
    tx:exclusivity 0.82 ;
    tx:exhaustivity 0.91 ;
    tx:granularity 0.78 ;
    tx:iteration 7 ;
    tx:converged true .

# Each correction operation tracked
ex:correction_42 a tx:CorrectionOperation ;
    tx:iteration 3 ;
    tx:action "SPLIT_CATEGORY" ;
    tx:targetConcept ex:DatabaseSystems ;
    tx:resultConcepts ex:RelationalDB , ex:NoSQLDB , ex:NewSQLDB ;
    tx:triggerMetric "granularity" ;
    tx:triggerValue 0.45 ;
    tx:llmProposal ex:llmProposal_42 ;
    prov:wasGeneratedBy ex:taxceRun1 .

# Final taxonomy with EEG scores per category
ex:RelationalDB a skos:Concept ;
    skos:prefLabel "Relational Database" ;
    skos:definition "Database based on relational model..." ;
    skos:broader ex:DatabaseSystems ;
    tx:eegExclusivity 0.85 ;
    tx:eegExhaustivity 0.93 ;
    tx:eegGranularity 0.81 ;
    tx:eegSupportCount 47 .
```

---

## Integration with Our Pipeline (Research Doc Stages)

| Research Doc Stage | TaxCE Contribution |
|-------------------|-------------------|
| 1. Corpus ingestion | → Document segmentation (Stage 1) |
| 2. Concept extraction | → Semantic unit deduplication (Stage 2) |
| 3. Concept normalization | → Topic label + definition generation (Stage 3) |
| 4. Semantic analysis | → Topic embeddings for clustering |
| 5. Concept clustering | → Hierarchical agglomerative clustering (Stage 4) |
| 6. Hierarchy induction | → Bottom-up construction + LLM naming (Stage 4) |
| 7. Semantic refinement | → **EEG metrics-in-the-loop (Stage 5)** ← **KEY ADOPTION** |
| 8. Taxonomy validation | → EEG + SHACL + structural |
| 9. RDF publication | → SKOS + EEG scores + full provenance |

---

## Critical Design Decisions

1. **EEG as first-class metrics** — Not post-hoc; drives the refinement loop
2. **Provenance per correction** — Every merge/split/add traced to metric trigger + LLM proposal
3. **Corpus-grounded** — Exhaustivity measured against source segments, not abstract quality
4. **Convergence criteria** — ΔEEG < ε for N iterations, not fixed iteration count
5. **Human-in-the-loop optional** — Corrections can be auto-applied or queued for review

---

## Open Questions

1. **Segmentation granularity** — Fixed window vs. LLM semantic vs. structural (sections)? LLM semantic best but costly
2. **Deduplication threshold** — 0.85-0.95; domain-dependent; tune per corpus
3. **LLM prompt engineering for corrections** — Critical for quality; version and test prompts
4. **Scalability** — Embedding all segments + pairwise sibling comparisons O(n²); need approximate nearest neighbors for large taxonomies
5. **Multi-dimensional EEG** — Compute EEG per facet (TaxoAdapt/CAM-Taxo) then aggregate?

---

## Next Steps

1. Implement `ProgressiveCondensationPipeline` with configurable stages
2. Build `EEGMetricsEngine` with embedding-based computation
3. Create `IterativeRefinementEngine` with convergence detection
4. Develop `TaxonomyCorrector` with LLM prompt templates for each correction type
5. Add EEG metrics to Oxigraph as first-class evaluation data
6. Integrate with CAM-Taxo benchmark for regression testing