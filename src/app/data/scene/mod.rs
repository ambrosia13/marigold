use glam::{Mat4, Quat, Vec3};

use crate::app::data::scene::geometry::GeometryId;

pub mod geometry;

pub struct Object {
    transform: Transform,
    geometry_type: GeometryId,
}

pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Transform {
    pub fn as_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}
