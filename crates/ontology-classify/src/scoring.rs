//! Text normalization and similarity helpers used by the matching strategies.

use std::collections::BTreeSet;

/// Lowercases, strips punctuation, and collapses whitespace.
#[must_use]
pub(crate) fn normalize(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut pending_space = false;
    for character in text.chars() {
        if character.is_alphanumeric() {
            if pending_space && !output.is_empty() {
                output.push(' ');
            }
            pending_space = false;
            for lowercase in character.to_lowercase() {
                output.push(lowercase);
            }
        } else {
            pending_space = true;
        }
    }
    output
}

/// Splits normalized text into a token set.
pub(crate) fn tokens(text: &str) -> BTreeSet<String> {
    normalize(text)
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// Jaccard similarity of two token sets.
pub(crate) fn jaccard(left: &str, right: &str) -> f64 {
    let left = tokens(left);
    let right = tokens(right);
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let intersection = left.intersection(&right).count();
    let union = left.union(&right).count();
    intersection as f64 / union as f64
}

/// Normalized Levenshtein similarity in `0.0..=1.0`.
pub(crate) fn levenshtein_similarity(left: &str, right: &str) -> f64 {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    if left.is_empty() && right.is_empty() {
        return 1.0;
    }
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0_usize; right.len() + 1];
    for (row, left_char) in left.iter().enumerate() {
        current[0] = row + 1;
        for (column, right_char) in right.iter().enumerate() {
            let cost = usize::from(left_char != right_char);
            current[column + 1] = (previous[column + 1] + 1)
                .min(current[column] + 1)
                .min(previous[column] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    let distance = previous[right.len()] as f64;
    let longest = left.len().max(right.len()) as f64;
    1.0 - distance / longest
}

/// Cosine similarity of two vectors.
pub(crate) fn cosine(left: &[f32], right: &[f32]) -> f64 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_strips_punctuation_and_collapses_spaces() {
        assert_eq!(normalize("  Hello,   WORLD!  "), "hello world");
        assert_eq!(normalize("Café—au_lait"), "café au lait");
        assert_eq!(normalize("---"), "");
    }

    #[test]
    fn tokenization_deduplicates_normalized_words() {
        assert_eq!(
            tokens("One one TWO, two!"),
            BTreeSet::from(["one".to_owned(), "two".to_owned()])
        );
        assert!(tokens(" ... ").is_empty());
    }

    #[test]
    fn jaccard_handles_empty_identical_and_partial_sets() {
        assert_eq!(jaccard("red apple", "red apple"), 1.0);
        assert_eq!(jaccard("red apple", "red pear"), 1.0 / 3.0);
        assert_eq!(jaccard("red", "blue"), 0.0);
        assert_eq!(jaccard("", "red"), 0.0);
    }

    #[test]
    fn levenshtein_is_normalized_and_handles_empty_inputs() {
        assert_eq!(levenshtein_similarity("same", "same"), 1.0);
        assert_eq!(levenshtein_similarity("", ""), 1.0);
        let score = levenshtein_similarity("kitten", "sitting");
        assert!((score - (1.0 - 3.0 / 7.0)).abs() < 1e-9);
    }

    #[test]
    fn cosine_handles_orthogonal_parallel_and_zero_vectors() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-9);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-9);
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
    }
}
