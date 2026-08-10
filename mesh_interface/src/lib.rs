use bvh::{AsBoundingVolume, AsBoundingVolumeIndices, BoundingVolume};
use bytemuck::{AnyBitPattern, Pod, Zeroable};
use derived_deref::Deref;
use glam::{Mat3A, Mat4, Vec2, Vec3, Vec3A};
use vulkano::buffer::BufferContents;

#[derive(AnyBitPattern, Default, Clone, Copy)]
#[repr(C)]
pub struct MeshVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
}

impl MeshVertex {
    pub fn transform(self, transform: Mat4, normal_matrix: Mat3A) -> Self {
        Self {
            position: (transform * self.position.extend(1.0)).truncate(),
            normal: (normal_matrix * self.normal).normalize(),
            uv: self.uv,
        }
    }
}

#[derive(Deref, Default, Clone, Copy)]
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

pub struct Scene {
    pub name: String,
    pub instances: Vec<MeshInstance>,
}

impl Scene {
    /// convert the scene to a single mesh, to reduce indirection and potentially gain runtime performance
    /// at the cost of GPU memory usage
    pub fn flatten_instances(&mut self, meshes: &[UnserializedMesh]) -> UnserializedMesh {
        let mut mesh = UnserializedMesh {
            vertices: Vec::new(),
            triangles: Vec::new(),
            bounds: BoundingVolume::EMPTY,
        };

        for instance in &self.instances {
            let instance_mesh = &meshes[instance.mesh_index];
            let normal_matrix = Mat3A::from_mat4(instance.transform.inverse()).transpose();

            let vertex_offset = mesh.vertices.len();
            mesh.vertices.extend(
                instance_mesh
                    .vertices
                    .iter()
                    .map(|v| v.transform(instance.transform, normal_matrix)),
            );

            for vertex in mesh.vertices.iter().skip(vertex_offset) {
                mesh.bounds.max = mesh.bounds.max.max(vertex.position.into());
                mesh.bounds.min = mesh.bounds.min.min(vertex.position.into());
            }

            mesh.triangles
                .extend(instance_mesh.triangles.iter().map(|t| MeshTriangle {
                    indices: t.indices.map(|i| i + vertex_offset as u32),
                }));
        }

        self.instances = vec![MeshInstance {
            transform: Mat4::IDENTITY,
            mesh_index: 0,
        }];

        mesh
    }
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
pub struct UploadedMesh {
    pub bounds_min: Vec3A,
    pub vertex_offset: u32,
    pub bounds_max: Vec3A,
    pub triangle_offset: u32,
    pub transform: Mat4,
    pub triangle_count: u32,
    pub blas_root: u32,
}

impl AsBoundingVolume for UploadedMesh {
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
