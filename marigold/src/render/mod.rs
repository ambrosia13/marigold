use std::sync::Arc;

use vulkano::{
    command_buffer::{
        RecordingCommandBuffer,
        raw::{DependencyInfo, ImageMemoryBarrier},
    },
    image::{Image, ImageAspects, ImageSubresourceRange, view::ImageView},
    sync::{AccessFlags, PipelineStage, PipelineStages},
};

pub mod geometry;

// pub struct ImageTracker {
//     pub image: Arc<Image>,
//     pub view: Arc<ImageView>, // basic view spanning the whole image, for common usage
//     pub previous_stages: PipelineStages,
//     pub previous_access: AccessFlags,
// }

// impl ImageTracker {
//     pub fn access(
//         &mut self,
//         cmd_buf: RecordingCommandBuffer,
//         stages: PipelineStages,
//         access: AccessFlags,
//     ) -> (Arc<Image>, Arc<ImageView>) {
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
