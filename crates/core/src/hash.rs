use image::DynamicImage;

pub const DEFAULT_MAX_DISTANCE: u32 = 8;

/// 64-bit difference hash. Robust to resize/recompress, not to crop/rotate.
pub fn dhash(img: &DynamicImage) -> i64 {
    let small = img
        .resize_exact(9, 8, image::imageops::FilterType::Triangle)
        .to_luma8();
    let mut hash: i64 = 0;
    for y in 0..8u32 {
        for x in 0..8u32 {
            let left = small.get_pixel(x, y)[0];
            let right = small.get_pixel(x + 1, y)[0];
            hash = (hash << 1) | i64::from(left > right);
        }
    }
    hash
}

pub fn hamming(a: i64, b: i64) -> u32 {
    (a ^ b).count_ones()
}

/// Maps a hamming distance to a 0..=1 similarity, 1.0 being an exact hash match.
pub fn similarity(distance: u32) -> f32 {
    1.0 - distance as f32 / 64.0
}

/// In-memory BK-tree over i64 hashes for fast "within N bits" lookup.
#[derive(Default)]
pub struct BkTree {
    nodes: Vec<Node>,
    len: usize,
}

struct Node {
    hash: i64,
    // Distinct media items can share a hash (e.g. a photo and a video frame of it).
    item_ids: Vec<i64>,
    children: Vec<(u32, usize)>,
}

impl BkTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn insert(&mut self, hash: i64, item_id: i64) {
        self.len += 1;
        if self.nodes.is_empty() {
            self.nodes.push(Node { hash, item_ids: vec![item_id], children: Vec::new() });
            return;
        }
        let mut cur = 0;
        loop {
            let d = hamming(self.nodes[cur].hash, hash);
            if d == 0 {
                self.nodes[cur].item_ids.push(item_id);
                return;
            }
            match self.nodes[cur].children.iter().find(|(dist, _)| *dist == d) {
                Some(&(_, child)) => cur = child,
                None => {
                    let new_idx = self.nodes.len();
                    self.nodes.push(Node { hash, item_ids: vec![item_id], children: Vec::new() });
                    self.nodes[cur].children.push((d, new_idx));
                    return;
                }
            }
        }
    }

    /// Returns `(item_id, distance)` for every item within `max_distance` bits of `query`.
    pub fn find(&self, query: i64, max_distance: u32) -> Vec<(i64, u32)> {
        let mut results = Vec::new();
        if self.nodes.is_empty() {
            return results;
        }
        // Iterative: a degenerate insertion order can make the tree arbitrarily deep.
        let mut stack = vec![0usize];
        while let Some(idx) = stack.pop() {
            let node = &self.nodes[idx];
            let d = hamming(node.hash, query);
            if d <= max_distance {
                results.extend(node.item_ids.iter().map(|&id| (id, d)));
            }
            let lo = d.saturating_sub(max_distance);
            let hi = d + max_distance;
            stack.extend(
                node.children
                    .iter()
                    .filter(|(child_dist, _)| (lo..=hi).contains(child_dist))
                    .map(|&(_, child)| child),
            );
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, GrayImage, Luma};

    fn gradient(width: u32, height: u32, invert: bool) -> DynamicImage {
        DynamicImage::ImageLuma8(GrayImage::from_fn(width, height, |x, _| {
            let v = (x * 255 / (width - 1)) as u8;
            Luma([if invert { 255 - v } else { v }])
        }))
    }

    #[test]
    fn dhash_of_known_gradients() {
        // Brightness decreasing left to right: every left > right comparison is true.
        assert_eq!(dhash(&gradient(90, 80, true)), -1);
        assert_eq!(dhash(&gradient(90, 80, false)), 0);
    }

    #[test]
    fn dhash_survives_resize() {
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(640, 480, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x * y) % 256) as u8])
        }));
        let small = img.resize(320, 240, image::imageops::FilterType::Lanczos3);
        assert!(hamming(dhash(&img), dhash(&small)) <= DEFAULT_MAX_DISTANCE);
    }

    #[test]
    fn hamming_distance() {
        assert_eq!(hamming(0, 0), 0);
        assert_eq!(hamming(0, -1), 64);
        assert_eq!(hamming(0b1011, 0b0001), 2);
        assert_eq!(similarity(0), 1.0);
        assert_eq!(similarity(64), 0.0);
    }

    #[test]
    fn bk_tree_matches_linear_scan() {
        let mut hashes = Vec::new();
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..2000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            hashes.push(state as i64);
        }
        let base = hashes[0];
        hashes.extend((0..64).map(|bit| base ^ (1i64 << bit)));

        let mut tree = BkTree::new();
        for (id, &h) in hashes.iter().enumerate() {
            tree.insert(h, id as i64);
        }
        assert_eq!(tree.len(), hashes.len());

        for max_distance in [0, 1, 8, 20] {
            let mut expected: Vec<(i64, u32)> = hashes
                .iter()
                .enumerate()
                .filter_map(|(id, &h)| {
                    let d = hamming(h, base);
                    (d <= max_distance).then_some((id as i64, d))
                })
                .collect();
            let mut found = tree.find(base, max_distance);
            expected.sort_unstable();
            found.sort_unstable();
            assert_eq!(found, expected, "max_distance = {max_distance}");
        }
    }

    #[test]
    fn bk_tree_keeps_items_sharing_a_hash() {
        let mut tree = BkTree::new();
        tree.insert(42, 1);
        tree.insert(42, 2);
        tree.insert(43, 3);
        let mut found = tree.find(42, 0);
        found.sort_unstable();
        assert_eq!(found, vec![(1, 0), (2, 0)]);
        assert!(BkTree::new().find(42, 64).is_empty());
    }
}
