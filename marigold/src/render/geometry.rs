use std::sync::Arc;

use bevy_ecs::{
    query::With,
    resource::Resource,
    system::{Commands, Local, NonSendMut, Res, ResMut, Single},
};
use vulkano::{
    command_buffer::raw::{
        DependencyInfo, ImageMemoryBarrier, RenderingAttachmentInfo, RenderingInfo,
    },
    format::{ClearValue, Format},
    image::{
        Image, ImageAspects, ImageCreateInfo, ImageLayout, ImageSubresourceRange, ImageUsage,
        view::{ImageView, ImageViewCreateInfo},
    },
    memory::allocator::{AllocationCreateInfo, MemoryAllocatePreference, MemoryTypeFilter},
    pipeline::{
        DynamicState, GraphicsPipeline, PipelineLayout, PipelineShaderStageCreateInfo,
        graphics::{
            GraphicsPipelineCreateInfo,
            color_blend::{ColorBlendAttachmentState, ColorBlendState},
            depth_stencil::{DepthState, DepthStencilState},
            input_assembly::{InputAssemblyState, PrimitiveTopology},
            multisample::MultisampleState,
            rasterization::{CullMode, FrontFace, RasterizationState},
            subpass::{PipelineRenderingCreateInfo, PipelineSubpassType},
            vertex_input::VertexInputState,
            viewport::{Scissor, Viewport, ViewportState},
        },
        layout::{PipelineLayoutCreateInfo, PushConstantRange},
    },
    render_pass::{AttachmentLoadOp, AttachmentStoreOp},
    shader::{EntryPoint, ShaderModule, ShaderModuleCreateInfo, ShaderStages},
    sync::{AccessFlags, PipelineStages},
};

use crate::{
    app::{
        camera::Camera,
        scene::{ActiveModel, ModelData, ModelInfo, UploadedModel},
    },
    vk::{FrameRecord, GpuHandle, SurfaceState},
    window::schedules::SystemResult,
};

#[derive(Resource)]
pub struct GeometryPass {
    gpu: GpuHandle,

    // unnecessary, but we store this to be able to easily recreate these resources upon resize
    pub depth_image_ci: ImageCreateInfo<'static>,
    pub image_ai: AllocationCreateInfo<'static>,

    pub depth_image: Arc<Image>,
    pub depth_image_view: Arc<ImageView>,
    pub pipeline: Arc<GraphicsPipeline>,
    pub pipeline_layout: Arc<PipelineLayout>,

    image_layout_transitioned: bool,
}

impl GeometryPass {
    pub fn init(mut commands: Commands, surface_state: Res<SurfaceState>) -> SystemResult {
        let surface_state: &SurfaceState = &surface_state;

        log::info!("Initializing geometry pass");

        let depth_image_ci = ImageCreateInfo {
            format: Format::D32_SFLOAT,
            extent: surface_state.swapchain.images[0].extent(),
            usage: ImageUsage::DEPTH_STENCIL_ATTACHMENT,
            ..Default::default()
        };

        let image_ai = AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            allocate_preference: MemoryAllocatePreference::AlwaysAllocate,
            ..Default::default()
        };

        let depth_image = Image::new(
            &surface_state.gpu.memory_allocator,
            &depth_image_ci,
            &image_ai,
        )?;

        let depth_image_view =
            ImageView::new(&depth_image, &ImageViewCreateInfo::from_image(&depth_image))?;

        unsafe {
            surface_state
                .gpu
                .device
                .set_debug_utils_object_name(&depth_image, Some("Geometry Pass Depth Image"))
        }?;

        unsafe {
            surface_state.gpu.device.set_debug_utils_object_name(
                &depth_image_view,
                Some("Geometry Pass Depth Image View"),
            )
        }?;

        log::info!("Created depth image");

        let shader_module = unsafe {
            ShaderModule::new(
                &surface_state.gpu.device,
                &ShaderModuleCreateInfo::new(&crate::util::get_spirv_source("geometry")),
            )
        }?;

        let vertex_entrypoint = shader_module.entry_point("vertex").unwrap();
        let fragment_entrypoint = shader_module.entry_point("fragment").unwrap();

        log::info!("Created shader module and entrypoints");

        let pipeline_layout = PipelineLayout::new(
            &surface_state.gpu.device,
            &PipelineLayoutCreateInfo {
                push_constant_ranges: &[PushConstantRange {
                    stages: ShaderStages::all_graphics(),
                    offset: 0,
                    // each pointer is 8 bytes, and we are passing three pointers (index buffer, vertex buffer, camera uniform)
                    size: 8 * 3,
                }],
                ..Default::default()
            },
        )?;

        let pipeline = GraphicsPipeline::new(
            &surface_state.gpu.device,
            None,
            &GraphicsPipelineCreateInfo {
                stages: &[
                    PipelineShaderStageCreateInfo::new(&vertex_entrypoint),
                    PipelineShaderStageCreateInfo::new(&fragment_entrypoint),
                ],
                vertex_input_state: Some(&VertexInputState::new()),
                input_assembly_state: Some(&InputAssemblyState {
                    topology: PrimitiveTopology::TriangleList,
                    ..Default::default()
                }),
                viewport_state: Some(&ViewportState::default()),
                rasterization_state: Some(&RasterizationState {
                    cull_mode: CullMode::Back,
                    front_face: FrontFace::CounterClockwise,
                    ..Default::default()
                }),
                depth_stencil_state: Some(&DepthStencilState {
                    depth: Some(DepthState::reverse()), // rev-z
                    ..Default::default()
                }),
                color_blend_state: Some(&ColorBlendState {
                    attachments: &[ColorBlendAttachmentState::default()],
                    ..Default::default()
                }),
                multisample_state: Some(&MultisampleState::default()),
                dynamic_state: &[DynamicState::Viewport, DynamicState::Scissor],
                subpass: Some(PipelineSubpassType::BeginRendering(
                    &PipelineRenderingCreateInfo {
                        color_attachment_formats: &[Some(surface_state.swapchain.format)],
                        depth_attachment_format: Some(Format::D32_SFLOAT),
                        ..Default::default()
                    },
                )),
                ..GraphicsPipelineCreateInfo::new(&pipeline_layout)
            },
        )?;

