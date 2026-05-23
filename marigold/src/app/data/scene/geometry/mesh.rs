use std::{path::Path, time::Instant};

use bevy_ecs::{
    resource::Resource,
    system::{Commands, ResMut},
};
use bvh::{BoundingVolumeHierarchy, BvhSettings};
use glam::{Vec3A, Vec4};
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
    let profiling_level = util::get_profiling_level();

    let mesh_dir_path = util::get_asset_path("meshes");

    let num_initial_meshes = meshes.len();

    for entry in std::fs::read_dir(&mesh_dir_path).unwrap() {
        let entry = entry.unwrap();
        let mesh_dir_name = entry.file_name();
        let mesh_name = mesh_dir_name.to_string_lossy();
        let mesh_name_str: &str = &mesh_name;

        let instant = Instant::now();

        let path = Path::new("meshes").join(&mesh_dir_name);
        let path = util::get_asset_path(path);

        let gltf_scene = GltfScene::load(path);
        let (mut unserialized_meshes, instances) = gltf_scene.into_meshes_and_instances();

        // build bvhs in parallel
        let bvhs: Vec<_> = unserialized_meshes
            .par_iter_mut()
            .enumerate()
            .map(|(index, mesh)| {
                let settings = BvhSettings {
                    name: &format!("{}_{}", mesh_name_str, index),
                    bounds: Some(mesh.bounds),
                    max_depth: BLAS_MAX_DEPTH,
                    profiling_info: profiling_level != 0,
                    profiling_info_directory: if profiling_level == 2 {
                        Some(&util::get_asset_root().join("bvh_debug"))
                    } else {
                        None
                    },
                };

                BoundingVolumeHierarchy::new::<_, _, 1, 1>(
                    &mut mesh.triangles,
                    &mesh.vertices,
                    settings,
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
                    label: mesh_name_str.to_owned(),
                    bounds: mesh.bounds,
                    metadata_index,
                };

                log::info!(
                    "serializing mesh and BLAS for sub-mesh {} of mesh '{}'",
                    i,
                    record.label
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

            // we need to transform the gltf local space AABB into a world space AABB for the TLAS
            let min = record.bounds_min;
            let max = record.bounds_max;

            // simple matrix multiply won't work so we have to transform all 8 corners and then select
            // min and max from the transformed corners :(

            // pre-fill them to vec4s for the transformation
            let transformed_corners = [
                Vec4::new(min.x, min.y, min.z, 1.0),
                Vec4::new(min.x, min.y, max.z, 1.0),
                Vec4::new(min.x, max.y, min.z, 1.0),
                Vec4::new(min.x, max.y, max.z, 1.0),
                Vec4::new(max.x, min.y, min.z, 1.0),
                Vec4::new(max.x, min.y, max.z, 1.0),
                Vec4::new(max.x, max.y, min.z, 1.0),
                Vec4::new(max.x, max.y, max.z, 1.0),
            ]
            .into_iter()
            .map(|v| Vec3A::from_vec4(instance.transform * v));

            let mut transformed_min = Vec3A::INFINITY;
            let mut transformed_max = Vec3A::NEG_INFINITY;

            for corner in transformed_corners {
                transformed_min = transformed_min.min(corner);
                transformed_max = transformed_max.max(corner);
            }

            let mesh_metadata = MeshMetadata {
                bounds_min: transformed_min,
                vertex_offset: record.vertex_offset,
                bounds_max: transformed_max,
                triangle_offset: record.triangle_offset,
                triangle_count: record.triangle_count,
                blas_root: record.blas_root,
                transform: instance.transform,
            };

            meshes.push(mesh_metadata);
        }

        let num_tris_total = meshes
            .iter()
            .skip(num_initial_meshes)
            .map(|mesh| mesh.triangle_count)
            .sum::<u32>();

        log::info!(
            "scene '{}' with {} triangles took {:.3} ms to load",
            mesh_name_str,
            num_tris_total,
            instant.elapsed().as_secs_f64() * 1000.0
        );
    }
}
