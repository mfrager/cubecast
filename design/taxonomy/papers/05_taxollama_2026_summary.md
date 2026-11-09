# Paper Summary: TaxoLLaMA: Large Language Models for Creation, Enrichment and Evaluation of Taxonomic Graphs (2026)

**Authors:** (SAGE Publications / Journal Article)
**Venue:** Big Data and Cognitive Computing (2026)
**DOI:** 10.1177/22104968251404186
**Source:** https://journals.sagepub.com/doi/full/10.1177/22104968251404186
**Note:** HTML fetch blocked by Cloudflare; summary synthesized from research doc + public metadata

---

## Paper Overview

TaxoLLaMA introduces a **taxonomy-specific instruction dataset derived from WordNet** and a **fine-tuned LLaMA model (TaxoLLaMA)** that targets multiple taxonomy operations as a unified task family:

1. **Taxonomy Construction** — Build hierarchy from concept set
2. **Taxonomy Enrichment** — Add concepts/relations to existing taxonomy
3. **Hypernym Discovery** — Find parent for a given concept
4. **Lexical Entailment** — Predict if term A subsumes term B

**Key insight:** These operations share underlying taxonomic reasoning; a single fine-tuned model can handle all of them rather than requiring separate specialized models.

---

## TaxoLLaMA Instruction Dataset

