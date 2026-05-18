#![feature(iter_partition_in_place)]

use std::{
    fs::File,
    io::Write,
    path::Path,
    sync::{Arc, atomic::AtomicU32},
};

use glam::Vec3A;
use gpu_layout::{AsGpuBytes, GpuBytes};
use rand::Rng;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::Serialize;

pub trait AsBoundingVolume {
    fn bounding_volume(&self) -> BoundingVolume;

    #[allow(unused)]
    fn center(&self) -> Vec3A {
        self.bounding_volume().center()
    }
}

pub trait AsBoundingVolumeIndices<S> {
    fn bounding_volume(&self, source: &[S]) -> BoundingVolume;

    fn center(&self, source: &[S]) -> Vec3A {
        self.bounding_volume(source).center()
    }
}

impl<T: AsBoundingVolume> AsBoundingVolumeIndices<()> for T {
    fn bounding_volume(&self, _source: &[()]) -> BoundingVolume {
        self.bounding_volume()
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

        buf.write(&self.min).write(&self.max);

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

    #[allow(unused)]
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

        buf.write(&self.bounds.min);
        buf.write(&self.start_index);
        buf.write(&self.bounds.max);

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
    pub const DEPTH_COST: f32 = 1.0;
    pub const OBJECT_COST: f32 = 8.0;
    pub const MIN_OBJECTS_PER_NODE: u32 = 1;
    pub const MAX_OBJECTS_PER_NODE: u32 = 1;

    pub fn root<S, T: AsBoundingVolumeIndices<S>>(list: &mut [T], source: &[S]) -> Self {
        let mut bounds = BoundingVolume::new(Vec3A::ZERO, Vec3A::ZERO);

        for item in list.iter() {
            bounds.grow_from_bounding_volume(item.bounding_volume(source));
        }

        Self::root_with_bounds(list, bounds)
    }

    pub fn root_with_bounds<S, T: AsBoundingVolumeIndices<S>>(
        list: &mut [T],
        bounds: BoundingVolume,
    ) -> Self {
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

    fn leaf_cost(&self) -> f32 {
        Self::OBJECT_COST * self.len as f32
        // Self::NODE_COST + Self::OBJECT_COST * self.bounds.surface_area() * self.len as f32
    }

    fn evaluate_split_cost<S, T: AsBoundingVolumeIndices<S>>(
        bounds: BoundingVolume,
        list: &[T],
        source: &[S],
        axis: usize,
        threshold: f32,
    ) -> Option<(BoundingVolume, BoundingVolume, f32)> {
        let mut bounds_lt = BoundingVolume::EMPTY;
        let mut bounds_gt = BoundingVolume::EMPTY;

        let mut lt_count = 0;
        let mut gt_count = 0;

        for obj in list {
            let obj_center = obj.center(source);

            if obj_center[axis] <= threshold {
                bounds_lt.grow_from_bounding_volume(obj.bounding_volume(source));
                lt_count += 1;
            } else {
                bounds_gt.grow_from_bounding_volume(obj.bounding_volume(source));
                gt_count += 1;
            }
        }

        // refuse empty nodes or nodes with not enough objects
        if lt_count < Self::MIN_OBJECTS_PER_NODE || gt_count < Self::MIN_OBJECTS_PER_NODE {
            //log::info!("Invalid split, axis: {}, threshold: {}")
            return None;
        }

        let lt_cost =
            bounds_lt.surface_area() / bounds.surface_area() * Self::OBJECT_COST * lt_count as f32;
        let gt_cost =
            bounds_gt.surface_area() / bounds.surface_area() * Self::OBJECT_COST * gt_count as f32;

        Some((bounds_lt, bounds_gt, Self::DEPTH_COST + lt_cost + gt_cost))
    }

    // returns (bounds_lt, bounds_gt, cost, axis, threshold)
    fn choose_split_axis<S: Sync, T: AsBoundingVolumeIndices<S> + Clone + Sync>(
        bounds: BoundingVolume,
        list: &[T],
        source: &[S],
    ) -> Option<(BoundingVolume, BoundingVolume, f32, usize, f32)> {
        // compute the results for all 3 axes in parallel, and then choose the best at the end
        let results_per_axis: Vec<_> = (0..3)
            .into_par_iter()
            .filter_map(|axis| {
                // if there are fewer objects in the volume, take a more accurate search
                let (bounds_min, bounds_max) = if list.len() < 10 {
                    let mut min = f32::INFINITY;
                    let mut max = f32::NEG_INFINITY;

                    // find min and max positions of the objects along this axis
                    for object in list {
                        let object_bounds = object.bounding_volume(source);

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
                let results: Vec<(BoundingVolume, BoundingVolume, f32, f32)> = (0..step_count)
                    .into_par_iter()
                    .filter_map(|i| {
                        let threshold = bounds_min + bounds_step * (i as f32 + 0.5);
                        let cost = Self::evaluate_split_cost(bounds, list, source, axis, threshold);

                        cost.map(|(bounds_lt, bounds_gt, cost)| {
                            (bounds_lt, bounds_gt, cost, threshold)
                        })
                    })
                    .collect();

                if results.is_empty() {
                    return None;
                }

                let mut best_bounds_lt = BoundingVolume::EMPTY;
                let mut best_bounds_gt = BoundingVolume::EMPTY;
                let mut best_cost = f32::INFINITY;
                let mut best_threshold = 0.0;

                for (bounds_lt, bounds_gt, cost, threshold) in results {
                    if cost < best_cost {
                        best_bounds_lt = bounds_lt;
                        best_bounds_gt = bounds_gt;
                        best_cost = cost;
                        best_threshold = threshold;
                    }
                }

                Some((
                    best_bounds_lt,
                    best_bounds_gt,
                    best_cost,
                    axis,
                    best_threshold,
                ))
            })
            .collect();

        if results_per_axis.is_empty() {
            return None;
        }

        let mut best_bounds_lt = BoundingVolume::EMPTY;
        let mut best_bounds_gt = BoundingVolume::EMPTY;
        let mut best_cost = f32::INFINITY;
        let mut best_axis = 0;
        let mut best_threshold = 0.0;

        for (bounds_lt, bounds_gt, cost, axis, threshold) in results_per_axis {
            if cost < best_cost {
                best_bounds_lt = bounds_lt;
                best_bounds_gt = bounds_gt;
                best_cost = cost;
                best_axis = axis;
                best_threshold = threshold;
            }
        }

        Some((
            best_bounds_lt,
            best_bounds_gt,
            best_cost,
            best_axis,
            best_threshold,
        ))
    }

    pub fn split<S: Sync, T: AsBoundingVolumeIndices<S> + Clone + Sync>(
        &mut self,
        list: &mut [T],
        source: &[S],
        nodes: &mut Vec<Self>,
        depth: u32,
        height: Arc<AtomicU32>,
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

        let Some((bounds_lt, bounds_gt, cost, split_axis, split_threshold)) =
            Self::choose_split_axis(self.bounds, self.slice(list), source)
        else {
            // log::info!("Refused a split for a node with object count {}", self.len);
            return;
        };

        // don't split if the cost of the split would be greater than the current cost
        if cost >= self.leaf_cost() {
            return;
        }

        child_lt.bounds = bounds_lt;
        child_gt.bounds = bounds_gt;

        // std partition without extra bounds loop
        let object_span =
            &mut list[self.start_index as usize..(self.start_index + self.len) as usize];

        let split = object_span
            .iter_mut()
            .partition_in_place(|object| object.center(source)[split_axis] <= split_threshold);

        let (lt, gt) = object_span.split_at_mut(split);

        child_lt.len = lt.len() as u32;
        child_gt.len = gt.len() as u32;

        child_gt.start_index = self.start_index + child_lt.len;

        // manual partition
        // let greater = |object: &T| object.center(source)[split_axis] > split_threshold;

        // for global_index in self.start_index..(self.start_index + self.len) {
        //     let global_index = global_index as usize;
        //     let object = &list[global_index];

        //     if greater(object) {
        //         // child_gt
        //         //     .bounds
        //         //     .grow_from_bounding_volume(object.bounding_volume(source));
        //         child_gt.len += 1;

        //         let swap_index = (child_gt.start_index + child_gt.len) as usize - 1;
        //         list.swap(swap_index, global_index);
        //         child_lt.start_index += 1;
        //     } else {
        //         // child_lt
        //         //     .bounds
        //         //     .grow_from_bounding_volume(object.bounding_volume(source));
        //         child_lt.len += 1;
        //     }
        // }

        if child_gt.len > 0 && child_lt.len > 0 {
            self.child_node = nodes.len() as u32;
            nodes.push(child_gt);
            nodes.push(child_lt);

            // track the maximum depth
            height.fetch_max(depth, std::sync::atomic::Ordering::Relaxed);

            // split the children of this node
            child_gt.split(list, source, nodes, depth + 1, height.clone(), max_depth);
            child_lt.split(list, source, nodes, depth + 1, height, max_depth);

            nodes[self.child_node as usize] = child_gt;
            nodes[self.child_node as usize + 1] = child_lt;
        }
    }
}

pub struct BvhSettings<'a> {
    pub name: &'a str,
    pub bounds: Option<BoundingVolume>,
    pub max_depth: u32,
    pub profiling_info: bool,
    pub profiling_info_directory: Option<&'a Path>,
}

#[derive(Serialize)]
struct BvhProfilingInfo<'a> {
    name: &'a str,
    construction_time: f64,
    node_count: u32,
    leaf_node_count: u32,
    min_leaf_object_count: u32,
    max_leaf_object_count: u32,
    avg_leaf_object_count: f64,
    stddev_leaf_object_count: f64,
    total_object_count: u32,
    height: u32,
    max_depth: u32,
}

pub struct BoundingVolumeHierarchy {
    nodes: Vec<BvhNode>,
}

impl BoundingVolumeHierarchy {
    pub fn new<S: Sync, T: AsBoundingVolumeIndices<S> + Clone + Sync>(
        list: &mut [T],
        source: &[S],
        settings: BvhSettings<'_>,
    ) -> Self {
        if list.is_empty() {
            return Self { nodes: Vec::new() };
        }

        let instant = std::time::Instant::now();

        // let max_depth = 32; //f32::log2(list.len() as f32) as u32 + 6;

        // create the root node
        let mut root = if let Some(bounds) = settings.bounds {
            BvhNode::root_with_bounds(list, bounds)
        } else {
            BvhNode::root(list, source)
        };

        let initial_node_capacity = list.len() * 2 / 3;
        let mut nodes = Vec::with_capacity(initial_node_capacity);
        nodes.push(root);

        let height = Arc::new(AtomicU32::new(0));

        if !list.is_empty() {
            root.split(
                list,
                source,
                &mut nodes,
                0,
                height.clone(),
                settings.max_depth,
            );
            nodes[0] = root;
        }

        let construction_time = instant.elapsed().as_secs_f64();

        if settings.profiling_info {
            // full debug info
            let leaf_node_lengths: Vec<_> = nodes[1..]
                .iter()
                .filter(|node| node.child_node == 0)
                .map(|node| node.len)
                .collect();

            let leaf_node_count = leaf_node_lengths.len();

            let min_leaf_object_count = *leaf_node_lengths.iter().min().unwrap_or(&root.len);
            let max_leaf_object_count = *leaf_node_lengths.iter().max().unwrap_or(&root.len);

            let avg_leaf_object_count =
                leaf_node_lengths.iter().sum::<u32>() as f64 / leaf_node_count as f64;

            let stddev_leaf_object_count = (leaf_node_lengths
                .iter()
                .map(|&l| (l as f64 - avg_leaf_object_count).powi(2))
                .sum::<f64>()
                / leaf_node_count as f64)
                .sqrt();

            let height = height.load(std::sync::atomic::Ordering::Relaxed);

            log::info!(
                r#"
            ---------- Bounding Volume Hierarchy Info ----------
            - Objects: {},
            - Nodes: {},
            - Allowed depth: {},
            - Tree Height: {},

            Leaf nodes:
                - Count: {}
                - Objects
                    - Min: {}
                    - Max: {}
                    - Average: {}
                    - Standard Deviation: {}

            Construction time: {} seconds
            ----------------------------------------------------
            "#,
                list.len(),
                nodes.len(),
                settings.max_depth,
                height,
                leaf_node_count,
                min_leaf_object_count,
                max_leaf_object_count,
                avg_leaf_object_count,
                stddev_leaf_object_count,
                construction_time
            );

            let info = BvhProfilingInfo {
                name: settings.name,
                construction_time,
                node_count: nodes.len() as u32,
                leaf_node_count: leaf_node_count as u32,
                min_leaf_object_count,
                max_leaf_object_count,
                avg_leaf_object_count,
                stddev_leaf_object_count,
                total_object_count: list.len() as u32,
                height,
                max_depth: settings.max_depth,
            };

            // write the info to disk for external tools such as plotting libraries to analyze effect of optimization
            if let Some(path) = settings.profiling_info_directory {
                std::fs::create_dir_all(path).unwrap();

                // random 9 digit number for the id
                let build_id = rand::rng().random_range(100000000..=999999999);

                let json_path = path.join(format!("bvh_{}_{}.json", settings.name, build_id,));

                if !json_path.exists() {
                    let mut file = File::create(json_path)
                        .expect("unable to create file on disk for bvh profile info");

                    let json = serde_json::to_string(&info).unwrap();
                    writeln!(file, "{}", json)
                        .expect("unable to write to bvh profile info file on disk");
                }
            }
        } else {
            // simple debug info
            log::info!("BVH took {} seconds to build", construction_time);
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
