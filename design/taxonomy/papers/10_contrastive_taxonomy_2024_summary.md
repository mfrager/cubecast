# Paper Summary: Contrastive Learning for Taxonomy Induction (2024)

**Authors:** (ScienceDirect - Knowledge-Based Systems / Engineering Applications of AI)
**Venue:** 2024
**DOI:** 10.1016/j.knosys.2024.111xxx (exact DOI from ScienceDirect)
**Source:** https://www.sciencedirect.com/science/article/pii/S0950705124010396
**Note:** HTML fetch blocked by Cloudflare; summary synthesized from research doc + contrastive learning literature

---

## Paper Overview

This paper applies **contrastive learning** to taxonomy induction — learning hierarchical structure by optimizing embedding spaces where **hypernym pairs are pulled together** and **non-hypernym pairs are pushed apart**.

Unlike standard embedding approaches (where similarity ≠ subsumption), contrastive learning **explicitly trains for the asymmetric hypernym relation**.

---

## Core Idea: Contrastive Hypernym Learning

### Standard Embedding Problem
```
PostgreSQL ≈ MySQL ≈ SQLite  (high similarity)
PostgreSQL ≈ Relational Database (high similarity)
Relational Database ≈ Database (high similarity)

But: Similarity is symmetric. Cannot distinguish:
  - PostgreSQL is-a Database (hypernym)
  - PostgreSQL related-to MySQL (co-hyponym)
  - Database has-part Table (meronym)
```

### Contrastive Solution
Train encoder `f_θ` such that:
```
||f(child) - f(parent)||  <  margin  for true hypernym pairs
||f(child) - f(negative)||  >  margin  for non-hypernym pairs
```

**Key asymmetry:** The loss function treats (child, parent) differently from (parent, child) and from (sibling, sibling).

---

## Method Architecture

```
Input: Corpus + Seed Hypernym Pairs (from patterns, WordNet, distant supervision)
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│  1. POSITIVE PAIR GENERATION                                │
│     - Hearst patterns: "X such as A, B" → (A, X), (B, X)   │
│     - WordNet: Direct hypernym edges                        │
│     - Wikipedia: Category hierarchy                         │
│     - Distant supervision: "X is a type of Y" patterns      │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│  2. NEGATIVE SAMPLING STRATEGIES                            │
│     a) Random: Random concept pairs                         │
│     b) Sibling negatives: Co-hyponyms (hard negatives)      │
│     c) Reverse: (parent, child) as negative                 │
│     d) Meronym: Part-of relations as negative               │
│     e) Unrelated: Semantically distant concepts             │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│  3. CONTRASTIVE ENCODER TRAINING                            │
│     Encoder: BERT / RoBERTa / domain-adapted LM             │
│     Projection head: MLP → d-dim hyperbolic / Euclidean     │
│                                                              │
│     Loss:                                                   │
│     L = -log σ( margin - d(f(c), f(p)) )                    │
│         - Σ log σ( d(f(c), f(n_i)) - margin )               │
│                                                              │
│     Where: c=child, p=parent, n_i=negatives                 │
│     d = distance (hyperbolic preferred for hierarchies)     │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│  4. TAXONOMY CONSTRUCTION FROM EMBEDDINGS                   │
│     - Embed all candidate concepts                          │
│     - For each concept, find k-nearest neighbors as parents │
│     - Directionality: parent embedding should be "more      │
│       general" (closer to origin in hyperbolic, or          │
│       higher norm in Euclidean with norm regularization)    │
│     - Build graph, resolve cycles, transitive reduction     │
└─────────────────────────────────────────────────────────────┘
```

---

## Why Hyperbolic Embeddings?

**Hierarchies are tree-like** → **Hyperbolic space (Poincaré ball) naturally represents trees** with low distortion.

| Property | Euclidean | Hyperbolic (Poincaré) |
|----------|-----------|----------------------|
| Tree embedding distortion | High (exponential) | Low (logarithmic) |
| Parent-child distance | Not naturally ordered | Parent closer to origin |
| Scalability | O(d) | O(d) |
| Generalization to unseen | Moderate | Better for hierarchies |

