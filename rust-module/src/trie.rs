use std::{
    collections::{HashMap, VecDeque},
    convert::TryInto,
    mem,
};

/// Sentinel for CompactNode (23 bits)
const COMPACT_NONE: u32 = 0x007FFFFF;

/// A compact node representation (8 bytes).
/// Optimized for space and cache locality.
///
/// Layout:
/// - label_start (4 bytes)
/// - packed (4 bytes):
///   - first_child: 23 bits (8M nodes max)
///   - label_len: 7 bits (127 chars max)
///   - is_terminal: 1 bit
///   - has_next_sibling: 1 bit
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct CompactNode {
    pub label_start: u32,
    pub packed: u32,
}

impl CompactNode {
    pub fn first_child(&self) -> u32 {
        self.packed & 0x007FFFFF
    }

    pub fn label_len(&self) -> u16 {
        ((self.packed >> 23) & 0x7F) as u16
    }

    pub fn is_terminal(&self) -> bool {
        ((self.packed >> 30) & 1) != 0
    }

    pub fn has_next_sibling(&self) -> bool {
        ((self.packed >> 31) & 1) != 0
    }

    pub fn new(
        label_start: u32,
        first_child: u32,
        label_len: u16,
        is_terminal: bool,
        has_next_sibling: bool,
    ) -> Self {
        debug_assert!(first_child <= 0x007FFFFF, "first_child index too large");
        debug_assert!(label_len <= 127, "label_len too large");

        let packed = (first_child & 0x007FFFFF)
            | ((label_len as u32 & 0x7F) << 23)
            | ((is_terminal as u32) << 30)
            | ((has_next_sibling as u32) << 31);

        CompactNode {
            label_start,
            packed,
        }
    }
}
#[derive(Debug, Default)]
struct Node {
    // The string segment associated with the edge leading to this node
    prefix: String,
    // Use HashMap to index children by their first character
    children: HashMap<char, Node>,
    // Marks if a word ends at this exact node
    is_leaf: bool,
}

impl Node {
    fn new(prefix: String, is_leaf: bool) -> Self {
        Self {
            prefix,
            is_leaf,
            children: HashMap::new(),
        }
    }
}

#[derive(Debug, Default)]
pub struct TrieBuilder {
    root: Node,
}

impl TrieBuilder {
    pub fn new() -> Self {
        Self {
            root: Node::new(String::from(""), false),
        }
    }

    pub fn insert(&mut self, word: &str) {
        let mut current_node = &mut self.root;
        let mut remaining_key = word;

        while !remaining_key.is_empty() {
            // 1. Look for a child that starts with the first char of our remaining key
            let first_char = remaining_key.chars().next().unwrap();

            if current_node.children.contains_key(&first_char) {
                let child_node = current_node.children.get_mut(&first_char).unwrap();
                // Calculate longest common prefix (LCP) between remaining_key and child.prefix
                let common_len = Self::common_prefix_len(&child_node.prefix, remaining_key);

                // Case 2: Full Match - We traverse deeper
                // Example: Tree has "apple", Insert "applepie" (common: "apple")
                if common_len == child_node.prefix.len() {
                    remaining_key = &remaining_key[common_len..];
                    current_node = child_node;

                    // If we consumed the whole key, mark this node as a word end
                    if remaining_key.is_empty() {
                        current_node.is_leaf = true;
                    }
                }
                // Case 3: Partial Match - We need to split the existing edge
                // Example: Tree has "apple", Insert "apply" (common: "appl")
                else {
                    // 3a. Split the existing child node
                    let child_suffix = child_node.prefix[common_len..].to_string();
                    let input_suffix = remaining_key[common_len..].to_string();

                    // Truncate the current child's prefix to the common part (e.g., "apple" -> "appl")
                    child_node.prefix.truncate(common_len);

                    // Create a new node for the split part of the original child (e.g., "e")
                    // It inherits the children and leaf status of the original node
                    let mut split_node = Node::new(child_suffix, child_node.is_leaf);
                    split_node.children = std::mem::take(&mut child_node.children);

                    // The original node is no longer a leaf (unless the new word ends exactly here)
                    child_node.is_leaf = false;

                    // Re-attach the split part
                    let split_key = split_node.prefix.chars().next().unwrap();
                    child_node.children.insert(split_key, split_node);

                    // 3b. Insert the new word's remaining part (if any)
                    if !input_suffix.is_empty() {
                        let input_key = input_suffix.chars().next().unwrap();
                        child_node
                            .children
                            .insert(input_key, Node::new(input_suffix, true));
                    } else {
                        // The inserted word ended exactly at the split point
                        child_node.is_leaf = true;
                    }

                    return;
                }
            } else {
                // No matching edge. Create a new one with the rest of the key.
                current_node
                    .children
                    .insert(first_char, Node::new(remaining_key.to_string(), true));
                return;
            }
        }
    }

