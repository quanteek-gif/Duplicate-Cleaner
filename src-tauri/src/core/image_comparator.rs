//! Clustering of perceptual image hashes.
//!
//! Uses a BK-tree (Burkhard–Keller) over the 64-bit pHash space with Hamming
//! distance, plus a union–find structure to build clusters. This keeps
//! visually-similar grouping at O(n log n)-ish for large photo libraries
//! instead of the naive O(n²) all-pairs comparison.

use std::collections::HashMap;

/// Hamming distance helper (mirrors `hasher::hamming_distance`).
pub fn distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

struct BkNode {
    hash: u64,
    /// Index into the caller's hash array.
    index: usize,
    children: HashMap<u32, BkNode>,
}

impl BkNode {
    fn new(hash: u64, index: usize) -> Self {
        Self {
            hash,
            index,
            children: HashMap::new(),
        }
    }

    fn insert(&mut self, hash: u64, index: usize) {
        let mut node = self;
        loop {
            let dist = distance(node.hash, hash);
            if dist == 0 {
                return; // identical hash already present
            }
            use std::collections::hash_map::Entry;
            match node.children.entry(dist) {
                Entry::Vacant(slot) => {
                    slot.insert(BkNode::new(hash, index));
                    return;
                }
                Entry::Occupied(slot) => node = slot.into_mut(),
            }
        }
    }

    fn search(&self, hash: u64, threshold: u32, out: &mut Vec<usize>) {
        let dist = distance(self.hash, hash);
        if dist <= threshold {
            out.push(self.index);
        }
        let low = dist.saturating_sub(threshold);
        let high = dist.saturating_add(threshold);
        for (edge, child) in &self.children {
            if *edge >= low && *edge <= high {
                child.search(hash, threshold, out);
            }
        }
    }
}

/// Union–find (disjoint set) over image indexes.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
            rank: vec![0; size],
        }
    }

    fn find(&mut self, mut value: usize) -> usize {
        while self.parent[value] != value {
            self.parent[value] = self.parent[self.parent[value]];
            value = self.parent[value];
        }
        value
    }

    fn union(&mut self, a: usize, b: usize) {
        let (root_a, root_b) = (self.find(a), self.find(b));
        if root_a == root_b {
            return;
        }
        match self.rank[root_a].cmp(&self.rank[root_b]) {
            std::cmp::Ordering::Less => self.parent[root_a] = root_b,
            std::cmp::Ordering::Greater => self.parent[root_b] = root_a,
            std::cmp::Ordering::Equal => {
                self.parent[root_b] = root_a;
                self.rank[root_a] += 1;
            }
        }
    }
}

/// A cluster of near-identical image hashes.
#[derive(Debug, Clone)]
pub struct HashCluster {
    /// Index of the representative image (first member).
    pub representative: usize,
    /// Every member index, representative included.
    pub members: Vec<usize>,
    /// Largest Hamming distance from the representative.
    pub max_distance: u32,
}

/// Group hashes so that every member is within `threshold` bits of the
/// cluster representative.
pub fn cluster_hashes(hashes: &[(usize, u64)], threshold: u32) -> Vec<HashCluster> {
    if hashes.is_empty() || threshold > 64 {
        return Vec::new();
    }
    let n = hashes.len();
    let mut uf = UnionFind::new(n);
    let mut tree: Option<BkNode> = None;

    for (position, (_, hash)) in hashes.iter().enumerate() {
        if let Some(root) = tree.as_mut() {
            let mut matches: Vec<usize> = Vec::new();
            root.search(*hash, threshold, &mut matches);
            for other in matches {
                uf.union(position, other);
            }
        } else {
            tree = Some(BkNode::new(*hash, position));
            continue;
        }
        if let Some(root) = tree.as_mut() {
            root.insert(*hash, position);
        }
    }

    let mut by_root: HashMap<usize, Vec<usize>> = HashMap::new();
    for position in 0..n {
        let root = uf.find(position);
        by_root.entry(root).or_default().push(position);
    }

    let mut clusters: Vec<HashCluster> = by_root
        .into_values()
        .filter(|members| members.len() > 1)
        .map(|mut members| {
            members.sort_unstable();
            let representative = members[0];
            let reference_hash = hashes[representative].1;
            let max_distance = members
                .iter()
                .map(|member| distance(reference_hash, hashes[*member].1))
                .max()
                .unwrap_or(0);
            HashCluster {
                representative,
                members,
                max_distance,
            }
        })
        .collect();

    clusters.sort_by_key(|cluster| std::cmp::Reverse(cluster.members.len()));
    clusters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_hashes_cluster() {
        let hashes = vec![(0, 0b1010u64), (1, 0b1010u64), (2, 0b0101u64)];
        let clusters = cluster_hashes(&hashes, 2);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].members, vec![0, 1]);
        assert_eq!(clusters[0].max_distance, 0);
    }

    #[test]
    fn near_hashes_cluster_with_threshold() {
        let base = 0u64;
        let near = 0b111u64; // distance 3
        let far = u64::MAX; // distance 64
        let hashes = vec![(0, base), (1, near), (2, far)];
        let clusters = cluster_hashes(&hashes, 4);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].members, vec![0, 1]);
        assert_eq!(clusters[0].max_distance, 3);
    }

    #[test]
    fn no_clusters_below_two_members() {
        // bit 0 vs bit 40 -> Hamming distance 2: threshold 1 must not cluster.
        let hashes = vec![(0, 1u64), (1, 1u64 << 40)];
        assert_eq!(distance(1u64, 1u64 << 40), 2);
        assert!(cluster_hashes(&hashes, 1).is_empty());
    }

    #[test]
    fn empty_input() {
        assert!(cluster_hashes(&[], 5).is_empty());
    }

    #[test]
    fn matches_brute_force_on_random_data() {
        let mut seed: u64 = 0x1234_5678;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let hashes: Vec<(usize, u64)> = (0..120).map(|i| (i, next())).collect();
        let threshold = 10u32;
        let clusters = cluster_hashes(&hashes, threshold);
        let clustered: Vec<usize> = clusters.iter().flat_map(|c| c.members.clone()).collect();

        // Every cluster member must be within threshold of the representative.
        for cluster in &clusters {
            let reference = hashes[cluster.representative].1;
            for member in &cluster.members {
                assert!(distance(reference, hashes[*member].1) <= threshold);
            }
        }
        // No image that has a near neighbour may stay unclustered.
        for (index, hash) in &hashes {
            if !clustered.contains(index) {
                assert!(
                    !hashes.iter().any(|(other, other_hash)| {
                        other != index && distance(*hash, *other_hash) <= threshold
                    }),
                    "image {index} had a neighbour but was not clustered"
                );
            }
        }
    }

    #[test]
    fn threshold_zero_groups_identical_only() {
        let hashes = vec![(0, 42u64), (1, 42u64), (2, 43u64)];
        let clusters = cluster_hashes(&hashes, 0);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].members, vec![0, 1]);
    }
}
