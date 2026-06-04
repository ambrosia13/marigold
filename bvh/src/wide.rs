use std::mem::MaybeUninit;

use crate::{BoundingVolumeHierarchy, NODE_COST, OBJECT_COST};

type BinaryBvh = BoundingVolumeHierarchy<1, 1>;

pub struct WideBvhNode {}

enum WideSplitDecision {
    Leaf,
    Internal,
    Distribute(u32),
}

struct WideSplit {
    decision: WideSplitDecision,
    cost: f32,
}

struct WideBvhBuilder<'a> {
    binary_bvh: &'a BinaryBvh,
    splits: Vec<MaybeUninit<WideSplit>>,
}

impl<'a> WideBvhBuilder<'a> {
    pub fn sah_cost(&self, node_index: usize, max_trees: usize) -> f32 {
        if max_trees == 1 {
            f32::min(self.leaf_cost(node_index), self.internal_cost(node_index))
        } else {
            f32::min(
                self.distribute_cost(node_index, max_trees),
                self.sah_cost(node_index, max_trees - 1),
            )
        }
    }

    pub fn leaf_cost(&self, node_index: usize) -> f32 {
        let node = &self.binary_bvh.nodes()[node_index];

        let area = node.bounds.surface_area();
        let primitive_count = node.len;

        if primitive_count <= 8 {
            area * primitive_count as f32 * OBJECT_COST
        } else {
            f32::INFINITY
        }
    }

    pub fn internal_cost(&self, node_index: usize) -> f32 {
        let node = &self.binary_bvh.nodes()[node_index];
        let area = node.bounds.surface_area();

        self.distribute_cost(node_index, 8) + area * NODE_COST
    }

    pub fn distribute_cost(&self, node_index: usize, max_trees: usize) -> f32 {
        let node = &self.binary_bvh.nodes()[node_index];
        let left_index = node.child_node;
        let right_index = node.child_node + 1;

        (0..max_trees)
            .map(|k| {
                self.sah_cost(left_index as usize, k)
                    + self.sah_cost(right_index as usize, max_trees - k)
            })
            .reduce(|a, b| a.min(b))
            .unwrap()
    }
}
