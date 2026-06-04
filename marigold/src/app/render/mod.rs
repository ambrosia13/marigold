use std::{borrow::Cow, default::Default, sync::Arc};

use bevy_ecs::resource::Resource;
use winit::{dpi::PhysicalSize, event_loop::EventLoop, window::Window};

#[expect(unused)]
pub mod debug;
pub mod ecs;

pub const WGPU_FEATURES: wgpu::Features = wgpu::Features::FLOAT32_FILTERABLE
    .union(wgpu::Features::RG11B10UFLOAT_RENDERABLE)
    .union(wgpu::Features::IMMEDIATES)
    .union(wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER)
    .union(wgpu::Features::ADDRESS_MODE_CLAMP_TO_ZERO)
    .union(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
    .union(wgpu::Features::TIMESTAMP_QUERY)
    .union(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS)
    .union(wgpu::Features::VERTEX_WRITABLE_STORAGE)
    .union(wgpu::Features::PASSTHROUGH_SHADERS)
    .union(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM);

pub const WGPU_LIMITS: wgpu::Limits = wgpu::Limits {
    max_immediate_size: 128,
    max_color_attachment_bytes_per_sample: 64,
    // https://vulkan.gpuinfo.org/displaydevicelimit.php?name=maxStorageBufferRange&platform=all
    max_storage_buffer_binding_size: 1073741820,
    // https://vulkan.gpuinfo.org/displaycoreproperty.php?core=1.3&name=maxBufferSize&platform=all
    max_buffer_size: 2147483648,
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

    // pub fn create_shader_module_with_specialization_constants(
    //     &self,
    //     label: &str,
    //     source: Cow<'_, [u32]>,
    //     constants: &[(u32, &[u8])],
    // ) -> wgpu::ShaderModule {
    //     let vk_device = unsafe { self.device.as_hal::<wgpu::hal::api::Vulkan>() }
    //         .expect("vulkan backend should always be selected");

    //     //wgpu::hal::vulkan::ShaderModule::Raw(())

    //     todo!()
    // }
}

pub enum FrameError {
    NeedsReconfigure,
    SkipFrame,
    Unrecoverable,
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
    pub fn create_instance(event_loop: &EventLoop<()>) -> wgpu::Instance {
        let mut instance_flags = wgpu::InstanceFlags::empty();

        // enable vulkan validation layer in debug builds
        #[cfg(debug_assertions)]
        {
            use crate::util::get_env_flag;

            if get_env_flag("DISABLE_VALIDATION_LAYERS") {
                // enable debug info, but not full validation
                instance_flags |= wgpu::InstanceFlags::DEBUG;
            } else {
                instance_flags |= wgpu::InstanceFlags::debugging();
            }
        }

        wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            flags: instance_flags,
            ..wgpu::InstanceDescriptor::new_with_display_handle(Box::new(
                event_loop.owned_display_handle(),
            ))
        })
    }

    pub async fn new(instance: wgpu::Instance, window: Arc<Window>) -> anyhow::Result<Self> {
        let viewport_size = window.inner_size();

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptionsBase {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;

        dbg!(adapter.limits());

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
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|&&s| {
                s == wgpu::TextureFormat::Bgra8Unorm || s == wgpu::TextureFormat::Rgba8Unorm
            })
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: viewport_size.width,
            height: viewport_size.height,
            // prefer mailbox, otherwise fallback to fifo
            present_mode: surface_caps
                .present_modes
                .iter()
                .find(|&&p| p == wgpu::PresentMode::Mailbox)
                .copied()
                .unwrap_or(wgpu::PresentMode::Fifo),
            alpha_mode: surface_caps.alpha_modes[0],
            desired_maximum_frame_latency: 2,
            view_formats: vec![],
        };

        surface.configure(&device, &config);

        log::info!("initial surface configuration: {:#?}", config);

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

    pub fn begin_frame(&self) -> Result<FrameRecord, FrameError> {
        let encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Frame Encoder"),
            });

        let current_surface_texture = self.surface.get_current_texture();

        let surface_texture = match current_surface_texture {
            wgpu::CurrentSurfaceTexture::Success(surface_texture) => surface_texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(_surface_texture) => {
                log::warn!("surface was suboptimal, reconfiguring surface and skipping this frame");
                return Err(FrameError::NeedsReconfigure);
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                log::warn!("surface was outdated, reconfiguring surface and skipping this frame");
                return Err(FrameError::NeedsReconfigure);
            }
            wgpu::CurrentSurfaceTexture::Timeout => {
                log::warn!("surface timed out, skipping frame");
                return Err(FrameError::SkipFrame);
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                return Err(FrameError::SkipFrame);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                log::error!("surface or device was lost, treating as unrecoverable error");
                return Err(FrameError::Unrecoverable);
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                log::error!("uncaught validation error, treating as unrecoverable error");
                return Err(FrameError::Unrecoverable);
            }
        };

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
