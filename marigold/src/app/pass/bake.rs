use bevy_ecs::{
    message::{MessageReader, MessageWriter},
    resource::Resource,
    system::{Commands, Res, ResMut},
};
use glam::UVec3;

use crate::{
    app::{
        data::atmosphere::AtmosphereBinding,
        messages::AtmosphereRebakeMessage,
        render::{FrameRecord, SurfaceState},
    },
    util,
};

pub const TRANSMITTANCE_LUT_WIDTH: u32 = 256;
pub const TRANSMITTANCE_LUT_HEIGHT: u32 = 128;

pub const MULTISCATTERING_LUT_WIDTH: u32 = 32;
pub const MULTISCATTERING_LUT_HEIGHT: u32 = 32;

// values in the texture are 0-1, so we can use a normalized texture format
pub const ATMOSPHERE_LUT_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Unorm;

#[derive(Resource)]
pub struct AtmosphereBakePass {
    pub transmittance_texture: wgpu::Texture,
    #[expect(unused)]
    pub transmittance_texture_view: wgpu::TextureView,
    pub multiscattering_texture: wgpu::Texture,
    #[expect(unused)]
    pub multiscattering_texture_view: wgpu::TextureView,

    transmittance_pass_bind_group: wgpu::BindGroup,
    transmittance_pass_pipeline: wgpu::ComputePipeline,

    multiscattering_pass_bind_group: wgpu::BindGroup, // this bind group is used for the multiscattering pass
    multiscattering_pass_pipeline: wgpu::ComputePipeline,

    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup, // bind group that contains both transmittance and scattering luts, with a sampler
}

