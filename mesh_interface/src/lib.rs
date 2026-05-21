use bvh::{AsBoundingVolume, AsBoundingVolumeIndices, BoundingVolume};
use derived_deref::Deref;
use glam::{Mat4, Vec3, Vec3A};
use gpu_layout::{AsGpuBytes, GpuBytes};

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

impl AsBoundingVolumeIndices<MeshVertex> for MeshTriangle {
    fn bounding_volume(&self, source: &[MeshVertex]) -> BoundingVolume {
        let v1 = &source[self.indices[0] as usize];
        let v2 = &source[self.indices[1] as usize];
        let v3 = &source[self.indices[2] as usize];

        let min = v1.position.min(v2.position).min(v3.position);
        let max = v1.position.max(v2.position).max(v3.position);

        BoundingVolume::new(min.to_vec3a(), max.to_vec3a())
    }
}

/// state/info of a mesh before it's been prepared to upload to the gpu
#[derive(Clone)]
pub struct UnserializedMesh {
    pub vertices: Vec<MeshVertex>,
    pub triangles: Vec<MeshTriangle>,
    pub bounds: BoundingVolume,
}

pub struct MeshInstance {
    pub transform: Mat4,
    pub mesh_index: usize,
}

impl AsBoundingVolume for UnserializedMesh {
    fn bounding_volume(&self) -> BoundingVolume {
        self.bounds
    }
}

#[derive(Default, Debug, Clone)]
pub struct MeshMetadata {
    pub bounds_min: Vec3A,
    pub vertex_offset: u32,
    pub bounds_max: Vec3A,
    pub triangle_offset: u32,
    pub transform: Mat4,
    pub triangle_count: u32,
    pub blas_root: u32,
}

impl AsGpuBytes for MeshMetadata {
    fn as_gpu_bytes<L: gpu_layout::GpuLayout + ?Sized>(&self) -> GpuBytes<'_, L> {
        let mut buf = GpuBytes::empty();

        buf.write(&self.bounds_min);
        buf.write(&self.vertex_offset);
        buf.write(&self.bounds_max);
        buf.write(&self.triangle_offset);
        buf.write(&self.triangle_count);
        buf.write(&self.blas_root);

        let inverse_transform = self.transform.inverse();

        buf.write(&self.transform);
        buf.write(&inverse_transform);

        buf
    }
}

impl AsBoundingVolume for MeshMetadata {
    fn bounding_volume(&self) -> BoundingVolume {
        BoundingVolume::new(self.bounds_min, self.bounds_max)
    }
}

pub struct MeshRecord {
    pub label: String,
    // want to keep track of bounds on cpu-side so we can normalize, and place on ground, etc.
    pub bounds: BoundingVolume,
    pub metadata_index: usize,
}