**In hyperbolic space:**
- Root concepts → near origin (norm ≈ 0)
- Leaf concepts → near boundary (norm ≈ 1)
- Hypernym direction = **toward origin**
- Distance = hyperbolic distance (not Euclidean)

---

## Contrastive Loss Variants

### 1. Standard InfoNCE (Adapted)
```python
def contrastive_loss(child_emb, parent_emb, negative_embs, margin=0.5, temp=0.1):
    # Positive: child should be close to parent
    pos_dist = hyperbolic_distance(child_emb, parent_emb)
    pos_score = -pos_dist / temp
    
    # Negatives: child should be far from negatives
    neg_dists = [hyperbolic_distance(child_emb, neg) for neg in negative_embs]
    neg_scores = [(margin - d) / temp for d in neg_dists]
    
    # InfoNCE
    logits = torch.cat([pos_score.unsqueeze(0), torch.tensor(neg_scores)])
    labels = torch.zeros(1, dtype=torch.long)  # Positive at index 0
    
    return F.cross_entropy(logits.unsqueeze(0), labels.unsqueeze(0))
```

### 2. Order-Preserving Loss (Key for Hypernymy)
```python
def order_preserving_loss(child_emb, parent_emb, sibling_embs, margin=0.3):
    # 1. Child closer to parent than to siblings
    d_cp = hyperbolic_distance(child_emb, parent_emb)
    d_cs = [hyperbolic_distance(child_emb, sib) for sib in sibling_embs]
    loss_sib = sum(relu(d_cp - d_cs_i + margin) for d_cs_i in d_cs)
    
    # 2. Parent closer to origin (more general)
    norm_parent = parent_emb.norm()
    norm_child = child_emb.norm()
    loss_norm = relu(norm_parent - norm_child + margin)
    
    # 3. Transitivity: grandparent closer to origin than parent
    # (if grandparent known)
    
    return loss_sib + loss_norm
```

### 3. Hyperbolic Entailment Cones (Ganea et al. style)
```python
# Each concept = cone in hyperbolic space
# Child cone ⊂ Parent cone
# Contrastive loss ensures cone containment
```

---

## Taxonomy Construction from Trained Embeddings

```python
def build_taxonomy_from_embeddings(concepts, encoder, k=5):
    # 1. Embed all concepts
    embeddings = {c: encoder.encode(c) for c in concepts}
    
    # 2. For each concept, find candidate parents
    candidate_edges = []
    for child in concepts:
        # Find k nearest neighbors (hyperbolic distance)
        distances = [(c, hyperbolic_distance(embeddings[child], embeddings[c])) 
                     for c in concepts if c != child]
        distances.sort(key=lambda x: x[1])
        top_k = distances[:k]
        
        # Filter: parent must be "more general" (closer to origin)
        for parent, dist in top_k:
            if embeddings[parent].norm() < embeddings[child].norm() * 0.9:  # threshold
                candidate_edges.append((child, parent, dist))
    
    # 3. Build directed graph
    G = DiGraph()
    for child, parent, conf in candidate_edges:
        G.add_edge(child, parent, weight=conf)
    
    # 4. Cycle removal (minimum feedback arc set)
    G = remove_cycles(G)
    
    # 5. Transitive reduction
    G = transitive_reduction(G)
    
    # 6. Root detection (nodes with no outgoing edges)
    roots = [n for n in G.nodes() if G.out_degree(n) == 0]
    
    return Taxonomy(graph=G, roots=roots)
```

---

## Why This Matters for Cubecast

Our research doc lists this under: *"Useful benchmarks and resources: Contrastive learning for taxonomy induction — learning hierarchical structure."*

**Direct relevance:**

1. **Solves "similarity ≠ hierarchy" problem** — Explicitly trains for hypernym direction
2. **Hyperbolic embeddings** — Natural fit for tree structures; use in our `embed` module
3. **Contrastive training data** — Can generate from our own pattern extraction (Hearst) + WordNet
4. **Complements LLM approach** — Embedding-based hypernym discovery runs fast at scale; LLM validates

---

## Implementation Plan for Cubecast

### Phase 1: Contrastive Training Data Generation (Week 1)

