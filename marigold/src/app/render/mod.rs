use std::{borrow::Cow, default::Default, sync::Arc};

use bevy_ecs::resource::Resource;
use vulkano::{
    VulkanLibrary,
    device::{Device, Queue, physical::PhysicalDevice},
    image::{Image, view::ImageView},
    instance::{
        Instance, InstanceCreateFlags, InstanceCreateInfo, InstanceExtensions,
        debug::{
            DebugUtilsMessageSeverity, DebugUtilsMessageType, DebugUtilsMessengerCallback,
            DebugUtilsMessengerCallbackData, DebugUtilsMessengerCallbackLabelIter,
            DebugUtilsMessengerCreateInfo, ValidationFeatureEnable,
        },
    },
    swapchain::{Surface, Swapchain},
};
use winit::{dpi::PhysicalSize, event_loop::EventLoop, window::Window};

use crate::util;

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
    // https://vulkan.gpuinfo.org/displaydevicelimit.php?name=maxBoundDescriptorSets&platform=all
    // safe to increase to 8, keep to 6 for now
    max_bind_groups: 6,
    ..wgpu::Limits::defaults()
};

fn labels_to_string(iter: DebugUtilsMessengerCallbackLabelIter<'_>) -> String {
    iter.map(|l| format!("'{}'", l.label_name))
        .intersperse(", ".into())
        .collect()
}

const fn object_type_to_str(int: i32) -> &'static str {
    match int {
        0 => "UNKNOWN",
        1 => "INSTANCE",
        2 => "PHYSICAL_DEVICE",
        3 => "DEVICE",
        4 => "QUEUE",
        5 => "SEMAPHORE",
        6 => "COMMAND_BUFFER",
        7 => "FENCE",
        8 => "DEVICE_MEMORY",
        9 => "BUFFER",
        10 => "IMAGE",
        11 => "EVENT",
        12 => "QUERY_POOL",
        13 => "BUFFER_VIEW",
        14 => "IMAGE_VIEW",
        15 => "SHADER_MODULE",
        16 => "PIPELINE_CACHE",
        17 => "PIPELINE_LAYOUT",
        18 => "RENDER_PASS",
        19 => "PIPELINE",
        20 => "DESCRIPTOR_SET_LAYOUT",
        21 => "SAMPLER",
        22 => "DESCRIPTOR_POOL",
        23 => "DESCRIPTOR_SET",
        24 => "FRAMEBUFFER",
        25 => "COMMAND_POOL",
        _ => "UNKNOWN_OBJECT_TYPE",
    }
}

fn debug_messenger(
    severity: DebugUtilsMessageSeverity,
    ty: DebugUtilsMessageType,
    data: DebugUtilsMessengerCallbackData<'_>,
) {
    match severity {
        DebugUtilsMessageSeverity::VERBOSE => {}
        DebugUtilsMessageSeverity::INFO => {}
        _ => {
            // error or warning, so log it

            let mut header = String::from("Vulkan debug message");

            if ty.contains(DebugUtilsMessageType::GENERAL) {
                header += " [General] ";
            }

            if ty.contains(DebugUtilsMessageType::PERFORMANCE) {
                header += " [Performance] ";
            }

            if ty.contains(DebugUtilsMessageType::VALIDATION) {
                header += " [Validation] ";
            }

            let message = format!(
                "{}:\n\
                - Queues: {}\n\
                - Command buffers: {}\n\
                - Objects: {}\n\
                - ID: ({}) {}\n\
                - Message: '{}'",
                header,
                labels_to_string(data.queue_labels),
                labels_to_string(data.cmd_buf_labels),
                data.objects
                    .map(|o| format!(
                        "'{}' ({}, 0x{:x})",
                        o.object_name.unwrap_or("no label"),
                        object_type_to_str(o.object_type.as_raw()),
                        o.object_handle
                    ))
                    .intersperse(", ".into())
                    .collect::<String>(),
                data.message_id_number,
                data.message_id_name.unwrap_or(""),
                data.message
            );

            if severity == DebugUtilsMessageSeverity::WARNING {
                log::warn!("{}", message);
            } else {
                log::error!("{}", message);
            }
        }
    }
}

