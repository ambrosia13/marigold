module pathtrace;

import lib.pass.geometry;
import lib.screen;
import lib.random;
import lib.math;

[[numthreads(8, 8, 1)]]
[[shader("compute")]]
void compute(
    uniform ParameterBlock<Screen> screen,

    uniform ParameterBlock<GeometryTextures> textures,
    
    uint3 globalInvocationId: SV_DispatchThreadID,
    uint3 localInvocationId: SV_GroupThreadID,
    uint localInvocationIndex: SV_GroupIndex,
) {
}