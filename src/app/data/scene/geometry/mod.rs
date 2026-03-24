use bevy_ecs::{
    change_detection::{DetectChanges, DetectChangesMut},
    resource::Resource,
    system::{Commands, Res, ResMut},
};
use derived_deref::{Deref, DerefMut};
use gpu_layout::{AsGpuBytes, GpuBytes};

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

impl AsGpuBytes for GeometryId {
    fn as_gpu_bytes<L: gpu_layout::GpuLayout + ?Sized>(&self) -> GpuBytes<L> {
        let mut buf = GpuBytes::empty();
        buf.write(&self.encode());

        buf
    }
}

impl Default for GeometryId {
    fn default() -> Self {
        Self::Mesh(0)
    }
}

impl GeometryId {
    pub fn encode(self) -> u32 {
        match self {
            GeometryId::Sphere => u32::MAX,
            GeometryId::Aabb => u32::MAX - 1,
            GeometryId::Quad => u32::MAX - 2,
            GeometryId::Mesh(i) => i,
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

#[derive(Resource, Deref, DerefMut)]
pub struct TlasNodes(Vec<BvhNode>);

#[derive(Resource, Deref, DerefMut)]
pub struct TlasNodesBuffer(GpuVec<BvhNode>);

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

    let tlas_nodes = TlasNodes(Vec::new());
    let tlas_nodes_buffer = TlasNodesBuffer(GpuVec::new(
        gpu,
        "tlas_nodes_buffer",
        &tlas_nodes,
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
    commands.insert_resource(tlas_nodes);
    commands.insert_resource(tlas_nodes_buffer);
}

// NOT A SYSTEM
pub fn serialize_mesh(
    mesh_vertices: &mut ResMut<MeshVertices>,
    mesh_triangles: &mut ResMut<MeshTriangles>,
    meshes: &mut ResMut<Meshes>,
    blas_nodes: &mut ResMut<BlasNodes>,
    mesh: UnserializedMesh,
    bvh: BoundingVolumeHierarchy,
) {
    let mesh_metadata = MeshMetadata {
        vertex_offset: mesh_vertices.len() as u32,
        bounds_min: mesh.bounds.min,
        triangle_offset: mesh_triangles.len() as u32,
        bounds_max: mesh.bounds.max,
        triangle_count: mesh.triangles.len() as u32,
        blas_root: blas_nodes.len() as u32,
    };

    mesh_vertices.extend_from_slice(&mesh.vertices);
    mesh_triangles.extend(
        mesh.triangles
            .into_iter()
            .map(|t| MeshTriangle { indices: t.0 }),
    );
    blas_nodes.extend_from_slice(bvh.nodes());

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
    tlas_nodes: Res<TlasNodes>,
    mut tlas_nodes_buffer: ResMut<TlasNodesBuffer>,
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

    // check tlas nodes too as a workaround for change detection shenanigans
    if meshes.is_changed() || tlas_nodes.is_changed() {
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

    if tlas_nodes.is_changed() {
        if tlas_nodes_buffer.should_reallocate(&tlas_nodes) {
            tlas_nodes_buffer.reallocate_buffer(&tlas_nodes);
        } else {
            tlas_nodes_buffer.update_existing_buffer(&tlas_nodes);
        }
    }
}

pub fn update_tlas(mut tlas_nodes: ResMut<TlasNodes>, mut meshes: ResMut<Meshes>) {
    if meshes.is_changed() {
        log::info!("building TLAS over {} meshes", meshes.len());
        tlas_nodes.clear();

        // need to bypass change detection to avoid triggering infinite cycle
        let meshes = meshes.bypass_change_detection();

        let bvh = BoundingVolumeHierarchy::new(meshes, None);
        tlas_nodes.extend_from_slice(bvh.nodes());
    }
}
