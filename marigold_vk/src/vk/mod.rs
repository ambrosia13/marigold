use std::sync::Arc;

use anyhow::anyhow;
use bevy_ecs::resource::Resource;
use itertools::Itertools;
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
            DebugUtilsMessageType, DebugUtilsMessengerCallback, DebugUtilsMessengerCreateInfo,
            ValidationFeatureEnable,
        },
    },
    memory::allocator::StandardMemoryAllocator,
    swapchain::{PresentMode, Surface, Swapchain, SwapchainCreateInfo, SwapchainPresentInfo},
    sync::{self, GpuFuture},
};
use winit::{event_loop::EventLoop, window::Window};

pub mod buffervec;

#[derive(Clone)]
pub struct GpuHandlePreInit {
    library: Arc<VulkanLibrary>,
    instance: Arc<Instance>,
}

#[derive(Resource, Clone)]
pub struct GpuHandle {
    pub library: Arc<VulkanLibrary>,
    pub instance: Arc<Instance>,
    pub physical_device: Arc<PhysicalDevice>,
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,

    pub memory_allocator: Arc<StandardMemoryAllocator>,
    pub cmd_buffer_allocator: Arc<StandardCommandBufferAllocator>,
}

impl GpuHandle {
    pub fn pre_init(event_loop: &EventLoop<()>) -> anyhow::Result<GpuHandlePreInit> {
        let library = unsafe { VulkanLibrary::new() }?;

        let validation_layer_available = library
            .layer_properties()
            .unwrap()
            .any(|l| l.name() == "VK_LAYER_KHRONOS_validation");

        if validation_layer_available {
            log::info!("Validation layers are present; enable them through vkconfig gui");
        } else {
            log::info!("Validation layers are not present");
        }

        let surface_extensions = Surface::required_extensions(event_loop);

        let instance = Instance::new(
            &library,
            &InstanceCreateInfo {
                // allow running on metal devices
                flags: InstanceCreateFlags::ENUMERATE_PORTABILITY,
                enabled_extensions: &InstanceExtensions {
                    khr_surface: true,
                    ext_debug_utils: true, // allows labeling
                    ..surface_extensions
                },
                ..InstanceCreateInfo::application_from_cargo_toml()
            },
        )?;

        log::info!(
            "Created instance, max api version is {}",
            instance.api_version()
        );

        Ok(GpuHandlePreInit { library, instance })
    }

    pub fn new(
        pre_init: GpuHandlePreInit,
        window: Arc<Window>,
    ) -> anyhow::Result<(Self, Arc<Surface>)> {
        let GpuHandlePreInit { library, instance } = pre_init;

        let surface = Surface::from_window(&instance, &window)?;

        log::info!("Created surface");

        let device_extensions = DeviceExtensions {
            // ext_descriptor_indexing: true,
            khr_swapchain: true,
            // ext_descriptor_buffer: true,
            ..DeviceExtensions::empty()
        };

        let device_features = DeviceFeatures {
            descriptor_indexing: true,
            shader_sampled_image_array_non_uniform_indexing: true,

            descriptor_binding_variable_descriptor_count: true,
            runtime_descriptor_array: true,
            buffer_device_address: true,

            dynamic_rendering: true,
            synchronization2: true,

            descriptor_binding_partially_bound: true,

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

        log::info!("Created physical device and selected the queue family index");

        let (device, mut queues) = Device::new(
            &physical_device,
            &DeviceCreateInfo {
                queue_create_infos: &[QueueCreateInfo {
                    queue_family_index,
                    ..Default::default()
                }],
                enabled_extensions: &device_extensions,
                enabled_features: &device_features,
                ..Default::default()
            },
        )?;

        let queue = queues.next().ok_or(anyhow!("no suitable queues found"))?;

        log::info!("Created device and queues");

        let memory_allocator = Arc::new(StandardMemoryAllocator::new(&device, &Default::default()));

        let cmd_buffer_allocator = Arc::new(StandardCommandBufferAllocator::new(
            &device,
            &Default::default(),
        ));

        log::info!("Created memory allocator and command buffer allocator");

        Ok((
            Self {
                library,
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
    ) -> anyhow::Result<(SwapchainCreateInfo<'_>, Format)> {
        let caps = gpu
            .physical_device
            .surface_capabilities(&surface, &Default::default())?;

        let composite_alpha = caps
            .supported_composite_alpha
            .into_iter()
            .next()
            .ok_or(anyhow!("no supported composite alpha modes"))?;

        let formats = gpu
            .physical_device
            .surface_formats(&surface, &Default::default())?;

        let present_modes = gpu
            .physical_device
            .surface_present_modes(&surface, &Default::default())?;

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

        log::info!(
            "Selected surface format is {:?}, selected present mode is {:?}",
            surface_format,
            present_mode
        );

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

        let (swapchain, images) = Swapchain::new(&gpu.device, &surface, &create_info)?;

        let views = images
            .iter()
            .map(|img| ImageView::new_default(img))
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
        log::info!("Recreating swapcahin");

        let (create_info, format) =
            Self::get_swapchain_create_info(&self.gpu, self.surface.clone(), self.window.clone())?;

        let (new_swapchain, new_images) = self.inner.recreate(&create_info)?;

        self.inner = new_swapchain;
        self.images = new_images;

        self.views = self
            .images
            .iter()
            .map(|img| ImageView::new_default(img))
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

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.swapchain_needs_recreate = true; // enqueue it rather than do it immediately
        // self.swapchain.recreate()?;
        // self.swapchain_needs_recreate = false; // clear this flag if it was already set
        } else {
            log::error!("invalid window resize: {:?}, skipping", new_size);
        }
    }

    pub fn begin_frame(&mut self) -> Result<FrameRecord, FrameError> {
        self.previous_frame_end
            .iter_mut()
            .for_each(|f| f.cleanup_finished());

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

        Ok(FrameRecord {
            cmd_builder,
            swapchain_image_index,
            future: acquire_future.boxed(),
        })
    }

    pub fn finish_frame(&mut self, frame: FrameRecord) -> anyhow::Result<()> {
        // we propagate this error because I think this function shouldn't have to worry about general
        // command submission failing, just surface presentation failing?
        let cmd_buffer = frame.cmd_builder.build()?;

        let future = frame
            .future
            // make sure that this runs after the previous frame's commands as well as after the swapchain image is ready
            .join(self.previous_frame_end.take().unwrap())
            // execute the commands recorded over the frame lifetime
            .then_execute(self.gpu.queue.clone(), cmd_buffer)?
            .then_signal_fence()
            .then_swapchain_present(
                self.gpu.queue.clone(),
                SwapchainPresentInfo::new(
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
