use std::{mem::MaybeUninit, num::NonZeroU64, ptr::NonNull, sync::Arc};

use bevy_ecs::{
    resource::Resource,
    system::{Commands, Res},
};
use bytemuck::{Pod, Zeroable};
use glam::{DVec2, Mat3, Mat4, Quat, Vec3};
use vulkano::{
    buffer::{Buffer, BufferCreateInfo, BufferMemory, BufferUsage},
    memory::allocator::{AllocationCreateInfo, DeviceLayout, MemoryTypeFilter},
};
use winit::{dpi::PhysicalSize, keyboard::KeyCode};

use crate::{
    app::{input::Input, time::Time},
    vk::{FRAMES_IN_FLIGHT, SurfaceState},
    window::schedules::SystemResult,
};

#[derive(Pod, Zeroable, Default, Clone, Copy)]
#[repr(C)]
pub struct CameraUniform {
    // represent matrixes as arrays so the type follows scalar alignment rules; glam's matrix types are always aligned to 16 bytes unless
    // scalar math is enabled across the whole crate (which would of course lose us some optimizations)
    view_projection_matrix: [f32; 16],
    view_matrix: [f32; 16],
    projection_matrix: [f32; 16],

    inverse_view_projection_matrix: [f32; 16],
    inverse_view_matrix: [f32; 16],
    inverse_projection_matrix: [f32; 16],

    previous_view_projection_matrix: [f32; 16],
    previous_view_matrix: [f32; 16],
    previous_projection_matrix: [f32; 16],

    position: Vec3,
    previous_position: Vec3,

    view: Vec3,
    previous_view: Vec3,

    right: Vec3,
    up: Vec3,
}

impl CameraUniform {
    #[allow(clippy::missing_transmute_annotations)]
    fn update_from(&mut self, camera: &Camera) {
        use std::mem::transmute;

        self.previous_projection_matrix = self.view_projection_matrix;
        self.previous_view_matrix = self.view_matrix;
        self.previous_projection_matrix = self.projection_matrix;

        // these transmutes should be safe because Mat4 is repr-C and is four Vec4s, which themselves are four floats,
        // so the representation of [f32; 16] and Mat4 is identical
        unsafe {
            self.view_matrix = transmute(camera.view_matrix());
            self.inverse_view_matrix = transmute(transmute::<_, Mat4>(self.view_matrix).inverse());

            self.projection_matrix = transmute(camera.projection_matrix());
            self.inverse_projection_matrix =
                transmute(transmute::<_, Mat4>(self.projection_matrix).inverse());

            self.view_projection_matrix = transmute(
                transmute::<_, Mat4>(self.projection_matrix)
                    * transmute::<_, Mat4>(self.view_matrix),
            );
            self.inverse_view_projection_matrix =
                transmute(transmute::<_, Mat4>(self.view_projection_matrix).inverse());
        }

        self.previous_position = self.position;
        self.position = camera.position;

        self.previous_view = self.view;
        self.view = camera.forward();

        self.right = camera.right();
        self.up = camera.up();
    }
}

#[derive(Resource)]
pub struct Camera {
    pub position: Vec3,
    pub rotation: Quat,

    pub movement_speed: f32,
    pub sensitivity: f32,
    pub fov: f32,

    aspect: f32,
    near: f32,
    far: f32,

    pitch: f64,
    yaw: f64,

    // things required to upload to the gpu
    pub buffers: [Arc<Buffer>; FRAMES_IN_FLIGHT],
    pub buffer_ptrs: [NonNull<[u8]>; FRAMES_IN_FLIGHT], // for cpu writes
    pub buffer_addresses: [NonZeroU64; FRAMES_IN_FLIGHT], // for gpu reads
}

unsafe impl Send for Camera {}
unsafe impl Sync for Camera {}

impl Camera {
    #[allow(clippy::missing_transmute_annotations)]
    pub fn init(mut commands: Commands, surface_state: Res<SurfaceState>) -> SystemResult {
        let surface_state: &SurfaceState = &surface_state;
        let window_size = surface_state.window.inner_size();

        let position = Vec3::ZERO;
        let target = Vec3::Z;
        let fov = 45.0;
        let aspect = window_size.width as f32 / window_size.height as f32;
        let near = 0.1;
        let far = 100.0;

        let movement_speed = 10.0;
        let sensitivity = 0.1;

        let (rotation, yaw, pitch) = Self::get_rotation_from_view_vector(position, target);

        use std::array::from_fn;
        let mut buffers: [_; FRAMES_IN_FLIGHT] = from_fn(|_| MaybeUninit::uninit());
        let mut buffer_ptrs: [_; FRAMES_IN_FLIGHT] = from_fn(|_| MaybeUninit::uninit());
        let mut buffer_addresses: [_; FRAMES_IN_FLIGHT] = from_fn(|_| MaybeUninit::uninit());

        for i in 0..buffers.len() {
            let buffer_ci = BufferCreateInfo {
                usage: BufferUsage::SHADER_DEVICE_ADDRESS,
                ..Default::default()
            };

            let alloc_ci = AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::HOST_SEQUENTIAL_WRITE
                    | MemoryTypeFilter::PREFER_DEVICE,
                ..Default::default()
            };

            let layout = DeviceLayout::new_sized::<CameraUniform>();

            let buffer = Buffer::new(
                &surface_state.gpu.memory_allocator,
                &buffer_ci,
                &alloc_ci,
                layout,
            )?;

            unsafe {
                surface_state
                    .gpu
                    .device
                    .set_debug_utils_object_name(&buffer, Some(&format!("Camera buffer {}", i)))
            }?;

            let BufferMemory::Normal(mem) = buffer.memory() else {
                panic!("camera buffer wasn't backed by normal memory");
            };

            let ptr = mem.mapped_slice(..).unwrap()?;

            let address = buffer.device_address();

            buffers[i] = MaybeUninit::new(buffer);
            buffer_ptrs[i] = MaybeUninit::new(ptr);
            buffer_addresses[i] = MaybeUninit::new(address);
        }

