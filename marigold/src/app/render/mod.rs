use std::{borrow::Cow, default::Default, sync::Arc};

use anyhow::anyhow;
use bevy_ecs::resource::Resource;
use vulkano::{
    Validated, VulkanError, VulkanLibrary,
    command_buffer::{AutoCommandBufferBuilder, PrimaryAutoCommandBuffer},
    device::{
        Device, DeviceCreateInfo, DeviceExtensions, DeviceFeatures, DeviceProperties, Queue,
        QueueCreateFlags, QueueCreateInfo, QueueFlags,
        physical::{PhysicalDevice, PhysicalDeviceType},
    },
    format::Format,
    image::{Image, ImageUsage, view::ImageView},
    instance::{
        Instance, InstanceCreateFlags, InstanceCreateInfo, InstanceExtensions,
        debug::{
            DebugUtilsMessageSeverity, DebugUtilsMessageType, DebugUtilsMessengerCallback,
            DebugUtilsMessengerCallbackData, DebugUtilsMessengerCallbackLabelIter,
            DebugUtilsMessengerCreateInfo, ValidationFeatureEnable,
        },
    },
    swapchain::{
        AcquireNextImageInfo, PresentMode, Surface, Swapchain, SwapchainAcquireFuture,
        SwapchainCreateInfo,
    },
    sync::{GpuFuture, future::FenceSignalFuture},
};
use winit::{dpi::PhysicalSize, event_loop::EventLoop, window::Window};

use crate::util;

#[expect(unused)]
pub mod debug;
pub mod ecs;

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
        _ => "<unknown>",
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
    Other(VulkanError),
}

pub struct FrameRecord {
    pub builder: AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
    pub acquire_future: SwapchainAcquireFuture,
    pub image_index: u32,
}

struct FrameContext {
    fence: Option<Arc<dyn GpuFuture + Send + Sync + 'static>>,
}

#[derive(Resource)]
pub struct SurfaceState {
    pub surface: Arc<Surface>,

    pub swapchain: Arc<Swapchain>,
    pub swapchain_images: Vec<Arc<Image>>,
    pub swapchain_views: Vec<Arc<ImageView>>,

    pub frames_in_flight: Vec<FrameContext>,
    pub current_frame: usize,

    pub viewport_size: PhysicalSize<u32>,
    pub window: Arc<Window>,

    pub gpu: GpuHandle,
}

pub struct SurfaceStatePreInit {
    library: Arc<VulkanLibrary>,
    instance: Arc<Instance>,
}

impl SurfaceState {
    pub fn pre_init(event_loop: &EventLoop<()>) -> anyhow::Result<SurfaceStatePreInit> {
        let library = VulkanLibrary::new()?;

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

        let surface_extensions = Surface::required_extensions(event_loop)?;

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
        )?;

