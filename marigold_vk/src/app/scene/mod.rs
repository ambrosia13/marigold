use std::{
    fs::FileType,
    path::{Path, PathBuf},
};

use bevy_ecs::{
    component::Component,
    entity::Entity,
    query::With,
    system::{Commands, Res, Single},
};
use gltf_loading::GltfScenes;
use mesh_interface::{Scene, UnserializedMesh};
use vulkano::{
    DeviceAddress,
    acceleration_structure::{AccelerationStructure, AccelerationStructureInstance},
    buffer::{Buffer, BufferCreateFlags, BufferCreateInfo, BufferUsage},
    memory::allocator::{AllocationCreateInfo, DeviceLayout, MemoryTypeFilter},
};

use crate::{util, vk::GpuHandle, window::schedules::SystemResult};

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
    pub mesh_buffers: Vec<(Buffer, Buffer)>,
    pub instance_buffers: Vec<Buffer>,
}

/// system to enumerate all scenes
pub fn enumerate_models(mut commands: Commands) -> SystemResult {
    let model_dir_root_path = util::get_asset_path("models");

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
    gpu: Res<GpuHandle>,
    mut commands: Commands,
    query: Single<(Entity, &ModelInfo, &ModelData), With<ActiveModel>>,
) -> SystemResult {
    let (entity, info, data) = *query;

    log::info!(
        "Uploading model {}'s data to the GPU, and deleting the CPU copy",
        info.name
    );

    let mut mesh_buffers: Vec<Buffer> = Vec::with_capacity(data.meshes.len());
    let mut instance_buffers: Vec<Buffer> = Vec::with_capacity(data.scenes.len());

    for mesh in &data.meshes {
        let buffer_ci = BufferCreateInfo {
            usage: BufferUsage::TRANSFER_SRC,
            ..Default::default()
        };

        let alloc_ci = AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        };

        let layout = DeviceLayout::for_value(mesh.vertices.as_slice()).unwrap();

        let staging_buffer = Buffer::new(&gpu.memory_allocator, &buffer_ci, &alloc_ci, layout)?;
    }

    Ok(())
}