        commands.insert_resource(Self {
            position,
            rotation,
            movement_speed,
            sensitivity,
            fov,
            aspect,
            near,
            far,
            pitch,
            yaw,
            buffers: unsafe { std::mem::transmute(buffers) },
            buffer_ptrs: unsafe { std::mem::transmute(buffer_ptrs) },
            buffer_addresses: unsafe { std::mem::transmute(buffer_addresses) },
        });

        log::info!("initialized camera system");

        Ok(())
    }

    pub fn reconfigure_aspect(&mut self, window_size: PhysicalSize<u32>) {
        self.aspect = window_size.width as f32 / window_size.height as f32;
    }

    pub fn look_at(&mut self, target: Vec3) {
        let (rotation, yaw, pitch) = Self::get_rotation_from_view_vector(self.position, target);

        self.rotation = rotation;
        self.yaw = yaw;
        self.pitch = pitch;
    }

    pub fn forward(&self) -> Vec3 {
        self.rotation * Vec3::Z
    }

    pub fn forward_xz(&self) -> Vec3 {
        let forward = self.forward();
        Vec3::new(forward.x, 0.0, forward.z).normalize()
    }

    pub fn right(&self) -> Vec3 {
        -(self.rotation * Vec3::X)
    }

    pub fn right_xz(&self) -> Vec3 {
        let right = self.right();
        Vec3::new(right.x, 0.0, right.z).normalize()
    }

    pub fn up(&self) -> Vec3 {
        -(self.rotation * Vec3::Y)
    }

    fn yaw_quat(&self) -> Quat {
        Quat::from_rotation_y(self.yaw.to_radians() as f32)
    }

    fn pitch_quat(&self) -> Quat {
        Quat::from_rotation_x(self.pitch.to_radians() as f32)
    }

    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.position + self.forward(), Vec3::Y)
    }

    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(self.fov.to_radians(), self.aspect, self.near, self.far)
    }

    pub fn update_rotation(&mut self, mouse_delta: DVec2, sensitivity: f64) {
        let yaw_delta = -mouse_delta.x * sensitivity;
        let pitch_delta = -mouse_delta.y * sensitivity;

        self.yaw += yaw_delta;
        self.pitch += pitch_delta;
        self.pitch = self.pitch.clamp(-89.5, 89.5);

        self.rotation = (self.yaw_quat() * self.pitch_quat()).normalize();
    }

    fn update_position(&mut self, input: &Input, time: &Time) {
        let mut velocity = Vec3::ZERO;
        let forward = self.forward_xz();
        let right = self.right_xz();
        let up = Vec3::Y;

        if input.keys.pressed(KeyCode::KeyW) {
            velocity += forward;
        }
        if input.keys.pressed(KeyCode::KeyS) {
            velocity -= forward;
        }
        if input.keys.pressed(KeyCode::KeyD) {
            velocity += right;
        }
        if input.keys.pressed(KeyCode::KeyA) {
            velocity -= right;
        }
        if input.keys.pressed(KeyCode::Space) {
            velocity += up;
        }
        if input.keys.pressed(KeyCode::ShiftLeft) {
            velocity -= up;
        }

        velocity = velocity.normalize_or_zero();
        self.position += velocity * self.movement_speed * time.delta().as_secs_f32();
    }

    fn get_rotation_from_view_vector(position: Vec3, target: Vec3) -> (Quat, f64, f64) {
        let forward = (target - position).normalize();
        let right = Vec3::Y.cross(forward).normalize();
        let up = forward.cross(right);

        let matrix = Mat3::from_cols(right, up, forward);
        let rotation = Quat::from_mat3(&matrix);

        let yaw = ((forward.z).atan2(forward.x) as f64).to_degrees();
        let pitch = ((forward.y).asin() as f64).to_degrees();

        (rotation, yaw, pitch)
    }
}
