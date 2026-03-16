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

- can be single index, where e.g. the 3 most significant bits will denote the geometry type, and the 29 least significant bits will denote the index into that geometry's bound array
- types:
    - sphere
    - aabb
    - quad?
    - mesh
- the meaning of the index will depend on the type
    - sphere, aabb, etc.: index into sphere, aabb, etc. array
    - mesh: index into BLAS array 

## bounding volume hierarchy structure

- use a TLAS/BLAS approach (top level, bottom level acceleration structure)
    - TLAS is a BVH over the whole scene, where leaf nodes are indexes/references to Object structs, which contain information about the geometry type and index
        - if the geometry type is a non-mesh, use the intersection function for the geometry primitive
        - otherwise, if mesh, traverse BLAS
        - note: TLAS bounding boxes are in world space
    - BLAS is a BVH over individual meshes. this can be reused for multiple instances of meshes to save memory. the object stores a transform matrix (rotation, scale, translation) while the BVH of the mesh will be in local space
- the TLAS is built over Objects and BvhNodes, while the BLAS is built over Triangles and BvhNodes. both are done by index so we can share all data in a global Triangle array, a TLAS BvhNode array, and a BLAS BvhNode array. We separate these arrays since the TLAS needs to account for geometry type, while the BLAS is simpler and is only for triangles, so we can skip the object multiplexing.

# data layout

object layout:
- transform translation xyz (12 bytes)
- geometry id (2 bytes, total 14 bytes)
- material id (2 bytes, total 16 bytes)
- transform scale (12 bytes, total 28 bytes)
- volume id (2 bytes, total 30 bytes)
- 2 unused bytes, total 32 bytes
- rotation (16 bytes, total 48 bytes)

bounding volume node layout (shared between TLAS and BLAS):
- bounds min (12 bytes)
- start index (4 bytes, total 16 bytes)
    - in TLAS, refers to index in object list; in BLAS refers to index in mesh list
- bounds max (12 bytes, total 28 bytes)
- length (4 bytes, total 32 bytes)
- child index (4 bytes, align +12 bytes, total 48 bytes)

triangle vertex layout:
- position xyz (12 bytes)
- uv x (4 bytes, total 16 bytes)
- normal xyz (12 bytes, total 28 bytes)
- uv y (4 bytes, total 32 bytes)

triangle layout:
- vertex a index (4 bytes)
- vertex b index (4 bytes, total 8 bytes)
- vertex c index (4 bytes, total 12 bytes)

mesh layout: 
- triangle start index (4 bytes)
- length (4 bytes, total 8 bytes)
