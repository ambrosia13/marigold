use std::path::Path;

use bevy_ecs::{
    resource::Resource,
    system::{Commands, ResMut},
};
use bvh::{BoundingVolumeHierarchy, BvhSettings};
use gltf_loading::GltfScene;
use mesh_interface::{MeshMetadata, MeshRecord};
use rayon::iter::{IndexedParallelIterator, IntoParallelRefMutIterator, ParallelIterator};

use crate::{
    app::data::scene::{
        BLAS_MAX_DEPTH,
        geometry::{BlasNodes, MeshTriangles, MeshVertices, Meshes},
    },
    util,
};

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

        let path = Path::new("meshes").join(&mesh_name);
        let path = util::get_asset_path(path);

        let gltf_scene = GltfScene::load(path);
        let (mut unserialized_meshes, instances) = gltf_scene.into_meshes_and_instances();

        let mesh_name = mesh_name.to_string_lossy();
        let mesh_name_str: &str = &mesh_name;

        // build bvhs in parallel
        let bvhs: Vec<_> = unserialized_meshes
            .par_iter_mut()
            .enumerate()
            .map(|(index, mesh)| {
                let settings = BvhSettings {
                    name: &format!("{}_{}", mesh_name_str, index),
                    bounds: Some(mesh.bounds),
                    max_depth: BLAS_MAX_DEPTH,
                    profiling_info: util::get_runtime_flag("PROFILING_INFO"),
                    profiling_info_directory: Some(&util::get_asset_root().join("bvh_debug")),
                    min_objects_per_leaf: 1,
                    max_objects_per_leaf: 1,
                };

                BoundingVolumeHierarchy::new(&mut mesh.triangles, &mesh.vertices, settings)
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
                    label: mesh_name_str.to_owned(),
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
