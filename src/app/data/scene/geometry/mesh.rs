use std::{collections::HashMap, ffi::OsStr, path::Path, sync::Arc};

use bevy_ecs::{
    resource::Resource,
    system::{Commands, ResMut},
};
use derived_deref::Deref;
use glam::{Mat4, Quat, Vec3, Vec3A, Vec4, Vec4Swizzles};
use gltf::{Gltf, mesh::Mode};
use gpu_layout::{AsGpuBytes, GpuBytes};
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};

use crate::{
    app::data::scene::{
        BLAS_MAX_DEPTH,
        bvh::{AsBoundingVolume, BoundingVolume, BoundingVolumeHierarchy},
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

struct MeshInstance {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    pub mesh_index: (usize, usize),
}

pub struct GltfScene {
    meshes: HashMap<(usize, usize), UnserializedMesh>,
    instances: Vec<MeshInstance>,
}

#[derive(Default, Debug, Clone)]
pub struct MeshMetadata {
    pub bounds_min: Vec3A,
    pub vertex_offset: u32,
    pub bounds_max: Vec3A,
    pub triangle_offset: u32,
    pub position: Vec3,
    pub triangle_count: u32,
    pub scale: Vec3,
    pub blas_root: u32,
    pub rotation: Quat,
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

        let transform =
            Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position);

        let inverse_transform = transform.inverse();

        buf.write(&transform);
        buf.write(&inverse_transform);

        buf
    }
}

impl AsBoundingVolume for MeshMetadata {
    fn bounding_volume(&self) -> BoundingVolume {
        let transform =
            Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position);

        BoundingVolume::new(
            (transform * self.bounds_min.extend(1.0)).xyz().into(),
            (transform * self.bounds_max.extend(1.0)).xyz().into(),
        )
    }
}

// doesn't preserve scene data, just collects mesh
fn load_gltf<P: AsRef<Path>>(path: P) -> GltfScene {
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

    let mut meshes: HashMap<(usize, usize), UnserializedMesh> = HashMap::new();

    gltf.meshes()
        // don't care about mesh-primitive hierarchy, flatten
        .for_each(|mesh| {
            let mesh_index = mesh.index();

            mesh.primitives()
                .filter(|p| p.mode() == Mode::Triangles)
                .for_each(|primitive| {
                    if meshes.contains_key(&(mesh_index, primitive.index())) {
                        return; // avoid processing duplicates
                    }

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
                        .map(|indices| MeshTriangleWithPtr(indices, vertices.clone()))
                        .collect();

                    let primitive_bounding_box = primitive.bounding_box();
                    let primitive_bounding_volume = BoundingVolume {
                        min: primitive_bounding_box.min.into(),
                        max: primitive_bounding_box.max.into(),
                        empty: primitive_bounding_box.min == primitive_bounding_box.max,
                    };

                    let mesh = UnserializedMesh {
                        vertices,
                        triangles,
                        bounds: primitive_bounding_volume,
                    };

                    meshes.insert((mesh_index, primitive.index()), mesh);
                });
        });

    let instances: Vec<MeshInstance> = gltf
        .scenes()
        .flat_map(|s| s.nodes())
        .filter(|n| n.mesh().is_some())
        .flat_map(|node| {
            let (position, rotation, scale) = node.transform().decomposed();
            let mesh = node.mesh().unwrap();

            mesh.primitives()
                .filter(|p| p.mode() == Mode::Triangles)
                .map(move |p| MeshInstance {
                    position: position.into(),
                    rotation: Quat::from_array(rotation),
                    scale: scale.into(),
                    mesh_index: (mesh.index(), p.index()),
                })
        })
        .collect();

    for (i, mesh) in meshes.iter() {
        log::info!(
            "Mesh #{:?} of mesh at path {} has {} vertices and {} triangles",
            i,
            &path.to_string_lossy(),
            mesh.vertices.len(),
            mesh.triangles.len()
        );
    }

    log::info!("Creating {} mesh instances", instances.len());

    GltfScene { meshes, instances }
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
    let mesh_dir_path = util::get_asset_path("meshes");

    for entry in std::fs::read_dir(&mesh_dir_path).unwrap() {
        let entry = entry.unwrap();
        let mesh_name = entry.file_name();
        let gltf_scene = load_gltf(Path::new("meshes").join(&mesh_name));

        let mut packed_mesh_indices: HashMap<(usize, usize), usize> = HashMap::new();

        let mut unserialized_meshes: Vec<_> = gltf_scene
            .meshes
            .into_iter()
            .enumerate()
            .map(|(packed, (sparse, m))| {
                packed_mesh_indices.insert(sparse, packed);
                m
            })
            .collect();

        let instances = gltf_scene.instances;

        // build bvhs in parallel
        let bvhs: Vec<_> = unserialized_meshes
            .par_iter_mut()
            .map(|mesh| {
                BoundingVolumeHierarchy::new(&mut mesh.triangles, Some(mesh.bounds), BLAS_MAX_DEPTH)
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
            let record = &mesh_records[packed_mesh_indices[&instance.mesh_index]];

            let mesh_metadata = MeshMetadata {
                bounds_min: record.bounds_min,
                vertex_offset: record.vertex_offset,
                bounds_max: record.bounds_max,
                triangle_offset: record.triangle_offset,
                position: instance.position,
                triangle_count: record.triangle_count,
                scale: instance.scale,
                blas_root: record.blas_root,
                rotation: instance.rotation,
            };

            meshes.push(mesh_metadata);
        }
    }
}
