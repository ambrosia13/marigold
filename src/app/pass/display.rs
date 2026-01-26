use bevy_ecs::{
    resource::Resource,
    system::{Commands, Res},
};

use crate::app::{pass::post::PostTextures, render::SurfaceState};

#[derive(Resource)]
pub struct DisplayBinding {
    sampler: wgpu::Sampler,

    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl DisplayBinding {
    pub fn init(
        mut commands: Commands,
        surface_state: Res<SurfaceState>,
        post_textures: Res<PostTextures>,
    ) {
        // the last output of the post textures is the input to the display pass
        let (input_texture, input_texture_view) = post_textures.current_output();

        let sample_type = input_texture
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

        let bind_group = surface_state
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("display_bind_group"),
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&input_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });

        let display_binding = Self {
            sampler,
            bind_group,
            bind_group_layout,
        };

        commands.insert_resource(display_binding);
        log::info!("created display binding");
    }
}