    /// Converts the pointer-based RadixTree into the flat, cache-friendly CompactRadixTrie.
    pub fn build(&self) -> (Vec<CompactNode>, Vec<u8>) {
        let mut nodes = Vec::new();
        let mut labels = Vec::<u8>::new();
        let mut queue = VecDeque::new();

        // 1. Process Root
        // The root usually has an empty label. We create it first.
        let root_label_len = self.root.prefix.len();
        if root_label_len > 127 {
            panic!("Label too long for compact node");
        }

        labels.extend_from_slice(self.root.prefix.as_bytes());

        nodes.push(CompactNode::new(
            0, // Root label starts at 0
            // Initialize with NO children. We will update this later if children exist.
            COMPACT_NONE,
            root_label_len as u16,
            self.root.is_leaf,
            false,
        ));

        // Queue tuple: (index_in_compact_vec, reference_to_original_node)
        queue.push_back((0, &self.root));

        // 2. BFS Traversal
        while let Some((parent_idx, source_node)) = queue.pop_front() {
            if source_node.children.is_empty() {
                continue;
            }

            // Get children and sort them to ensure deterministic sibling order
            // (Crucial for consistent linear iteration)
            let mut child_list: Vec<&Node> = source_node.children.values().collect();
            child_list.sort_by(|a, b| a.prefix.cmp(&b.prefix));

            // The children will be stored contiguously starting at this index
            let start_child_idx = nodes.len();

            // Safety check for the 23-bit child index limit (8 million nodes)
            if start_child_idx > 0x007FFFFF {
                panic!("Trie too large: > 8M nodes");
            }

            // 3. Update Parent's "first_child" pointer
            // We need to preserve the parent's existing flags/len, only updating the child index bits.
            let parent_packed = nodes[parent_idx].packed;
            // Clear the old child index (bottom 23 bits) and OR in the new index
            nodes[parent_idx].packed = (parent_packed & !0x007FFFFF) | (start_child_idx as u32);

            // 4. Process Children
            for (i, child) in child_list.iter().enumerate() {
                let label_len = child.prefix.len();
                if label_len > 127 {
                    panic!(
                        "Label '{}' too long (max 127 bytes in compact trie)",
                        child.prefix
                    );
                }

                // Add label to the main byte array
                let label_start = labels.len() as u32;
                labels.extend_from_slice(child.prefix.as_bytes());

                // Determine if this child has a subsequent sibling in the block
                let has_next_sibling = i < child_list.len() - 1;

                // Push the new compact node
                nodes.push(CompactNode::new(
                    label_start,
                    COMPACT_NONE, // Placeholder, will be updated when we process this node
                    label_len as u16,
                    child.is_leaf,
                    has_next_sibling,
                ));

                // Add to queue to process this child's children later
                queue.push_back((start_child_idx + i, child));
            }
        }

        compress_labels(&mut labels, &mut nodes);

        (nodes, labels)
    }

    // Helper to find length of common prefix
    fn common_prefix_len(s1: &str, s2: &str) -> usize {
        s1.bytes()
            .zip(s2.bytes())
            .take_while(|(a, b)| a == b)
            .count()
    }
}

/// An immutable, space-optimized Radix Trie.
/// Nodes are 8 bytes each (vs 12 bytes in Builder).
pub struct CompactRadixTrie<'a> {
    pub nodes: &'a [CompactNode],
    pub labels: &'a [u8],
}

impl<'a> CompactRadixTrie<'a> {
    pub fn new(nodes: &'a [CompactNode], labels: &'a [u8]) -> Self {
        Self { nodes, labels }
    }

