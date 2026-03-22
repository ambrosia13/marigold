# scene

pseudocode structure:

```cpp
Scene {
    ObjectList[];

    // geometry
    TriangleVertexList[];
    TriangleList[];
    MeshList[];

    BLASNodes[];
    TLASNodes[];

    // materials
    LambertList[];
    MetalList[];
    TexturedList[];

    MaterialTextures[];
    Samplers[];
}
```

Each Object will be generic over specific properties, such as geometry, material, and volume (volumes are a later task). Each object probably also needs to store a transform to support rotations and scaling of the same geometry (essentially just instancing). 

Since each object stores a transform, primitive geometry such as spheres will not contain any information about position, radius, etc. since the same (and more) information can be encoded in the transform of a unit sphere. The same goes for quads and AABBs.

## geometry primitives

- since we are using transforms to encode shape information, primitives such as spheres, AABBs, etc. can be represented as unit spheres and unit boxes with transforms applied. as such, the  

## bounding volume hierarchy structure

- use a TLAS/BLAS approach (top level, bottom level acceleration structure)
    - TLAS is a BVH over the whole scene, where leaf nodes are indexes/references to Object structs, which contain information about the geometry type and index
        - if the geometry type is a non-mesh, use the intersection function for the geometry primitive
        - otherwise, if mesh, traverse BLAS
        - note: TLAS bounding boxes are in world space
    - BLAS is a BVH over individual meshes. this can be reused for multiple instances of meshes to save memory. the object stores a transform matrix (rotation, scale, translation) while the BVH of the mesh will be in local space
- the TLAS is built over Objects and BvhNodes, while the BLAS is built over Triangles and BvhNodes. both are done by index so we can share all data in a global Triangle array, a TLAS BvhNode array, and a BLAS BvhNode array. We separate these arrays since the TLAS needs to account for geometry type, while the BLAS is simpler and is only for triangles, so we can skip the object multiplexing.

# data layout

```
object layout:
- transform translation xyz                                                             (12 bytes)
- geometry id                                                                           (2 bytes, total 14 bytes)
- material id                                                                           (2 bytes, total 16 bytes)
- transform scale                                                                       (12 bytes, total 28 bytes)
- volume id                                                                             (2 bytes, total 30 bytes)
- unused                                                                                (2 bytes, total 32 bytes)
- transform rotation                                                                    (16 bytes, total 48 bytes)

bounding volume node layout (shared between TLAS and BLAS):
- bounds min                                                                            (12 bytes)
- start index                                                                           (4 bytes, total 16 bytes)
    - in TLAS, refers to index in object list; in BLAS refers to index in mesh list
- bounds max                                                                            (12 bytes, total 28 bytes)
- length                                                                                (4 bytes, total 32 bytes)
- child index                                                                           (4 bytes, align +12 bytes, total 48 bytes)

triangle vertex layout:
- position xyz                                                                          (12 bytes)
- uv x                                                                                  (4 bytes, total 16 bytes)
- normal xyz                                                                            (12 bytes, total 28 bytes)
- uv y                                                                                  (4 bytes, total 32 bytes)

triangle layout:
- vertex a index                                                                        (4 bytes)
- vertex b index                                                                        (4 bytes, total 8 bytes)
- vertex c index                                                                        (4 bytes, total 12 bytes)

mesh layout: 
- triangle start index                                                                  (4 bytes)
- triangle count                                                                        (4 bytes, total 8 bytes)
- triangle vertex index offset                                                          (4 bytes, total 12 bytes)
```

# traversal algorithm

```py
def traverse_tlas(ray):
    leaf = traverse_bvh(ray, tlas_root)
    hit = {}

    for obj in leaf:
        if obj.geometry.is_primitive():
            primitive_hit = ray.intersect(obj.transform * obj.geometry.get_primitive())
            primitive_hit = obj.transform.inv() * primitive_hit
            hit = merge(hit, primitive_hit)
        else if obj.geometry.is_mesh():
            mesh_hit = traverse_blas(ray, obj.transform, obj.geometry.get_mesh())
            hit = merge(hit, mesh_hit)

    return hit

def traverse_blas(transform, mesh, ray):
    ray = transform * ray

    leaf = traverse_bvh(ray, mesh.blas_root)
    hit = {}

    for triangle_index in leaf:
        # reason for mesh.triangle_vertex_offset and mesh.triangle_offset 
        # is that the triangle and vertex lists are global, instead of
        # local for each mesh, so each mesh stores offsets for these.
        
        # so the BLASes are built on the individual meshes, which have
        # local vertices/indices, so in order to access the global lists
        # we need the mesh offsets
        triangle_index += mesh.triangle_offset
        v1 = vertices[triangle.i1 + mesh.triangle_vertex_offset]
        v2 = vertices[triangle.i2 + mesh.triangle_vertex_offset]
        v3 = vertices[triangle.i3 + mesh.triangle_vertex_offset]

        triangle = Triangle(v1, v2, v3)
        triangle_hit = ray.intersect(triangle)

        hit = merge(hit, triangle_hit)

    return transform.inv() * hit

```