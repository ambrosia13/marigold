use bevy_ecs::{
    message::MessageWriter,
    resource::Resource,
    system::{Commands, Res, ResMut},
};
use glam::Vec3;
use gpu_layout::{AsGpuBytes, Std140Layout};

use crate::app::{messages::AtmosphereRebakeMessage, render::SurfaceState};

#[derive(Resource, AsGpuBytes, PartialEq, Clone)]
pub struct AtmosphereParams {
    pub sun_color: Vec3, // (realtime)
    pub ground_radius: f32,
    pub moon_color: Vec3, // (realtime)
    pub atmosphere_radius: f32,

    pub rayleigh_scattering_base: Vec3,
    pub rayleigh_absorption_base: f32,

    pub ozone_absorption_base: Vec3,
    pub mie_scattering_base: f32,

    pub ground_albedo: Vec3,
    pub mie_absorption_base: f32,

    pub origin: Vec3, // where the atmosphere origin is, by default starts at 500m below player (realtime)

    pub atmosphere_g: f32, // default: 0.76385

    pub sun_direction: Vec3,                // (realtime)
    pub moon_to_sun_illuminance_ratio: f32, // celestial contribution for night sky is calculated as moon_color * moon_to_sun_illuminance_ratio (realtime)

    pub moon_direction: Vec3, // (realtime)
    pub meters_per_unit: f32, // represents unit scale of all length values in this struct, default megameters (Mm) (realtime)

    // these values describe the scale of the density distribution in the atmosphere
    pub rayleigh_scale_height: f32, // default: 8.0 / 1000.0 = 0.008
    pub mie_scale_height: f32,      // default: 1.2 / 1000.0 = 0.0012

    pub transmittance_lut_steps: u32,          // default: 80
    pub multiscattering_lut_steps: u32,        // default: 40
    pub multiscattering_lut_sqrt_samples: u32, // default: 16
    pub sky_view_lut_steps: u32,               // default: 32 (realtime)
}

impl Default for AtmosphereParams {
    fn default() -> Self {
        let ground_radius = 6.360;
        let meters_per_unit = 1.0e6;

        Self {
            sun_color: Vec3::new(1.0, 0.85, 0.8),
            ground_radius,
            moon_color: Vec3::new(1.0, 1.0, 0.9),
            atmosphere_radius: 6.460,
            rayleigh_scattering_base: Vec3::new(5.802, 13.558, 33.1),
            rayleigh_absorption_base: 0.0,
            ozone_absorption_base: Vec3::new(0.650, 1.881, 0.085),
            mie_scattering_base: 25.996,
            ground_albedo: Vec3::splat(0.3),
            mie_absorption_base: 4.4,
            origin: Vec3::new(0.0, -ground_radius * meters_per_unit - 500.0, 0.0), // camera starts at 500m above the ground
            atmosphere_g: 0.76385,
            moon_to_sun_illuminance_ratio: 2.5e-6,
            meters_per_unit, // megameters
            rayleigh_scale_height: 8.0 / 1000.0,
            mie_scale_height: 1.2 / 1000.0,
            transmittance_lut_steps: 80,
            multiscattering_lut_steps: 40,
            multiscattering_lut_sqrt_samples: 16,
            sky_view_lut_steps: 32,
            sun_direction: Vec3::new(0.2, 0.9, 0.3).normalize(),
            moon_direction: Vec3::new(-0.5, -0.8, 0.1).normalize(),
        }
    }
}

impl AtmosphereParams {
    // compares only non-realtime fields, ie those used in the baking process
    pub fn should_rebake(&self, old: &Self) -> bool {
        self.ground_radius != old.ground_radius
            || self.atmosphere_radius != old.atmosphere_radius
            || self.rayleigh_scattering_base != old.rayleigh_scattering_base
            || self.rayleigh_absorption_base != old.rayleigh_absorption_base
            || self.ozone_absorption_base != old.ozone_absorption_base
            || self.mie_scattering_base != old.mie_scattering_base
            || self.ground_albedo != old.ground_albedo
            || self.mie_absorption_base != old.mie_absorption_base
            || self.atmosphere_g != old.atmosphere_g
            || self.rayleigh_scale_height != old.rayleigh_scale_height
            || self.mie_scale_height != old.mie_scale_height
            || self.transmittance_lut_steps != old.transmittance_lut_steps
            || self.multiscattering_lut_steps != old.multiscattering_lut_steps
            || self.multiscattering_lut_sqrt_samples != old.multiscattering_lut_sqrt_samples
    }

    pub fn init(mut commands: Commands) {
        commands.insert_resource(Self::default());
        log::info!("initialized atmosphere params");
    }
}

#[derive(Resource)]
pub struct AtmosphereBinding {
    // we keep a copy of the data we uploaded so we can check if it needs to be updated
    pub uploaded_params: AtmosphereParams,

    pub buffer: wgpu::Buffer,

    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

impl AtmosphereBinding {
    pub fn init(mut commands: Commands, surface_state: Res<SurfaceState>) {
        let uploaded_params = AtmosphereParams::default();

        let buffer = surface_state
            .gpu
            .device
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("atmosphere_buffer"),
                size: uploaded_params
                    .as_gpu_bytes::<Std140Layout>()
                    .as_slice()
                    .len() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

        let bind_group_layout =
            surface_state
                .gpu
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("atmosphere_bind_group_layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::all(),
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });

        let bind_group = surface_state
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("atmosphere_bind_group"),
                layout: &bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });

        commands.insert_resource(Self {
            uploaded_params,
            buffer,
            bind_group_layout,
            bind_group,
        });

        log::info!("created atmosphere binding and buffer");
    }

    pub fn update(
        surface_state: Res<SurfaceState>,
        atmosphere_params: Res<AtmosphereParams>,
        mut rebake_events: MessageWriter<AtmosphereRebakeMessage>,
        mut atmosphere_binding: ResMut<AtmosphereBinding>,
    ) {
        // if atmosphere_binding.uploaded_params == *atmosphere_params {
        //     // skip upload, because data is up to date
        //     return;
        // }

        if atmosphere_binding
            .uploaded_params
            .should_rebake(&atmosphere_params)
        {
            // signal that the bake passes should run again
            rebake_events.write(AtmosphereRebakeMessage);
        }

        log::info!("atmosphere params changed, re-uploading to gpu buffer");
        surface_state.gpu.queue.write_buffer(
            &atmosphere_binding.buffer,
            0,
            atmosphere_params.as_gpu_bytes::<Std140Layout>().as_slice(),
        );

        // keep a copy of the params we just uploaded for change detection
        atmosphere_binding.uploaded_params = atmosphere_params.clone();
    }
}
