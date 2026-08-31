use bevy_ecs::{
    query::With,
    system::{NonSendMut, Res, ResMut, Single},
};
use vulkano::{
    command_buffer::raw::{
        DependencyInfo, ImageMemoryBarrier, RenderingAttachmentInfo, RenderingInfo,
    },
    format::ClearValue,
    image::{ImageAspects, ImageLayout, ImageSubresourceRange},
    pipeline::graphics::viewport::{Scissor, Viewport},
    render_pass::{AttachmentLoadOp, AttachmentStoreOp},
    sync::{AccessFlags, PipelineStages},
};

use crate::{
    app::{
        camera::Camera,
        scene::{ActiveModel, ModelData, ModelInfo, UploadedModel},
    },
    render::geometry::GeometryPass,
    vk::{FrameRecord, SurfaceState},
    window::schedules::SystemResult,
};

pub mod geometry;

/// a single function that draws all the passes in the renderer, so we can have context-aware barriers
pub fn draw(
    surface_state: Res<SurfaceState>,
    mut geometry_pass: ResMut<GeometryPass>,
    mut frame: NonSendMut<FrameRecord>,
    query: Single<(&ModelInfo, &ModelData, &UploadedModel), With<ActiveModel>>,
    camera: Res<Camera>,
) -> SystemResult {
    let surface_state: &SurfaceState = &surface_state;
    let geometry_pass: &mut GeometryPass = &mut geometry_pass;
    let camera: &Camera = &camera;

    let flight_index = frame.flight_index;
    let swapchain_image_index = frame.swapchain_image_index;

    // transition the swapchain image from undefined to general so we can begin drawing to it
    unsafe {
        frame.cmd_buffer.pipeline_barrier(&DependencyInfo {
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
                    &surface_state.swapchain.images[swapchain_image_index as usize],
                )
            }],
            ..Default::default()
        });
    }

    // geometry pass that renders the active model
    {
        if !geometry_pass.image_layout_transitioned {
            log::info!(
                "Transitioning geometry pass depth image to be ImageLayout::DepthAttachmentOptimal"
            );

            // transition the depth image away from undefined image layout, only do this once
            unsafe {
                frame.cmd_buffer.pipeline_barrier(&DependencyInfo {
                    image_memory_barriers: &[ImageMemoryBarrier {
                        src_stages: PipelineStages::TOP_OF_PIPE,
                        src_access: AccessFlags::empty(),

                        dst_stages: PipelineStages::EARLY_FRAGMENT_TESTS
                            | PipelineStages::LATE_FRAGMENT_TESTS,
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
                    ..RenderingAttachmentInfo::new(
                        &surface_state.swapchain.views[swapchain_image_index as usize],
                    )
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
            let (vertex_buffer_address, index_buffer_address) =
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
                    model_data.meshes[instance.mesh_index].triangles.len() as u32 * 3,
                    1,
                    0,
                    0,
                )
            };
        }

        unsafe { frame.cmd_buffer.end_rendering() };
    }

    // transition back into the present optimal layout now that we are done with all rendering
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
                    &surface_state.swapchain.images[swapchain_image_index as usize],
                )
            }],
            ..Default::default()
        })
    };

    Ok(())
}