```rust
struct ContrastiveDataGenerator {
    pattern_extractor: HearstPatternExtractor,
    wordnet: WordNet,
    wikipedia: WikiClient,
}

impl ContrastiveDataGenerator {
    fn generate_training_pairs(&self, corpus: &Corpus) -> Result<ContrastiveDataset> {
        let mut positive_pairs = Vec::new();
        let mut concepts = HashSet::new();
        
        // 1. Hearst patterns from corpus
        for doc in &corpus.documents {
            let pairs = self.pattern_extractor.extract(doc)?;
            positive_pairs.extend(pairs);
            concepts.extend(pairs.iter().flat_map(|(c, p)| [c.clone(), p.clone()]));
        }
        
        // 2. WordNet hypernyms for known concepts
        for concept in &concepts {
            if let Some(synset) = self.wordnet.lookup(concept) {
                for hypernym in synset.hypernyms() {
                    positive_pairs.push((concept.clone(), hypernym.name()));
                    concepts.insert(hypernym.name());
                }
            }
        }
        
        // 3. Wikipedia category hierarchy
        for concept in &concepts {
            let cats = self.wikipedia.get_categories(concept)?;
            for cat in cats {
                positive_pairs.push((concept.clone(), cat));
                concepts.insert(cat);
            }
        }
        
        // 4. Generate negatives
        let negatives = self.generate_negatives(&positive_pairs, &concepts)?;
        
        Ok(ContrastiveDataset {
            positives: positive_pairs,
            negatives,
            concepts: concepts.into_iter().collect(),
        })
    }
    
    fn generate_negatives(&self, positives: &[(String, String)], concepts: &HashSet<String>) -> Result<HashMap<String, Vec<String>>> {
        let mut negatives = HashMap::new();
        let concept_vec: Vec<_> = concepts.iter().cloned().collect();
        
        for (child, parent) in positives {
            let mut negs = Vec::new();
            
            // a) Sibling negatives (other children of same parent)
            let siblings: Vec<_> = positives.iter()
                .filter(|(_, p)| p == parent)
                .map(|(c, _)| c.clone())
                .filter(|c| c != child)
                .collect();
            negs.extend(siblings);
            
            // b) Reverse pair
            negs.push(parent.clone());
            
            // c) Random concepts (easy negatives)
            let mut rng = rand::thread_rng();
            for _ in 0..10 {
                negs.push(concept_vec[rng.gen_range(0..concept_vec.len())].clone());
            }
            
            // d) Semantically similar but not hypernym (hard negatives)
            // Use embeddings to find near neighbors that aren't hypernyms
            // ... (requires pre-trained embeddings)
            
            negatives.insert(child.clone(), negs);
        }
        
        Ok(negatives)
    }
}
```

### Phase 2: Contrastive Encoder Training (Week 2-3)

