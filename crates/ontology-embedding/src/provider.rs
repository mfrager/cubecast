//! A deterministic local embedding provider.
//!
//! Text is lowercased, split into alphanumeric tokens, and hashed into a fixed
//! number of dimensions with signed buckets (the hashing-trick baseline). It
//! requires no model files or network access and is reproducible across runs,
//! which makes it useful for tests and as a fallback provider.

use ontology_core::error::{Error, Result};
use ontology_core::semantic::{Embedding, EmbeddingProvider};

/// Hashing-trick embedding provider.
pub struct HashingEmbeddingProvider {
    dimensions: usize,
    model_id: String,
}

impl HashingEmbeddingProvider {
    /// Creates a provider with the given number of dimensions.
    ///
    /// # Panics
    ///
    /// Panics when `dimensions` is zero; use [`Self::try_new`] for fallible
    /// construction.
    #[must_use]
    pub fn new(dimensions: usize) -> Self {
        Self::try_new(dimensions).expect("embedding dimensions must be positive")
    }

    /// Creates a provider with the given number of dimensions.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] when `dimensions` is zero.
    pub fn try_new(dimensions: usize) -> Result<Self> {
        if dimensions == 0 {
            return Err(Error::validation("embedding dimensions must be positive"));
        }
        Ok(Self {
            dimensions,
            model_id: "hashing-v1".to_owned(),
        })
    }

    /// Overrides the model identifier.
    #[must_use]
    pub fn with_model_id(mut self, model_id: impl Into<String>) -> Self {
        self.model_id = model_id.into();
        self
    }
}

fn tokens(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
}

fn hash_token(token: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in token.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[async_trait::async_trait]
impl EmbeddingProvider for HashingEmbeddingProvider {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Embedding>> {
        Ok(texts
            .iter()
            .map(|text| {
                let mut vector = vec![0.0_f32; self.dimensions];
                for token in tokens(text) {
                    let hash = hash_token(&token);
                    let index = (hash as usize) % self.dimensions;
                    let sign = if hash & (1 << 63) == 0 { 1.0 } else { -1.0 };
                    vector[index] += sign;
                }
                let norm = vector
                    .iter()
                    .map(|value| f64::from(*value) * f64::from(*value))
                    .sum::<f64>()
                    .sqrt();
                if norm > 0.0 {
                    for value in &mut vector {
                        *value = (f64::from(*value) / norm) as f32;
                    }
                }
                Embedding::new(vector)
            })
            .collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ontology_core::error::Error;

    #[test]
    fn rejects_zero_dimensions() {
        assert!(matches!(
            HashingEmbeddingProvider::try_new(0),
            Err(Error::Validation { .. })
        ));
    }

    #[tokio::test]
    async fn produces_normalized_deterministic_vectors() {
        let provider = HashingEmbeddingProvider::new(32);
        let embeddings = provider
            .embed(&[
                "alpha beta".to_owned(),
                "beta, alpha".to_owned(),
                String::new(),
            ])
            .await
            .expect("embed");
        assert_eq!(embeddings.len(), 3);
        assert_eq!(embeddings[0].vector.len(), 32);
        assert_eq!(embeddings[0].vector, embeddings[1].vector);
        let norm = embeddings[0]
            .vector
            .iter()
            .map(|value| f64::from(*value) * f64::from(*value))
            .sum::<f64>()
            .sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
        assert!(embeddings[2].vector.iter().all(|value| *value == 0.0));
    }

    #[test]
    fn exposes_model_metadata() {
        let provider = HashingEmbeddingProvider::new(8).with_model_id("custom");
        assert_eq!(provider.dimensions(), 8);
        assert_eq!(provider.model_id(), "custom");
        assert_eq!(provider.model_version(), "1");
    }
}
