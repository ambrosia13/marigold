module blit;

import lib.vertex;

struct TextureInput {
    SamplerState sampler;
    Texture2D texture;
}

[[shader("fragment")]]
float4 fragment(
    VertexOutput input,
    uniform ParameterBlock<TextureInput> textureInput,
): SV_Target {
    return textureInput.texture.Sample(textureInput.sampler, input.uv);
}