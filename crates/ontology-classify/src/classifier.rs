//! Multi-strategy matching against SKOS concepts.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::Utc;
use ontology_core::error::{Error, Result};
use ontology_core::id::{Id, OperationId};
use ontology_core::model::Instance;
use ontology_core::semantic::{
    AttributeMatch, Classification, ClassificationBatch, ClassificationOptions, ClassificationRule,
    ClassificationStrategy, Classifier, EmbeddingProvider, Explanation, Rejection, ReviewStatus,
};
use ontology_core::skos::{Concept, TaxonomyManager, TaxonomyQuery};

use crate::scoring::{cosine, jaccard, levenshtein_similarity, normalize};

#[derive(Debug, Clone)]
struct Candidate {
    concept: Id,
    scheme: Option<Id>,
    labels: Vec<(String, Option<String>)>,
}

fn candidates(concepts: Vec<Concept>, language: Option<&str>) -> Vec<Candidate> {
    concepts
        .into_iter()
        .map(|concept| {
            let mut labels: Vec<(String, Option<String>)> = concept
                .pref_labels
                .iter()
                .chain(&concept.alt_labels)
                .chain(&concept.hidden_labels)
                .map(|label| (label.value.clone(), label.language.clone()))
                .chain(concept.notations.iter().map(|value| (value.clone(), None)))
                .collect();
            if let Some(language) = language {
                labels.retain(|(_, tag)| tag.as_deref().is_none_or(|tag| tag == language));
            }
            labels.sort();
            labels.dedup();
            Candidate {
                concept: concept.id,
                scheme: concept.scheme,
                labels,
            }
        })
        .collect()
}

fn label_score(
    strategy: ClassificationStrategy,
    texts: &[(Id, String)],
    labels: &[(String, Option<String>)],
) -> Option<(f64, AttributeMatch)> {
    let mut best: Option<(f64, AttributeMatch)> = None;
    for (attribute, value) in texts {
        for (label, _language) in labels {
            let score = match strategy {
                ClassificationStrategy::Exact => {
                    if value == label {
                        1.0
                    } else {
                        continue;
                    }
                }
                ClassificationStrategy::Normalized => {
                    if normalize(value) == normalize(label) {
                        0.95
                    } else {
                        continue;
                    }
                }
                ClassificationStrategy::Token => jaccard(value, label),
                ClassificationStrategy::Fuzzy => {
                    levenshtein_similarity(&normalize(value), &normalize(label))
                }
                _ => continue,
            };
            if score <= 0.0 {
                continue;
            }
            if best.as_ref().is_none_or(|(current, _)| score > *current) {
                best = Some((
                    score,
                    AttributeMatch {
                        attribute: attribute.clone(),
                        value: value.clone(),
                        label: label.clone(),
                    },
                ));
            }
        }
    }
    best
}

fn combine(
    scores: &BTreeMap<ClassificationStrategy, (f64, AttributeMatch)>,
) -> Option<(f64, AttributeMatch)> {
    let mut total = 0.0;
    let mut weight = 0.0;
    let mut best: Option<(f64, AttributeMatch)> = None;
    for (strategy, (score, matched)) in scores {
        let strategy_weight = strategy.weight();
        if strategy_weight <= 0.0 {
            continue;
        }
        total += score * strategy_weight;
        weight += strategy_weight;
        if best.as_ref().is_none_or(|(current, _)| *score > *current) {
            best = Some((*score, matched.clone()));
        }
    }
    if weight == 0.0 {
        None
    } else {
        Some((total / weight, best?.1))
    }
}

/// Multi-strategy classifier over the managed taxonomies.
pub struct MatchingClassifier {
    taxonomy: Arc<dyn TaxonomyManager>,
    provider: Option<Arc<dyn EmbeddingProvider>>,
    rules: Vec<ClassificationRule>,
}

impl MatchingClassifier {
    /// Creates a classifier without semantic matching or rules.
    #[must_use]
    pub fn new(taxonomy: Arc<dyn TaxonomyManager>) -> Self {
        Self {
            taxonomy,
            provider: None,
            rules: Vec::new(),
        }
    }

    /// Enables embedding-based semantic matching.
    #[must_use]
    pub fn with_embedding_provider(mut self, provider: Arc<dyn EmbeddingProvider>) -> Self {
        self.provider = Some(provider);
        self
    }

    /// Adds attribute rules.
    #[must_use]
    pub fn with_rules(mut self, rules: Vec<ClassificationRule>) -> Self {
        self.rules = rules;
        self
    }

