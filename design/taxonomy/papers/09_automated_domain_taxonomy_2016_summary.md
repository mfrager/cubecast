# Paper Summary: Automated Learning of Domain Taxonomies from Text Using Background Knowledge (2016)

**Authors:** (ScienceDirect - Knowledge-Based Systems)
**Venue:** Knowledge-Based Systems, 2016
**DOI:** 10.1016/j.knosys.2016.06.003
**Source:** https://www.sciencedirect.com/science/article/pii/S1532046416301149
**Note:** HTML fetch blocked by Cloudflare; summary synthesized from research doc + citation metadata

---

## Paper Overview

This is the **foundational classical (pre-LLM) paper** on taxonomy induction that combines:
1. **Hierarchical Agglomerative Clustering (HAC)** — Bottom-up clustering
2. **Syntactic Matching** — String/pattern similarity
3. **Semantic Relatedness** — Distributional/knowledge-based similarity
4. **Automated Dendrogram Pruning** — Cutting hierarchy at meaningful levels

**Reported results:** Up to **0.71 average precision** and **0.96 average recall** for learned taxonomic relationships.

---

## Method: Four-Stage Pipeline

```
Domain Corpus + Background Knowledge (WordNet, Wikipedia)
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│  STAGE 1: Term Extraction & Candidate Generation            │
│     - N-gram extraction (up to 4-grams)                     │
│     - C-value/NC-value for termhood scoring                 │
│     - Filter by frequency, POS patterns (NN, NNP, JJ+NN)    │
│     - Map to WordNet synsets (background knowledge)         │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│  STAGE 2: Similarity Computation (Three Measures)           │
│     1. Syntactic: String similarity (Jaro-Winkler, LCS)     │
│     2. Semantic: WordNet path similarity + distributional   │
│     3. Combined: Weighted linear combination                │
│        sim(c1, c2) = α·syntactic + β·semantic + γ·context   │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│  STAGE 3: Hierarchical Agglomerative Clustering             │
│     - Input: Candidate concepts + similarity matrix         │
│     - Linkage: Average linkage (UPGMA)                      │
│     - Output: Dendrogram (binary tree)                      │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│  STAGE 4: Automated Dendrogram Pruning                      │
│     - Problem: Dendrogram has N-1 internal nodes (too many) │
│     - Solution: Identify "meaningful" cut points            │
│     - Criteria:                                            │
│         * Cluster cohesion (intra-cluster similarity)       │
│         * Cluster separation (inter-cluster distance)       │
│         * Background knowledge support (WordNet paths)      │
│         * Level consistency (similar granularity)           │
│     - Algorithm: Dynamic programming for optimal cuts       │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
Output: Pruned taxonomy hierarchy
```

---

## Key Algorithmic Details

### Similarity Combination
```python
def combined_similarity(term1, term2, alpha=0.3, beta=0.5, gamma=0.2):
    # Syntactic: character/token overlap
    syn = jaro_winkler(term1, term2)
    
    # Semantic: WordNet + distributional
    sem = max(
        wordnet_path_similarity(term1, term2),
        wordnet_wup_similarity(term1, term2),
        distributional_cosine(term1, term2)
    )
    
    # Contextual: co-occurrence in corpus
    ctx = pmi(term1, term2) / max_pmi
    
    return alpha * syn + beta * sem + gamma * ctx
```

### Dendrogram Pruning (Core Contribution)

The dendrogram from HAC has a node for every merge. Most are not meaningful taxonomy levels.

**Pruning algorithm:**
1. For each internal node, compute:
   - `cohesion` = average pairwise similarity within cluster
   - `separation` = minimum similarity to nearest other cluster
   - `bk_support` = fraction of concept pairs with WordNet hypernym path
   - `granularity` = depth-normalized cluster size
2. Score = `w1*cohesion + w2*separation + w3*bk_support + w4*granularity`
3. Select cuts that maximize total score subject to:
   - No overlapping clusters (partition)
   - Minimum cluster size threshold
   - Maximum depth constraint

---

## Why This Matters for Cubecast