impl AtmosphereBakePass {
    pub fn init(
        mut commands: Commands,
        mut rebake_events: MessageWriter<AtmosphereRebakeMessage>,
        surface_state: Res<SurfaceState>,
        atmosphere_binding: Res<AtmosphereBinding>,
    ) {
        // send at least one rebake message
        rebake_events.write(AtmosphereRebakeMessage);

        log::info!("initializing atmosphere bake pass");

        let gpu = &surface_state.gpu;

        let desc = wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d::default(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: ATMOSPHERE_LUT_TEXTURE_FORMAT,
            // written to with a storage texture, read with a texture sampler
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };

        let transmittance_texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atmosphere_transmittance_texture"),
            size: wgpu::Extent3d {
                width: TRANSMITTANCE_LUT_WIDTH,
                height: TRANSMITTANCE_LUT_HEIGHT,
                depth_or_array_layers: 1,
            },
            ..desc
        });

        let transmittance_texture_view = transmittance_texture.create_view(&Default::default());

        let multiscattering_texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atmosphere_multiscattering_texture"),
            size: wgpu::Extent3d {
                width: MULTISCATTERING_LUT_WIDTH,
                height: MULTISCATTERING_LUT_HEIGHT,
                depth_or_array_layers: 1,
            },
            ..desc
        });

        let multiscattering_texture_view = multiscattering_texture.create_view(&Default::default());

        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atmosphere_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        let shader_source = util::get_spirv_source("atmosphere/transmittance.slang");
        let shader_module = gpu.create_shader_module("transmittance", shader_source.into());

        let bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("transmittance_pass_bind_group_layout"),
                    entries: &[
                        // storage texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::ReadWrite,
                                format: transmittance_texture.format(),
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                    ],
                });

        let transmittance_pass_bind_group =
            gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("transmittance_pass_bind_group"),
                layout: &bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&transmittance_texture_view),
                }],
            });

        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("transmittance_pass_pipeline_layout"),
                bind_group_layouts: &[&atmosphere_binding.bind_group_layout, &bind_group_layout],
                immediate_size: 4 * 2, // 2 uints, width and height
            });

        let transmittance_pass_pipeline =
            gpu.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("transmittance_pass_pipeline"),
                    layout: Some(&pipeline_layout),
                    module: &shader_module,
                    entry_point: Some("compute"),
                    compilation_options: Default::default(),
                    cache: Default::default(),
                });

        let shader_source = util::get_spirv_source("atmosphere/multiscattering.slang");
        let shader_module = gpu.create_shader_module("multiscattering", shader_source.into());

        // the layout for the bind group used for multiscattering bake pass
        let bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("multiscattering_pass_bind_group_layout"),
                    entries: &[
                        // sampler
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::all(),
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        // texture view
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::all(),
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // multiscat storage tex
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::ReadWrite,
                                format: multiscattering_texture.format(),
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                    ],
                });

        let multiscattering_pass_bind_group =
            gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("multiscattering_pass_bind_group"),
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&transmittance_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&multiscattering_texture_view),
                    },
                ],
            });

        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("multiscattering_pass_pipeline_layout"),
                bind_group_layouts: &[&atmosphere_binding.bind_group_layout, &bind_group_layout],
                immediate_size: 4 * 2, // 2 uints, width and height
            });

        let multiscattering_pass_pipeline =
            gpu.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("multiscattering_pass_pipeline"),
                    layout: Some(&pipeline_layout),
                    module: &shader_module,
                    entry_point: Some("compute"),
                    compilation_options: Default::default(),
                    cache: Default::default(),
                });

        // the layout for the bind group used by the sky view pass and anyone else who wants to access the luts
        let bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("atmosphere_lut_bind_group"),
                    entries: &[
                        // sampler
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::all(),
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        // transmittance texture view
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::all(),
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // multiscattering texture view
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::all(),
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                    ],
                });

        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atmosphere_lut_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&transmittance_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&multiscattering_texture_view),
                },
            ],
        });

        log::info!("initialized atmosphere bake pass");

        commands.insert_resource(Self {
            transmittance_texture,
            transmittance_texture_view,
            multiscattering_texture,
            multiscattering_texture_view,
            transmittance_pass_bind_group,
            transmittance_pass_pipeline,
            multiscattering_pass_bind_group,
            multiscattering_pass_pipeline,
            bind_group_layout,
            bind_group,
        });
    }

    pub fn update(
        mut rebake_events: MessageReader<AtmosphereRebakeMessage>,
        mut frame: ResMut<FrameRecord>,
        atmosphere_binding: Res<AtmosphereBinding>,
        atmosphere_bake_pass: Res<AtmosphereBakePass>,
    ) {
        let mut has_event = false;

        for _ in rebake_events.read() {
            has_event = true;
        }

        if !has_event {
            return;
        }

        log::info!("running atmosphere bake passes");

        let mut transmittance_pass =
            frame
                .encoder
                .begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("transmittance_pass"),
                    timestamp_writes: None,
                });

        transmittance_pass.set_pipeline(&atmosphere_bake_pass.transmittance_pass_pipeline);

        transmittance_pass.set_bind_group(0, &atmosphere_binding.bind_group, &[]);
        transmittance_pass.set_bind_group(
            1,
            &atmosphere_bake_pass.transmittance_pass_bind_group,
            &[],
        );
        transmittance_pass.set_immediates(
            0,
            bytemuck::cast_slice(&[
                atmosphere_bake_pass.transmittance_texture.width(),
                atmosphere_bake_pass.transmittance_texture.height(),
            ]),
        );

        let workgroup_size = UVec3::new(8, 8, 1);
        let dimensions = UVec3::new(
            atmosphere_bake_pass.transmittance_texture.width(),
            atmosphere_bake_pass.transmittance_texture.height(),
            1,
        );

        let workgroups = util::get_workgroup_count_from_size(workgroup_size, dimensions);
        transmittance_pass.dispatch_workgroups(workgroups.x, workgroups.y, workgroups.z);

        drop(transmittance_pass);

        let mut multiscattering_pass =
            frame
                .encoder
                .begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("multiscattering_pass"),
                    timestamp_writes: None,
                });

        multiscattering_pass.set_pipeline(&atmosphere_bake_pass.multiscattering_pass_pipeline);

        multiscattering_pass.set_bind_group(0, &atmosphere_binding.bind_group, &[]);
        multiscattering_pass.set_bind_group(
            1,
            &atmosphere_bake_pass.multiscattering_pass_bind_group,
            &[],
        );
        multiscattering_pass.set_immediates(
            0,
            bytemuck::cast_slice(&[
                atmosphere_bake_pass.multiscattering_texture.width(),
                atmosphere_bake_pass.multiscattering_texture.height(),
            ]),
        );

        let workgroup_size = UVec3::new(8, 8, 1);
        let dimensions = UVec3::new(
            atmosphere_bake_pass.multiscattering_texture.width(),
            atmosphere_bake_pass.multiscattering_texture.height(),
            1,
        );

        let workgroups = util::get_workgroup_count_from_size(workgroup_size, dimensions);
        multiscattering_pass.dispatch_workgroups(workgroups.x, workgroups.y, workgroups.z);
    }
}