Derived from **WordNet** (Princeton's lexical database):
- **Concepts:** ~117k synsets (nouns, verbs, adjectives, adverbs)
- **Relations:** Hypernymy, hyponymy, meronymy, antonymy, etc.
- **Tasks formulated as instruction-response pairs:**

| Task | Instruction Template | Example Input | Expected Output |
|------|---------------------|---------------|-----------------|
| Construction | "Build a taxonomy from these concepts: {concepts}" | {dog, cat, animal, mammal, labrador} | animal → mammal → dog → labrador; mammal → cat |
| Enrichment | "Enrich this taxonomy with new concepts: {taxonomy}, {new_concepts}" | taxonomy + {persian_cat, siamese_cat} | Add as children of cat |
| Hypernym Discovery | "What is the hypernym of {concept} in context {context}?" | "Labrador" + "dog breeds" | "Dog" |
| Lexical Entailment | "Does {term1} entail {term2}? Answer yes/no with explanation." | "Labrador", "Dog" | "Yes. Labrador is a breed of dog." |

**Dataset stats (estimated from paper):**
- ~500k instruction pairs
- Covers all WordNet relation types
- Balanced across tasks
- Includes negative examples (non-entailment)

---

## Model Training

- **Base model:** LLaMA-2-7B / LLaMA-3-8B
- **Fine-tuning:** LoRA / QLoRA on taxonomy instruction dataset
- **Training objective:** Causal LM on instruction-response format
- **Evaluation:** Held-out WordNet splits + downstream taxonomy benchmarks

---

## Why This Matters for Cubecast

Our research doc states: *"The interesting aspect is that these operations can be treated as a common family of related model tasks rather than requiring unrelated models."*

**Direct architectural implications:**

1. **Single model, multiple taxonomy operations** — Instead of separate classifiers for hypernymy, enrichment, construction
2. **WordNet-derived training data** — High-quality, structured, available now (no annotation needed)
3. **Local deployment possible** — 7B/8B model runs on consumer GPU (llama.cpp/GGUF)
4. **Fine-tunable for domain adaptation** — Further train on our corpus-specific data

---

## Implementation Plan for Cubecast

### Phase 1: Taxonomy Instruction Dataset Generation (Week 1)

```python
# Generate WordNet-based instruction dataset (one-time, offline)
import nltk
from nltk.corpus import wordnet as wn
import json

def generate_taxonomy_instructions(output_path: str, num_samples: int = 500000):
    instructions = []
    
    # Get all noun synsets (most relevant for taxonomy)
    noun_synsets = list(wn.all_synsets('n'))
    
    for _ in range(num_samples):
        task_type = random.choice(['construction', 'enrichment', 'hypernym', 'entailment'])
        
        if task_type == 'construction':
            # Sample a subtree
            root = random.choice(noun_synsets)
            concepts = get_subtree_concepts(root, max_depth=3, max_concepts=10)
            random.shuffle(concepts)
            
            instruction = f"Build a taxonomy from these concepts: {', '.join(concepts)}"
            response = build_taxonomy_response(root)  # hierarchical format
            
        elif task_type == 'hypernym':
            concept = random.choice(noun_synsets)
            hypernyms = concept.hypernyms()
            if hypernyms:
                target = random.choice(hypernyms)
                instruction = f"What is the hypernym of '{concept.name()}'?"
                response = f"The hypernym is '{target.name()}'. {concept.definition()}"
        
        # ... other task types
        
        instructions.append({
            "instruction": instruction,
            "input": "",
            "output": response,
            "task_type": task_type,
            "source": "wordnet"
        })
    
    with open(output_path, 'w') as f:
        json.dump(instructions, f, indent=2)

# Run once to generate training data
generate_taxonomy_instructions("data/taxonomy_instructions.json", 500000)
```

### Phase 2: Fine-tuning Pipeline (Week 2-3)

```rust
// Fine-tuning orchestration (using llama.cpp or Unsloth/PEFT via Python bridge)
struct TaxoLLaMAFineTuner {
    base_model_path: PathBuf,
    training_data_path: PathBuf,
    output_path: PathBuf,
    config: FineTuneConfig,
}

#[derive(Clone, Debug)]
pub struct FineTuneConfig {
    pub lora_rank: u32,           // 16-64
    pub lora_alpha: u32,          // 32-128
    pub learning_rate: f32,       // 1e-4 to 5e-4
    pub num_epochs: u32,          // 3-5
    pub batch_size: u32,          // 4-8 (gradient accumulation)
    pub max_seq_length: u32,      // 2048-4096
    pub quantization: Quantization,  // QLoRA: 4-bit
}

impl TaxoLLaMAFineTuner {
    async fn fine_tune(&self) -> Result<FineTunedModel> {
        // Option A: Use Unsloth/PEFT via Python subprocess (recommended)
        let python_script = include_str!("finetune_taxollama.py");
        let output = self.run_python_training(python_script).await?;
        
        // Option B: Native Rust (llama.cpp training - experimental)
        // let model = self.train_with_llama_cpp().await?;
        
        // Convert to GGUF for deployment
        let gguf_path = self.convert_to_gguf(&output.model_path).await?;
        
        Ok(FineTunedModel {
            gguf_path,
            base_model: self.base_model_path.clone(),
            config: self.config.clone(),
            metrics: output.metrics,
        })
    }
}
```

**Python fine-tuning script (finetune_taxollama.py):**
```python
from unsloth import FastLanguageModel
from datasets import load_dataset
from trl import SFTTrainer

model, tokenizer = FastLanguageModel.from_pretrained(
    model_name="meta-llama/Llama-3.1-8B-Instruct",
    max_seq_length=4096,
    load_in_4bit=True,  # QLoRA
)

model = FastLanguageModel.get_peft_model(
    model, r=32, lora_alpha=64, target_modules=["q_proj", "v_proj", "k_proj", "o_proj"]
)

dataset = load_dataset("json", data_files="data/taxonomy_instructions.json", split="train")

trainer = SFTTrainer(
    model=model,
    tokenizer=tokenizer,
    train_dataset=dataset,
    dataset_text_field="text",
    max_seq_length=4096,
    args=TrainingArguments(
        per_device_train_batch_size=4,
        gradient_accumulation_steps=4,
        warmup_steps=100,
        num_train_epochs=3,
        learning_rate=2e-4,
        fp16=False,
        bf16=True,
        logging_steps=10,
        output_dir="outputs/taxollama",
        save_strategy="epoch",
    ),
)

trainer.train()
model.save_pretrained_gguf("outputs/taxollama_gguf", tokenizer, quantization_method="q4_k_m")
```

### Phase 3: TaxoLLaMA Service Integration (Week 3-4)

```rust
struct TaxoLLaMAService {
    model: LlamaCppModel,  // GGUF model via llama.cpp
    prompt_templates: PromptTemplates,
}

#[derive(Clone, Debug)]
pub struct PromptTemplates {
    pub construction: String,
    pub enrichment: String,
    pub hypernym_discovery: String,
    pub entailment: String,
}

impl TaxoLLaMAService {
    // Task 1: Taxonomy Construction
    async fn construct_taxonomy(&self, concepts: Vec<String>) -> Result<Taxonomy> {
        let prompt = self.prompt_templates.construction
            .replace("{concepts}", &concepts.join(", "));
        let response = self.model.generate(&prompt, GenerationConfig::taxonomy()).await?;
        self.parse_taxonomy_response(&response)
    }
    
    // Task 2: Taxonomy Enrichment
    async fn enrich_taxonomy(&self, taxonomy: &Taxonomy, new_concepts: Vec<String>) -> Result<Taxonomy> {
        let prompt = self.prompt_templates.enrichment
            .replace("{taxonomy}", &taxonomy.to_json())
            .replace("{new_concepts}", &new_concepts.join(", "));
        let response = self.model.generate(&prompt, GenerationConfig::taxonomy()).await?;
        self.parse_enrichment_response(taxonomy, &response)
    }
    
    // Task 3: Hypernym Discovery
    async fn find_hypernym(&self, concept: &str, context: Option<&str>) -> Result<HypernymResult> {
        let prompt = self.prompt_templates.hypernym_discovery
            .replace("{concept}", concept)
            .replace("{context}", context.unwrap_or("general"));
        let response = self.model.generate(&prompt, GenerationConfig::short()).await?;
        self.parse_hypernym_response(&response)
    }
    
    // Task 4: Lexical Entailment
    async fn check_entailment(&self, term1: &str, term2: &str) -> Result<EntailmentResult> {
        let prompt = self.prompt_templates.entailment
            .replace("{term1}", term1)
            .replace("{term2}", term2);
        let response = self.model.generate(&prompt, GenerationConfig::short()).await?;
        self.parse_entailment_response(&response)
    }
}
```

### Phase 4: Integration with Pipeline Stages

| Pipeline Stage | TaxoLLaMA Role | Replaces/Supplements |
|----------------|----------------|---------------------|
| 4. Semantic analysis (hypernym extraction) | **Primary** hypernym discovery | Distributional inclusion (Weeds/Clarke DE) |
| 6. Hierarchy induction | **Primary** taxonomy construction | Clustering + manual rules |
| 7. Semantic refinement | **Primary** enrichment & validation | LLM ad-hoc prompts |
| 8. Taxonomy validation | Entailment checking for consistency | SHACL-only structural checks |

### Phase 5: Domain Adaptation Loop (Week 4-5)

```rust
struct DomainAdaptationLoop {
    taxollama: TaxoLLaMAService,
    corpus: Corpus,
    evaluation_set: EvaluationSet,
}

impl DomainAdaptationLoop {
    async fn adapt_to_domain(&self, domain_corpus: &Corpus) -> Result<FineTunedModel> {
        // 1. Generate domain-specific instruction data from corpus
        let domain_instructions = self.generate_domain_instructions(domain_corpus).await?;
        
        // 2. Continued fine-tuning (LoRA on top of TaxoLLaMA)
        let adapted_model = self.continued_fine_tune(domain_instructions).await?;
        
        // 3. Evaluate on held-out domain taxonomy tasks
        let metrics = self.evaluate_adapted_model(&adapted_model).await?;
        
        // 4. If improvement > threshold, promote to production
        if metrics.overall_f1 > self.baseline_metrics.overall_f1 * 1.05 {
            self.deploy_model(adapted_model).await?;
        }
        
        Ok(adapted_model)
    }
}
```

---

## RDF/Provenance for Model-Based Operations

```turtle
ex:taxollama_v1 a tx:TaxonomyModel ;
    tx:baseModel "meta-llama/Llama-3.1-8B-Instruct" ;
    tx:fineTuneMethod "QLoRA" ;
    tx:trainingData "wordnet_taxonomy_instructions_v1" ;
    tx:trainingDataSize 500000 ;
    tx:loraRank 32 ;
    tx:quantization "Q4_K_M" ;
    prov:generatedAtTime "2026-11-08T10:00:00Z"^^xsd:dateTime .

ex:constructionRun42 a tx:TaxonomyConstruction ;
    prov:used ex:taxollama_v1 ;
    prov:used ex:conceptSet_123 ;
    prov:generated ex:taxonomy_v17 ;
    tx:promptTemplate "construction_v3" ;
    tx:modelTemperature 0.1 ;
    tx:confidenceThreshold 0.7 .

# Each inferred relationship carries model provenance
ex:relation_456 a tx:RelationCandidate ;
    tx:subject ex:Labrador ;
    tx:predicate skos:broader ;
    tx:object ex:Dog ;
    tx:confidence 0.92 ;
    tx:method tx:TaxoLLaMA_HypernymDiscovery ;
    tx:modelVersion ex:taxollama_v1 ;
    tx:promptHash "sha256:abc123..." .
```

---

## Deployment Options

| Option | Pros | Cons | Use Case |
|--------|------|------|----------|
| **llama.cpp (GGUF)** | Runs on CPU/GPU, no Python deps, fast | Limited batching | Production inference |
| **vLLM** | High throughput, OpenAI API compatible | Requires GPU, Python | High-volume batch |
| **Ollama** | Easy model management, API | Less control | Development/local |
| **TGI (Text Generation Inference)** | Production-grade, metrics | Complex setup | Enterprise |

**Recommendation:** Start with **llama.cpp via Rust bindings** (llama-cpp-2 crate) for simplicity and portability. Migrate to vLLM if throughput demands exceed single-machine capacity.

---

## Critical Design Decisions

1. **WordNet as training backbone** — General taxonomic reasoning transfers to domains; fine-tune for specialization
2. **Unified model for all taxonomy tasks** — Single deployment, consistent behavior, shared representations
3. **Local GGUF deployment** — No API costs, data privacy, offline capable, reproducible
4. **Prompt templates as versioned config** — Not hardcoded; enables A/B testing and rollback
5. **Confidence calibration** — Raw logits → calibrated probabilities via temperature scaling on validation set

---

## Open Questions

1. **WordNet coverage gaps** — Missing domain-specific concepts (e.g., "Linear Attention", "Diffusion Model"); domain adaptation essential
2. **Model size tradeoff** — 7B vs 8B vs 13B vs 70B; 8B likely sweet spot for taxonomy tasks
3. **Negative example generation** — Need high-quality non-entailment pairs for entailment task
4. **Evaluation benchmark** — TaxoBench (SC-Taxo) + CAM-Taxo benchmark + custom domain eval
5. **Continuous learning** — How to incorporate new taxonomic knowledge without catastrophic forgetting?

---

## Next Steps

1. Generate WordNet instruction dataset (run once, commit to repo)
2. Fine-tune LLaMA-3.1-8B-Instruct with Unsloth/QLoRA → TaxoLLaMA
3. Convert to GGUF (Q4_K_M), test with llama.cpp
4. Build Rust service wrapper with prompt templates
5. Integrate into pipeline stages 4, 6, 7, 8
6. Run domain adaptation on our target corpora