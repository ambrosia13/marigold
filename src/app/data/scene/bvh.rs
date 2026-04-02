use glam::Vec3A;
use gpu_layout::{AsGpuBytes, GpuBytes};
use rayon::iter::{IntoParallelIterator, ParallelIterator};

pub trait AsBoundingVolume {
    fn bounding_volume(&self) -> BoundingVolume;

    fn center(&self) -> Vec3A {
        self.bounding_volume().center()
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct BoundingVolume {
    pub min: Vec3A,
    pub max: Vec3A,
    pub empty: bool,
}

impl AsGpuBytes for BoundingVolume {
    fn as_gpu_bytes<L: gpu_layout::GpuLayout + ?Sized>(&self) -> gpu_layout::GpuBytes<'_, L> {
        let mut buf = GpuBytes::empty();

        buf.write(&self.min.to_vec3()).write(&self.max.to_vec3());

        buf
    }
}

impl AsBoundingVolume for BoundingVolume {
    fn bounding_volume(&self) -> BoundingVolume {
        *self
    }
}

impl BoundingVolume {
    pub const EMPTY: Self = Self {
        min: Vec3A::ZERO,
        max: Vec3A::ZERO,
        empty: true,
    };

    pub fn new(min: Vec3A, max: Vec3A) -> Self {
        Self {
            min,
            max,
            empty: false,
        }
    }

    pub fn center(self) -> Vec3A {
        (self.min + self.max) * 0.5
    }

    pub fn extent(self) -> Vec3A {
        self.max - self.min
    }

    pub fn surface_area(self) -> f32 {
        let extent = self.extent();

        let width = extent.x;
        let height = extent.y;
        let depth = extent.z;

        2.0 * (width * height + width * depth + height * depth)
    }

    pub fn grow<T: AsBoundingVolume>(&mut self, object: &T) {
        let bounds = object.bounding_volume();
        self.grow_from_bounding_volume(bounds);
    }

    pub fn grow_from_bounding_volume(&mut self, bounds: BoundingVolume) {
        if !self.is_empty() {
            self.min = self.min.min(bounds.min);
            self.max = self.max.max(bounds.max);
        } else {
            *self = bounds;
        }
    }

    pub fn is_empty(self) -> bool {
        self.empty
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct BvhNode {
    pub bounds: BoundingVolume,
    pub start_index: u32,
    pub len: u32,
    pub child_node: u32,
}

impl AsGpuBytes for BvhNode {
    fn as_gpu_bytes<L: gpu_layout::GpuLayout + ?Sized>(&self) -> GpuBytes<'_, L> {
        let mut buf = GpuBytes::empty();

        buf.write(&self.bounds.min.to_vec3());
        buf.write(&self.start_index);
        buf.write(&self.bounds.max.to_vec3());

        if self.child_node == 0 {
            assert!(self.len < 128);
        }

        let packed = (self.len & 0b11111) << 25; // upper 7 bits
        let packed = packed | (self.child_node & ((1 << 25) - 1)); // lower 25 bits

        buf.write(&packed);

        buf
    }
}

impl BvhNode {
    pub const NODE_COST: f32 = 1.0;
    pub const OBJECT_COST: f32 = 8.0;
    pub const MIN_OBJECTS_PER_NODE: u32 = 1;

    pub fn root<T: AsBoundingVolume>(list: &mut [T]) -> Self {
        let mut bounds = BoundingVolume::new(Vec3A::ZERO, Vec3A::ZERO);

        for item in list.iter() {
            bounds.grow(item);
        }

        Self::root_with_bounds(list, bounds)
    }

    pub fn root_with_bounds<T: AsBoundingVolume>(list: &mut [T], bounds: BoundingVolume) -> Self {
        Self {
            // The root node's bounding volume encompasses all objects
            bounds,
            // The root node includes all objects in the list
            start_index: 0,
            len: list.len() as u32,
            // 0 represents no child nodes (yet)
            child_node: 0,
        }
    }

    pub fn slice<T>(self, list: &[T]) -> &[T] {
        let start = self.start_index as usize;
        let end = start + self.len as usize;
        &list[start..end]
    }

    fn cost(&self) -> f32 {
        Self::NODE_COST + Self::OBJECT_COST * self.bounds.surface_area() * self.len as f32
    }