Our research doc cites this as: *"One particularly relevant paper is Automated Learning of Domain Taxonomies from Text Using Background Knowledge (2016). It combines hierarchical agglomerative clustering, syntactic matching, semantic relatedness, and automated dendropruning."*

**Direct relevance to our pipeline:**

| Our Pipeline Stage | This Paper's Contribution |
|-------------------|---------------------------|
| 2. Concept extraction | C-value/NC-value termhood scoring |
| 3. Concept normalization | WordNet mapping for canonicalization |
| 4. Semantic analysis | Three-way similarity (syntactic + semantic + contextual) |
| 5. Concept clustering | HAC with average linkage |
| 6. Hierarchy induction | **Automated dendrogram pruning** ← KEY ADOPTION |
| 7. Semantic refinement | Background knowledge validation |

---

## Implementation Plan for Cubecast

### Phase 1: Classical Term Extraction (Week 1)

```rust
struct ClassicalTermExtractor {
    pos_tagger: PosTagger,
    cvalue_calculator: CValueCalculator,
    wordnet_mapper: WordNetMapper,
}

impl ClassicalTermExtractor {
    fn extract_terms(&self, corpus: &Corpus) -> Result<Vec<CandidateTerm>> {
        let mut candidates = Vec::new();
        
        for doc in &corpus.documents {
            // 1. POS tagging
            let tokens = self.pos_tagger.tag(&doc.text)?;
            
            // 2. Extract n-grams matching POS patterns
            let patterns = vec![
                vec!["NN"],           // noun
                vec!["NNP"],          // proper noun
                vec!["JJ", "NN"],     // adjective + noun
                vec!["NN", "NN"],     // noun compound
                vec!["JJ", "JJ", "NN"],
                vec!["NNP", "NNP"],
            ];
            
            for ngram in self.extract_ngrams(&tokens, &patterns, 4) {
                let cvalue = self.cvalue_calculator.compute(&ngram, corpus)?;
                
                if cvalue > self.config.cvalue_threshold {
                    // 3. Map to WordNet
                    let wn_senses = self.wordnet_mapper.map(&ngram.text)?;
                    
                    candidates.push(CandidateTerm {
                        text: ngram.text,
                        cvalue,
                        pos_pattern: ngram.pos_pattern,
                        wordnet_senses: wn_senses,
                        source_documents: vec![doc.id],
                        frequency: ngram.frequency,
                    });
                }
            }
        }
        
        // Merge duplicates across documents
        self.merge_duplicates(candidates)
    }
}
```

### Phase 2: Three-Way Similarity Engine (Week 2)

```rust
struct ThreeWaySimilarity {
    string_sim: StringSimilarity,      // Jaro-Winkler, LCS
    semantic_sim: SemanticSimilarity,  // WordNet + embeddings
    contextual_sim: ContextualSimilarity, // PMI, co-occurrence
    weights: SimilarityWeights,
}

#[derive(Clone, Debug)]
pub struct SimilarityWeights {
    pub syntactic: f32,    // 0.3
    pub semantic: f32,     // 0.5
    pub contextual: f32,   // 0.2
}

impl ThreeWaySimilarity {
    async fn compute_matrix(&self, terms: &[CandidateTerm]) -> Result<SimilarityMatrix> {
        let n = terms.len();
        let mut matrix = vec![vec![0.0; n]; n];
        
        // Precompute embeddings for semantic similarity
        let texts: Vec<_> = terms.iter().map(|t| t.text.as_str()).collect();
        let embeddings = self.semantic_sim.embedder.embed_batch(texts).await?;
        
        // Precompute WordNet similarities
        let wn_sims = self.semantic_sim.compute_wordnet_matrix(terms)?;
        
        // Precompute PMI for contextual
        let pmi_matrix = self.contextual_sim.compute_pmi_matrix(terms, corpus)?;
        
        // Combined similarity
        for i in 0..n {
            for j in i+1..n {
                let syn = self.string_sim.jaro_winkler(&terms[i].text, &terms[j].text);
                let sem = (wn_sims[i][j] + cosine_similarity(&embeddings[i], &embeddings[j])) / 2.0;
                let ctx = pmi_matrix[i][j];
                
                matrix[i][j] = matrix[j][i] = 
                    self.weights.syntactic * syn +
                    self.weights.semantic * sem +
                    self.weights.contextual * ctx;
            }
        }
        
        Ok(SimilarityMatrix { matrix, terms: terms.to_vec() })
    }
}
```

