//! Port of the reference crawler `pkg/utils/filters` — simple unique URL/content filter with
//! cycle detection, plus the path-trie (`pkg/utils/pathtrie.go`) and simhash
//! (`pkg/similarity`) based similarity filtering.

use std::collections::HashSet;
use std::sync::Mutex;

use md5::{Digest, Md5};

use crate::utils::similarity::{jaccard_similarity, SimHash};

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

/// Adaptive per-host path trie used by `-filter-similar`
/// (reference crawler `pkg/utils/pathtrie.go` + `urlfingerprint.go`):
/// path positions with more than `threshold` distinct children are
/// *permanently promoted* to `{param}`.
#[derive(Debug, Default)]
pub struct PathTrie {
    hosts: std::collections::HashMap<String, std::sync::Arc<Mutex<TrieNode>>>,
    threshold: usize,
}

const PATH_TRIE_MAX_HOSTS: usize = 10_000;

#[derive(Debug, Default)]
struct TrieNode {
    children: std::collections::HashMap<String, std::sync::Arc<Mutex<TrieNode>>>,
    promoted: bool,
    param_child: Option<std::sync::Arc<Mutex<TrieNode>>>,
}

impl PathTrie {
    pub fn new(threshold: usize) -> Self {
        PathTrie { hosts: std::collections::HashMap::new(), threshold: threshold.max(1) }
    }

    fn host_root(&mut self, host: &str) -> std::sync::Arc<Mutex<TrieNode>> {
        if !self.hosts.contains_key(host) {
            if self.hosts.len() >= PATH_TRIE_MAX_HOSTS {
                // Simple cap: drop an arbitrary host when the bound is hit.
                if let Some(k) = self.hosts.keys().next().cloned() {
                    self.hosts.remove(&k);
                }
            }
            self.hosts.insert(host.to_string(), std::sync::Arc::default());
        }
        self.hosts.get(host).cloned().expect("just inserted")
    }

    /// Walk (and register) the segments in the trie for the given host,
    /// returning the segments with promoted positions replaced by `{param}`
    /// (reference crawler `PathTrie.Fingerprint`).
    pub fn fingerprint(&mut self, host: &str, segments: &[String]) -> Vec<String> {
        let root = self.host_root(host);
        let mut result = Vec::with_capacity(segments.len());
        let mut current = root;
        for seg in segments {
            let mut node = current.lock().unwrap();
            if node.promoted {
                let next = node
                    .param_child
                    .clone()
                    .unwrap_or_else(std::sync::Arc::default);
                drop(node);
                result.push("{param}".to_string());
                current = next;
                continue;
            }
            if !node.children.contains_key(seg) {
                node.children.insert(seg.clone(), std::sync::Arc::default());
                if node.children.len() > self.threshold {
                    let next: std::sync::Arc<Mutex<TrieNode>> = std::sync::Arc::default();
                    drop(node);
                    {
                        let mut n = current.lock().unwrap();
                        n.promoted = true;
                        n.param_child = Some(next.clone());
                        n.children.clear();
                    }
                    result.push("{param}".to_string());
                    current = next;
                    continue;
                }
            }
            let next = node.children.get(seg).cloned().expect("just inserted");
            drop(node);
            result.push(seg.clone());
            current = next;
        }
        result
    }
}

/// Layer-1 segment patterns, most specific first (reference crawler
/// `segmentPatterns`): uuid, sha256, sha1, md5, oid, hex, date, ts, num.
fn normalize_segment(segment: &str) -> Option<&'static str> {
    let contains_hex_letter = segment
        .bytes()
        .any(|b| matches!(b, b'a'..=b'f' | b'A'..=b'F'));
    let is_hex = |s: &str| s.bytes().all(|b| b.is_ascii_hexdigit());
    if segment.len() == 36
        && segment.as_bytes()[8] == b'-'
        && regex::Regex::new(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")
            .expect("uuid regex")
            .is_match(segment)
    {
        return Some("{uuid}");
    }
    if segment.len() == 64 && is_hex(segment) && contains_hex_letter {
        return Some("{sha256}");
    }
    if segment.len() == 40 && is_hex(segment) && contains_hex_letter {
        return Some("{sha1}");
    }
    if segment.len() == 32 && is_hex(segment) && contains_hex_letter {
        return Some("{md5}");
    }
    if segment.len() == 24 && is_hex(segment) && contains_hex_letter {
        return Some("{oid}");
    }
    if segment.len() >= 8 && is_hex(segment) && contains_hex_letter {
        return Some("{hex}");
    }
    if segment.len() == 10
        && segment.as_bytes()[4] == b'-'
        && regex::Regex::new(r"^\d{4}-\d{2}-\d{2}$").expect("date regex").is_match(segment)
    {
        return Some("{date}");
    }
    if regex::Regex::new(r"^\d{10}(\d{3})?$").expect("ts regex").is_match(segment) {
        return Some("{ts}");
    }
    if !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit()) {
        return Some("{num}");
    }
    None
}

