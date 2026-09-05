//! Port of the reference crawler `pkg/utils/filters` — simple unique URL/content filter with
//! cycle detection, plus the path-trie (`pkg/utils/pathtrie.go`) and simhash
//! (`pkg/similarity`) based similarity filtering.

use std::collections::HashSet;
use std::sync::Mutex;

use md5::{Digest, Md5};

use crate::utils::similarity::SimHash;

/// celestia filter limits.
pub const MAX_CHROME_URL_LENGTH: usize = 2_097_152;
pub const MIN_SEQUENCE_LENGTH: usize = 10;
pub const MAX_SEQUENCE_COUNT: usize = 10;

/// Simple unique URL + content filter (reference crawler `filters.Simple`).
#[derive(Debug, Default)]
pub struct SimpleFilter {
    data: Mutex<HashSet<String>>,
}

impl SimpleFilter {
    pub fn new() -> Self {
        SimpleFilter::default()
    }

    /// Returns true if the URL is unique (reference crawler `UniqueURL`).
    pub fn unique_url(&self, url: &str) -> bool {
        let mut guard = self.data.lock().unwrap();
        guard.insert(url.to_string())
    }

    /// Returns true if the content hash is unique (reference crawler `UniqueContent`).
    pub fn unique_content(&self, data: &[u8]) -> bool {
        let mut hasher = Md5::new();
        hasher.update(data);
        let encoded = format!("{:x}", hasher.finalize());
        let mut guard = self.data.lock().unwrap();
        guard.insert(encoded)
    }

    /// Attempts to determine if the URL is a repetition cycle
    /// (reference crawler `IsCycle`): URL too long for Chrome, or containing a long
    /// repeating sequence.
    pub fn is_cycle(url: &str) -> bool {
        if url.len() > MAX_CHROME_URL_LENGTH {
            return true;
        }
        if let Some((seq, count)) = longest_repeating_sequence(url) {
            return count >= MAX_SEQUENCE_COUNT && seq.len() > MIN_SEQUENCE_LENGTH;
        }
        false
    }
}

/// Finds the longest repeating sequence and its count in a string
/// (port of `stringsutil.LongestRepeatingSequence`). Preference is given to
/// the highest repeat count, then the longer sequence.
pub fn longest_repeating_sequence(s: &str) -> Option<(String, usize)> {
    let n = s.len();
    let mut best: Option<(String, usize)> = None;

    for len in 1..=n / 2 {
        for start in 0..=(n - len * 2) {
            let seq = &s[start..start + len];
            let mut count = 1;
            let mut pos = start + len;
            while pos + len <= n && &s[pos..pos + len] == seq {
                count += 1;
                pos += len;
            }
            if count > 1 {
                let better = match &best {
                    None => true,
                    Some((bseq, bcount)) => {
                        count > *bcount || (count == *bcount && seq.len() > bseq.len())
                    }
                };
                if better {
                    best = Some((seq.to_string(), count));
                }
            }
        }
    }
    best
}

/// Trie of path segments used by `-filter-similar` to normalize variable path
/// segments (reference crawler `pkg/utils/pathtrie.go`): positions with >= `threshold`
/// distinct values at the same parent are treated as parameters (`*`).
#[derive(Debug, Default)]
pub struct PathTrie {
    root: TrieNode,
}

#[derive(Debug, Default)]
struct TrieNode {
    children: std::collections::HashMap<String, TrieNode>,
    terminal: bool,
}

impl PathTrie {
    pub fn new() -> Self {
        PathTrie::default()
    }

    /// Record a URL path in the trie.
    pub fn insert(&mut self, path: &str) {
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut node = &mut self.root;
        for seg in &segments {
            node = node.children.entry((*seg).to_string()).or_default();
        }
        node.terminal = true;
    }

