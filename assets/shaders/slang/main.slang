import lib.header;
import lib.screen;
import lib.random;

struct VertexInput {
    float3 position;
    float tileIndex;
    float3 normal;
    float biomeIndex;
    float height;
    uint binningDepth;
}

public struct VertexOutput {
    public float4 clipPosition : SV_Position;
    public float3 position;
    public float tileIndex;
    public float3 normal;
    public float biomeIndex;
    public float height;
    public uint binningDepth;
    public float3 vertexColor;
}

[[shader("vertex")]]
VertexOutput vertex(
    ParameterBlock<Screen> screen,
    VertexInput input,
    uint vertexIndex: SV_VertexID,
) {
    float4 clipPosition = mul(screen.camera.viewProjectionMatrix, float4(input.position, 1.0));
    
    Random random = Random(vertexIndex);
    float3 vertexColor = float3(random.getFloat(), random.getFloat(), random.getFloat());

    return VertexOutput(
        clipPosition, 
        input.position, 
        input.tileIndex, 
        input.normal, 
        input.biomeIndex, 
        input.height,
        input.binningDepth,
        vertexColor
    );
}

[[shader("fragment")]]
float4 fragment(VertexOutput input) {
    return float4(input.vertexColor, 1.0);
}