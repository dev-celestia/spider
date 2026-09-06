//! Port of the reference crawler `pkg/utils/queue` — a variable-behavior queue supporting
//! FIFO (breadth-first) and LIFO (depth-first) strategies, plus a variable
//! queue (round-robin over per-item queues, used for per-input fairness).

use crate::types::options::Strategy;

/// Navigation item stored in the queue (reference crawler `queue.Variable`-style).
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub method: String,
    pub url: String,
    pub body: String,
    pub depth: i32,
    pub priority: i32,
    /// Seeds skip dequeue-time scope validation (reference crawler
    /// `navigation.Request.SkipValidation`).
    pub skip_validation: bool,
    /// Custom field extraction results carried with the request
    /// (reference crawler `navigation.Request.CustomFields`).
    pub custom_fields: std::collections::HashMap<String, Vec<String>>,
}

impl Item {
    pub fn get(&self) -> (&str, &str, &str, i32) {
        (&self.method, &self.url, &self.body, self.depth)
    }
}

/// A push/pop navigation structure honoring a [`Strategy`].
///
/// - `DepthFirst` pops from the back (LIFO stack)
/// - `BreadthFirst` pops the shallowest item first (min-heap on depth,
///   stable for equal depths — reference crawler `queue.priority_queue.go`)
#[derive(Debug)]
pub struct Queue {
    strategy: Strategy,
    data: Vec<Item>,
}

impl Queue {
    pub fn new(strategy: Strategy) -> Self {
        Queue { strategy, data: Vec::new() }
    }

    pub fn strategy(&self) -> Strategy {
        self.strategy
    }

    pub fn push(&mut self, item: Item) {
        self.data.push(item);
    }

    pub fn pop(&mut self) -> Option<Item> {
        match self.strategy {
            Strategy::BreadthFirst => {
                if self.data.is_empty() {
                    return None;
                }
                // Min-heap pop: lowest priority (depth) first; ties keep
                // insertion order (stable min by priority).
                let mut best = 0;
                for (i, item) in self.data.iter().enumerate().skip(1) {
                    if item.priority < self.data[best].priority {
                        best = i;
                    }
                }
                Some(self.data.remove(best))
            }
            Strategy::DepthFirst => self.data.pop(),
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Snapshot of pending items (used for resume state serialization).
    pub fn items(&self) -> &[Item] {
        &self.data
    }
}

/// A `Stack` (reference crawler `utils/queue/stack.go`) is a plain LIFO.
#[derive(Debug, Default)]
pub struct Stack<T> {
    data: Vec<T>,
}

impl<T> Stack<T> {
    pub fn new() -> Self {
        Stack { data: Vec::new() }
    }
    pub fn push(&mut self, item: T) {
        self.data.push(item);
    }
    pub fn pop(&mut self) -> Option<T> {
        self.data.pop()
    }
    pub fn len(&self) -> usize {
        self.data.len()
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Priority queue behavior flags (reference crawler `queue.priority_queue.go`): items with
/// equal priority keep insertion order within a breadth-first pass; priority
/// ranks are used to prefer lower-depth items.
pub fn priority_rank(depth: i32) -> i32 {
    depth
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(url: &str, depth: i32) -> Item {
        Item {
            method: "GET".into(),
            url: url.into(),
            body: String::new(),
            depth,
            priority: priority_rank(depth),
            skip_validation: false,
            custom_fields: Default::default(),
        }
    }

    #[test]
    fn test_breadth_first_order() {
        let mut q = Queue::new(Strategy::BreadthFirst);
        q.push(item("/a", 1));
        q.push(item("/b", 1));
        q.push(item("/c", 1));
        assert_eq!(q.pop().unwrap().url, "/a");
        assert_eq!(q.pop().unwrap().url, "/b");
        assert_eq!(q.pop().unwrap().url, "/c");
        assert!(q.pop().is_none());
    }

    #[test]
    fn test_breadth_first_prefers_shallower() {
        // Go's BFS is a min-heap on depth: a deeper item pushed first must
        // not pop before a shallower item discovered later.
        let mut q = Queue::new(Strategy::BreadthFirst);
        q.push(item("/deep", 2));
        q.push(item("/shallow", 1));
        assert_eq!(q.pop().unwrap().url, "/shallow");
        assert_eq!(q.pop().unwrap().url, "/deep");
    }

    #[test]
    fn test_depth_first_order() {
        let mut q = Queue::new(Strategy::DepthFirst);
        q.push(item("/a", 1));
        q.push(item("/b", 2));
        q.push(item("/c", 3));
        assert_eq!(q.pop().unwrap().url, "/c");
        assert_eq!(q.pop().unwrap().url, "/b");
        assert_eq!(q.pop().unwrap().url, "/a");
    }

    #[test]
    fn test_stack_lifo() {
        let mut s: Stack<i32> = Stack::new();
        s.push(1);
        s.push(2);
        assert_eq!(s.pop(), Some(2));
        assert_eq!(s.pop(), Some(1));
        assert!(s.is_empty());
    }
}
