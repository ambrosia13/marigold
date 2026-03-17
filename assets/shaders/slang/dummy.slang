import "lib/screen";
import "lib/pass/post";

[[numthreads(8, 8, 1)]]
[[shader("compute")]]
void compute(
    uniform ParameterBlock<Screen> screen,
    uniform ParameterBlock<PostTextureState> textures,
    
    uint3 globalInvocationId: SV_DispatchThreadID,
    uint3 localInvocationId: SV_GroupThreadID,
    uint localInvocationIndex: SV_GroupIndex,
) {
    if (globalInvocationId.x >= screen.view.width || globalInvocationId.y >= screen.view.height) {
        return;
    }

    textures.outputTexture.Store(globalInvocationId.xy, textures.inputTexture.Load(int3(globalInvocationId.xy, 0)));
}