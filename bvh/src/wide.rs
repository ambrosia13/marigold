use std::mem::MaybeUninit;

use glam::Vec3;

use crate::{BoundingVolumeHierarchy, NODE_COST, OBJECT_COST};

type BinaryBvh = BoundingVolumeHierarchy<1, 1>;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WideBvhNode {
    origin: Vec3,
    extent: [u8; 3],
    imask: u8,
    min_x: [u8; 8],
    min_y: [u8; 8],
    min_z: [u8; 8],
    max_x: [u8; 8],
    max_y: [u8; 8],
    max_z: [u8; 8],
    child_base_index: u32,
    primitive_base_index: u32,
    meta: [u8; 8],
}

impl WideBvhNode {
    pub fn is_leaf(&self, child_index: usize) -> bool {
        // the low 5 bits of the meta field ranges from 0 to 23 for leaf nodes,
        // while the high 3 bits stores the number of triangles using unary encoding
        (self.meta[child_index] & 0b00011111) < 24
    }
}

pub struct WideBvh {
    nodes: Vec<WideBvhNode>,
}

impl WideBvh {
    pub fn new(binary_bvh: &BinaryBvh) -> Self {
        let mut decisions: Vec<MaybeUninit<WideSplit>> =
            Vec::with_capacity(binary_bvh.nodes().len());

        // there is one decision for each node in the binary bvh
        unsafe { decisions.set_len(binary_bvh.nodes().len()) };

        todo!()
    }

    /// Referenced from https://github.com/jan-van-bergen/GPU-Raytracer/blob/6559ae2241c8fdea0ddaec959fe1a47ec9b3ab0d/Src/BVH/Converters/BVH8Converter.cpp#L24
    /// to demystify the big picture of the construction algorithm that the paper describes
    fn calculate_decision(
        decisions: &mut [MaybeUninit<WideSplit>],
        node_index: usize,
        max_subtrees: usize,
        binary_bvh: &BinaryBvh,
    ) -> usize {
        let node = &binary_bvh.nodes()[node_index];

        if max_subtrees == 1 {
            // minimum between leaf and internal cost
            let split: WideSplit = if node.len <= 3 {
                // only consider leaf nodes if it covers fewer than 3 objects
                WideSplit {
                    decision_type: WideSplitDecisionType::Leaf,
                    cost: node.bounds.surface_area() * node.len as f32 * OBJECT_COST,
                }
            } else {
                let leaf_cost = WideSplit {
                    decision_type: WideSplitDecisionType::Leaf,
                    cost: node.bounds.surface_area() * node.len as f32 * OBJECT_COST,
                };

                // for the internal cost, we need the distribute cost
                todo!()
            };
        }

        todo!()

        // let primitive_count;

        // if node.child_node == 0 {
        //     // binary bvh leaf node, should have one object
        //     assert!(node.len == 1);

        //     let leaf_cost = node.bounds.surface_area(); // multiplied by node.len, which is one
        //     let decision = WideSplit {
        //         decision_type: WideSplitDecisionType::Leaf,
        //         cost: leaf_cost,
        //     };

        //     decisions[node_index] = MaybeUninit::new(decision);
        //     primitive_count = 1;
        // } else {
        //     // binary bvh internal node, process its children first
        //     let left_child_index = node.child_node as usize;
        //     let right_child_index = left_child_index + 1;

        //     primitive_count =
        //         Self::calculate_decision(decisions, left_child_index, max_subtrees, binary_bvh)
        //             + Self::calculate_decision(
        //                 decisions,
        //                 right_child_index,
        //                 max_subtrees,
        //                 binary_bvh,
        //             );

        //     // first, find the distribute cost
        // }

        // primitive_count
    }

    /// this is a nonstandard implementation, but since WideBvhNode has no padding and is
    /// 80 bytes, which is a multiple of 16, its std140, std430, and scalar layouts are the
    /// same, so we can use a simple cast with no copying
    pub fn as_gpu_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.nodes)
    }
}

#[derive(PartialEq, Eq)]
enum WideSplitDecisionType {
    Leaf,
    Internal,
    Distribute { left: u32, right: u32 },
}

#[derive(PartialEq)]
struct WideSplit {
    decision_type: WideSplitDecisionType,
    cost: f32,
}

impl Eq for WideSplit {}

impl PartialOrd for WideSplit {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WideSplit {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.cost.total_cmp(&other.cost)
    }
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
