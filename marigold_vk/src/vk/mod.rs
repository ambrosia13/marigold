use std::{any::Any, sync::Arc};

use anyhow::anyhow;
use bevy_ecs::resource::Resource;
use vulkano::{
    Validated, VulkanError, VulkanLibrary,
    command_buffer::{
        AutoCommandBufferBuilder, CommandBufferBeginInfo, CommandBufferLevel,
        CommandBufferSubmitInfo, CommandBufferUsage, PrimaryAutoCommandBuffer,
        RecordingCommandBuffer, SemaphoreSubmitInfo, SubmitInfo,
        allocator::{CommandBufferAllocator, StandardCommandBufferAllocator},
        raw::{DependencyInfo, ImageMemoryBarrier, RenderingAttachmentInfo, RenderingInfo},
    },
    device::{
        Device, DeviceCreateInfo, DeviceExtensions, DeviceFeatures, Queue, QueueCreateInfo,
        QueueFlags,
        physical::{PhysicalDevice, PhysicalDeviceType},
    },
    format::{ClearValue, Format},
    image::{Image, ImageAspects, ImageLayout, ImageSubresourceRange, ImageUsage, view::ImageView},
    instance::{Instance, InstanceCreateFlags, InstanceCreateInfo, InstanceExtensions},
    memory::allocator::StandardMemoryAllocator,
    render_pass::{AttachmentLoadOp, AttachmentStoreOp},
    swapchain::{
        AcquireNextImageInfo, PresentInfo, PresentMode, SemaphorePresentInfo, Surface, Swapchain,
        SwapchainAcquireFuture, SwapchainCreateInfo, SwapchainPresentInfo,
    },
    sync::{
        self, AccessFlags, GpuFuture, MemoryBarrier, PipelineStages,
        fence::{Fence, FenceCreateFlags, FenceCreateInfo},
        future::FenceSignalFuture,
        semaphore::{Semaphore, SemaphoreCreateInfo},
    },
};
use winit::{event_loop::EventLoop, window::Window};

pub mod buffervec;

