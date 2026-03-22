use std::{collections::HashMap, ffi::OsStr, path::Path, sync::Arc};

use bevy_ecs::{
    component::Component,
    resource::Resource,
    system::{Commands, ResMut},
};
use derived_deref::Deref;
use glam::Vec3;
use gltf::{Gltf, mesh::Mode};
use gpu_layout::{AsGpuBytes, GpuBytes};

use crate::{
    app::data::scene::{
        bvh::{AsBoundingVolume, BoundingVolume},
        geometry::{BlasNodes, MeshTriangles, MeshVertices, Meshes},
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

        BoundingVolume::new(min, max)
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

#[derive(AsGpuBytes, Default)]
pub struct MeshMetadata {
    pub vertex_offset: u32,
    pub triangle_offset: u32,
    pub blas_root: u32,
}

// doesn't preserve scene data, just collects mesh
fn collect_meshes_from_gltf<P: AsRef<Path>>(path: P) -> Vec<UnserializedMesh> {
    let path = util::get_asset_path(path);
    let error_string = format!("gltf path {} wasn't valid", path.to_string_lossy());

    // path represents the directory the .gltf and all associated assets are kept, so find the gltf file
    let gltf_path = path
        .read_dir()
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().unwrap() == OsStr::new("gltf"))
        .expect(&error_string);

    let gltf = Gltf::open(gltf_path).expect(&error_string);

    let bin_data = gltf.blob.as_deref();
    let mut uri_data: HashMap<&str, Vec<u8>> = HashMap::new();

    for buffer in gltf.buffers() {
        match buffer.source() {
            gltf::buffer::Source::Bin => {}
            gltf::buffer::Source::Uri(uri) => {
                let data =
                    std::fs::read(path.join(uri)).expect("failed to get uri data for gltf mesh");
                uri_data.insert(uri, data);
            }
        }
    }

    let meshes: Vec<UnserializedMesh> = gltf
        .meshes()
        .flat_map(|mesh| {
            mesh.primitives()
                .filter(|p| p.mode() == Mode::Triangles)
                .map(|primitive| {
                    let reader = primitive.reader(|buf| match buf.source() {
                        gltf::buffer::Source::Bin => bin_data,
                        gltf::buffer::Source::Uri(uri) => Some(uri_data.get(&uri).unwrap()),
                    });

                    let vertices: Vec<_> = reader
                        .read_positions()
                        .expect("couldn't read mesh positions")
                        .map(Vec3::from)
                        .zip(
                            reader
                                .read_normals()
                                .expect("couldn't read mesh normals")
                                .map(Vec3::from),
                        )
                        .zip(
                            reader
                                .read_tex_coords(0)
                                .expect("couldn't read mesh uvs from TEXCOORD_0")
                                .into_f32(),
                        )
                        .map(|((p, n), u)| MeshVertex {
                            position: p,
                            uv_x: u[0],
                            normal: n,
                            uv_y: u[1],
                        })
                        .collect();

                    let vertices = Arc::new(vertices);

                    // no need to check remainder since Mode == Triangles?
                    let triangles: Vec<_> = reader
                        .read_indices()
                        .expect("couldn't read mesh indices")
                        .into_u32()
                        .array_chunks::<3>()
                        .map(|[a, b, c]| MeshTriangleWithPtr([a, b, c], vertices.clone()))
                        .collect();

                    let primitive_bounding_box = primitive.bounding_box();

                    UnserializedMesh {
                        vertices,
                        triangles,
                        bounds: BoundingVolume {
                            min: primitive_bounding_box.min.into(),
                            max: primitive_bounding_box.max.into(),
                            empty: primitive_bounding_box.min == primitive_bounding_box.max,
                        },
                    }
                })
        })
        .collect();

    for (i, mesh) in meshes.iter().enumerate() {
        log::info!(
            "Mesh #{} of mesh at path {} has {} vertices and {} triangles",
            i,
            &path.to_string_lossy(),
            mesh.vertices.len(),
            mesh.triangles.len()
        );
    }

    meshes
}

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
    let asset_root = util::get_asset_root();
    let mesh_dir_path = util::get_asset_path("meshes");

    for entry in std::fs::read_dir(&mesh_dir_path).unwrap() {
        let entry = entry.unwrap();
        let mesh_name = entry.file_name();
        let unserialized_meshes = collect_meshes_from_gltf(Path::new("meshes").join(&mesh_name));

        for mesh in unserialized_meshes {
            let metadata_index = meshes.len();

            let record = MeshRecord {
                label: mesh_name.to_string_lossy().into(),
                bounds: mesh.bounds,
                metadata_index,
            };

            log::info!(
                "serializing mesh and building BLAS for mesh '{}'",
                &record.label
            );
            super::serialize_meshes(
                &mut mesh_vertices,
                &mut mesh_triangles,
                &mut meshes,
                &mut blases,
                mesh,
            );

            loaded_meshes.records.push(record);
        }
    }
}
