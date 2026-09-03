use std::sync::Arc;

use bevy_ecs::{
    resource::Resource,
    system::{Commands, Res, ResMut},
};
use vulkano::{
    format::Format,
    image::{Image, ImageCreateInfo, ImageUsage, view::ImageView},
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
            viewport::ViewportState,
        },
        layout::{PipelineLayoutCreateInfo, PushConstantRange},
    },
    shader::{ShaderModule, ShaderModuleCreateInfo, ShaderStages},
};

use crate::{
    vk::{GpuHandle, SurfaceState},
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

    pub image_layout_transitioned: bool,
}

impl GeometryPass {
    pub fn init(mut commands: Commands, surface_state: Res<SurfaceState>) -> SystemResult {
        let surface_state: &SurfaceState = &surface_state;

        log::info!("Initializing geometry pass");

        let depth_image_ci = ImageCreateInfo {
            format: Format::D32_SFLOAT,
            extent: surface_state.current_image().extent(),
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

        let depth_image_view = ImageView::new_default(&depth_image)?;

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

        geometry_pass.depth_image_view = ImageView::new_default(&geometry_pass.depth_image)?;

        unsafe {
            geometry_pass.gpu.device.set_debug_utils_object_name(
                &geometry_pass.depth_image,
                Some("Geometry Pass Depth Image"),
            )
        }?;

        unsafe {
            geometry_pass.gpu.device.set_debug_utils_object_name(
                &geometry_pass.depth_image_view,
                Some("Geometry Pass Depth Image View"),
            )
        }?;

        geometry_pass.image_layout_transitioned = false;

        Ok(())
    }
}