/// Structural fingerprint of a URL (reference crawler `utils.FingerprintURL`):
/// layer-1 regex segment normalization, layer-2 adaptive per-host trie, and
/// layer-3 query keys (sorted, values dropped). Registers the path in the trie.
pub fn fingerprint_url(raw_url: &str, trie: &mut PathTrie) -> String {
    let Ok(u) = url::Url::parse(raw_url) else {
        return raw_url.to_string();
    };
    let path = u.path();
    let fingerprinted = if path.is_empty() || path == "/" {
        "/".to_string()
    } else {
        let trimmed = path.trim_matches('/');
        let mut segments: Vec<String> = if trimmed.is_empty() {
            Vec::new()
        } else {
            trimmed.split('/').map(|s| s.to_string()).collect()
        };
        // Layer 1: heuristic regex normalization.
        for seg in &mut segments {
            if let Some(placeholder) = normalize_segment(seg) {
                *seg = placeholder.to_string();
            }
        }
        // Layer 2: adaptive trie normalization.
        let segments = trie.fingerprint(u.host_str().unwrap_or(""), &segments);
        let mut p = format!("/{}", segments.join("/"));
        if path.ends_with('/') {
            p.push('/');
        }
        p
    };

    let mut out = String::new();
    if !u.scheme().is_empty() {
        out.push_str(u.scheme());
        out.push_str("://");
    }
    out.push_str(&match u.port() {
        Some(port) => format!("{}:{}", u.host_str().unwrap_or(""), port),
        None => u.host_str().unwrap_or("").to_string(),
    });
    out.push_str(&fingerprinted);
    if u.query().map(|q| !q.is_empty()).unwrap_or(false) {
        let mut keys: Vec<String> = u.query_pairs().map(|(k, _)| k.to_string()).collect();
        keys.sort();
        keys.dedup();
        if !keys.is_empty() {
            out.push('?');
            out.push_str(&keys.join("&"));
        }
    }
    out
}

/// Content similarity index (reference crawler `pkg/similarity`): tracks
/// fingerprints of processed pages and reports near-duplicates. SimHash mode
/// compares Hamming distance; tfidf/bm25 modes use the Jaccard approximation
/// with a score threshold and a per-cluster processing budget.
#[derive(Debug, Default)]
pub struct SimilarityIndex {
    entries: Mutex<Vec<SimilarityEntry>>,
    mode: crate::types::options::SimilarityMode,
    distance: u32,
    threshold: f64,
    budget: usize,
    /// Pages evaluated (reference crawler similarity Stats.processed).
    processed: std::sync::atomic::AtomicUsize,
    /// Pages passed through (Stats.accepted).
    accepted: std::sync::atomic::AtomicUsize,
    /// Pages dropped as similar (Stats.filtered).
    filtered: std::sync::atomic::AtomicUsize,
}

/// Aggregate similarity-filter counters (reference crawler `similarity.Stats`).
#[derive(Debug, Clone, Copy, Default)]
pub struct SimilarityStats {
    pub processed: usize,
    pub accepted: usize,
    pub filtered: usize,
}

#[derive(Debug, Default)]
struct SimilarityEntry {
    simhash: u64,
    body: String,
    processed: usize,
}

