use std::{path::Path, time::Instant};

use bevy_ecs::{
    change_detection::DetectChanges,
    component::Component,
    entity::Entity,
    query::With,
    resource::Resource,
    system::{Commands, ResMut, Single},
};
use bvh::{BoundingVolumeHierarchy, BvhSettings};
use glam::{Vec3A, Vec4};
use gltf_loading::GltfScenes;
use mesh_interface::{Scene, UnserializedMesh, UploadedMesh};
use rayon::iter::{IndexedParallelIterator, IntoParallelRefMutIterator, ParallelIterator};

use crate::{
    app::data::scene::{
        BLAS_MAX_DEPTH,
        geometry::{BlasNodes, MeshTriangles, MeshVertices, SerializedMesh, UploadedMeshes},
    },
    util,
};

/// A record of the current scene uploaded to the gpu buffers
#[derive(Resource)]
pub struct CurrentUploadedScene {
    pub model: Entity,
    pub scene: usize,
}

/// should be attached to one model to represent it's active. an active model means the user can
/// choose which scene in the model to load, which by default is the first available scene.
///
/// it also means that one scene of this model is uploaded to the gpu (i.e. currently being rendered).
/// to query which scene is uploaded, use the `active_scene` field of `Model`. you can also use the
/// `scene` field of the `CurrentUploadedScene` resource, but that's more for renderer bookkeeping to
/// decide when to reupload.
#[derive(Component)]
pub struct ActiveModel;

/// a loaded model. a model being loaded means it's parsed from the model file and is kept in-memory,
/// as well as the bounding volume hierarchies being constructed for each mesh. however, a model being
/// loaded does not mean that it's uploaded to the gpu; that information is given by the [`CurrentUploadedScene`]
/// resource.
#[derive(Component)]
pub struct Model {
    pub name: String,
    pub unserialized_meshes: Vec<UnserializedMesh>,
    pub bounding_volume_hierarchies: Vec<BoundingVolumeHierarchy>,
    pub scenes: Vec<Scene>,
    pub active_scene: usize,
}

pub fn load_all_models(mut commands: Commands) {
    let profiling_level = util::get_profiling_level();
    let model_dir_root_path = util::get_asset_path("meshes");

    // we keep track of the first model loaded
    let mut first_model = true;

    for entry in std::fs::read_dir(&model_dir_root_path).unwrap() {
        let instant = Instant::now();

        let entry = entry.unwrap();
        let model_dir_name = entry.file_name();
        let model_name = model_dir_name.to_string_lossy();
        let model_name_str: &str = &model_name;

        let path = Path::new("meshes").join(&model_dir_name);
        let path = util::get_asset_path(path);

        let gltf_scenes = GltfScenes::load(path);
        let (mut unserialized_meshes, scenes) = gltf_scenes.into_meshes_and_scenes();

        // build bvhs in parallel
        let bvhs: Vec<_> = unserialized_meshes
            .par_iter_mut()
            .enumerate()
            .map(|(index, mesh)| {
                let settings = BvhSettings {
                    name: &format!("{}_{}", model_name_str, index),
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

        // if this model has no defined scenes, skip loading it
        if scenes.is_empty() {
            log::warn!("model '{}' has no defined scenes; ignoring", model_name_str);
            continue;
        }

        let scene_count = scenes.len();

        let model = Model {
            name: model_name_str.to_owned(),
            unserialized_meshes,
            bounding_volume_hierarchies: bvhs,
            scenes,
            active_scene: 0, // set the first scene active until user decides otherwise
        };

        let mut entity = commands.spawn(model);

        // set the first loaded model as active until user decides otherwise
        if first_model {
            entity.insert(ActiveModel);
            first_model = false;

            let model_entity_id = entity.id();

            // to tell teh renderer what to upload initially
            commands.insert_resource(CurrentUploadedScene {
                model: model_entity_id,
                scene: 0,
            });
        }

        log::info!(
            "loading model file {} with {} scenes took {} ms",
            model_name_str,
            scene_count,
            instant.elapsed().as_secs_f64() * 1000.0
        );
    }
}

pub fn upload_current_scene(
    single: Single<(Entity, &Model), With<ActiveModel>>,
    mut current_scene: ResMut<CurrentUploadedScene>,
    mut mesh_vertices: ResMut<MeshVertices>,
    mut mesh_triangles: ResMut<MeshTriangles>,
    mut uploaded_meshes: ResMut<UploadedMeshes>,
    mut blas_nodes: ResMut<BlasNodes>,
) {
    let (model_entity, model) = *single;

    // if active model is different from our records of currently uploaded scene,
    // we clear the buffers and reupload
    if current_scene.is_added()
        || current_scene.model != model_entity
        || current_scene.scene != model.active_scene
    {
        log::info!(
            "scene changed, clearing buffers & reuploading scene '{}' (#{}) of model '{}'",
            model.scenes[model.active_scene].name,
            model.active_scene,
            model.name
        );

        mesh_vertices.clear();
        mesh_triangles.clear();
        blas_nodes.clear();
        uploaded_meshes.clear();

        // upload the meshes along with their bvhs
        let serialized_meshes: Vec<_> = model
            .unserialized_meshes
            .iter()
            .zip(model.bounding_volume_hierarchies.iter())
            .map(|(mesh, bvh)| {
                let serialized = SerializedMesh {
                    vertex_offset: mesh_vertices.len() as u32,
                    bounds_min: mesh.bounds.min,
                    triangle_offset: mesh_triangles.len() as u32,
                    bounds_max: mesh.bounds.max,
                    triangle_count: mesh.triangles.len() as u32,
                    blas_root: blas_nodes.len() as u32,
                };

                mesh_vertices.extend_from_slice(&mesh.vertices);
                mesh_triangles.extend_from_slice(&mesh.triangles);
                blas_nodes.extend_from_slice(bvh.nodes());

                serialized
            })
            .collect();

        let scene = &model.scenes[model.active_scene];

        for instance in &scene.instances {
            let mesh = &serialized_meshes[instance.mesh_index];

            // we need to transform the gltf local space AABB into a world space AABB for the TLAS
            let min = mesh.bounds_min;
            let max = mesh.bounds_max;

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

            let mesh_metadata = UploadedMesh {
                bounds_min: transformed_min,
                vertex_offset: mesh.vertex_offset,
                bounds_max: transformed_max,
                triangle_offset: mesh.triangle_offset,
                triangle_count: mesh.triangle_count,
                blas_root: mesh.blas_root,
                transform: instance.transform,
            };

            uploaded_meshes.push(mesh_metadata);
        }

        // update current scene record
        current_scene.model = model_entity;
        current_scene.scene = model.active_scene;
    }
}
