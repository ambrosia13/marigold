use std::{collections::HashMap, ffi::OsStr, path::Path};

use glam::{Quat, Vec3};
use gltf::{Gltf, mesh::Mode};

use crate::{
    app::data::scene::{
        bvh::BoundingVolume,
        geometry::mesh::{MeshInstance, MeshTriangle, MeshVertex, UnserializedMesh},
    },
    util,
};

struct GltfMeshInstance {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    pub mesh_index: (usize, usize),
}

pub struct GltfScene {
    pub meshes: HashMap<(usize, usize), UnserializedMesh>,
    instances: Vec<GltfMeshInstance>,
}

impl GltfScene {
    // doesn't preserve scene data, just collects mesh
    pub fn load<P: AsRef<Path>>(path: P) -> Self {
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
                    let data = std::fs::read(path.join(uri))
                        .expect("failed to get uri data for gltf mesh");
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

                        // no need to check remainder since Mode == Triangles?
                        let triangles: Vec<_> = reader
                            .read_indices()
                            .expect("couldn't read mesh indices")
                            .into_u32()
                            .array_chunks::<3>()
                            .map(|indices| MeshTriangle { indices })
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

        let instances: Vec<GltfMeshInstance> = gltf
            .scenes()
            .flat_map(|s| s.nodes())
            .filter(|n| n.mesh().is_some())
            .flat_map(|node| {
                let (position, rotation, scale) = node.transform().decomposed();
                let mesh = node.mesh().unwrap();

                mesh.primitives()
                    .filter(|p| p.mode() == Mode::Triangles)
                    .map(move |p| GltfMeshInstance {
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

        Self { meshes, instances }
    }

    pub fn into_meshes_and_instances(self) -> (Vec<UnserializedMesh>, Vec<MeshInstance>) {
        let mut packed_mesh_indices: HashMap<(usize, usize), usize> = HashMap::new();

        (
            self.meshes
                .into_iter()
                .enumerate()
                .map(|(packed, (sparse, m))| {
                    packed_mesh_indices.insert(sparse, packed);
                    m
                })
                .collect(),
            self.instances
                .into_iter()
                .map(|i| MeshInstance {
                    position: i.position,
                    rotation: i.rotation,
                    scale: i.scale,
                    mesh_index: packed_mesh_indices[&i.mesh_index],
                })
                .collect(),
        )
    }
}
