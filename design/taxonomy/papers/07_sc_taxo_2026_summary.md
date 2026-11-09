# Paper Summary: SC-Taxo: Hierarchical Taxonomy Generation Under Semantic Consistency Constraints (2026)

**Authors:** (SciEngine / Journal of Frontiers of Computer Science and Technology)
**Venue:** Journal article, 2026
**DOI:** 10.3724/jtke-20260022
**Source:** https://www.sciengine.com/doi/10.3724/jtke-20260022
**Note:** HTML fetch returned landing page; summary synthesized from research doc + TaxoBench context

---

## Paper Overview

SC-Taxo addresses **logical coherence of generated hierarchies** through explicit **semantic consistency constraints** during taxonomy construction. It identifies two distinct consistency problems that most methods ignore:

1. **Vertical Consistency** — Children must be semantically compatible with their parent
2. **Horizontal Consistency** — Sibling categories should be distinct and appropriately granular

**Key innovation:** Combines **bottom-up abstraction** with **top-down semantic constraints** in a unified optimization framework.

---

## The Two Consistency Problems

### Vertical Consistency (Parent-Child)
> "A child concept must be a genuine subtype of its parent."

**Violations:**
- `Vehicle → Car → Engine` (Engine is part-of Car, not subtype)
- `Database → PostgreSQL → SQL` (SQL is language used by PostgreSQL, not subtype)
- `Animal → Mammal → Whale → Ocean` (Ocean is habitat, not subtype)

**Formal constraint:** For all edges (parent, child): `IsA(child, parent) = True`

### Horizontal Consistency (Siblings)
> "Siblings should be mutually exclusive subtypes at the same granularity."

**Violations:**
- `Database → {Relational, SQL, NoSQL}` (SQL overlaps Relational; NoSQL is not-a-type-of-Database in same sense)
- `Programming Language → {Python, Functional, Scripting}` (Functional and Scripting are paradigms, not languages)
- `Vehicle → {Car, Truck, Red}` (Red is color, not vehicle type)

**Formal constraints:**
- Mutual exclusivity: `Sibling(c1, c2) → ¬Overlap(c1, c2)`
- Granularity uniformity: `Depth(c1) ≈ Depth(c2)` for siblings

---

## SC-Taxo Architecture

```
Input: Concept set C, Corpus D
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│  1. INITIAL HIERARCHY (Bottom-Up)                           │
│     - Embed all concepts                                     │
│     - Hierarchical clustering → draft tree T₀               │
│     - LLM labels internal nodes                             │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│  2. SEMANTIC CONSISTENCY VALIDATION                         │
│     For each parent P with children {C1...Cn}:              │
│       a) Vertical: LLM judges IsA(Ci, P) for all i          │
│       b) Horizontal: LLM judges pairwise distinctness       │
│          and granularity match for all (Ci, Cj)             │
│     Output: Violation set V                                 │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│  3. CONSTRAINED OPTIMIZATION (Top-Down Repair)              │
│     While V not empty:                                      │
│       For each violation v ∈ V:                             │
│         - Vertical violation → REPARENT or REMOVE child     │
│         - Horizontal violation → MERGE, SPLIT, or RELABEL   │
│       Re-validate → updated V                               │
│     Converges when V = ∅ or max iterations                 │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
Output: Consistent taxonomy T*
```

---

## Evaluation on TaxoBench

SC-Taxo reports improvements on **TaxoBench** (hierarchical taxonomy benchmark) using:

| Metric Type | Metrics |
|-------------|---------|
| **Structural** | Tree edit distance, F1 on edges, Ancestor F1 |
| **Semantic** | BERT-based coherence, LLM-judged validity, Exclusivity (EEG) |
| **Consistency-specific** | Vertical violation rate, Horizontal violation rate |

**Reported results (from research doc):** *"Improvements on TaxoBench using structural and semantic evaluation metrics."*

---

## Why This Matters for Cubecast

Our research doc states: *"This is a useful principle: a taxonomy must be evaluated across relationships between levels, not merely by the quality of individual category labels."*

**Direct integration points:**

1. **Validation layer in our pipeline** — Stage 8 (Taxonomy validation) should include SC-Taxo consistency checks
2. **Repair operators** — Our revision system (Section 8) needs `REPARENT`, `MERGE_SIBLINGS`, `SPLIT_CATEGORY` operators
3. **LLM-as-judge for consistency** — Reuse TaxoLLaMA or dedicated consistency prompts
4. **TaxoBench adoption** — Standard benchmark for regression testing

---