    /// Normalize a path: walk the trie replacing any position whose parent has
    /// >= `threshold` children with `*`. Returns the normalized path plus the
    /// collapsed prefix when a variable position was crossed (e.g. `/users`
    /// for `/users/*`), so callers can learn variable positions globally
    /// (the reference crawler's "simplify" behavior).
    pub fn normalize(&mut self, path: &str, threshold: usize) -> (String, Option<String>) {
        // Ensure the path is registered first.
        self.insert(path);

        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut normalized = String::new();
        let mut collapsed_prefix: Option<String> = None;
        let mut node = &self.root;
        for seg in &segments {
            if node.children.len() >= threshold {
                normalized.push_str("/*");
                collapsed_prefix = Some(if normalized == "/*" {
                    "/".to_string()
                } else {
                    normalized.trim_end_matches("/*").to_string()
                });
                break;
            }
            match node.children.get(*seg) {
                Some(c) => {
                    normalized.push('/');
                    normalized.push_str(seg);
                    node = c;
                }
                None => {
                    normalized.push('/');
                    normalized.push_str(seg);
                    break;
                }
            }
        }
        if normalized.is_empty() {
            normalized.push('/');
        }
        (normalized, collapsed_prefix)
    }
}

/// Content similarity index (reference crawler `pkg/similarity`): tracks simhash values of
/// processed pages and reports near-duplicates within a Hamming distance.
#[derive(Debug, Default)]
pub struct SimilarityIndex {
    entries: Mutex<Vec<(String, u64)>>, // (url, simhash)
    distance: u32,
}

impl SimilarityIndex {
    pub fn new(distance: u32) -> Self {
        SimilarityIndex { entries: Mutex::new(Vec::new()), distance }
    }

    /// Returns true if content is similar to an already-seen page.
    pub fn is_similar(&self, url: &str, content: &str) -> bool {
        let hash = SimHash::hash(content);
        let mut entries = self.entries.lock().unwrap();
        for (seen_url, seen_hash) in entries.iter() {
            let d = SimHash::hamming_distance(hash, *seen_hash);
            if d <= self.distance {
                return true;
            }
            let _ = seen_url;
        }
        entries.push((url.to_string(), hash));
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unique_url_and_content() {
        let f = SimpleFilter::new();
        assert!(f.unique_url("https://a.com"));
        assert!(!f.unique_url("https://a.com"));
        assert!(f.unique_content(b"hello"));
        assert!(!f.unique_content(b"hello"));
    }

    #[test]
    fn test_is_cycle() {
        assert!(!SimpleFilter::is_cycle("https://example.com/page"));
        // Long URL
        let long = format!("https://example.com/{}", "a".repeat(2_097_200));
        assert!(SimpleFilter::is_cycle(&long));
        // Repeating sequence: >= 10 consecutive repeats of an 11-char unit.
        let repeated = format!("https://example.com/{}", "abcdefghij/".repeat(12));
        assert!(SimpleFilter::is_cycle(&repeated), "repeating should be a cycle");
        // Short repeating unit (len <= 10) is NOT a cycle per celestia rules.
        let short_unit = format!("https://example.com/{}", "ab/".repeat(15));
        assert!(!SimpleFilter::is_cycle(&short_unit));
    }

    #[test]
    fn test_longest_repeating_sequence() {
        let (seq, count) = longest_repeating_sequence("abcabcabcabc").unwrap();
        assert_eq!(count, 4);
        assert_eq!(seq, "abc");
        assert!(longest_repeating_sequence("abcdef").is_none());
    }

    #[test]
    fn test_similarity_index() {
        let idx = SimilarityIndex::new(3);
        assert!(!idx.is_similar("u1", "The quick brown fox jumps over the lazy dog"));
        assert!(idx.is_similar("u2", "The quick brown fox jumps over the lazy dog!"));
        assert!(!idx.is_similar("u3", "Completely different content about rust programming languages and compilers"));
    }

    #[test]
    fn test_path_trie_normalize() {
        let mut t = PathTrie::new();
        t.insert("/users/123");
        t.insert("/users/456");
        let (norm, collapsed) = t.normalize("/users/123", 10);
        assert_eq!(norm, "/users/123");
        assert_eq!(collapsed, None);

        // With threshold 2, /users has 2 distinct children so position 2 collapses.
        let mut t = PathTrie::new();
        t.insert("/users/123");
        let (norm, collapsed) = t.normalize("/users/456", 2);
        assert_eq!(norm, "/users/*");
        assert_eq!(collapsed.as_deref(), Some("/users"));
    }
}
