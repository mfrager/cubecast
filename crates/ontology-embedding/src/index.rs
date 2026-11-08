//! An in-process vector index.
//!
//! Records are kept in a map guarded by a read/write lock and searched
//! linearly, which is appropriate for single-node deployments and tests. A
//! dedicated vector backend can replace it through the `VectorIndex` trait.

use std::collections::BTreeMap;
use std::sync::RwLock;

use ontology_core::error::{Error, Result};
use ontology_core::id::Id;
use ontology_core::semantic::{
    SimilarityMetric, VectorFilter, VectorIndex, VectorMatch, VectorRecord,
};

/// In-memory vector index with cosine, dot, and Euclidean metrics.
#[derive(Default)]
pub struct InMemoryVectorIndex {
    records: RwLock<BTreeMap<Id, VectorRecord>>,
}

impl InMemoryVectorIndex {
    /// Creates an empty index.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of indexed records.
    ///
    /// # Errors
    ///
    /// Returns an error when the lock is poisoned.
    pub fn len(&self) -> Result<usize> {
        Ok(self.read()?.len())
    }

    /// Whether the index is empty.
    ///
    /// # Errors
    ///
    /// Returns an error when the lock is poisoned.
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }

    fn read(&self) -> Result<std::sync::RwLockReadGuard<'_, BTreeMap<Id, VectorRecord>>> {
        self.records
            .read()
            .map_err(|_| Error::validation("vector index lock is poisoned"))
    }

    fn write(&self) -> Result<std::sync::RwLockWriteGuard<'_, BTreeMap<Id, VectorRecord>>> {
        self.records
            .write()
            .map_err(|_| Error::validation("vector index lock is poisoned"))
    }
}

fn cosine(left: &[f32], right: &[f32]) -> f64 {
    let mut dot = 0.0_f64;
    let mut left_norm = 0.0_f64;
    let mut right_norm = 0.0_f64;
    for (a, b) in left.iter().zip(right) {
        dot += f64::from(*a) * f64::from(*b);
        left_norm += f64::from(*a) * f64::from(*a);
        right_norm += f64::from(*b) * f64::from(*b);
    }
    let denominator = left_norm.sqrt() * right_norm.sqrt();
    if denominator == 0.0 {
        0.0
    } else {
        dot / denominator
    }
}

fn dot(left: &[f32], right: &[f32]) -> f64 {
    left.iter()
        .zip(right)
        .map(|(a, b)| f64::from(*a) * f64::from(*b))
        .sum()
}

fn euclidean(left: &[f32], right: &[f32]) -> f64 {
    let sum = left
        .iter()
        .zip(right)
        .map(|(a, b)| {
            let difference = f64::from(*a) - f64::from(*b);
            difference * difference
        })
        .sum::<f64>();
    1.0 / (1.0 + sum.sqrt())
}

fn matches(record: &VectorRecord, filter: Option<&VectorFilter>) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    filter
        .instance
        .as_ref()
        .is_none_or(|instance| &record.instance == instance)
        && filter
            .attribute
            .as_ref()
            .is_none_or(|attribute| record.attribute.as_ref() == Some(attribute))
        && filter
            .model_id
            .as_ref()
            .is_none_or(|model| &record.model_id == model)
}

#[async_trait::async_trait]
impl VectorIndex for InMemoryVectorIndex {
    async fn upsert(&self, records: &[VectorRecord]) -> Result<()> {
        let mut stored = self.write()?;
        for record in records {
            stored.insert(record.id.clone(), record.clone());
        }
        Ok(())
    }