pub const FRAMES_IN_FLIGHT: usize = 3;

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

        log::info!("Created instance");

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
        log::info!(
            "Using {}; {:?}; Vulkan {}",
            physical_device.properties().device_name,
            physical_device.properties().device_type,
            physical_device.properties().api_version
        );

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

        log::info!("Created memory allocator");

        Ok((
            Self {
                library,
                instance,
                physical_device,
                device,
                queue,
                memory_allocator,
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

    // one for each swapchain image
    pub render_semaphores: Vec<Arc<Semaphore>>, // used so the gpu doesn't present the swapchain image until the cmd execution is done

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
                // swapchain images is one more than frames in flight
                min_image_count: (FRAMES_IN_FLIGHT as u32 + 1).clamp(
                    caps.min_image_count,
                    caps.max_image_count.unwrap_or(u32::MAX),
                ),
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

        let render_semaphores = (0..images.len())
            .map(|i| {
                let semaphore =
                    Arc::new(Semaphore::new(&gpu.device, &SemaphoreCreateInfo::default()).unwrap());

                unsafe {
                    gpu.device
                        .set_debug_utils_object_name(
                            &semaphore,
                            Some(&format!("Render Semaphore {}", i)),
                        )
                        .unwrap()
                };

                semaphore
            })
            .collect();

        Ok(Self {
            inner: swapchain,
            images,
            views,
            format,
            surface,
            window,
            render_semaphores,
            gpu: gpu.clone(),
        })
    }

    pub fn recreate(
        &mut self,
        deletion_queue: &mut Vec<Arc<dyn Any + Send + Sync>>,
    ) -> anyhow::Result<()> {
        let (create_info, format) =
            Self::get_swapchain_create_info(&self.gpu, self.surface.clone(), self.window.clone())?;

        log::info!(
            "Recreating swapcahin; new size is {}x{}",
            create_info.image_extent[0],
            create_info.image_extent[1]
        );

        let (new_swapchain, new_images) = self.inner.recreate(&create_info)?;

        // queue the previous resources for deletion
        for view in &self.views {
            deletion_queue.push(view.clone());
        }
        deletion_queue.push(self.inner.clone());

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

    pub frame_in_flight_index: usize,

    // one for each frame in flight
    pub acquire_semaphores: [Arc<Semaphore>; FRAMES_IN_FLIGHT], // used so the gpu doesn't begin executing cmds until the swapchain image is available
    pub submit_fences: [Arc<Fence>; FRAMES_IN_FLIGHT], // used so the cpu does not render more than FRAMES_IN_FLIGHT frames ahead of the gpu

    pub cmd_buffer_allocators: [Arc<StandardCommandBufferAllocator>; FRAMES_IN_FLIGHT],
    pub deletion_queues: [Vec<Arc<dyn Any + Send + Sync>>; FRAMES_IN_FLIGHT],

    swapchain_needs_recreate: bool,
}

impl SurfaceState {
    pub fn new(
        gpu: &GpuHandle,
        surface: Arc<Surface>,
        window: Arc<Window>,
    ) -> anyhow::Result<Self> {
        let swapchain = SwapchainState::new(gpu, surface.clone(), window.clone())?;

        let acquire_semaphores = std::array::from_fn(|i| {
            let semaphore =
                Arc::new(Semaphore::new(&gpu.device, &SemaphoreCreateInfo::default()).unwrap());

            unsafe {
                gpu.device
                    .set_debug_utils_object_name(
                        &semaphore,
                        Some(&format!("Acquire Semaphore {}", i)),
                    )
                    .unwrap()
            };

            semaphore
        });

        let submit_fences = std::array::from_fn(|i| {
            let fence = Arc::new(
                Fence::new(
                    &gpu.device,
                    &FenceCreateInfo {
                        // create in the signaled state so the first frame knows not to wait on anything
                        flags: FenceCreateFlags::SIGNALED,
                        ..Default::default()
                    },
                )
                .unwrap(),
            );

            unsafe {
                gpu.device
                    .set_debug_utils_object_name(&fence, Some(&format!("Submit Fence {}", i)))
                    .unwrap()
            };

            fence
        });

        let cmd_buffer_allocators = std::array::from_fn(|_| {
            Arc::new(StandardCommandBufferAllocator::new(
                &gpu.device,
                &Default::default(),
            ))
        });

        log::info!("created command buffer allocators for each frame");

        let deletion_queues = std::array::from_fn(|_| vec![]);

        Ok(Self {
            surface,
            swapchain,
            window,
            gpu: gpu.clone(),
            frame_in_flight_index: 0,
            acquire_semaphores,
            submit_fences,
            cmd_buffer_allocators,
            deletion_queues,
            swapchain_needs_recreate: false,
        })
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.swapchain_needs_recreate = true; // enqueue it rather than do it immediately
        } else {
            log::error!("invalid window resize: {:?}, skipping", new_size);
        }
    }

    pub fn begin_frame(&mut self) -> Result<FrameRecord, FrameError> {
        // wait until the previous cycle's fence is signaled, so we don't render too far ahead
        self.submit_fences[self.frame_in_flight_index]
            .wait(None)
            .unwrap();

        // reset the fence so it can be signaled again later
        unsafe {
            self.submit_fences[self.frame_in_flight_index]
                .reset()
                .unwrap()
        };

        // clear previous deletion queue cycle
        self.deletion_queues[self.frame_in_flight_index].clear();

        if self.swapchain_needs_recreate {
            self.swapchain
                .recreate(&mut self.deletion_queues[self.frame_in_flight_index])
                .map_err(FrameError::Other)?;
            self.swapchain_needs_recreate = false;
        }

        let acquire_info = AcquireNextImageInfo {
            timeout: None,
            semaphore: Some(&self.acquire_semaphores[self.frame_in_flight_index]),
            fence: None,
            ..Default::default()
        };

        let (swapchain_image_index, suboptimal) =
            match unsafe { self.swapchain.inner.acquire_next_image(&acquire_info) } {
                Ok(aq_img) => (aq_img.image_index, aq_img.is_suboptimal),
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

        // initialize the command buffer with a clear command
        let mut cmd_buffer = RecordingCommandBuffer::new(
            &self.cmd_buffer_allocators[self.frame_in_flight_index],
            self.gpu.queue.queue_family_index(),
            CommandBufferLevel::Primary,
            &CommandBufferBeginInfo {
                usage: CommandBufferUsage::OneTimeSubmit,
                ..Default::default()
            },
        )
        .map_err(FrameError::Vulkan)?;

        unsafe {
            cmd_buffer.pipeline_barrier(&DependencyInfo {
                image_memory_barriers: &[ImageMemoryBarrier {
                    src_stages: PipelineStages::TOP_OF_PIPE,
                    src_access: AccessFlags::empty(),

                    dst_stages: PipelineStages::COLOR_ATTACHMENT_OUTPUT,
                    dst_access: AccessFlags::COLOR_ATTACHMENT_WRITE,

                    old_layout: ImageLayout::Undefined,
                    new_layout: ImageLayout::ColorAttachmentOptimal,

                    subresource_range: ImageSubresourceRange {
                        aspects: ImageAspects::COLOR,
                        ..Default::default()
                    },
                    ..ImageMemoryBarrier::new(
                        &self.swapchain.images[swapchain_image_index as usize],
                    )
                }],
                ..Default::default()
            });

            cmd_buffer.begin_rendering(&RenderingInfo {
                color_attachments: &[Some(RenderingAttachmentInfo {
                    load_op: AttachmentLoadOp::Clear,
                    store_op: AttachmentStoreOp::Store,
                    clear_value: Some(ClearValue::Float([1.0, 0.5, 0.2, 1.0])),
                    ..RenderingAttachmentInfo::new(
                        &self.swapchain.views[swapchain_image_index as usize],
                    )
                })],
                depth_attachment: None,
                stencil_attachment: None,
                ..Default::default()
            });

            cmd_buffer.end_rendering();
        }

        Ok(FrameRecord {
            cmd_buffer,
            flight_index: self.frame_in_flight_index,
            swapchain_image_index,
        })
    }

    pub fn finish_frame(&mut self, mut frame: FrameRecord) -> anyhow::Result<()> {
        // transition back into the present optimal layout
        unsafe {
            frame.cmd_buffer.pipeline_barrier(&DependencyInfo {
                image_memory_barriers: &[ImageMemoryBarrier {
                    src_stages: PipelineStages::COLOR_ATTACHMENT_OUTPUT,
                    src_access: AccessFlags::COLOR_ATTACHMENT_WRITE,

                    dst_stages: PipelineStages::BOTTOM_OF_PIPE,
                    dst_access: AccessFlags::empty(),

                    old_layout: ImageLayout::ColorAttachmentOptimal,
                    new_layout: ImageLayout::PresentSrc,

                    subresource_range: ImageSubresourceRange {
                        aspects: ImageAspects::COLOR,
                        ..Default::default()
                    },
                    ..ImageMemoryBarrier::new(
                        &self.swapchain.images[frame.swapchain_image_index as usize],
                    )
                }],
                ..Default::default()
            })
        };

        // we propagate this error because I think this function shouldn't have to worry about general
        // command submission failing, just surface presentation failing?
        let cmd_buffer = unsafe { frame.cmd_buffer.end() }?;

        match self.gpu.queue.with(|mut q| {
            let submit_info = SubmitInfo {
                wait_semaphores: &[SemaphoreSubmitInfo::new(
                    &self.acquire_semaphores[frame.flight_index],
                )],
                command_buffers: &[CommandBufferSubmitInfo::new(&cmd_buffer)],
                signal_semaphores: &[SemaphoreSubmitInfo::new(
                    &self.swapchain.render_semaphores[frame.swapchain_image_index as usize],
                )],
                ..Default::default()
            };

            let present_info = PresentInfo {
                wait_semaphores: vec![SemaphorePresentInfo::new(
                    self.swapchain.render_semaphores[frame.swapchain_image_index as usize].clone(),
                )],
                swapchain_infos: vec![SwapchainPresentInfo::new(
                    self.swapchain.inner.clone(),
                    frame.swapchain_image_index,
                )],
                ..Default::default()
            };

            unsafe {
                q.submit(
                    &[submit_info],
                    Some(&self.submit_fences[frame.flight_index]),
                )
                .and_then(|_| {
                    self.window.pre_present_notify();
                    q.present(&present_info)
                })
                .and_then(|mut suboptimal| suboptimal.next().unwrap()) // we can unwrap because we know we're presenting to only one swapchain
                .inspect(|_| {
                    // advance the frame in flight if those were successful
                    self.frame_in_flight_index = (frame.flight_index + 1) % FRAMES_IN_FLIGHT
                })
            }
        }) {
            Ok(suboptimal) if suboptimal && !self.swapchain_needs_recreate => {
                log::info!("suboptimal present, marking swapchain for recreation");
                self.swapchain_needs_recreate = true;
            }
            Ok(_) => {}
            Err(VulkanError::OutOfDate) => {
                log::warn!("swapchain is out of date at the end of frame");
                self.swapchain_needs_recreate = true;
            }
            Err(e) => Err(e)?,
        }

        Ok(())
    }
}

pub struct FrameRecord {
    pub cmd_buffer: RecordingCommandBuffer,
    pub flight_index: usize,
    pub swapchain_image_index: u32,
}

#[derive(Debug)]
pub enum FrameError {
    SkipFrame,
    Unrecoverable,
    Vulkan(VulkanError),
    Other(anyhow::Error),
}
