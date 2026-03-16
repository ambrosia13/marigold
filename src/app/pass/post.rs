use std::sync::Arc;

use bevy_ecs::{
    resource::Resource,
    system::{Commands, Res, ResMut},
    world::World,
};
use glam::UVec3;

use crate::{
    app::{
        data::camera::ScreenBinding,
        debug_menu::DebugMenuBinding,
        pass::geometry::GeometryTextures,
        render::{FrameRecord, SurfaceState},
    },
    util,
};

pub struct PostTextureState {
    pub input: wgpu::Texture,
    pub input_view: wgpu::TextureView,
    pub output: wgpu::Texture,
    pub output_view: wgpu::TextureView,

    pub sampler: wgpu::Sampler,

    pub bind_group: wgpu::BindGroup,

    swapped: bool,
}

impl PostTextureState {
    pub fn state(&self) -> &'static str {
        if !self.swapped {
            "main_to_alt"
        } else {
            "alt_to_main"
        }
    }
}

#[derive(Resource)]
pub struct PostTextures {
    pub main: wgpu::Texture,
    pub main_view: wgpu::TextureView,

    pub alt: wgpu::Texture,
    pub alt_view: wgpu::TextureView,

    pub sampler: wgpu::Sampler,

    pub main_to_alt_bind_group: wgpu::BindGroup,
    pub alt_to_main_bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,

    pub swapped: bool,

    blit_bind_group: wgpu::BindGroup,
    blit_pipeline: wgpu::RenderPipeline,
}