    async fn search(
        &self,
        vector: &[f32],
        limit: usize,
        filter: Option<VectorFilter>,
        metric: SimilarityMetric,
    ) -> Result<Vec<VectorMatch>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let stored = self.read()?;
        let mut matches: Vec<VectorMatch> = stored
            .values()
            .filter(|record| matches(record, filter.as_ref()))
            .filter(|record| record.vector.len() == vector.len())
            .map(|record| VectorMatch {
                id: record.id.clone(),
                instance: record.instance.clone(),
                attribute: record.attribute.clone(),
                model_id: record.model_id.clone(),
                score: match metric {
                    SimilarityMetric::Cosine => cosine(vector, &record.vector),
                    SimilarityMetric::Dot => dot(vector, &record.vector),
                    SimilarityMetric::Euclidean => euclidean(vector, &record.vector),
                },
            })
            .collect();
        matches.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.id.cmp(&right.id))
        });
        matches.truncate(limit);
        Ok(matches)
    }

    async fn delete(&self, ids: &[Id]) -> Result<usize> {
        let mut stored = self.write()?;
        let mut removed = 0;
        for id in ids {
            if stored.remove(id).is_some() {
                removed += 1;
            }
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn id(value: &str) -> Id {
        Id::new(format!("urn:example:{value}")).expect("test IRI")
    }

    fn record(value: &str, vector: Vec<f32>) -> VectorRecord {
        VectorRecord {
            id: id(value),
            instance: id(&format!("instance-{value}")),
            attribute: None,
            model_id: "test".to_owned(),
            vector,
            text_hash: String::new(),
            generated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn orders_cosine_hits_and_respects_limit() {
        let index = InMemoryVectorIndex::new();
        index
            .upsert(&[
                record("near", vec![1.0, 0.0]),
                record("middle", vec![0.9, 0.1]),
                record("far", vec![0.0, 1.0]),
            ])
            .await
            .expect("upsert");
        let hits = index
            .search(&[1.0, 0.0], 2, None, SimilarityMetric::Cosine)
            .await
            .expect("search");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, id("near"));
        assert_eq!(hits[1].id, id("middle"));
    }

    #[tokio::test]
    async fn filters_by_instance_attribute_and_model() {
        let index = InMemoryVectorIndex::new();
        let mut first = record("first", vec![1.0, 0.0]);
        first.attribute = Some(id("name"));
        let mut second = record("second", vec![1.0, 0.0]);
        second.attribute = Some(id("description"));
        second.model_id = "other".to_owned();
        index.upsert(&[first, second]).await.expect("upsert");

        let by_attribute = index
            .search(
                &[1.0, 0.0],
                10,
                Some(VectorFilter {
                    instance: None,
                    attribute: Some(id("name")),
                    model_id: None,
                }),
                SimilarityMetric::Cosine,
            )
            .await
            .expect("attribute filter");
        assert_eq!(by_attribute.len(), 1);
        assert_eq!(by_attribute[0].id, id("first"));

        let by_model = index
            .search(
                &[1.0, 0.0],
                10,
                Some(VectorFilter {
                    instance: None,
                    attribute: None,
                    model_id: Some("other".to_owned()),
                }),
                SimilarityMetric::Cosine,
            )
            .await
            .expect("model filter");
        assert_eq!(by_model.len(), 1);
        assert_eq!(by_model[0].id, id("second"));
    }

    #[tokio::test]
    async fn excludes_dimension_mismatches_and_zero_limits() {
        let index = InMemoryVectorIndex::new();
        index
            .upsert(&[record("two", vec![1.0, 0.0])])
            .await
            .expect("upsert");
        assert!(index
            .search(&[1.0, 0.0, 0.0], 10, None, SimilarityMetric::Cosine)
            .await
            .expect("mismatch search")
            .is_empty());
        assert!(index
            .search(&[1.0, 0.0], 0, None, SimilarityMetric::Cosine)
            .await
            .expect("zero limit")
            .is_empty());
    }

    #[tokio::test]
    async fn supports_dot_and_euclidean_metrics() {
        let index = InMemoryVectorIndex::new();
        index
            .upsert(&[record("one", vec![3.0, 4.0])])
            .await
            .expect("upsert");
        let dot = index
            .search(&[1.0, 2.0], 1, None, SimilarityMetric::Dot)
            .await
            .expect("dot");
        assert!((dot[0].score - 11.0).abs() < 1e-9);
        let euclidean = index
            .search(&[0.0, 0.0], 1, None, SimilarityMetric::Euclidean)
            .await
            .expect("euclidean");
        assert!((euclidean[0].score - 1.0 / 6.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn upsert_replaces_and_delete_counts() {
        let index = InMemoryVectorIndex::new();
        index
            .upsert(&[record("one", vec![1.0, 0.0])])
            .await
            .expect("upsert");
        index
            .upsert(&[record("one", vec![0.0, 1.0])])
            .await
            .expect("replace");
        assert_eq!(index.len().expect("len"), 1);
        let hits = index
            .search(&[0.0, 1.0], 1, None, SimilarityMetric::Cosine)
            .await
            .expect("search");
        assert!((hits[0].score - 1.0).abs() < 1e-9);
        assert_eq!(index.delete(&[id("one")]).await.expect("delete"), 1);
        assert_eq!(index.delete(&[id("one")]).await.expect("delete again"), 0);
        assert!(index.is_empty().expect("empty"));
    }
}
