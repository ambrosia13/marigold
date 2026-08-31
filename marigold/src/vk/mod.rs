use std::{any::Any, sync::Arc};

use anyhow::anyhow;
use bevy_ecs::resource::Resource;
use bytemuck::{NoUninit, Pod};
use vulkano::{
    VulkanError, VulkanLibrary,
    buffer::{Buffer, BufferContents, BufferCreateInfo, BufferMemory, BufferUsage},
    command_buffer::{
        CommandBufferBeginInfo, CommandBufferLevel, CommandBufferSubmitInfo, CommandBufferUsage,
        RecordingCommandBuffer, SemaphoreSubmitInfo, SubmitInfo,
        allocator::StandardCommandBufferAllocator,
        raw::{
            CopyBufferInfo, DependencyInfo, ImageMemoryBarrier, RenderingAttachmentInfo,
            RenderingInfo,
        },
    },
    device::{
        Device, DeviceCreateInfo, DeviceExtensions, DeviceFeatures, Queue, QueueCreateInfo,
        QueueFlags,
        physical::{PhysicalDevice, PhysicalDeviceType},
    },
    format::{ClearValue, Format},
    image::{Image, ImageAspects, ImageLayout, ImageSubresourceRange, ImageUsage, view::ImageView},
    instance::{Instance, InstanceCreateFlags, InstanceCreateInfo, InstanceExtensions},
    memory::{
        MappedMemoryRange,
        allocator::{
            AllocationCreateInfo, DeviceLayout, MemoryTypeFilter, StandardMemoryAllocator,
        },
    },
    render_pass::{AttachmentLoadOp, AttachmentStoreOp},
    swapchain::{
        AcquireNextImageInfo, PresentInfo, PresentMode, SemaphorePresentInfo, Surface, Swapchain,
        SwapchainCreateInfo, SwapchainPresentInfo,
    },
    sync::{
        AccessFlags, PipelineStages,
        fence::{Fence, FenceCreateFlags, FenceCreateInfo},
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
            .map(|mut it| it.any(|l| l.name() == "VK_LAYER_KHRONOS_validation"))
            .unwrap_or(false);

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
            khr_swapchain: true,

            khr_acceleration_structure: true,
            khr_ray_query: true,
            khr_ray_tracing_pipeline: true,
            khr_ray_tracing_position_fetch: true,
            // khr_ray_tracing_maintenance1: true,
            ..DeviceExtensions::empty()
        };

        let device_features = DeviceFeatures {
            buffer_device_address: true,
            shader_draw_parameters: true, // for vertex pulling via BDA

            descriptor_indexing: true,
            runtime_descriptor_array: true,
            descriptor_binding_variable_descriptor_count: true,
            descriptor_binding_partially_bound: true,
            shader_sampled_image_array_non_uniform_indexing: true,

            dynamic_rendering: true,
            synchronization2: true,

            scalar_block_layout: true,

            acceleration_structure: true,
            ray_query: true,
            ray_tracing_pipeline: true,
            ray_tracing_position_fetch: true,
            ..Default::default()
        };

        let (physical_device, queue_family_index) = instance
            .enumerate_physical_devices()?
            .filter(|pd| {
                let supported = pd.supported_extensions().contains(&device_extensions);

                if !supported {
                    log::info!("Skipping physical device {} because not all of our desired extensions are supported", pd.properties().device_name);
                }

                supported
            })
            .filter(|pd| {
                let supported = pd.supported_features().contains(&device_features);

                if !supported {
                    log::info!("Skipping physical device {} because not all of our desired features are supported", pd.properties().device_name);
                }

                supported
            })
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
                let semaphore = Arc::new(Semaphore::new(
                    &gpu.device,
                    &SemaphoreCreateInfo::default(),
                )?);

                unsafe {
                    gpu.device.set_debug_utils_object_name(
                        &semaphore,
                        Some(&format!("Render Semaphore {}", i)),
                    )
                }?;

                Ok(semaphore)
            })
            .collect::<anyhow::Result<_>>()?;

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
        keep_alive_list: &mut Vec<Arc<dyn Any + Send + Sync>>,
    ) -> anyhow::Result<()> {
        // self.gpu.device.wait_idle()?;

        let (create_info, format) =
            Self::get_swapchain_create_info(&self.gpu, self.surface.clone(), self.window.clone())?;

        log::info!(
            "Recreating swapcahin; new size is {}x{}",
            create_info.image_extent[0],
            create_info.image_extent[1]
        );

        let (new_swapchain, new_images) = self.inner.recreate(&create_info)?;

        // ensure previous resources are kept alive until the next frame cycle
        self.views
            .drain(..)
            .for_each(|v| keep_alive_list.push(v.clone()));

        keep_alive_list.push(self.inner.clone());

        self.inner = new_swapchain;
        self.images = new_images;

        self.views = self
            .images
            .iter()
            .map(|img| ImageView::new_default(img))
            .collect::<Result<Vec<_>, _>>()?;

        self.format = format;

        self.render_semaphores
            .drain(..)
            .for_each(|s| keep_alive_list.push(s.clone()));

        self.render_semaphores = (0..self.images.len())
            .map(|i| {
                let semaphore = Arc::new(Semaphore::new(
                    &self.gpu.device,
                    &SemaphoreCreateInfo::default(),
                )?);

                unsafe {
                    self.gpu.device.set_debug_utils_object_name(
                        &semaphore,
                        Some(&format!("Render Semaphore {}", i)),
                    )
                }?;

                Ok(semaphore)
            })
            .collect::<anyhow::Result<_>>()?;

        Ok(())
    }
}

