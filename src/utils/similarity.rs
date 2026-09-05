//! Port of the reference crawler `pkg/similarity` — SimHash content fingerprinting and
//! lexical similarity scoring (TF-IDF/BM25 modes approximated with token
//! Jaccard similarity over normalized text).

use std::collections::HashSet;

/// 64-bit SimHash of text content (reference crawler `simhash.Simhash`).
pub struct SimHash;

impl SimHash {
    /// Compute the simhash: word tokens are hashed (FNV-1a 64) and accumulated
    /// by sign into a 64-bit fingerprint.
    pub fn hash(text: &str) -> u64 {
        let tokens = normalize_tokens(text);
        let mut v = [0i64; 64];
        if tokens.is_empty() {
            return 0;
        }
        for token in &tokens {
            let h = fnv1a_64(token.as_bytes());
            for bit in 0..64 {
                if h & (1u64 << bit) != 0 {
                    v[bit] += 1;
                } else {
                    v[bit] -= 1;
                }
            }
        }
        let mut fingerprint = 0u64;
        for bit in 0..64 {
            if v[bit] > 0 {
                fingerprint |= 1u64 << bit;
            }
        }
        fingerprint
    }

    /// Hamming distance between two fingerprints.
    pub fn hamming_distance(a: u64, b: u64) -> u32 {
        (a ^ b).count_ones()
    }
}

/// Jaccard similarity between two texts over normalized token sets — used as
/// the native approximation for the reference crawler's tfidf/bm25 similarity modes.
pub fn jaccard_similarity(a: &str, b: &str) -> f64 {
    let ta: HashSet<String> = normalize_tokens(a).into_iter().collect();
    let tb: HashSet<String> = normalize_tokens(b).into_iter().collect();
    if ta.is_empty() && tb.is_empty() {
        return 1.0;
    }
    let inter = ta.intersection(&tb).count();
    let union = ta.union(&tb).count();
    if union == 0 {
        return 0.0;
    }
    inter as f64 / union as f64
}

/// Lowercase, strip non-alphanumerics, split into words, drop 1-char tokens.
fn normalize_tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 1)
        .map(|t| t.to_string())
        .collect()
}

/// FNV-1a 64-bit hash.
fn fnv1a_64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in data {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simhash_identical() {
        let a = "The quick brown fox jumps over the lazy dog";
        assert_eq!(SimHash::hash(a), SimHash::hash(a));
    }

    #[test]
    fn test_simhash_similar_close() {
        let a = "the quick brown fox jumps over the lazy dog again and again";
        let b = "the quick brown fox jumps over the lazy dog again";
        assert!(SimHash::hamming_distance(SimHash::hash(a), SimHash::hash(b)) <= 6);
    }

    #[test]
    fn test_simhash_different_far() {
        let a = "alpha beta gamma delta epsilon zeta eta theta iota kappa";
        let b = "rust programming language compiler memory safety garbage";
        assert!(SimHash::hamming_distance(SimHash::hash(a), SimHash::hash(b)) > 10);
    }

    #[test]
    fn test_jaccard() {
        let a = "hello world foo";
        let b = "hello world foo";
        assert!((jaccard_similarity(a, b) - 1.0).abs() < 1e-9);
        let c = "completely other words here";
        assert!(jaccard_similarity(a, c) < 0.3);
    }

    #[test]
    fn test_fnv() {
        assert_ne!(fnv1a_64(b"abc"), fnv1a_64(b"abd"));
        assert_eq!(fnv1a_64(b"abc"), fnv1a_64(b"abc"));
    }
}
