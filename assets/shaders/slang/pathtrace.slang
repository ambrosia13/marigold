module pathtrace;

import "lib/pass/geometry";
import "lib/screen";
import "lib/random";
import "lib/math";

struct Background {
    SamplerState sampler;
    TextureCube texture;
}

[[numthreads(8, 8, 1)]]
[[shader("compute")]]
void compute(
    uniform ParameterBlock<Screen> screen,
    uniform ParameterBlock<Background> background,
    uniform ParameterBlock<GeometryTextures> textures,
    
    uint3 globalInvocationId: SV_DispatchThreadID,
    uint3 localInvocationId: SV_GroupThreadID,
    uint localInvocationIndex: SV_GroupIndex,
) {
    if (globalInvocationId.x >= screen.view.width || globalInvocationId.y >= screen.view.height) {
        return;
    }

    float2 texcoord = float2(globalInvocationId.xy) / float2(screen.view.width, screen.view.height);
    float3 viewDir = screen.camera.viewDir(texcoord);

    float3 color = background.texture.SampleLevel(background.sampler, viewDir, 0).rgb;

    // if (texcoord.x < 0.5) {
    //     color = viewDir;
    // }

    textures.current.Store(globalInvocationId.xy, float4(color, 1.0));
}