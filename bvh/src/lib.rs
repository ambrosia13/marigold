#![feature(iter_partition_in_place)]

use std::{
    fs::File,
    io::Write,
    ops::Deref,
    path::Path,
    sync::{Arc, atomic::AtomicU32},
};

use glam::Vec3A;
use gpu_layout::{AsGpuBytes, GpuBytes};
use rand::Rng;
use serde::Serialize;

pub trait AsBoundingVolume {
    fn bounding_volume(&self) -> BoundingVolume;

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

    pub fn grow<T: AsBoundingVolume>(&mut self, object: &T) {
        let bounds = object.bounding_volume();
        self.grow_from_bounding_volume(bounds);
    }

    pub fn grow_from_bounding_volume(&mut self, bounds: BoundingVolume) {
        if !self.is_empty() && !bounds.is_empty() {
            self.min = self.min.min(bounds.min);
            self.max = self.max.max(bounds.max);
        } else if self.is_empty() {
            *self = bounds;
        }
    }

    pub fn is_empty(self) -> bool {
        self.empty
    }

    pub fn contains(self, point: Vec3A) -> bool {
        !self.empty && point.cmpge(self.min).all() && point.cmple(self.max).all()
    }
}

enum SplitDescriptor {
    Threshold { axis: usize, threshold: f32 },
    Index { axis: usize, index: usize },
    Exact { index: usize },
}

// represents a potential node split along with its cost
struct CandidateSplit {
    bounds_lt: BoundingVolume,
    bounds_gt: BoundingVolume,
    count_lt: u32,
    count_gt: u32,
    cost: f32,
}

// represents a split that is possible given our constraints, along with additional
// information about its axis and threshold
struct SuccessfulSplit {
    candidate: CandidateSplit,
    desc: SplitDescriptor,
}

impl Deref for SuccessfulSplit {
    type Target = CandidateSplit;

    fn deref(&self) -> &Self::Target {
        &self.candidate
    }
}

#[derive(Clone)]
struct SplitBin {
    count: usize,
    bounds: BoundingVolume,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct BvhNode<const MIN_LEAF_OBJECTS: u32, const MAX_LEAF_OBJECTS: u32> {
    pub bounds: BoundingVolume,
    pub start_index: u32,
    pub len: u32,
    pub child_node: u32,
}

impl<const MIN_LEAF_OBJECTS: u32, const MAX_LEAF_OBJECTS: u32> AsGpuBytes
    for BvhNode<MIN_LEAF_OBJECTS, MAX_LEAF_OBJECTS>
{
    fn as_gpu_bytes<L: gpu_layout::GpuLayout + ?Sized>(&self) -> GpuBytes<'_, L> {
        let mut buf = GpuBytes::empty();

        if MAX_LEAF_OBJECTS == 1 {
            // no need to encode length at all, we know leaf count is exactly 1
            // and leaf node is checked as child_node == 0
            buf.write(&self.bounds.min);
            buf.write(&self.start_index);
            buf.write(&self.bounds.max);
            buf.write(&self.child_node);
        } else {
            buf.write(&self.bounds.min);
            buf.write(&self.start_index);
            buf.write(&self.bounds.max);

            // number of bits required for the length
            let len_overflow = MAX_LEAF_OBJECTS.next_power_of_two();

            let len_bits = len_overflow.ilog2();
            let child_node_bits = 32 - len_bits;

            assert!(len_bits < 32);

            if self.child_node == 0 {
                // make sure the length fits in the bits
                assert!(self.len < len_overflow);
            }

            let len_mask = (1 << len_bits) - 1;
            let child_node_mask = !len_mask;

            let packed = (self.len & len_mask) << child_node_bits;
            let packed = packed | (self.child_node & child_node_mask);

            buf.write(&packed);
        }

        // buf.write(&self.bounds.min);
        // buf.write(&self.start_index);
        // buf.write(&self.bounds.max);

        // if self.child_node == 0 {
        //     assert!(self.len < 128);
        // }

        // let packed = (self.len & 0b11111) << 25; // upper 5 bits
        // let packed = packed | (self.child_node & ((1 << 25) - 1)); // lower 27 bits

        // buf.write(&packed);

        buf
    }
}

