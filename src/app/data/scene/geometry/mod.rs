use bevy_ecs::{
    change_detection::DetectChanges,
    message::{MessageReader, Messages},
    resource::Resource,
    system::{Commands, Res, ResMut},
};
use derived_deref::{Deref, DerefMut};
use gpu_layout::{AsGpuBytes, GpuBytes, Std430Layout};
use rand::rand_core::le;

use crate::{
    app::{
        data::scene::{
            bvh::{BoundingVolumeHierarchy, BvhNode},
            geometry::mesh::{MeshMetadata, MeshTriangle, MeshVertex, UnserializedMesh},
        },
        render::SurfaceState,
    },
    util::buffer::GpuVec,
};

pub mod mesh;

#[derive(Clone, Copy)]
pub enum GeometryId {
    Sphere,
    Aabb,
    Quad,
    Mesh(u32),
}

impl GeometryId {
    pub fn encode(self) -> u16 {
        match self {
            GeometryId::Sphere => u16::MAX,
            GeometryId::Aabb => u16::MAX - 1,
            GeometryId::Quad => u16::MAX - 2,
            GeometryId::Mesh(i) => i as u16,
        }
    }
}

// need the below to be their own resource structs for change detection
// e.g. when MeshVertices is changed, update MeshVerticesBuffer
// when MeshVerticesBuffer is changed (buffer reallocated), update any bindings that contain it

#[derive(Resource, Deref, DerefMut)]
pub struct MeshVertices(Vec<MeshVertex>);

#[derive(Resource, Deref, DerefMut)]
pub struct MeshVerticesBuffer(GpuVec<MeshVertex>);

#[derive(Resource, Deref, DerefMut)]
pub struct MeshTriangles(Vec<MeshTriangle>);

#[derive(Resource, Deref, DerefMut)]
pub struct MeshTrianglesBuffer(GpuVec<MeshTriangle>);

#[derive(Resource, Deref, DerefMut)]
pub struct Meshes(Vec<MeshMetadata>);

#[derive(Resource, Deref, DerefMut)]
pub struct MeshesBuffer(GpuVec<MeshMetadata>);

#[derive(Resource, Deref, DerefMut)]
pub struct BlasNodes(Vec<BvhNode>);

#[derive(Resource, Deref, DerefMut)]
pub struct BlasNodesBuffer(GpuVec<BvhNode>);

pub fn init_geometry_buffers(mut commands: Commands, surface_state: Res<SurfaceState>) {
    log::info!("initializing scene geometry buffers");

    let gpu = &surface_state.gpu;

    let mesh_vertices = MeshVertices(Vec::new());
    let mesh_vertices_buffer = MeshVerticesBuffer(GpuVec::new(
        gpu,
        "mesh_vertices_buffer",
        &mesh_vertices,
        wgpu::BufferUsages::empty(),
    ));

    let mesh_triangles = MeshTriangles(Vec::new());
    let mesh_triangles_buffer = MeshTrianglesBuffer(GpuVec::new(
        gpu,
        "mesh_triangles_buffer",
        &mesh_triangles,
        wgpu::BufferUsages::empty(),
    ));

    let meshes = Meshes(Vec::new());
    let meshes_buffer = MeshesBuffer(GpuVec::new(
        gpu,
        "meshes_buffer",
        &meshes,
        wgpu::BufferUsages::empty(),
    ));

    let blas_nodes = BlasNodes(Vec::new());
    let blas_nodes_buffer = BlasNodesBuffer(GpuVec::new(
        gpu,
        "blas_nodes_buffer",
        &blas_nodes,
        wgpu::BufferUsages::empty(),
    ));

    commands.insert_resource(mesh_vertices);
    commands.insert_resource(mesh_vertices_buffer);
    commands.insert_resource(mesh_triangles);
    commands.insert_resource(mesh_triangles_buffer);
    commands.insert_resource(meshes);
    commands.insert_resource(meshes_buffer);
    commands.insert_resource(blas_nodes);
    commands.insert_resource(blas_nodes_buffer);
}

// NOT A SYSTEM
pub fn serialize_meshes(
    mesh_vertices: &mut ResMut<MeshVertices>,
    mesh_triangles: &mut ResMut<MeshTriangles>,
    meshes: &mut ResMut<Meshes>,
    blas_nodes: &mut ResMut<BlasNodes>,
    mut mesh: UnserializedMesh,
) {
    let blas = BoundingVolumeHierarchy::new(&mut mesh.triangles, Some(mesh.bounds));

    let mesh_metadata = MeshMetadata {
        vertex_offset: mesh_vertices.len() as u32,
        triangle_offset: mesh_triangles.len() as u32,
        blas_root: blas_nodes.len() as u32,
    };

    mesh_vertices.extend_from_slice(&mesh.vertices);
    mesh_triangles.extend(
        mesh.triangles
            .into_iter()
            .map(|t| MeshTriangle { indices: t.0 }),
    );
    blas_nodes.extend_from_slice(blas.nodes());

    meshes.push(mesh_metadata);
}

#[allow(clippy::too_many_arguments)]
pub fn update_geometry_buffers(
    mesh_vertices: Res<MeshVertices>,
    mut mesh_vertices_buffer: ResMut<MeshVerticesBuffer>,
    mesh_triangles: Res<MeshTriangles>,
    mut mesh_triangles_buffer: ResMut<MeshTrianglesBuffer>,
    meshes: Res<Meshes>,
    mut meshes_buffer: ResMut<MeshesBuffer>,
    blas_nodes: Res<BlasNodes>,
    mut blas_nodes_buffer: ResMut<BlasNodesBuffer>,
) {
    // this clunky logic is so that we only trigger change detection on the gpu buffers when they are reallocated
    // so we defer dereferencing it mutably until we know we must reallocate
    if mesh_vertices.is_changed() {
        if mesh_vertices_buffer.should_reallocate(&mesh_vertices) {
            mesh_vertices_buffer.reallocate_buffer(&mesh_vertices);
        } else {
            mesh_vertices_buffer.update_existing_buffer(&mesh_vertices);
        }
    }

    if mesh_triangles.is_changed() {
        if mesh_triangles_buffer.should_reallocate(&mesh_triangles) {
            mesh_triangles_buffer.reallocate_buffer(&mesh_triangles);
        } else {
            mesh_triangles_buffer.update_existing_buffer(&mesh_triangles);
        }
    }

    if meshes.is_changed() {
        if meshes_buffer.should_reallocate(&meshes) {
            meshes_buffer.reallocate_buffer(&meshes);
        } else {
            meshes_buffer.update_existing_buffer(&meshes);
        }
    }

    if blas_nodes.is_changed() {
        if blas_nodes_buffer.should_reallocate(&blas_nodes) {
            blas_nodes_buffer.reallocate_buffer(&blas_nodes);
        } else {
            blas_nodes_buffer.update_existing_buffer(&blas_nodes);
        }
    }
}
