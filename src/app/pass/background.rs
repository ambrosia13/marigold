use bevy_ecs::{
    resource::Resource,
    system::{Commands, Res, ResMut},
};
use glam::UVec3;

use crate::{
    app::{
        data::{atmosphere::AtmosphereBinding, camera::ScreenBinding},
        pass::{BACKGROUND_TEXTURE_FORMAT, bake::AtmosphereBakePass},
        render::{FrameRecord, GpuHandle, SurfaceState},
    },
    util,
};

// keep as multiples of 8 to make things simpler wrt. workgroup counts
pub const CUBEMAP_SIZE: u32 = 2048;
pub const SKY_VIEW_WIDTH: u32 = 400;
pub const SKY_VIEW_HEIGHT: u32 = 400;

// holds binding to the active background pass output, ie. atmosphere cubemap
#[derive(Resource)]
pub struct BackgroundBinding {
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl BackgroundBinding {
    pub fn init(
        mut commands: Commands,
        surface_state: Res<SurfaceState>,
        atmosphere_cubemap_pass: Res<AtmosphereCubemapPass>,
    ) {
        let sampler = surface_state
            .gpu
            .device
            .create_sampler(&wgpu::SamplerDescriptor {
                label: Some("background_cubemap_sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            });

        let bind_group_layout =
            surface_state
                .gpu
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("background_bind_group_layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::Cube,
                                multisampled: false,
                            },
                            count: None,
                        },
                    ],
                });

        let bind_group = surface_state
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("background_bind_group"),
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(
                            &atmosphere_cubemap_pass.cubemap_texture_view,
                        ),
                    },
                ],
            });

        log::info!("created background binding");
        commands.insert_resource(Self {
            bind_group,
            bind_group_layout,
        });
    }
}

#[derive(Resource)]
pub struct AtmosphereCubemapPass {
    pub cubemap_texture: wgpu::Texture,
    pub cubemap_texture_view: wgpu::TextureView,
    pub cubemap_face_texture_views: [wgpu::TextureView; 6],
    pub sky_view_day_texture: wgpu::Texture,
    pub sky_view_day_texture_view: wgpu::TextureView,
    pub sky_view_night_texture: wgpu::Texture,
    pub sky_view_night_texture_view: wgpu::TextureView,

    sky_view_pass_bind_group: wgpu::BindGroup,
    sky_view_pass_pipeline: wgpu::ComputePipeline,
    cubemap_pass_bind_group: wgpu::BindGroup,
    cubemap_pass_pipeline: wgpu::RenderPipeline,
}

