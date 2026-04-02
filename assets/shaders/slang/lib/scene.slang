import "intersect";
import "stack";
import "math";

public static const uint TLAS_MAX_DEPTH = 8;
public static const uint BLAS_MAX_DEPTH = 24;
public static const uint BVH_STACK_SIZE = TLAS_MAX_DEPTH + BLAS_MAX_DEPTH;

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
        Stack<BvhNodePayload, BVH_STACK_SIZE> payloadStack;

        Hit closestHit = Hit.NO_HIT;

        BvhNode root = tlasNodes[0];
        BvhHit rootHit = rayIntersectBoundingVolume(ray, root.boundsMin, root.boundsMax);

        // miss the bounding volume, no hit
        if (!rootHit.success) {
            return closestHit;
        }

        payloadStack.push(root.payload());
        
        while (!payloadStack.isEmpty()) {
            BvhNodePayload payload = payloadStack.pop();
            traversalInfo.x++; // nodes visited

            uint childIndex = payload.childNode();

            if (childIndex != 0) {
                // node has children, push to stack so we can test them next
                uint childAIndex = childIndex;
                uint childBIndex = childAIndex + 1;

                BvhNode childA = tlasNodes[childAIndex];
                BvhNode childB = tlasNodes[childBIndex];

                BvhHit childAHit = rayIntersectBoundingVolume(ray, childA.boundsMin, childA.boundsMax);
                BvhHit childBHit = rayIntersectBoundingVolume(ray, childB.boundsMin, childB.boundsMax);
                traversalInfo.y += 2;

                bool pushChildA = childAHit.success && !(closestHit.success && closestHit.distance < childAHit.distance);
                bool pushChildB = childBHit.success && !(closestHit.success && closestHit.distance < childBHit.distance);

                if (pushChildA && pushChildB) {
                    payloadStack.push(select(childAHit.distance < childBHit.distance, childB.payload(), childA.payload()));
                    payloadStack.push(select(childAHit.distance < childBHit.distance, childA.payload(), childB.payload()));
                } else if (pushChildA) {
                    payloadStack.push(childA.payload());
                } else if (pushChildB) {
                    payloadStack.push(childB.payload());
                }
            } else {
                uint startIndex = payload.startIndex;
                uint length = payload.length();

                // node has no children, trace BLASes directly
                for (uint i = startIndex; i < startIndex + length; i++) {
                    // raytraceBlas modifies closestHit and also modifies payloadStack
                    raytraceBlas(ray, closestHit, i, payloadStack, traversalInfo);
                    // traversalInfo.z++; // triangle intersection tests
                }
            }
        }

        return closestHit;
    }

    public void raytraceBlas(
        Ray ray, 
        inout Hit closestHit, 
        uint meshIndex, 
        inout Stack<BvhNodePayload, BVH_STACK_SIZE> payloadStack, 
        inout uint3 traversalInfo
    ) {
        Mesh mesh = meshes[meshIndex];

        Ray localRay = Ray(
            mul(mesh.inverseTransform, float4(ray.pos, 1.0)).xyz,
            mul(mesh.inverseTransform, float4(ray.dir, 0.0)).xyz
        );

        BvhHit meshHit = rayIntersectBoundingVolume(localRay, mesh.boundsMin, mesh.boundsMax);

        // miss the bounding volume, no hit
        if (!meshHit.success) {
            return;
        }

        uint baseStackSize = payloadStack.size();
        payloadStack.push(blasNodes[mesh.blasRoot].payload());
        
        while (payloadStack.size() != baseStackSize) {
            BvhNodePayload payload = payloadStack.pop();
            traversalInfo.x++; // nodes visited

            uint childIndex = payload.childNode();

            if (childIndex != 0) {
                // node has children, push to stack so we can test them next
                BvhNode childA = blasNodes[mesh.blasRoot + childIndex];
                BvhNode childB = blasNodes[mesh.blasRoot + childIndex + 1];
                
                float closestHitDistance = select(closestHit.success, closestHit.distance, float.maxValue);

                BvhNodePayload payloadA = childA.payload();
                BvhNodePayload payloadB = childB.payload();

                BvhHit childAHit = rayIntersectBoundingVolume(localRay, childA.boundsMin, childA.boundsMax);
                BvhHit childBHit = rayIntersectBoundingVolume(localRay, childB.boundsMin, childB.boundsMax);
                traversalInfo.y += 2;

                bool pushChildA = childAHit.success && childAHit.distance < closestHitDistance;
                bool pushChildB = childBHit.success && childBHit.distance < closestHitDistance;

                // 1
                // if (pushChildA && pushChildB) {
                //     payloadStack.push(select(childAHit.distance < childBHit.distance, payloadB, payloadA));
                //     payloadStack.push(select(childAHit.distance < childBHit.distance, payloadA, payloadB));
                // } else if (pushChildA) {
                //     payloadStack.push(payloadA);
                // } else if (pushChildB) {
                //     payloadStack.push(payloadB);
                // }

                // 2
                if (pushChildA && pushChildB) {
                    payloadStack.push(select(childAHit.distance < childBHit.distance, payloadB, payloadA));
                    payloadStack.push(select(childAHit.distance < childBHit.distance, payloadA, payloadB));
                } else if (pushChildA || pushChildB) {
                    payloadStack.push(select(pushChildA, payloadA, payloadB));
                }
            } else {
                uint startIndex = payload.startIndex;
                uint length = payload.length();

                // node has no children, trace triangles directly
                for (uint i = startIndex; i < startIndex + length; i++) {
                    Hit hit = rayIntersectTriangle(localRay, mesh.vertexOffset, mesh.triangleOffset + i);

                    // transform hit
                    hit.position.xyz = mul(mesh.transform, float4(hit.position.xyz, 1.0)).xyz;
                    hit.normal = normalize(mul(transpose(float3x3(mesh.transform)), hit.normal));
                    hit.distance = distance(hit.position, ray.pos);

                    closestHit = Hit.merge(closestHit, hit);
                    traversalInfo.z++; // triangle intersection tests
                }
            }
        }
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
    public float4x4 transform;
    public float4x4 inverseTransform;
}

public struct BvhHit {
    public bool success;
    public float distance;
}

public struct BvhNode {
    public float3 boundsMin;
    public uint startIndex;
    public float3 boundsMax;
    uint childNodeLower25bits_lengthUpper7bits;

    public BvhNodePayload payload() {
        return BvhNodePayload(startIndex, childNodeLower25bits_lengthUpper7bits);
    }

    public uint childNode() {
        return childNodeLower25bits_lengthUpper7bits & ((1 << 25) - 1);
    }

    public uint length() {
        return childNodeLower25bits_lengthUpper7bits >> 25;
    }
}

public struct BvhNodePayload {
    public uint startIndex;
    public uint childNodeLower25bits_lengthUpper7bits;

    public uint childNode() {
        return childNodeLower25bits_lengthUpper7bits & ((1 << 25) - 1);
    }

    public uint length() {
        return childNodeLower25bits_lengthUpper7bits >> 25;
    }
}

public BvhHit rayIntersectBoundingVolume(Ray ray, float3 boundsMin, float3 boundsMax) {
    float3 invDir = 1.0 / ray.dir;
    
    float3 t_min = (boundsMin - ray.pos) * invDir;
    float3 t_max = (boundsMax - ray.pos) * invDir;

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

    // //            inside box    outside box
    // return BvhHit(front_face || t_far >= t_near && t_far > 0.0, select(front_face, t_far, t_near));
}