#[derive(Clone)]
#[allow(unused)]
pub struct GpuHandle {
    pub instance: Arc<Instance>,
    pub physical_device: Arc<PhysicalDevice>,
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
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
    pub surface: Arc<Surface>,

    pub swapchain: Arc<Swapchain>,
    pub swapchain_images: Vec<Arc<Image>>,
    pub swapchain_views: Vec<Arc<ImageView>>,

    pub viewport_size: PhysicalSize<u32>,
    pub window: Arc<Window>,

    pub gpu: GpuHandle,
}

pub struct SurfaceStatePreInit {
    library: Arc<VulkanLibrary>,
    instance: Arc<Instance>,
}

impl SurfaceState {
    pub fn pre_init(event_loop: &EventLoop<()>) -> SurfaceStatePreInit {
        let library = VulkanLibrary::new().expect("couldn't load vulkan library");

        let validation_layer = library
            .layer_properties()
            .unwrap()
            .map(|layer| layer.name().to_string())
            .find(|layer| layer == "VK_LAYER_KHRONOS_validation")
            .filter(|_| !util::get_env_flag("DISABLE_VALIDATION_LAYERS"));

        let validation_enabled = validation_layer.is_some();

        if cfg!(debug_assertions) && !validation_enabled {
            log::warn!(
                "Running a debug build, but no vulkan validation layer installed, so important gpu validation may be missing"
            );
        }

        let surface_extensions = Surface::required_extensions(event_loop).unwrap();

        let user_callback = unsafe { DebugUtilsMessengerCallback::new(debug_messenger) };

        // this will only be used in debug builds
        let debug_messenger_create_info = DebugUtilsMessengerCreateInfo {
            message_type: DebugUtilsMessageType::GENERAL
                | DebugUtilsMessageType::PERFORMANCE
                | DebugUtilsMessageType::VALIDATION,
            ..DebugUtilsMessengerCreateInfo::user_callback(user_callback)
        };

        let instance = Instance::new(
            library.clone(),
            InstanceCreateInfo {
                // allow running on metal devices
                flags: InstanceCreateFlags::ENUMERATE_PORTABILITY,
                enabled_layers: validation_layer.into_iter().collect(),
                enabled_extensions: InstanceExtensions {
                    khr_surface: true,
                    ext_debug_utils: cfg!(debug_assertions),
                    ..surface_extensions
                },
                debug_utils_messengers: if cfg!(debug_assertions) {
                    vec![debug_messenger_create_info.clone()]
                } else {
                    vec![]
                },
                enabled_validation_features: if validation_enabled {
                    vec![
                        ValidationFeatureEnable::BestPractices,
                        ValidationFeatureEnable::SynchronizationValidation,
                    ]
                } else {
                    vec![]
                },
                disabled_validation_features: vec![],
                ..InstanceCreateInfo::application_from_cargo_toml()
            },
        )
        .expect("couldn't create vulkan instance");

        SurfaceStatePreInit { library, instance }

        // let mut instance_flags = wgpu::InstanceFlags::empty();

        // // enable vulkan validation layer in debug builds
        // #[cfg(debug_assertions)]
        // {
        //     use crate::util::get_env_flag;

        //     if get_env_flag("DISABLE_VALIDATION_LAYERS") {
        //         // enable debug info, but not full validation
        //         instance_flags |= wgpu::InstanceFlags::DEBUG;
        //     } else {
        //         instance_flags |= wgpu::InstanceFlags::debugging();
        //     }
        // }

        // wgpu::Instance::new(wgpu::InstanceDescriptor {
        //     backends: wgpu::Backends::VULKAN,
        //     flags: instance_flags,
        //     ..wgpu::InstanceDescriptor::new_with_display_handle(Box::new(
        //         event_loop.owned_display_handle(),
        //     ))
        // })
    }

    pub async fn new(pre_init: SurfaceStatePreInit, window: Arc<Window>) -> anyhow::Result<Self> {
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
            desired_maximum_frame_latency: 3,
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