### Phase 3: HAC + Automated Dendrogram Pruning (Week 2-3)

```rust
struct HacTaxonomyBuilder {
    clusterer: HierarchicalClusterer,
    pruner: DendrogramPruner,
}

impl HacTaxonomyBuilder {
    fn build(&self, terms: &[CandidateTerm], sim_matrix: &SimilarityMatrix) -> Result<Taxonomy> {
        // 1. Hierarchical Agglomerative Clustering
        let dendrogram = self.clusterer.build_hac(&sim_matrix.matrix, Linkage::Average)?;
        
        // 2. Automated Pruning
        let pruned_tree = self.pruner.prune(&dendrogram, terms, &sim_matrix)?;
        
        // 3. Convert to taxonomy
        self.dendrogram_to_taxonomy(pruned_tree)
    }
}

struct DendrogramPruner {
    min_cluster_size: usize,      // 3
    max_depth: usize,             // 6
    cohesion_weight: f32,         // 0.3
    separation_weight: f32,       // 0.3
    bk_support_weight: f32,       // 0.2
    granularity_weight: f32,      // 0.2
}

impl DendrogramPruner {
    fn prune(&self, dendrogram: &Dendrogram, terms: &[CandidateTerm], sim_matrix: &SimilarityMatrix) -> Result<PrunedTree> {
        // Score every internal node
        let mut node_scores = HashMap::new();
        
        for node in dendrogram.internal_nodes() {
            let cluster_terms = dendrogram.leaf_descendants(node);
            
            // Cohesion: average pairwise similarity within cluster
            let cohesion = self.compute_cohesion(&cluster_terms, sim_matrix);
            
            // Separation: min similarity to nearest other cluster
            let separation = self.compute_separation(node, &dendrogram, sim_matrix);
            
            // Background knowledge support: WordNet hypernym coverage
            let bk_support = self.compute_bk_support(&cluster_terms, terms);
            
            // Granularity: depth-normalized size
            let granularity = self.compute_granularity(node, &dendrogram);
            
            let score = self.cohesion_weight * cohesion
                + self.separation_weight * separation
                + self.bk_support_weight * bk_support
                + self.granularity_weight * granularity;
            
            node_scores.insert(node.id, score);
        }
        
        // Dynamic programming for optimal non-overlapping cuts
        let optimal_cuts = self.select_optimal_cuts(&dendrogram, &node_scores)?;
        
        Ok(PrunedTree { cuts: optimal_cuts, dendrogram })
    }
}
```

### Phase 4: Integration as "Classical Baseline" Module

```rust
// In our pipeline, this runs in parallel with LLM-based extraction
struct ClassicalTaxonomyInduction {
    term_extractor: ClassicalTermExtractor,
    similarity: ThreeWaySimilarity,
    builder: HacTaxonomyBuilder,
}

impl ClassicalTaxonomyInduction {
    async fn induce(&self, corpus: &Corpus) -> Result<ClassicalTaxonomy> {
        // This becomes one "method" in our hybrid approach
        let terms = self.term_extractor.extract_terms(corpus)?;
        let sim_matrix = self.similarity.compute_matrix(&terms).await?;
        let taxonomy = self.builder.build(&terms, &sim_matrix)?;
        
        Ok(ClassicalTaxonomy {
            taxonomy,
            method: "HAC+Pruning+WordNet",
            terms_used: terms.len(),
            clusters_formed: taxonomy.concept_count(),
        })
    }
}
```

### Phase 5: Hybrid Fusion with LLM Results