    async fn semantic_scores(
        &self,
        candidates: &[Candidate],
        texts: &[(Id, String)],
    ) -> Result<BTreeMap<Id, (f64, AttributeMatch)>> {
        let Some(provider) = &self.provider else {
            return Ok(BTreeMap::new());
        };
        let mut inputs: Vec<String> = texts.iter().map(|(_, value)| value.clone()).collect();
        let mut offsets = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let start = inputs.len();
            inputs.extend(candidate.labels.iter().map(|(label, _)| label.clone()));
            offsets.push((start, inputs.len()));
        }
        let embeddings = provider.embed(&inputs).await?;
        if embeddings.len() != inputs.len() {
            return Err(Error::validation(
                "embedding provider returned a mismatched number of vectors",
            ));
        }
        let mut scores: BTreeMap<Id, (f64, AttributeMatch)> = BTreeMap::new();
        for (candidate, (start, end)) in candidates.iter().zip(offsets) {
            let mut best: Option<(f64, AttributeMatch)> = None;
            for label_index in start..end {
                let (label, _) = &candidate.labels[label_index - start];
                for (text_index, (attribute, value)) in texts.iter().enumerate() {
                    let score = cosine(
                        &embeddings[text_index].vector,
                        &embeddings[label_index].vector,
                    );
                    if best.as_ref().is_none_or(|(current, _)| score > *current) {
                        best = Some((
                            score,
                            AttributeMatch {
                                attribute: attribute.clone(),
                                value: value.clone(),
                                label: label.clone(),
                            },
                        ));
                    }
                }
            }
            if let Some(score) = best {
                scores.insert(candidate.concept.clone(), score);
            }
        }
        Ok(scores)
    }
}

fn rule_matches(rule: &ClassificationRule, value: &str) -> bool {
    let value = normalize(value);
    let expected = normalize(&rule.value);
    match rule.operator {
        ontology_core::semantic::RuleOperator::Equals => value == expected,
        ontology_core::semantic::RuleOperator::Contains => value.contains(&expected),
        ontology_core::semantic::RuleOperator::Prefix => value.starts_with(&expected),
    }
}

