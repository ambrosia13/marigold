use std::{num::NonZeroU64, path::PathBuf, sync::Arc};

use bevy_ecs::{
    component::Component,
    entity::Entity,
    query::{Added, With},
    system::{Commands, NonSendMut, ResMut, Single},
};
use gltf_loading::GltfScenes;
use mesh_interface::{Scene, UnserializedMesh};
use vulkano::buffer::{Buffer, BufferCreateInfo, BufferUsage};

use crate::{
    util,
    vk::{FrameRecord, SurfaceState},
    window::schedules::SystemResult,
};

/// attached to the model entity that is active
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct ActiveModel;

#[derive(Component)]
pub struct ModelInfo {
    pub name: String,
    pub path: PathBuf,
    pub active_scene: usize,
}

/// the cpu-side loaded model
#[derive(Component)]
pub struct ModelData {
    pub meshes: Vec<UnserializedMesh>,
    pub scenes: Vec<Scene>,
}

/// the gpu-side loaded model
#[derive(Component)]
pub struct UploadedModel {
    /// (vertex_buffer, index_buffer)
    pub mesh_buffers: Vec<(Arc<Buffer>, Arc<Buffer>)>,
    pub mesh_addresses: Vec<(NonZeroU64, NonZeroU64)>,

    pub instance_buffers: Vec<Arc<Buffer>>,
    pub instance_addresses: Vec<NonZeroU64>,
}

/// system to enumerate all scenes
pub fn enumerate_models(mut commands: Commands) -> SystemResult {
    let model_dir_root_path = util::get_asset_path("models");

    log::info!(
        "Model directory resolves to {}",
        model_dir_root_path.to_string_lossy()
    );

    if !std::fs::exists(&model_dir_root_path).unwrap_or(false) {
        log::warn!("assets/models directory does not exist, here be dragons");
    }

    // keep track of the first model so we can mark it as the default active
    let mut first_model = true;

    for entry in std::fs::read_dir(&model_dir_root_path)? {
        let entry = entry?;
        assert!(entry.file_type()?.is_dir());

        let model_dir_name = entry.file_name();
        let model_name = model_dir_name.to_string_lossy();

        log::info!("Found model file '{}'", model_name);

        let mut mesh_entity = commands.spawn(ModelInfo {
            name: model_name.into_owned(),
            path: entry.path(),
            active_scene: 0,
        });

        if first_model {
            mesh_entity.insert(ActiveModel);
            first_model = false;
        }
    }

    Ok(())
}

/// system that loads active model data to the cpu
pub fn load_active_model(
    mut commands: Commands,
    query: Single<(Entity, &ModelInfo), With<ActiveModel>>,
) {
    let (entity, info) = *query;

    log::info!("Loading model {}'s data to the CPU", info.name);

    let gltf = GltfScenes::load(&info.path);
    let (meshes, scenes) = gltf.into_meshes_and_scenes();

    commands.entity(entity).insert(ModelData { meshes, scenes });
}

/// system that uploads active model data to the gpu, then discards the cpu-side copy
pub fn upload_active_model(
    mut surface_state: ResMut<SurfaceState>,
    mut frame: NonSendMut<FrameRecord>,
    mut commands: Commands,
    query: Single<(Entity, &ModelInfo, &ModelData), Added<ActiveModel>>,
) -> SystemResult {
    // workaround because rust-analyzer isn't giving intellisense for Res<T>
    let surface_state: &mut SurfaceState = &mut surface_state;

    let (entity, info, data) = *query;

    log::info!(
        "Uploading model {}'s data to the GPU, and deleting the CPU copy",
        info.name
    );

    let mut mesh_buffers = Vec::with_capacity(data.meshes.len());
    let mut mesh_addresses = Vec::with_capacity(data.meshes.len());

    let mut instance_buffers = Vec::with_capacity(data.scenes.len());
    let mut instance_addresses = Vec::with_capacity(data.scenes.len());

    for (index, mesh) in data.meshes.iter().enumerate() {
        let vertex_buffer_ci = BufferCreateInfo {
            usage: BufferUsage::SHADER_DEVICE_ADDRESS
                | BufferUsage::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY
                | BufferUsage::TRANSFER_DST,
            ..Default::default()
        };

        let index_buffer_ci = BufferCreateInfo {
            usage: BufferUsage::SHADER_DEVICE_ADDRESS
                | BufferUsage::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY
                | BufferUsage::TRANSFER_DST,
            ..Default::default()
        };

        let vertex_buffer = frame.upload_buffer(
            surface_state,
            &mesh.vertices,
            &vertex_buffer_ci,
            Some(&format!(
                "model {}; mesh #{} vertex buffer",
                info.name, index
            )),
        )?;

        let index_buffer = frame.upload_buffer(
            surface_state,
            &mesh.triangles,
            &index_buffer_ci,
            Some(&format!(
                "model {}; mesh #{} index buffer",
                info.name, index
            )),
        )?;

        mesh_addresses.push((
            vertex_buffer.device_address(),
            index_buffer.device_address(),
        ));

        mesh_buffers.push((vertex_buffer, index_buffer));
    }

    for (index, instance) in data.scenes[info.active_scene].instances.iter().enumerate() {
        let instance_buffer_ci = BufferCreateInfo {
            usage: BufferUsage::SHADER_DEVICE_ADDRESS
                | BufferUsage::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY
                | BufferUsage::TRANSFER_DST,
            ..Default::default()
        };

        let instance_buffer = frame.upload_buffer(
            surface_state,
            &[instance.transform],
            &instance_buffer_ci,
            Some(&format!("model {}; instance #{}", info.name, index)),
        )?;

        instance_addresses.push(instance_buffer.device_address());
        instance_buffers.push(instance_buffer);
    }

    commands.entity(entity).insert(UploadedModel {
        mesh_buffers,
        mesh_addresses,
        instance_buffers,
        instance_addresses,
    });

    Ok(())
}

// pub fn unload_inactive_models(
//     mut surface_state: ResMut<SurfaceState>,
//     mut frame: NonSendMut<FrameRecord>,
//     mut commands: Commands,
//     query: Query<(Entity, &ModelInfo, &ModelData), Without<ActiveModel>>,
// ) -> SystemResult {
//     for (entity, info, )

//     Ok(())
// }
