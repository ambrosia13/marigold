use std::{collections::HashMap, hash::Hash, ops::Range, sync::Arc};

use vulkano::{
    buffer::Buffer,
    command_buffer::{
        RecordingCommandBuffer,
        raw::{DependencyInfo, ImageMemoryBarrier},
    },
    image::{Image, ImageAspects, ImageSubresourceRange, view::ImageView},
    sync::{AccessFlags, MemoryBarrier, PipelineStage, PipelineStages},
};

pub mod geometry;

/// tracks the most recent access on each resource type
///
/// that is to say, for example if we write to buffer X with no dependencies, that operation
/// can be carried out without a pipeline barrier. in the meantime, we can record the most recent
/// operation on buffer X in the tracker. then, if we operate on buffer Y (whether read or write)
/// while depending on the result of the buffer X operation, then this second operation will need
/// to insert a pipeline barrier (more specifically a buffer memory barrier) before it is recorded.
///
/// because we stored the previous access of buffer X in the tracker, we can now automatically tell
/// what pipeline barrier we need, because the only dependency of the buffer Y operation is the
/// buffer X operation.
///
/// this can be expanded to multiple and more types of resource accesses, like if an operation
/// depends on the results of two separate resources, or if memory within a single resource also
/// needs to have the correct barriers, such as with bloom mip writes (we write to the same resource,
/// but different mips).
pub struct ResourceTracker {
    images: HashMap<Arc<Image>, Vec<ResourceAccess>>,
    buffers: HashMap<Arc<Buffer>, Vec<ResourceAccess>>,
}

// impl ResourceTracker {
//     pub fn access<'a>(&mut self, access: ResourceAccess) -> Option<DependencyInfo<'a>> {
//         match access.info {
//             Some(AccessInfo::Buffer {
//                 buffer,
//                 offset,
//                 size,
//             }) => todo!(),
//             Some(AccessInfo::Image {
//                 image,
//                 mips,
//                 layers,
//             }) => self.images.entry(image).or_default().push,
//             None => todo!(),
//         }
//     }
// }

pub struct ResourceAccess {
    pub stages: PipelineStages,
    pub access: AccessFlags,
    pub info: Option<AccessInfo>,
}

pub enum AccessInfo {
    Buffer {
        buffer: Arc<Buffer>,
        offset: u64,
        size: u64,
    },
    Image {
        image: Arc<Image>,
        mips: Range<u32>,
        layers: Range<u32>,
    },
}

// pub struct ImageTracker {
//     pub image: Arc<Image>,
//     pub view: Arc<ImageView>, // basic view spanning the whole image, for common usage

//     // used for automatic pipeline barrier insertion
//     pub previous_accesses: Vec<ResourceAccess>,
// }

// impl ImageTracker {
//     pub fn access(
//         &mut self,
//         cmd_buf: RecordingCommandBuffer,
//         accesses: Vec<ResourceAccess>,
//     ) -> (Arc<Image>, Arc<ImageView>) {
//         let mut memory_barriers = Vec::new();
//         let mut buffer_memory_barriers = Vec::new();
//         let mut image_memory_barriers = Vec::new();

//         for access in self.previous_accesses.drain(..) {
//             match access.info {
//                 None => {
//                     memory_barriers.push(MemoryBarrier {
//                         src_stages: todo!(),
//                         src_access: todo!(),
//                         dst_stages: todo!(),
//                         dst_access: todo!(),
//                         ..Default::default()
//                     });
//                 }
//                 Some(AccessInfo::Buffer {
//                     buffer,
//                     offset,
//                     size,
//                 }) => todo!(),
//                 Some(AccessInfo::Image {
//                     image,
//                     mips,
//                     layers,
//                 }) => todo!(),
//             }
//         }

//         let dependency_info = DependencyInfo {
//             memory_barriers: todo!(),
//             buffer_memory_barriers: todo!(),
//             image_memory_barriers: &[ImageMemoryBarrier {
//                 src_stages: self.previous_stages,
//                 src_access: self.previous_access,
//                 dst_stages: stages,
//                 dst_access: access,
//                 old_layout: vulkano::image::ImageLayout::General,
//                 new_layout: vulkano::image::ImageLayout::General,
//                 queue_family_ownership_transfer: todo!(),
//                 image: todo!(),
//                 subresource_range: ImageSubresourceRange {
//                     aspects: self.image.format().aspects(),
//                     base_mip_level: todo!(),
//                     level_count: todo!(),
//                     base_array_layer: todo!(),
//                     layer_count: todo!(),
//                 },
//                 _ne: todo!(),
//             }],
//             ..Default::default()
//         };

//         unsafe { cmd_buf.pipeline_barrier(&dependency_info) };

//         (self.image.clone(), self.view.clone())
//     }
// }
