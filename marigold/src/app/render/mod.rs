use std::{default::Default, sync::Arc};

use anyhow::anyhow;
use bevy_ecs::resource::Resource;
use vulkano::{
    Validated, VulkanError, VulkanLibrary,
    command_buffer::{
        AutoCommandBufferBuilder, PrimaryAutoCommandBuffer,
        allocator::StandardCommandBufferAllocator,
    },
    device::{
        Device, DeviceCreateInfo, DeviceExtensions, DeviceFeatures, Queue, QueueCreateInfo,
        QueueFlags,
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
    memory::allocator::StandardMemoryAllocator,
    swapchain::{PresentMode, Surface, Swapchain, SwapchainCreateInfo, SwapchainPresentInfo},
    sync::{self, GpuFuture},
};
use winit::{event_loop::EventLoop, window::Window};

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
pub struct GpuHandlePreInit {
    library: Arc<VulkanLibrary>,
    instance: Arc<Instance>,
}

#[derive(Resource, Clone)]
pub struct GpuHandle {
    pub instance: Arc<Instance>,
    pub physical_device: Arc<PhysicalDevice>,
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,

    pub memory_allocator: Arc<StandardMemoryAllocator>,
    pub cmd_buffer_allocator: Arc<StandardCommandBufferAllocator>,
}

impl GpuHandle {
    pub fn pre_init(event_loop: &EventLoop<()>) -> anyhow::Result<GpuHandlePreInit> {
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

        Ok(GpuHandlePreInit { library, instance })
    }

    pub fn new(
        pre_init: GpuHandlePreInit,
        window: Arc<Window>,
    ) -> anyhow::Result<(Self, Arc<Surface>)> {
        let GpuHandlePreInit { instance, .. } = pre_init;

        let surface = Surface::from_window(instance.clone(), window.clone())?;

        let device_extensions = DeviceExtensions {
            khr_swapchain: true,
            ext_descriptor_buffer: true,
            ext_descriptor_indexing: true,
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

        let memory_allocator = Arc::new(StandardMemoryAllocator::new_default(device.clone()));

        let cmd_buffer_allocator = Arc::new(StandardCommandBufferAllocator::new(
            device.clone(),
            Default::default(),
        ));

        Ok((
            Self {
                instance,
                physical_device,
                device,
                queue,
                memory_allocator,
                cmd_buffer_allocator,
            },
            surface,
        ))
    }
}

pub struct SwapchainState {
    pub inner: Arc<Swapchain>,
    pub images: Vec<Arc<Image>>,
    pub views: Vec<Arc<ImageView>>,
    pub format: Format,

    surface: Arc<Surface>,
    window: Arc<Window>,

    gpu: GpuHandle,
}

impl SwapchainState {
    fn get_swapchain_create_info(
        gpu: &GpuHandle,
        surface: Arc<Surface>,
        window: Arc<Window>,
    ) -> anyhow::Result<(SwapchainCreateInfo, Format)> {
        let caps = gpu
            .physical_device
            .surface_capabilities(&surface, Default::default())?;

        let composite_alpha = caps
            .supported_composite_alpha
            .into_iter()
            .next()
            .ok_or(anyhow!("no supported composite alpha modes"))?;

        let formats = gpu
            .physical_device
            .surface_formats(&surface, Default::default())?;

        let present_modes = gpu
            .physical_device
            .surface_present_modes(&surface, Default::default())?;

        let present_mode_priority = [PresentMode::Mailbox, PresentMode::Fifo];

        let present_mode = present_mode_priority
            .iter()
            .find(|pm| present_modes.contains(pm))
            .copied()
            .unwrap(); // safe to unwrap bc fifo is always supported

        let surface_format = formats
            .iter()
            .find(|(f, _)| *f == Format::B8G8R8A8_UNORM)
            .or_else(|| formats.iter().find(|(f, _)| *f == Format::R8G8B8A8_UNORM))
            .unwrap_or(&formats[0])
            .0;

        Ok((
            SwapchainCreateInfo {
                min_image_count: (caps.min_image_count + 1)
                    .min(caps.max_image_count.unwrap_or(u32::MAX)),
                image_format: surface_format,
                image_extent: window.inner_size().into(),
                image_usage: ImageUsage::COLOR_ATTACHMENT,
                present_mode,
                composite_alpha,
                ..Default::default()
            },
            surface_format,
        ))
    }

    pub fn new(
        gpu: &GpuHandle,
        surface: Arc<Surface>,
        window: Arc<Window>,
    ) -> anyhow::Result<Self> {
        let (create_info, format) =
            Self::get_swapchain_create_info(gpu, surface.clone(), window.clone())?;

        let (swapchain, images) = Swapchain::new(gpu.device.clone(), surface.clone(), create_info)?;

        let views = images
            .iter()
            .map(|img| ImageView::new_default(img.clone()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            inner: swapchain,
            images,
            views,
            format,
            surface,
            window,
            gpu: gpu.clone(),
        })
    }

    pub fn recreate(&mut self) -> anyhow::Result<()> {
        let (create_info, format) =
            Self::get_swapchain_create_info(&self.gpu, self.surface.clone(), self.window.clone())?;

        let (new_swapchain, new_images) = self.inner.recreate(create_info)?;

        self.inner = new_swapchain;
        self.images = new_images;

        self.views = self
            .images
            .iter()
            .map(|img| ImageView::new_default(img.clone()))
            .collect::<Result<Vec<_>, _>>()?;

        self.format = format;

        Ok(())
    }
}

// non-send resource
pub struct SurfaceState {
    pub surface: Arc<Surface>,
    pub swapchain: SwapchainState,
    pub window: Arc<Window>,

    pub gpu: GpuHandle,

    swapchain_needs_recreate: bool,
    previous_frame_end: Option<Box<dyn GpuFuture>>,
}

impl SurfaceState {
    pub fn new(
        gpu: &GpuHandle,
        surface: Arc<Surface>,
        window: Arc<Window>,
    ) -> anyhow::Result<Self> {
        let swapchain = SwapchainState::new(gpu, surface.clone(), window.clone())?;

        Ok(Self {
            surface,
            swapchain,
            window,
            gpu: gpu.clone(),
            swapchain_needs_recreate: false,
            previous_frame_end: Some(sync::now(gpu.device.clone()).boxed()),
        })
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) -> anyhow::Result<()> {
        if new_size.width > 0 && new_size.height > 0 {
            self.swapchain_needs_recreate = true; // enqueue it rather than do it immediately
        // self.swapchain.recreate()?;
        // self.swapchain_needs_recreate = false; // clear this flag if it was already set
        } else {
            log::error!("invalid window resize: {:?}, skipping", new_size);
        }

        Ok(())
    }

    pub fn begin_frame(&mut self) -> Result<FrameRecord, FrameError> {
        if self.swapchain_needs_recreate {
            self.swapchain.recreate().map_err(FrameError::Other)?;
            self.swapchain_needs_recreate = false;
        }

        let (swapchain_image_index, suboptimal, acquire_future) =
            match vulkano::swapchain::acquire_next_image(self.swapchain.inner.clone(), None)
                .map_err(Validated::unwrap)
            {
                Ok(r) => r,
                Err(VulkanError::OutOfDate) => {
                    log::warn!("swapchain is out of date, skipping this frame");
                    self.swapchain_needs_recreate = true;
                    return Err(FrameError::SkipFrame);
                }
                Err(e) => return Err(FrameError::Vulkan(e)),
            };

        if suboptimal {
            self.swapchain_needs_recreate = true;
            log::warn!(
                "Suboptimal swapchain image, queueing swapchain recreation and proceeding with frame"
            );
        }

        // basically the vulkano equivalent of wgpu's CommandEncoder
        let cmd_builder = AutoCommandBufferBuilder::primary(
            self.gpu.cmd_buffer_allocator.clone(),
            self.gpu.queue.queue_family_index(),
            vulkano::command_buffer::CommandBufferUsage::OneTimeSubmit,
        )
        .map_err(Validated::unwrap)
        .map_err(FrameError::Vulkan)?;

        let future = self.previous_frame_end.take().unwrap().join(acquire_future);

        Ok(FrameRecord {
            cmd_builder,
            swapchain_image_index,
            future: future.boxed(),
        })
    }

    pub fn finish_frame(&mut self, frame: FrameRecord) -> anyhow::Result<()> {
        // we propagate this error because I think this function shouldn't have to worry about general
        // command submission failing, just surface presentation failing?
        let cmd_buffer = frame.cmd_builder.build()?;

        let future = frame
            .future
            // execute the commands recorded over the frame lifetime
            .then_execute(self.gpu.queue.clone(), cmd_buffer)?
            .then_swapchain_present(
                self.gpu.queue.clone(),
                SwapchainPresentInfo::swapchain_image_index(
                    self.swapchain.inner.clone(),
                    frame.swapchain_image_index,
                ),
            )
            .then_signal_fence_and_flush();

        match future.map_err(Validated::unwrap) {
            Ok(mut future) => {
                future.cleanup_finished();

                self.previous_frame_end = Some(future.boxed());
            }
            Err(VulkanError::OutOfDate) => {
                log::warn!("swapchain is out of date at the end of frame");
                self.swapchain_needs_recreate = true;
                self.previous_frame_end = Some(sync::now(self.gpu.device.clone()).boxed());
            }
            Err(e) => {
                log::warn!("error when flushing future: {}", e);
                self.previous_frame_end = Some(sync::now(self.gpu.device.clone()).boxed());
            }
        }

        Ok(())
    }
}

pub struct FrameRecord {
    pub cmd_builder: AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
    pub swapchain_image_index: u32,
    future: Box<dyn GpuFuture>,
}

#[derive(Debug)]
pub enum FrameError {
    SkipFrame,
    Unrecoverable,
    Vulkan(VulkanError),
    Other(anyhow::Error),
}