    pub fn from_bytes(data: &'a [u8]) -> Self {
        let node_size = mem::size_of::<CompactNode>();
        let node_count = u32::from_le_bytes(data[0..4].try_into().unwrap());

        let nodes_start = 4;
        let nodes_end = nodes_start + (node_count as usize * node_size);
        let nodes_bytes = &data[nodes_start..nodes_end];

        let labels_count = u32::from_le_bytes(data[nodes_end..nodes_end + 4].try_into().unwrap());

        let labels_start = nodes_end + 4;
        let labels_end = labels_start + (labels_count as usize);

        let labels_bytes = &data[labels_start..labels_end];

        let nodes: &[CompactNode] = unsafe {
            std::slice::from_raw_parts(
                nodes_bytes.as_ptr() as *const CompactNode,
                nodes_bytes.len() / node_size,
            )
        };

        Self {
            nodes,
            labels: labels_bytes,
        }
    }

    fn get_label(&self, node_idx: u32) -> &[u8] {
        let node = &self.nodes[node_idx as usize];
        let start = node.label_start as usize;
        let end = start + node.label_len() as usize;
        &self.labels[start..end]
    }

    pub fn contains(&self, key: &str) -> bool {
        let key_bytes = key.as_bytes();
        let mut node_idx = 0;
        let mut key_cursor = 0;

        while key_cursor < key_bytes.len() {
            let mut child_idx = self.nodes[node_idx].first_child();

            if child_idx == COMPACT_NONE {
                return false;
            }

            let mut matched_child = false;

            // Iterate through sequential siblings
            loop {
                let child_label = self.get_label(child_idx);
                let current_key_part = &key_bytes[key_cursor..];

                if current_key_part.starts_with(child_label) {
                    key_cursor += child_label.len();
                    node_idx = child_idx as usize;
                    matched_child = true;
                    break;
                }

                if self.nodes[child_idx as usize].has_next_sibling() {
                    child_idx += 1;
                } else {
                    break;
                }
            }

            if !matched_child {
                return false;
            }
        }

        self.nodes[node_idx].is_terminal()
    }

    pub fn suggest(&self, prefix: &str, num_suggestions: usize) -> Vec<String> {
        let mut results = Vec::new();
        let prefix_bytes = prefix.as_bytes();
        let mut node_idx = 0;
        let mut key_cursor = 0;
        let mut buffer = vec![];

        while key_cursor < prefix_bytes.len() {
            let mut child_idx = self.nodes[node_idx].first_child();
            if child_idx == COMPACT_NONE {
                return results;
            }

            let mut found_child = false;

            loop {
                let child_label = self.get_label(child_idx);
                let current_key_part = &prefix_bytes[key_cursor..];
                let common_len = common_prefix_len(child_label, current_key_part);

                if common_len > 0 {
                    buffer.extend_from_slice(&child_label[..common_len]);

                    if common_len == current_key_part.len() {
                        let mut buffer = String::from_utf8(buffer).unwrap();
                        self.collect_suggestions(
                            child_idx,
                            common_len,
                            &mut buffer,
                            &mut results,
                            num_suggestions,
                        );
                        return results;
                    }

                    if common_len == child_label.len() {
                        key_cursor += common_len;
                        node_idx = child_idx as usize;
                        found_child = true;
                        break;
                    }

                    return results;
                }

                if self.nodes[child_idx as usize].has_next_sibling() {
                    child_idx += 1;
                } else {
                    break;
                }
            }

            if !found_child {
                return results;
            }
        }

        let mut buffer = String::from(prefix);
        if self.nodes[node_idx as usize].is_terminal() {
            results.push(buffer.clone());
        }

        let mut child = self.nodes[node_idx as usize].first_child();
        if child != COMPACT_NONE {
            loop {
                self.collect_suggestions(child, 0, &mut buffer, &mut results, num_suggestions);
                if results.len() >= num_suggestions {
                    return results;
                }
                if self.nodes[child as usize].has_next_sibling() {
                    child += 1;
                } else {
                    break;
                }
            }
        }

        results
    }

