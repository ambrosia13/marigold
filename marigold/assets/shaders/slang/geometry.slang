import lib.vertex;
import lib.camera;

struct VertexOutput {
    float4 ndcPos: SV_Position;
    float3 position;
    float3 normal;
    float2 uv;
}

struct FragmentOutput {
    float4 color;
}

[[shader("vertex")]]
VertexOutput vertex(
    uint drawIndex: SV_DrawIndex,

    // push constants
    uniform uint* indices,
    uniform MeshVertex* vertices,
    uniform CameraUniform* camera,
) {
    uint vertexIndex = indices[drawIndex];
    MeshVertex vertex = vertices[vertexIndex];

    VertexOutput output;

    output.ndcPos = mul(camera.viewProjectionMatrix, float4(vertex.position, 1.0));
    output.position = vertex.position;
    output.normal = vertex.normal;
    output.uv = vertex.uv;

    return output;
}

[[shader("fragment")]]
FragmentOutput fragment(
    VertexOutput input,
) {
    FragmentOutput output;

    output.color = float4(input.normal * 0.5 + 0.5, 1.0);

    return output;
}
