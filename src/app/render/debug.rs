use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    time::Duration,
};

use bevy_ecs::{
    resource::Resource,
    system::{Commands, Res, ResMut},
};

use crate::app::{
    data::time::Time,
    render::{GpuHandle, SurfaceState},
};

pub const DEBUG_PROFILER_SAMPLE_COUNT: usize = 60;

struct SampleList {
    samples: [Duration; DEBUG_PROFILER_SAMPLE_COUNT],
    index: usize,
    count: usize,
}

impl SampleList {
    fn push(&mut self, duration: Duration) {
        self.samples[self.index] = duration;
        self.index = (self.index + 1) % self.samples.len();
        self.count = (self.count + 1).min(self.samples.len());
    }

    pub fn average(&self) -> Duration {
        if self.count == 0 {
            return Duration::ZERO;
        }

        let sum: Duration = self.samples.iter().take(self.count).sum();
        sum / self.count as u32
    }
}

pub struct DebugProfiler {}

pub struct TimeQuery {
    started: bool,

    query_set: wgpu::QuerySet,
    resolve_buffer: wgpu::Buffer,

    current_sample_index: u32,

    tx: Sender<(u64, u64)>,
    rx: Receiver<(u64, u64)>,
}

impl TimeQuery {
    pub fn new(device: &wgpu::Device) -> Self {
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: None,
            ty: wgpu::QueryType::Timestamp,
            count: 2 * DEBUG_PROFILER_SAMPLE_COUNT as u32, // one for before timestamp, one for after, multiplied by the number of samples
        });

        let resolve_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 2 * 8 * DEBUG_PROFILER_SAMPLE_COUNT as u64, // 2 u64s, 8 bytes each, multiplied by the number of samples
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let (tx, rx) = mpsc::channel();

        Self {
            started: false,
            query_set,
            resolve_buffer,
            current_sample_index: 0,
            tx,
            rx,
        }
    }

    fn get_readback_buffer(device: &wgpu::Device) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 2 * 8, // 2 u64s, 8 bytes each, holds one sample
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn wrap_sample_index(&self) -> u32 {
        self.current_sample_index % DEBUG_PROFILER_SAMPLE_COUNT as u32
    }

    pub fn compute_timestamp_writes(&self) -> wgpu::ComputePassTimestampWrites<'_> {
        wgpu::ComputePassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index: Some(self.wrap_sample_index() * 2),
            end_of_pass_write_index: Some(self.wrap_sample_index() * 2 + 1),
        }
    }

    pub fn render_timestamp_writes(&self) -> wgpu::RenderPassTimestampWrites<'_> {
        wgpu::RenderPassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index: Some(self.wrap_sample_index() * 2),
            end_of_pass_write_index: Some(self.wrap_sample_index() * 2 + 1),
        }
    }

    pub fn write_start_timestamp(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if self.started {
            panic!("Attempted to write a start timestamp more than once");
        }

        self.started = true;
        encoder.write_timestamp(&self.query_set, self.wrap_sample_index() * 2);
    }

    pub fn write_end_timestamp(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if !self.started {
            panic!("Attempted to write an end timestamp without first starting");
        }

        self.started = false;
        encoder.write_timestamp(&self.query_set, self.wrap_sample_index() * 2 + 1);

        // increment the sample index
        self.current_sample_index += 1;
    }

    // call every frame, returns the buffer that the results are stored in
    fn resolve(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) -> wgpu::Buffer {
        let readback_buffer = Self::get_readback_buffer(device);

        encoder.resolve_query_set(
            &self.query_set,
            // read the previous sample only
            ((self.current_sample_index - 1) * 2)..(self.current_sample_index - 1) * 2 + 1,
            &self.resolve_buffer,
            0,
        );

        // Copy the data to a mapped buffer so it can be read on the cpu
        encoder.copy_buffer_to_buffer(
            &self.resolve_buffer,
            0,
            &readback_buffer,
            0,
            readback_buffer.size(),
        );

        readback_buffer
    }

    // should be called every frame
    pub fn begin_read(&self, gpu: &GpuHandle) {
        // resolve with temporary command encoder instead of the frame encoder
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        let buffer = self.resolve(&gpu.device, &mut encoder);
        gpu.queue.submit(std::iter::once(encoder.finish()));

        let tx = self.tx.clone();

        buffer
            .clone()
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| match result {
                Ok(()) => {
                    let view = buffer.slice(..).get_mapped_range();
                    let timestamps: &[u64] = bytemuck::cast_slice(&view);

                    let time_start = timestamps[0];
                    let time_end = timestamps[1];

                    tx.send((time_start, time_end)).unwrap();
                }
                Err(e) => log::error!("Buffer map failed: {}", e),
            });
    }

    pub fn get_results(&self, gpu: &GpuHandle) -> anyhow::Result<Vec<Duration>> {
        let mut results = Vec::new();

        if gpu.device.poll(wgpu::PollType::Poll)?.wait_finished() {
            let (start, end) = self.rx.recv()?;

            let timestamp_period = gpu.queue.get_timestamp_period() as f64;
            let nanoseconds = (end - start) as f64 * timestamp_period;

            results.push(Duration::from_nanos(nanoseconds as u64));
        }

        Ok(results)
    }
}