    pub fn collect_suggestions(
        &self,
        node_idx: u32,
        offset: usize,
        buffer: &mut String,
        results: &mut Vec<String>,
        num_suggestions: usize,
    ) {
        if results.len() >= num_suggestions {
            return;
        }

        let node = &self.nodes[node_idx as usize];
        let full_label = self.get_label(node_idx);
        let remainder = &full_label[offset..];
        let remainder_str = unsafe { std::str::from_utf8_unchecked(remainder) };
        let added_len = remainder_str.len();
        buffer.push_str(remainder_str);

        if node.is_terminal() {
            results.push(buffer.clone());
            if results.len() >= num_suggestions {
                buffer.truncate(buffer.len() - added_len);
                return;
            }
        }

        let mut child = node.first_child();
        if child != COMPACT_NONE {
            loop {
                self.collect_suggestions(child, 0, buffer, results, num_suggestions);
                if results.len() >= num_suggestions {
                    buffer.truncate(buffer.len() - added_len);
                    return;
                }
                if self.nodes[child as usize].has_next_sibling() {
                    child += 1;
                } else {
                    break;
                }
            }
        }

        buffer.truncate(buffer.len() - added_len);
    }

    pub fn size_in_bytes(&self) -> usize {
        (self.nodes.len() * mem::size_of::<CompactNode>()) + (self.labels.len())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::new();

        let node_count = self.nodes.len() as u32;
        data.extend_from_slice(&node_count.to_le_bytes());

        let nodes_bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                self.nodes.as_ptr() as *const u8,
                self.nodes.len() * mem::size_of::<CompactNode>(),
            )
        };
        data.extend_from_slice(nodes_bytes);

        let label_count = self.labels.len() as u32;
        data.extend_from_slice(&label_count.to_le_bytes());
        data.extend_from_slice(self.labels);

        data
    }
}

pub fn compress_labels(labels: &mut Vec<u8>, nodes: &mut Vec<CompactNode>) {
    let total_nodes = nodes.len();
    println!("Starting smart compression on {} nodes...", total_nodes);

    let mut string_to_id = std::collections::HashMap::new();
    let mut unique_strings = Vec::new();
    let mut node_to_unique_id = vec![0usize; total_nodes];

    for (i, node) in nodes.iter().enumerate() {
        let start = node.label_start as usize;
        let end = start + node.label_len() as usize;
        let s = String::from_utf8_lossy(&labels[start..end]).to_string();

        if let Some(&id) = string_to_id.get(&s) {
            node_to_unique_id[i] = id;
        } else {
            let id = unique_strings.len();
            string_to_id.insert(s.clone(), id);
            unique_strings.push(s);
            node_to_unique_id[i] = id;
        }
    }

    let num_uniques = unique_strings.len();
    println!("Reduced to {} unique strings. Continuing...", num_uniques);

    let mut redirects: Vec<(usize, u32)> = (0..num_uniques).map(|i| (i, 0)).collect();
    let mut is_active = vec![true; num_uniques];

    let mut sorted_indices: Vec<usize> = (0..num_uniques).collect();
    sorted_indices.sort_unstable_by(|&a, &b| unique_strings[a].cmp(&unique_strings[b]));

    for i in 0..num_uniques - 1 {
        let small_id = sorted_indices[i];
        let large_id = sorted_indices[i + 1];

        if unique_strings[large_id].starts_with(&unique_strings[small_id]) {
            redirects[small_id] = (large_id, 0);
            is_active[small_id] = false;
        }
    }

    let mut active_indices: Vec<usize> = (0..num_uniques).filter(|&i| is_active[i]).collect();
    active_indices.sort_unstable_by(|&a, &b| {
        unique_strings[a]
            .chars()
            .rev()
            .cmp(unique_strings[b].chars().rev())
    });

    for i in 0..active_indices.len() - 1 {
        let small_id = active_indices[i];
        let large_id = active_indices[i + 1];

        let s_small = &unique_strings[small_id];
        let s_large = &unique_strings[large_id];

        if s_large
            .chars()
            .rev()
            .zip(s_small.chars().rev())
            .all(|(a, b)| a == b)
        {
            let offset = (s_large.len() - s_small.len()) as u32;
            redirects[small_id] = (large_id, offset);
            is_active[small_id] = false;
        }
    }

    let mut final_resolution: Vec<(usize, u32)> = vec![(0, 0); num_uniques];

    for i in 0..num_uniques {
        let mut curr = i;
        let mut total_offset = 0;
        let mut depth = 0;

        while !is_active[curr] {
            let (next, off) = redirects[curr];
            if next == curr {
                break;
            }
            total_offset += off;
            curr = next;
            depth += 1;
            if depth > 1000 {
                break;
            }
        }
        final_resolution[i] = (curr, total_offset);
    }

    println!("Constructing super-buffer...");

    let mut super_buffer = Vec::new();
    let mut root_addresses = vec![0u32; num_uniques];

    for i in 0..num_uniques {
        if is_active[i] {
            let start_addr = super_buffer.len() as u32;
            super_buffer.extend_from_slice(unique_strings[i].as_bytes());
            root_addresses[i] = start_addr;
        }
    }

    println!("Updating pointers for {} nodes...", total_nodes);

    for (i, node) in nodes.iter_mut().enumerate() {
        let unique_id = node_to_unique_id[i];
        let (root_id, relative_offset) = final_resolution[unique_id];
        let absolute_base = root_addresses[root_id];
        node.label_start = absolute_base + relative_offset;
    }

    labels.clear();
    labels.append(&mut super_buffer);
    println!(
        "Smart compression complete. Final size: {} bytes.",
        labels.len()
    );
}