// non-send resource
#[derive(Resource)]
pub struct SurfaceState {
    pub swapchain: SwapchainState,
    pub window: Arc<Window>,

    pub gpu: GpuHandle,

    // one for each frame in flight
    pub acquire_semaphores: [Arc<Semaphore>; FRAMES_IN_FLIGHT], // used so the gpu doesn't begin executing cmds until the swapchain image is available
    pub submit_fences: [Arc<Fence>; FRAMES_IN_FLIGHT], // used so the cpu does not render more than FRAMES_IN_FLIGHT frames ahead of the gpu

    pub cmd_buffer_allocators: [Arc<StandardCommandBufferAllocator>; FRAMES_IN_FLIGHT],
    pub keep_alive_lists: [Vec<Arc<dyn Any + Send + Sync>>; FRAMES_IN_FLIGHT],

    frame_in_flight_index: usize,
    swapchain_needs_recreate: bool,
}

impl SurfaceState {
    pub fn new(
        gpu: &GpuHandle,
        surface: Arc<Surface>,
        window: Arc<Window>,
    ) -> anyhow::Result<Self> {
        let swapchain = SwapchainState::new(gpu, surface.clone(), window.clone())?;

        let acquire_semaphores = std::array::try_from_fn(|i| -> anyhow::Result<_> {
            let semaphore = Arc::new(Semaphore::new(
                &gpu.device,
                &SemaphoreCreateInfo::default(),
            )?);

            unsafe {
                gpu.device.set_debug_utils_object_name(
                    &semaphore,
                    Some(&format!("Acquire Semaphore {}", i)),
                )?
            };

            Ok(semaphore)
        })?;

        let submit_fences = std::array::try_from_fn(|i| -> anyhow::Result<_> {
            let fence = Arc::new(Fence::new(
                &gpu.device,
                &FenceCreateInfo {
                    // create in the signaled state so the first frame knows not to wait on anything
                    flags: FenceCreateFlags::SIGNALED,
                    ..Default::default()
                },
            )?);

            unsafe {
                gpu.device
                    .set_debug_utils_object_name(&fence, Some(&format!("Submit Fence {}", i)))?
            };

            Ok(fence)
        })?;

        let cmd_buffer_allocators = std::array::from_fn(|_| {
            Arc::new(StandardCommandBufferAllocator::new(
                &gpu.device,
                &Default::default(),
            ))
        });

        log::info!("created command buffer allocators for each frame");

        let keep_alive_lists = std::array::from_fn(|_| vec![]);

        Ok(Self {
            swapchain,
            window,
            gpu: gpu.clone(),
            frame_in_flight_index: 0,
            acquire_semaphores,
            submit_fences,
            cmd_buffer_allocators,
            keep_alive_lists,
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

    /// ensures that a vulkano object is kept alive for at least as many frames in flight there are
    pub fn keep_alive<T: Any + Send + Sync>(&mut self, obj: Arc<T>) {
        self.keep_alive_lists[self.frame_in_flight_index].push(obj);
    }

    pub fn begin_frame(&mut self) -> Result<FrameRecord, FrameError> {
        // wait until the previous cycle's fence is signaled, so we don't render too far ahead
        self.submit_fences[self.frame_in_flight_index]
            .wait(None)
            .map_err(FrameError::Vulkan)?;

        // reset the fence so it can be signaled again later
        unsafe {
            self.submit_fences[self.frame_in_flight_index]
                .reset()
                .map_err(FrameError::Vulkan)
        }?;

        // free previous cycle's keep alive list to allow resources that are unneeded to be destroyed
        self.keep_alive_lists[self.frame_in_flight_index].clear();

        if self.swapchain_needs_recreate {
            self.swapchain
                .recreate(&mut self.keep_alive_lists[self.frame_in_flight_index])
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
                    new_layout: ImageLayout::General,

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

                    old_layout: ImageLayout::General,
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
                    // on wayland, notify compositor we are about to present
                    self.window.pre_present_notify();
                    q.present(&present_info)
                })
                // we can unwrap here because we know we're presenting to only one swapchain
                .and_then(|mut suboptimal| suboptimal.next().unwrap())
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

impl FrameRecord {
    /// uploads some data to device-local memory using a staging buffer
    pub fn upload_buffer<T: BufferContents + Pod>(
        &mut self,
        surface_state: &mut SurfaceState,
        data: &[T],
        create_info: &BufferCreateInfo,
        name: Option<&str>,
    ) -> anyhow::Result<Arc<Buffer>> {
        let buffer_ci = BufferCreateInfo {
            usage: BufferUsage::TRANSFER_SRC,
            ..Default::default()
        };

        let alloc_ci = AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::HOST_SEQUENTIAL_WRITE
                | MemoryTypeFilter::PREFER_HOST,
            ..Default::default()
        };

        let layout = DeviceLayout::for_value(data).unwrap();

        let staging_buffer = Buffer::new(
            &surface_state.gpu.memory_allocator,
            &buffer_ci,
            &alloc_ci,
            layout,
        )?;

        unsafe {
            surface_state.gpu.device.set_debug_utils_object_name(
                &staging_buffer,
                Some(&format!("Staging buffer for {}", name.unwrap_or("unnamed"))),
            )
        }?;

        let BufferMemory::Normal(mem) = staging_buffer.memory() else {
            panic!("staging buffer wasn't backed by normal memory");
        };

        unsafe {
            mem.mapped_slice(..)
                .unwrap()?
                .as_mut()
                .copy_from_slice(bytemuck::cast_slice(data));

            mem.flush_range(&MappedMemoryRange::default())?
        };

        assert!(create_info.usage.contains(BufferUsage::TRANSFER_DST));

        let alloc_ci = AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        };

        let actual_buffer = Buffer::new(
            &surface_state.gpu.memory_allocator,
            create_info,
            &alloc_ci,
            layout,
        )?;

        unsafe {
            surface_state
                .gpu
                .device
                .set_debug_utils_object_name(&actual_buffer, name)
        }?;

        unsafe {
            self.cmd_buffer
                .copy_buffer(&CopyBufferInfo::new(&staging_buffer, &actual_buffer));
        }

        // push both buffers to the keep alive list because they need to be alive until the commmand buffer is submitted
        surface_state.keep_alive(staging_buffer);
        surface_state.keep_alive(actual_buffer.clone());

        Ok(actual_buffer)
    }
}

#[derive(Debug)]
pub enum FrameError {
    SkipFrame,
    Unrecoverable,
    Vulkan(VulkanError),
    Other(anyhow::Error),
}
