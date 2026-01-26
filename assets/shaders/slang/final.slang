module final;

import lib.vertex;

struct PostOutput {
    Texture2D texture;
    SamplerState sampler;
}

[[shader("fragment")]]
float4 fragment(
    VertexOutput input,
    uniform ParameterBlock<PostOutput> postOutput,
): SV_Target {
    // float4 color = postOutput.texture.Sample(postOutput.sampler, input.uv);
    float4 color = float4(input.uv, 0.0, 1.0);
    color = max(float4(0.0), color);

    color.rgb = pow(color.rgb, float3(1.0 / 2.2));
    
    return color;
}