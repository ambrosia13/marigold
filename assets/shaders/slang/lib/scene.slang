import "intersect";
import "stack";

public struct Scene {
    public ConstantBuffer<Counts> counts;

    public StructuredBuffer<TriangleVertex> triangleVertices;
    public StructuredBuffer<Triangle> triangles;
    public StructuredBuffer<Mesh> meshes;

    public StructuredBuffer<BvhNode> blasNodes;
    public StructuredBuffer<BvhNode> tlasNodes;

    public Hit rayIntersectTriangle(Ray ray, uint vertexOffset, uint triangleIndex) {
        Hit hit = Hit.NO_HIT;
        Triangle triangle = triangles[triangleIndex];

        uint i1 = triangle.i1 + vertexOffset;
        uint i2 = triangle.i2 + vertexOffset;
        uint i3 = triangle.i3 + vertexOffset;

        TriangleVertex v1 = triangleVertices[i1];
        TriangleVertex v2 = triangleVertices[i2];
        TriangleVertex v3 = triangleVertices[i3];

        // float3 v1_pos = float3(2.0f,  1.0f,  0.0f); // Top center
        // float3 v2_pos = float3(2.0f, -1.0f, -1.0f); // Bottom left
        // float3 v3_pos = float3(2.0f, -1.0f,  1.0f); // Bottom right

        // float3 edge1 = v2_pos - v1_pos;
        // float3 edge2 = v3_pos - v1_pos;
        
        float3 edge1 = v2.position - v1.position;
        float3 edge2 = v3.position - v1.position;
        
        float3 h = cross(ray.dir, edge2);
        float determinant = dot(h, edge1);

        if (abs(determinant) < 1e-6) {
            return hit;
        }

        float f = 1.0 / determinant;
        float3 s = ray.pos - v1.position;
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

        float w = 1.0 - u - v;

        float3 planeNormal = normalize(cross(edge1, edge2));
        float dirDotNormal = dot(ray.dir, planeNormal);

        hit.success = true;
        hit.position = ray.pos + t * ray.dir;
        hit.distance = t;
        // we want to give physically accurate normals even for back faces
        hit.normal = (v1.normal * w + v2.normal * u + v3.normal * v) * -sign(dirDotNormal);
        hit.frontFace = dirDotNormal < 0.0;
        hit.uv = float2(
            v1.uvX,
            v1.uvY
        );

        return hit;
    }

    public Hit raytraceTlas(Ray ray, out uint3 traversalInfo) {
        Stack<uint, 32> nodeStack;

        Hit closestHit = Hit.NO_HIT;

        BvhNode root = tlasNodes[0];
        BvhHit rootHit = rayIntersectBoundingVolume(ray, root.boundsMin, root.boundsMax);

        // miss the bounding volume, no hit
        if (!rootHit.success) {
            return closestHit;
        }

        nodeStack.push(0);
        
        while (!nodeStack.isEmpty()) {
            uint nodeIndex = nodeStack.pop();
            traversalInfo.x++; // nodes visited
            
            // add the mesh's root node offset because all blas-es share the same list
            BvhNode node = tlasNodes[nodeIndex];
            // BvhHit nodeHit = rayIntersectBoundingVolume(ray, node.boundsMin, node.boundsMax);
            // traversalInfo.y++; // bounding box intersection tests

            // skip this node if the ray doesn't hit it
            // or if we've already found a closer hit
            // if (!nodeHit.success || (closestHit.success && closestHit.distance < nodeHit.distance)) {
            //     continue;
            // }

            if (node.childNode != 0) {
                // node has children, push to stack so we can test them next
                BvhNode childA = tlasNodes[node.childNode];
                BvhNode childB = tlasNodes[node.childNode + 1];

                BvhHit childAHit = rayIntersectBoundingVolume(ray, childA.boundsMin, childA.boundsMax);
                BvhHit childBHit = rayIntersectBoundingVolume(ray, childB.boundsMin, childB.boundsMax);
                traversalInfo.y += 2;

                bool pushChildA = childAHit.success && !(closestHit.success && closestHit.distance < childAHit.distance);
                bool pushChildB = childBHit.success && !(closestHit.success && closestHit.distance < childBHit.distance);

                if (pushChildA && pushChildB) {
                    nodeStack.push(select(childAHit.distance < childBHit.distance, node.childNode + 1, node.childNode));
                    nodeStack.push(select(childAHit.distance < childBHit.distance, node.childNode, node.childNode + 1));
                } else if (pushChildA) {
                    nodeStack.push(node.childNode);
                } else if (pushChildB) {
                    nodeStack.push(node.childNode + 1);
                }
            } else {
                // node has no children, trace BLASes directly
                for (uint i = node.startIndex; i < node.startIndex + node.length; i++) {
                    Hit hit = raytraceBlas(ray, closestHit, i, traversalInfo);
                    closestHit = Hit.merge(closestHit, hit);
                    traversalInfo.z++; // triangle intersection tests
                }
            }
        }

        return closestHit;
    }