        log::info!("Created graphics pipeline");

        commands.insert_resource(Self {
            gpu: surface_state.gpu.clone(),
            depth_image_ci,
            image_ai,
            depth_image,
            depth_image_view,
            pipeline,
            pipeline_layout,
            image_layout_transitioned: false,
        });

        Ok(())
    }

    pub fn on_resize(mut geometry_pass: ResMut<Self>) -> SystemResult {
        let geometry_pass: &mut GeometryPass = &mut geometry_pass;

        log::info!("Recreating swapchain-dependent geometry pass resources due to screen resize");

        geometry_pass.depth_image = Image::new(
            &geometry_pass.gpu.memory_allocator,
            &geometry_pass.depth_image_ci,
            &geometry_pass.image_ai,
        )?;

        geometry_pass.image_layout_transitioned = false;

        Ok(())
    }

    pub fn draw(
        surface_state: Res<SurfaceState>,
        mut geometry_pass: ResMut<Self>,
        mut frame: NonSendMut<FrameRecord>,
        query: Single<(&ModelInfo, &ModelData, &UploadedModel), With<ActiveModel>>,
        camera: Res<Camera>,
    ) -> SystemResult {
        let surface_state: &SurfaceState = &surface_state;
        let geometry_pass: &mut GeometryPass = &mut geometry_pass;
        let camera: &Camera = &camera;

        let flight_index = frame.flight_index;

        if !geometry_pass.image_layout_transitioned {
            log::info!(
                "Transitioning geometry pass depth image to be ImageLayout::DepthAttachmentOptimal"
            );

            unsafe {
                frame.cmd_buffer.pipeline_barrier(&DependencyInfo {
                    image_memory_barriers: &[ImageMemoryBarrier {
                        src_stages: PipelineStages::LATE_FRAGMENT_TESTS,
                        src_access: AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,

                        dst_stages: PipelineStages::EARLY_FRAGMENT_TESTS,
                        dst_access: AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,

                        old_layout: ImageLayout::Undefined,
                        new_layout: ImageLayout::DepthAttachmentOptimal,

                        subresource_range: ImageSubresourceRange {
                            aspects: ImageAspects::DEPTH,
                            ..Default::default()
                        },
                        ..ImageMemoryBarrier::new(&geometry_pass.depth_image)
                    }],
                    ..Default::default()
                });
            }

            geometry_pass.image_layout_transitioned = true;
        }

        unsafe {
            frame.cmd_buffer.begin_rendering(&RenderingInfo {
                color_attachments: &[Some(RenderingAttachmentInfo {
                    image_layout: ImageLayout::General,
                    load_op: AttachmentLoadOp::Clear,
                    store_op: AttachmentStoreOp::Store,
                    clear_value: Some(ClearValue::Float([0.0, 0.0, 0.0, 1.0])),
                    ..RenderingAttachmentInfo::new(&surface_state.swapchain.views[flight_index])
                })],
                depth_attachment: Some(&Some(RenderingAttachmentInfo {
                    image_layout: ImageLayout::DepthAttachmentOptimal,
                    load_op: AttachmentLoadOp::Clear,
                    store_op: AttachmentStoreOp::DontCare,
                    clear_value: Some(ClearValue::Depth(0.0)),
                    ..RenderingAttachmentInfo::new(&geometry_pass.depth_image_view)
                })),

                ..Default::default()
            })
        };

        unsafe { frame.cmd_buffer.bind_pipeline(&geometry_pass.pipeline) };

        unsafe {
            frame.cmd_buffer.set_viewport(
                0,
                &[Viewport {
                    extent: [
                        geometry_pass.depth_image.extent()[0] as f32,
                        geometry_pass.depth_image.extent()[1] as f32,
                    ],
                    ..Default::default()
                }],
            )
        };

        unsafe {
            frame.cmd_buffer.set_scissor(
                0,
                &[Scissor {
                    extent: [
                        geometry_pass.depth_image.extent()[0],
                        geometry_pass.depth_image.extent()[1],
                    ],
                    ..Default::default()
                }],
            )
        };

        let (info, model_data, uploaded_model): (&ModelInfo, &ModelData, &UploadedModel) = *query;

        for instance in &model_data.scenes[info.active_scene].instances {
            let (index_buffer_address, vertex_buffer_address) =
                uploaded_model.mesh_addresses[instance.mesh_index];

            unsafe {
                frame.cmd_buffer.push_constants(
                    &geometry_pass.pipeline_layout,
                    0,
                    bytemuck::cast_slice::<_, u8>(&[
                        index_buffer_address,
                        vertex_buffer_address,
                        camera.buffer_addresses[flight_index],
                    ]),
                )
            };

            unsafe {
                frame.cmd_buffer.draw(
                    model_data.meshes[instance.mesh_index].triangles.len() as u32,
                    1,
                    0,
                    0,
                )
            };
        }

        unsafe { frame.cmd_buffer.end_rendering() };

        Ok(())
    }
}