impl AtmosphereCubemapPass {
    fn create_cubemap_face_view(texture: &wgpu::Texture, face: u32) -> wgpu::TextureView {
        texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some(&format!("atmosphere_cubemap_face_{}_texture_view", face)),
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: face,
            array_layer_count: Some(1),
            ..Default::default()
        })
    }

    pub fn init(
        mut commands: Commands,
        surface_state: Res<SurfaceState>,
        screen_binding: Res<ScreenBinding>,
        atmosphere_binding: Res<AtmosphereBinding>,
        atmosphere_bake_pass: Res<AtmosphereBakePass>,
    ) {
        log::info!("initializing atmosphere background passes");

        let gpu = &surface_state.gpu;

        let cubemap_texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atmosphere_cubemap_texture"),
            size: wgpu::Extent3d {
                width: CUBEMAP_SIZE,
                height: CUBEMAP_SIZE,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: BACKGROUND_TEXTURE_FORMAT, // this is the background texture
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let cubemap_texture_view = cubemap_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("atmosphere_cubemap_texture_view"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });

        let cubemap_face_texture_views = [
            Self::create_cubemap_face_view(&cubemap_texture, 0),
            Self::create_cubemap_face_view(&cubemap_texture, 1),
            Self::create_cubemap_face_view(&cubemap_texture, 2),
            Self::create_cubemap_face_view(&cubemap_texture, 3),
            Self::create_cubemap_face_view(&cubemap_texture, 4),
            Self::create_cubemap_face_view(&cubemap_texture, 5),
        ];

        let sky_view_texture_desc = wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: SKY_VIEW_WIDTH,
                height: SKY_VIEW_HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: cubemap_texture.format(), // match the cubemap format bc cubemap uses this lut to render itself
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };

        let sky_view_day_texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sky_view_day_texture"),
            ..sky_view_texture_desc
        });

        let sky_view_day_texture_view = sky_view_day_texture.create_view(&Default::default());

        let sky_view_night_texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sky_view_night_texture"),
            ..sky_view_texture_desc
        });

        let sky_view_night_texture_view = sky_view_night_texture.create_view(&Default::default());

        let shader_source = util::get_spirv_source("atmosphere/sky_view.slang");
        let shader_module = gpu.create_shader_module("sky_view", shader_source.into());

        let bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("sky_view_pass_bind_group_layout"),
                    entries: &[
                        // day sky
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::ReadWrite,
                                format: sky_view_day_texture.format(),
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        // night sky
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::ReadWrite,
                                format: sky_view_day_texture.format(),
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                    ],
                });

        let sky_view_pass_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky_view_pass_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&sky_view_day_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&sky_view_night_texture_view),
                },
            ],
        });

        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("sky_view_pass_pipeline_layout"),
                bind_group_layouts: &[
                    &screen_binding.bind_group_layout,
                    &atmosphere_binding.bind_group_layout,
                    &atmosphere_bake_pass.bind_group_layout,
                    &bind_group_layout,
                ],
                immediate_size: 4 * 2,
            });

        let sky_view_pass_pipeline =
            gpu.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("sky_view_pass_pipeline"),
                    layout: Some(&pipeline_layout),
                    module: &shader_module,
                    entry_point: Some("compute"),
                    compilation_options: Default::default(),
                    cache: Default::default(),
                });

        // cubemap pass
        let shader_source = util::get_spirv_source("atmosphere/cubemap");
        let fragment_shader_module =
            gpu.create_shader_module("atmosphere_cubemap", shader_source.into());
        let shader_source = util::get_spirv_source("frame");
        let vertex_shader_module = gpu.create_shader_module("frame", shader_source.into());

        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sky_view_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("atmosphere_cubemap_pass_bind_group_layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        // day
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // night
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                    ],
                });

        let cubemap_pass_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atmosphere_cubemap_pass_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&sky_view_day_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&sky_view_night_texture_view),
                },
            ],
        });

        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("atmosphere_cubemap_pass_pipeline_layout"),
                bind_group_layouts: &[
                    &atmosphere_binding.bind_group_layout,
                    &atmosphere_bake_pass.bind_group_layout,
                    &bind_group_layout,
                ],
                immediate_size: 0,
            });

        let cubemap_pass_pipeline =
            gpu.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("cubemap_pass_pipeline"),
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
                        targets: &std::array::from_fn::<_, 6, _>(|_| {
                            Some(wgpu::ColorTargetState {
                                format: BACKGROUND_TEXTURE_FORMAT,
                                blend: None,
                                write_mask: wgpu::ColorWrites::all(),
                            })
                        }),
                    }),
                    multiview_mask: None,
                    cache: None,
                });

        log::info!("initialized atmosphere background passes");
        commands.insert_resource(Self {
            cubemap_texture,
            cubemap_texture_view,
            cubemap_face_texture_views,
            sky_view_day_texture,
            sky_view_day_texture_view,
            sky_view_night_texture,
            sky_view_night_texture_view,
            sky_view_pass_bind_group,
            sky_view_pass_pipeline,
            cubemap_pass_bind_group,
            cubemap_pass_pipeline,
        });
    }

    pub fn update(
        mut frame: ResMut<FrameRecord>,
        screen_binding: Res<ScreenBinding>,
        atmosphere_binding: Res<AtmosphereBinding>,
        atmosphere_bake_pass: Res<AtmosphereBakePass>,
        atmosphere_cubemap_pass: Res<AtmosphereCubemapPass>,
    ) {
        let mut sky_view_pass = frame
            .encoder
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("sky_view_pass"),
                timestamp_writes: None,
            });

        sky_view_pass.set_pipeline(&atmosphere_cubemap_pass.sky_view_pass_pipeline);

        sky_view_pass.set_bind_group(0, &screen_binding.bind_group, &[]);
        sky_view_pass.set_bind_group(1, &atmosphere_binding.bind_group, &[]);
        sky_view_pass.set_bind_group(2, &atmosphere_bake_pass.bind_group, &[]);
        sky_view_pass.set_bind_group(3, &atmosphere_cubemap_pass.sky_view_pass_bind_group, &[]);

        sky_view_pass.set_immediates(
            0,
            bytemuck::cast_slice(&[
                atmosphere_cubemap_pass.sky_view_day_texture.width(),
                atmosphere_cubemap_pass.sky_view_day_texture.height(),
            ]),
        );

        let workgroup_size = UVec3::new(8, 8, 1);
        let dimensions = UVec3::new(
            atmosphere_cubemap_pass.sky_view_day_texture.width(),
            atmosphere_cubemap_pass.sky_view_day_texture.height(),
            1,
        );

        let workgroups = util::get_workgroup_count_from_size(workgroup_size, dimensions);
        sky_view_pass.dispatch_workgroups(workgroups.x, workgroups.y, workgroups.z);

        drop(sky_view_pass);

        let mut cubemap_pass = frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cubemap_pass"),
                color_attachments: &std::array::from_fn::<_, 6, _>(|i| {
                    Some(wgpu::RenderPassColorAttachment {
                        view: &atmosphere_cubemap_pass.cubemap_face_texture_views[i],
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::DontCare(unsafe {
                                wgpu::LoadOpDontCare::enabled()
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })
                }),
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

        cubemap_pass.set_pipeline(&atmosphere_cubemap_pass.cubemap_pass_pipeline);

        cubemap_pass.set_bind_group(0, &atmosphere_binding.bind_group, &[]);
        cubemap_pass.set_bind_group(1, &atmosphere_bake_pass.bind_group, &[]);
        cubemap_pass.set_bind_group(2, &atmosphere_cubemap_pass.cubemap_pass_bind_group, &[]);

        cubemap_pass.draw(0..6, 0..1);
    }
}