    public Hit raytraceBlas(Ray ray, Hit closestHit, uint meshIndex, inout uint3 traversalInfo) {
        Mesh mesh = meshes[meshIndex];
        Stack<uint, 32> nodeStack;

        BvhHit meshHit = rayIntersectBoundingVolume(ray, mesh.boundsMin, mesh.boundsMax);

        // miss the bounding volume, no hit
        if (!meshHit.success) {
            return closestHit;
        }

        nodeStack.push(0);
        
        while (!nodeStack.isEmpty()) {
            uint nodeIndex = nodeStack.pop();
            traversalInfo.x++; // nodes visited
            
            // add the mesh's root node offset because all blas-es share the same list
            BvhNode node = blasNodes[mesh.blasRoot + nodeIndex];
            // BvhHit nodeHit = rayIntersectBoundingVolume(ray, node.boundsMin, node.boundsMax);
            // traversalInfo.y++; // bounding box intersection tests

            // skip this node if the ray doesn't hit it
            // or if we've already found a closer hit
            // if (!nodeHit.success || (closestHit.success && closestHit.distance < nodeHit.distance)) {
            //     continue;
            // }

            if (node.childNode != 0) {
                // node has children, push to stack so we can test them next
                BvhNode childA = blasNodes[mesh.blasRoot + node.childNode];
                BvhNode childB = blasNodes[mesh.blasRoot + node.childNode + 1];

                BvhHit childAHit = rayIntersectBoundingVolume(ray, childA.boundsMin, childA.boundsMax);
                BvhHit childBHit = rayIntersectBoundingVolume(ray, childB.boundsMin, childB.boundsMax);
                traversalInfo.y += 2;

                bool pushChildA = childAHit.success && !(closestHit.success && closestHit.distance < childAHit.distance);
                bool pushChildB = childBHit.success && !(closestHit.success && closestHit.distance < childBHit.distance);

                if (pushChildA && pushChildB) {
                    nodeStack.push(select(childAHit.distance < childBHit.distance, node.childNode + 1, node.childNode));
                    nodeStack.push(select(childAHit.distance < childBHit.distance, node.childNode, node.childNode + 1));
                } else if (pushChildA) {
                    nodeStack.push(node.childNode);
                } else if (pushChildB) {
                    nodeStack.push(node.childNode + 1);
                }
            } else {
                // node has no children, trace triangles directly
                for (uint i = node.startIndex; i < node.startIndex + node.length; i++) {
                    Hit hit = rayIntersectTriangle(ray, mesh.vertexOffset, mesh.triangleOffset + i);
                    closestHit = Hit.merge(closestHit, hit);
                    traversalInfo.z++; // triangle intersection tests
                }
            }
        }

        return closestHit;
    }
}

public struct Counts {
    public uint triangleVertexCount;
    public uint triangleCount;
    public uint meshCount;

    public uint blasNodeCount;
    public uint tlasNodeCount;
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
    public float3 boundsMin;
    public uint vertexOffset;
    public float3 boundsMax;
    public uint triangleOffset;
    public uint triangleCount; // for tracing directly
    public uint blasRoot;
}

public struct BvhHit {
    public bool success;
    public float distance;
}

public struct BvhNode {
    public float3 boundsMin;
    public uint startIndex;
    public float3 boundsMax;
    public uint length;
    public uint childNode;
}

public BvhHit rayIntersectBoundingVolume(Ray ray, float3 boundsMin, float3 boundsMax) {
    float3 t_min = (boundsMin - ray.pos) / ray.dir;
    float3 t_max = (boundsMax - ray.pos) / ray.dir;

    float3 t1 = min(t_min, t_max);
    float3 t2 = max(t_min, t_max);

    float t_near = max(max(t1.x, t1.y), t1.z);
    float t_far = min(min(t2.x, t2.y), t2.z);

    bool front_face = t_near == 0.0;

    if (front_face) { // ray inside box
        return BvhHit(true, t_far);   
    } else {
        return BvhHit(t_far >= t_near && t_far > 0.0, t_near);
    }
}