impl PostTextures {
    pub fn init(
        mut commands: Commands,
        surface_state: Res<SurfaceState>,
        geometry_textures: Res<GeometryTextures>,
    ) {
        log::info!("beginning creation of post pass textures");

        let sampler = surface_state
            .gpu
            .device
            .create_sampler(&wgpu::SamplerDescriptor {
                label: Some("post_pass_sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            });

        let desc = wgpu::TextureDescriptor {
            label: Some("post_pass_textures_main"),
            size: wgpu::Extent3d {
                width: surface_state.viewport_size.width,
                height: surface_state.viewport_size.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: super::POST_TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        };

        let main = surface_state.gpu.device.create_texture(&desc);
        let alt = surface_state
            .gpu
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("post_pass_textures_alt"),
                ..desc
            });

        let main_view = main.create_view(&Default::default());
        let alt_view = alt.create_view(&Default::default());

        let bind_group_layout =
            surface_state
                .gpu
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("post_pass_bind_group_layout"),
                    entries: &[
                        // input sampler + texture view
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::all(),
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
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
                        // output storage texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::all(),
                            ty: wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::ReadWrite,
                                format: desc.format,
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                    ],
                });

        let main_to_alt_bind_group =
            surface_state
                .gpu
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("post_pass_main_to_alt_bind_group"),
                    layout: &bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&main_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&alt_view),
                        },
                    ],
                });

        let alt_to_main_bind_group =
            surface_state
                .gpu
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("post_pass_alt_to_main_bind_group"),
                    layout: &bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&alt_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&main_view),
                        },
                    ],
                });

        let blit_bind_group_layout =
            surface_state
                .gpu
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("post_pass_blit_bind_group_layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                    ],
                });

        let blit_bind_group =
            surface_state
                .gpu
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("post_pass_blit_bind_group"),
                    layout: &blit_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(
                                &geometry_textures.current_view,
                            ),
                        },
                    ],
                });

        let vertex_shader_source = util::get_spirv_source("frame.slang");
        let vertex_shader_module = surface_state
            .gpu
            .create_shader_module("blit", vertex_shader_source.into());

        let fragment_shader_source = util::get_spirv_source("blit.slang");
        let fragment_shader_module = surface_state
            .gpu
            .create_shader_module("blit", fragment_shader_source.into());

        let pipeline_layout =
            surface_state
                .gpu
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("post_pass_blit_pipeline_layout"),
                    bind_group_layouts: &[&blit_bind_group_layout],
                    immediate_size: 0,
                });

        let blit_pipeline =
            surface_state
                .gpu
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("post_pass_blit_pipeline"),
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
                            format: main.format(),
                            blend: None,
                            write_mask: wgpu::ColorWrites::all(),
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                });

        let post_textures = Self {
            main,
            main_view,
            alt,
            alt_view,
            sampler,
            main_to_alt_bind_group,
            alt_to_main_bind_group,
            bind_group_layout,
            swapped: false,
            blit_bind_group,
            blit_pipeline,
        };

        commands.insert_resource(post_textures);
        log::info!("created post pass textures")
    }

    pub fn update(post_textures: Res<PostTextures>, mut frame: ResMut<FrameRecord>) {
        let mut render_pass = frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post_pass_blit_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &post_textures.main_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::DontCare(unsafe { wgpu::LoadOpDontCare::enabled() }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

        render_pass.set_bind_group(0, &post_textures.blit_bind_group, &[]);
        render_pass.set_pipeline(&post_textures.blit_pipeline);

        render_pass.draw(0..6, 0..1);
    }

    pub fn current_input(&self) -> (wgpu::Texture, wgpu::TextureView) {
        if !self.swapped {
            // if not swapped, main -> alt
            (self.main.clone(), self.main_view.clone())
        } else {
            // if swapped, alt -> main
            (self.alt.clone(), self.alt_view.clone())
        }
    }

    pub fn current_output(&self) -> (wgpu::Texture, wgpu::TextureView) {
        if !self.swapped {
            // if not swapped, main -> alt
            (self.alt.clone(), self.alt_view.clone())
        } else {
            // if swapped, alt -> main
            (self.main.clone(), self.main_view.clone())
        }
    }

    pub fn write(&mut self) -> PostTextureState {
        if !self.swapped {
            self.swapped = !self.swapped;

            PostTextureState {
                input: self.main.clone(),
                input_view: self.main_view.clone(),
                output: self.alt.clone(),
                output_view: self.alt_view.clone(),
                sampler: self.sampler.clone(),
                bind_group: self.main_to_alt_bind_group.clone(),
                swapped: !self.swapped,
            }
        } else {
            self.swapped = !self.swapped;

            PostTextureState {
                input: self.alt.clone(),
                input_view: self.alt_view.clone(),
                output: self.main.clone(),
                output_view: self.main_view.clone(),
                sampler: self.sampler.clone(),
                bind_group: self.alt_to_main_bind_group.clone(),
                swapped: !self.swapped,
            }
        }
    }
}

#[derive(Resource)]
pub struct PostPasses {
    passes: Arc<Vec<Box<dyn PostPass>>>,
}

impl PostPasses {
    pub fn init(world: &mut World) {
        let passes: Vec<Box<dyn PostPass>> = vec![
            Box::new(DummyPostPass::new(world)),
            Box::new(MenuPass::new(world)),
        ];
        let passes = Arc::new(passes);

        world.insert_resource(Self { passes });
    }

    pub fn update(world: &mut World) {
        let passes = world.resource::<Self>().passes.clone();

        for pass in passes.iter() {
            for _ in 0..pass.count() {
                pass.run(world);
            }
        }
    }
}

trait PostPass: Send + Sync {
    fn count(&self) -> usize {
        1
    }

    fn run(&self, world: &mut World);
}

pub struct DummyPostPass {
    pipeline: wgpu::ComputePipeline,
}

impl DummyPostPass {
    pub fn new(world: &mut World) -> Self {
        let surface_state = world.resource::<SurfaceState>();
        let screen_binding = world.resource::<ScreenBinding>();
        let post_textures = world.resource::<PostTextures>();

        let gpu = &surface_state.gpu;

        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("dummy_post_pipeline_layout"),
                bind_group_layouts: &[
                    &screen_binding.bind_group_layout,
                    &post_textures.bind_group_layout,
                ],
                immediate_size: 0,
            });

        let shader_source = util::get_spirv_source("dummy.slang");
        let shader_module = gpu.create_shader_module("dummy", shader_source.into());

        let pipeline = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("dummy_post_pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader_module,
                entry_point: Some("compute"),
                compilation_options: Default::default(),
                cache: None,
            });

        log::info!("created dummy post pass");

        Self { pipeline }
    }
}

