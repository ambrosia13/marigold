import "intersect";

public struct Scene {
    public ConstantBuffer<Counts> counts;

    // public StructuredBuffer<Object> objects;

    public StructuredBuffer<TriangleVertex> triangleVertices;
    public StructuredBuffer<Triangle> triangles;
    public StructuredBuffer<Mesh> meshes;

    public StructuredBuffer<BvhNode> blasNodes;
    // public StructuredBuffer<BvhNode> tlasNodes;

    public Hit rayIntersectTriangle(Ray ray, uint triangleIndex) {
        Hit hit;
        Triangle triangle = triangles[triangleIndex];

        float3 edge1 = triangleVertices[triangle.i2].position - triangleVertices[triangle.i1].position;
        float3 edge2 = triangleVertices[triangle.i3].position - triangleVertices[triangle.i1].position;
        
        float3 h = cross(ray.dir, edge2);
        float determinant = dot(h, edge1);

        if (abs(determinant) < 1e-6) {
            return hit;
        }

        float f = 1.0 / determinant;
        float3 s = ray.pos - triangleVertices[triangle.i1].position;
        float u = f * dot(s, h);

        if (u < 0.0 || u > 1.0) {
            // outside triangle
            return hit;
        }

        float3 q = cross(s, edge1);
        float v = f * dot(ray.dir, q);

        if (v < 0.0 || u + v > 1.0) {
            // outside triangle
            return hit;
        }

        float t = f * dot(edge2, q);
        if (t < 1e-6) {
            return hit;
        }

        float3 planeNormal = normalize(cross(edge1, edge2));
        float dirDotNormal = dot(ray.dir, planeNormal);

        hit.success = true;
        hit.position = ray.pos + t * ray.dir;
        hit.distance = t;
        hit.normal = planeNormal;
        hit.frontFace = dirDotNormal < 0.0;
        hit.uv = float2(
            triangleVertices[triangle.i1].uvX,
            triangleVertices[triangle.i1].uvY
        );
    }
}

public struct Counts {
    public uint objectCount;

    public uint triangleVertexCount;
    public uint triangleCount;
    public uint meshCount;

    public uint blasNodeCount;
    public uint tlasNodeCount;
}

public struct Object {
    uint test;
}

public struct TriangleVertex {
    public float3 position;
    public float uvX;
    public float3 normal;
    public float uvY;
}

public struct Triangle {
    public uint i1;
    public uint i2;
    public uint i3;
}

public struct Mesh {
    uint test;
}

public struct BvhNode {
    public float3 boundsMin;
    public uint startIndex;
    public float3 boundsMax;
    public uint length;
    public uint childNode;
}
