use std::{borrow::Cow, sync::Arc};

use bevy_ecs::resource::Resource;
use wgpu::SurfaceError;
use winit::{dpi::PhysicalSize, window::Window};

pub mod debug;

pub const WGPU_FEATURES: wgpu::Features = wgpu::Features::FLOAT32_FILTERABLE
    .union(wgpu::Features::RG11B10UFLOAT_RENDERABLE)
    .union(wgpu::Features::IMMEDIATES)
    .union(wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER)
    .union(wgpu::Features::ADDRESS_MODE_CLAMP_TO_ZERO)
    .union(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
    .union(wgpu::Features::TIMESTAMP_QUERY)
    .union(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS)
    .union(wgpu::Features::VERTEX_WRITABLE_STORAGE)
    .union(wgpu::Features::EXPERIMENTAL_PASSTHROUGH_SHADERS);

pub const WGPU_LIMITS: wgpu::Limits = wgpu::Limits {
    max_immediate_size: 128,
    ..wgpu::Limits::defaults()
};

#[derive(Clone)]
#[allow(unused)]
pub struct GpuHandle {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl GpuHandle {
    pub fn create_shader_module(&self, label: &str, source: Cow<'_, [u32]>) -> wgpu::ShaderModule {
        // #[cfg(debug_assertions)]
        // return self
        //     .device
        //     .create_shader_module(wgpu::ShaderModuleDescriptor {
        //         label: Some(label),
        //         source: wgpu::ShaderSource::SpirV(source),
        //     });

        // // use passthrough shader modules when in release mode so we don't needlessly send spirv shaders through naga
        // #[cfg(not(debug_assertions))]
        unsafe {
            self.device
                .create_shader_module_passthrough(wgpu::ShaderModuleDescriptorPassthrough {
                    label: Some(label),
                    spirv: Some(source),
                    ..Default::default()
                })
        }
    }
}

#[derive(Resource)]
pub struct FrameRecord {
    pub encoder: wgpu::CommandEncoder,
    pub surface_texture: wgpu::SurfaceTexture,

    pub surface_texture_view: wgpu::TextureView,
}

#[derive(Resource)]
pub struct SurfaceState {
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,

    pub viewport_size: PhysicalSize<u32>,
    pub window: Arc<Window>,

    pub gpu: GpuHandle,
}

impl SurfaceState {
    pub async fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let viewport_size = window.inner_size();

        let mut instance_flags = wgpu::InstanceFlags::empty();

        // enable vulkan validation layer in debug builds
        #[cfg(debug_assertions)]
        {
            instance_flags |= wgpu::InstanceFlags::debugging();
        }

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            flags: instance_flags,
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptionsBase {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: WGPU_FEATURES,
                required_limits: WGPU_LIMITS,
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
                experimental_features: unsafe { wgpu::ExperimentalFeatures::enabled() },
            })
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats[0];

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: viewport_size.width,
            height: viewport_size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            desired_maximum_frame_latency: 2,
            view_formats: vec![],
        };

        surface.configure(&device, &config);

        Ok(Self {
            surface,
            config,
            viewport_size,
            window,
            gpu: GpuHandle {
                instance,
                adapter,
                device,
                queue,
            },
        })
    }

    pub fn reconfigure_surface(&self) {
        self.surface.configure(&self.gpu.device, &self.config);
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.viewport_size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.reconfigure_surface();
        }
    }

    pub fn begin_frame(&self) -> Result<FrameRecord, SurfaceError> {
        let encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Frame Encoder"),
            });

        let surface_texture = self.surface.get_current_texture()?;
        let surface_texture_view = surface_texture.texture.create_view(&Default::default());

        Ok(FrameRecord {
            encoder,
            surface_texture,
            surface_texture_view,
        })
    }

    pub fn finish_frame(&self, frame: FrameRecord) {
        self.gpu
            .queue
            .submit(std::iter::once(frame.encoder.finish()));

        frame.surface_texture.present();
    }
}