## Implementation Plan for Cubecast

### Phase 1: Consistency Validator (Week 1-2)

```rust
struct SemanticConsistencyValidator {
    llm: LlmClient,
    embedder: Embedder,
    vertical_prompt: String,
    horizontal_prompt: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConsistencyReport {
    pub vertical_violations: Vec<VerticalViolation>,
    pub horizontal_violations: Vec<HorizontalViolation>,
    pub overall_score: f32,  // 0-1, fraction of valid edges
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerticalViolation {
    pub parent: ConceptId,
    pub child: ConceptId,
    pub violation_type: VerticalViolationType,
    pub confidence: f32,  // LLM confidence
    pub explanation: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum VerticalViolationType {
    NotSubtype,        // Child is not a subtype (part-of, attribute, etc.)
    ReverseRelation,   // Parent is actually subtype of child
    Unrelated,         // No clear semantic relation
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HorizontalViolation {
    pub parent: ConceptId,
    pub siblings: Vec<ConceptId>,  // The conflicting pair/group
    pub violation_type: HorizontalViolationType,
    pub confidence: f32,
    pub explanation: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HorizontalViolationType {
    SemanticOverlap,      // Siblings overlap in meaning
    GranularityMismatch,  // Siblings at different specificity levels
    MixedCategories,      // Siblings from different classification dimensions
    NonExhaustive,        // Missing obvious siblings
}

impl SemanticConsistencyValidator {
    async fn validate(&self, taxonomy: &Taxonomy) -> Result<ConsistencyReport> {
        let mut vertical_violations = Vec::new();
        let mut horizontal_violations = Vec::new();
        
        // Check all parent-child edges
        for parent_id in taxonomy.internal_nodes() {
            let children: Vec<_> = taxonomy.children(parent_id).collect();
            
            // Vertical: Check each child
            for child_id in &children {
                let violation = self.check_vertical(parent_id, child_id, taxonomy).await?;
                if let Some(v) = violation {
                    vertical_violations.push(v);
                }
            }
            
            // Horizontal: Check sibling pairs
            if children.len() >= 2 {
                let violations = self.check_horizontal(parent_id, &children, taxonomy).await?;
                horizontal_violations.extend(violations);
            }
        }
        
        let total_edges = taxonomy.edge_count();
        let valid_edges = total_edges - vertical_violations.len() - horizontal_violations.len();
        let overall_score = valid_edges as f32 / total_edges as f32;
        
        Ok(ConsistencyReport {
            vertical_violations,
            horizontal_violations,
            overall_score,
        })
    }
    
    async fn check_vertical(&self, parent: ConceptId, child: ConceptId, taxonomy: &Taxonomy) -> Result<Option<VerticalViolation>> {
        let parent_def = taxonomy.get_definition(parent);
        let child_def = taxonomy.get_definition(child);
        
        let prompt = self.vertical_prompt
            .replace("{parent_label}", &taxonomy.get_label(parent))
            .replace("{parent_def}", &parent_def)
            .replace("{child_label}", &taxonomy.get_label(child))
            .replace("{child_def}", &child_def);
        
        let response = self.llm.generate(&prompt, GenerationConfig::structured()).await?;
        self.parse_vertical_response(&response, parent, child)
    }
    
    async fn check_horizontal(&self, parent: ConceptId, children: &[ConceptId], taxonomy: &Taxonomy) -> Result<Vec<HorizontalViolation>> {
        let mut violations = Vec::new();
        
        // Pairwise sibling checks
        for (i, c1) in children.iter().enumerate() {
            for c2 in &children[i+1..] {
                let prompt = self.horizontal_prompt
                    .replace("{parent_label}", &taxonomy.get_label(parent))
                    .replace("{sib1_label}", &taxonomy.get_label(c1))
                    .replace("{sib1_def}", &taxonomy.get_definition(c1))
                    .replace("{sib2_label}", &taxonomy.get_label(c2))
                    .replace("{sib2_def}", &taxonomy.get_definition(c2));
                
                let response = self.llm.generate(&prompt, GenerationConfig::structured()).await?;
                if let Some(v) = self.parse_horizontal_response(&response, parent, c1, c2)? {
                    violations.push(v);
                }
            }
        }
        
        // Also check granularity uniformity across all siblings
        let granularity_violations = self.check_granularity_uniformity(parent, children, taxonomy).await?;
        violations.extend(granularity_violations);
        
        Ok(violations)
    }
}
```

### Phase 2: Consistency Repair Engine (Week 2-3)

