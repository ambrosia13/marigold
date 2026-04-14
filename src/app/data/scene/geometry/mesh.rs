use std::path::Path;

use bevy_ecs::{
    resource::Resource,
    system::{Commands, ResMut},
};
use derived_deref::Deref;
use glam::{Mat4, Quat, Vec3, Vec3A, Vec4Swizzles};
use gpu_layout::{AsGpuBytes, GpuBytes};
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};

use crate::{
    app::data::scene::{
        BLAS_MAX_DEPTH,
        bvh::{AsBoundingVolume, AsBoundingVolumeIndices, BoundingVolume, BoundingVolumeHierarchy},
        geometry::{BlasNodes, MeshTriangles, MeshVertices, Meshes, gltf::GltfScene},
    },
    util,
};

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

// state/info of a mesh before it's been prepared to upload to the gpu
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

        buf.write(&self.bounds_min.to_vec3());
        buf.write(&self.vertex_offset);
        buf.write(&self.bounds_max.to_vec3());
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
        BoundingVolume::new(
            (self.transform * self.bounds_min.extend(1.0)).xyz().into(),
            (self.transform * self.bounds_max.extend(1.0)).xyz().into(),
        )
    }
}

#[allow(unused)]
pub struct MeshRecord {
    pub label: String,
    // want to keep track of bounds on cpu-side so we can normalize, and place on ground, etc.
    pub bounds: BoundingVolume,
    pub metadata_index: usize,
}

#[derive(Resource)]
pub struct LoadedMeshes {
    pub records: Vec<MeshRecord>,
}

impl LoadedMeshes {
    pub fn init(mut commands: Commands) {
        commands.insert_resource(Self {
            records: Vec::new(),
        });
    }
}

pub fn load_all_mesh_assets(
    mut loaded_meshes: ResMut<LoadedMeshes>,
    mut mesh_vertices: ResMut<MeshVertices>,
    mut mesh_triangles: ResMut<MeshTriangles>,
    mut meshes: ResMut<Meshes>,
    mut blases: ResMut<BlasNodes>,
) {
    let mesh_dir_path = util::get_asset_path("meshes");

    for entry in std::fs::read_dir(&mesh_dir_path).unwrap() {
        let entry = entry.unwrap();
        let mesh_name = entry.file_name();

        let gltf_scene = GltfScene::load(Path::new("meshes").join(&mesh_name));
        let (mut unserialized_meshes, instances) = gltf_scene.into_meshes_and_instances();

        // build bvhs in parallel
        let bvhs: Vec<_> = unserialized_meshes
            .par_iter_mut()
            .map(|mesh| {
                BoundingVolumeHierarchy::new(
                    &mut mesh.triangles,
                    &mesh.vertices,
                    Some(mesh.bounds),
                    BLAS_MAX_DEPTH,
                )
            })
            .collect();

        // upload meshes and their bvhs in pairs
        let mesh_records: Vec<_> = unserialized_meshes
            .into_iter()
            .zip(bvhs)
            .enumerate()
            .map(|(i, (mesh, bvh))| {
                let metadata_index = meshes.len();

                let record = MeshRecord {
                    label: mesh_name.to_string_lossy().into(),
                    bounds: mesh.bounds,
                    metadata_index,
                };

                log::info!(
                    "serializing mesh and BLAS for sub-mesh {} of mesh '{}'",
                    i,
                    &record.label
                );

                loaded_meshes.records.push(record);

                super::serialize_mesh(
                    &mut mesh_vertices,
                    &mut mesh_triangles,
                    &mut blases,
                    mesh,
                    bvh,
                )
            })
            .collect();

        // spawn each instance in the gltf scene
        for instance in instances {
            let record = &mesh_records[instance.mesh_index];

            let mesh_metadata = MeshMetadata {
                bounds_min: record.bounds_min,
                vertex_offset: record.vertex_offset,
                bounds_max: record.bounds_max,
                triangle_offset: record.triangle_offset,
                triangle_count: record.triangle_count,
                blas_root: record.blas_root,
                transform: instance.transform,
            };

            meshes.push(mesh_metadata);
        }
    }
}