        Ok(SurfaceStatePreInit { library, instance })
    }

    fn score_physical_device(pd: &PhysicalDevice) {}

    pub fn new(pre_init: SurfaceStatePreInit, window: Arc<Window>) -> anyhow::Result<Self> {
        let SurfaceStatePreInit { instance, .. } = pre_init;

        let surface = Surface::from_window(instance.clone(), window.clone())?;

        let device_extensions = DeviceExtensions {
            khr_swapchain: true,
            ext_descriptor_buffer: true,
            ..DeviceExtensions::empty()
        };

        let device_features = DeviceFeatures {
            dynamic_rendering: true,
            synchronization2: true,
            buffer_device_address: true,

            descriptor_indexing: true,
            runtime_descriptor_array: true,
            descriptor_binding_partially_bound: true,
            descriptor_binding_variable_descriptor_count: true,

            shader_storage_image_read_without_format: true,
            shader_storage_image_write_without_format: true,

            ..Default::default()
        };

        let (physical_device, queue_family_index) = instance
            .enumerate_physical_devices()?
            .filter(|pd| pd.supported_extensions().contains(&device_extensions))
            .filter(|pd| pd.supported_features().contains(&device_features))
            .filter_map(|pd| {
                pd.queue_family_properties()
                    .iter()
                    .enumerate()
                    .position(|(i, q)| {
                        q.queue_flags.contains(
                            QueueFlags::GRAPHICS | QueueFlags::COMPUTE | QueueFlags::TRANSFER,
                        ) && pd.surface_support(i as u32, &surface).unwrap_or(false)
                    })
                    .map(|q| (pd, q as u32))
            })
            .min_by_key(|(pd, _)| match pd.properties().device_type {
                PhysicalDeviceType::DiscreteGpu => 0,
                PhysicalDeviceType::IntegratedGpu => 1,
                PhysicalDeviceType::VirtualGpu => 2,
                PhysicalDeviceType::Cpu => 3,
                _ => 4,
            })
            .ok_or(anyhow!("no suitable physical devices found"))?;

        let (device, mut queues) = Device::new(
            physical_device.clone(),
            DeviceCreateInfo {
                queue_create_infos: vec![QueueCreateInfo {
                    queue_family_index,
                    ..Default::default()
                }],
                enabled_extensions: device_extensions,
                enabled_features: device_features,
                ..Default::default()
            },
        )?;

        let queue = queues.next().ok_or(anyhow!("no suitable queues found"))?;
        let (swapchain, swapchain_images, swapchain_views) = Self::create_swapchain(
            physical_device.clone(),
            device.clone(),
            surface.clone(),
            window.clone(),
        )?;

        let num_frames_in_flight = 3;

        Ok(Self {
            surface,
            swapchain,
            swapchain_images,
            swapchain_views,
            viewport_size: window.inner_size(),
            window,
            gpu: GpuHandle {
                instance,
                physical_device,
                device,
                queue,
            },
        })
    }

    fn get_swapchain_create_info(
        physical_device: Arc<PhysicalDevice>,
        surface: Arc<Surface>,
        window: Arc<Window>,
    ) -> anyhow::Result<SwapchainCreateInfo> {
        let caps = physical_device
            .surface_capabilities(&surface, Default::default())
            .expect("couldn't get surface capabililities");

        let composite_alpha = caps
            .supported_composite_alpha
            .into_iter()
            .next()
            .ok_or(anyhow!("no supported composite alpha modes"))?;

        let formats = physical_device.surface_formats(&surface, Default::default())?;

        let present_modes = physical_device.surface_present_modes(&surface, Default::default())?;

        let present_mode = if present_modes.contains(&PresentMode::Mailbox) {
            PresentMode::Mailbox
        } else {
            PresentMode::Fifo
        };

        let surface_format = formats
            .iter()
            .find(|(f, _)| *f == Format::B8G8R8A8_UNORM)
            .or_else(|| formats.iter().find(|(f, _)| *f == Format::R8G8B8A8_UNORM))
            .unwrap_or(&formats[0])
            .0;

        Ok(SwapchainCreateInfo {
            min_image_count: (caps.min_image_count + 1)
                .min(caps.max_image_count.unwrap_or(u32::MAX)),
            image_format: surface_format,
            image_extent: window.inner_size().into(),
            image_usage: ImageUsage::COLOR_ATTACHMENT,
            present_mode,
            composite_alpha,
            ..Default::default()
        })
    }

    fn create_swapchain(
        physical_device: Arc<PhysicalDevice>,
        device: Arc<Device>,
        surface: Arc<Surface>,
        window: Arc<Window>,
    ) -> anyhow::Result<(Arc<Swapchain>, Vec<Arc<Image>>, Vec<Arc<ImageView>>)> {
        let (swapchain, images) = Swapchain::new(
            device.clone(),
            surface.clone(),
            Self::get_swapchain_create_info(physical_device, surface, window)?,
        )?;

        let image_views = images
            .iter()
            .map(|img| ImageView::new_default(img.clone()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok((swapchain, images, image_views))
    }

    pub fn recreate_swapchain(&mut self) -> anyhow::Result<()> {
        let (new_swapchain, new_images) =
            self.swapchain.recreate(Self::get_swapchain_create_info(
                self.gpu.physical_device.clone(),
                self.surface.clone(),
                self.window.clone(),
            )?)?;

        self.swapchain = new_swapchain;
        self.swapchain_images = new_images;

        self.swapchain_views = self
            .swapchain_images
            .iter()
            .map(|img| ImageView::new_default(img.clone()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(())
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) -> anyhow::Result<()> {
        if new_size.width > 0 && new_size.height > 0 {
            self.viewport_size = new_size;
            self.recreate_swapchain()?;
        }

        Ok(())
    }

    pub fn begin_frame(&self) -> Result<FrameRecord, FrameError> {
        let (swapchain_index, acquire_future) =
            match vulkano::swapchain::acquire_next_image(self.swapchain.clone(), None)
                .map_err(Validated::unwrap)
            {
                Ok((idx, suboptimal, future)) => {
                    if suboptimal {
                        log::warn!(
                            "surface was suboptimal, reconfiguring surface and skipping this frame"
                        );
                        return Err(FrameError::NeedsReconfigure);
                    } else {
                        (idx, future)
                    }
                }
                Err(VulkanError::OutOfDate) => {
                    log::warn!(
                        "surface was outdated, reconfiguring surface and skipping this frame"
                    );
                    return Err(FrameError::NeedsReconfigure);
                }
                Err(e) => return Err(FrameError::Other(e)),
            };

        Ok(FrameRecord {
            builder: todo!(),
            acquire_future,
            image_index: todo!(),
        })

        // let encoder = self
        //     .gpu
        //     .device
        //     .create_command_encoder(&wgpu::CommandEncoderDescriptor {
        //         label: Some("Frame Encoder"),
        //     });

        // let current_surface_texture = self.surface.get_current_texture();

        // let surface_texture = match current_surface_texture {
        //     wgpu::CurrentSurfaceTexture::Success(surface_texture) => surface_texture,
        //     wgpu::CurrentSurfaceTexture::Suboptimal(_surface_texture) => {
        //         log::warn!("surface was suboptimal, reconfiguring surface and skipping this frame");
        //         return Err(FrameError::NeedsReconfigure);
        //     }
        //     wgpu::CurrentSurfaceTexture::Outdated => {
        //         log::warn!("surface was outdated, reconfiguring surface and skipping this frame");
        //         return Err(FrameError::NeedsReconfigure);
        //     }
        //     wgpu::CurrentSurfaceTexture::Timeout => {
        //         log::warn!("surface timed out, skipping frame");
        //         return Err(FrameError::SkipFrame);
        //     }
        //     wgpu::CurrentSurfaceTexture::Occluded => {
        //         return Err(FrameError::SkipFrame);
        //     }
        //     wgpu::CurrentSurfaceTexture::Lost => {
        //         log::error!("surface or device was lost, treating as unrecoverable error");
        //         return Err(FrameError::Unrecoverable);
        //     }
        //     wgpu::CurrentSurfaceTexture::Validation => {
        //         log::error!("uncaught validation error, treating as unrecoverable error");
        //         return Err(FrameError::Unrecoverable);
        //     }
        // };

        // let surface_texture_view = surface_texture.texture.create_view(&Default::default());

        // Ok(FrameRecord {
        //     encoder,
        //     surface_texture,
        //     surface_texture_view,
        // })
    }

    pub fn finish_frame(&self, frame: FrameRecord) {
        // self.gpu
        //     .queue
        //     .submit(std::iter::once(frame.encoder.finish()));

        // frame.surface_texture.present();
    }
}
