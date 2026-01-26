use bevy_ecs::{
    resource::Resource,
    system::{Commands, Res},
};

use crate::app::render::SurfaceState;

pub struct PostTextureState {
    pub input: wgpu::Texture,
    pub input_view: wgpu::TextureView,
    pub output: wgpu::Texture,
    pub output_view: wgpu::TextureView,

    pub sampler: wgpu::Sampler,

    pub bind_group: wgpu::BindGroup,
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
}

impl PostTextures {
    pub fn init(mut commands: Commands, surface_state: Res<SurfaceState>) {
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
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
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
        let alt_view = main.create_view(&Default::default());

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
                    label: Some("post_pass_main_to_alt_bind_group"),
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
        };

        commands.insert_resource(post_textures);
        log::info!("created post pass textures")
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
            }
        }
    }
}

pub struct MenuPass {}
