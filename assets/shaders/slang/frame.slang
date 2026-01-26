module frame;

import lib.vertex;

struct Vertex {
    float2 position;
    float2 uv;
    float2 texcoord;
}

[[shader("vertex")]]
VertexOutput vertex(uint vertexIndex: SV_VulkanVertexID) {
    VertexOutput output;

    Vertex vertices[4] = {
        Vertex(float2(-1.0, -1.0), float2(0.0, 1.0), float2(0.0, 0.0)),
        Vertex(float2(1.0, -1.0), float2(1.0, 1.0), float2(1.0, 0.0)),
        Vertex(float2(1.0, 1.0), float2(1.0, 0.0), float2(1.0, 1.0)),
        Vertex(float2(-1.0, 1.0), float2(0.0, 0.0), float2(0.0, 1.0))
    };

    uint indices[6] = { 0, 1, 2, 0, 2, 3 };

    var index = indices[vertexIndex];
    var vertex = vertices[index];

    output.clipPosition = float4(vertex.position.xy, 0.0, 1.0);
    output.uv = vertex.uv;
    output.texcoord = vertex.texcoord;

    return output;
}