    fn evaluate_split_cost<T: AsBoundingVolume>(list: &[T], axis: usize, threshold: f32) -> f32 {
        let mut bounds_a = BoundingVolume::EMPTY;
        let mut bounds_b = BoundingVolume::EMPTY;

        let mut a_count = 0;
        let mut b_count = 0;

        for obj in list {
            let obj_center = obj.center();

            if obj_center[axis] < threshold {
                bounds_a.grow(obj);
                a_count += 1;
            } else {
                bounds_b.grow(obj);
                b_count += 1;
            }
        }

        // discourage empty nodes or nodes with only one object (because it means the node slows us down)
        if a_count <= Self::MIN_OBJECTS_PER_NODE || b_count <= Self::MIN_OBJECTS_PER_NODE {
            //log::info!("Invalid split, axis: {}, threshold: {}")
            return f32::MAX;
        }

        let a_cost = bounds_a.surface_area() * a_count as f32 * Self::OBJECT_COST;
        let b_cost = bounds_b.surface_area() * b_count as f32 * Self::OBJECT_COST;

        Self::NODE_COST + a_cost + b_cost
    }

    // returns (cost, axis, threshold)
    fn choose_split_axis<T: AsBoundingVolume + Clone + Sync>(
        bounds: BoundingVolume,
        list: &[T],
    ) -> (f32, usize, f32) {
        // compute the results for all 3 axes in parallel, and then choose the best at the end
        let results_per_axis: Vec<_> = (0..3)
            .into_par_iter()
            .map(|axis| {
                // if there are fewer objects in the volume, take a more accurate search
                let (bounds_min, bounds_max) = if list.len() < 10 {
                    let mut min = f32::INFINITY;
                    let mut max = f32::NEG_INFINITY;

                    // find min and max positions of the objects along this axis
                    for object in list {
                        let object_bounds = object.bounding_volume();

                        if object_bounds.min[axis] < min {
                            min = object_bounds.min[axis];
                        }
                        if object_bounds.max[axis] > max {
                            max = object_bounds.max[axis];
                        }
                    }

                    (min, max)
                } else {
                    (bounds.min[axis], bounds.max[axis])
                };

                let step_count = list.len().clamp(5, 20);
                let bounds_step = (bounds_max - bounds_min) / step_count as f32;

                // Vec<(cost, threshold)>
                // compute all the results in parallel and then choose the best one at the end
                let results: Vec<(f32, f32)> = (0..step_count)
                    .into_par_iter()
                    .map(|i| {
                        let threshold = bounds_min + bounds_step * (i as f32 + 0.5);
                        let cost = Self::evaluate_split_cost(list, axis, threshold);

                        (cost, threshold)
                    })
                    .collect();

                let mut best_cost = f32::INFINITY;
                let mut best_threshold = 0.0;

                for (cost, threshold) in results {
                    if cost < best_cost {
                        best_cost = cost;
                        best_threshold = threshold;
                    }
                }

                (best_cost, axis, best_threshold)
            })
            .collect();

        let mut best_cost = f32::INFINITY;
        let mut best_axis = 0;
        let mut best_threshold = 0.0;

        for (cost, axis, threshold) in results_per_axis {
            if cost < best_cost {
                best_cost = cost;
                best_axis = axis;
                best_threshold = threshold;
            }
        }

        (best_cost, best_axis, best_threshold)
    }

