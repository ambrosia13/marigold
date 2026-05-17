use bevy_ecs::{
    component::Component,
    query::With,
    resource::Resource,
    system::{Commands, Res, ResMut, Single},
};
use derived_deref::Deref;
use glam::UVec3;

use crate::{
    app::{
        data::{camera::ScreenBinding, profile::GeometryPassFrametimes, scene::SceneBinding},
        pass::background::BackgroundBinding,
        render::{FrameRecord, SurfaceState},
    },
    util,
};

#[derive(Resource)]
pub struct GeometryTextures {
    pub current: wgpu::Texture,
    pub previous: wgpu::Texture,

    pub current_view: wgpu::TextureView,
    #[expect(unused)]
    pub previous_view: wgpu::TextureView,

    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

impl GeometryTextures {
    pub fn init(mut commands: Commands, surface_state: Res<SurfaceState>) {
        log::info!("beginning creation of geometry pass textures");

        let gpu = &surface_state.gpu;

        let current_desc = wgpu::TextureDescriptor {
            label: Some("geometry_pass_current_texture"),
            size: wgpu::Extent3d {
                width: surface_state.config.width,
                height: surface_state.config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: super::INTERMEDIATE_TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        };

        let previous_desc = wgpu::TextureDescriptor {
            label: Some("geometry_pass_previous_texture"),
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            ..current_desc
        };

        let current = gpu.device.create_texture(&current_desc);
        let previous = gpu.device.create_texture(&previous_desc);

        let current_view = current.create_view(&Default::default());
        let previous_view = previous.create_view(&Default::default());

        let bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("geometry_pass_bind_group_layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::WriteOnly,
                                format: current.format(),
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                    ],
                });

        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("geometry_pass_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&current_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&previous_view),
                },
            ],
        });

        let geometry_textures = Self {
            current,
            previous,
            current_view,
            previous_view,
            bind_group_layout,
            bind_group,
        };

        commands.insert_resource(geometry_textures);
        log::info!("created geometry pass textures");
    }
}

#[derive(Resource)]
pub struct GeometryCommon {
    pipeline_layout: wgpu::PipelineLayout,
}

impl GeometryCommon {
    pub fn init(
        mut commands: Commands,
        surface_state: Res<SurfaceState>,
        screen_binding: Res<ScreenBinding>,
        background_binding: Res<BackgroundBinding>,
        scene_binding: Res<SceneBinding>,
        geometry_textures: Res<GeometryTextures>,
    ) {
        let gpu = &surface_state.gpu;

        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("geometry_pass_pipeline_layout"),
                bind_group_layouts: &[
                    &screen_binding.bind_group_layout,
                    &background_binding.bind_group_layout,
                    &scene_binding.bind_group_layout,
                    &geometry_textures.bind_group_layout,
                ],
                immediate_size: 0,
            });

        commands.insert_resource(Self { pipeline_layout });

        log::info!("created geometry pass pipeline layout");
    }
}

#[derive(Component, Deref)]
pub struct GeometryPipeline(wgpu::ComputePipeline);

#[derive(Component)]
pub struct ActiveGeometryPipeline;

pub fn create_pathtrace_pipeline(
    mut commands: Commands,
    surface_state: Res<SurfaceState>,
    common: Res<GeometryCommon>,
) {
    let gpu = &surface_state.gpu;

    let shader_source = util::get_spirv_source("pathtrace.slang");
    let shader_module = gpu.create_shader_module("pathtrace", shader_source.into());

    let pipeline_layout = &common.pipeline_layout;

    let pipeline = gpu
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("pathtrace_pipeline"),
            layout: Some(pipeline_layout),
            module: &shader_module,
            entry_point: Some("compute"),
            compilation_options: Default::default(),
            cache: None,
        });

    commands.spawn((GeometryPipeline(pipeline), ActiveGeometryPipeline));
    log::info!("created pathtrace pipeline for geometry pass");
}

// draw geometry with the active geometry pipeline
pub fn draw_geometry(
    surface_state: Res<SurfaceState>,
    mut frame: ResMut<FrameRecord>,
    screen_binding: Res<ScreenBinding>,
    background_binding: Res<BackgroundBinding>,
    scene_binding: Res<SceneBinding>,
    geometry_textures: Res<GeometryTextures>,

    mut frametimes: ResMut<GeometryPassFrametimes>,

    active_pipeline: Single<&GeometryPipeline, With<ActiveGeometryPipeline>>,
) {
    // copy current texture to previous texture
    frame.encoder.copy_texture_to_texture(
        geometry_textures.current.as_image_copy(),
        geometry_textures.previous.as_image_copy(),
        geometry_textures.current.size(),
    );

    let mut compute_pass = frame
        .encoder
        .begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("geometry_pass"),
            timestamp_writes: None,
        });

    compute_pass.set_bind_group(0, &screen_binding.bind_group, &[]);
    compute_pass.set_bind_group(1, &background_binding.bind_group, &[]);
    compute_pass.set_bind_group(2, &scene_binding.bind_group, &[]);
    compute_pass.set_bind_group(3, &geometry_textures.bind_group, &[]);

    compute_pass.set_pipeline(&active_pipeline);

    let workgroup_size = UVec3::new(8, 8, 1);
    let dimensions = UVec3::new(
        geometry_textures.current.size().width,
        geometry_textures.current.size().height,
        1,
    );

    let workgroups = util::get_workgroup_count_from_size(workgroup_size, dimensions);

    // TODO: replace this with gpu time queries
    let start = std::time::Instant::now();

    compute_pass.dispatch_workgroups(workgroups.x, workgroups.y, workgroups.z);

    surface_state
        .gpu
        .device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();

    let time = start.elapsed();
    frametimes.tick(time);
}
