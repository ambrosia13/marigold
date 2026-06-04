// use bevy_ecs::{
//     component::Component,
//     entity::Entity,
//     query::Without,
//     system::{Commands, Query, Res},
// };
// use derived_deref::{Deref, DerefMut};

// use crate::app::render::SurfaceState;

// #[derive(Component, Deref, DerefMut)]
// pub struct EcsShaderSource(pub Vec<u8>);

// #[derive(Component)]
// pub struct EcsShaderModuleDescriptor {
//     pub label: String,
//     pub source: Entity,
// }

// #[derive(Component, Deref, DerefMut)]
// pub struct EcsShaderModule(pub wgpu::ShaderModule);

// #[derive(Component)]
// pub struct EcsComputePipelineDescriptor {
//     pub label: String,
//     pub module: Entity,
//     pub entry_point: Option<String>,
// }

// pub struct EcsComputePipeline

// fn create_shader_module(mut commands: Commands) {
//     let source = commands.spawn(EcsShaderSource(vec![])).id();

//     let desc = commands.spawn(EcsShaderModuleDescriptor {
//         label: String::from("main_shader"),
//         source,
//     });
// }

// fn init_shader_modules(
//     surface_state: Res<SurfaceState>,
//     mut commands: Commands,
//     source_query: Query<&EcsShaderSource>,
//     desc_query: Query<(Entity, &EcsShaderModuleDescriptor), Without<EcsShaderModule>>,
// ) {
//     for (entity, desc) in desc_query {
//         let source = source_query.get(desc.source).unwrap();

//         let shader_module =
//             surface_state
//                 .gpu
//                 .device
//                 .create_shader_module(wgpu::ShaderModuleDescriptor {
//                     label: Some(&desc.label),
//                     source: wgpu::ShaderSource::SpirV(bytemuck::cast_slice(source).into()),
//                 });

//         commands
//             .entity(entity)
//             .insert(EcsShaderModule(shader_module));
//     }
// }