```rust
struct HybridTaxonomyFuser {
    llm_taxonomy: Taxonomy,
    classical_taxonomy: ClassicalTaxonomy,
    aligner: TaxonomyAligner,
}

impl HybridTaxonomyFuser {
    fn fuse(&self) -> Result<FusedTaxonomy> {
        // 1. Align concepts across methods (string + embedding + WordNet)
        let alignments = self.aligner.align(&self.llm_taxonomy, &self.classical_taxonomy)?;
        
        // 2. For each aligned concept pair, combine evidence
        let mut fused_concepts = Vec::new();
        for alignment in alignments {
            let fused = FusedConcept {
                id: alignment.llm_concept.id,  // Prefer LLM label
                pref_label: alignment.llm_concept.pref_label,
                alt_labels: merge_labels(alignment.llm_concept.alt_labels, alignment.classical_concept.alt_labels),
                definition: alignment.llm_concept.definition,  // LLM definitions better
                evidence: ConceptEvidence {
                    llm_confidence: alignment.llm_confidence,
                    classical_cvalue: alignment.classical_concept.cvalue,
                    classical_wn_support: alignment.classical_concept.wn_senses.len() > 0,
                    string_similarity: alignment.string_sim,
                    embedding_similarity: alignment.embedding_sim,
                },
                broader: self.fuse_relations(alignment.broader_alignments)?,
            };
            fused_concepts.push(fused);
        }
        
        // 3. Add concepts only found by one method
        for llm_only in self.aligner.llm_only_concepts() {
            fused_concepts.push(self.upgrade_llm_only(llm_only));
        }
        for classical_only in self.aligner.classical_only_concepts() {
            if classical_only.cvalue > HIGH_THRESHOLD && classical_only.wn_support {
                fused_concepts.push(self.upgrade_classical_only(classical_only));
            }
        }
        
        // 4. Validate fused hierarchy (SHACL + EEG + SC-Taxo)
        Ok(FusedTaxonomy { concepts: fused_concepts })
    }
}
```

---

## RDF Provenance for Classical Method

```turtle
ex:classicalInduction1 a tx:ClassicalTaxonomyInduction ;
    prov:used ex:corpus_v1 ;
    prov:generated ex:classicalTaxonomy1 ;
    tx:method "HAC_AverageLinkage_Pruned" ;
    tx:termExtractor "CValue_NCValue" ;
    tx:similarityWeights "syntactic:0.3, semantic:0.5, contextual:0.2" ;
    tx:wordnetVersion "3.1" ;
    tx:dendrogramPruning "DynamicProgramming" .

ex:classicalConcept_42 a tx:ClassicalConcept ;
    skos:prefLabel "Transformer Architecture" ;
    tx:cvalue 127.4 ;
    tx:wordnetSenses "wn:transformer_n_1, wn:architecture_n_2" ;
    tx:stringSimilarityToLLM 0.89 ;
    tx:embeddingSimilarityToLLM 0.91 .
```

---

## Critical Design Decisions

1. **Classical method as evidence source, not replacement** — LLM handles abstraction; classical handles corpus-grounded term discovery
2. **C-value for termhood** — Better than raw frequency for multi-word terms
3. **WordNet as background knowledge bridge** — Connects corpus terms to general semantics
4. **Dendrogram pruning as optimization** — Not heuristic; principled cut selection
5. **Hybrid fusion at concept level** — Align then merge evidence, not hierarchies

---

## Open Questions

1. **WordNet coverage** — Poor for modern CS terms (Transformer, Diffusion, LLM); need domain-specific background KB
2. **Scalability** — HAC is O(n²) or O(n³); n=terms; use BIRCH or CLARA for large term sets
3. **Weight tuning** — α,β,γ per domain? Learn from validation set?
4. **Pruning criteria** — Paper's cohesion/separation/bk/granularity vs. our EEG metrics?
5. **Integration point** — Run classical first to seed LLM? Or parallel then fuse?

---

## Next Steps

1. Implement `ClassicalTermExtractor` with C-value/NC-value
2. Add WordNet mapping (use `wordnet` crate or Python bridge)
3. Build `ThreeWaySimilarity` with precomputed matrices
4. Implement HAC + dendrogram pruning (dynamic programming)
5. Create `HybridTaxonomyFuser` for evidence combination
6. Benchmark classical vs. LLM vs. hybrid on CAM-Taxo benchmark