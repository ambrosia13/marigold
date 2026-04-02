use std::sync::Arc;

use derived_deref::Deref;
use glam::Vec3;
use gpu_layout::{AsGpuBytes, GpuBytes};

use crate::app::data::scene::bvh::{AsBoundingVolume, BoundingVolume};

#[derive(AsGpuBytes, Default, Clone, Copy)]
pub struct MeshVertex {
    pub position: Vec3,
    pub uv_x: f32,
    pub normal: Vec3,
    pub uv_y: f32,
}

#[derive(Deref, AsGpuBytes, Default, Clone, Copy)]
pub struct MeshTriangle {
    pub indices: [u32; 3],
}

impl From<MeshTriangleWithPtr> for MeshTriangle {
    fn from(value: MeshTriangleWithPtr) -> Self {
        Self { indices: value.0 }
    }
}

#[derive(Deref, Clone)]
pub struct MeshTriangleWithPtr(#[target] pub [u32; 3], Arc<Vec<MeshVertex>>);

impl AsBoundingVolume for MeshTriangleWithPtr {
    fn bounding_volume(&self) -> BoundingVolume {
        let [a, b, c] = self.0;
        let v1 = &self.1[a as usize];
        let v2 = &self.1[b as usize];
        let v3 = &self.1[c as usize];

        let min = v1.position.min(v2.position).min(v3.position);
        let max = v1.position.max(v2.position).max(v3.position);

        BoundingVolume::new(min.to_vec3a(), max.to_vec3a())
    }
}

impl AsGpuBytes for MeshTriangleWithPtr {
    fn as_gpu_bytes<L: gpu_layout::GpuLayout + ?Sized>(&self) -> gpu_layout::GpuBytes<'_, L> {
        let mut buf = GpuBytes::empty();

        buf.write(&self.0[0]).write(&self.0[1]).write(&self.0[2]);

        buf
    }
}

// state/info of a mesh before it's been prepared to upload to the gpu
pub struct UnserializedMesh {
    pub vertices: Arc<Vec<MeshVertex>>,
    pub triangles: Vec<MeshTriangleWithPtr>,
    pub bounds: BoundingVolume,
}

impl AsBoundingVolume for UnserializedMesh {
    fn bounding_volume(&self) -> BoundingVolume {
        self.bounds
    }
}

pub struct GltfScene {}