impl SimilarityIndex {
    pub fn new(
        mode: crate::types::options::SimilarityMode,
        distance: u32,
        threshold: f64,
        budget: usize,
    ) -> Self {
        SimilarityIndex {
            entries: Mutex::new(Vec::new()),
            mode,
            distance,
            threshold,
            budget: budget.max(1),
            processed: std::sync::atomic::AtomicUsize::new(0),
            accepted: std::sync::atomic::AtomicUsize::new(0),
            filtered: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Aggregate filter counters.
    pub fn stats(&self) -> SimilarityStats {
        use std::sync::atomic::Ordering;
        SimilarityStats {
            processed: self.processed.load(Ordering::SeqCst),
            accepted: self.accepted.load(Ordering::SeqCst),
            filtered: self.filtered.load(Ordering::SeqCst),
        }
    }

    /// Similarity mode name (reference crawler `ContentSimilarity.Mode()`).
    pub fn mode_name(&self) -> &'static str {
        match self.mode {
            crate::types::options::SimilarityMode::SimHash => "simhash",
            crate::types::options::SimilarityMode::TfIdf => "tfidf",
            crate::types::options::SimilarityMode::Bm25 => "bm25",
        }
    }

    /// Returns true if content is similar to an already-seen page.
    pub fn is_similar(&self, _url: &str, content: &str) -> bool {
        use std::sync::atomic::Ordering;
        self.processed.fetch_add(1, Ordering::SeqCst);
        let mut entries = self.entries.lock().unwrap();
        let similar = match self.mode {
            crate::types::options::SimilarityMode::SimHash => {
                let hash = SimHash::hash(content);
                entries
                    .iter()
                    .any(|entry| SimHash::hamming_distance(hash, entry.simhash) <= self.distance)
            }
            _ => {
                // tfidf/bm25 approximation: Jaccard token similarity against
                // each cluster representative, honoring the per-cluster budget
                // (pages fully processed per similarity cluster, `-pcsn`).
                for entry in entries.iter_mut() {
                    if jaccard_similarity(content, &entry.body) >= self.threshold {
                        if entry.processed < self.budget {
                            entry.processed += 1;
                            return false;
                        }
                        return true;
                    }
                }
                false
            }
        };
        if similar {
            self.filtered.fetch_add(1, Ordering::SeqCst);
            return true;
        }
        entries.push(SimilarityEntry {
            simhash: SimHash::hash(content),
            body: content.to_string(),
            processed: 1,
        });
        self.accepted.fetch_add(1, Ordering::SeqCst);
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
        let idx = SimilarityIndex::new(
            crate::types::options::SimilarityMode::SimHash,
            3,
            0.85,
            1,
        );
        assert!(!idx.is_similar("u1", "The quick brown fox jumps over the lazy dog"));
        assert!(idx.is_similar("u2", "The quick brown fox jumps over the lazy dog!"));
        assert!(!idx.is_similar("u3", "Completely different content about rust programming languages and compilers"));
    }

    #[test]
    fn test_similarity_index_jaccard_mode_budget() {
        // tfidf mode with budget 1: first similar page is processed once, the
        // next is skipped.
        let idx = SimilarityIndex::new(
            crate::types::options::SimilarityMode::TfIdf,
            3,
            0.7,
            1,
        );
        let a = "the quick brown fox jumps over the lazy dog";
        assert!(!idx.is_similar("u1", a));
        assert!(idx.is_similar("u2", a));
    }

    #[test]
    fn test_path_trie_promotion() {
        // Below threshold: segments are kept.
        let mut t = PathTrie::new(10);
        assert_eq!(t.fingerprint("h", &["users".into(), "123".into()]), vec!["users", "123"]);
        assert_eq!(t.fingerprint("h", &["users".into(), "456".into()]), vec!["users", "456"]);

        // Above threshold (more than 2 children at position 2): permanently promoted.
        let mut t = PathTrie::new(2);
        let _ = t.fingerprint("h", &["users".into(), "123".into()]);
        let _ = t.fingerprint("h", &["users".into(), "456".into()]);
        let out = t.fingerprint("h", &["users".into(), "789".into()]);
        assert_eq!(out, vec!["users", "{param}"]);
        // Promotion persists for subsequent walks.
        let out = t.fingerprint("h", &["users".into(), "abc".into()]);
        assert_eq!(out, vec!["users", "{param}"]);
    }

    #[test]
    fn test_normalize_segment() {
        assert_eq!(normalize_segment("550e8400-e29b-41d4-a716-446655440000"), Some("{uuid}"));
        assert_eq!(normalize_segment("2024-01-15"), Some("{date}"));
        assert_eq!(normalize_segment("1700000000"), Some("{ts}"));
        assert_eq!(normalize_segment("12345"), Some("{num}"));
        assert_eq!(normalize_segment("deadbeef"), Some("{hex}"));
        // Pure-digit segments >= 8 fall through hex to timestamp/numeric
        // (reference crawler containsHexLetter validation).
        assert_eq!(normalize_segment("12345678"), Some("{num}"));
        assert_eq!(normalize_segment("1700000000"), Some("{ts}"));
        assert_eq!(normalize_segment("17000000000000"), Some("{num}"));
        assert_eq!(normalize_segment("users"), None);
    }

    #[test]
    fn test_fingerprint_url() {
        let mut t = PathTrie::new(10);
        // Layer 1 + layer 3: variable segments normalized, only the sorted
        // query KEYS survive (values are dropped).
        assert_eq!(
            fingerprint_url("https://x.com/users/123?b=2&a=1", &mut t),
            "https://x.com/users/{num}?a&b"
        );
        // Duplicate keys collapse.
        assert_eq!(
            fingerprint_url("https://x.com/p?a=1&a=2", &mut t),
            "https://x.com/p?a"
        );
    }
}