impl PostPass for DummyPostPass {
    fn run(&self, world: &mut World) {
        let mut post_textures = world.resource_mut::<PostTextures>();
        let texture_state = post_textures.write();

        // log::info!("dummy post texture state: {}", texture_state.state());

        let screen_binding = world.resource::<ScreenBinding>();
        let screen_bind_group = screen_binding.bind_group.clone();

        let surface_state = world.resource::<SurfaceState>();
        let workgroups = util::get_workgroup_count_from_size(
            UVec3::new(8, 8, 1),
            UVec3 {
                x: surface_state.config.width,
                y: surface_state.config.height,
                z: 1,
            },
        );

        let mut frame = world.resource_mut::<FrameRecord>();

        let mut compute_pass = frame
            .encoder
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("dummy_post_pass"),
                timestamp_writes: None,
            });

        compute_pass.set_pipeline(&self.pipeline);
        compute_pass.set_bind_group(0, &screen_bind_group, &[]);
        compute_pass.set_bind_group(1, &texture_state.bind_group, &[]);

        compute_pass.dispatch_workgroups(workgroups.x, workgroups.y, workgroups.z);
    }
}

pub struct MenuPass {
    pipeline: wgpu::ComputePipeline,
}

impl MenuPass {
    pub fn new(world: &mut World) -> Self {
        let surface_state = world.resource::<SurfaceState>();
        let screen_binding = world.resource::<ScreenBinding>();
        let post_textures = world.resource::<PostTextures>();
        let menu_binding = world.resource::<DebugMenuBinding>();

        let gpu = &surface_state.gpu;

        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("menu_pipeline_layout"),
                bind_group_layouts: &[
                    &screen_binding.bind_group_layout,
                    &post_textures.bind_group_layout,
                    &menu_binding.bind_group_layout,
                ],
                immediate_size: 0,
            });

        let shader_source = util::get_spirv_source("menu.slang");
        let shader_module = gpu.create_shader_module("menu", shader_source.into());

        let pipeline = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("menu_pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader_module,
                entry_point: Some("compute"),
                compilation_options: Default::default(),
                cache: None,
            });

        log::info!("created menu post pass");

        Self { pipeline }
    }
}

impl PostPass for MenuPass {
    fn run(&self, world: &mut World) {
        let mut post_textures = world.resource_mut::<PostTextures>();
        let texture_state = post_textures.write();

        // log::info!("menu post texture state: {}", texture_state.state());

        let screen_binding = world.resource::<ScreenBinding>();
        let screen_bind_group = screen_binding.bind_group.clone();

        let menu_binding = world.resource::<DebugMenuBinding>();
        let menu_bind_group = menu_binding.bind_group.clone();

        let surface_state = world.resource::<SurfaceState>();
        let workgroups = util::get_workgroup_count_from_size(
            UVec3::new(8, 8, 1),
            UVec3 {
                x: surface_state.config.width,
                y: surface_state.config.height,
                z: 1,
            },
        );

        let mut frame = world.resource_mut::<FrameRecord>();

        let mut compute_pass = frame
            .encoder
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("menu_pass"),
                timestamp_writes: None,
            });

        compute_pass.set_pipeline(&self.pipeline);
        compute_pass.set_bind_group(0, &screen_bind_group, &[]);
        compute_pass.set_bind_group(1, &texture_state.bind_group, &[]);
        compute_pass.set_bind_group(2, &menu_bind_group, &[]);

        compute_pass.dispatch_workgroups(workgroups.x, workgroups.y, workgroups.z);
    }
}
