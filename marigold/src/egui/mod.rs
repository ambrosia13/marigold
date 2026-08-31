use std::{num::NonZeroU64, sync::Arc};

use bevy_ecs::{
    resource::Resource,
    system::{NonSendMut, Res},
};
use vulkano::{buffer::Buffer, pipeline::GraphicsPipeline};
use winit::event::WindowEvent;

use crate::vk::{GpuHandle, SurfaceState};

pub struct Egui {
    state: egui_winit::State,

    gpu: GpuHandle,

    pipeline: Arc<GraphicsPipeline>,

    index_buffer: Arc<Buffer>,
    vertex_buffer: Arc<Buffer>,
    index_buffer_address: NonZeroU64,
    vertex_buffer_address: NonZeroU64,
}

impl Egui {
    pub fn on_window_event(&mut self, surface_state: &SurfaceState, event: WindowEvent) {
        self.state.on_window_event(&surface_state.window, &event);
    }
}

struct Painter {
    gpu: GpuHandle,

    pipeline: Arc<GraphicsPipeline>,

    index_buffer: Arc<Buffer>,
    vertex_buffer: Arc<Buffer>,
    index_buffer_address: NonZeroU64,
    vertex_buffer_address: NonZeroU64,
}