// Helper to find length of common prefix
fn common_prefix_len(s1: &[u8], s2: &[u8]) -> usize {
    s1.iter().zip(s2).take_while(|(a, b)| a == b).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_insertion_and_search() {
        let mut builder = TrieBuilder::new();
        builder.insert("apple");
        builder.insert("app");
        builder.insert("banana");
        builder.insert("bandana");

        let (nodes, labels) = builder.build();
        let trie = CompactRadixTrie::new(&nodes, &labels);

        assert!(trie.contains("apple"));
        assert!(trie.contains("app"));
        assert!(trie.contains("banana"));
        assert!(trie.contains("bandana"));

        assert!(!trie.contains("ban"));
        assert!(!trie.contains("apples"));
        assert!(!trie.contains("orange"));
    }

    #[test]
    fn test_split_logic() {
        let mut builder = TrieBuilder::new();
        builder.insert("test");
        builder.insert("team");

        let (nodes, labels) = builder.build();
        let trie = CompactRadixTrie::new(&nodes, &labels);

        assert!(trie.contains("test"));
        assert!(trie.contains("team"));
    }

    #[test]
    fn test_compact_node_memory_layout() {
        // Verify CompactNode is 8 bytes
        assert_eq!(std::mem::size_of::<CompactNode>(), 8);

        let node = CompactNode::new(100, 200, 50, true, true);
        assert_eq!(node.label_start, 100);
        assert_eq!(node.first_child(), 200);
        assert_eq!(node.label_len(), 50);
        assert_eq!(node.is_terminal(), true);
        assert_eq!(node.has_next_sibling(), true);
    }

    #[test]
    fn test_empty_trie() {
        let builder = TrieBuilder::new();
        let (nodes, labels) = builder.build();
        let trie = CompactRadixTrie::new(&nodes, &labels);

        assert!(!trie.contains(""));
        assert!(!trie.contains("anything"));

        let suggestions = trie.suggest("test", 10);
        assert_eq!(suggestions.len(), 0);
    }

    #[test]
    fn test_single_word() {
        let mut builder = TrieBuilder::new();
        builder.insert("hello");

        let (nodes, labels) = builder.build();
        let trie = CompactRadixTrie::new(&nodes, &labels);

        assert!(trie.contains("hello"));
        assert!(!trie.contains("hel"));
        assert!(!trie.contains("hello world"));
        assert!(!trie.contains(""));
    }

    #[test]
    fn test_prefix_words() {
        let mut builder = TrieBuilder::new();
        builder.insert("a");
        builder.insert("ab");
        builder.insert("abc");
        builder.insert("abcd");

        let (nodes, labels) = builder.build();
        let trie = CompactRadixTrie::new(&nodes, &labels);

        assert!(trie.contains("a"));
        assert!(trie.contains("ab"));
        assert!(trie.contains("abc"));
        assert!(trie.contains("abcd"));
        assert!(!trie.contains("abcde"));
    }
}