impl<const MIN_LEAF_OBJECTS: u32, const MAX_LEAF_OBJECTS: u32>
    BvhNode<MIN_LEAF_OBJECTS, MAX_LEAF_OBJECTS>
{
    pub const DEPTH_COST: f32 = 1.0;
    pub const OBJECT_COST: f32 = 8.0;

    pub fn root<S, T: AsBoundingVolumeIndices<S>>(list: &mut [T], source: &[S]) -> Self {
        let mut bounds = BoundingVolume::EMPTY;

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
    }

    fn evaluate_binned_split(
        bounds: BoundingVolume,
        bins: &[SplitBin],
        bin_split: usize,
    ) -> CandidateSplit {
        let (bins_lt, bins_gt) = bins.split_at(bin_split);

        let mut bounds_lt = BoundingVolume::EMPTY;
        let mut bounds_gt = BoundingVolume::EMPTY;

        let mut lt_count = 0;
        let mut gt_count = 0;

        for bin in bins_lt {
            lt_count += bin.count as u32;
            bounds_lt.grow_from_bounding_volume(bin.bounds);
        }

        for bin in bins_gt {
            gt_count += bin.count as u32;
            bounds_gt.grow_from_bounding_volume(bin.bounds);
        }

        let lt_cost =
            bounds_lt.surface_area() / bounds.surface_area() * Self::OBJECT_COST * lt_count as f32;
        let gt_cost =
            bounds_gt.surface_area() / bounds.surface_area() * Self::OBJECT_COST * gt_count as f32;

        CandidateSplit {
            bounds_lt,
            bounds_gt,
            count_lt: lt_count,
            count_gt: gt_count,
            cost: Self::DEPTH_COST + lt_cost + gt_cost,
        }
    }

    fn evaluate_threshold_split<S, T: AsBoundingVolumeIndices<S>>(
        bounds: BoundingVolume,
        list: &[T],
        source: &[S],
        axis: usize,
        threshold: f32,
    ) -> CandidateSplit {
        let mut lt_count = 0;
        let mut gt_count = 0;

        let mut bounds_lt = BoundingVolume::EMPTY;
        let mut bounds_gt = BoundingVolume::EMPTY;

        for object in list {
            let bounds = object.bounding_volume(source);

            if bounds.center()[axis] < threshold {
                bounds_lt.grow_from_bounding_volume(bounds);
                lt_count += 1;
            } else {
                bounds_gt.grow_from_bounding_volume(bounds);
                gt_count += 1;
            }
        }

        let lt_cost =
            bounds_lt.surface_area() / bounds.surface_area() * Self::OBJECT_COST * lt_count as f32;
        let gt_cost =
            bounds_gt.surface_area() / bounds.surface_area() * Self::OBJECT_COST * gt_count as f32;

        CandidateSplit {
            bounds_lt,
            bounds_gt,
            count_lt: lt_count,
            count_gt: gt_count,
            cost: Self::DEPTH_COST + lt_cost + gt_cost,
        }
    }

    fn binned_sweep<S: Sync, T: AsBoundingVolumeIndices<S> + Sync>(
        parent_bounds: BoundingVolume,
        centroid_bounds: BoundingVolume,
        list: &[T],
        source: &[S],
        axis: usize,
    ) -> Option<SuccessfulSplit> {
        // refuse the split if the parent node doesn't cover any area on this axis
        if centroid_bounds.extent()[axis] == 0.0 {
            return None;
        }

        let bin_count = list.len().clamp(16, 32);

        let mut bins: Vec<SplitBin> = vec![
            SplitBin {
                count: 0,
                bounds: BoundingVolume::EMPTY,
            };
            bin_count
        ];

        // populate bins
        for object in list {
            let object_bounds = object.bounding_volume(source);
            let center = object_bounds.center();

            let percent_along_bounds =
                (center[axis] - centroid_bounds.min[axis]) / centroid_bounds.extent()[axis];

            let bin_index = (percent_along_bounds * bin_count as f32).floor() as usize;
            let bin_index = bin_index.min(bin_count - 1);

            bins[bin_index].count += 1;
            bins[bin_index]
                .bounds
                .grow_from_bounding_volume(object_bounds);
        }

        // iterate from the second to the last bin as the split point
        // safe bc there are at least two bins
        // compute all results, then choose the best
        (1..bin_count)
            .filter_map(|i| {
                let split = Self::evaluate_binned_split(parent_bounds, &bins, i);
                let index = split.count_lt as usize; // instead of computing and passing threshold, save index instead

                // refuse a split if too few objects are in each child
                if split.count_lt < MIN_LEAF_OBJECTS || split.count_gt < MIN_LEAF_OBJECTS {
                    return None;
                }

                Some(SuccessfulSplit {
                    candidate: split,
                    desc: SplitDescriptor::Index { axis, index },
                })
            })
            .min_by(|split_a, split_b| split_a.cost.total_cmp(&split_b.cost))
    }

    #[allow(unused)]
    fn adaptive_sweep<S: Sync, T: AsBoundingVolumeIndices<S> + Sync>(
        parent_bounds: BoundingVolume,
        centroid_bounds: BoundingVolume,
        list: &[T],
        source: &[S],
        axis: usize,
    ) -> Option<SuccessfulSplit> {
        let accurate_search_threshold = 8;

        // extract common code into function
        let evaluate = |threshold| {
            let split =
                Self::evaluate_threshold_split(parent_bounds, list, source, axis, threshold);

            // refuse a split if too few objects are in each child
            if split.count_lt < MIN_LEAF_OBJECTS || split.count_gt < MIN_LEAF_OBJECTS {
                return None;
            }

            Some(SuccessfulSplit {
                candidate: split,
                desc: SplitDescriptor::Threshold { axis, threshold },
            })
        };

        if list.len() < accurate_search_threshold {
            // for small amounts of objects, do the most accurate search
            // since the dataset is small, do a sequential search rather than parallel search
            list.iter()
                .filter_map(|object| {
                    let bounds = object.bounding_volume(source);
                    let threshold = bounds.center()[axis];

                    evaluate(threshold)
                })
                .min_by(|split_a, split_b| split_a.cost.total_cmp(&split_b.cost))
        } else {
            // do an approximate search
            let step_count = list.len().clamp(accurate_search_threshold, 32);
            let bounds_step = centroid_bounds.extent()[axis] / step_count as f32;

            // since we are using this search for small sizes, do a sequential iteration instead of parallel
            (0..step_count)
                .filter_map(|i| {
                    let threshold = centroid_bounds.min[axis] + bounds_step * (i as f32 + 0.5);

                    evaluate(threshold)
                })
                .min_by(|split_a, split_b| split_a.cost.total_cmp(&split_b.cost))
        }
    }

    fn median_split<S: Sync, T: AsBoundingVolumeIndices<S> + Sync>(
        parent_bounds: BoundingVolume,
        list: &mut [T],
        source: &[S],
        axis: usize,
    ) -> SuccessfulSplit {
        let median_index = list.len() / 2;

        // want the median to be part of `gt` slice so ignore this result and call split later
        let _ = list.select_nth_unstable_by(median_index, |a, b| {
            a.center(source)[axis].total_cmp(&b.center(source)[axis])
        });

        let (lt, gt) = list.split_at(median_index);

        let mut bounds_lt = BoundingVolume::EMPTY;
        let mut bounds_gt = BoundingVolume::EMPTY;

        for object in lt.iter() {
            bounds_lt.grow_from_bounding_volume(object.bounding_volume(source));
        }

        for object in gt.iter() {
            bounds_gt.grow_from_bounding_volume(object.bounding_volume(source));
        }

        let lt_cost = bounds_lt.surface_area() / parent_bounds.surface_area()
            * Self::OBJECT_COST
            * lt.len() as f32;
        let gt_cost = bounds_gt.surface_area() / parent_bounds.surface_area()
            * Self::OBJECT_COST
            * gt.len() as f32;

        let split = CandidateSplit {
            bounds_lt,
            bounds_gt,
            count_lt: lt.len() as u32,
            count_gt: gt.len() as u32,
            cost: Self::DEPTH_COST + lt_cost + gt_cost,
        };

        SuccessfulSplit {
            candidate: split,
            desc: SplitDescriptor::Exact {
                index: median_index,
            },
        }
    }

    fn select_split<S: Sync, T: AsBoundingVolumeIndices<S> + Clone + Sync>(
        parent_bounds: BoundingVolume,
        list: &mut [T],
        source: &[S],
    ) -> Option<SuccessfulSplit> {
        // compute centroid bounds, can be shared across all axes
        let mut centroid_min = Vec3A::INFINITY;
        let mut centroid_max = Vec3A::NEG_INFINITY;

        for object in list.iter() {
            let center = object.center(source);

            centroid_min = centroid_min.min(center);
            centroid_max = centroid_max.max(center);
        }

        let centroid_bounds = BoundingVolume::new(centroid_min, centroid_max);

        // only median split needs list mutability, so convert to immutable ref
        let list_ref = &*list;

        // compute the results for all 3 axes in parallel, and then choose the best
        let mut split = (0..3)
            .filter_map(|axis| {
                // choose adaptive sweep if not too many objects, results in higher quality split
                if list_ref.len() <= 32 {
                    Self::adaptive_sweep(parent_bounds, centroid_bounds, list_ref, source, axis)
                } else {
                    Self::binned_sweep(parent_bounds, centroid_bounds, list_ref, source, axis)
                }
            })
            .min_by(|split_a, split_b| split_a.cost.total_cmp(&split_b.cost));

        // if no split was found, but there are too many nodes, we can't stop here, so force a median split
        if split.is_none() && list.len() as u32 > MAX_LEAF_OBJECTS {
            split = (0..3)
                .map(|axis| Self::median_split(parent_bounds, list, source, axis))
                .min_by(|split_a, split_b| split_a.cost.total_cmp(&split_b.cost));
        }

        split
    }

    #[allow(clippy::too_many_arguments)]
    pub fn split<S: Sync, T: AsBoundingVolumeIndices<S> + Clone + Sync>(
        &mut self,
        list: &mut [T],
        source: &[S],
        nodes: &mut Vec<Self>,
        depth: u32,
        height: Arc<AtomicU32>,
        max_depth: u32,
    ) {
        // hard minimum on 1 element, no matter what min_leaf_objects is, because then one of the two
        // halves will have 0 elements, which saves zero work during traversal
        if self.len <= 1 {
            return;
        }

        // if we have less than double the min objects per leaf, there's no way for the
        // split to result in both halves having at least the min objects per leaf, so refuse
        // however, only perform this check if we are within the max objects per leaf to prioritize
        // the max constraint over the min constraint
        if depth == max_depth || (self.len <= MAX_LEAF_OBJECTS && self.len < MIN_LEAF_OBJECTS * 2) {
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

        let Some(split) = Self::select_split(self.bounds, list, source) else {
            // log::info!("Refused a split for a node with object count {}", self.len);
            return;
        };

        // don't split if the cost of the split would be greater than the current cost
        // however, only perform this check if we are within the max objects per leaf
        if self.len <= MAX_LEAF_OBJECTS && split.cost >= self.leaf_cost() {
            return;
        }

        // // never try to split 1 element, because one of the resulting halves will have 0 elements
        assert!(list.len() >= 2);

        child_lt.bounds = split.bounds_lt;
        child_gt.bounds = split.bounds_gt;

        child_lt.len = split.count_lt;
        child_gt.len = split.count_gt;

        // std partition
        let split_index = match split.desc {
            SplitDescriptor::Threshold { axis, threshold } => list
                .iter_mut()
                .partition_in_place(|object| object.center(source)[axis] < threshold),
            SplitDescriptor::Index { axis, index } => {
                let _ = list.select_nth_unstable_by(index, |a, b| {
                    a.bounding_volume(source).center()[axis]
                        .total_cmp(&b.bounding_volume(source).center()[axis])
                });

                index
            }
            SplitDescriptor::Exact { index } => index,
        };

        let (list_lt, list_gt) = list.split_at_mut(split_index);

        // temporary assertions to ensure correctness
        let actual_lt_len = list_lt.len() as u32;
        let actual_gt_len = list_gt.len() as u32;

        assert_ne!(list_lt.len(), 0);
        assert_ne!(list_gt.len(), 0);

        assert_eq!(
            child_lt.len,
            actual_lt_len,
            "was median split used? {}; was binned? {}",
            matches!(split.desc, SplitDescriptor::Exact { .. }),
            matches!(split.desc, SplitDescriptor::Index { .. }),
        );
        assert_eq!(
            child_gt.len,
            actual_gt_len,
            "was median split used? {}; was binned? {}",
            matches!(split.desc, SplitDescriptor::Exact { .. }),
            matches!(split.desc, SplitDescriptor::Index { .. }),
        );

        // assert our bounds are correct
        assert!(
            list_lt
                .iter()
                .all(|obj| child_lt.bounds.contains(obj.center(source)))
        );
        assert!(
            list_gt
                .iter()
                .all(|obj| child_gt.bounds.contains(obj.center(source)))
        );

        child_gt.start_index = self.start_index + child_lt.len;

        if child_gt.len > 0 && child_lt.len > 0 {
            self.child_node = nodes.len() as u32;
            nodes.push(child_gt);
            nodes.push(child_lt);

            // track the maximum depth
            height.fetch_max(depth, std::sync::atomic::Ordering::Relaxed);

            // split the children of this node
            child_gt.split(list_gt, source, nodes, depth + 1, height.clone(), max_depth);
            child_lt.split(list_lt, source, nodes, depth + 1, height, max_depth);

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

pub struct BoundingVolumeHierarchy<const MIN_LEAF_OBJECTS: u32, const MAX_LEAF_OBJECTS: u32> {
    nodes: Vec<BvhNode<MIN_LEAF_OBJECTS, MAX_LEAF_OBJECTS>>,
}

impl<const MIN_LEAF_OBJECTS: u32, const MAX_LEAF_OBJECTS: u32>
    BoundingVolumeHierarchy<MIN_LEAF_OBJECTS, MAX_LEAF_OBJECTS>
{
    pub fn new<S: Sync, T: AsBoundingVolumeIndices<S> + Clone + Sync>(
        list: &mut [T],
        source: &[S],
        settings: BvhSettings<'_>,
    ) -> Self {
        if list.is_empty() {
            log::warn!(
                "tried constructing a BVH with an empty object list, returning an empty BVH"
            );
            return Self { nodes: Vec::new() };
        }

        let feasible = if MIN_LEAF_OBJECTS == MAX_LEAF_OBJECTS {
            (list.len() as u32).is_multiple_of(MIN_LEAF_OBJECTS)
        } else {
            MAX_LEAF_OBJECTS >= 2 * MIN_LEAF_OBJECTS - 1
        };

        if !feasible {
            log::warn!(
                "BVH may be impossible to construct with the given constraints; \
                min leaf objects {} and max leaf objects {} is likely impossible with total objects {}. \
                BVH construction will follow the max objects per leaf rule, \
                but may not follow the min objects per leaf rule.",
                MIN_LEAF_OBJECTS,
                MAX_LEAF_OBJECTS,
                list.len()
            );
        }

        let instant = std::time::Instant::now();

        // create the root node
        let mut root = if let Some(bounds) = settings.bounds {
            BvhNode::root_with_bounds(list, bounds)
        } else {
            BvhNode::root(list, source)
        };

        // since we enforce the max objects per leaf rule, the lower bound of the number of leaves is
        // the total objects divided by the maximum possible number of leaves per object
        // rounded up to account for remainder, since the objects are distributed among the leaves
        //
        // note that for our case, min == max == 1, this is an exact value, not just an estimate,
        // so we allocate exactly as much space as we need. this holds for all min == max that are feasible
        let num_leaves_lower_bound = list.len().div_ceil(MAX_LEAF_OBJECTS as usize);

        // since a BVH is a full binary tree, the total number of nodes given N leaves is 2N - 1
        let initial_node_capacity = 2 * num_leaves_lower_bound - 1;

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

            if feasible {
                // additional assertions to make sure we have exact stats with a feasible bvh configuration
                assert!(min_leaf_object_count >= MIN_LEAF_OBJECTS);
                assert!(max_leaf_object_count <= MAX_LEAF_OBJECTS);
            }

            // assert that our estimate is correct if our params are such that we can make an exact estimate
            // to do this, just make sure the initial and final capacities are equal, meaning we didnt reallocate
            if MIN_LEAF_OBJECTS == 1 && MAX_LEAF_OBJECTS == 1 {
                let final_node_capacity = nodes.capacity();
                assert_eq!(initial_node_capacity, final_node_capacity);
            }

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
            log::info!(
                "BVH for {} took {} seconds to build",
                settings.name,
                construction_time
            );
        }

        Self { nodes }
    }

    pub fn nodes(&self) -> &[BvhNode<MIN_LEAF_OBJECTS, MAX_LEAF_OBJECTS>] {
        &self.nodes
    }

    pub fn into_nodes(self) -> Vec<BvhNode<MIN_LEAF_OBJECTS, MAX_LEAF_OBJECTS>> {
        self.nodes
    }
}
