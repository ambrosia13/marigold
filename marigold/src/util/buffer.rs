use std::{marker::PhantomData, num::NonZero};

use gpu_layout::{AsGpuBytes, Std430Layout};

use crate::{app::render::GpuHandle, util};

pub const MIN_GPU_VEC_CAPACITY: usize = 1;

pub struct GpuVec<T>
where
    T: Default + AsGpuBytes,
{
    gpu: GpuHandle,
    label: String,
    buffer: wgpu::Buffer,
    uploaded_capacity: usize,
    aligned_element_size: usize,
    padding: u64,
    _marker: PhantomData<T>,
}

impl<T> GpuVec<T>
where
    T: Default + AsGpuBytes,
{
    pub fn new(
        gpu: &GpuHandle,
        label: &str,
        source: &Vec<T>,
        extra_usages: wgpu::BufferUsages,
    ) -> Self {
        let aligned_element_size = T::default().as_gpu_bytes::<Std430Layout>().as_slice().len();
        let capacity = source.capacity().max(MIN_GPU_VEC_CAPACITY); // match the capacity of the source vector

        let data_size = (aligned_element_size * capacity) as u64;
        let len_size = (aligned_element_size * source.len()) as u64;
        let padded_size = data_size.next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);

        let padding = padded_size - data_size;

        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: padded_size,
            usage: extra_usages | wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });

        let mut view = buffer.get_mapped_range_mut(..);

        view[0..(len_size as usize)]
            .copy_from_slice(source.as_gpu_bytes::<Std430Layout>().as_slice());

        drop(view);
        buffer.unmap();
        log::info!("buffer {} explicitly unmapped", label);

        Self {
            gpu: gpu.clone(),
            label: label.to_owned(),
            buffer,
            uploaded_capacity: capacity,
            aligned_element_size,
            padding,
            _marker: PhantomData,
        }
    }

    pub fn should_reallocate(&self, source: &Vec<T>) -> bool {
        source.capacity() > self.uploaded_capacity
    }

    // this is its own method so the immutable reference avoids triggering change detection
    pub fn update_existing_buffer(&self, source: &Vec<T>) {
        let capacity = source.capacity().max(MIN_GPU_VEC_CAPACITY);
        assert!(capacity <= self.uploaded_capacity);

        // if zero length, avoid doing a write as this should be handled by the counts buffer elsewhere
        if source.is_empty() {
            return;
        }

        // can write within existing buffer
        let mut view = self
            .gpu
            .queue
            .write_buffer_with(
                &self.buffer,
                0,
                NonZero::new((self.aligned_element_size * source.len()) as u64).unwrap(),
            )
            .unwrap();

        // write contents
        let mut data_bytes = source.as_gpu_bytes::<Std430Layout>();
        let data_bytes = data_bytes.as_slice();
        view[..].copy_from_slice(data_bytes);
    }

    // returns true if buffer was reallocated
    pub fn reallocate_buffer(&mut self, source: &Vec<T>) {
        let capacity = source.capacity().max(MIN_GPU_VEC_CAPACITY);
        assert!(capacity > self.uploaded_capacity);

        let old_capacity_bytes = self.uploaded_capacity * self.aligned_element_size;
        let new_capacity_bytes = capacity * self.aligned_element_size;
        let (old_size, old_units) = util::display_byte_size(old_capacity_bytes);
        let (new_size, new_units) = util::display_byte_size(new_capacity_bytes);

        // need to reallocate buffer & return true
        log::info!(
            "buffer {} grew beyond capacity, reallocating. old capacity: {} ({:.2} {}), new capacity: {} ({:.2} {})",
            &self.label,
            self.uploaded_capacity,
            old_size,
            old_units,
            capacity,
            new_size,
            new_units,
        );

        let data_size = (self.aligned_element_size * capacity) as u64;
        let len_size = (self.aligned_element_size * source.len()) as u64;
        let padded_size = data_size.next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);

        let padding = padded_size - data_size;

        self.buffer = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&self.label),
            size: padded_size,
            usage: self.buffer.usage(),
            mapped_at_creation: true,
        });

        let mut view = self.buffer.get_mapped_range_mut(..);

        view[0..(len_size as usize)]
            .copy_from_slice(source.as_gpu_bytes::<Std430Layout>().as_slice());

        drop(view);
        self.buffer.unmap();
        log::info!("buffer {} explicitly unmapped", &self.label);

        self.uploaded_capacity = capacity;
        self.padding = padding;
    }

    pub fn as_buffer_binding(&self) -> wgpu::BindingResource<'_> {
        wgpu::BindingResource::Buffer(
            self.buffer
                .slice(0..(self.buffer.size() - self.padding))
                .into(),
        )
    }
}
