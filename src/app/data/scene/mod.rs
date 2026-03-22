use bevy_ecs::{
    change_detection::DetectChanges,
    resource::Resource,
    system::{Commands, Res, ResMut},
};
use glam::{Mat4, Quat, Vec3};
use gpu_layout::{AsGpuBytes, Std140Layout};
use wgpu::util::DeviceExt;

use crate::app::{
    data::scene::geometry::{
        BlasNodes, BlasNodesBuffer, GeometryId, MeshTriangles, MeshTrianglesBuffer, MeshVertices,
        MeshVerticesBuffer, Meshes, MeshesBuffer,
    },
    render::SurfaceState,
};

pub mod bvh;
pub mod geometry;

#[derive(AsGpuBytes)]
struct Counts {
    pub object_count: u32,

    pub triangle_vertex_count: u32,
    pub triangle_count: u32,
    pub mesh_count: u32,

    pub blas_node_count: u32,
    pub tlas_node_count: u32,
}

#[derive(Resource)]
pub struct SceneBinding {
    pub counts_buffer: wgpu::Buffer,

    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl SceneBinding {
    pub fn init(
        mut commands: Commands,
        surface_state: Res<SurfaceState>,
        mesh_vertices: Res<MeshVertices>,
        mesh_vertices_buffer: Res<MeshVerticesBuffer>,
        mesh_triangles: Res<MeshTriangles>,
        mesh_triangles_buffer: Res<MeshTrianglesBuffer>,
        meshes: Res<Meshes>,
        meshes_buffer: Res<MeshesBuffer>,
        blas_nodes: Res<BlasNodes>,
        blas_nodes_buffer: Res<BlasNodesBuffer>,
    ) {
        log::info!("creating scene binding");

        let gpu = &surface_state.gpu;

        let counts = Counts {
            object_count: 0,
            triangle_vertex_count: mesh_vertices.len() as u32,
            triangle_count: mesh_triangles.len() as u32,
            mesh_count: meshes.len() as u32,
            blas_node_count: blas_nodes.len() as u32,
            tlas_node_count: 0,
        };

        let counts_buffer = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("scene_counts_buffer"),
                contents: counts.as_gpu_bytes::<Std140Layout>().as_slice(),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("scene_bind_group_layout"),
                    entries: &[
                        // counts buffer
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // triangle vertices
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // triangles
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // meshes
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // blas nodes
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                });

        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: counts_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: mesh_vertices_buffer.as_buffer_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: mesh_triangles_buffer.as_buffer_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: meshes_buffer.as_buffer_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: blas_nodes_buffer.as_buffer_binding(),
                },
            ],
        });

        commands.insert_resource(Self {
            counts_buffer,
            bind_group,
            bind_group_layout,
        });
    }

    pub fn update(
        mut scene_binding: ResMut<SceneBinding>,
        surface_state: Res<SurfaceState>,
        mesh_vertices: Res<MeshVertices>,
        mesh_vertices_buffer: Res<MeshVerticesBuffer>,
        mesh_triangles: Res<MeshTriangles>,
        mesh_triangles_buffer: Res<MeshTrianglesBuffer>,
        meshes: Res<Meshes>,
        meshes_buffer: Res<MeshesBuffer>,
        blas_nodes: Res<BlasNodes>,
        blas_nodes_buffer: Res<BlasNodesBuffer>,
    ) {
        let gpu = &surface_state.gpu;

        if mesh_vertices.is_changed()
            || mesh_triangles.is_changed()
            || meshes.is_changed()
            || blas_nodes.is_changed()
        {
            // update counts
            let counts = Counts {
                object_count: 0,
                triangle_vertex_count: mesh_vertices.len() as u32,
                triangle_count: mesh_triangles.len() as u32,
                mesh_count: meshes.len() as u32,
                blas_node_count: blas_nodes.len() as u32,
                tlas_node_count: 0,
            };

            log::info!("scene counts changed, updating counts buffer");

            gpu.queue.write_buffer(
                &scene_binding.counts_buffer,
                0,
                counts.as_gpu_bytes::<Std140Layout>().as_slice(),
            );
        }

        if mesh_vertices_buffer.is_changed()
            || mesh_triangles_buffer.is_changed()
            || meshes_buffer.is_changed()
            || blas_nodes_buffer.is_changed()
        {
            log::info!("scene buffer(s) reallocated, recreating scene binding");

            // recreate bind group
            scene_binding.bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("scene_bind_group"),
                layout: &scene_binding.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: scene_binding.counts_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: mesh_vertices_buffer.as_buffer_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: mesh_triangles_buffer.as_buffer_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: meshes_buffer.as_buffer_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: blas_nodes_buffer.as_buffer_binding(),
                    },
                ],
            });
        }
    }
}

pub struct Object {
    transform: Transform,
    geometry_type: GeometryId,
}

#[derive(Clone, Copy)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Transform {
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    pub fn from_matrix(matrix: Mat4) -> Self {
        let (scale, rotation, translation) = matrix.to_scale_rotation_translation();
        Self {
            translation,
            rotation,
            scale,
        }
    }

    pub fn to_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}