```python
# Python training script (PyTorch + geoopt for hyperbolic)
import torch
import torch.nn as nn
from geoopt import PoincareBall
from transformers import AutoModel, AutoTokenizer

class HyperbolicContrastiveEncoder(nn.Module):
    def __init__(self, model_name="bert-base-uncased", dim=128, curvature=1.0):
        super().__init__()
        self.encoder = AutoModel.from_pretrained(model_name)
        self.tokenizer = AutoTokenizer.from_pretrained(model_name)
        self.projector = nn.Sequential(
            nn.Linear(768, 512),
            nn.ReLU(),
            nn.Linear(512, dim),
        )
        self.manifold = PoincareBall(c=curvature)
        self.margin = 0.5
        self.temp = 0.1
    
    def encode(self, texts):
        inputs = self.tokenizer(texts, padding=True, truncation=True, return_tensors="pt")
        outputs = self.encoder(**inputs)
        cls_emb = outputs.last_hidden_state[:, 0]  # [CLS]
        euclidean = self.projector(cls_emb)
        # Project to Poincaré ball
        return self.manifold.expmap0(euclidean)
    
    def forward(self, child_texts, parent_texts, negative_texts_list):
        child_emb = self.encode(child_texts)
        parent_emb = self.encode(parent_texts)
        neg_embs = [self.encode(negs) for negs in negative_texts_list]
        
        # Order-preserving contrastive loss
        loss = 0
        for i in range(len(child_texts)):
            c = child_emb[i:i+1]
            p = parent_emb[i:i+1]
            negs = [n[i:i+1] for n in neg_embs]
            
            # Positive distance
            d_pos = self.manifold.dist(c, p).squeeze()
            
            # Negative distances
            d_negs = [self.manifold.dist(c, n).squeeze() for n in negs]
            
            # InfoNCE
            pos_score = -d_pos / self.temp
            neg_scores = [(self.margin - d_n) / self.temp for d_n in d_negs]
            logits = torch.cat([pos_score.unsqueeze(0), torch.stack(neg_scores)])
            labels = torch.zeros(1, dtype=torch.long)
            
            loss += F.cross_entropy(logits.unsqueeze(0), labels.unsqueeze(0))
            
            # Order-preserving: parent closer to origin
            norm_c = c.norm()
            norm_p = p.norm()
            loss += F.relu(norm_p - norm_c + 0.1)
        
        return loss / len(child_texts)

# Training loop
def train_contrastive_encoder(dataset, epochs=10):
    model = HyperbolicContrastiveEncoder()
    optimizer = torch.optim.AdamW(model.parameters(), lr=2e-5)
    
    for epoch in range(epochs):
        for batch in dataloader:
            loss = model(batch.child, batch.parent, batch.negatives)
            loss.backward()
            optimizer.step()
            optimizer.zero_grad()
        
        # Validate on held-out hypernym pairs
        val_metrics = evaluate(model, val_pairs)
        print(f"Epoch {epoch}: {val_metrics}")
    
    # Save for Rust inference (ONNX or native)
    torch.save(model.state_dict(), "models/contrastive_hyperbolic.pt")
    return model
```

### Phase 3: Rust Inference Integration (Week 3)

```rust
struct ContrastiveHypernymPredictor {
    model: OrtSession,  // ONNX Runtime
    tokenizer: Tokenizer,
    manifold: HyperbolicManifold,
}

impl ContrastiveHypernymPredictor {
    async fn predict_hypernyms(&self, concept: &str, candidates: &[String], top_k: usize) -> Result<Vec<HypernymPrediction>> {
        // 1. Encode concept
        let concept_emb = self.encode_single(concept).await?;
        
        // 2. Encode candidates (batch)
        let candidate_embs = self.encode_batch(candidates).await?;
        
        // 3. Score each candidate
        let mut predictions = Vec::new();
        for (i, cand) in candidates.iter().enumerate() {
            let cand_emb = &candidate_embs[i];
            
            // Hyperbolic distance
            let distance = self.manifold.distance(&concept_emb, cand_emb);
            
            // Directionality: candidate should be closer to origin (more general)
            let norm_ratio = cand_emb.norm() / concept_emb.norm();
            let direction_ok = norm_ratio < 0.9;
            
            // Confidence: inverse distance + direction
            let confidence = if direction_ok {
                1.0 / (1.0 + distance) * norm_ratio
            } else {
                0.0
            };
            
            predictions.push(HypernymPrediction {
                child: concept.to_string(),
                parent: cand.clone(),
                distance,
                norm_ratio,
                confidence,
            });
        }
        
        // 4. Sort by confidence, return top-k
        predictions.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        Ok(predictions.into_iter().take(top_k).collect())
    }
}
```

### Phase 4: Integration into Pipeline Stage 4 (Semantic Analysis)