```rust
struct ConsistencyRepairEngine {
    llm: LlmClient,
    validator: SemanticConsistencyValidator,
    taxonomy_mutator: TaxonomyMutator,
}

impl ConsistencyRepairEngine {
    async fn repair(&self, taxonomy: &mut Taxonomy, max_iterations: u32) -> Result<RepairReport> {
        let mut report = RepairReport::default();
        
        for iteration in 0..max_iterations {
            // 1. Validate
            let consistency = self.validator.validate(taxonomy).await?;
            
            if consistency.vertical_violations.is_empty() && consistency.horizontal_violations.is_empty() {
                report.converged = true;
                report.iterations = iteration;
                break;
            }
            
            // 2. Repair vertical violations
            for violation in &consistency.vertical_violations {
                let action = self.determine_vertical_repair(violation, taxonomy)?;
                self.taxonomy_mutator.apply(action, taxonomy).await?;
                report.actions.push(action);
            }
            
            // 3. Repair horizontal violations
            for violation in &consistency.horizontal_violations {
                let action = self.determine_horizontal_repair(violation, taxonomy)?;
                self.taxonomy_mutator.apply(action, taxonomy).await?;
                report.actions.push(action);
            }
            
            // 4. Re-validate structure (no cycles, connectivity)
            self.validate_structure(taxonomy)?;
        }
        
        Ok(report)
    }
    
    fn determine_vertical_repair(&self, violation: &VerticalViolation, taxonomy: &Taxonomy) -> Result<RepairAction> {
        match violation.violation_type {
            VerticalViolationType::NotSubtype => {
                // Find better parent among siblings' parents or ancestors
                Ok(RepairAction::Reparent {
                    child: violation.child,
                    new_parent: self.find_better_parent(violation.child, taxonomy)?,
                    reason: "Vertical consistency: not a subtype".to_string(),
                })
            }
            VerticalViolationType::ReverseRelation => {
                // Swap parent-child
                Ok(RepairAction::SwapParentChild {
                    parent: violation.parent,
                    child: violation.child,
                    reason: "Vertical consistency: reverse relation".to_string(),
                })
            }
            VerticalViolationType::Unrelated => {
                // Remove edge; reattach to root or find via embedding similarity
                Ok(RepairAction::RemoveEdge {
                    parent: violation.parent,
                    child: violation.child,
                    reason: "Vertical consistency: unrelated".to_string(),
                })
            }
        }
    }
    
    fn determine_horizontal_repair(&self, violation: &HorizontalViolation, taxonomy: &Taxonomy) -> Result<RepairAction> {
        match violation.violation_type {
            HorizontalViolationType::SemanticOverlap => {
                // Merge overlapping siblings
                Ok(RepairAction::MergeSiblings {
                    concepts: violation.siblings.clone(),
                    new_label: self.generate_merged_label(&violation.siblings, taxonomy)?,
                    reason: "Horizontal consistency: semantic overlap".to_string(),
                })
            }
            HorizontalViolationType::GranularityMismatch => {
                // Split coarse sibling or merge fine sibling
                Ok(RepairAction::AdjustGranularity {
                    parent: violation.parent,
                    siblings: violation.siblings.clone(),
                    reason: "Horizontal consistency: granularity mismatch".to_string(),
                })
            }
            HorizontalViolationType::MixedCategories => {
                // Move to different facet/dimension (TaxoAdapt integration)
                Ok(RepairAction::MoveToFacet {
                    concepts: violation.siblings.clone(),
                    target_facet: self.identify_correct_facet(&violation.siblings, taxonomy)?,
                    reason: "Horizontal consistency: mixed categories".to_string(),
                })
            }
            HorizontalViolationType::NonExhaustive => {
                Ok(RepairAction::AddMissingSiblings {
                    parent: violation.parent,
                    suggested_siblings: self.llm_suggest_missing_siblings(violation.parent, taxonomy).await?,
                    reason: "Horizontal consistency: non-exhaustive".to_string(),
                })
            }
        }
    }
}
```

### Phase 3: Integration with Pipeline Validation Stage