    pub fn split<T: AsBoundingVolume + Clone + Sync>(
        &mut self,
        list: &mut [T],
        nodes: &mut Vec<Self>,
        depth: u32,
        max_depth: u32,
    ) {
        if depth == max_depth || self.len <= Self::MIN_OBJECTS_PER_NODE * 2 {
            return;
        }

        // the child containing objects greater than the split threshold
        let mut child_gt = Self {
            bounds: BoundingVolume::EMPTY,
            start_index: self.start_index,
            len: 0,
            child_node: 0,
        };

        // the child containing objects less than the split threshold
        let mut child_lt = Self {
            bounds: BoundingVolume::EMPTY,
            start_index: self.start_index,
            len: 0,
            child_node: 0,
        };

        let (cost, split_axis, split_threshold) =
            Self::choose_split_axis(self.bounds, self.slice(list));

        // don't split if the cost of the split would be greater than the current cost
        if cost >= self.cost() {
            return;
        }

        let greater = |object: &T| object.center()[split_axis] > split_threshold;

        for global_index in self.start_index..(self.start_index + self.len) {
            let global_index = global_index as usize;
            let object = &list[global_index];

            if greater(object) {
                child_gt.bounds.grow(object);
                child_gt.len += 1;

                let swap_index = (child_gt.start_index + child_gt.len) as usize - 1;
                list.swap(swap_index, global_index);
                child_lt.start_index += 1;
            } else {
                child_lt.bounds.grow(object);
                child_lt.len += 1;
            }
        }

        if child_gt.len > 0 && child_lt.len > 0 {
            self.child_node = nodes.len() as u32;
            nodes.push(child_gt);
            nodes.push(child_lt);

            // split the children of this node
            child_gt.split(list, nodes, depth + 1, max_depth);
            child_lt.split(list, nodes, depth + 1, max_depth);

            nodes[self.child_node as usize] = child_gt;
            nodes[self.child_node as usize + 1] = child_lt;
        }
    }
}

pub struct BoundingVolumeHierarchy {
    nodes: Vec<BvhNode>,
}

impl BoundingVolumeHierarchy {
    pub fn new<T: AsBoundingVolume + Clone + Sync>(
        list: &mut [T],
        bounds: Option<BoundingVolume>,
        max_depth: u32,
    ) -> Self {
        if list.is_empty() {
            return Self { nodes: Vec::new() };
        }

        let instant = std::time::Instant::now();

        // let max_depth = 32; //f32::log2(list.len() as f32) as u32 + 6;

        // create the root node
        let mut root = if let Some(bounds) = bounds {
            BvhNode::root_with_bounds(list, bounds)
        } else {
            BvhNode::root(list)
        };

        let initial_node_capacity = list.len() * 2 / 3;
        let mut nodes = Vec::with_capacity(initial_node_capacity);
        nodes.push(root);

        if !list.is_empty() {
            root.split(list, &mut nodes, 0, max_depth);
            nodes[0] = root;
        }

        let construction_time = instant.elapsed().as_secs_f64();

        // print out construction time in release builds
        #[cfg(not(debug_assertions))]
        {
            log::info!("BVH took {} seconds to build", construction_time);
        }

        // print out full debug info in debug builds
        #[cfg(debug_assertions)]
        {
            let leaf_node_lengths: Vec<_> = nodes[1..]
                .iter()
                .filter(|node| node.child_node == 0)
                .map(|node| node.len)
                .collect();

            let leaf_node_count = leaf_node_lengths.len();

            let min_leaf_object_count = leaf_node_lengths.iter().min().unwrap_or(&root.len);
            let max_leaf_object_count = leaf_node_lengths.iter().max().unwrap_or(&root.len);

            let average_leaf_object_count =
                leaf_node_lengths.iter().sum::<u32>() as f32 / leaf_node_count as f32;

            fn find_height(nodes: &[BvhNode], index: u32) -> i32 {
                if nodes[index as usize].child_node == 0 {
                    return -1;
                }

                let lt_height = find_height(nodes, nodes[index as usize].child_node);
                let gt_height = find_height(nodes, nodes[index as usize].child_node + 1);

                lt_height.max(gt_height) + 1
            }

            let max_actual_depth = find_height(&nodes, 0);

            log::info!(
                r#"
            ---------- Bounding Volume Hierarchy Info ----------
            - Object count: {},
            - Number of nodes: {},
            - Max allowed height: {},
            - Actual height: {},

            Leaf nodes:
                - Count: {}
                - Object count
                    - Min: {}
                    - Max: {}
                    - Average: {}

            Construction time: {} seconds
            ----------------------------------------------------
            "#,
                list.len(),
                nodes.len(),
                max_depth,
                max_actual_depth,
                leaf_node_count,
                min_leaf_object_count,
                max_leaf_object_count,
                average_leaf_object_count,
                construction_time
            );
        }

        Self { nodes }
    }

    #[allow(unused)]
    pub fn nodes(&self) -> &[BvhNode] {
        &self.nodes
    }

    pub fn into_nodes(self) -> Vec<BvhNode> {
        self.nodes
    }
}
