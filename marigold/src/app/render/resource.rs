// use std::sync::Arc;

// use derived_deref::Deref;
// use vulkano::{buffer::Subbuffer, descriptor_set::layout::DescriptorSetLayout};

// // there are other types of descriptors, but we only care about these three.
// // note that binding index 1 (used for combined texture samplers) is skipped since we don't use those
// // this is decided according to slang descriptor handle specification

// #[derive(Deref)]
// pub struct SamplerHandle(u32); // binding index 0

// #[derive(Deref)]
// pub struct SampledImageHandle(u32); // binding index 2

// #[derive(Deref)]
// pub struct StorageImageHandle(u32); // binding index 3

// #[derive(Default)]
// struct HandleAllocator {
//     next: u32,
//     free: Vec<u32>,
// }

// impl HandleAllocator {
//     pub fn allocate(&mut self) -> u32 {
//         self.free.pop().unwrap_or_else(|| {
//             let handle = self.next;
//             self.next += 1;
//             handle
//         })
//     }

//     pub fn free(&mut self, handle: u32) {
//         self.free.push(handle);
//     }
// }

// pub struct RenderResourceHeap {
//     sampler_handle_allocator: HandleAllocator,
//     sampled_image_handle_allocator: HandleAllocator,
//     storage_image_handle_allocator: HandleAllocator,

//     max_samplers: u32,
//     max_sampled_images: u32,
//     max_storage_images: u32,

//     sampler_descriptor_size: usize,
//     sampled_image_descriptor_size: usize,
//     storage_image_descriptor_size: usize,

//     layout: Arc<DescriptorSetLayout>,
//     descriptor_buffer: Subbuffer<[u8]>,
// }

// impl RenderResourceHeap {
//     pub fn
// }