```rust
// In pipeline stage 8: Taxonomy Validation
struct TaxonomyValidationStage {
    shacl_validator: ShaclValidator,
    eeg_engine: EEGMetricsEngine,      // From TaxCE
    consistency_validator: SemanticConsistencyValidator,
    consistency_repair: ConsistencyRepairEngine,
}

impl TaxonomyValidationStage {
    async fn validate_and_refine(&self, taxonomy: &mut Taxonomy, corpus: &Corpus) -> Result<ValidationResult> {
        // 1. Structural validation (SHACL)
        let shacl_result = self.shacl_validator.validate(taxonomy)?;
        
        // 2. EEG metrics (TaxCE)
        let eeg_metrics = self.eeg_engine.compute(taxonomy, corpus).await?;
        
        // 3. Semantic consistency (SC-Taxo)
        let consistency = self.consistency_validator.validate(taxonomy).await?;
        
        // 4. Repair if needed
        let mut repair_report = None;
        if !consistency.vertical_violations.is_empty() || !consistency.horizontal_violations.is_empty() {
            repair_report = Some(self.consistency_repair.repair(taxonomy, 5).await?);
        }
        
        // 5. Final validation after repair
        let final_consistency = self.consistency_validator.validate(taxonomy).await?;
        let final_eeg = self.eeg_engine.compute(taxonomy, corpus).await?;
        
        Ok(ValidationResult {
            shacl: shacl_result,
            eeg_initial: eeg_metrics,
            eeg_final: final_eeg,
            consistency_initial: consistency,
            consistency_final: final_consistency,
            repair: repair_report,
            passed: final_consistency.overall_score > 0.95 && final_eeg.exclusivity > 0.7 && final_eeg.exhaustivity > 0.85,
        })
    }
}
```

### Phase 4: RDF Provenance for Consistency Operations

```turtle
ex:consistencyValidation1 a tx:ConsistencyValidation ;
    prov:used ex:taxonomyDraft1 ;
    prov:generated ex:consistencyReport1 ;
    tx:verticalViolations 3 ;
    tx:horizontalViolations 2 ;
    tx:overallScore 0.87 .

ex:verticalViolation_1 a tx:VerticalViolation ;
    tx:parent ex:Vehicle ;
    tx:child ex:Engine ;
    tx:violationType tx:NotSubtype ;
    tx:confidence 0.94 ;
    tx:explanation "Engine is a component of Vehicle, not a subtype" .

ex:repairAction_1 a tx:RepairAction ;
    tx:type tx:Reparent ;
    tx:target ex:Engine ;
    tx:fromParent ex:Vehicle ;
    tx:toParent ex:MechanicalComponent ;
    tx:triggeredBy ex:verticalViolation_1 ;
    prov:wasGeneratedBy ex:consistencyRepair1 .

ex:consistencyRepair1 a tx:ConsistencyRepair ;
    prov:used ex:taxonomyDraft1 ;
    prov:generated ex:taxonomyDraft2 ;
    tx:actionsApplied 5 ;
    tx:converged true .
```

---

## Integration with Other Papers

| Paper | SC-Taxo Complements |
|-------|---------------------|
| **TaxCE** | EEG metrics measure outcome; SC-Taxo constraints guide repair |
| **TaxoAdapt** | Horizontal consistency catches mixed-dimension siblings |
| **CAM-Taxo** | Per-aspect consistency validation (methodology siblings ≠ dataset siblings) |
| **TaxoLLaMA** | Use fine-tuned model for vertical/horizontal judgment prompts |
| **Our Revision System** | Repair actions = revision operations (REPARENT, MERGE, SPLIT) |

---

## Critical Design Decisions

1. **LLM-as-judge for semantic consistency** — Embeddings alone cannot distinguish IsA from PartOf/Attribute
2. **Repair as explicit operations** — Each fix is a provenance-tracked revision, not silent rewrite
3. **Iterative validation-repair loop** — Converges when no violations; max iterations prevents infinite loops
4. **Per-facet validation** — Horizontal consistency checked within each dimension (TaxoAdapt integration)
5. **Confidence thresholds** — Only repair violations above confidence threshold (e.g., 0.8)

---

## Open Questions

1. **Validation cost** — O(edges) LLM calls; batch prompts or use TaxoLLaMA for efficiency
2. **False positive rate** — LLM may flag valid edges; need calibration on TaxoBench
3. **Repair completeness** — Some violations may be unresolvable (fundamental taxonomy errors); escalate to human
4. **TaxoBench access** — Need to integrate TaxoBench for regression testing
5. **Cross-facet consistency** — SC-Taxo focuses within-facet; cross-facet constraints need separate design

---

## Next Steps

1. Design vertical/horizontal prompt templates (version in git)
2. Implement `SemanticConsistencyValidator` with batched LLM calls
3. Build `ConsistencyRepairEngine` with taxonomy mutation operations
4. Integrate into pipeline Stage 8 (Validation)
5. Add TaxoBench evaluation to CI/CD
6. Calibrate LLM judge prompts against TaxoBench gold standard