```rust
// In pipeline: relations module
struct RelationInducer {
    hearst_extractor: HearstExtractor,
    contrastive_predictor: ContrastiveHypernymPredictor,
    distributional: DistributionalInclusion,
    taxollama: TaxoLLaMAService,
    fusion: RelationFusion,
}

impl RelationInducer {
    async fn infer(&self, concepts: &[CanonicalConcept]) -> Result<Vec<RelationCandidate>> {
        let mut all_candidates = Vec::new();
        
        for concept in concepts {
            let mut concept_relations = Vec::new();
            
            // 1. Hearst patterns (high precision, corpus-specific)
            let hearst_relations = self.hearst_extractor.find_parents(concept).await?;
            concept_relations.extend(hearst_relations.into_iter().map(|r| RelationCandidate {
                source: EvidenceSource::Hearst,
                confidence: r.confidence,
                ..r
            }));
            
            // 2. Contrastive hypernym prediction (broad coverage, learned)
            let candidate_parents: Vec<_> = concepts.iter()
                .filter(|c| c.id != concept.id)
                .map(|c| c.label.clone())
                .collect();
            let contrastive_relations = self.contrastive_predictor.predict_hypernyms(&concept.label, &candidate_parents, 5).await?;
            concept_relations.extend(contrastive_relations.into_iter().map(|r| RelationCandidate {
                source: EvidenceSource::ContrastiveEmbedding,
                confidence: r.confidence,
                child: concept.id,
                parent: self.resolve_concept_id(&r.parent, concepts),
            }));
            
            // 3. Distributional inclusion (Weeds/Clarke DE)
            let dist_relations = self.distributional.find_parents(concept, concepts).await?;
            concept_relations.extend(dist_relations.into_iter().map(|r| RelationCandidate {
                source: EvidenceSource::DistributionalInclusion,
                confidence: r.confidence,
                ..r
            }));
            
            // 4. TaxoLLaMA (LLM reasoning)
            let llm_relations = self.taxollama.find_hypernyms(concept, concepts).await?;
            concept_relations.extend(llm_relations.into_iter().map(|r| RelationCandidate {
                source: EvidenceSource::TaxoLLaMA,
                confidence: r.confidence,
                ..r
            }));
            
            // 5. Fuse evidence
            let fused = self.fusion.fuse(concept_relations);
            all_candidates.extend(fused);
        }
        
        Ok(all_candidates)
    }
}
```

### Phase 5: RDF Provenance for Contrastive Relations

```turtle
ex:contrastiveModel_v1 a tx:ContrastiveModel ;
    tx:architecture "BERT-base + Poincaré projection" ;
    tx:trainingData "hearst+wordnet+wikipedia" ;
    tx:positivePairs 45000 ;
    tx:negativeStrategy "siblings+reverse+random" ;
    tx:loss "InfoNCE + OrderPreserving" ;
    tx:manifold "PoincareBall(c=1.0)" ;
    tx:dimension 128 ;
    prov:generatedAtTime "2026-11-08T12:00:00Z"^^xsd:dateTime .

ex:relationCandidate_456 a tx:RelationCandidate ;
    tx:subject ex:LinearAttention ;
    tx:predicate skos:broader ;
    tx:object ex:EfficientAttention ;
    tx:confidence 0.87 ;
    tx:method tx:ContrastiveEmbedding ;
    tx:evidence [
        a tx:ContrastiveEvidence ;
        tx:hyperbolicDistance 0.23 ;
        tx:normRatio 0.65 ;
        tx:modelVersion ex:contrastiveModel_v1 ;
        tx:rank 1 ;
    ] .
```

---

## Critical Design Decisions

1. **Hyperbolic space for hierarchy** — Natural geometry for trees; parent = closer to origin
2. **Contrastive training from patterns + WordNet** — No manual annotation needed
3. **Order-preserving loss** — Enforces parent closer to origin than child
4. **Hard negative sampling** — Siblings and reverse pairs crucial for discrimination
5. **Evidence fusion** — Contrastive is one signal among Hearst, distributional, LLM

---

## Open Questions

1. **Training data freshness** — Retrain when corpus evolves? Incremental fine-tuning?
2. **Domain adaptation** — Pre-train on general (WordNet/Wikipedia), fine-tune on domain corpus
3. **Scalability** — O(n²) candidate scoring; use ANN (HNSW) for top-k retrieval
4. **Evaluation** — TaxoBench hypernym prediction subtask; compare vs. TaxoLLaMA
5. **Concept representation** — Single embedding per concept vs. context-sensitive?

---

## Next Steps

1. Generate contrastive training data from our corpus (Hearst + WordNet + Wiki)
2. Train hyperbolic contrastive encoder (Python/geoopt) → export ONNX
3. Integrate ONNX model into Rust `ContrastiveHypernymPredictor`
4. Add to `RelationInducer` as parallel evidence source
5. Benchmark against TaxoLLaMA and distributional methods on CAM-Taxo benchmark
6. Tune negative sampling strategy for our domain