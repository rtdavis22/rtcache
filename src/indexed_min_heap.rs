use std::collections::HashMap;

/// A node in the heap. `priority` is used to order nodes (smallest = highest priority).
#[derive(Debug)]
struct HeapNode<K, P> {
    key: K,
    priority: P,
}

/// An indexed min-heap. The smallest `priority` is at the "top".
#[derive(Debug)]
pub struct IndexedMinHeap<K, P> {
    /// The actual heap storage (array-based).
    nodes: Vec<HeapNode<K, P>>,
    /// Maps keys -> index in the `nodes` vector.
    indices: HashMap<K, usize>,
}

impl<K, P> IndexedMinHeap<K, P>
where
    K: Eq + std::hash::Hash + Clone,
    P: Ord + Clone,
{
    /// Creates an empty IndexedMinHeap.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            indices: HashMap::new(),
        }
    }

    /// Returns the number of items in the heap.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns `true` if the heap is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Inserts a new `(key, priority)` pair.  
    /// If the key already exists, this will overwrite its priority.
    pub fn insert(&mut self, key: K, priority: P) {
        if let Some(&idx) = self.indices.get(&key) {
            // Key already in heap, update its priority and bubble up/down.
            self.nodes[idx].priority = priority;
            self.bubble_up(idx);
            self.bubble_down(idx);
        } else {
            // New key
            let idx = self.nodes.len();
            self.nodes.push(HeapNode {
                key: key.clone(),
                priority,
            });
            self.indices.insert(key, idx);
            self.bubble_up(idx);
        }
    }

    /// Removes and returns the `(key, priority)` with the smallest priority.
    /// Returns `None` if empty.
    pub fn pop_min(&mut self) -> Option<(K, P)> {
        if self.nodes.is_empty() {
            return None;
        }
        // Swap the top with the last
        let last_idx = self.nodes.len() - 1;
        self.nodes.swap(0, last_idx);
        let min_node = self.nodes.pop().unwrap();
        // Remove from indices map
        self.indices.remove(&min_node.key);

        // Now bubble down the new root
        if !self.nodes.is_empty() {
            self.indices.insert(self.nodes[0].key.clone(), 0);
            self.bubble_down(0);
        }

        Some((min_node.key, min_node.priority))
    }

    /// Removes a specific key from the heap, returning its priority if it was present.
    pub fn remove(&mut self, key: &K) -> Option<P> {
        let &idx = self.indices.get(key)?;
        // Swap with last
        let last_idx = self.nodes.len() - 1;
        self.nodes.swap(idx, last_idx);
        let removed_node = self.nodes.pop().unwrap();
        self.indices.remove(&removed_node.key);

        // If we swapped in a new node at `idx`, bubble it up/down
        if idx < self.nodes.len() {
            self.indices.insert(self.nodes[idx].key.clone(), idx);
            self.bubble_up(idx);
            self.bubble_down(idx);
        }

        Some(removed_node.priority)
    }

    /// Updates the priority of a given key, if it exists.
    pub fn update_priority(&mut self, key: &K, new_priority: P) -> bool {
        if let Some(&idx) = self.indices.get(key) {
            self.nodes[idx].priority = new_priority;
            self.bubble_up(idx);
            self.bubble_down(idx);
            true
        } else {
            false
        }
    }

    /// Returns a reference to the minimum `(key, priority)` without removing it.
    pub fn peek_min(&self) -> Option<(&K, &P)> {
        self.nodes.get(0).map(|node| (&node.key, &node.priority))
    }

    // Helper: bubble up from `idx` if heap property is violated.
    fn bubble_up(&mut self, mut idx: usize) {
        while idx > 0 {
            let parent_idx = (idx - 1) / 2;
            if self.nodes[idx].priority < self.nodes[parent_idx].priority {
                // Swap
                self.nodes.swap(idx, parent_idx);
                // Update indices map
                self.indices.insert(self.nodes[idx].key.clone(), idx);
                self.indices
                    .insert(self.nodes[parent_idx].key.clone(), parent_idx);
                idx = parent_idx;
            } else {
                break;
            }
        }
    }

    // Helper: bubble down from `idx` if children have smaller priority.
    fn bubble_down(&mut self, mut idx: usize) {
        let len = self.nodes.len();
        loop {
            let left_child = 2 * idx + 1;
            let right_child = 2 * idx + 2;
            let mut smallest = idx;

            if left_child < len && self.nodes[left_child].priority < self.nodes[smallest].priority {
                smallest = left_child;
            }
            if right_child < len && self.nodes[right_child].priority < self.nodes[smallest].priority
            {
                smallest = right_child;
            }
            if smallest != idx {
                self.nodes.swap(idx, smallest);
                self.indices.insert(self.nodes[idx].key.clone(), idx);
                self.indices
                    .insert(self.nodes[smallest].key.clone(), smallest);
                idx = smallest;
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut heap = IndexedMinHeap::new();

        // Test empty heap
        assert!(heap.is_empty());
        assert_eq!(heap.len(), 0);
        assert_eq!(heap.peek_min(), None);
        assert_eq!(heap.pop_min(), None);

        // Test insertion
        heap.insert("a", 3);
        heap.insert("b", 1);
        heap.insert("c", 2);

        assert_eq!(heap.len(), 3);
        assert!(!heap.is_empty());

        // Test peek_min
        assert_eq!(heap.peek_min(), Some((&"b", &1)));

        // Test pop_min
        assert_eq!(heap.pop_min(), Some(("b", 1)));
        assert_eq!(heap.pop_min(), Some(("c", 2)));
        assert_eq!(heap.pop_min(), Some(("a", 3)));
        assert_eq!(heap.pop_min(), None);
    }

    #[test]
    fn test_update_priority() {
        let mut heap = IndexedMinHeap::new();

        heap.insert("a", 3);
        heap.insert("b", 2);
        heap.insert("c", 1);

        // Update to a higher priority (smaller number)
        assert!(heap.update_priority(&"b", 0));
        assert_eq!(heap.peek_min(), Some((&"b", &0)));

        // Update to a lower priority (larger number)
        assert!(heap.update_priority(&"c", 4));

        // Update non-existent key
        assert!(!heap.update_priority(&"d", 5));

        // Check final order
        assert_eq!(heap.pop_min(), Some(("b", 0)));
        assert_eq!(heap.pop_min(), Some(("a", 3)));
        assert_eq!(heap.pop_min(), Some(("c", 4)));
    }

    #[test]
    fn test_remove() {
        let mut heap = IndexedMinHeap::new();

        heap.insert("a", 3);
        heap.insert("b", 1);
        heap.insert("c", 2);

        // Remove middle element
        assert_eq!(heap.remove(&"c"), Some(2));
        assert_eq!(heap.len(), 2);

        // Remove non-existent element
        assert_eq!(heap.remove(&"d"), None);

        // Remove remaining elements
        assert_eq!(heap.remove(&"b"), Some(1));
        assert_eq!(heap.remove(&"a"), Some(3));
        assert!(heap.is_empty());
    }

    #[test]
    fn test_duplicate_keys() {
        let mut heap = IndexedMinHeap::new();

        // Insert initial value
        heap.insert("a", 3);
        assert_eq!(heap.len(), 1);

        // Update via insert
        heap.insert("a", 1);
        assert_eq!(heap.len(), 1);
        assert_eq!(heap.peek_min(), Some((&"a", &1)));

        // Update via update_priority
        heap.update_priority(&"a", 2);
        assert_eq!(heap.peek_min(), Some((&"a", &2)));
    }
}