#[async_trait::async_trait]
impl Classifier for MatchingClassifier {
    async fn classify(
        &self,
        instance: &Instance,
        options: ClassificationOptions,
    ) -> Result<Vec<Classification>> {
        if options.max_results == 0 {
            return Ok(Vec::new());
        }
        let concepts = self
            .taxonomy
            .list_concepts(TaxonomyQuery {
                scheme: options.scheme.clone(),
                label_contains: None,
                limit: 100_000,
                offset: 0,
            })
            .await?;
        if concepts.is_empty() {
            return Ok(Vec::new());
        }
        let candidates = candidates(concepts, options.language.as_deref());
        let texts: Vec<(Id, String)> = instance
            .attribute_texts()
            .into_iter()
            .filter(|(attribute, _)| {
                options.attributes.is_empty() || options.attributes.contains(attribute)
            })
            .collect();
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut per_strategy: BTreeMap<
            Id,
            BTreeMap<ClassificationStrategy, (f64, AttributeMatch)>,
        > = BTreeMap::new();
        for candidate in &candidates {
            let mut scores = BTreeMap::new();
            for strategy in [
                ClassificationStrategy::Exact,
                ClassificationStrategy::Normalized,
                ClassificationStrategy::Token,
                ClassificationStrategy::Fuzzy,
            ] {
                if let Some(score) = label_score(strategy, &texts, &candidate.labels) {
                    scores.insert(strategy, score);
                }
            }
            per_strategy.insert(candidate.concept.clone(), scores);
        }

        for rule in &self.rules {
            if !per_strategy.contains_key(&rule.concept) {
                continue;
            }
            for (attribute, value) in &texts {
                if *attribute == rule.attribute && rule_matches(rule, value) {
                    per_strategy
                        .entry(rule.concept.clone())
                        .or_default()
                        .insert(
                            ClassificationStrategy::Rules,
                            (
                                rule.confidence,
                                AttributeMatch {
                                    attribute: attribute.clone(),
                                    value: value.clone(),
                                    label: rule.name.clone(),
                                },
                            ),
                        );
                    break;
                }
            }
        }

        for (concept, score) in self.semantic_scores(&candidates, &texts).await? {
            per_strategy
                .entry(concept)
                .or_default()
                .insert(ClassificationStrategy::Semantic, score);
        }

        let requested = if options.strategies.is_empty() {
            vec![ClassificationStrategy::Hybrid]
        } else {
            options.strategies.clone()
        };

        let mut scored: Vec<(Candidate, f64, ClassificationStrategy, AttributeMatch)> = Vec::new();
        for candidate in candidates {
            let scores = per_strategy.get(&candidate.concept);
            let mut best: Option<(f64, ClassificationStrategy, AttributeMatch)> = None;
            for strategy in &requested {
                let entry = if *strategy == ClassificationStrategy::Hybrid {
                    scores.and_then(combine)
                } else {
                    scores.and_then(|scores| scores.get(strategy).cloned())
                };
                let Some((score, matched)) = entry else {
                    continue;
                };
                if best.as_ref().is_none_or(|(current, _, _)| score > *current) {
                    best = Some((score, *strategy, matched));
                }
            }
            if let Some((score, strategy, matched)) = best {
                scored.push((candidate, score, strategy, matched));
            }
        }

        let mut rejected: Vec<Rejection> = scored
            .iter()
            .filter(|(_, score, _, _)| *score < options.threshold)
            .map(|(candidate, score, _, _)| Rejection {
                concept: candidate.concept.clone(),
                score: *score,
                reason: "score below threshold".to_owned(),
            })
            .collect();
        rejected.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.concept.cmp(&right.concept))
        });
        rejected.truncate(5);

        scored.retain(|(_, score, _, _)| *score >= options.threshold);
        scored.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.0.concept.cmp(&right.0.concept))
        });
        scored.truncate(options.max_results);

        let mut versions: BTreeMap<Id, Option<String>> = BTreeMap::new();
        for (candidate, _, _, _) in &scored {
            if let Some(scheme) = &candidate.scheme {
                if !versions.contains_key(scheme) {
                    let version = self
                        .taxonomy
                        .get_scheme(scheme)
                        .await
                        .ok()
                        .and_then(|scheme| scheme.version);
                    versions.insert(scheme.clone(), version);
                }
            }
        }

        let mut results = Vec::new();
        for (candidate, score, strategy, matched) in &scored {
            results.push(
                self.classification(
                    instance,
                    &candidate.concept,
                    candidate.scheme.clone(),
                    *score,
                    *strategy,
                    matched.clone(),
                    candidate
                        .scheme
                        .as_ref()
                        .and_then(|scheme| versions.get(scheme))
                        .cloned()
                        .flatten(),
                )?,
            );
        }

        if options.include_ancestors {
            let mut seen: Vec<Id> = results
                .iter()
                .map(|result| result.concept.clone())
                .collect();
            let mut expanded = results.clone();
            for result in &results {
                for ancestor in self.taxonomy.ancestors(&result.concept).await? {
                    if seen.contains(&ancestor.id) {
                        continue;
                    }
                    seen.push(ancestor.id.clone());
                    let mut classification = self.classification(
                        instance,
                        &ancestor.id,
                        ancestor.scheme.clone(),
                        result.confidence * 0.9,
                        result.strategy,
                        result
                            .matched_attributes
                            .first()
                            .cloned()
                            .unwrap_or(AttributeMatch {
                                attribute: instance.id.clone(),
                                value: String::new(),
                                label: String::new(),
                            }),
                        result.taxonomy_version.clone(),
                    )?;
                    classification.explanation.summary = format!(
                        "ancestor of {} ({})",
                        result.concept, result.explanation.summary
                    );
                    expanded.push(classification);
                }
            }
            results = expanded;
        }

        if !rejected.is_empty() {
            if let Some(first) = results.first_mut() {
                first.explanation.rejections = rejected.clone();
            }
        }
        Ok(results)
    }

    async fn classify_batch(
        &self,
        instances: &[Instance],
        options: ClassificationOptions,
    ) -> Result<ClassificationBatch> {
        let mut classifications = Vec::new();
        let mut rejected = Vec::new();
        for instance in instances {
            let mut results = self.classify(instance, options.clone()).await?;
            if let Some(first) = results.first_mut() {
                rejected.extend(first.explanation.rejections.clone());
            }
            classifications.extend(results);
        }
        Ok(ClassificationBatch {
            instance_count: instances.len(),
            classifications,
            rejected,
        })
    }
}

impl MatchingClassifier {
    fn classification(
        &self,
        instance: &Instance,
        concept: &Id,
        scheme: Option<Id>,
        confidence: f64,
        strategy: ClassificationStrategy,
        matched: AttributeMatch,
        taxonomy_version: Option<String>,
    ) -> Result<Classification> {
        let rule_name = self
            .rules
            .iter()
            .find(|rule| &rule.concept == concept)
            .map(|rule| rule.name.clone());
        let mut explanation = Explanation {
            summary: format!(
                "matched {:?} in attribute {} using {}",
                matched.value,
                matched.attribute,
                strategy.as_str()
            ),
            strategies: vec![strategy],
            matched: vec![matched.clone()],
            ..Explanation::default()
        };
        if let Some(rule_name) = rule_name {
            explanation.rules.push(rule_name);
        }
        Ok(Classification {
            id: Id::new(format!(
                "urn:onto:classification:{}",
                OperationId::generate()
            ))?,
            instance: instance.id.clone(),
            concept: concept.clone(),
            scheme,
            confidence,
            strategy,
            matched_attributes: vec![matched],
            taxonomy_version,
            manual: false,
            status: ReviewStatus::Pending,
            classified_at: Utc::now(),
            explanation,
        })
    }
}
