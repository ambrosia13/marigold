use bevy_ecs::{
    resource::Resource,
    system::{Commands, Res, ResMut},
};

use crate::{
    app::{
        pass::post::PostTextures,
        render::{FrameRecord, SurfaceState},
    },
    util,
};

#[derive(Resource)]
pub struct DisplayPass {
    pipeline: wgpu::RenderPipeline,

    sampler: wgpu::Sampler,

    bind_group_layout: wgpu::BindGroupLayout,
    bind_group_main: wgpu::BindGroup,
    bind_group_alt: wgpu::BindGroup,
}

impl DisplayPass {
    pub fn init(
        mut commands: Commands,
        surface_state: Res<SurfaceState>,
        post_textures: Res<PostTextures>,
    ) {
        log::info!("beginning creation of display pass");

        let sample_type = post_textures
            .main
            .format()
            .sample_type(None, Some(surface_state.gpu.device.features()))
            .unwrap();

        let sampler = surface_state
            .gpu
            .device
            .create_sampler(&wgpu::SamplerDescriptor {
                label: Some("display_input_sampler"),
                ..Default::default()
            });

        let bind_group_layout =
            surface_state
                .gpu
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("display_bind_group_layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type,
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                            count: None,
                        },
                    ],
                });

        let bind_group_main =
            surface_state
                .gpu
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("display_bind_group_alt"),
                    layout: &bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&post_textures.main_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                });

        let bind_group_alt =
            surface_state
                .gpu
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("display_bind_group_alt"),
                    layout: &bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&post_textures.alt_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                });

        let pipeline_layout =
            surface_state
                .gpu
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("display_pipeline_layout"),
                    bind_group_layouts: &[&bind_group_layout],
                    push_constant_ranges: &[],
                });

        let vertex_shader_source = util::get_spirv_source("frame.slang");
        let fragment_shader_source = util::get_spirv_source("final.slang");

        log::info!("compiling display vertex shader");

        let vertex_shader_module = surface_state
            .gpu
            .create_shader_module("display_vertex_shader", vertex_shader_source.into());

        log::info!("compiling display fragment shader");

        let fragment_shader_module = surface_state
            .gpu
            .create_shader_module("display_fragment_shader", fragment_shader_source.into());

        let pipeline =
            surface_state
                .gpu
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("display_pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &vertex_shader_module,
                        entry_point: Some("vertex"),
                        compilation_options: Default::default(),
                        buffers: &[],
                    },
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &fragment_shader_module,
                        entry_point: Some("fragment"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: surface_state.config.format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::all(),
                        })],
                    }),
                    multiview: None,
                    cache: None,
                });

        let display_binding = Self {
            pipeline,
            sampler,
            bind_group_layout,
            bind_group_main,
            bind_group_alt,
        };

        commands.insert_resource(display_binding);
        log::info!("created display pass");
    }

    pub fn update(
        post_textures: Res<PostTextures>,
        display_pass: Res<Self>,
        mut frame: ResMut<FrameRecord>,
    ) {
        let bind_group = if post_textures.swapped {
            // log::info!("blitting alt texture to surface");
            &display_pass.bind_group_alt
        } else {
            // log::info!("blitting main texture to surface");
            &display_pass.bind_group_main
        };

        let view = frame.surface_texture_view.clone();

        let mut render_pass = frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("display_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

        render_pass.set_bind_group(0, bind_group, &[]);

        render_pass.set_pipeline(&display_pass.pipeline);

        render_pass.draw(0..6, 0..1);
    }
}
